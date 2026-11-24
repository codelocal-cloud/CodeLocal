//! Workspace preference settings. Persisted preferences are not execution grants.
use crate::{
    auth::AuthConfig,
    auth_session::{csrf_matches, error, identify},
    store::{PreferenceChange, RuntimePreference, now_millis},
};
use axum::{
    Form, Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Target {
    scope: String,
    device_id: String,
    workspace_id: String,
}

impl Target {
    fn normalized(mut self) -> Option<Self> {
        self.scope = self.scope.trim().to_owned();
        self.device_id = self.device_id.trim().to_owned();
        self.workspace_id = self.workspace_id.trim().to_owned();
        if self.scope.is_empty() {
            self.scope = "global".into();
        }
        match self.scope.as_str() {
            "global" => {
                self.device_id.clear();
                self.workspace_id.clear();
            }
            "device" => {
                self.workspace_id.clear();
            }
            "workspace" => {
                if !crate::runtime::valid_workspace_id(&self.workspace_id) {
                    return None;
                }
            }
            _ => return None,
        }
        if self.scope != "global"
            && (self.device_id.is_empty()
                || self.device_id.len() > 200
                || self.device_id.chars().any(char::is_control))
        {
            return None;
        }
        Some(self)
    }
}

#[derive(Deserialize)]
pub struct SettingsForm {
    #[serde(flatten)]
    target: Target,
    csrf: String,
    #[serde(default)]
    key: String,
    #[serde(default)]
    action: String,
    #[serde(default)]
    value: String,
    #[serde(default)]
    mode: String,
}

fn worktree_change(key: &str, action: &str, value: &str) -> Result<PreferenceChange, &'static str> {
    if key != "CODELOCAL_MAX_WORKTREES" {
        return Err("unsupported_runtime_key");
    }
    if action == "delete" {
        return Ok(PreferenceChange::WorktreeLimit(None));
    }
    if action != "set" && !action.is_empty() {
        return Err("invalid_action");
    }
    let limit = value
        .trim()
        .parse::<i32>()
        .ok()
        .filter(|n| (1..=20).contains(n))
        .ok_or("invalid_worktree_limit")?;
    Ok(PreferenceChange::WorktreeLimit(Some(limit)))
}

fn resource(target: Target, pref: RuntimePreference) -> Value {
    let mut values = serde_json::Map::new();
    if let Some(limit) = pref.max_worktrees {
        values.insert("CODELOCAL_MAX_WORKTREES".into(), json!(limit.to_string()));
    }
    let configured = pref.mode.is_some();
    let mode = pref.mode.as_deref().unwrap_or("safe");
    json!({
        "scope":target.scope, "deviceId":target.device_id,"workspaceId":target.workspace_id,
        "layer":{"scope":target.scope,"executionMode":mode,"executionModeConfigured":configured,
            "values":values,"updatedAt":pref.updated_at},
        "effective":{"executionMode":mode,"executionModeConfigured":configured,
            "values":values,"version":pref.updated_at,"updatedAt":pref.updated_at}
    })
}

pub async fn get_settings(
    State(config): State<AuthConfig>,
    headers: HeaderMap,
    Query(target): Query<Target>,
) -> Response {
    let Some(store) = &config.store else {
        return error(StatusCode::UNAUTHORIZED, "unauthorized");
    };
    let (_, _, user) = match identify(store, &config, &headers).await {
        Ok(Some(identity)) => identity,
        Ok(None) => return error(StatusCode::UNAUTHORIZED, "unauthorized"),
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "identity_unavailable"),
    };
    let Some(target) = target.normalized() else {
        return error(StatusCode::BAD_REQUEST, "invalid_runtime_scope");
    };
    let pref = match store
        .runtime_preference(&user.id, &target.device_id, &target.workspace_id)
        .await
    {
        Ok(Some(pref)) => pref,
        Ok(None) => return error(StatusCode::NOT_FOUND, "runtime_target_not_found"),
        Err(_) => {
            return error(
                StatusCode::SERVICE_UNAVAILABLE,
                "runtime_settings_unavailable",
            );
        }
    };
    let mut response = Json(resource(target, pref)).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}

