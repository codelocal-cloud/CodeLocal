// Beta workspace preferences: no environment injection or execution.
#[derive(Debug, Clone, Default)]
pub struct RuntimePreference {
    pub mode: Option<String>,
    pub max_worktrees: Option<i32>,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub enum PreferenceChange {
    Mode(String),
    WorktreeLimit(Option<i32>),
}

impl AuthStore {
    /// Global has no persisted preferences in this slice. Device and workspace
    /// reads must prove ownership; revoked or removed targets are not visible.
    pub async fn runtime_preference(
        &self, user_id: &str, device_id: &str, workspace_id: &str,
    ) -> Result<Option<RuntimePreference>, StoreError> {
        if device_id.is_empty() {
            return Ok(Some(RuntimePreference::default()));
        }
        let client = self.pool.get().await
            .map_err(|_| unavailable("beta settings database unavailable"))?;
        if workspace_id.is_empty() {
            let owned: bool = client.query_one(
                "SELECT EXISTS(SELECT 1 FROM codelocal_devices
                 WHERE user_id=$1 AND device_id=$2 AND revoked_at IS NULL)",
                &[&user_id,&device_id],
            ).await.map_err(|_| unavailable("beta settings target lookup failed"))?.get(0);
            return Ok(owned.then(RuntimePreference::default));
        }
        let row = client.query_opt(
            "SELECT p.execution_mode,p.max_worktrees,COALESCE(p.updated_at,0)
             FROM codelocal_workspaces w
             LEFT JOIN codelocal_beta_runtime_preferences p
             ON p.user_id=w.user_id AND p.device_id=w.device_id AND p.workspace_id=w.workspace_id
             WHERE w.user_id=$1 AND w.device_id=$2 AND w.workspace_id=$3
             AND EXISTS(SELECT 1 FROM codelocal_devices d WHERE d.user_id=w.user_id
               AND d.device_id=w.device_id AND d.revoked_at IS NULL)",
            &[&user_id,&device_id,&workspace_id],
        ).await.map_err(|_| unavailable("beta settings lookup failed"))?;
        Ok(row.map(|r|RuntimePreference {
            mode:r.get(0),max_worktrees:r.get(1),updated_at:r.get(2)
        }))
    }

    /// Serializes with workspace sync/revocation by locking the device then
    /// the workspace. Audit and update are committed in the same transaction.
    pub async fn change_runtime_preference(
        &self, user_id: &str, device_id: &str, workspace_id: &str, change: PreferenceChange,
    ) -> Result<bool, StoreError> {
        let mut client = self.pool.get().await
            .map_err(|_| unavailable("beta settings database unavailable"))?;
        let tx = client.transaction().await
            .map_err(|_| unavailable("beta settings transaction unavailable"))?;
        let device = tx.query_opt(
            "SELECT credential_id FROM codelocal_devices
             WHERE user_id=$1 AND device_id=$2 AND revoked_at IS NULL
             ORDER BY created_at DESC LIMIT 1 FOR UPDATE",
            &[&user_id,&device_id],
        ).await.map_err(|_| unavailable("beta settings device lock failed"))?;
        if device.is_none() { return Ok(false); }
        let workspace = tx.query_opt(
            "SELECT workspace_id FROM codelocal_workspaces
             WHERE user_id=$1 AND device_id=$2 AND workspace_id=$3 FOR UPDATE",
            &[&user_id,&device_id,&workspace_id],
        ).await.map_err(|_| unavailable("beta settings workspace lock failed"))?;
        if workspace.is_none() { return Ok(false); }
        let now = now_millis();
        let (event, detail) = match change {
            PreferenceChange::Mode(mode) => {
                tx.execute(
                    "INSERT INTO codelocal_beta_runtime_preferences(user_id,device_id,workspace_id,execution_mode,updated_at)
                     VALUES($1,$2,$3,$4,$5) ON CONFLICT(user_id,device_id,workspace_id)
                     DO UPDATE SET execution_mode=EXCLUDED.execution_mode,updated_at=GREATEST(codelocal_beta_runtime_preferences.updated_at+1,EXCLUDED.updated_at)",
                    &[&user_id,&device_id,&workspace_id,&mode,&now],
                ).await.map_err(|_| unavailable("beta mode update failed"))?;
                ("runtime.execution_mode_updated",serde_json::json!({"mode":mode}))
            }
            PreferenceChange::WorktreeLimit(limit) => {
                tx.execute(
                    "INSERT INTO codelocal_beta_runtime_preferences(user_id,device_id,workspace_id,max_worktrees,updated_at)
                     VALUES($1,$2,$3,$4,$5) ON CONFLICT(user_id,device_id,workspace_id)
                     DO UPDATE SET max_worktrees=EXCLUDED.max_worktrees,updated_at=GREATEST(codelocal_beta_runtime_preferences.updated_at+1,EXCLUDED.updated_at)",
                    &[&user_id,&device_id,&workspace_id,&limit,&now],
                ).await.map_err(|_| unavailable("beta worktree limit update failed"))?;
                ("runtime.config_updated",serde_json::json!({"key":"CODELOCAL_MAX_WORKTREES","action":if limit.is_some() {"set"} else {"delete"}}))
            }
        };
        let mut bytes = [0u8;16];
        getrandom::fill(&mut bytes).map_err(|_| unavailable("secure random unavailable"))?;
        let audit_id = hex::encode(bytes);
        let details = serde_json::json!({"deviceId":device_id,"workspaceId":workspace_id,"change":detail}).to_string();
        tx.execute(
            "INSERT INTO codelocal_audit_logs(id,user_id,event,detail,created_at)
             VALUES($1,$2,$3,($4::text)::jsonb,$5)",
            &[&audit_id,&user_id,&event,&details,&now],
        ).await.map_err(|_| unavailable("beta settings audit failed"))?;
        tx.commit().await.map_err(|_| unavailable("beta settings commit failed"))?;
        Ok(true)
    }
}
