//! Beta native-runtime WebSocket presence: signed GET + one-use Redis nonce
//! and an authenticated registration frame. NO remote tool/terminal/agent
//! dispatch is enabled here; presence is a liveness signal only.
use crate::{
    auth::AuthConfig,
    auth_session::error,
    store::{AuthStore, DeviceRecord, now_millis},
    workspaces::authorize_signed_request,
};
use axum::{
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio::time::{Instant, interval, timeout};

const MAX_WS_FRAME: usize = 16 * 1024;
const REGISTER_TIMEOUT: Duration = Duration::from_secs(10);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(35);

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RuntimeEnvelope {
    #[serde(rename = "type")]
    pub kind: String,
    pub client_version: String,
    pub credential_id: String,
    pub credential_secret: String,
    pub device_id: String,
    pub workspace_ids: Vec<String>,
    pub now: i64,
}

pub(crate) fn valid_workspace_id(value: &str) -> bool {
    !value.is_empty()
        && !matches!(value, "." | "..")
        && value.len() <= 80
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

pub(crate) fn valid_workspace_ids(values: &[String]) -> bool {
    if values.len() > 100 {
        return false;
    }
    let mut seen = HashSet::new();
    values
        .iter()
        .all(|id| valid_workspace_id(id) && seen.insert(id.as_str()))
}

fn subset_of_authorized(workspaces: &[String], registered: &[String]) -> bool {
    valid_workspace_ids(workspaces)
        && workspaces
            .iter()
            .all(|id| registered.iter().any(|known| known == id))
}

fn valid_registration(register: &RuntimeEnvelope, device: &DeviceRecord) -> bool {
    if register.kind != "runtime_register"
        || register.credential_id != device.credential_id
        || register.device_id != device.device_id
        || register.credential_secret.len() != 80
        || register.client_version.len() > 80
        || register.client_version.chars().any(char::is_control)
    {
        return false;
    }
    let presented_hash = hex::encode(Sha256::digest(register.credential_secret.as_bytes()));
    bool::from(subtle::ConstantTimeEq::ct_eq(
        presented_hash.as_bytes(),
        device.secret_hash.as_bytes(),
    ))
}

pub async fn connect(
    State(config): State<AuthConfig>,
    headers: HeaderMap,
    uri: Uri,
    ws: WebSocketUpgrade,
) -> Response {
    let Some(store) = config.store else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "runtime_store_unavailable");
    };
    // Upgrade is never granted to unauthenticated or replayed requests.
    // The same Ed25519 canonical request used for workspace sync signs GET
    // /api/client/runtime/ws with an empty body.
    let device = match authorize_signed_request(&store, &headers, &uri, "GET", b"").await {
        Ok(device) => device,
        Err(response) => return *response,
    };
    ws.max_message_size(MAX_WS_FRAME)
        .max_frame_size(MAX_WS_FRAME)
        .on_upgrade(move |socket| runtime_session(socket, store, device))
        .into_response()
}

async fn send_json(socket: &mut WebSocket, data: serde_json::Value) -> Result<(), ()> {
    let serialized = serde_json::to_string(&data).map_err(|_| ())?;
    socket
        .send(Message::Text(serialized.into()))
        .await
        .map_err(|_| ())
}

async fn allowed_runtime_workspaces(
    store: &AuthStore,
    device: &DeviceRecord,
    requested: &[String],
) -> bool {
    match store
        .registered_runtime_workspaces(&device.user_id, &device.device_id)
        .await
    {
        Ok(known) => subset_of_authorized(requested, &known),
        Err(_) => false,
    }
}

