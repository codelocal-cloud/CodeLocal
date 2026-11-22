//! Local-only MCP JSON-RPC stdio server for Rust beta.
//! No network listener, remote execution, terminal, editing, or cloud access.
//! All filesystem tools require a user-granted beta workspace.
use crate::{
    approvals::{ApprovalStore, Decision, fresh_session},
    localfs::LocalReader,
    workspace::Registry,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{self, BufRead, BufReader, Read, Write};

const MAX_INPUT: u64 = 256 * 1024;
const PROTOCOL_VERSION: &str = "2025-06-18";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReadInput {
    workspace_id: String,
    path: String,
    start_line: Option<usize>,
    end_line: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReadManyInput {
    workspace_id: String,
    paths: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SearchInput {
    workspace_id: String,
    query: String,
    mode: String,
    max_results: Option<usize>,
}

fn catalog() -> Value {
    json!([
        {
            "name":"codelocal_workspace_list",
            "description":"List explicitly granted beta workspace IDs and names without local paths",
            "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
            "annotations":{"readOnlyHint":true}
        },
        {
            "name":"codelocal_local_info",
            "description":"Inspect file metadata and SHA-256 inside a granted local workspace",
            "inputSchema":{
                "type":"object",
                "properties":{"workspaceId":{"type":"string"},"path":{"type":"string"}},
                "required":["workspaceId","path"],"additionalProperties":false
            },
            "annotations":{"readOnlyHint":true}
        },
        {
            "name":"codelocal_local_read",
            "description":"Read bounded UTF-8 content or 1-based inclusive lines in a granted workspace",
            "inputSchema":{
                "type":"object",
                "properties":{
                    "workspaceId":{"type":"string"},"path":{"type":"string"},
                    "startLine":{"type":"integer","minimum":1},
                    "endLine":{"type":"integer","minimum":1}
                },
                "required":["workspaceId","path"],"additionalProperties":false
            },
            "annotations":{"readOnlyHint":true}
        },
        {
            "name":"codelocal_local_read_many",
            "description":"Read up to 20 granted local files in one bounded operation, after one exact human approval (2 MiB/file and 8 MiB total)",
            "inputSchema":{
                "type":"object",
                "properties":{
                    "workspaceId":{"type":"string"},
                    "paths":{"type":"array","items":{"type":"string"},"minItems":1,"maxItems":20}
                },
                "required":["workspaceId","paths"],"additionalProperties":false
            },
            "annotations":{"readOnlyHint":true}
        },
        {
            "name":"codelocal_local_search",
            "description":"Search filenames or fixed UTF-8 text with gitignore and sensitive-path filtering",
            "inputSchema":{
                "type":"object",
                "properties":{
                    "workspaceId":{"type":"string"},
                    "query":{"type":"string"},
                    "mode":{"type":"string","enum":["files","text"]},
                    "maxResults":{"type":"integer","minimum":1,"maximum":200}
                },
                "required":["workspaceId","query","mode"],"additionalProperties":false
            },
            "annotations":{"readOnlyHint":true}
        }
    ])
}

fn rpc_result(id: Value, result: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"result":result})
}
fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}
fn tool_reply(value: Result<Value, String>) -> Value {
    match value {
        Ok(data) => json!({"content":[{"type":"text","text":data.to_string()}],"isError":false}),
        Err(message) => json!({"content":[{"type":"text","text":message}],"isError":true}),
    }
}
fn call(name: &str, arguments: Value, registry: &Registry) -> Result<Value, String> {
    match name {
        "codelocal_workspace_list" => {
            if arguments.as_object().is_none_or(|v| !v.is_empty()) {
                return Err("workspace list does not take arguments".into());
            }
            let grants = registry
                .list()
                .map_err(|_| "beta workspace registry unavailable")?;
            Ok(json!({"workspaces":grants.iter().map(|w| json!({
                "workspaceId":w.workspace_id,"workspaceName":w.workspace_name
            })).collect::<Vec<_>>()}))
        }
        "codelocal_local_info" | "codelocal_local_read" => {
            let input: ReadInput =
                serde_json::from_value(arguments).map_err(|_| "invalid read arguments")?;
            let reader = LocalReader::from_registry(registry, &input.workspace_id)
                .map_err(|error| error.to_string())?;
            if name == "codelocal_local_info" {
                if input.start_line.is_some() || input.end_line.is_some() {
                    return Err("info accepts no line ranges".into());
                }
                serde_json::to_value(reader.info(&input.path).map_err(|e| e.to_string())?)
                    .map_err(|_| "could not serialize file info".into())
            } else {
                serde_json::to_value(
                    reader
                        .read(&input.path, input.start_line, input.end_line)
                        .map_err(|e| e.to_string())?,
                )
                .map_err(|_| "could not serialize local read".into())
            }
        }
        "codelocal_local_read_many" => {
            let input: ReadManyInput =
                serde_json::from_value(arguments).map_err(|_| "invalid read_many arguments")?;
            if input.paths.is_empty() {
                return Err("read_many requires at least one path".into());
            }
            let reader = LocalReader::from_registry(registry, &input.workspace_id)
                .map_err(|error| error.to_string())?;
            serde_json::to_value(reader.many(&input.paths).map_err(|e| e.to_string())?)
                .map_err(|_| "could not serialize batch read".into())
        }
        "codelocal_local_search" => {
            let input: SearchInput =
                serde_json::from_value(arguments).map_err(|_| "invalid search arguments")?;
            let reader = LocalReader::from_registry(registry, &input.workspace_id)
                .map_err(|error| error.to_string())?;
            let limit = input.max_results.unwrap_or(100);
            match input.mode.as_str() {
                "files" => serde_json::to_value(
                    reader
                        .search_files(&input.query, limit)
                        .map_err(|e| e.to_string())?,
                )
                .map_err(|_| "could not serialize filename matches".into()),
                "text" => serde_json::to_value(
                    reader
                        .search_text(&input.query, limit)
                        .map_err(|e| e.to_string())?,
                )
                .map_err(|_| "could not serialize content matches".into()),
                _ => Err("search mode must be files or text".into()),
            }
        }
        _ => Err("unknown local MCP tool".into()),
    }
}

