//! Email-verified, invite-only account creation compatible with unchanged Next.js.
use axum::{
    Form, Json,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use form_urlencoded::Serializer;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::{
    auth::AuthConfig,
    auth_session::{
        cookie_value, csrf_matches, error, issue_session, random_url_token, redirect_to, safe_next,
    },
    password,
    store::{PendingKind, now_millis, valid_pending_token},
};

const TTL: u64 = 600;
const CSRF_COOKIE: &str = "codelocal_csrf";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSignup {
    pub csrf: String,
    pub email: String,
    pub password: String,
    pub referral_code: String,
    #[serde(default)]
    pub next: String,
}

#[derive(Deserialize)]
pub struct VerifySignup {
    pub csrf: String,
    pub token: String,
    pub code: String,
}

#[derive(Deserialize)]
pub struct TokenQuery {
    token: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingSignup {
    email: String,
    password_hash: String,
    password_salt: String,
    referral_code: String,
    next: String,
    code_hash: String,
    attempts: u32,
    verification_at: i64,
}

fn signup_redirect(next: &str, referral: &str, message: &str) -> Response {
    let qs = Serializer::new(String::new())
        .append_pair("next", safe_next(next))
        .append_pair("ref", referral)
        .append_pair("error", message)
        .finish();
    redirect_to(&format!("/signup?{qs}"))
}

fn verify_redirect(token: &str, key: &str, value: &str) -> Response {
    let qs = Serializer::new(String::new())
        .append_pair("token", token)
        .append_pair(key, value)
        .finish();
    redirect_to(&format!("/signup/verify?{qs}"))
}

pub(crate) fn valid_email(value: &str) -> bool {
    if value.is_empty() || value.len() > 254 || value.bytes().any(|b| b.is_ascii_whitespace()) {
        return false;
    }
    let Some((name, host)) = value.split_once('@') else {
        return false;
    };
    !name.is_empty()
        && host.contains('.')
        && !host.starts_with('.')
        && !host.ends_with('.')
        && !host.contains('@')
}

fn normalize_referral(raw: &str) -> String {
    raw.trim().to_ascii_uppercase()
}

pub(crate) fn otp_hash(secret: &str, scope: &str, token: &str, code: &str) -> String {
    let bytes = format!("{secret}\0{scope}\0{token}\0{code}");
    hex::encode(Sha256::digest(bytes.as_bytes()))
}

pub(crate) fn verify_code(secret: &str, scope: &str, token: &str, code: &str, hash: &str) -> bool {
    code.len() == 6
        && code.bytes().all(|b| b.is_ascii_digit())
        && hash.len() == 64
        && bool::from(
            otp_hash(secret, scope, token, code)
                .as_bytes()
                .ct_eq(hash.as_bytes()),
        )
}

/// 6 decimal digits, uniform by rejection-sampling 32 random bits.
pub(crate) fn random_otp() -> Option<String> {
    const RANGE: u32 = 1_000_000;
    const LIMIT: u32 = u32::MAX - (u32::MAX % RANGE);
    loop {
        let mut bytes = [0u8; 4];
        getrandom::fill(&mut bytes).ok()?;
        let value = u32::from_be_bytes(bytes);
        if value < LIMIT {
            return Some(format!("{:06}", value % RANGE));
        }
    }
}
pub(crate) fn random_salt() -> Option<String> {
    let mut bytes = [0u8; 24];
    getrandom::fill(&mut bytes).ok()?;
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    Some(URL_SAFE_NO_PAD.encode(bytes))
}

pub(crate) fn mask_email(email: &str) -> String {
    let Some((local, domain)) = email.split_once('@') else {
        return String::new();
    };
    if local.is_empty() {
        return String::new();
    }
    let count = local.chars().count().saturating_sub(1).min(6);
    format!(
        "{}{}@{domain}",
        local.chars().next().unwrap_or('*'),
        "*".repeat(count.max(3))
    )
}

pub async fn start(
    State(config): State<AuthConfig>,
    headers: HeaderMap,
    Form(form): Form<StartSignup>,
) -> Response {
    let invite = normalize_referral(&form.referral_code);
    let next = safe_next(&form.next);
    if !csrf_matches(
        cookie_value(&headers, CSRF_COOKIE).unwrap_or(""),
        &form.csrf,
    ) {
        return signup_redirect(next, &invite, "Security token expired. Please try again.");
    }
    let Some(store) = config.store.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "signup_unavailable");
    };
    // Never silently create accounts or pretend an OTP was sent without delivery configured.
    let Some(mailer) = config.mailer.as_ref() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "email_delivery_unavailable",
        );
    };
    let email = form.email.trim().to_lowercase();
    if !valid_email(&email) {
        return signup_redirect(next, &invite, "Enter a valid email address.");
    }
    match store.rate_limit("signup-account", &email, 4, 3600).await {
        Ok(true) => {}
        Ok(false) => {
            return signup_redirect(
                next,
                &invite,
                "Too many sign-up attempts. Please try again later.",
            );
        }
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "signup_unavailable"),
    }
    match store.by_email(&email).await {
        Ok(Some(_)) => {
            return signup_redirect(next, &invite, "An account with this email already exists.");
        }
        Ok(None) => {}
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "signup_unavailable"),
    };
    match store
        .referral_allowed(
            &invite,
            &email,
            config.admin_emails.first().map(String::as_str),
        )
        .await
    {
        Ok(true) => {}
        Ok(false) => {
            return signup_redirect(
                next,
                &invite,
                "Referral code is invalid. Ask an existing CodeLocal member for a valid invite code.",
            );
        }
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "signup_unavailable"),
    }
    if form.password.len() < 10 || form.password.len() > 256 {
        let message = if form.password.len() < 10 {
            "Password must be at least 10 characters."
        } else {
            "Password is too long."
        };
        return signup_redirect(next, &invite, message);
    }
    let Some(salt) = random_salt() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "signup_unavailable");
    };
    let password = form.password;
    let salt_for_hash = salt.clone();
    let Ok(Some(hash)) =
        tokio::task::spawn_blocking(move || password::hash_password(&password, &salt_for_hash))
            .await
    else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "signup_unavailable");
    };
    let (Some(token), Some(code)) = (random_url_token(), random_otp()) else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "signup_unavailable");
    };
    let secret = config.binding_secret.as_deref().unwrap_or_default();
    let code_hash = otp_hash(secret, "signup-email-otp-v1", &token, &code);
    let pending = PendingSignup {
        email: email.clone(),
        password_hash: hash,
        password_salt: salt,
        referral_code: invite.clone(),
        next: next.to_owned(),
        code_hash: code_hash.clone(),
        attempts: 0,
        verification_at: now_millis(),
    };
    if store
        .pending_put(PendingKind::Signup, &token, &pending, TTL)
        .await
        .is_err()
    {
        return error(StatusCode::SERVICE_UNAVAILABLE, "signup_unavailable");
    }
    let message = format!(
        "Your CodeLocal verification code is {code}. It expires in 10 minutes. If you did not request this, ignore this email."
    );
    let idempotency = format!("signup-otp-{token}-{}", &code_hash[..16]);
    if mailer
        .send(
            &email,
            "Your CodeLocal verification code",
            &message,
            &idempotency,
        )
        .await
        .is_err()
    {
        let _ = store.pending_delete(PendingKind::Signup, &token).await;
        return signup_redirect(
            next,
            &invite,
            "We could not send the verification email. Please try again.",
        );
    }
    let qs = Serializer::new(String::new())
        .append_pair("token", &token)
        .finish();
    redirect_to(&format!("/signup/verify?{qs}"))
}

