// Bounded administrative read model. Never select password material or credentials.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminUserRow {
    pub id: String,
    pub email: String,
    pub referral_code: String,
    pub referred_by_code: String,
    pub created_at: i64,
    pub invite_count: i64,
    pub last_device_seen_at: i64,
    pub last_mcp_used_at: i64,
    pub runtime_active: bool,
    pub mcp_active: bool,
}

impl AuthStore {
    pub async fn admin_users(&self) -> Result<Vec<AdminUserRow>, StoreError> {
        let client = self.pool.get().await
            .map_err(|_| unavailable("beta admin database unavailable"))?;
        let rows = client.query(
            "SELECT u.id,u.email,u.referral_code,COALESCE(u.referred_by_code,''),u.created_at,
             (SELECT COUNT(*) FROM codelocal_users c WHERE UPPER(c.referred_by_code)=UPPER(u.referral_code)),
             COALESCE((SELECT MAX(d.last_seen_at) FROM codelocal_devices d WHERE d.user_id=u.id AND d.revoked_at IS NULL),0),
             COALESCE((SELECT MAX(m.created_at) FROM codelocal_beta_mcp_usage_events m WHERE m.user_id=u.id),0)
             FROM codelocal_users u ORDER BY u.created_at ASC,u.email ASC LIMIT 501",
            &[],
        ).await.map_err(|_| unavailable("beta admin user query failed"))?;
        if rows.len() > 500 {
            return Err(unavailable("beta admin pagination required"));
        }
        let mut users: Vec<AdminUserRow> = rows.into_iter().map(|r| AdminUserRow {
            id:r.get(0),email:r.get(1),referral_code:r.get(2),referred_by_code:r.get(3),
            created_at:r.get(4),invite_count:r.get(5),last_device_seen_at:r.get(6),
            last_mcp_used_at:r.get(7),runtime_active:false,mcp_active:false,
        }).collect();
        drop(client);
        let ids:Vec<String> = users.iter().map(|u|u.id.clone()).collect();
        let online = self.invited_user_runtime_online(&ids).await?;
        for user in &mut users {
            user.runtime_active = online.contains(&user.id);
        }
        // MCP active sessions are not ported. Persisted historical usage is
        // deliberately not treated as evidence of an active MCP connection.
        Ok(users)
    }
}
