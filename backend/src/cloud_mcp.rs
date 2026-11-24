//! Stateless, read-only beta MCP over HTTP. No remote runtime invocation.
use crate::{auth::AuthConfig, auth_session::error, public_url, store::AuthStore};
use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use std::collections::HashMap;

const VERSION: &str = "2025-06-18";

fn challenge(origin: &str) -> Response {
    let mut response = error(StatusCode::UNAUTHORIZED, "invalid_token");
    if let Ok(value) = HeaderValue::from_str(&format!(
        "Bearer resource_metadata=\"{origin}/.well-known/oauth-protected-resource/mcp\", scope=\"mcp:tools\""
    )) {
        response
            .headers_mut()
            .insert(header::WWW_AUTHENTICATE, value);
    }
    response
}

pub async fn metadata(State(config): State<AuthConfig>) -> Response {
    let Some(origin) = public_url::beta_origin().filter(|_| config.store.is_some()) else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "mcp_unavailable");
    };
    Json(
        json!({"resource":format!("{origin}/mcp"),"authorization_servers":[origin],
        "scopes_supported":["mcp:tools"],"bearer_methods_supported":["header"]}),
    )
    .into_response()
}

async fn authenticate(config: &AuthConfig, headers: &HeaderMap) -> Result<String, Box<Response>> {
    let Some(origin) = public_url::beta_origin() else {
        return Err(Box::new(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "mcp_unavailable",
        )));
    };
    if headers.contains_key(header::ORIGIN)
        && headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) != Some(origin.as_str())
    {
        return Err(Box::new(error(StatusCode::FORBIDDEN, "invalid_origin")));
    }
    let Some(store) = &config.store else {
        return Err(Box::new(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "mcp_unavailable",
        )));
    };
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("Bearer"))
        .map(|(_, token)| token)
        .filter(|token| {
            token.len() == 43
                && URL_SAFE_NO_PAD
                    .decode(token)
                    .is_ok_and(|bytes| bytes.len() == 32)
        });
    let Some(token) = token else {
        return Err(Box::new(challenge(&origin)));
    };
    match store.rate_limit("beta-mcp", "global", 600, 60).await {
        Ok(true) => (),
        Ok(false) => {
            return Err(Box::new(error(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
            )));
        }
        Err(_) => {
            return Err(Box::new(error(
                StatusCode::SERVICE_UNAVAILABLE,
                "mcp_unavailable",
            )));
        }
    }
    let user = match store
        .oauth_access_account(token, &format!("{origin}/mcp"))
        .await
    {
        Ok(Some(user)) => user,
        Ok(None) => return Err(Box::new(challenge(&origin))),
        Err(_) => {
            return Err(Box::new(error(
                StatusCode::SERVICE_UNAVAILABLE,
                "mcp_unavailable",
            )));
        }
    };
    match store.rate_limit("beta-mcp-account", &user, 120, 60).await {
        Ok(true) => Ok(user),
        Ok(false) => Err(Box::new(error(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
        ))),
        Err(_) => Err(Box::new(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "mcp_unavailable",
        ))),
    }
}

fn rpc(payload: Value) -> Response {
    let mut response = Json(payload).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
fn fail(id: Value, code: i64, message: &str) -> Response {
    rpc(json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}}))
}
fn success(id: Value, result: Value) -> Response {
    rpc(json!({"jsonrpc":"2.0","id":id,"result":result}))
}

/// Go-compatible payload estimate: roughly four Unicode scalar values per
/// token. Serialized byte length is tracked separately.
fn estimate_payload(value: &Value) -> (i64, i64) {
    let serialized = value.to_string();
    (
        serialized.len() as i64,
        serialized.chars().count().div_ceil(4) as i64,
    )
}

fn tools() -> Value {
    json!({"tools":[{"name":"workspace","description":"List only your registered workspace metadata. No filesystem or remote execution is available.",
        "inputSchema":{"type":"object","properties":{"action":{"type":"string","enum":["list"]}},"required":["action"],"additionalProperties":false},
        "annotations":{"readOnlyHint":true,"destructiveHint":false,"openWorldHint":false}}]})
}

async fn workspace_list(store: &AuthStore, user: &str) -> Result<Value, ()> {
    let rows = store.list_workspace_catalog(user).await.map_err(|_| ())?;
    if rows.len() >= 1000 {
        return Err(());
    }
    let mut presence = HashMap::new();
    let mut items = Vec::with_capacity(rows.len());
    for w in rows {
        if w.managed || w.system_app {
            continue;
        }
        if !presence.contains_key(&w.credential_id) {
            let lease = store
                .runtime_presence(&w.credential_id)
                .await
                .map_err(|_| ())?;
            presence.insert(w.credential_id.clone(), lease);
        }
        let online = presence
            .get(&w.credential_id)
            .and_then(Option::as_ref)
            .is_some_and(|lease| lease.workspace_ids.contains(&w.workspace_id));
        items.push(json!({"deviceId":w.device_id,"deviceName":w.device_name,
            "workspaceId":w.workspace_id,"workspaceName":w.workspace_name,
            "runtimeOnline":online,"status":if online {"active"}else{"offline"}}));
    }
    Ok(json!({"items":items,"remoteExecutionAvailable":false}))
}