/// Notifications have no ID and produce no response. Tools never bypass beta
/// workspace grants. Unknown JSON-RPC methods are rejected.
pub fn dispatch_with_approvals(
    request: &Value,
    registry: &Registry,
    approvals: &ApprovalStore,
    session: &str,
) -> Option<Value> {
    let id = request.get("id")?.clone();
    if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Some(rpc_error(id, -32600, "invalid JSON-RPC version"));
    }
    let Some(method) = request.get("method").and_then(Value::as_str) else {
        return Some(rpc_error(id, -32600, "missing method"));
    };
    Some(match method {
        "initialize" => rpc_result(
            id,
            json!({
                "protocolVersion":PROTOCOL_VERSION,
                "serverInfo":{"name":"codelocal-rust-beta-stdio","version":env!("CARGO_PKG_VERSION")},
                "capabilities":{"tools":{"listChanged":false}},
                "instructions":"Local-only, human-approved one-use read/search of granted beta workspaces; no remote execution."
            }),
        ),
        "ping" => rpc_result(id, json!({})),
        "tools/list" => rpc_result(id, json!({"tools":catalog()})),
        "tools/call" => {
            let params = request.get("params").and_then(Value::as_object);
            let Some(name) = params.and_then(|p| p.get("name")).and_then(Value::as_str) else {
                return Some(rpc_error(id, -32602, "missing tools/call name"));
            };
            let args = params
                .and_then(|p| p.get("arguments"))
                .cloned()
                .unwrap_or_else(|| json!({}));
            let approval_id = params
                .and_then(|p| p.get("approvalId"))
                .and_then(Value::as_str);
            rpc_result(
                id,
                tool_reply(gated_call(
                    name,
                    args,
                    registry,
                    approvals,
                    session,
                    approval_id,
                )),
            )
        }
        _ => rpc_error(id, -32601, "unknown method"),
    })
}

