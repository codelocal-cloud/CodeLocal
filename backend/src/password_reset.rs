//! Password reset and authenticated password rotation using existing Next.js forms.
//! A reset increments codelocal_users.security_version to revoke all old sessions.
use crate::{
    auth::AuthConfig,
    auth_session::{
        cookie_value, csrf_matches, error, identify, random_url_token, redirect_to, set_cookie,
    },
    password,
    signup::{mask_email, otp_hash, random_otp, random_salt, valid_email, verify_code},
    store::{PendingKind, now_millis, valid_pending_token},
};
use axum::{
    Form, Json,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use form_urlencoded::Serializer;
use serde::{Deserialize, Serialize};
use serde_json::json;

const CSRF_COOKIE: &str = "codelocal_csrf";
const SESSION_COOKIE: &str = "codelocal_session";
const RESET_TTL: u64 = 600;

#[derive(Deserialize)]
pub struct ForgotForm {
    csrf: String,
    email: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetForm {
    csrf: String,
    token: String,
    code: String,
    password: String,
    confirm_password: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeForm {
    csrf: String,
    current_password: String,
    password: String,
    confirm_password: String,
}

#[derive(Deserialize)]
pub struct TokenQuery {
    token: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingReset {
    user_id: String,
    email: String,
    code_hash: String,
}

fn redirect_query(path: &str, pairs: &[(&str, &str)]) -> Response {
    let mut encoded = Serializer::new(String::new());
    for (key, value) in pairs {
        encoded.append_pair(key, value);
    }
    redirect_to(&format!("{path}?{}", encoded.finish()))
}

fn reset_redirect(token: &str, key: &str, value: &str) -> Response {
    redirect_query("/reset-password", &[("token", token), (key, value)])
}

fn password_err_message(password: &str) -> Option<&'static str> {
    if password.len() < 10 {
        Some("Password must be at least 10 characters.")
    } else if password.len() > 256 {
        Some("Password is too long.")
    } else {
        None
    }
}

pub async fn forgot(
    State(config): State<AuthConfig>,
    headers: HeaderMap,
    Form(form): Form<ForgotForm>,
) -> Response {
    if !csrf_matches(
        cookie_value(&headers, CSRF_COOKIE).unwrap_or_default(),
        &form.csrf,
    ) {
        return redirect_query(
            "/forgot-password",
            &[("error", "Security token expired. Please try again.")],
        );
    }
    let Some(store) = &config.store else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "reset_unavailable");
    };
    let Some(mailer) = &config.mailer else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "email_delivery_unavailable",
        );
    };
    let email = form.email.trim().to_lowercase();
    if !valid_email(&email) {
        return redirect_query(
            "/forgot-password",
            &[("error", "Enter a valid email address.")],
        );
    }
    match store
        .rate_limit("password-reset-account", &email, 2, 86400)
        .await
    {
        Ok(true) => {}
        Ok(false) => {
            return redirect_query(
                "/forgot-password",
                &[("error", "Reset email limit reached. Try again later.")],
            );
        }
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "reset_unavailable"),
    };
    let user = match store.by_email(&email).await {
        Ok(user) => user,
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "reset_unavailable"),
    };
    if let Some(user) = user {
        let (Some(token), Some(code)) = (random_url_token(), random_otp()) else {
            return error(StatusCode::SERVICE_UNAVAILABLE, "reset_unavailable");
        };
        let secret = config.binding_secret.as_deref().unwrap_or_default();
        let code_hash = otp_hash(secret, "password-reset-otp-v1", &token, &code);
        let pending = PendingReset {
            user_id: user.id.clone(),
            email: user.email.clone(),
            code_hash: code_hash.clone(),
        };
        if store
            .pending_put(PendingKind::Reset, &token, &pending, RESET_TTL)
            .await
            .is_err()
        {
            return error(StatusCode::SERVICE_UNAVAILABLE, "reset_unavailable");
        }
        let Some(origin) = crate::public_url::beta_origin() else {
            let _ = store.pending_delete(PendingKind::Reset, &token).await;
            return error(StatusCode::SERVICE_UNAVAILABLE, "public_base_url_required");
        };
        let url = format!("{origin}/reset-password?token={token}");
        let text = format!(
            "Use code {code} to reset your CodeLocal password. It expires in 10 minutes. Open {url} to continue. If you did not request this, ignore this email."
        );
        let idempotency = format!("password-reset-{token}-{}", &code_hash[..16]);
        if mailer
            .send(&email, "Reset your CodeLocal password", &text, &idempotency)
            .await
            .is_err()
        {
            let _ = store.pending_delete(PendingKind::Reset, &token).await;
            let _ = store
                .audit(
                    Some(&user.id),
                    "auth.password_reset_delivery_failed",
                    json!({}),
                )
                .await;
            // We deliberately do not reveal whether the email is registered.
            return redirect_query("/forgot-password", &[("sent", "1")]);
        }
        let _ = store
            .audit(Some(&user.id), "auth.password_reset_requested", json!({}))
            .await;
    }
    redirect_query("/forgot-password", &[("sent", "1")])
}

