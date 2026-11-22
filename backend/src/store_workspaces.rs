// Signed beta workspace metadata sync. Never store local absolute paths.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceMetadata {
    pub workspace_id: String,
    pub workspace_name: String,
    #[serde(default)]
    pub managed: bool,
    #[serde(default)]
    pub system_app: bool,
}
#[derive(Debug, Clone)]
pub struct WorkspaceRow {
    pub device_id: String,
    pub device_name: String,
    pub workspace_id: String,
    pub workspace_name: String,
    pub managed: bool,
    pub system_app: bool,
    pub last_seen_at: i64,
}
impl AuthStore {
    /// Atomic Redis NX nonce gate: a successfully verified signature can be used once
    /// across all gateway instances. Fails closed if Redis cannot be reached.
    pub async fn consume_device_nonce(&self, credential_id: &str, nonce: &str) -> Result<bool, StoreError> {
        if credential_id.len() > 128 || nonce.len() > 128 {
            return Ok(false);
        }
        let key_hash = hex::encode(Sha256::digest(format!("{credential_id}\0{nonce}").as_bytes()));
        let key = format!("codelocal:beta:device-nonce:{key_hash}");
        let mut conn = self.redis.get_multiplexed_async_connection().await
            .map_err(|_| unavailable("beta Redis unavailable"))?;
        let inserted: Option<String> = redis::cmd("SET")
            .arg(key).arg("1").arg("NX").arg("EX").arg(240)
            .query_async(&mut conn).await
            .map_err(|_| unavailable("beta nonce store unavailable"))?;
        Ok(inserted.as_deref() == Some("OK"))
    }

    /// Requires authenticated, non-revoked device ownership already checked
    /// at the HTTP signed-request boundary. The entire set is replaced atomically.
    pub async fn sync_workspaces(&self, device: &DeviceRecord, workspaces: &[WorkspaceMetadata]) -> Result<u64, StoreError> {
        let mut client = self.pool.get().await.map_err(|_| unavailable("beta PostgreSQL unavailable"))?;
        let tx = client.transaction().await.map_err(|_| unavailable("beta workspace transaction unavailable"))?;
        // Serialize updates for the device with a row lock and recheck revocation.
        let current = tx.query_opt(
            "SELECT revoked_at FROM codelocal_devices WHERE credential_id=$1 AND user_id=$2 AND device_id=$3 FOR UPDATE",
            &[&device.credential_id, &device.user_id, &device.device_id],
        ).await.map_err(|_| unavailable("beta device lock failed"))?;
        match current {
            Some(row) if row.get::<_, Option<i64>>(0).is_none() => {}
            _ => return Err(unavailable("device revoked during workspace sync")),
        }
        let now = now_millis();
        for workspace in workspaces {
            tx.execute(
                "INSERT INTO codelocal_workspaces(user_id,device_id,workspace_id,workspace_name,managed,system_app,created_at,last_seen_at) \
                VALUES($1,$2,$3,$4,$5,$6,$7,$7) \
                ON CONFLICT(user_id,device_id,workspace_id) DO UPDATE SET \
                workspace_name=EXCLUDED.workspace_name,managed=EXCLUDED.managed,system_app=EXCLUDED.system_app,last_seen_at=EXCLUDED.last_seen_at",
                &[&device.user_id,&device.device_id,&workspace.workspace_id,&workspace.workspace_name,&workspace.managed,&workspace.system_app,&now]
            ).await.map_err(|_| unavailable("beta workspace insert failed"))?;
        }
        let ids: Vec<String> = workspaces.iter().map(|w| w.workspace_id.clone()).collect();
        let removed = tx.execute(
            "DELETE FROM codelocal_workspaces WHERE user_id=$1 AND device_id=$2 AND NOT (workspace_id=ANY($3::text[]))",
            &[&device.user_id,&device.device_id,&ids]
        ).await.map_err(|_| unavailable("beta workspace stale cleanup failed"))?;
        tx.execute(
            "UPDATE codelocal_devices SET last_seen_at=$2 WHERE credential_id=$1",
            &[&device.credential_id, &now]
        ).await.map_err(|_| unavailable("beta device touch failed"))?;
        tx.commit().await.map_err(|_| unavailable("beta workspace transaction commit failed"))?;
        Ok(removed)
    }

    pub async fn list_workspace_catalog(&self, user_id: &str) -> Result<Vec<WorkspaceRow>, StoreError> {
        let client = self.pool.get().await.map_err(|_| unavailable("beta PostgreSQL unavailable"))?;
        let rows = client.query(
            "SELECT w.device_id,d.device_name,w.workspace_id,w.workspace_name,w.managed,w.system_app,w.last_seen_at \
            FROM codelocal_workspaces w JOIN codelocal_devices d ON d.user_id=w.user_id AND d.device_id=w.device_id \
            WHERE w.user_id=$1 AND d.revoked_at IS NULL ORDER BY w.last_seen_at DESC LIMIT 1000",
            &[&user_id]
        ).await.map_err(|_| unavailable("beta workspace query failed"))?;
        Ok(rows.into_iter().map(|row| WorkspaceRow {
            device_id:row.get(0),device_name:row.get(1),workspace_id:row.get(2),
            workspace_name:row.get(3),managed:row.get(4),system_app:row.get(5),
            last_seen_at:row.get(6),
        }).collect())
    }
}
