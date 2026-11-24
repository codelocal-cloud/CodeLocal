//! Account-scoped beta dashboard overview for the unchanged Next.js page.
//! Only observed devices, registered workspaces, and persisted usage appear.
//! Unported skills / model capabilities are not fabricated.
use crate::{
    auth::AuthConfig,
    auth_session::{error, identify},
    store::{AuthStore, DeviceRecord, RuntimePresence, UsageReport, WorkspaceRow},
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use std::collections::HashMap;

fn build_overview(
    email: &str,
    is_admin: bool,
    devices: Vec<DeviceRecord>,
    workspaces: Vec<WorkspaceRow>,
    usage: UsageReport,
    presence: &HashMap<String, Option<RuntimePresence>>,
) -> Value {
    let paired = devices
        .iter()
        .filter(|device| device.revoked_at.is_none())
        .count();
    let online = devices
        .iter()
        .filter(|device| {
            device.revoked_at.is_none()
                && presence
                    .get(&device.credential_id)
                    .is_some_and(Option::is_some)
        })
        .count();

    let total = workspaces.len();
    let mut active = 0_usize;
    let mut recent = Vec::with_capacity(total.min(5));
    for workspace in workspaces {
        let is_active = presence
            .get(&workspace.credential_id)
            .and_then(Option::as_ref)
            .is_some_and(|state| {
                state
                    .workspace_ids
                    .iter()
                    .any(|id| id == &workspace.workspace_id)
            });
        if is_active {
            active += 1;
        }
        if recent.len() < 5 {
            recent.push(json!({
                "deviceId": workspace.device_id,
                "deviceName": workspace.device_name,
                "workspaceId": workspace.workspace_id,
                "workspaceName": workspace.workspace_name,
                "status": if is_active { "active" } else { "offline" },
                "runtimeOnline": is_active,
                "lastSeenAt": workspace.last_seen_at,
            }));
        }
    }
    json!({
        "user":{"email":email,"isAdmin":is_admin},
        "devices":{"paired":paired,"online":online},
        "workspaces":{
            "total":total,
            "active":active,
            "sleeping":0,
            "offline":total-active,
            "recent":recent
        },
        "usage":{
            "available":true,
            "estimated":true,
            "scope":usage.scope,
            "last24h":usage.last24h,
            "last30d":usage.last30d,
            "allTime":usage.all_time
        }
    })
}

/// Check liveness per credential only once per snapshot, never read a device
/// outside the session's user-scoped PostgreSQL results. A revoked credential
/// must not remain online even if its Redis lease has not expired yet.
async fn load_presence(
    store: &AuthStore,
    devices: &[DeviceRecord],
) -> Result<HashMap<String, Option<RuntimePresence>>, &'static str> {
    let mut result = HashMap::new();
    for device in devices {
        if device.revoked_at.is_none() && !result.contains_key(&device.credential_id) {
            let state = store
                .runtime_presence(&device.credential_id)
                .await
                .map_err(|_| "presence_unavailable")?;
            result.insert(device.credential_id.clone(), state);
        }
    }
    Ok(result)
}