pub async fn context(
    State(config): State<AuthConfig>,
    Query(query): Query<TokenQuery>,
) -> Response {
    let Some(store) = &config.store else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "reset_unavailable");
    };
    let mut response = match store
        .pending_get::<PendingReset>(PendingKind::Reset, &query.token)
        .await
    {
        Ok(Some((pending, _))) => {
            Json(json!({"valid": true, "emailMasked": mask_email(&pending.email)})).into_response()
        }
        Ok(None) => (StatusCode::GONE, Json(json!({"valid": false}))).into_response(),
        Err(_) => error(StatusCode::SERVICE_UNAVAILABLE, "reset_unavailable"),
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

pub async fn reset(
    State(config): State<AuthConfig>,
    headers: HeaderMap,
    Form(form): Form<ResetForm>,
) -> Response {
    if !csrf_matches(
        cookie_value(&headers, CSRF_COOKIE).unwrap_or_default(),
        &form.csrf,
    ) {
        return reset_redirect(
            &form.token,
            "error",
            "Invalid security token. Please try again.",
        );
    }
    let Some(store) = &config.store else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "reset_unavailable");
    };
    if !valid_pending_token(&form.token) {
        return reset_redirect(&form.token, "expired", "1");
    }
    match store
        .rate_limit("password-reset-token", &form.token, 12, 600)
        .await
    {
        Ok(true) => {}
        Ok(false) => {
            return reset_redirect(
                &form.token,
                "error",
                "Too many reset attempts. Request a new code or try again later.",
            );
        }
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "reset_unavailable"),
    }
    let pending = match store
        .pending_get::<PendingReset>(PendingKind::Reset, &form.token)
        .await
    {
        Ok(Some((pending, _))) => pending,
        Ok(None) => return reset_redirect(&form.token, "expired", "1"),
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "reset_unavailable"),
    };
    if !verify_code(
        config.binding_secret.as_deref().unwrap_or_default(),
        "password-reset-otp-v1",
        &form.token,
        &form.code,
        &pending.code_hash,
    ) {
        return match store
            .pending_attempt(PendingKind::Reset, &form.token, 5)
            .await
        {
            Ok(true) => reset_redirect(&form.token, "error", "The reset code is incorrect."),
            Ok(false) => reset_redirect(&form.token, "expired", "1"),
            Err(_) => error(StatusCode::SERVICE_UNAVAILABLE, "reset_unavailable"),
        };
    }
    if form.password != form.confirm_password {
        return reset_redirect(&form.token, "error", "Passwords do not match.");
    }
    if let Some(message) = password_err_message(&form.password) {
        return reset_redirect(&form.token, "error", message);
    }
    let user = match store.by_id(&pending.user_id).await {
        Ok(Some(user)) => user,
        _ => return error(StatusCode::SERVICE_UNAVAILABLE, "reset_unavailable"),
    };
    if password::verify_password(&form.password, &user.password_salt, &user.password_hash) {
        return reset_redirect(
            &form.token,
            "error",
            "Choose a password you have not already been using.",
        );
    }
    let salt = match random_salt() {
        Some(value) => value,
        None => return error(StatusCode::SERVICE_UNAVAILABLE, "reset_unavailable"),
    };
    let salt_copy = salt.clone();
    let password = form.password;
    let Ok(Some(hash)) =
        tokio::task::spawn_blocking(move || password::hash_password(&password, &salt_copy)).await
    else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "reset_unavailable");
    };
    match store.pending_lock(PendingKind::Reset, &form.token).await {
        Ok(true) => {}
        Ok(false) => {
            return reset_redirect(
                &form.token,
                "error",
                "This password reset is already being finalized. If it does not complete, request a new reset code.",
            );
        }
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "reset_unavailable"),
    }
    if store.update_password(&user.id, &hash, &salt).await.is_err() {
        return error(StatusCode::SERVICE_UNAVAILABLE, "reset_unavailable");
    }
    let _ = store.pending_delete(PendingKind::Reset, &form.token).await;
    let _ = store
        .audit(Some(&user.id), "auth.password_reset_completed", json!({}))
        .await;
    let mut response = redirect_query("/reset-password", &[("done", "1")]);
    set_cookie(
        response.headers_mut(),
        SESSION_COOKIE,
        "",
        0,
        config.secure_cookies,
    );
    response
}

