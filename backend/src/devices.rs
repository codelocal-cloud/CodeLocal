//! Actual beta device records for the preserved Next.js Devices page.
//! Runtime presence is not implemented. Never fabricate an online device.
use crate::{
    auth::AuthConfig,
    auth_session::{error, identify},
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::json;

pub async fn list(State(config): State<AuthConfig>, headers: HeaderMap) -> Response {
    let Some(store) = &config.store else {
        return error(StatusCode::UNAUTHORIZED, "unauthorized");
    };
    let user = match identify(store, &config, &headers).await {
        Ok(Some((_, _, user))) => user,
        Ok(None) => return error(StatusCode::UNAUTHORIZED, "unauthorized"),
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "identity_unavailable"),
    };
    let devices = match store.list_devices(&user.id).await {
        Ok(items) => items,
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "devices_unavailable"),
    };
    let mut paired = 0;
    let mut online = 0;
    let mut revoked = 0;
    let mut items = Vec::with_capacity(devices.len());
    for device in devices {
        let status = if device.revoked_at.is_some() {
            revoked += 1;
            "revoked"
        } else {
            paired += 1;
            match store.runtime_presence(&device.credential_id).await {
                Ok(Some(_)) => {
                    online += 1;
                    "online"
                }
                Ok(None) => "offline",
                Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "presence_unavailable"),
            }
        };
        items.push(json!({
            "deviceId": device.device_id,
            "deviceName": device.device_name,
            "status": status,
            "createdAt": device.created_at,
            "lastSeenAt": device.last_seen_at,
        }));
    }
    Json(json!({
        "summary": { "paired": paired, "online": online, "revoked": revoked },
        "items": items,
    }))
    .into_response()
}

#[derive(serde::Deserialize)]
pub struct RevokeForm {
    csrf: String,
}

/// Revokes only a device owned by the authenticated account.
pub async fn revoke(
    State(config): State<AuthConfig>,
    headers: HeaderMap,
    axum::extract::Path(device_id): axum::extract::Path<String>,
    axum::Form(form): axum::Form<RevokeForm>,
) -> Response {
    let Some(store) = &config.store else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "devices_unavailable");
    };
    let (session_id, session, user) = match identify(store, &config, &headers).await {
        Ok(Some(identity)) => identity,
        Ok(None) => return error(StatusCode::UNAUTHORIZED, "unauthorized"),
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "identity_unavailable"),
    };
    if session.risk_until > crate::store::now_millis() {
        let _ = store.delete_session(&session_id).await;
        return error(StatusCode::UNAUTHORIZED, "reauthentication_required");
    }
    if !crate::auth_session::csrf_matches(&session.csrf, &form.csrf) {
        return error(StatusCode::FORBIDDEN, "invalid_csrf");
    }
    let device_id = device_id.trim();
    if device_id.is_empty() || device_id.len() > 160 || device_id.chars().any(char::is_control) {
        return error(StatusCode::BAD_REQUEST, "invalid_device");
    }
    let devices = match store.list_devices(&user.id).await {
        Ok(items) => items,
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "devices_unavailable"),
    };
    let Some(device) = devices
        .into_iter()
        .find(|device| device.device_id == device_id && device.revoked_at.is_none())
    else {
        return error(StatusCode::NOT_FOUND, "device_not_found");
    };
    match store.revoke_device(&user.id, &device.credential_id).await {
        Ok(true) => {
            let _ = store
                .audit(
                    Some(&user.id),
                    "device.revoked",
                    json!({"credentialId": device.credential_id}),
                )
                .await;
            Json(json!({"ok": true, "deviceId": device_id})).into_response()
        }
        Ok(false) => error(StatusCode::CONFLICT, "device_already_revoked"),
        Err(_) => error(StatusCode::SERVICE_UNAVAILABLE, "device_revoke_failed"),
    }
}
