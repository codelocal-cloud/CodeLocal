//! Isolated beta public-client OAuth authorization code + PKCE S256.
//! No remote MCP execution, refresh grant or OIDC is advertised by this slice.
use crate::{
    auth::AuthConfig,
    auth_session::{csrf_matches, error, identify},
    public_url,
    store::{AuthStore, OAuthClient, OAuthGrant, now_millis},
};
use axum::{
    Form, Json,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const SCOPE: &str = "mcp:tools";

fn reply(status: StatusCode, data: Value) -> Response {
    let mut response = (status, Json(data)).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
        .headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    response
}

fn valid_redirect(raw: &str) -> bool {
    if raw.len() > 2048
        || raw.trim() != raw
        || raw.chars().any(char::is_control)
        || raw.contains('\\')
    {
        return false;
    }
    let Ok(uri) = url::Url::parse(raw) else {
        return false;
    };
    if uri.username() != ""
        || uri.password().is_some()
        || uri.fragment().is_some()
        || uri.host_str().is_none()
    {
        return false;
    }
    let scheme = uri.scheme();
    if !(scheme == "https"
        || (scheme == "http"
            && matches!(uri.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))))
    {
        return false;
    }
    // Prevent ambiguous callback parameters when appending the code/state.
    !uri.query_pairs().any(|(name, _)| {
        matches!(
            name.as_ref(),
            "code" | "state" | "error" | "error_description"
        )
    })
}

fn valid_challenge(raw: &str) -> bool {
    raw.len() == 43 && URL_SAFE_NO_PAD.decode(raw).is_ok_and(|v| v.len() == 32)
}
fn valid_verifier(raw: &str) -> bool {
    (43..=128).contains(&raw.len())
        && raw
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~'))
}
fn valid_client_id(raw: &str) -> bool {
    raw.strip_prefix("codelocal_").is_some_and(valid_challenge)
}
fn pkce(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}
fn scope_ok(value: &str) -> bool {
    value.is_empty() || value == SCOPE
}

#[derive(Deserialize)]
pub struct Registration {
    client_name: Option<String>,
    redirect_uris: Vec<String>,
    token_endpoint_auth_method: Option<String>,
    grant_types: Option<Vec<String>>,
    response_types: Option<Vec<String>>,
}
fn registration_valid(r: &Registration) -> bool {
    (1..=8).contains(&r.redirect_uris.len())
        && r.redirect_uris.iter().all(|v| valid_redirect(v))
        && r.redirect_uris
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            == r.redirect_uris.len()
        && r.client_name
            .as_ref()
            .is_none_or(|v| v.len() <= 160 && !v.chars().any(char::is_control))
        && r.token_endpoint_auth_method
            .as_deref()
            .is_none_or(|v| v == "none")
        && r.grant_types
            .as_ref()
            .is_none_or(|v| v == &["authorization_code"])
        && r.response_types.as_ref().is_none_or(|v| v == &["code"])
}
async fn throttle(
    store: &AuthStore,
    scope: &'static str,
    subject: &str,
    limit: i64,
) -> Result<(), Box<Response>> {
    match store.rate_limit(scope, subject, limit, 60).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(Box::new(error(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
        ))),
        Err(_) => Err(Box::new(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "temporarily_unavailable",
        ))),
    }
}

pub async fn metadata(State(config): State<AuthConfig>) -> Response {
    let Some(origin) = public_url::beta_origin().filter(|_| config.store.is_some()) else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "oauth_unavailable");
    };
    reply(
        StatusCode::OK,
        json!({
            "issuer":origin,"authorization_endpoint":format!("{origin}/authorize"),
            "token_endpoint":format!("{origin}/token"),"registration_endpoint":format!("{origin}/register"),
            "revocation_endpoint":format!("{origin}/revoke"),
            "response_types_supported":["code"],"grant_types_supported":["authorization_code"],
            "code_challenge_methods_supported":["S256"],"token_endpoint_auth_methods_supported":["none"],
            "revocation_endpoint_auth_methods_supported":["none"],"scopes_supported":[SCOPE]
        }),
    )
}

pub async fn register(
    State(config): State<AuthConfig>,
    Json(input): Json<Registration>,
) -> Response {
    let Some(store) = &config.store else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "oauth_unavailable");
    };
    if public_url::beta_origin().is_none() {
        return error(StatusCode::SERVICE_UNAVAILABLE, "oauth_unavailable");
    }
    if let Err(response) = throttle(store, "oauth-registration", "beta-global", 30).await {
        return *response;
    }
    if !registration_valid(&input) {
        return error(StatusCode::BAD_REQUEST, "invalid_client_metadata");
    }
    let name = input
        .client_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("MCP client");
    match store
        .register_oauth_client(name, &input.redirect_uris)
        .await
    {
        Ok(client) => reply(
            StatusCode::CREATED,
            json!({
                "client_id":client.client_id,"client_name":client.client_name,
                "redirect_uris":client.redirect_uris,"grant_types":["authorization_code"],
                "response_types":["code"],"token_endpoint_auth_method":"none"
            }),
        ),
        Err(_) => error(StatusCode::SERVICE_UNAVAILABLE, "temporarily_unavailable"),
    }
}

