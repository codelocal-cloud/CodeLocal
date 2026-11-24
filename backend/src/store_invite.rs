// Account-scoped referral members. No unmasked email leaves the store layer.
#[derive(Debug, Clone)]
pub struct InvitedUser {
    pub id: String,
    pub email: String,
    pub created_at: i64,
}

impl AuthStore {
    /// The browser's own verified referral code is always passed here; never
    /// accept an arbitrary referral query parameter from an unauthenticated caller.
    pub async fn invited_by_referral(&self, code: &str) -> Result<Vec<InvitedUser>, StoreError> {
        if !crate::invite::valid_referral_code(code) {
            return Err(unavailable("invalid beta referral code"));
        }
        let client = self.pool.get().await
            .map_err(|_| unavailable("beta invite database unavailable"))?;
        let rows = client.query(
            "SELECT id,email,created_at FROM codelocal_users
             WHERE UPPER(referred_by_code)=UPPER($1)
             ORDER BY created_at DESC,email ASC LIMIT 501",
            &[&code],
        ).await.map_err(|_| unavailable("beta invitation lookup unavailable"))?;
        // Fail closed rather than pretending a truncated member list is complete.
        if rows.len() > 500 {
            return Err(unavailable("beta invite member pagination required"));
        }
        Ok(rows.into_iter().map(|row| InvitedUser {
            id:row.get(0),
            email:row.get(1),
            created_at:row.get(2),
        }).collect())
    }

    /// Real Redis beta runtime leases for devices owned by the selected invited
    /// users. MCP activity is not modeled as runtime presence.
    pub async fn invited_user_runtime_online(
        &self, ids: &[String],
    ) -> Result<std::collections::HashSet<String>, StoreError> {
        if ids.is_empty() {
            return Ok(std::collections::HashSet::new());
        }
        if ids.len() > 500 || ids.iter().any(|id| id.is_empty() || id.len() > 128) {
            return Err(unavailable("invalid beta invited account set"));
        }
        let client = self.pool.get().await
            .map_err(|_| unavailable("beta invited device database unavailable"))?;
        let rows = client.query(
            "SELECT user_id,credential_id FROM codelocal_devices
             WHERE user_id=ANY($1::text[]) AND revoked_at IS NULL
             ORDER BY user_id,credential_id LIMIT 3001",
            &[&ids],
        ).await.map_err(|_| unavailable("beta invited devices lookup unavailable"))?;
        if rows.len() > 3000 {
            return Err(unavailable("beta invite devices pagination required"));
        }
        if rows.is_empty() {
            return Ok(std::collections::HashSet::new());
        }
        let mut user_ids = Vec::with_capacity(rows.len());
        let mut keys = Vec::with_capacity(rows.len());
        for row in rows {
            let user_id:String = row.get(0);
            let credential_id:String = row.get(1);
            user_ids.push(user_id);
            keys.push(Self::presence_key(&credential_id)?);
        }
        let mut conn = self.redis.get_multiplexed_async_connection().await
            .map_err(|_| unavailable("beta invite Redis presence unavailable"))?;
        let values:Vec<Option<String>> = redis::cmd("MGET").arg(keys)
            .query_async(&mut conn).await
            .map_err(|_| unavailable("beta invited runtime presence lookup failed"))?;
        let online = user_ids.into_iter().zip(values).filter_map(|(user_id, value)| {
            value.as_deref().and_then(RuntimePresence::decode).map(|_|user_id)
        }).collect();
        Ok(online)
    }
}