pub async fn context(
    State(config): State<AuthConfig>,
    Query(query): Query<TokenQuery>,
) -> Response {
    let Some(store) = &config.store else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "signup_unavailable");
    };
    let mut response = match store
        .pending_get::<PendingSignup>(PendingKind::Signup, &query.token)
        .await
    {
        Ok(Some((pending, _))) => {
            Json(json!({"valid": true, "emailMasked": mask_email(&pending.email)})).into_response()
        }
        Ok(None) => (StatusCode::GONE, Json(json!({"valid": false}))).into_response(),
        Err(_) => error(StatusCode::SERVICE_UNAVAILABLE, "verification_unavailable"),
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

pub async fn verify(
    State(config): State<AuthConfig>,
    headers: HeaderMap,
    Form(form): Form<VerifySignup>,
) -> Response {
    if !csrf_matches(
        cookie_value(&headers, CSRF_COOKIE).unwrap_or(""),
        &form.csrf,
    ) {
        return verify_redirect(
            &form.token,
            "error",
            "Security token expired. Please reload and try again.",
        );
    }
    let Some(store) = &config.store else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "signup_unavailable");
    };
    if !valid_pending_token(&form.token) {
        return verify_redirect(&form.token, "expired", "1");
    }
    match store
        .rate_limit("signup-verify-token", &form.token, 12, 600)
        .await
    {
        Ok(true) => {}
        Ok(false) => {
            return verify_redirect(
                &form.token,
                "error",
                "Too many verification attempts. Start again or try later.",
            );
        }
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "signup_unavailable"),
    }
    let pending = match store
        .pending_get::<PendingSignup>(PendingKind::Signup, &form.token)
        .await
    {
        Ok(Some((pending, _))) => pending,
        Ok(None) => return verify_redirect(&form.token, "expired", "1"),
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "signup_unavailable"),
    };
    if !verify_code(
        config.binding_secret.as_deref().unwrap_or(""),
        "signup-email-otp-v1",
        &form.token,
        &form.code,
        &pending.code_hash,
    ) {
        return match store
            .pending_attempt(PendingKind::Signup, &form.token, 5)
            .await
        {
            Ok(true) => {
                verify_redirect(&form.token, "error", "The verification code is incorrect.")
            }
            Ok(false) => verify_redirect(&form.token, "expired", "1"),
            Err(_) => error(StatusCode::SERVICE_UNAVAILABLE, "signup_unavailable"),
        };
    }
    match store.pending_lock(PendingKind::Signup, &form.token).await {
        Ok(true) => {}
        Ok(false) => {
            return verify_redirect(
                &form.token,
                "error",
                "Verification is already being finalized.",
            );
        }
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "signup_unavailable"),
    }
    if !store
        .referral_allowed(
            &pending.referral_code,
            &pending.email,
            config.admin_emails.first().map(String::as_str),
        )
        .await
        .unwrap_or(false)
    {
        let _ = store.pending_delete(PendingKind::Signup, &form.token).await;
        return signup_redirect(
            &pending.next,
            &pending.referral_code,
            "Referral code is invalid. Ask an existing CodeLocal member for a valid invite code.",
        );
    }
    let user = match store
        .create_verified_user(
            &pending.email,
            &pending.password_hash,
            &pending.password_salt,
            &pending.referral_code,
        )
        .await
    {
        Ok(user) => user,
        Err(_) => {
            let _ = store.pending_delete(PendingKind::Signup, &form.token).await;
            return signup_redirect(
                &pending.next,
                &pending.referral_code,
                "Unable to create account. Please try again.",
            );
        }
    };
    let _ = store.pending_delete(PendingKind::Signup, &form.token).await;
    issue_session(
        &config,
        &headers,
        &user,
        form.csrf,
        &pending.next,
        "auth.register",
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deterministic_sha256_namespaces_and_correct_code_validation() {
        let token = "abcdefghijklmnopqrstuvwxyzABCDEF";
        let secret = "fixture-secret-longer-than-thirty-two-bytes";
        let hash = otp_hash(secret, "signup-email-otp-v1", token, "123456");
        assert!(verify_code(
            secret,
            "signup-email-otp-v1",
            token,
            "123456",
            &hash
        ));
        assert!(!verify_code(
            secret,
            "signup-email-otp-v1",
            token,
            "654321",
            &hash
        ));
        assert!(!verify_code(
            secret,
            "password-reset-otp-v1",
            token,
            "123456",
            &hash
        ));
        assert!(!verify_code(
            "other-secret",
            "signup-email-otp-v1",
            token,
            "123456",
            &hash
        ));
    }

    #[test]
    fn signup_referral_email_validation() {
        assert!(valid_email("someone@example.com"));
        for invalid in ["a@b", "a@@b.com", "a@example", "a@.example.com", ""] {
            assert!(!valid_email(invalid));
        }
        assert_eq!(normalize_referral(" abCD12 "), "ABCD12");
    }

    #[test]
    fn otp_is_six_numeric_digits_and_mask_does_not_expose_address() {
        let code = random_otp().unwrap();
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
        assert_eq!(mask_email("alice@example.com"), "a****@example.com");
    }
}
