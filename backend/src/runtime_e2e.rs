//! Real loopback HTTP upgrade test against the disposable beta database/Redis.
use crate::{
    device_signing,
    store::{AuthStore, DeviceRecord, now_millis},
};
use axum::Router;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signer, SigningKey};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::{
    net::{TcpListener, TcpStream},
    time::{sleep, timeout},
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Error, Message, client::IntoClientRequest, http::Request},
};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
const PATH: &str = "/api/client/runtime/ws";

fn request(
    endpoint: &str,
    device: &DeviceRecord,
    secret: &str,
    key: &SigningKey,
    nonce: &str,
) -> Request<()> {
    let mut request = endpoint.into_client_request().unwrap();
    let ts = now_millis() / 1000;
    let signature = URL_SAFE_NO_PAD.encode(
        key.sign(&device_signing::canonical("GET", PATH, b"", ts, nonce))
            .to_bytes(),
    );
    let headers = request.headers_mut();
    for (name, value) in [
        ("x-codelocal-credential-id", device.credential_id.clone()),
        ("authorization", format!("Device {secret}")),
        ("x-codelocal-signature-timestamp", ts.to_string()),
        ("x-codelocal-signature-nonce", nonce.to_owned()),
        ("x-codelocal-signature", signature),
    ] {
        headers.insert(name, value.parse().unwrap());
    }
    request
}

fn nonce() -> String {
    let mut bytes = [0u8; 18];
    getrandom::fill(&mut bytes).unwrap();
    URL_SAFE_NO_PAD.encode(bytes)
}

async fn connect(request: Request<()>) -> Socket {
    timeout(Duration::from_secs(5), connect_async(request))
        .await
        .unwrap()
        .unwrap()
        .0
}

async fn send(socket: &mut Socket, value: Value) {
    timeout(
        Duration::from_secs(5),
        socket.send(Message::Text(value.to_string().into())),
    )
    .await
    .unwrap()
    .unwrap();
}

async fn receive(socket: &mut Socket) -> Message {
    timeout(Duration::from_secs(5), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
}

async fn wait_presence(store: &AuthStore, id: &str, expected: Option<&[String]>) {
    timeout(Duration::from_secs(5), async {
        loop {
            let state = store.runtime_presence(id).await.unwrap();
            if state.as_ref().map(|s| s.workspace_ids.as_slice()) == expected {
                return;
            }
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("runtime lease did not reach expected state");
}

pub(crate) async fn verify_socket(
    app: Router,
    store: &AuthStore,
    device: &DeviceRecord,
    secret: &str,
    key: &SigningKey,
    ids: &[String],
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("ws://{}{PATH}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let registration = |workspaces: &[String]| {
        json!({"type":"runtime_register",
        "credentialId":device.credential_id,"credentialSecret":secret,
        "deviceId":device.device_id,"workspaceIds":workspaces,"clientVersion":"ci"})
    };
    let mut rejected = connect(request(&endpoint, device, secret, key, &nonce())).await;
    send(
        &mut rejected,
        registration(&["unregistered-workspace".into()]),
    )
    .await;
    assert!(matches!(receive(&mut rejected).await, Message::Close(_)));
    wait_presence(store, &device.credential_id, None).await;

    let used_nonce = nonce();
    let mut socket = connect(request(&endpoint, device, secret, key, &used_nonce)).await;
    send(&mut socket, registration(ids)).await;
    let message = receive(&mut socket).await;
    let Message::Text(raw) = message else {
        panic!("missing registration acknowledgement");
    };
    let ack: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(ack["type"], "runtime_registered");
    wait_presence(store, &device.credential_id, Some(ids)).await;
    let replay = timeout(
        Duration::from_secs(5),
        connect_async(request(&endpoint, device, secret, key, &used_nonce)),
    )
    .await
    .unwrap();
    assert!(matches!(replay,Err(Error::Http(response)) if response.status().as_u16()==409));

    let ping = receive(&mut socket).await;
    let Message::Text(raw) = ping else {
        panic!("missing runtime heartbeat");
    };
    let ping: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(ping["type"], "runtime_ping");
    send(&mut socket, json!({"type":"runtime_pong"})).await;
    send(
        &mut socket,
        json!({"type":"runtime_sync","workspaceIds":[]}),
    )
    .await;
    wait_presence(store, &device.credential_id, Some(&[])).await;
    send(
        &mut socket,
        json!({"type":"runtime_sync","workspaceIds":ids}),
    )
    .await;
    wait_presence(store, &device.credential_id, Some(ids)).await;
    // Unsupported messages must close the presence-only socket.
    send(&mut socket, json!({"type":"unsupported_fixture"})).await;
    assert!(matches!(receive(&mut socket).await, Message::Close(_)));
    wait_presence(store, &device.credential_id, None).await;
    server.abort();
}
