//! Stateful beta authentication against isolated PostgreSQL + Redis.
//! The legacy Next.js login, account and logout routes retain their exact external shapes.
//! No fake sessions: without configured beta stores these routes fail closed.
use std::{sync::Arc, time::Duration};

use axum::{
    Form, Json,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use form_urlencoded::Serializer;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::{
    auth::AuthConfig,
    password,
    store::{AuthStore, SecuritySignal, SessionState, User, now_millis},
};

const SESSION_COOKIE: &str = "codelocal_session";
const DEVICE_COOKIE: &str = "codelocal_device";
const CSRF_COOKIE: &str = "codelocal_csrf";
const SESSION_MAX_AGE: u64 = 2_592_000;
const DEVICE_MAX_AGE: u64 = 31_536_000;

#[derive(Deserialize)]
pub struct LoginForm {
    csrf: String,
    email: String,
    password: String,
    #[serde(default)]
    next: String,
}

#[derive(Deserialize)]
pub struct LogoutForm {
    csrf: String,
    #[serde(default)]
    next: String,
}

pub(crate) fn safe_next(next: &str) -> &str {
    if next.is_empty()
        || !next.starts_with('/')
        || next.starts_with("//")
        || next.contains('\\')
        || next.chars().any(char::is_control)
        || next.to_ascii_lowercase().contains("%5c")
    {
        "/dashboard"
    } else {
        next
    }
}

fn login_redirect(next: &str, error: &str) -> Response {
    let query = Serializer::new(String::new())
        .append_pair("next", safe_next(next))
        .append_pair("error", error)
        .finish();
    redirect_to(&format!("/login?{query}"))
}

pub(crate) fn redirect_to(target: &str) -> Response {
    let mut response = StatusCode::SEE_OTHER.into_response();
    // Location paths are strictly local/safe; never reflect unsanitized header data.
    if let Ok(location) = HeaderValue::from_str(target) {
        response.headers_mut().insert(header::LOCATION, location);
    }
    response
}

pub(crate) fn error(code: StatusCode, key: &'static str) -> Response {
    let mut response = (code, Json(json!({"error": key}))).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

pub(crate) fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|h| h.to_str().ok())
        .flat_map(|s| s.split(';'))
        .filter_map(|chunk| chunk.trim().split_once('='))
        .find_map(|(cookie_name, value)| (cookie_name.trim() == name).then_some(value.trim()))
}

pub(crate) fn valid_csrf(value: &str) -> bool {
    (24..=128).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub(crate) fn csrf_matches(expected: &str, submitted: &str) -> bool {
    valid_csrf(expected)
        && expected.len() == submitted.len()
        && bool::from(expected.as_bytes().ct_eq(submitted.as_bytes()))
}

pub(crate) fn random_url_token() -> Option<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).ok()?;
    Some(URL_SAFE_NO_PAD.encode(bytes))
}

pub(crate) fn set_cookie(
    headers: &mut HeaderMap,
    name: &str,
    value: &str,
    max_age: u64,
    secure: bool,
) {
    let secure_attribute = if secure { "; Secure" } else { "" };
    let cookie = format!(
        "{name}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure_attribute}"
    );
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        headers.append(header::SET_COOKIE, value);
    }
}

fn normalize_agent(raw: &str) -> String {
    let mut output = String::new();
    let mut last_digit = false;
    for ch in raw.trim().to_lowercase().chars() {
        if ch.is_ascii_digit() {
            if !last_digit {
                output.push('#');
            }
            last_digit = true;
        } else {
            last_digit = false;
            output.push(ch);
        }
        if output.len() >= 512 {
            break;
        }
    }
    output
}

fn binding_hash(secret: &str, kind: &str, value: &str) -> String {
    if secret.trim().is_empty() || value.trim().is_empty() {
        return String::new();
    }
    let input = format!(
        "{}\0request-security-v1\0{}\0{}",
        secret.trim(),
        kind,
        value.trim()
    );
    hex::encode(Sha256::digest(input.as_bytes()))
}

