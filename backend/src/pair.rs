//! Rust beta device pairing. Does NOT expose local filesystem/MCP execution.
//! Credentials are still subject to signed request verification before privileged APIs.
use axum::{
    Form, Json,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use form_urlencoded::Serializer;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{
    auth::AuthConfig,
    auth_session::{csrf_matches, error, identify, redirect_to},
    store::now_millis,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartPair {
    device_id: String,
    device_name: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairApproveForm {
    pairing_id: String,
    code: String,
    csrf: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimPair {
    pairing_id: String,
    code: String,
    #[serde(default)]
    credential_id: String,
    #[serde(default)]
    credential_secret_hash: String,
    #[serde(default)]
    device_public_key: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairQuery {
    pairing_id: String,
}

fn invalid_pair_id(value: &str) -> bool {
    value.len() != 32 || !value.bytes().all(|c| c.is_ascii_hexdigit())
}
fn valid_code(code: &str) -> bool {
    code.len() == 6 && code.bytes().all(|b| b.is_ascii_digit())
}
fn random_pair_code() -> Option<String> {
    let mut raw = [0u8; 4];
    getrandom::fill(&mut raw).ok()?;
    let n = (u32::from_be_bytes(raw) % 900_000) + 100_000;
    Some(format!("{n:06}"))
}

fn redirect_pair(id: &str, key: &str, value: &str) -> Response {
    let encoded = Serializer::new(String::new())
        .append_pair("pairingId", id)
        .append_pair(key, value)
        .finish();
    redirect_to(&format!("/pair/approve?{encoded}"))
}

pub async fn start(State(config): State<AuthConfig>, Json(input): Json<StartPair>) -> Response {
    let id = input.device_id.trim();
    if id.is_empty() || id.len() > 200 || id.chars().any(char::is_control) {
        return error(StatusCode::BAD_REQUEST, "deviceId_required");
    }
    let Some(store) = config.store.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "pairing_unavailable");
    };
    match store.rate_limit("pair-start-device", id, 12, 600).await {
        Ok(true) => {}
        Ok(false) => return error(StatusCode::TOO_MANY_REQUESTS, "pairing_rate_limited"),
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "pairing_unavailable"),
    };
    match store.rate_limit("pair-start-global", "all", 500, 600).await {
        Ok(true) => {}
        Ok(false) => return error(StatusCode::TOO_MANY_REQUESTS, "pairing_rate_limited"),
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "pairing_unavailable"),
    };
    let Some(base) = crate::public_url::beta_origin() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "public_base_url_required");
    };
    let name = input.device_name.as_deref().unwrap_or(id).trim();
    let name = if name.is_empty() { id } else { name };
    let name = name.chars().take(120).collect::<String>();
    let Some(code) = random_pair_code() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "pairing_unavailable");
    };
    let pair = match store.create_pairing(id, &name, &code).await {
        Ok(pair) => pair,
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "pairing_failed"),
    };
    debug_assert!(pair.created_at <= now_millis());
    Json(json!({
        "pairingId": pair.pairing_id,
        "code": pair.code,
        "expiresAt": pair.expires_at,
        "approveUrl": format!("{base}/pair/approve?pairingId={}", pair.pairing_id),
        "retrySafeClaim": true,
        "deviceSigning": false
    }))
    .into_response()
}

pub async fn approve_context(
    State(config): State<AuthConfig>,
    headers: HeaderMap,
    Query(query): Query<PairQuery>,
) -> Response {
    let Some(store) = config.store.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "pairing_unavailable");
    };
    match identify(store, &config, &headers).await {
        Ok(Some(_)) => {}
        Ok(None) => return error(StatusCode::UNAUTHORIZED, "unauthorized"),
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "identity_unavailable"),
    };
    if invalid_pair_id(&query.pairing_id) {
        return error(StatusCode::NOT_FOUND, "pairing_unavailable");
    }
    match store.get_pairing(&query.pairing_id).await {
        Ok(Some(pair)) if pair.expires_at > now_millis() && pair.claimed_at.is_none() => {
            Json(json!({
                "pairingId": pair.pairing_id,
                "code": pair.code,
                "deviceId": pair.device_id,
                "deviceName": pair.device_name,
                "expiresAt": pair.expires_at
            }))
            .into_response()
        }
        Ok(_) => error(StatusCode::NOT_FOUND, "pairing_unavailable"),
        Err(_) => error(StatusCode::SERVICE_UNAVAILABLE, "pairing_unavailable"),
    }
}

pub async fn approve(
    State(config): State<AuthConfig>,
    headers: HeaderMap,
    Form(form): Form<PairApproveForm>,
) -> Response {
    let Some(store) = config.store.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "pairing_unavailable");
    };
    let (session_id, session, user) = match identify(store, &config, &headers).await {
        Ok(Some(identity)) => identity,
        Ok(None) => return error(StatusCode::UNAUTHORIZED, "unauthorized"),
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "identity_unavailable"),
    };
    if session.risk_until > now_millis() {
        let _ = store.delete_session(&session_id).await;
        return redirect_pair(
            &form.pairing_id,
            "error",
            "Reauthentication required. Please sign in again.",
        );
    }
    if !csrf_matches(&session.csrf, &form.csrf) {
        return redirect_pair(
            &form.pairing_id,
            "error",
            "Invalid security token. Please try again.",
        );
    }
    if invalid_pair_id(&form.pairing_id) || !valid_code(&form.code) {
        return redirect_pair(
            &form.pairing_id,
            "error",
            "Invalid or expired pairing request/code.",
        );
    }
    match store
        .approve_pairing(&form.pairing_id, &form.code, &user.id)
        .await
    {
        Ok(Some(pair)) => {
            let _ = store
                .audit(
                    Some(&user.id),
                    "device.pairing_approved",
                    json!({"deviceName": pair.device_name}),
                )
                .await;
            redirect_pair(&form.pairing_id, "approved", "1")
        }
        Ok(None) => redirect_pair(
            &form.pairing_id,
            "error",
            "Invalid or expired pairing request/code.",
        ),
        Err(_) => error(StatusCode::SERVICE_UNAVAILABLE, "pairing_unavailable"),
    }
}