fn gated_call(
    name: &str,
    args: Value,
    registry: &Registry,
    approvals: &ApprovalStore,
    session: &str,
    approval_id: Option<&str>,
) -> Result<Value, String> {
    if name == "codelocal_workspace_list" {
        return call(name, args, registry);
    }
    if !matches!(
        name,
        "codelocal_local_info"
            | "codelocal_local_read"
            | "codelocal_local_read_many"
            | "codelocal_local_search"
    ) {
        return Err("unknown or disabled local MCP tool".into());
    }
    let workspace_id = args
        .get("workspaceId")
        .and_then(Value::as_str)
        .ok_or("a granted workspaceId is required")?;
    // Verify the exact current grant BEFORE creating any approval. A revoked
    // grant cannot be restored by a previously approved ticket.
    let granted = registry
        .list()
        .map_err(|e| e.to_string())?
        .into_iter()
        .any(|workspace| workspace.workspace_id == workspace_id);
    if !granted {
        return Err("workspace not currently granted in Rust beta".into());
    }
    match approvals.check_or_queue(session, name, &args, approval_id, workspace_id)? {
        Decision::Allowed => call(name, args, registry),
        Decision::Pending { id, expires_at } => Ok(json!({
            "status":"approval_required",
            "executionStarted":false,
            "approvalId":id,
            "expiresAt":expires_at,
            "message":"Review the exact request with codelocal approvals; approve or deny its ID. Retry this same tool call with params.approvalId."
        })),
        Decision::Denied => Err("approval was denied, expired or already consumed".into()),
    }
}

#[cfg(test)]
fn dispatch(request: &Value, registry: &Registry) -> Option<Value> {
    let approvals = ApprovalStore::new(registry.state_dir());
    dispatch_with_approvals(request, registry, &approvals, &"a".repeat(32))
}

pub fn serve_stdio() -> Result<(), String> {
    let registry = Registry::beta_default().map_err(|e| e.to_string())?;
    let approvals = ApprovalStore::new(registry.state_dir());
    let session = fresh_session()?;
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = BufReader::new(stdin.lock());
    let mut output = stdout.lock();
    loop {
        let mut line = Vec::new();
        let read = (&mut input)
            .take(MAX_INPUT + 1)
            .read_until(b'\n', &mut line)
            .map_err(|e| format!("MCP input read failed: {e}"))?;
        if read == 0 {
            break;
        }
        if read as u64 > MAX_INPUT {
            return Err("MCP input exceeds 256 KiB; closing session".into());
        }
        let request: Value = match serde_json::from_slice(&line) {
            Ok(value) => value,
            Err(_) => {
                output_line(&mut output, &rpc_error(Value::Null, -32700, "invalid JSON"))?;
                continue;
            }
        };
        if let Some(response) = dispatch_with_approvals(&request, &registry, &approvals, &session) {
            output_line(&mut output, &response)?;
        }
    }
    Ok(())
}

