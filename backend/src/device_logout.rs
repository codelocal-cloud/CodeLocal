//! Explicit beta credential sign-out. Requires signed, replay-protected request.
//! A successful revocation immediately invalidates authenticated device requests.
use crate::{auth::AuthConfig, auth_session::error, workspaces::authorize_signed_device};
use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use serde_json::json;

pub async fn logout(
    State(config): State<AuthConfig>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    let Some(store) = config.store.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "credential_unavailable");
    };
    let device = match authorize_signed_device(store, &headers, &uri, &body).await {
        Ok(device) => device,
        Err(response) => return response,
    };
    if body.len() > 1024 || serde_json::from_slice::<serde_json::Value>(&body).is_err() {
        return error(StatusCode::BAD_REQUEST, "invalid_request");
    }
    let revoked = match store
        .revoke_device(&device.user_id, &device.credential_id)
        .await
    {
        Ok(revoked) => revoked,
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "device_logout_failed"),
    };
    if !revoked {
        return error(StatusCode::CONFLICT, "device_already_revoked");
    }
    let _ = store
        .audit(
            Some(&device.user_id),
            "device.logout",
            json!({"deviceId":device.device_id, "credentialId":device.credential_id}),
        )
        .await;
    Json(json!({"ok":true,"revoked":true})).into_response()
}