pub async fn claim(State(config): State<AuthConfig>, Json(input): Json<ClaimPair>) -> Response {
    let Some(store) = config.store.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "pairing_unavailable");
    };
    if invalid_pair_id(&input.pairing_id) || !valid_code(&input.code) {
        return error(StatusCode::CONFLICT, "pairing_not_approved_or_expired");
    }
    match store
        .rate_limit("pair-claim-token", &input.pairing_id, 720, 600)
        .await
    {
        Ok(true) => {}
        Ok(false) => return error(StatusCode::TOO_MANY_REQUESTS, "pairing_rate_limited"),
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "pairing_unavailable"),
    }
    let (credential_id, secret_hash, plain_secret) = if input.credential_id.is_empty()
        && input.credential_secret_hash.is_empty()
    {
        // Legacy CLI compatibility. New CLIs should supply retry-safe credential details.
        let mut id_bytes = [0u8; 16];
        let mut secret_bytes = [0u8; 40];
        if getrandom::fill(&mut id_bytes).is_err() || getrandom::fill(&mut secret_bytes).is_err() {
            return error(StatusCode::SERVICE_UNAVAILABLE, "pairing_failed");
        }
        let secret = hex::encode(secret_bytes);
        (
            format!("cld_{}", hex::encode(id_bytes)),
            hex::encode(Sha256::digest(secret.as_bytes())),
            Some(secret),
        )
    } else if input.credential_id.starts_with("cld_")
        && input.credential_id.len() <= 128
        && input.credential_secret_hash.len() == 64
        && hex::decode(&input.credential_secret_hash).is_ok_and(|value| value.len() == 32)
    {
        (
            input.credential_id,
            input.credential_secret_hash.to_lowercase(),
            None,
        )
    } else {
        return error(StatusCode::BAD_REQUEST, "invalid_credential_claim");
    };
    let key = input.device_public_key.trim();
    if !key.is_empty()
        && URL_SAFE_NO_PAD
            .decode(key)
            .map_or(true, |value| value.len() != 32)
    {
        return error(StatusCode::BAD_REQUEST, "invalid_device_public_key");
    }
    let public_key = if key.is_empty() { None } else { Some(key) };
    let device = match store
        .claim_pairing(
            &input.pairing_id,
            &input.code,
            &credential_id,
            &secret_hash,
            public_key,
        )
        .await
    {
        Ok(Some(device)) => device,
        Ok(None) => return error(StatusCode::CONFLICT, "pairing_not_approved_or_expired"),
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "pairing_failed"),
    };
    let email = match store.by_id(&device.user_id).await {
        Ok(Some(user)) => Some(user.email),
        _ => None,
    };
    let _ = store
        .audit(
            Some(&device.user_id),
            "device.paired",
            json!({
                "credentialId": credential_id, "deviceName": device.device_name
            }),
        )
        .await;
    let mut output = json!({
        "credentialId": device.credential_id,
        "deviceId": device.device_id,
        "deviceName": device.device_name
    });
    if let Some(email) = email {
        output["email"] = json!(email);
    }
    if let Some(secret) = plain_secret {
        output["credentialSecret"] = json!(secret);
    }
    Json(output).into_response()
}

pub async fn auth_check(State(config): State<AuthConfig>, headers: HeaderMap) -> Response {
    let mut response = auth_check_inner(config, headers).await;
    response.headers_mut().insert(
        "x-codelocal-auth-check",
        HeaderValue::from_static("credential-v1"),
    );
    response
}

async fn auth_check_inner(config: AuthConfig, headers: HeaderMap) -> Response {
    let credential_id = headers
        .get("x-codelocal-credential-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let secret = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Device "))
        .unwrap_or_default();
    if credential_id.is_empty() || secret.is_empty() || secret.len() > 256 {
        return error(StatusCode::UNAUTHORIZED, "credential_invalid");
    }
    let Some(store) = config.store.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "credential_unavailable");
    };
    let hash = hex::encode(Sha256::digest(secret.as_bytes()));
    let device = match store.authenticate_device(credential_id, &hash).await {
        Ok(Some(device)) => device,
        Ok(None) => return error(StatusCode::UNAUTHORIZED, "credential_invalid"),
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "credential_unavailable"),
    };
    let mut output = json!({"ok": true, "deviceId": device.device_id, "now": now_millis()});
    if let Ok(Some(user)) = store.by_id(&device.user_id).await {
        output["email"] = json!(user.email);
    }
    Json(output).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_pairing_and_public_keys() {
        assert!(invalid_pair_id("too-short"));
        assert!(!invalid_pair_id("a01234567890abcdef01234567890abc"));
        assert!(valid_code("123456"));
        assert!(!valid_code("12345"));
        assert!(!valid_code("12345a"));
        let code = random_pair_code().unwrap();
        assert!(valid_code(&code));
    }
}