fn output_line(out: &mut impl Write, response: &Value) -> Result<(), String> {
    serde_json::to_writer(&mut *out, response).map_err(|e| format!("MCP JSON encode: {e}"))?;
    out.write_all(b"\n")
        .map_err(|e| format!("MCP stdout: {e}"))?;
    out.flush().map_err(|e| format!("MCP flush: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    fn setup() -> (tempfile::TempDir, Registry, String) {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("main.rs"), "hello source\n").unwrap();
        fs::write(project.join("readme.md"), "beta docs\n").unwrap();
        fs::write(project.join(".env"), "TOKEN=hidden\n").unwrap();
        let registry = Registry::new(temp.path().join("beta-state"));
        let grant = registry.grant(project).unwrap();
        (temp, registry, grant.workspace_id)
    }
    #[test]
    fn mcp_initialize_list_and_notifications_work() {
        let (_temp, r, _) = setup();
        let response =
            dispatch(&json!({"jsonrpc":"2.0","id":1,"method":"initialize"}), &r).unwrap();
        assert_eq!(response["result"]["protocolVersion"], PROTOCOL_VERSION);
        let tools = dispatch(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}), &r).unwrap();
        let names = tools["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(names.len(), 5);
        assert!(
            !names
                .iter()
                .any(|name| name.contains("edit") || name.contains("terminal"))
        );
        assert!(
            dispatch(
                &json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
                &r
            )
            .is_none()
        );
    }
    #[test]
    fn grant_checks_prevent_reading_secrets_and_unauthorized_roots() {
        let (_temp, r, id) = setup();
        let ws = dispatch(
            &json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":"codelocal_workspace_list","arguments":{}}}),
            &r,
        )
        .unwrap();
        assert!(!ws.to_string().contains("localPath"));
        let good = dispatch(&json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"codelocal_local_read","arguments":{"workspaceId":id,"path":"main.rs"}}}),&r).unwrap();
        assert_eq!(good["result"]["isError"], false);
        assert!(!good.to_string().contains("hello source"));
        let pending: Value =
            serde_json::from_str(good["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(pending["status"], "approval_required");
        assert_eq!(pending["executionStarted"], false);
        let ticket_id = pending["approvalId"].as_str().unwrap();
        let approvals = ApprovalStore::new(r.state_dir());
        assert_eq!(approvals.list().unwrap().len(), 1);
        assert!(approvals.decide(ticket_id, true).unwrap());
        let read = dispatch(
            &json!({"jsonrpc":"2.0","id":22,"method":"tools/call",
            "params":{"name":"codelocal_local_read",
                "approvalId":ticket_id,
                "arguments":{"workspaceId":id,"path":"main.rs"}}}),
            &r,
        )
        .unwrap();
        assert_eq!(read["result"]["isError"], false);
        assert!(read.to_string().contains("hello source"));
        let replay = dispatch(
            &json!({"jsonrpc":"2.0","id":23,"method":"tools/call",
            "params":{"name":"codelocal_local_read",
                "approvalId":ticket_id,
                "arguments":{"workspaceId":id,"path":"main.rs"}}}),
            &r,
        )
        .unwrap();
        assert_eq!(replay["result"]["isError"], true);
        assert!(!replay.to_string().contains("hello source"));
        let secret = dispatch(
            &json!({"jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{"name":"codelocal_local_read","arguments":{"workspaceId":id,"path":".env"}}}),
            &r,
        )
        .unwrap();
        let secret_pending: Value =
            serde_json::from_str(secret["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(secret_pending["status"], "approval_required");
        assert!(!secret.to_string().contains("TOKEN=hidden"));
        let secret_ticket = secret_pending["approvalId"].as_str().unwrap();
        assert!(approvals.decide(secret_ticket, true).unwrap());
        let secret_after_approval = dispatch(
            &json!({"jsonrpc":"2.0","id":31,"method":"tools/call",
            "params":{"name":"codelocal_local_read",
              "approvalId":secret_ticket,
              "arguments":{"workspaceId":id,"path":".env"}}}),
            &r,
        )
        .unwrap();
        assert_eq!(secret_after_approval["result"]["isError"], true);
        assert!(!secret_after_approval.to_string().contains("TOKEN=hidden"));
        let unknown = dispatch(&json!({"jsonrpc":"2.0","id":4,"method":"tools/call",
            "params":{"name":"codelocal_local_read","arguments":{"workspaceId":"ungranted","path":"main.rs"}}}),&r).unwrap();
        assert_eq!(unknown["result"]["isError"], true);
    }
    #[test]
    fn revoked_grant_invalidates_approved_ticket() {
        let (_temp, registry, id) = setup();
        let pending = dispatch(
            &json!({"jsonrpc":"2.0","id":5,"method":"tools/call",
            "params":{"name":"codelocal_local_info",
                "arguments":{"workspaceId":id,"path":"main.rs"}}}),
            &registry,
        )
        .unwrap();
        let details: Value =
            serde_json::from_str(pending["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap();
        let ticket = details["approvalId"].as_str().unwrap();
        let approvals = ApprovalStore::new(registry.state_dir());
        assert!(approvals.decide(ticket, true).unwrap());
        assert!(registry.revoke(&id).unwrap());
        let denied = dispatch(
            &json!({"jsonrpc":"2.0","id":6,"method":"tools/call",
            "params":{"name":"codelocal_local_info", "approvalId":ticket,
                "arguments":{"workspaceId":id,"path":"main.rs"}}}),
            &registry,
        )
        .unwrap();
        assert_eq!(denied["result"]["isError"], true);
        assert!(!denied.to_string().contains("hello source"));
    }

    #[test]
    fn batch_read_requires_one_exact_approval_and_blocks_sensitive_mixed_paths() {
        let (_temp, registry, id) = setup();
        let tools = dispatch(
            &json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
            &registry,
        )
        .unwrap();
        assert!(
            tools["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tool| tool["name"] == "codelocal_local_read_many")
        );
        let args = json!({"workspaceId":id,"paths":["main.rs","readme.md"]});
        let req = |approval: Option<&str>| {
            let mut params = json!({"name":"codelocal_local_read_many","arguments":args});
            if let Some(value) = approval {
                params["approvalId"] = Value::String(value.to_owned());
            }
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":params})
        };
        let pending = dispatch(&req(None), &registry).unwrap();
        let details: Value =
            serde_json::from_str(pending["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(details["status"], "approval_required");
        assert!(!pending.to_string().contains("hello source"));
        let ticket = details["approvalId"].as_str().unwrap();
        let approvals = ApprovalStore::new(registry.state_dir());
        assert!(approvals.decide(ticket, true).unwrap());
        let approved = dispatch(&req(Some(ticket)), &registry).unwrap();
        assert_eq!(approved["result"]["isError"], false);
        assert!(approved.to_string().contains("hello source"));
        assert!(approved.to_string().contains("beta docs"));
        let replay = dispatch(&req(Some(ticket)), &registry).unwrap();
        assert_eq!(replay["result"]["isError"], true);

        let sensitive_args = json!({"workspaceId":id,"paths":["main.rs",".env"]});
        let pending_bad = dispatch(
            &json!({"jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{"name":"codelocal_local_read_many","arguments":sensitive_args}}),
            &registry,
        )
        .unwrap();
        let info: Value = serde_json::from_str(
            pending_bad["result"]["content"][0]["text"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        let bad_ticket = info["approvalId"].as_str().unwrap();
        assert!(approvals.decide(bad_ticket, true).unwrap());
        let denied = dispatch(
            &json!({"jsonrpc":"2.0","id":4,"method":"tools/call",
            "params":{"name":"codelocal_local_read_many",
                "approvalId":bad_ticket,
                "arguments":sensitive_args}}),
            &registry,
        )
        .unwrap();
        assert_eq!(denied["result"]["isError"], true);
        assert!(!denied.to_string().contains("TOKEN=hidden"));
    }

    #[test]
    fn invalid_method_cannot_execute_terminal_and_stdout_is_framed() {
        let (_temp, r, _) = setup();
        let wrong = dispatch(
            &json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":"terminal","arguments":{"command":"whoami"}}}),
            &r,
        )
        .unwrap();
        assert_eq!(wrong["result"]["isError"], true);
        let unknown = dispatch(&json!({"jsonrpc":"2.0","id":2,"method":"unknown"}), &r).unwrap();
        assert_eq!(unknown["error"]["code"], -32601);
        let mut out = Vec::new();
        output_line(&mut out, &wrong).unwrap();
        let output = String::from_utf8(out).unwrap();
        assert_eq!(output.lines().count(), 1);
    }
}
