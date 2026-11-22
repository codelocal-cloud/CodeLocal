//! Beta-only PostgreSQL user store and Redis session store. No implicit migrations.
//!
//! Go compatibility: codelocal_users columns, 30-day JSON SessionState, securityVersion.
//! Operational isolation: environment variables are BETA_ONLY and session keys are namespaced.
//! During initial migration only loopback development stores are accepted (NoTls).
use deadpool_postgres::{Manager, Pool};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    env,
    error::Error,
    fmt,
    str::FromStr,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio_postgres::NoTls;

pub const SESSION_TTL_SECONDS: u64 = 30 * 24 * 60 * 60;
const SESSION_PREFIX: &str = "codelocal:beta:session:";
const LOGIN_PREFIX: &str = "codelocal:beta:login-account:";

#[derive(Debug)]
pub struct StoreError(String);

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for StoreError {}

fn unavailable(reason: &str) -> StoreError {
    StoreError(reason.to_owned())
}

#[derive(Debug, Clone)]
pub struct User {
    pub id: String,
    pub email: String,
    pub password_hash: String,
    pub password_salt: String,
    pub password_changed_at: i64,
    pub security_version: i64,
    pub referral_code: String,
    pub referred_by_code: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecuritySignal {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub device_hash: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub agent_hash: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub network_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionState {
    pub user_id: String,
    pub csrf: String,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub security_version: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security: Option<SecuritySignal>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub risk_until: i64,
}

fn is_zero(value: &i64) -> bool {
    *value == 0
}

impl SessionState {
    pub fn invalid_for_user(&self, user: &User) -> bool {
        if self.security_version > 0 && user.security_version > 0 {
            self.security_version != user.security_version
        } else {
            user.password_changed_at > 0 && self.created_at < user.password_changed_at
        }
    }
}

pub struct AuthStore {
    pool: Pool,
    redis: redis::Client,
}

impl AuthStore {
    /// Explicit opt-in with beta-only vars. Refuse partial configuration or external
    /// PostgreSQL without TLS: an unfinished beta must never talk to live Cloud.
    pub async fn from_beta_env() -> Result<Option<Arc<Self>>, StoreError> {
        let db_url = env::var("CODELOCAL_BETA_DATABASE_URL").unwrap_or_default();
        let redis_url = env::var("CODELOCAL_BETA_REDIS_URL").unwrap_or_default();
        if db_url.is_empty() && redis_url.is_empty() {
            return Ok(None);
        }
        if db_url.is_empty() || redis_url.is_empty() {
            return Err(unavailable(
                "beta auth requires both CODELOCAL_BETA_DATABASE_URL and CODELOCAL_BETA_REDIS_URL",
            ));
        }
        let pg_config = tokio_postgres::Config::from_str(&db_url)
            .map_err(|_| unavailable("invalid CODELOCAL_BETA_DATABASE_URL"))?;
        let db_name = pg_config
            .get_dbname()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !db_name.contains("beta") {
            return Err(unavailable(
                "refusing database not named for beta; create an isolated *_beta database",
            ));
        }
        let hosts = pg_config.get_hosts();
        if hosts.is_empty() || !hosts.iter().all(|host| matches!(
            host,
            tokio_postgres::config::Host::Tcp(name) if matches!(name.as_str(), "localhost" | "127.0.0.1" | "::1")
        )) {
            return Err(unavailable("remote PostgreSQL requires verified TLS support; beta auth only permits local DB for now"));
        }
        if !is_loopback_redis(&redis_url) {
            return Err(unavailable("beta auth only permits loopback Redis for now"));
        }
        if env::var("CODELOCAL_BETA_SECURITY_BINDING_SECRET")
            .ok()
            .is_none_or(|value| value.trim().len() < 32)
        {
            return Err(unavailable(
                "CODELOCAL_BETA_SECURITY_BINDING_SECRET must be configured with >=32 characters",
            ));
        }
        let manager = Manager::new(pg_config, NoTls);
        let pool = Pool::builder(manager)
            .max_size(8)
            .build()
            .map_err(|_| unavailable("unable to construct beta PostgreSQL pool"))?;
        let client = redis::Client::open(redis_url)
            .map_err(|_| unavailable("invalid beta Redis configuration"))?;
        let store = Self {
            pool,
            redis: client,
        };
        store.verify_connections().await?;
        Ok(Some(Arc::new(store)))
    }

    async fn verify_connections(&self) -> Result<(), StoreError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|_| unavailable("beta PostgreSQL unavailable"))?;
        // We intentionally do not run Go schema migrations automatically.
        let columns: i64 = client.query_one(
            "SELECT COUNT(*) FROM information_schema.columns WHERE table_schema=current_schema() AND table_name='codelocal_users' AND column_name IN ('id','email','password_hash','password_salt','password_changed_at','security_version','referral_code','referred_by_code','created_at')",
            &[],
        ).await.map_err(|_| unavailable("beta account schema check failed"))?.get(0);
        if columns != 9 {
            return Err(unavailable(
                "beta codelocal_users schema is missing columns; run reviewed SQL bootstrap first",
            ));
        }
        let audit_ready: bool = client
            .query_one(
                "SELECT to_regclass('codelocal_audit_logs') IS NOT NULL",
                &[],
            )
            .await
            .map_err(|_| unavailable("beta audit schema check failed"))?
            .get(0);
        if !audit_ready {
            return Err(unavailable(
                "beta codelocal_audit_logs table is missing; apply reviewed SQL bootstrap",
            ));
        }
        let pair_schema_ready: bool = client.query_one(
            "SELECT to_regclass('codelocal_devices') IS NOT NULL AND to_regclass('codelocal_pairings') IS NOT NULL", &[],
        ).await.map_err(|_| unavailable("beta pairing schema check failed"))?.get(0);
        if !pair_schema_ready {
            return Err(unavailable(
                "beta device tables missing; apply reviewed 002_beta_devices.sql",
            ));
        }
        let mut conn = self
            .redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|_| unavailable("beta Redis unavailable"))?;
        let _: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(|_| unavailable("beta Redis PING failed"))?;
        Ok(())
    }

    pub async fn by_email(&self, email: &str) -> Result<Option<User>, StoreError> {
        self.find_user("email", email).await
    }

    pub async fn by_id(&self, user_id: &str) -> Result<Option<User>, StoreError> {
        self.find_user("id", user_id).await
    }

    async fn find_user(&self, by: &str, value: &str) -> Result<Option<User>, StoreError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|_| unavailable("beta PostgreSQL unavailable"))?;
        let query = match by {
            "email" => {
                "SELECT id,email,password_hash,password_salt,password_changed_at,security_version,referral_code,referred_by_code,created_at FROM codelocal_users WHERE email=$1"
            }
            "id" => {
                "SELECT id,email,password_hash,password_salt,password_changed_at,security_version,referral_code,referred_by_code,created_at FROM codelocal_users WHERE id=$1"
            }
            _ => return Err(unavailable("unsupported user lookup")),
        };
        let row = client
            .query_opt(query, &[&value])
            .await
            .map_err(|_| unavailable("beta user query failed"))?;
        Ok(row.map(|row| User {
            id: row.get(0),
            email: row.get(1),
            password_hash: row.get(2),
            password_salt: row.get(3),
            password_changed_at: row.get(4),
            security_version: row.get(5),
            referral_code: row.get(6),
            referred_by_code: row.get(7),
            created_at: row.get(8),
        }))
    }

    pub async fn throttle_login(&self, normalized_email: &str) -> Result<bool, StoreError> {
        // Mirrors Go's 12/10m account ceiling; atomic Lua avoids concurrent bypass.
        const LUA: &str = "local n=redis.call('INCR',KEYS[1]); if n==1 then redis.call('EXPIRE',KEYS[1],600) end; return n";
        let digest = Sha256::digest(normalized_email.as_bytes());
        let key = format!("{LOGIN_PREFIX}{}", hex::encode(digest));
        let mut conn = self
            .redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|_| unavailable("beta Redis unavailable"))?;
        let count: i64 = redis::cmd("EVAL")
            .arg(LUA)
            .arg(1)
            .arg(&key)
            .query_async(&mut conn)
            .await
            .map_err(|_| unavailable("beta login rate limiter unavailable"))?;
        Ok(count <= 12)
    }

    pub async fn read_session(&self, session_id: &str) -> Result<Option<SessionState>, StoreError> {
        if !valid_session_id(session_id) {
            return Ok(None);
        }
        let mut conn = self
            .redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|_| unavailable("beta Redis unavailable"))?;
        let raw: Option<String> = conn
            .get(format!("{SESSION_PREFIX}{session_id}"))
            .await
            .map_err(|_| unavailable("beta session lookup failed"))?;
        Ok(raw
            .and_then(|raw| serde_json::from_str::<SessionState>(&raw).ok())
            .filter(|session| !session.user_id.is_empty() && !session.csrf.is_empty()))
    }

    pub async fn create_session(&self, value: &SessionState) -> Result<String, StoreError> {
        let payload =
            serde_json::to_string(value).map_err(|_| unavailable("unable to encode session"))?;
        let mut conn = self
            .redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|_| unavailable("beta Redis unavailable"))?;
        for _ in 0..3 {
            let mut bytes = [0_u8; 40];
            getrandom::fill(&mut bytes).map_err(|_| unavailable("secure random unavailable"))?;
            let id = hex::encode(bytes);
            let key = format!("{SESSION_PREFIX}{id}");
            let result: Option<String> = redis::cmd("SET")
                .arg(&key)
                .arg(&payload)
                .arg("EX")
                .arg(SESSION_TTL_SECONDS)
                .arg("NX")
                .query_async(&mut conn)
                .await
                .map_err(|_| unavailable("beta session storage failed"))?;
            if result.as_deref() == Some("OK") {
                return Ok(id);
            }
        }
        Err(unavailable("unable to allocate unique beta session"))
    }

    pub async fn update_session(&self, id: &str, state: &SessionState) -> Result<(), StoreError> {
        if !valid_session_id(id) {
            return Err(unavailable("invalid beta session identifier"));
        }
        let payload =
            serde_json::to_string(state).map_err(|_| unavailable("unable to encode session"))?;
        let mut conn = self
            .redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|_| unavailable("beta Redis unavailable"))?;
        // Replace without extending TTL, and do not recreate expired sessions.
        let script = "if redis.call('EXISTS',KEYS[1])==0 then return 0 end; redis.call('SET',KEYS[1],ARGV[1],'KEEPTTL'); return 1";
        let updated: i64 = redis::cmd("EVAL")
            .arg(script)
            .arg(1)
            .arg(format!("{SESSION_PREFIX}{id}"))
            .arg(payload)
            .query_async(&mut conn)
            .await
            .map_err(|_| unavailable("beta session update failed"))?;
        if updated != 1 {
            return Err(unavailable("beta session expired"));
        }
        Ok(())
    }

    pub async fn delete_session(&self, id: &str) -> Result<(), StoreError> {
        if !valid_session_id(id) {
            return Ok(());
        }
        let mut conn = self
            .redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|_| unavailable("beta Redis unavailable"))?;
        let _: i64 = conn
            .del(format!("{SESSION_PREFIX}{id}"))
            .await
            .map_err(|_| unavailable("beta session revocation failed"))?;
        Ok(())
    }

    pub async fn audit(
        &self,
        user_id: Option<&str>,
        event: &str,
        detail: serde_json::Value,
    ) -> Result<(), StoreError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|_| unavailable("beta PostgreSQL unavailable"))?;
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes).map_err(|_| unavailable("secure random unavailable"))?;
        let id = hex::encode(bytes);
        let detail_str = detail.to_string();
        // Avoid depending on tokio-postgres serde_json sql features.
        client.execute(
            "INSERT INTO codelocal_audit_logs(id,user_id,event,detail,created_at) VALUES($1,$2,$3,$4::jsonb,$5)",
            &[&id, &user_id, &event, &detail_str, &now_millis()],
        ).await.map_err(|_| unavailable("beta login audit unavailable"))?;
        Ok(())
    }
}