/// Mirrors Go request-security-v1 device/agent binding. Network binding will be
/// added after trusted-proxy IP authentication is implemented and validated.
fn signal_for(config: &AuthConfig, headers: &HeaderMap, device_token: &str) -> SecuritySignal {
    let secret = config.binding_secret.as_deref().unwrap_or_default();
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default();
    SecuritySignal {
        device_hash: binding_hash(secret, "device", device_token),
        agent_hash: binding_hash(secret, "agent", &normalize_agent(ua)),
        network_hash: String::new(),
    }
}

fn device_mismatch(previous: &SecuritySignal, current: &SecuritySignal) -> bool {
    !previous.device_hash.is_empty()
        && !bool::from(
            previous
                .device_hash
                .as_bytes()
                .ct_eq(current.device_hash.as_bytes()),
        )
}

fn agent_changed(previous: &SecuritySignal, current: &SecuritySignal) -> bool {
    !previous.agent_hash.is_empty()
        && !current.agent_hash.is_empty()
        && !bool::from(
            previous
                .agent_hash
                .as_bytes()
                .ct_eq(current.agent_hash.as_bytes()),
        )
}

fn email_normalize(email: &str) -> String {
    email.trim().to_lowercase()
}

async fn verify_password_async(password: String, user: Option<User>) -> bool {
    tokio::task::spawn_blocking(move || {
        match user {
            Some(user) => {
                password::verify_password(&password, &user.password_salt, &user.password_hash)
            }
            None => {
                // Go pads unknown-account timing using scrypt.
                let candidate = if password.len() >= 10 && password.len() <= 256 {
                    password.as_str()
                } else {
                    "unknown-account-constant-time-padding"
                };
                let _ = password::hash_password(candidate, "codelocal-login-timing-padding-v1");
                false
            }
        }
    })
    .await
    .unwrap_or(false)
}

pub async fn login(
    State(config): State<AuthConfig>,
    headers: HeaderMap,
    Form(form): Form<LoginForm>,
) -> Response {
    let next = safe_next(&form.next);
    let cookie = cookie_value(&headers, CSRF_COOKIE).unwrap_or_default();
    if !csrf_matches(cookie, &form.csrf) {
        return login_redirect(next, "Security token expired. Please try again.");
    }
    let Some(store) = &config.store else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "auth_unavailable");
    };
    let email = email_normalize(&form.email);
    if email.len() > 254 || !email.contains('@') || email.chars().any(char::is_whitespace) {
        return login_redirect(next, "Email or password is incorrect.");
    }
    match store.throttle_login(&email).await {
        Ok(true) => {}
        Ok(false) => {
            return login_redirect(next, "Too many sign-in attempts. Please try again later.");
        }
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "auth_unavailable"),
    }

    let user = match store.by_email(&email).await {
        Ok(user) => user,
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "auth_unavailable"),
    };
    if !verify_password_async(form.password, user.clone()).await {
        if store
            .audit(
                user.as_ref().map(|u| u.id.as_str()),
                "auth.login_failed",
                json!({"email":email}),
            )
            .await
            .is_err()
        {
            return error(StatusCode::SERVICE_UNAVAILABLE, "auth_unavailable");
        }
        return login_redirect(next, "Email or password is incorrect.");
    }
    let Some(user) = user else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "auth_unavailable");
    };
    issue_session(&config, &headers, &user, form.csrf, next, "auth.login").await
}

pub(crate) async fn issue_session(
    config: &AuthConfig,
    headers: &HeaderMap,
    user: &User,
    csrf: String,
    next: &str,
    event: &'static str,
) -> Response {
    let Some(store) = &config.store else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "auth_unavailable");
    };
    let existing_device = cookie_value(headers, DEVICE_COOKIE).filter(|value| valid_csrf(value));
    let Some(device_token) = existing_device.map(str::to_owned).or_else(random_url_token) else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "auth_unavailable");
    };
    let session = SessionState {
        user_id: user.id.clone(),
        csrf,
        created_at: now_millis(),
        security_version: user.security_version,
        security: Some(signal_for(config, headers, &device_token)),
        risk_until: 0,
    };
    let session_id = match store.create_session(&session).await {
        Ok(id) => id,
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "auth_unavailable"),
    };
    let detail = if event == "auth.register" {
        json!({"emailVerified": true})
    } else {
        json!({"method": "password"})
    };
    if store.audit(Some(&user.id), event, detail).await.is_err() {
        let _ = store.delete_session(&session_id).await;
        return error(StatusCode::SERVICE_UNAVAILABLE, "auth_unavailable");
    }
    let mut response = redirect_to(safe_next(next));
    set_cookie(
        response.headers_mut(),
        SESSION_COOKIE,
        &session_id,
        SESSION_MAX_AGE,
        config.secure_cookies,
    );
    if existing_device.is_none() {
        set_cookie(
            response.headers_mut(),
            DEVICE_COOKIE,
            &device_token,
            DEVICE_MAX_AGE,
            config.secure_cookies,
        );
    }
    response
}

