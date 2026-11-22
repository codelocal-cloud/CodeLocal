//! Initial real API migration: public CSRF bootstrap used by the existing Next.js forms.
//! This endpoint does not authenticate users and never claims login/session parity.
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::json;

const CSRF_COOKIE: &str = "codelocal_csrf";
const SESSION_TTL_SECONDS: u32 = 30 * 24 * 60 * 60;

#[derive(Clone, Debug)]
pub struct AuthConfig {
    pub secure_cookies: bool,
}

impl AuthConfig {
    pub fn from_env() -> Self {
        let production = std::env::var("APP_ENV").is_ok_and(|env| env == "production")
            || std::env::var("NODE_ENV").is_ok_and(|env| env == "production");
        let https_origin = ["CODELOCAL_PUBLIC_BASE_URL", "PUBLIC_BASE_URL"]
            .iter()
            .filter_map(|key| std::env::var(key).ok())
            .any(|url| url.trim().starts_with("https://"));
        Self {
            secure_cookies: production || https_origin,
        }
    }
}

fn valid_csrf(value: &str) -> bool {
    (24..=128).contains(&value.len())
        && value
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == b'-' || ch == b'_')
}

fn csrf_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|header| header.to_str().ok())
        .flat_map(|header| header.split(';'))
        .filter_map(|part| part.trim().split_once('='))
        .find_map(|(name, value)| {
            if name.trim() == CSRF_COOKIE && valid_csrf(value.trim()) {
                Some(value.trim().to_owned())
            } else {
                None
            }
        })
}

fn generate_csrf() -> std::result::Result<String, getrandom::Error> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

pub async fn csrf(State(config): State<AuthConfig>, headers: HeaderMap) -> Response {
    let (token, fresh) = match csrf_cookie(&headers) {
        Some(token) => (token, false),
        None => match generate_csrf() {
            Ok(token) => (token, true),
            Err(_) => {
                let mut response = (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({"error": "csrf_unavailable"})),
                )
                    .into_response();
                response
                    .headers_mut()
                    .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
                return response;
            }
        },
    };

    let mut response = Json(json!({ "csrf": token })).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if fresh {
        let secure = if config.secure_cookies {
            "; Secure"
        } else {
            ""
        };
        let cookie = format!(
            "{CSRF_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={SESSION_TTL_SECONDS}{secure}"
        );
        // The token is base64url and cannot inject arbitrary header data.
        if let Ok(value) = HeaderValue::from_str(&cookie) {
            response.headers_mut().append(header::SET_COOKIE, value);
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router_with_auth;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn call(cookie: Option<&str>, secure: bool) -> Response {
        let mut request = Request::builder().uri("/api/v1/auth/csrf");
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
        }
        router_with_auth(AuthConfig {
            secure_cookies: secure,
        })
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn creates_http_only_cookie_and_existing_next_dto() {
        let response = call(None, false).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let cookie = response.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .to_owned();
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Lax"));
        assert!(cookie.contains("Path=/"));
        assert!(cookie.contains("Max-Age=2592000"));
        assert!(!cookie.contains("; Secure"));
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let token = payload["csrf"].as_str().unwrap();
        assert_eq!(token.len(), 43);
        assert!(valid_csrf(token));
        assert!(cookie.starts_with(&format!("{CSRF_COOKIE}={token};")));
    }

    #[tokio::test]
    async fn preserves_valid_cookie_without_rotating() {
        let first = call(None, true).await;
        let cookie = first.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .to_owned();
        assert!(cookie.ends_with("; Secure"));
        let token = cookie.split(';').next().unwrap();
        let second = call(Some(&format!("other=a; {token}; other2=b")), true).await;
        assert_eq!(second.status(), StatusCode::OK);
        assert!(!second.headers().contains_key(header::SET_COOKIE));
        let bytes = second.into_body().collect().await.unwrap().to_bytes();
        let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            payload["csrf"],
            token.strip_prefix("codelocal_csrf=").unwrap()
        );
    }

    #[tokio::test]
    async fn malformed_cookie_cannot_be_reflected_or_injected() {
        let bad = "codelocal_csrf=x%0D%0ASet-Cookie:evil";
        let response = call(Some(bad), true).await;
        assert_eq!(response.status(), StatusCode::OK);
        let cookie = response.headers()[header::SET_COOKIE].to_str().unwrap();
        assert!(!cookie.contains("%0D"));
        assert!(!cookie.contains("evil"));
    }
}