async fn mutate(
    config: AuthConfig,
    headers: HeaderMap,
    form: SettingsForm,
    execution: bool,
) -> Response {
    let Some(store) = &config.store else {
        return error(StatusCode::UNAUTHORIZED, "unauthorized");
    };
    let (_, session, user) = match identify(store, &config, &headers).await {
        Ok(Some(identity)) => identity,
        Ok(None) => return error(StatusCode::UNAUTHORIZED, "unauthorized"),
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "identity_unavailable"),
    };
    if session.risk_until > now_millis() {
        return error(StatusCode::FORBIDDEN, "reauthentication_required");
    }
    if !csrf_matches(&session.csrf, &form.csrf) {
        return error(StatusCode::FORBIDDEN, "invalid_csrf");
    }
    let Some(target) = form.target.normalized() else {
        return error(StatusCode::BAD_REQUEST, "invalid_runtime_scope");
    };
    if target.scope != "workspace" {
        return error(StatusCode::BAD_REQUEST, "workspace_scope_required");
    }
    let change = if execution {
        if !matches!(form.mode.as_str(), "safe" | "live") {
            return error(StatusCode::BAD_REQUEST, "invalid_execution_mode");
        }
        PreferenceChange::Mode(form.mode.clone())
    } else {
        match worktree_change(&form.key, &form.action, &form.value) {
            Ok(change) => change,
            Err(reason) => return error(StatusCode::BAD_REQUEST, reason),
        }
    };
    match store
        .change_runtime_preference(&user.id, &target.device_id, &target.workspace_id, change)
        .await
    {
        Ok(true) => {
            let mut body = json!({"ok":true,"runtimeApplied":false});
            if execution {
                body["mode"] = json!(form.mode);
            }
            let mut response = Json(body).into_response();
            response.headers_mut().insert(
                header::CACHE_CONTROL,
                axum::http::HeaderValue::from_static("no-store"),
            );
            response
        }
        Ok(false) => error(StatusCode::NOT_FOUND, "runtime_target_not_found"),
        Err(_) => error(
            StatusCode::SERVICE_UNAVAILABLE,
            "runtime_settings_update_failed",
        ),
    }
}

pub async fn set_config(
    State(config): State<AuthConfig>,
    headers: HeaderMap,
    Form(form): Form<SettingsForm>,
) -> Response {
    mutate(config, headers, form, false).await
}
pub async fn set_execution(
    State(config): State<AuthConfig>,
    headers: HeaderMap,
    Form(form): Form<SettingsForm>,
) -> Response {
    mutate(config, headers, form, true).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    #[test]
    fn scopes_are_canonical_and_workspace_ids_checked() {
        let global = Target {
            scope: "global".into(),
            device_id: "ignored".into(),
            workspace_id: "ignored".into(),
        }
        .normalized()
        .unwrap();
        assert!(global.device_id.is_empty() && global.workspace_id.is_empty());
        assert!(
            Target {
                scope: "workspace".into(),
                device_id: "device".into(),
                workspace_id: "../outside".into()
            }
            .normalized()
            .is_none()
        );
        assert!(
            Target {
                scope: "device".into(),
                ..Default::default()
            }
            .normalized()
            .is_none()
        );
    }
    #[test]
    fn config_only_accepts_bounded_worktree_limits() {
        for input in ["0", "21", "1.5", "x"] {
            assert!(worktree_change("CODELOCAL_MAX_WORKTREES", "set", input).is_err());
        }
        assert!(matches!(
            worktree_change("CODELOCAL_MAX_WORKTREES", "set", "20"),
            Ok(PreferenceChange::WorktreeLimit(Some(20)))
        ));
        assert!(matches!(
            worktree_change("CODELOCAL_MAX_WORKTREES", "delete", ""),
            Ok(PreferenceChange::WorktreeLimit(None))
        ));
        assert!(worktree_change("API_KEY", "set", "test").is_err());
    }
    #[test]
    fn default_contract_has_safe_mode_and_no_fabricated_config() {
        let body = resource(
            Target::default().normalized().unwrap(),
            RuntimePreference::default(),
        );
        assert_eq!(body["scope"], "global");
        assert_eq!(body["effective"]["executionMode"], "safe");
        assert_eq!(body["effective"]["executionModeConfigured"], false);
        assert_eq!(body["effective"]["version"], 0);
        assert!(body["layer"]["values"].as_object().unwrap().is_empty());
    }
    #[tokio::test]
    async fn valid_form_is_decoded_before_anonymous_mutation_is_denied() {
        let response = crate::router().oneshot(
            Request::builder().method("POST")
                .uri("/api/v1/runtime/settings/config")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("scope=workspace&deviceId=test-device&workspaceId=repo&csrf=token&key=CODELOCAL_MAX_WORKTREES&value=5&action=set"))
                .unwrap()
        ).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn anonymous_read_is_denied() {
        let response = crate::router()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/runtime/settings")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
