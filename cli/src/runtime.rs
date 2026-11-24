//! Rust-beta native runtime liveness client.
//! Uses an existing paired Ed25519 device and signs every WebSocket GET.
//! No remote activation, file, terminal or MCP event is executed.
use crate::{
    pair::{self, RuntimeAccess},
    workspace::Registry,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::{
    runtime::Builder,
    time::{Instant, interval, sleep},
};
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest, http};

const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(50);
const WORKSPACE_SYNC_INTERVAL: Duration = Duration::from_secs(30);

fn granted_workspace_ids() -> Result<Vec<String>, String> {
    let registry = Registry::beta_default().map_err(|error| error.to_string())?;
    let list = registry.list().map_err(|error| error.to_string())?;
    if list.len() > 100 {
        return Err("beta runtime permits at most 100 granted workspaces".into());
    }
    Ok(list
        .into_iter()
        .map(|workspace| workspace.workspace_id)
        .collect())
}

fn websocket_endpoint(origin: &str) -> Result<String, String> {
    if let Some(rest) = origin.strip_prefix("https://") {
        Ok(format!("wss://{rest}/api/client/runtime/ws"))
    } else if let Some(rest) = origin.strip_prefix("http://") {
        Ok(format!("ws://{rest}/api/client/runtime/ws"))
    } else {
        Err("invalid beta runtime API scheme".into())
    }
}

fn handshake_request(access: &RuntimeAccess) -> Result<http::Request<()>, String> {
    let endpoint = websocket_endpoint(&access.origin)?;
    let mut request = endpoint
        .into_client_request()
        .map_err(|_| "invalid beta WebSocket endpoint")?;
    for (name, value) in pair::runtime_get_headers(access)? {
        let name = http::header::HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| "invalid beta signature header")?;
        let value =
            http::HeaderValue::from_str(&value).map_err(|_| "invalid beta signature value")?;
        request.headers_mut().insert(name, value);
    }
    Ok(request)
}

fn registration(access: &RuntimeAccess, ids: &[String]) -> Value {
    json!({
        "type":"runtime_register",
        "credentialId":access.credential_id,
        "credentialSecret":access.credential_secret,
        "deviceId":access.device_id,
        "workspaceIds":ids,
        "clientVersion":env!("CARGO_PKG_VERSION"),
    })
}

