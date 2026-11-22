// Redis-backed beta runtime presence. Not a privileged runtime routing channel.
// Each active WebSocket owns a unique lease. Older sockets cannot refresh or
// clear presence once a newer signed socket replaces their lease.
const PRESENCE_PREFIX: &str = "codelocal:beta:runtime-presence:";
pub const PRESENCE_TTL_SECONDS: u32 = 45;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePresence {
    pub lease: String,
    pub workspace_ids: Vec<String>,
}

impl RuntimePresence {
    fn encoded(&self) -> String {
        format!("{}:{}", self.lease, self.workspace_ids.join(","))
    }
    fn decode(raw: &str) -> Option<Self> {
        let (lease, ids) = raw.split_once(':')?;
        if lease.len() != 32 || !lease.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let workspaces = if ids.is_empty() {
            Vec::new()
        } else {
            ids.split(',').map(str::to_owned).collect()
        };
        if workspaces.len() > 100 || workspaces.iter().any(|id| !crate::runtime::valid_workspace_id(id)) {
            return None;
        }
        Some(Self {lease:lease.to_owned(), workspace_ids:workspaces})
    }
}

impl AuthStore {
    fn presence_key(credential_id: &str) -> Result<String, StoreError> {
        if credential_id.len() > 128
            || !credential_id.starts_with("cld_")
            || !credential_id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(unavailable("invalid beta device credential identifier"));
        }
        Ok(format!("{PRESENCE_PREFIX}{credential_id}"))
    }

    pub async fn begin_runtime_presence(
        &self, credential_id: &str, workspaces: &[String],
    ) -> Result<RuntimePresence, StoreError> {
        let key = Self::presence_key(credential_id)?;
        if !crate::runtime::valid_workspace_ids(workspaces) {
            return Err(unavailable("invalid beta runtime workspace list"));
        }
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random)
            .map_err(|_| unavailable("runtime secure random unavailable"))?;
        let lease = RuntimePresence {
            lease:hex::encode(random), workspace_ids:workspaces.to_vec()
        };
        let mut conn = self.redis.get_multiplexed_async_connection().await
            .map_err(|_| unavailable("beta Redis runtime presence unavailable"))?;
        let _: String = redis::cmd("SET")
            .arg(key).arg(lease.encoded()).arg("EX").arg(PRESENCE_TTL_SECONDS)
            .query_async(&mut conn).await
            .map_err(|_| unavailable("beta Redis presence registration failed"))?;
        Ok(lease)
    }

    pub async fn refresh_runtime_presence(
        &self, credential_id: &str, current: &RuntimePresence, workspaces: &[String],
    ) -> Result<bool, StoreError> {
        let key = Self::presence_key(credential_id)?;
        if !crate::runtime::valid_workspace_ids(workspaces) {
            return Err(unavailable("invalid beta runtime workspace list"));
        }
        let fresh = RuntimePresence { lease:current.lease.clone(), workspace_ids:workspaces.to_vec() };
        const LUA: &str = r#"
            local current=redis.call('GET',KEYS[1])
            if current == false or string.sub(current,1,string.len(ARGV[1])) ~= ARGV[1] then return 0 end
            redis.call('SET',KEYS[1],ARGV[2],'EX',ARGV[3])
            return 1
        "#;
        let mut conn = self.redis.get_multiplexed_async_connection().await
            .map_err(|_| unavailable("beta Redis runtime presence unavailable"))?;
        let renewed: i64 = redis::cmd("EVAL").arg(LUA).arg(1)
            .arg(key).arg(format!("{}:",current.lease))
            .arg(fresh.encoded()).arg(PRESENCE_TTL_SECONDS)
            .query_async(&mut conn).await
            .map_err(|_| unavailable("beta Redis runtime renewal failed"))?;
        Ok(renewed == 1)
    }

    pub async fn clear_runtime_presence(
        &self, credential_id: &str, current: &RuntimePresence,
    ) -> Result<(), StoreError> {
        let key = Self::presence_key(credential_id)?;
        const LUA: &str = r#"
            local current=redis.call('GET',KEYS[1])
            if current ~= false and string.sub(current,1,string.len(ARGV[1])) == ARGV[1] then
                redis.call('DEL',KEYS[1]); return 1
            end
            return 0
        "#;
        let mut conn = self.redis.get_multiplexed_async_connection().await
            .map_err(|_| unavailable("beta Redis runtime presence unavailable"))?;
        let _: i64 = redis::cmd("EVAL").arg(LUA).arg(1)
            .arg(key).arg(format!("{}:",current.lease))
            .query_async(&mut conn).await
            .map_err(|_| unavailable("beta Redis runtime removal failed"))?;
        Ok(())
    }

    pub async fn runtime_presence(
        &self, credential_id: &str,
    ) -> Result<Option<RuntimePresence>, StoreError> {
        let key = Self::presence_key(credential_id)?;
        let mut conn = self.redis.get_multiplexed_async_connection().await
            .map_err(|_| unavailable("beta Redis runtime presence unavailable"))?;
        let raw: Option<String> = redis::cmd("GET").arg(key)
            .query_async(&mut conn).await
            .map_err(|_| unavailable("beta Redis runtime lookup failed"))?;
        Ok(raw.as_deref().and_then(RuntimePresence::decode))
    }

    /// A runtime may advertise only workspace IDs currently in its
    /// own device-specific, account-scoped metadata catalog.
    pub async fn registered_runtime_workspaces(
        &self, user_id: &str, device_id: &str,
    ) -> Result<Vec<String>, StoreError> {
        let client = self.pool.get().await
            .map_err(|_| unavailable("beta PostgreSQL runtime grants unavailable"))?;
        let rows = client.query(
            "SELECT workspace_id FROM codelocal_workspaces WHERE user_id=$1 AND device_id=$2 ORDER BY workspace_id LIMIT 101",
            &[&user_id, &device_id],
        ).await.map_err(|_| unavailable("beta runtime grants lookup failed"))?;
        Ok(rows.into_iter().map(|r|r.get(0)).collect())
    }
}

#[cfg(test)]
mod runtime_presence_tests {
    use super::*;

    #[test]
    fn lease_payload_round_trip_and_invalid_data_fail_closed() {
        let state = RuntimePresence {
            lease:"0123456789abcdef0123456789abcdef".into(),
            workspace_ids:vec!["project-123".into(),"other.ws".into()],
        };
        assert_eq!(RuntimePresence::decode(&state.encoded()),Some(state));
        assert_eq!(RuntimePresence::decode("not-a-lease:abc"),None);
        assert_eq!(RuntimePresence::decode("0123456789abcdef0123456789abcdef:../escape"),None);
        assert_eq!(RuntimePresence::decode("0123456789abcdef0123456789abcdef:has:colon"),None);
        assert!(RuntimePresence::decode("0123456789abcdef0123456789abcdef:").is_some());
    }
}