async fn runtime_session(mut socket: WebSocket, store: Arc<AuthStore>, device: DeviceRecord) {
    let registration = match timeout(REGISTER_TIMEOUT, socket.recv()).await {
        Ok(Some(Ok(Message::Text(raw)))) if raw.len() <= MAX_WS_FRAME => {
            serde_json::from_str::<RuntimeEnvelope>(raw.as_str()).ok()
        }
        _ => None,
    };
    let Some(register) = registration else {
        let _ = socket.send(Message::Close(None)).await;
        return;
    };
    if !valid_registration(&register, &device)
        || !allowed_runtime_workspaces(&store, &device, &register.workspace_ids).await
    {
        let _ = socket.send(Message::Close(None)).await;
        return;
    }
    let valid_device = store
        .authenticate_device(&device.credential_id, &device.secret_hash)
        .await
        .ok()
        .flatten()
        .is_some();
    if !valid_device {
        let _ = socket.send(Message::Close(None)).await;
        return;
    }
    let mut workspaces = register.workspace_ids;
    let Ok(lease) = store
        .begin_runtime_presence(&device.credential_id, &workspaces)
        .await
    else {
        let _ = socket.send(Message::Close(None)).await;
        return;
    };
    let _ = store
        .audit(
            Some(&device.user_id),
            "runtime.connected",
            json!({"deviceId":device.device_id,"workspaceCount":workspaces.len()}),
        )
        .await;
    if send_json(
        &mut socket,
        json!({"type":"runtime_registered","now":now_millis()}),
    )
    .await
    .is_err()
    {
        let _ = store
            .clear_runtime_presence(&device.credential_id, &lease)
            .await;
        return;
    }

    // Only liveness messages may be processed in this beta phase.
    // runtime_event, activation_result and command payloads are NOT executed.
    let mut tick = interval(HEARTBEAT_INTERVAL);
    let mut last_response = Instant::now();
    loop {
        tokio::select! {
            _ = tick.tick() => {
                if last_response.elapsed() > HEARTBEAT_TIMEOUT {
                    break;
                }
                let live = store.authenticate_device(&device.credential_id, &device.secret_hash)
                    .await.ok().flatten().is_some();
                if !live || send_json(&mut socket, json!({
                    "type":"runtime_ping","now":now_millis()
                })).await.is_err() {
                    break;
                }
            }
            next = socket.recv() => {
                let Ok(frame) = next.transpose() else { break };
                match frame {
                    Some(Message::Text(raw)) if raw.len() <= MAX_WS_FRAME => {
                        let Ok(envelope) = serde_json::from_str::<RuntimeEnvelope>(raw.as_str())
                        else { break };
                        if !matches!(envelope.kind.as_str(), "runtime_sync" | "runtime_pong") {
                            // A privileged frame is unsupported: close rather than
                            // silently claiming that activation has succeeded.
                            break;
                        }
                        let requested = if envelope.kind == "runtime_sync" {
                            envelope.workspace_ids
                        } else {
                            workspaces.clone()
                        };
                        if !allowed_runtime_workspaces(&store, &device, &requested).await {
                            break;
                        }
                        let live = store.authenticate_device(&device.credential_id, &device.secret_hash)
                            .await.ok().flatten().is_some();
                        if !live { break; }
                        match store.refresh_runtime_presence(&device.credential_id, &lease, &requested).await {
                            Ok(true) => {
                                workspaces = requested;
                                last_response = Instant::now();
                            }
                            _ => break,
                        }
                    }
                    Some(Message::Ping(contents)) => {
                        if socket.send(Message::Pong(contents)).await.is_err() {
                            break;
                        }
                    }
                    Some(Message::Pong(_)) => {
                        // Raw WebSocket pong does not refresh the Redis lease.
                        // Only an authenticated runtime_pong message may.
                    }
                    _ => break,
                }
            }
        }
    }
    let _ = store
        .clear_runtime_presence(&device.credential_id, &lease)
        .await;
    let _ = socket.send(Message::Close(None)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device() -> DeviceRecord {
        DeviceRecord {
            credential_id: "cld_0123456789abcdef".into(),
            user_id: "tester".into(),
            device_id: "test-device".into(),
            device_name: "Mac".into(),
            public_key: None,
            secret_hash: hex::encode(Sha256::digest(b"secret")),
            revoked_at: None,
            created_at: 0,
            last_seen_at: 0,
        }
    }

    #[test]
    fn register_requires_matching_device_secret_and_explicit_workspace_grants() {
        let valid = RuntimeEnvelope {
            kind: "runtime_register".into(),
            credential_id: "cld_0123456789abcdef".into(),
            credential_secret: "secret".into(),
            device_id: "test-device".into(),
            workspace_ids: vec!["repo-one".into()],
            client_version: "0.1".into(),
            now: 0,
        };
        let mut d = device();
        assert!(!valid_registration(&valid, &d)); // Secret must be 80 chars like real credential.
        d.secret_hash = hex::encode(Sha256::digest("a".repeat(80).as_bytes()));
        let mut valid = RuntimeEnvelope {
            credential_secret: "a".repeat(80),
            ..valid
        };
        assert!(valid_registration(&valid, &d));
        assert!(subset_of_authorized(
            &valid.workspace_ids,
            &["repo-one".into()]
        ));
        assert!(!subset_of_authorized(
            &valid.workspace_ids,
            &["repo-two".into()]
        ));
        valid.device_id = "other-device".into();
        assert!(!valid_registration(&valid, &d));
    }

    #[test]
    fn invalid_workspaces_and_excessive_ids_are_rejected() {
        assert!(valid_workspace_ids(&[]));
        assert!(valid_workspace_ids(&["a".into(), "second.repo".into()]));
        assert!(!valid_workspace_ids(&["same".into(), "same".into()]));
        assert!(!valid_workspace_ids(&["../secret".into()]));
        assert!(!valid_workspace_ids(&vec!["a".into(); 101]));
        assert!(!valid_workspace_id("has:colon"));
    }

    #[tokio::test]
    async fn malformed_upgrade_without_onupgrade_is_rejected() {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;
        let response = crate::router()
            .oneshot(
                Request::builder()
                    .uri("/api/client/runtime/ws")
                    .header("connection", "Upgrade")
                    .header("upgrade", "websocket")
                    .header("sec-websocket-version", "13")
                    .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UPGRADE_REQUIRED);
    }
}