pub async fn overview(State(config): State<AuthConfig>, headers: HeaderMap) -> Response {
    let Some(store) = &config.store else {
        return error(StatusCode::UNAUTHORIZED, "unauthorized");
    };
    let (_, _, user) = match identify(store, &config, &headers).await {
        Ok(Some(identity)) => identity,
        Ok(None) => return error(StatusCode::UNAUTHORIZED, "unauthorized"),
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "identity_unavailable"),
    };
    let devices = match store.list_devices(&user.id).await {
        Ok(devices) => devices,
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "devices_unavailable"),
    };
    let workspaces = match store.list_workspace_catalog(&user.id).await {
        Ok(workspaces) => workspaces,
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "workspaces_unavailable"),
    };
    // The beta workspace catalog currently caps the returned set at 1000.
    // Refuse a potentially truncated summary instead of reporting false totals.
    if workspaces.len() >= 1000 {
        return error(StatusCode::SERVICE_UNAVAILABLE, "workspace_catalog_limit");
    }
    let presence = match load_presence(store, &devices).await {
        Ok(presence) => presence,
        Err(reason) => return error(StatusCode::SERVICE_UNAVAILABLE, reason),
    };
    let usage = match store.usage_report(&user.id).await {
        Ok(usage) => usage,
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "usage_unavailable"),
    };
    let is_admin = config.admin_emails.iter().any(|email| email == &user.email);
    let payload = build_overview(&user.email, is_admin, devices, workspaces, usage, &presence);
    let mut response = Json(payload).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn sample_usage() -> UsageReport {
        let mcp = crate::store::McpUsageWindow {
            calls: 3,
            input_tokens_estimated: 10,
            output_tokens_estimated: 4,
            total_tokens_estimated: 14,
        };
        let chat = crate::store::ReportedUsageWindow::default();
        UsageReport {
            estimated: true,
            scope: "MCP payload only; not full AI model/provider billing tokens",
            last1h: mcp.clone(),
            last24h: mcp.clone(),
            last30d: mcp.clone(),
            all_time: mcp.clone(),
            mcp: crate::store::EstimatedWindows {
                source: "payload_estimated",
                windows: crate::store::FourWindows {
                    last1h: mcp.clone(),
                    last24h: mcp.clone(),
                    last30d: mcp.clone(),
                    all_time: mcp,
                },
            },
            web_chat: crate::store::ReportedWindows {
                source: "provider_reported",
                windows: crate::store::FourWindows {
                    last1h: chat.clone(),
                    last24h: chat.clone(),
                    last30d: chat.clone(),
                    all_time: chat,
                },
            },
        }
    }

    fn sample_device(credential_id: &str, revoked: bool) -> DeviceRecord {
        DeviceRecord {
            credential_id: credential_id.into(),
            user_id: "account-a".into(),
            device_id: format!("device-{credential_id}"),
            device_name: "Mac Test".into(),
            public_key: None,
            secret_hash: "unused".into(),
            revoked_at: if revoked { Some(42) } else { None },
            created_at: 1,
            last_seen_at: 100,
        }
    }

    fn sample_workspace(credential_id: &str, workspace_id: &str) -> WorkspaceRow {
        WorkspaceRow {
            credential_id: credential_id.into(),
            device_id: format!("device-{credential_id}"),
            device_name: "Mac Test".into(),
            workspace_id: workspace_id.into(),
            workspace_name: format!("Project {workspace_id}"),
            managed: false,
            system_app: false,
            last_seen_at: 123,
        }
    }

    #[tokio::test]
    async fn anonymous_overview_does_not_expose_account_summary() {
        let response = crate::router()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/dashboard/overview")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"], "unauthorized");
        assert!(body.get("devices").is_none());
    }

    #[test]
    fn uses_only_real_presence_and_never_leaks_credentials() {
        let active = RuntimePresence {
            lease: "0123456789abcdef0123456789abcdef".into(),
            workspace_ids: vec!["project-one".into()],
        };
        let presence = HashMap::from([("cld_test1".into(), Some(active))]);
        let payload = build_overview(
            "beta@example.test",
            false,
            vec![
                sample_device("cld_test1", false),
                sample_device("cld_revoked", true),
            ],
            vec![
                sample_workspace("cld_test1", "project-one"),
                sample_workspace("cld_test1", "project-two"),
            ],
            sample_usage(),
            &presence,
        );
        assert_eq!(payload["user"]["email"], "beta@example.test");
        assert_eq!(payload["user"]["isAdmin"], false);
        assert_eq!(payload["devices"]["paired"], 1);
        assert_eq!(payload["devices"]["online"], 1);
        assert_eq!(payload["workspaces"]["total"], 2);
        assert_eq!(payload["workspaces"]["active"], 1);
        assert_eq!(payload["workspaces"]["offline"], 1);
        assert_eq!(payload["workspaces"]["recent"][0]["status"], "active");
        assert_eq!(payload["workspaces"]["recent"][1]["status"], "offline");
        assert_eq!(payload["usage"]["last24h"]["totalTokensEstimated"], 14);
        assert_eq!(payload["usage"]["available"], true);
        let serialized = payload.to_string();
        assert!(!serialized.contains("credentialId"));
        assert!(!serialized.contains("secretHash"));
        assert!(!serialized.contains("publicKey"));
        assert!(!serialized.contains("lease"));
        assert_eq!(payload.as_object().unwrap().len(), 4);
    }

    #[test]
    fn revoked_devices_and_unregistered_workspaces_never_go_online() {
        let presence = HashMap::new();
        let payload = build_overview(
            "beta@example.test",
            true,
            vec![sample_device("cld_revoked", true)],
            vec![],
            sample_usage(),
            &presence,
        );
        assert_eq!(payload["devices"]["online"], 0);
        assert_eq!(payload["devices"]["paired"], 0);
        assert_eq!(payload["workspaces"]["total"], 0);
        assert!(
            payload["workspaces"]["recent"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(payload["user"]["isAdmin"], true);
    }
}
