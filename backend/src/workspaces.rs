//! Signed device -> Cloud workspace metadata synchronization.
//! No native file/terminal operations are available in this beta endpoint.
use crate::{
    auth::AuthConfig,
    auth_session::{error, identify},
    device_signing::{self, DeviceProof},
    store::{AuthStore, DeviceRecord, WorkspaceMetadata, now_millis},
};
use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncBody {
    #[serde(default)]
    client_version: String,
    workspaces: Vec<WorkspaceMetadata>,
}

fn valid_workspace_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
}
fn validate_sync(input: &SyncBody) -> bool {
    if input.workspaces.len() > 100
        || input.client_version.len() > 80
        || input.client_version.chars().any(char::is_control)
    {
        return false;
    }
    let mut ids = HashSet::new();
    input.workspaces.iter().all(|w| {
        valid_workspace_id(&w.workspace_id)
            && !w.workspace_name.trim().is_empty()
            && w.workspace_name.len() <= 120
            && !w.workspace_name.chars().any(char::is_control)
            && !w.managed
            && !w.system_app
            && ids.insert(w.workspace_id.as_str())
    })
}

fn required_header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(name)?
        .to_str()
        .ok()
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn presented_credential(headers: &HeaderMap) -> Option<(&str, &str)> {
    let id = required_header(headers, "x-codelocal-credential-id")?;
    let secret =
        required_header(headers, header::AUTHORIZATION.as_str())?.strip_prefix("Device ")?;
    if !id.starts_with("cld_") || id.len() > 128 || secret.len() > 256 || secret.is_empty() {
        return None;
    }
    Some((id, secret))
}

async fn authorize_signed_device(
    store: &AuthStore,
    headers: &HeaderMap,
    uri: &Uri,
    body: &[u8],
) -> Result<DeviceRecord, Response> {
    let Some((id, secret)) = presented_credential(headers) else {
        return Err(error(StatusCode::UNAUTHORIZED, "credential_invalid"));
    };
    let secret_hash = hex::encode(Sha256::digest(secret.as_bytes()));
    let device = match store.authenticate_device(id, &secret_hash).await {
        Ok(Some(device)) => device,
        Ok(None) => return Err(error(StatusCode::UNAUTHORIZED, "credential_invalid")),
        Err(_) => {
            return Err(error(
                StatusCode::SERVICE_UNAVAILABLE,
                "credential_unavailable",
            ));
        }
    };
    let Some(public_key) = device.public_key.as_deref() else {
        return Err(error(
            StatusCode::UNAUTHORIZED,
            "device_public_key_required",
        ));
    };
    let timestamp = required_header(headers, "x-codelocal-signature-timestamp").unwrap_or("");
    let nonce = required_header(headers, "x-codelocal-signature-nonce").unwrap_or("");
    let signature = required_header(headers, "x-codelocal-signature").unwrap_or("");
    let path = uri.path_and_query().map_or(uri.path(), |v| v.as_str());
    let proof = DeviceProof {
        method: "POST",
        request_uri: path,
        timestamp,
        nonce,
        signature,
        body,
    };
    if device_signing::verify(public_key, proof, now_millis() / 1000).is_err() {
        return Err(error(StatusCode::UNAUTHORIZED, "invalid_device_signature"));
    }
    match store.consume_device_nonce(id, nonce).await {
        Ok(true) => Ok(device),
        Ok(false) => Err(error(StatusCode::CONFLICT, "device_signature_replayed")),
        Err(_) => Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "replay_protection_unavailable",
        )),
    }
}

pub async fn sync(
    State(config): State<AuthConfig>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    let Some(store) = &config.store else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "workspace_sync_unavailable",
        );
    };
    let device = match authorize_signed_device(store, &headers, &uri, &body).await {
        Ok(device) => device,
        Err(response) => return response,
    };
    let input: SyncBody = match serde_json::from_slice(&body) {
        Ok(input) => input,
        Err(_) => return error(StatusCode::BAD_REQUEST, "invalid_request"),
    };
    if !validate_sync(&input) {
        return error(StatusCode::BAD_REQUEST, "invalid_workspace_sync");
    }
    let removed = match store.sync_workspaces(&device, &input.workspaces).await {
        Ok(removed) => removed,
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "workspace_sync_failed"),
    };
    let _ = store
        .audit(
            Some(&device.user_id),
            "runtime.workspaces_synced",
            json!({
                "deviceId":device.device_id,"count": input.workspaces.len(),
            }),
        )
        .await;
    Json(json!({
        "synced": input.workspaces.len(),
        "syncedAt": now_millis(),
        "removed": removed,
        "knowledge": {},
        "portableSkills": {},
        "runtimeSettings": {},
        "projectBrain":{"cloudSyncEnabled":false},
        "note":"beta metadata-only sync; no runtime presence established"
    }))
    .into_response()
}

/// Authenticated dashboard catalog in the pre-existing Next.js DTO shape.
pub async fn catalog(State(config): State<AuthConfig>, headers: HeaderMap) -> Response {
    let Some(store) = &config.store else {
        return error(StatusCode::UNAUTHORIZED, "unauthorized");
    };
    let user = match identify(store, &config, &headers).await {
        Ok(Some((_, _, user))) => user,
        Ok(None) => return error(StatusCode::UNAUTHORIZED, "unauthorized"),
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "identity_unavailable"),
    };
    let rows = match store.list_workspace_catalog(&user.id).await {
        Ok(rows) => rows,
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "workspaces_unavailable"),
    };
    let total = rows.len();
    let items = rows
        .into_iter()
        .map(|w| {
            json!({
                "deviceId": w.device_id,
                "deviceName": w.device_name,
                "workspaceId": w.workspace_id,
                "workspaceName": w.workspace_name,
                "systemApp": w.system_app,
                "managed": w.managed,
                "status": "offline",
                "runtimeOnline": false,
                "lastSeenAt": w.last_seen_at,
            })
        })
        .collect::<Vec<_>>();
    Json(json!({
        "summary": {"total":total, "active":0, "sleeping":0, "offline":total},
        "items": items
    }))
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_workspace_impersonation_and_unsafe_metadata() {
        let valid = SyncBody {
            client_version: "0.1.0".into(),
            workspaces: vec![WorkspaceMetadata {
                workspace_id: "my-repo-00adbc".into(),
                workspace_name: "My Repo".into(),
                managed: false,
                system_app: false,
            }],
        };
        assert!(validate_sync(&valid));
        assert!(!valid_workspace_id("../secret"));
        assert!(!valid_workspace_id("/etc/passwd"));
        assert!(!valid_workspace_id("bad?name"));
        assert!(validate_sync(&SyncBody {
            workspaces: Vec::new(),
            client_version: "".into()
        }));
        let duplicates = SyncBody {
            client_version: "".into(),
            workspaces: vec![valid.workspaces[0].clone(), valid.workspaces[0].clone()],
        };
        assert!(!validate_sync(&duplicates));
        let privilege = SyncBody {
            client_version: "".into(),
            workspaces: vec![WorkspaceMetadata {
                managed: true,
                ..valid.workspaces[0].clone()
            }],
        };
        assert!(!validate_sync(&privilege));
    }
}
