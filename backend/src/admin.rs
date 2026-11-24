//! Privileged, read-only beta administrator dashboard.
use crate::{
    auth::AuthConfig,
    auth_session::{error, identify},
    store::{AdminUserRow, now_millis},
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

fn build_admin(users: Vec<AdminUserRow>) -> Value {
    let runtime_online = users.iter().filter(|u| u.runtime_active).count();
    let using_mcp_now = users.iter().filter(|u| u.mcp_active).count();
    let active_users = users
        .iter()
        .filter(|u| u.runtime_active || u.mcp_active)
        .count();
    json!({
        "totalUsers":users.len(),
        "activeUsers":active_users,
        "runtimeOnline":runtime_online,
        "usingMcpNow":using_mcp_now,
        "users":users,
    })
}

pub async fn resource(State(config): State<AuthConfig>, headers: HeaderMap) -> Response {
    let Some(store) = &config.store else {
        return error(StatusCode::UNAUTHORIZED, "unauthorized");
    };
    let (_, session, user) = match identify(store, &config, &headers).await {
        Ok(Some(identity)) => identity,
        Ok(None) => return error(StatusCode::UNAUTHORIZED, "unauthorized"),
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "identity_unavailable"),
    };
    // Never authorize from request parameters, a client-side claim or an invite code.
    if !config.admin_emails.iter().any(|email| email == &user.email) {
        return error(StatusCode::FORBIDDEN, "forbidden");
    }
    if session.risk_until > now_millis() {
        return error(StatusCode::FORBIDDEN, "reauthentication_required");
    }
    let users = match store.admin_users().await {
        Ok(users) => users,
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "admin_unavailable"),
    };
    if store
        .audit(
            Some(&user.id),
            "admin.users_viewed",
            json!({"count":users.len()}),
        )
        .await
        .is_err()
    {
        return error(StatusCode::SERVICE_UNAVAILABLE, "admin_audit_unavailable");
    }
    let mut response = Json(build_admin(users)).into_response();
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

    #[tokio::test]
    async fn anonymous_admin_has_no_user_data() {
        let response = crate::router()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/admin")
                    .header("x-admin", "true")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let payload: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(payload.get("users").is_none());
    }

    #[test]
    fn aggregate_counts_distinct_users_and_matches_next_contract() {
        let row = |id: &str, runtime_active: bool, mcp_active: bool| AdminUserRow {
            id: id.into(),
            email: format!("{id}@example.test"),
            referral_code: "ABC123".into(),
            referred_by_code: String::new(),
            created_at: 42,
            invite_count: 2,
            last_device_seen_at: 123,
            last_mcp_used_at: 456,
            runtime_active,
            mcp_active,
        };
        let payload = build_admin(vec![
            row("one", true, true),
            row("two", true, false),
            row("three", false, false),
        ]);
        assert_eq!(payload["totalUsers"], 3);
        assert_eq!(payload["activeUsers"], 2);
        assert_eq!(payload["runtimeOnline"], 2);
        assert_eq!(payload["usingMcpNow"], 1);
        assert_eq!(payload["users"][0]["lastMcpUsedAt"], 456);
        assert_eq!(payload["users"][0]["inviteCount"], 2);
        assert_eq!(payload["users"][0]["runtimeActive"], true);
        assert_eq!(payload["users"][0].as_object().unwrap().len(), 10);
        let empty = build_admin(Vec::new());
        assert_eq!(empty["totalUsers"], 0);
        assert!(empty["users"].as_array().unwrap().is_empty());
    }
}