fn is_loopback_redis(value: &str) -> bool {
    let Ok(parsed) = url::Url::parse(value) else {
        return false;
    };
    parsed.scheme() == "redis"
        && matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
}

pub fn valid_session_id(id: &str) -> bool {
    id.len() == 80 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

// Auth flow operations share the same store, credentials, and isolation checks.
include!("store_auth_flows.rs");
include!("store_devices.rs");

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_user() -> User {
        User {
            id: "user1".into(),
            email: "someone@example.com".into(),
            password_hash: "hash".into(),
            password_salt: "salt".into(),
            password_changed_at: 200,
            security_version: 3,
            referral_code: "ABC123".into(),
            referred_by_code: None,
            created_at: 100,
        }
    }

    #[test]
    fn session_json_matches_go_field_names() {
        let state = SessionState {
            user_id: "user1".into(),
            csrf: "csrf123".into(),
            created_at: 250,
            security_version: 3,
            security: Some(SecuritySignal {
                device_hash: "abc".into(),
                agent_hash: "def".into(),
                network_hash: String::new(),
            }),
            risk_until: 0,
        };
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json["userId"], "user1");
        assert_eq!(json["createdAt"], 250);
        assert_eq!(json["securityVersion"], 3);
        assert_eq!(json["security"]["deviceHash"], "abc");
        assert!(json.get("riskUntil").is_none());
        let recovered: SessionState = serde_json::from_value(json).unwrap();
        assert_eq!(recovered.security.unwrap().agent_hash, "def");
    }

    #[test]
    fn go_security_version_revocation_contract() {
        let user = sample_user();
        let mut s = SessionState {
            user_id: user.id.clone(),
            csrf: "csrf".into(),
            created_at: 250,
            security_version: 3,
            security: None,
            risk_until: 0,
        };
        assert!(!s.invalid_for_user(&user));
        s.security_version = 2;
        assert!(s.invalid_for_user(&user));
        s.security_version = 0;
        s.created_at = 199;
        assert!(s.invalid_for_user(&user));
        s.created_at = 200;
        assert!(!s.invalid_for_user(&user));
    }

    #[test]
    fn refuses_malformed_session_ids() {
        assert!(!valid_session_id("bad"));
        assert!(!valid_session_id(&"g".repeat(80)));
        assert!(valid_session_id(&"a".repeat(80)));
    }

    /// Integration test against disposable local *_beta PostgreSQL and Redis stores.
    /// Explicitly ignored by default because it creates and deletes real test rows.
    #[tokio::test]
    #[ignore = "requires CODELOCAL_BETA_DATABASE_URL, CODELOCAL_BETA_REDIS_URL and beta security secret"]
    async fn isolated_beta_login_account_logout_real_stores() {
        use crate::{auth::AuthConfig, router_with_auth};
        use axum::{
            body::Body,
            http::{Request, StatusCode, header},
        };
        use http_body_util::BodyExt;
        use tower::ServiceExt;

        let store = AuthStore::from_beta_env()
            .await
            .unwrap()
            .expect("isolated beta store required");
        let mut random = [0u8; 8];
        getrandom::fill(&mut random).unwrap();
        let suffix = hex::encode(random);
        let user_id = format!("rust-beta-test-{suffix}");
        let email = format!("rust-beta-{suffix}@example.test");
        let referral = format!("R{}", suffix[..5].to_uppercase());
        let pwd = "test-password-not-production";
        let salt = "fixed-integration-salt";
        let hash = crate::password::hash_password(pwd, salt).unwrap();
        let client = store.pool.get().await.unwrap();
        client.execute(
            "INSERT INTO codelocal_users(id,email,password_hash,password_salt,created_at,referral_code) VALUES($1,$2,$3,$4,$5,$6)",
            &[&user_id, &email, &hash, &salt, &now_millis(), &referral],
        ).await.unwrap();
        drop(client);

        let mut config = AuthConfig::from_env();
        config.store = Some(store.clone());
        let app = router_with_auth(config);
        let csrf_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/auth/csrf")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(csrf_response.status(), StatusCode::OK);
        let csrf_cookie = csrf_response.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let body = csrf_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let token = serde_json::from_slice::<serde_json::Value>(&body).unwrap()["csrf"]
            .as_str()
            .unwrap()
            .to_owned();

        let form = form_urlencoded::Serializer::new(String::new())
            .append_pair("email", &email)
            .append_pair("password", pwd)
            .append_pair("csrf", &token)
            .append_pair("next", "/dashboard")
            .finish();
        let login = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/login")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .header(header::COOKIE, csrf_cookie)
                    .header(header::USER_AGENT, "CodeLocal Integration Test/1.0")
                    .body(Body::from(form))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(login.status(), StatusCode::SEE_OTHER);
        let cookies: Vec<_> = login
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_owned())
            .collect();
        assert_eq!(cookies.len(), 2);
        let combined = cookies.join("; ");

        let account_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/account")
                    .header(header::COOKIE, &combined)
                    .header(header::USER_AGENT, "CodeLocal Integration Test/1.0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(account_response.status(), StatusCode::OK);
        let account_body = account_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let account: serde_json::Value = serde_json::from_slice(&account_body).unwrap();
        assert_eq!(account["email"], email);
        assert_eq!(account["csrf"], token);
        assert_eq!(account["userId"], user_id);

        let logout_form = form_urlencoded::Serializer::new(String::new())
            .append_pair("csrf", &token)
            .append_pair("next", "/dashboard")
            .finish();
        let logout = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/logout")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .header(header::COOKIE, &combined)
                    .header(header::USER_AGENT, "CodeLocal Integration Test/1.0")
                    .body(Body::from(logout_form))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(logout.status(), StatusCode::SEE_OTHER);
        assert_eq!(logout.headers()[header::LOCATION], "/login");
        assert!(
            logout.headers()[header::SET_COOKIE]
                .to_str()
                .unwrap()
                .contains("Max-Age=0")
        );

        let after = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/account")
                    .header(header::COOKIE, &combined)
                    .header(header::USER_AGENT, "CodeLocal Integration Test/1.0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(after.status(), StatusCode::UNAUTHORIZED);

        let client = store.pool.get().await.unwrap();
        client
            .execute(
                "DELETE FROM codelocal_audit_logs WHERE user_id=$1",
                &[&user_id],
            )
            .await
            .unwrap();
        client
            .execute("DELETE FROM codelocal_users WHERE id=$1", &[&user_id])
            .await
            .unwrap();
    }

    #[test]
    fn beta_redis_must_not_accept_lookalike_remote_hostname() {
        assert!(is_loopback_redis("redis://localhost:6379/15"));
        assert!(is_loopback_redis("redis://127.0.0.1:6379/15"));
        assert!(!is_loopback_redis("redis://localhost.evil.example:6379/15"));
        assert!(!is_loopback_redis("rediss://remote.example:6379/15"));
    }

    #[test]
    fn beta_db_requires_isolated_name_and_local_host() {
        let good: tokio_postgres::Config = "postgres://localhost/codelocal_beta".parse().unwrap();
        assert!(good.get_dbname().unwrap().contains("beta"));
        let remote: tokio_postgres::Config =
            "postgres://db.remote.tld/codelocal_beta".parse().unwrap();
        assert!(
            !remote.get_hosts().iter().all(
                |h| matches!(h, tokio_postgres::config::Host::Tcp(name) if name == "localhost")
            )
        );
    }
}
