// Encrypted secret writes and metadata-only reads for owned runtime scopes.
#[derive(Default)]
pub struct RuntimeSecretRefs {
    pub layer: std::collections::BTreeMap<String, serde_json::Value>,
    pub effective: std::collections::BTreeMap<String, serde_json::Value>,
    pub updated_at: i64,
    pub layer_updated_at: i64,
}

impl AuthStore {
    pub async fn runtime_secret_refs(
        &self, user: &str, device: &str, workspace: &str,
    ) -> Result<RuntimeSecretRefs, StoreError> {
        let device = (!device.is_empty()).then_some(device);
        let workspace = (!workspace.is_empty()).then_some(workspace);
        let client = self.pool.get().await.map_err(|_| unavailable("beta secret database unavailable"))?;
        // Never select encrypted material, key IDs or nonces for dashboard reads.
        // Order global first, then device, then workspace for override semantics.
        let rows = client.query(
            "SELECT name,device_id,workspace_id,updated_at FROM codelocal_beta_runtime_secrets
             WHERE user_id=$1 AND (device_id IS NULL OR device_id=$2)
             AND (workspace_id IS NULL OR workspace_id=$3)
             AND (device_id IS NULL OR EXISTS(SELECT 1 FROM codelocal_devices d
                WHERE d.user_id=$1 AND d.device_id=codelocal_beta_runtime_secrets.device_id AND d.revoked_at IS NULL))
             ORDER BY device_id NULLS FIRST,workspace_id NULLS FIRST,name LIMIT 193",
            &[&user,&device,&workspace],
        ).await.map_err(|_| unavailable("beta secret references unavailable"))?;
        if rows.len() > 192 { return Err(unavailable("beta secret reference limit")); }
        let mut refs = RuntimeSecretRefs::default();
        for row in rows {
            let name: String = row.get(0);
            let d: Option<String> = row.get(1);
            let w: Option<String> = row.get(2);
            let updated: i64 = row.get(3);
            refs.effective.insert(name.clone(),serde_json::json!({"configured":true}));
            refs.updated_at = refs.updated_at.max(updated);
            if d.as_deref()==device && w.as_deref()==workspace {
                refs.layer.insert(name,serde_json::json!({"configured":true}));
                refs.layer_updated_at = refs.layer_updated_at.max(updated);
            }
        }
        // Include the latest change even when the last secret was deleted.
        let row = client.query_one(
            "SELECT COALESCE(MAX(created_at),0),
                    COALESCE(MAX(created_at) FILTER (WHERE device_id IS NOT DISTINCT FROM $2
                        AND workspace_id IS NOT DISTINCT FROM $3),0)
             FROM codelocal_audit_logs WHERE user_id=$1 AND event='runtime.secret_updated'
             AND (device_id IS NULL OR device_id=$2)
             AND (workspace_id IS NULL OR workspace_id=$3)",
            &[&user,&device,&workspace],
        ).await.map_err(|_| unavailable("beta secret revision unavailable"))?;
        refs.updated_at = refs.updated_at.max(row.get(0));
        refs.layer_updated_at = refs.layer_updated_at.max(row.get(1));
        Ok(refs)
    }

    pub async fn change_runtime_secret(
        &self,user:&str,device:&str,workspace:&str,name:&str,
        sealed:Option<crate::runtime_secrets::SealedSecret>,
    ) -> Result<bool,StoreError> {
        if !crate::runtime_secrets::valid_name(name) || (device.is_empty() && !workspace.is_empty()) {
            return Err(unavailable("invalid beta secret target"));
        }
        let device = (!device.is_empty()).then_some(device);
        let workspace = (!workspace.is_empty()).then_some(workspace);
        let mut client = self.pool.get().await.map_err(|_| unavailable("beta secret database unavailable"))?;
        let tx = client.transaction().await.map_err(|_| unavailable("beta secret transaction unavailable"))?;
        // Serialize all secret changes for an account, without blocking FK
        // KEY SHARE locks in concurrent workspace sync.
        if tx.query_opt("SELECT id FROM codelocal_users WHERE id=$1 FOR NO KEY UPDATE",&[&user])
            .await.map_err(|_| unavailable("beta secret account lock failed"))?.is_none() {
            return Ok(false);
        }
        if let Some(d)=device
            && tx.query_opt("SELECT credential_id FROM codelocal_devices
              WHERE user_id=$1 AND device_id=$2 AND revoked_at IS NULL FOR UPDATE",&[&user,&d])
                .await.map_err(|_| unavailable("beta secret device lock failed"))?.is_none() {
            return Ok(false);
        }
        if let Some(w)=workspace
            && tx.query_opt("SELECT workspace_id FROM codelocal_workspaces
              WHERE user_id=$1 AND device_id=$2 AND workspace_id=$3 FOR UPDATE",&[&user,&device,&w])
                .await.map_err(|_| unavailable("beta secret workspace lock failed"))?.is_none() {
            return Ok(false);
        }
        let now=now_millis();
        let action=if let Some(sealed)=sealed {
            let row=tx.query_one(
                "SELECT COUNT(*),COALESCE(BOOL_OR(name=$4),FALSE)
                 FROM codelocal_beta_runtime_secrets WHERE user_id=$1
                 AND device_id IS NOT DISTINCT FROM $2 AND workspace_id IS NOT DISTINCT FROM $3",
                &[&user,&device,&workspace,&name],
            ).await.map_err(|_| unavailable("beta secret quota unavailable"))?;
            if row.get::<_,i64>(0)>=crate::runtime_secrets::MAX_SECRETS_PER_LAYER && !row.get::<_,bool>(1) {
                return Err(unavailable("beta secret layer full"));
            }
            tx.execute(
                "INSERT INTO codelocal_beta_runtime_secrets(user_id,device_id,workspace_id,name,nonce,ciphertext,key_id,updated_at)
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8)
                 ON CONFLICT(user_id,device_id,workspace_id,name) DO UPDATE SET
                 nonce=EXCLUDED.nonce,ciphertext=EXCLUDED.ciphertext,key_id=EXCLUDED.key_id,
                 updated_at=GREATEST(codelocal_beta_runtime_secrets.updated_at+1,EXCLUDED.updated_at)",
                &[&user,&device,&workspace,&name,&sealed.nonce,&sealed.ciphertext,&sealed.key_id,&now],
            ).await.map_err(|_| unavailable("beta secret update failed"))?;
            "set"
        } else {
            tx.execute(
                "DELETE FROM codelocal_beta_runtime_secrets WHERE user_id=$1
                AND device_id IS NOT DISTINCT FROM $2 AND workspace_id IS NOT DISTINCT FROM $3 AND name=$4",
                &[&user,&device,&workspace,&name],
            ).await.map_err(|_| unavailable("beta secret removal failed"))?;
            "delete"
        };
        let mut random=[0u8;16];
        getrandom::fill(&mut random).map_err(|_| unavailable("secure random unavailable"))?;
        let id=hex::encode(random);
        let detail=serde_json::json!({"name":name,"action":action}).to_string();
        tx.execute(
            "INSERT INTO codelocal_audit_logs(id,user_id,event,device_id,workspace_id,detail,created_at)
             VALUES($1,$2,'runtime.secret_updated',$3,$4,($5::text)::jsonb,$6)",
            &[&id,&user,&device,&workspace,&detail,&now],
        ).await.map_err(|_| unavailable("beta secret audit failed"))?;
        tx.commit().await.map_err(|_| unavailable("beta secret commit failed"))?;
        Ok(true)
    }
}