#[derive(Deserialize)]
pub struct Authorize {
    client_id: String,
    redirect_uri: String,
    code_challenge: String,
    resource: String,
    #[serde(default)]
    response_type: String,
    #[serde(default)]
    code_challenge_method: String,
    #[serde(default)]
    scope: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    csrf: String,
}

impl Authorize {
    fn valid(&self, resource: &str, query: bool) -> bool {
        valid_client_id(&self.client_id)
            && valid_redirect(&self.redirect_uri)
            && valid_challenge(&self.code_challenge)
            && self.resource == resource
            && scope_ok(&self.scope)
            && self.state.len() <= 512
            && !self.state.chars().any(char::is_control)
            && if query {
                self.response_type == "code" && self.code_challenge_method == "S256"
            } else {
                matches!(self.response_type.as_str(), "" | "code")
                    && matches!(self.code_challenge_method.as_str(), "" | "S256")
            }
    }
    async fn client(
        &self,
        store: &AuthStore,
        resource: &str,
        query: bool,
    ) -> Result<Option<OAuthClient>, crate::store::StoreError> {
        if !self.valid(resource, query) {
            return Ok(None);
        }
        Ok(store
            .oauth_client(&self.client_id)
            .await?
            .filter(|c| c.redirect_uris.contains(&self.redirect_uri)))
    }
}
pub async fn context(
    State(config): State<AuthConfig>,
    headers: HeaderMap,
    Query(input): Query<Authorize>,
) -> Response {
    let Some(store) = &config.store else {
        return error(StatusCode::UNAUTHORIZED, "unauthorized");
    };
    let (_, session, user) = match identify(store, &config, &headers).await {
        Ok(Some(v)) => v,
        Ok(None) => return error(StatusCode::UNAUTHORIZED, "unauthorized"),
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "temporarily_unavailable"),
    };
    if session.risk_until > now_millis() {
        return error(StatusCode::UNAUTHORIZED, "reauthentication_required");
    }
    let Some(origin) = public_url::beta_origin() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "oauth_unavailable");
    };
    let client = match input.client(store, &format!("{origin}/mcp"), true).await {
        Ok(Some(c)) => c,
        Ok(None) => return error(StatusCode::BAD_REQUEST, "invalid_authorization_request"),
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "temporarily_unavailable"),
    };
    reply(
        StatusCode::OK,
        json!({"clientId":client.client_id,"clientName":client.client_name,
        "redirectUri":input.redirect_uri,"responseType":"code","codeChallenge":input.code_challenge,
        "resource":input.resource,"scope":SCOPE,"state":input.state,"email":user.email,"csrf":session.csrf}),
    )
}

pub async fn authorize(
    State(config): State<AuthConfig>,
    headers: HeaderMap,
    Form(input): Form<Authorize>,
) -> Response {
    let Some(store) = &config.store else {
        return error(StatusCode::UNAUTHORIZED, "unauthorized");
    };
    let (_, session, user) = match identify(store, &config, &headers).await {
        Ok(Some(v)) => v,
        Ok(None) => return error(StatusCode::UNAUTHORIZED, "unauthorized"),
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "temporarily_unavailable"),
    };
    if session.risk_until > now_millis() {
        return error(StatusCode::FORBIDDEN, "reauthentication_required");
    }
    if !csrf_matches(&session.csrf, &input.csrf) {
        return error(StatusCode::FORBIDDEN, "invalid_csrf");
    }
    if let Err(response) = throttle(store, "oauth-consent", &user.id, 20).await {
        return *response;
    }
    let Some(origin) = public_url::beta_origin() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "oauth_unavailable");
    };
    match input.client(store, &format!("{origin}/mcp"), false).await {
        Ok(Some(_)) => (),
        Ok(None) => return error(StatusCode::BAD_REQUEST, "invalid_authorization_request"),
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "temporarily_unavailable"),
    }
    let grant = OAuthGrant {
        client_id: &input.client_id,
        redirect_uri: &input.redirect_uri,
        challenge: &input.code_challenge,
        resource: &input.resource,
    };
    let code = match store.issue_oauth_code(&user, &grant).await {
        Ok(Some(code)) => code,
        Ok(None) => return error(StatusCode::FORBIDDEN, "reauthentication_required"),
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "temporarily_unavailable"),
    };
    let mut target = url::Url::parse(&input.redirect_uri).expect("validated callback");
    target
        .query_pairs_mut()
        .append_pair("code", &code)
        .append_pair("state", &input.state);
    let Ok(location) = HeaderValue::from_str(target.as_str()) else {
        return error(StatusCode::BAD_REQUEST, "invalid_redirect_uri");
    };
    let mut response = reply(StatusCode::FOUND, json!({}));
    response.headers_mut().insert(header::LOCATION, location);
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response
}