pub(crate) async fn identify(
    store: &Arc<AuthStore>,
    config: &AuthConfig,
    headers: &HeaderMap,
) -> Result<Option<(String, SessionState, User)>, StatusCode> {
    let Some(session_id) = cookie_value(headers, SESSION_COOKIE) else {
        return Ok(None);
    };
    let Some(mut session) = store
        .read_session(session_id)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
    else {
        return Ok(None);
    };
    let Some(user) = store
        .by_id(&session.user_id)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
    else {
        return Ok(None);
    };
    if session.invalid_for_user(&user) {
        store
            .delete_session(session_id)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        return Ok(None);
    }
    let Some(device_token) = cookie_value(headers, DEVICE_COOKIE).filter(|c| valid_csrf(c)) else {
        store
            .delete_session(session_id)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        return Ok(None);
    };
    let Some(previous_security) = session.security.as_ref() else {
        // A beta session must have device binding; don't silently adopt unbound sessions.
        store
            .delete_session(session_id)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        return Ok(None);
    };
    let current_security = signal_for(config, headers, device_token);
    if device_mismatch(previous_security, &current_security) {
        store
            .delete_session(session_id)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        return Ok(None);
    }
    if agent_changed(previous_security, &current_security) {
        session.risk_until = now_millis() + Duration::from_secs(600).as_millis() as i64;
        session.security = Some(current_security);
        store
            .update_session(session_id, &session)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    }
    Ok(Some((session_id.to_owned(), session, user)))
}

pub async fn account(State(config): State<AuthConfig>, headers: HeaderMap) -> Response {
    let Some(store) = &config.store else {
        // Anonymous UI needs 401 to show sign-in; configured sessions cannot be used
        // without a backing store.
        return if cookie_value(&headers, SESSION_COOKIE).is_none() {
            error(StatusCode::UNAUTHORIZED, "unauthorized")
        } else {
            error(StatusCode::SERVICE_UNAVAILABLE, "account_unavailable")
        };
    };
    let identity = identify(store, &config, &headers).await;
    let (_, session, user) = match identity {
        Ok(Some(identity)) => identity,
        Ok(None) => return error(StatusCode::UNAUTHORIZED, "unauthorized"),
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "identity_unavailable"),
    };
    let is_admin = config.admin_emails.iter().any(|email| email == &user.email);
    let body = json!({
        "email": user.email,
        "userId": user.id,
        "referralCode": user.referral_code,
        "invitedBy": user.referred_by_code.as_deref().unwrap_or("Direct / legacy account"),
        "createdAt": user.created_at,
        "passwordChangedAt": user.password_changed_at,
        "isAdmin": is_admin,
        "requiresReauthentication": session.risk_until > now_millis(),
        "csrf": session.csrf,
    });
    let mut response = Json(body).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

