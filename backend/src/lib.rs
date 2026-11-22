//! CodeLocal Rust API foundation.
//! No privileged workspace, auth, or MCP operations are exposed until
//! their authorization and protocol contracts are implemented and tested.

mod auth;
mod auth_session;
mod devices;
// Verify request signatures before adding privileged native-runtime routes.
pub mod device_signing;
mod mailer;
mod pair;
mod password;
mod password_reset;
mod public_url;
mod signup;
mod store;

use axum::{Json, Router, routing::get};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceStatus {
    pub service: &'static str,
    pub state: &'static str,
    pub version: &'static str,
}

pub fn service_status() -> ServiceStatus {
    ServiceStatus {
        service: "codelocal-api",
        state: "bootstrap",
        version: env!("CARGO_PKG_VERSION"),
    }
}

async fn healthz() -> &'static str {
    "ok"
}

async fn status() -> Json<ServiceStatus> {
    Json(service_status())
}

pub fn router() -> Router {
    router_with_auth(auth::AuthConfig::from_env())
}

/// Stateful auth is enabled only with explicit isolated beta PostgreSQL/Redis settings.
pub async fn live_router() -> Result<Router, Box<dyn std::error::Error>> {
    let mut config = auth::AuthConfig::from_env();
    config.store = store::AuthStore::from_beta_env().await?;
    Ok(router_with_auth(config))
}

pub(crate) fn router_with_auth(config: auth::AuthConfig) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/api/v1/system/status", get(status))
        .route("/api/v1/auth/csrf", get(auth::csrf))
        .route("/api/v1/account", get(auth_session::account))
        .route("/login", axum::routing::post(auth_session::login))
        .route("/signup", axum::routing::post(signup::start))
        .route("/signup/verify", axum::routing::post(signup::verify))
        .route("/api/v1/auth/signup-verification", get(signup::context))
        .route(
            "/forgot-password",
            axum::routing::post(password_reset::forgot),
        )
        .route(
            "/reset-password",
            axum::routing::post(password_reset::reset),
        )
        .route(
            "/account/password",
            axum::routing::post(password_reset::change),
        )
        .route("/api/v1/auth/password-reset", get(password_reset::context))
        .route("/pair/start", axum::routing::post(pair::start))
        .route("/pair/approve", axum::routing::post(pair::approve))
        .route("/pair/claim", axum::routing::post(pair::claim))
        .route("/api/v1/pair/approve", get(pair::approve_context))
        .route(
            "/api/client/auth/check",
            axum::routing::post(pair::auth_check),
        )
        .route("/api/v1/devices", get(devices::list))
        .route(
            "/api/v1/devices/{device_id}/revoke",
            axum::routing::post(devices::revoke),
        )
        .route("/logout", axum::routing::post(auth_session::logout))
        .layer(axum::extract::DefaultBodyLimit::max(16 * 1024))
        .with_state(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[test]
    fn system_status_does_not_claim_feature_readiness() {
        let value = serde_json::to_value(service_status()).unwrap();
        assert_eq!(value["state"], "bootstrap");
        assert_eq!(value["service"], "codelocal-api");
    }

    #[tokio::test]
    async fn health_endpoint_returns_ok() {
        let response = router()
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body.as_ref(), b"ok");
    }

    #[tokio::test]
    async fn public_status_endpoint_has_no_privileged_data() {
        let response = router()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/system/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["state"], "bootstrap");
        assert_eq!(json.as_object().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn unimplemented_legacy_routes_return_not_found() {
        let response = router()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/workspaces")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