#[derive(Deserialize)]
pub struct TokenForm {
    grant_type: String,
    client_id: String,
    #[serde(default)]
    code: String,
    #[serde(default)]
    redirect_uri: String,
    #[serde(default)]
    code_verifier: String,
    #[serde(default)]
    resource: String,
}

pub async fn token(State(config): State<AuthConfig>, Form(input): Form<TokenForm>) -> Response {
    let Some(store) = &config.store else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "oauth_unavailable");
    };
    if input.grant_type != "authorization_code" {
        return error(StatusCode::BAD_REQUEST, "unsupported_grant_type");
    }
    if !valid_client_id(&input.client_id)
        || !valid_challenge(&input.code)
        || !valid_verifier(&input.code_verifier)
        || !valid_redirect(&input.redirect_uri)
    {
        return error(StatusCode::BAD_REQUEST, "invalid_grant");
    }
    if let Err(response) = throttle(store, "oauth-exchange-global", "beta-global", 120).await {
        return *response;
    }
    if let Err(response) = throttle(store, "oauth-exchange-client", &input.client_id, 60).await {
        return *response;
    }
    let Some(origin) = public_url::beta_origin() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "oauth_unavailable");
    };
    let resource = format!("{origin}/mcp");
    if !input.resource.is_empty() && input.resource != resource {
        return error(StatusCode::BAD_REQUEST, "invalid_grant");
    }
    let challenge = pkce(&input.code_verifier);
    let grant = OAuthGrant {
        client_id: &input.client_id,
        redirect_uri: &input.redirect_uri,
        challenge: &challenge,
        resource: &resource,
    };
    match store.exchange_oauth_code(&input.code, &grant).await {
        Ok(Some(token)) => reply(
            StatusCode::OK,
            json!({"access_token":token,"token_type":"Bearer","expires_in":3600,"scope":SCOPE}),
        ),
        Ok(None) => error(StatusCode::BAD_REQUEST, "invalid_grant"),
        Err(_) => error(StatusCode::SERVICE_UNAVAILABLE, "temporarily_unavailable"),
    }
}

#[derive(Deserialize)]
pub struct RevokeForm {
    client_id: String,
    token: String,
}
pub async fn revoke(State(config): State<AuthConfig>, Form(input): Form<RevokeForm>) -> Response {
    let Some(store) = &config.store else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "oauth_unavailable");
    };
    if !valid_client_id(&input.client_id) || !valid_challenge(&input.token) {
        return error(StatusCode::BAD_REQUEST, "invalid_request");
    }
    if let Err(response) = throttle(store, "oauth-revoke", "beta-global", 120).await {
        return *response;
    }
    match store
        .revoke_oauth_access(&input.client_id, &input.token)
        .await
    {
        Ok(()) => reply(StatusCode::OK, json!({})),
        Err(_) => error(StatusCode::SERVICE_UNAVAILABLE, "temporarily_unavailable"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pkce_rfc7636_and_redirect_policy() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert!(valid_verifier(verifier));
        assert_eq!(
            pkce(verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        assert!(valid_challenge(&pkce(verifier)));
        assert!(!valid_verifier("short"));
        assert!(!valid_verifier(&"a".repeat(129)));
        assert!(valid_redirect("https://example.test/callback?app=beta"));
        assert!(valid_redirect("http://127.0.0.1:5050/callback"));
        assert!(!valid_redirect("http://example.test/callback"));
        assert!(!valid_redirect("https://name@example.test/callback"));
        assert!(!valid_redirect("https://example.test/callback#fragment"));
        assert!(!valid_redirect(
            "https://example.test/callback?code=existing"
        ));
        assert!(!valid_redirect("javascript:alert(1)"));
        assert!(!scope_ok("mcp:tools offline_access"));
    }

    #[tokio::test]
    async fn no_database_no_oauth_context() {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;
        let response=crate::router().oneshot(Request::builder()
            .uri("/api/v1/oauth/authorize-context?client_id=bad&redirect_uri=bad&code_challenge=bad&resource=bad")
            .body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