pub async fn logout(
    State(config): State<AuthConfig>,
    headers: HeaderMap,
    Form(form): Form<LogoutForm>,
) -> Response {
    let Some(store) = &config.store else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "auth_unavailable");
    };
    let (session_id, session, user) = match identify(store, &config, &headers).await {
        Ok(Some(identity)) => identity,
        Ok(None) => return error(StatusCode::FORBIDDEN, "invalid_security_token"),
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "auth_unavailable"),
    };
    if !csrf_matches(&session.csrf, &form.csrf) {
        return error(StatusCode::FORBIDDEN, "invalid_security_token");
    }
    if store.delete_session(&session_id).await.is_err() {
        return error(StatusCode::SERVICE_UNAVAILABLE, "auth_unavailable");
    }
    // Avoid resurrecting an invalid session if audit storage becomes unavailable.
    let _ = store.audit(Some(&user.id), "auth.logout", json!({})).await;
    let next = safe_next(&form.next);
    let path = if next == "/dashboard" {
        "/login".to_owned()
    } else {
        let query = Serializer::new(String::new())
            .append_pair("next", next)
            .finish();
        format!("/login?{query}")
    };
    let mut response = redirect_to(&path);
    set_cookie(
        response.headers_mut(),
        SESSION_COOKIE,
        "",
        0,
        config.secure_cookies,
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router_with_auth;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    #[test]
    fn safe_redirects_never_leave_origin() {
        assert_eq!(safe_next(""), "/dashboard");
        assert_eq!(safe_next("https://evil.example"), "/dashboard");
        assert_eq!(safe_next("//evil.example"), "/dashboard");
        assert_eq!(safe_next("/\\evil"), "/dashboard");
        assert_eq!(safe_next("/%5c%5cevil"), "/dashboard");
        assert_eq!(
            safe_next("/dashboard?tab=workspaces"),
            "/dashboard?tab=workspaces"
        );
    }

    #[test]
    fn csrf_double_submit_requires_same_length_and_cookie() {
        let token = "0123456789ABCDEFGHIJKLMNOPQRSTUV";
        assert!(csrf_matches(token, token));
        assert!(!csrf_matches(token, "other"));
        assert!(!csrf_matches("", ""));
        assert!(!csrf_matches(token, "0123456789ABCDEFGHIJKLMNOPQRSTUW"));
    }

    #[test]
    fn device_and_agent_signals_are_stable_and_sensitive_to_changes() {
        let cfg = AuthConfig {
            secure_cookies: false,
            store: None,
            binding_secret: Some("test-binding-secret-long-long-long-long".into()),
            admin_emails: vec![],
            mailer: None,
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            "Mozilla/5.0 Browser/125.0".parse().unwrap(),
        );
        let first = signal_for(&cfg, &headers, "device-token-abcdefghijklmnopqrstuvwxyz");
        let second = signal_for(&cfg, &headers, "device-token-abcdefghijklmnopqrstuvwxyz");
        assert_eq!(first.device_hash, second.device_hash);
        assert_eq!(first.agent_hash, second.agent_hash);
        assert_eq!(first.device_hash.len(), 64);
        let changed = signal_for(&cfg, &headers, "another-token-abcdefghijklmnopqrstuvwxyz");
        assert!(device_mismatch(&first, &changed));
        let _ = headers.insert(header::USER_AGENT, "AnotherBrowser/125.0".parse().unwrap());
        assert!(agent_changed(
            &first,
            &signal_for(&cfg, &headers, "device-token-abcdefghijklmnopqrstuvwxyz")
        ));
    }

    #[tokio::test]
    async fn auth_mutations_fail_closed_without_store() {
        let csrf = "abcdefghijklmnopqrstuvwxyzABCDEF";
        let request = Request::builder()
            .method("POST").uri("/login")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(header::COOKIE, format!("{CSRF_COOKIE}={csrf}"))
            .body(Body::from(format!("email=demo%40example.com&password=correct-horse-battery-staple&csrf={csrf}&next=%2Fdashboard")))
            .unwrap();
        let response = router_with_auth(AuthConfig {
            secure_cookies: false,
            store: None,
            binding_secret: None,
            admin_emails: vec![],
            mailer: None,
        })
        .oneshot(request)
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(!response.headers().contains_key(header::SET_COOKIE));
    }

    #[tokio::test]
    async fn unauthenticated_account_and_logout_are_rejected() {
        let config = AuthConfig {
            secure_cookies: false,
            store: None,
            binding_secret: None,
            admin_emails: vec![],
            mailer: None,
        };
        let router = router_with_auth(config);
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/account")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/logout")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from("csrf=x&next=%2Fdashboard"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