fn is_registered(response: &Value) -> bool {
    response["type"].as_str() == Some("runtime_registered")
        && response["now"].as_i64().is_some_and(|now| now > 0)
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const SEND_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_FRAME: usize = 16 * 1024;
const STABLE_CONNECTION: Duration = Duration::from_secs(60);

type RuntimeSocket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

fn retry(message: &str) -> pair::SyncFailure {
    pair::SyncFailure::Retryable(message.into())
}

fn connect_failure(error: tokio_tungstenite::tungstenite::Error) -> pair::SyncFailure {
    use tokio_tungstenite::tungstenite::Error;
    match error {
        Error::Http(response) => {
            let status = response.status();
            let message = format!("beta runtime upgrade failed ({status})");
            if status.is_server_error() || matches!(status.as_u16(), 408 | 429) {
                pair::SyncFailure::Retryable(message)
            } else {
                pair::SyncFailure::Permanent(message)
            }
        }
        _ => retry("beta runtime WebSocket connection failed"),
    }
}

async fn sync_for(access: &RuntimeAccess) -> Result<Vec<String>, pair::SyncFailure> {
    let expected = access.credential_id.clone();
    tokio::task::spawn_blocking(move || pair::sync_metadata(Some(&expected)))
        .await
        .map_err(|_| "beta workspace sync worker failed")?
}

async fn send_frame(socket: &mut RuntimeSocket, message: Message) -> Result<(), pair::SyncFailure> {
    tokio::time::timeout(SEND_TIMEOUT, socket.send(message))
        .await
        .map_err(|_| retry("beta runtime send timed out"))?
        .map_err(|_| retry("beta runtime send failed"))
}

async fn connect_once(
    access: &RuntimeAccess,
    once: bool,
    mut ids: Vec<String>,
    registered_at: &mut Option<Instant>,
) -> Result<(), pair::SyncFailure> {
    let request = handshake_request(access)?;
    let config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(MAX_FRAME))
        .max_frame_size(Some(MAX_FRAME));
    let (mut socket, _) = tokio::time::timeout(
        CONNECT_TIMEOUT,
        tokio_tungstenite::connect_async_with_config(request, Some(config), false),
    )
    .await
    .map_err(|_| retry("beta runtime WebSocket connect timed out"))?
    .map_err(connect_failure)?;
    send_frame(
        &mut socket,
        Message::Text(registration(access, &ids).to_string().into()),
    )
    .await?;

    let registered = tokio::time::timeout(Duration::from_secs(12), socket.next())
        .await
        .map_err(|_| retry("beta runtime registration timed out"))?
        .ok_or_else(|| retry("beta runtime ended before registration"))?
        .map_err(|_| retry("beta runtime registration connection failed"))?;
    let Message::Text(raw) = registered else {
        return Err("beta runtime did not acknowledge device registration".into());
    };
    let response: Value =
        serde_json::from_str(raw.as_str()).map_err(|_| "beta runtime registration was not JSON")?;
    if !is_registered(&response) {
        return Err("beta runtime did not confirm registration".into());
    }
    *registered_at = Some(Instant::now());
    if once {
        send_frame(&mut socket, Message::Close(None)).await?;
        return Ok(());
    }
    eprintln!("Rust beta runtime connected: device presence only; no remote tools.");
    let mut tick = interval(WORKSPACE_SYNC_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_rx = Instant::now();
    loop {
        tokio::select! {
            event = socket.next() => {
                let Some(message) = event else {
                    return Err(retry("beta runtime connection closed"));
                };
                let message = message.map_err(|_| retry("beta runtime connection failed"))?;
                match message {
                    Message::Text(raw) => {
                        let value: Value = serde_json::from_str(raw.as_str())
                            .map_err(|_| "beta runtime returned invalid JSON")?;
                        match value["type"].as_str() {
                            Some("runtime_ping") => {
                                send_frame(&mut socket, Message::Text(json!({"type":"runtime_pong"}).to_string().into())).await?;
                                last_rx = Instant::now();
                            }
                            _ => return Err("unsupported remote runtime event rejected".into()),
                        }
                    }
                    Message::Ping(payload) => send_frame(&mut socket, Message::Pong(payload)).await?,
                    Message::Pong(_) => {}
                    Message::Close(_) => return Err(retry("beta runtime server closed connection")),
                    _ => return Err("unsupported runtime frame rejected".into()),
                }
            }
            _ = tokio::time::sleep_until(last_rx + HEARTBEAT_TIMEOUT) => {
                return Err(retry("beta runtime heartbeat expired"));
            }
            _ = tick.tick() => {
                if granted_workspace_ids()? != ids {
                    // Advertise the exact acknowledged snapshot, never a second
                    // registry read taken after the signed sync request.
                    let current = sync_for(access).await?;
                    send_frame(&mut socket, Message::Text(json!({
                        "type":"runtime_sync","workspaceIds":current
                    }).to_string().into())).await?;
                    ids = current;
                }
            }
        }
    }
}

/// Starts foreground liveness runtime, or makes one signed connection with --once.
pub fn start(once: bool) -> Result<(), String> {
    // Hold a fixed device identity for the lifetime of this process.
    let access = pair::runtime_access()?;
    let runtime = Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|_| "cannot create Rust beta runtime executor")?;
    runtime.block_on(async {
        let mut delay = 1u64;
        let stop = tokio::signal::ctrl_c();
        tokio::pin!(stop);
        loop {
            let mut registered_at = None;
            let result = tokio::select! {
                result = async {
                    let ids = sync_for(&access).await?;
                    connect_once(&access, once, ids, &mut registered_at).await
                } => result,
                _ = &mut stop => return Ok(()),
            };
            if once {
                return result.map_err(|err| err.to_string());
            }
            match result {
                Ok(()) => eprintln!("beta runtime connection ended"),
                Err(pair::SyncFailure::Permanent(message)) => return Err(message),
                Err(pair::SyncFailure::Retryable(message)) => {
                    eprintln!("beta runtime disconnected: {message}");
                }
            }
            if registered_at.is_some_and(|instant| instant.elapsed() >= STABLE_CONNECTION) {
                delay = 1;
            }
            eprintln!("reconnecting beta runtime in {} seconds", delay);
            tokio::select! {
                _ = sleep(Duration::from_secs(delay)) => {},
                _ = &mut stop => return Ok(()),
            }
            delay = (delay * 2).min(30);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use ed25519_dalek::{Signature, SigningKey, Verifier};
    use sha2::{Digest, Sha256};

    #[test]
    fn runtime_urls_and_signed_registration_are_bounded() {
        assert_eq!(
            websocket_endpoint("https://beta.example.com").unwrap(),
            "wss://beta.example.com/api/client/runtime/ws"
        );
        assert_eq!(
            websocket_endpoint("http://127.0.0.1:8080").unwrap(),
            "ws://127.0.0.1:8080/api/client/runtime/ws"
        );
        assert!(websocket_endpoint("file:///tmp").is_err());
        let access = RuntimeAccess {
            origin: "http://127.0.0.1:8080".into(),
            credential_id: "cld_123".into(),
            credential_secret: "a".repeat(80),
            device_id: "beta-test".into(),
            signing_key: SigningKey::from_bytes(&[25u8; 32]),
        };
        let registered = registration(&access, &["granted-id".into()]);
        assert_eq!(registered["type"], "runtime_register");
        assert_eq!(registered["workspaceIds"][0], "granted-id");
        assert!(!is_registered(&json!({"type":"runtime_event","now":42})));
        assert!(is_registered(
            &json!({"type":"runtime_registered","now":42})
        ));
        let req = handshake_request(&access).unwrap();
        assert_eq!(req.uri().path(), "/api/client/runtime/ws");
        assert!(req.headers().contains_key("x-codelocal-signature"));
        let get_header = |key: &str| req.headers().get(key).unwrap().to_str().unwrap();
        let nonce = get_header("x-codelocal-signature-nonce");
        let timestamp = get_header("x-codelocal-signature-timestamp");
        let digest = hex::encode(Sha256::digest([]));
        let payload = format!("GET\n/api/client/runtime/ws\n{timestamp}\n{nonce}\n{digest}");
        let raw = URL_SAFE_NO_PAD
            .decode(get_header("x-codelocal-signature"))
            .unwrap();
        let bytes: [u8; 64] = raw.try_into().unwrap();
        access
            .signing_key
            .verifying_key()
            .verify(payload.as_bytes(), &Signature::from_bytes(&bytes))
            .unwrap();
    }
}