pub async fn get(State(config): State<AuthConfig>, headers: HeaderMap) -> Response {
    if let Err(response) = authenticate(&config, &headers).await {
        return *response;
    }
    let mut response = StatusCode::METHOD_NOT_ALLOWED.into_response();
    response
        .headers_mut()
        .insert(header::ALLOW, HeaderValue::from_static("POST"));
    response
}

pub async fn post(State(config): State<AuthConfig>, headers: HeaderMap, body: Bytes) -> Response {
    let user = match authenticate(&config, &headers).await {
        Ok(user) => user,
        Err(response) => return *response,
    };
    if headers
        .get("mcp-protocol-version")
        .is_some_and(|v| v.as_bytes() != VERSION.as_bytes())
    {
        return error(StatusCode::BAD_REQUEST, "unsupported_protocol_version");
    }
    if !headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(';')
                .next()
                .is_some_and(|v| v.trim().eq_ignore_ascii_case("application/json"))
        })
    {
        return error(StatusCode::UNSUPPORTED_MEDIA_TYPE, "json_required");
    }
    let request: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => return fail(Value::Null, -32700, "Parse error"),
    };
    if !request.is_object() || request["jsonrpc"] != "2.0" || !request["method"].is_string() {
        return fail(Value::Null, -32600, "Invalid Request");
    }
    let method = request["method"].as_str().unwrap_or_default();
    let Some(id) = request.get("id").cloned() else {
        // Notifications never invoke tools.
        return StatusCode::ACCEPTED.into_response();
    };
    if !(id.is_string() || id.as_i64().is_some() || id.as_u64().is_some()) {
        return fail(Value::Null, -32600, "Invalid Request ID");
    }
    match method {
        "initialize" => success(
            id,
            json!({"protocolVersion":VERSION,"capabilities":{"tools":{}},
            "serverInfo":{"name":"codelocal-beta","version":env!("CARGO_PKG_VERSION")},
            "instructions":"Read-only workspace metadata. No remote execution."}),
        ),
        "ping" => success(id, json!({})),
        "tools/list" => success(id, tools()),
        "tools/call" => {
            let params = &request["params"];
            if params["name"] != "workspace" || params["arguments"] != json!({"action":"list"}) {
                return fail(id, -32602, "Only workspace list is available");
            }
            let Some(store) = &config.store else {
                return fail(id, -32603, "Store unavailable");
            };
            let result = match workspace_list(store, &user).await {
                Ok(result) => {
                    json!({"content":[{"type":"text","text":result.to_string()}],"isError":false})
                }
                Err(()) => return fail(id, -32603, "Workspace catalog unavailable"),
            };
            // Meter the actual successful tool payload, not provider billing or
            // authentication traffic. Persist before returning a successful result.
            let (input_bytes, input_tokens_est) = estimate_payload(&params["arguments"]);
            let (output_bytes, output_tokens_est) = estimate_payload(&result);
            let mut event_bytes = [0u8; 16];
            if getrandom::fill(&mut event_bytes).is_err() {
                return fail(id, -32603, "Usage event unavailable");
            }
            let event_id = format!("mcp-{}", hex::encode(event_bytes));
            let event = crate::store::McpUsageEvent {
                event_id: &event_id,
                user_id: &user,
                device_id: "",
                workspace_id: "",
                tool: "workspace",
                calls: 1,
                input_bytes,
                output_bytes,
                input_tokens_est,
                output_tokens_est,
                created_at: crate::store::now_millis(),
            };
            match store.record_mcp_usage(&event).await {
                Ok(true) => success(id, result),
                _ => fail(id, -32603, "Usage recording unavailable"),
            }
        }
        _ => fail(id, -32601, "Method not found"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn payload_estimates_are_unicode_aware() {
        let value = json!({"message":"xin chào Việt Nam"});
        let serialized = value.to_string();
        let (bytes, tokens) = estimate_payload(&value);
        assert_eq!(bytes, serialized.len() as i64);
        assert_eq!(tokens, serialized.chars().count().div_ceil(4) as i64);
        assert!(bytes > serialized.chars().count() as i64);
        assert_eq!(estimate_payload(&json!({})), (2, 1));
    }

    #[test]
    fn catalog_exposes_only_read_only_metadata() {
        let catalog = tools();
        let list = catalog["tools"].as_array().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["name"], "workspace");
        assert_eq!(
            list[0]["inputSchema"]["properties"]["action"]["enum"],
            json!(["list"])
        );
    }
}