pub async fn change(
    State(config): State<AuthConfig>,
    headers: HeaderMap,
    Form(form): Form<ChangeForm>,
) -> Response {
    let path = "/dashboard/account";
    let Some(store) = &config.store else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "account_unavailable");
    };
    let (old_session_id, previous_session, user) = match identify(store, &config, &headers).await {
        Ok(Some(identity)) => identity,
        Ok(None) => return redirect_query("/login", &[("next", path)]),
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "account_unavailable"),
    };
    if previous_session.risk_until > now_millis() {
        let _ = store.delete_session(&old_session_id).await;
        return redirect_query("/login", &[("next", path)]);
    }
    if !csrf_matches(&previous_session.csrf, &form.csrf) {
        return redirect_query(
            path,
            &[("error", "Security token expired. Please try again.")],
        );
    }
    match store
        .rate_limit("password-change-user", &user.id, 8, 600)
        .await
    {
        Ok(true) => {}
        Ok(false) => {
            return redirect_query(
                path,
                &[("error", "Too many password changes. Try again later.")],
            );
        }
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "account_unavailable"),
    }
    if !password::verify_password(
        &form.current_password,
        &user.password_salt,
        &user.password_hash,
    ) {
        return redirect_query(path, &[("error", "Current password is incorrect.")]);
    }
    if form.password != form.confirm_password {
        return redirect_query(path, &[("error", "New passwords do not match.")]);
    }
    if password::verify_password(&form.password, &user.password_salt, &user.password_hash) {
        return redirect_query(path, &[("error", "Choose a different password.")]);
    }
    if let Some(message) = password_err_message(&form.password) {
        return redirect_query(path, &[("error", message)]);
    }
    let salt = match random_salt() {
        Some(salt) => salt,
        None => return error(StatusCode::SERVICE_UNAVAILABLE, "account_unavailable"),
    };
    let salt_copy = salt.clone();
    let password = form.password;
    let Ok(Some(hash)) =
        tokio::task::spawn_blocking(move || password::hash_password(&password, &salt_copy)).await
    else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "account_unavailable");
    };
    let version = match store.update_password(&user.id, &hash, &salt).await {
        Ok(version) => version,
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "account_unavailable"),
    };
    let mut session = previous_session;
    session.created_at = now_millis();
    session.security_version = version;
    session.risk_until = 0;
    let new_session_id = match store.create_session(&session).await {
        Ok(id) => id,
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "account_unavailable"),
    };
    let _ = store.delete_session(&old_session_id).await;
    let _ = store
        .audit(Some(&user.id), "auth.password_changed", json!({}))
        .await;
    let mut response = redirect_query(
        path,
        &[(
            "ok",
            "Password updated. Other signed-in sessions were revoked.",
        )],
    );
    set_cookie(
        response.headers_mut(),
        SESSION_COOKIE,
        &new_session_id,
        2_592_000,
        config.secure_cookies,
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn password_reset_requires_strong_match() {
        assert_eq!(
            password_err_message("short"),
            Some("Password must be at least 10 characters.")
        );
        assert_eq!(
            password_err_message(&"a".repeat(257)),
            Some("Password is too long.")
        );
        assert_eq!(password_err_message("very-long-password"), None);
    }
    #[test]
    fn different_otp_purposes_have_distinct_hash() {
        let token = "abcdefghijklmnopqrstuvwxyzABCDEF";
        let code = "123456";
        let secret = "test-secret-with-more-than-32-characters";
        assert_ne!(
            otp_hash(secret, "signup-email-otp-v1", token, code),
            otp_hash(secret, "password-reset-otp-v1", token, code)
        );
    }
}
