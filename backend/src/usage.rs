//! Account-scoped real beta usage telemetry for unchanged Next.js UI.
//! The backend never manufactures reported provider token counts.
use crate::{
    auth::AuthConfig,
    auth_session::{error, identify},
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};

pub async fn report(State(config): State<AuthConfig>, headers: HeaderMap) -> Response {
    let Some(store) = &config.store else {
        return error(StatusCode::UNAUTHORIZED, "unauthorized");
    };
    let (_, _, user) = match identify(store, &config, &headers).await {
        Ok(Some(identity)) => identity,
        Ok(None) => return error(StatusCode::UNAUTHORIZED, "unauthorized"),
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "identity_unavailable"),
    };
    let usage = match store.usage_report(&user.id).await {
        Ok(usage) => usage,
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "usage_unavailable"),
    };
    let mut response = Json(usage).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{
        EstimatedWindows, FourWindows, McpUsageWindow, ReportedUsageWindow, ReportedWindows,
        UsageReport,
    };
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn usage_endpoint_requires_session() {
        let response = crate::router()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/usage")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn usage_dto_matches_existing_next_contract() {
        let m = McpUsageWindow {
            calls: 2,
            input_tokens_estimated: 9,
            output_tokens_estimated: 6,
            total_tokens_estimated: 15,
        };
        let c = ReportedUsageWindow {
            turns: 1,
            input_tokens: 21,
            output_tokens: 5,
            total_tokens: 26,
        };
        let usage = UsageReport {
            estimated: true,
            scope: "MCP payload only; not full AI model/provider billing tokens",
            last1h: m.clone(),
            last24h: m.clone(),
            last30d: m.clone(),
            all_time: m.clone(),
            web_chat: ReportedWindows {
                source: "provider_reported",
                windows: FourWindows {
                    last1h: c.clone(),
                    last24h: c.clone(),
                    last30d: c.clone(),
                    all_time: c,
                },
            },
            mcp: EstimatedWindows {
                source: "payload_estimated",
                windows: FourWindows {
                    last1h: m.clone(),
                    last24h: m.clone(),
                    last30d: m.clone(),
                    all_time: m,
                },
            },
        };
        let value = serde_json::to_value(usage).unwrap();
        assert_eq!(value["estimated"], true);
        assert_eq!(value["mcp"]["source"], "payload_estimated");
        assert_eq!(value["mcp"]["last24h"]["calls"], 2);
        assert_eq!(value["last1h"]["totalTokensEstimated"], 15);
        assert_eq!(value["webChat"]["source"], "provider_reported");
        assert_eq!(value["webChat"]["allTime"]["totalTokens"], 26);
        assert_eq!(value["webChat"]["last30d"]["turns"], 1);
        assert_eq!(
            value["scope"],
            "MCP payload only; not full AI model/provider billing tokens"
        );
        assert_eq!(value.as_object().unwrap().len(), 8);
    }

    #[tokio::test]
    async fn anonymous_usage_is_not_exposed_as_mock_zero_metrics() {
        let response = crate::router()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/usage")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let data: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(data["error"], "unauthorized");
        assert!(data.get("allTime").is_none());
    }
}
