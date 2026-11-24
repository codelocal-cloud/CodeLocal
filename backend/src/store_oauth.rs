// Beta OAuth persistence. All bearer/code material is hashed at rest.
#[derive(Clone, Serialize)]
pub struct OAuthClient {
    pub client_id: String,
    pub client_name: String,
    pub redirect_uris: Vec<String>,
}

pub struct OAuthGrant<'a> {
    pub client_id: &'a str,
    pub redirect_uri: &'a str,
    pub challenge: &'a str,
    pub resource: &'a str,
}

fn oauth_random() -> Result<String, StoreError> {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    let mut bytes = [0u8;32];
    getrandom::fill(&mut bytes).map_err(|_| unavailable("OAuth randomness unavailable"))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn oauth_hash(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

impl AuthStore {
    pub async fn register_oauth_client(&self, name:&str, uris:&[String]) -> Result<OAuthClient,StoreError> {
        let mut db=self.pool.get().await.map_err(|_| unavailable("OAuth database unavailable"))?;
        let tx=db.transaction().await.map_err(|_| unavailable("OAuth transaction unavailable"))?;
        // Bound public registration storage; serialize quota checks only.
        tx.batch_execute("LOCK TABLE codelocal_beta_oauth_clients IN SHARE ROW EXCLUSIVE MODE")
            .await.map_err(|_| unavailable("OAuth registration lock failed"))?;
        let count:i64=tx.query_one("SELECT COUNT(*) FROM codelocal_beta_oauth_clients",&[])
            .await.map_err(|_| unavailable("OAuth registration quota unavailable"))?.get(0);
        if count>=1000 { return Err(unavailable("OAuth registration capacity reached")); }
        let id=format!("codelocal_{}",oauth_random()?);
        tx.execute("INSERT INTO codelocal_beta_oauth_clients(client_id,client_name,redirect_uris,created_at)
            VALUES($1,$2,$3,$4)",&[&id,&name,&uris,&now_millis()])
            .await.map_err(|_| unavailable("OAuth registration failed"))?;
        tx.commit().await.map_err(|_| unavailable("OAuth registration commit failed"))?;
        Ok(OAuthClient {client_id:id,client_name:name.into(),redirect_uris:uris.to_vec()})
    }

    pub async fn oauth_client(&self,id:&str) -> Result<Option<OAuthClient>,StoreError> {
        let db=self.pool.get().await.map_err(|_| unavailable("OAuth database unavailable"))?;
        let row=db.query_opt("SELECT client_id,client_name,redirect_uris FROM codelocal_beta_oauth_clients WHERE client_id=$1",&[&id])
            .await.map_err(|_| unavailable("OAuth client lookup failed"))?;
        Ok(row.map(|r| OAuthClient {client_id:r.get(0),client_name:r.get(1),redirect_uris:r.get(2)}))
    }

    pub async fn issue_oauth_code(&self,user:&User,grant:&OAuthGrant<'_>) -> Result<Option<String>,StoreError> {
        let mut db=self.pool.get().await.map_err(|_| unavailable("OAuth database unavailable"))?;
        let tx=db.transaction().await.map_err(|_| unavailable("OAuth transaction unavailable"))?;
        let row=tx.query_opt("SELECT security_version FROM codelocal_users WHERE id=$1 FOR NO KEY UPDATE",&[&user.id])
            .await.map_err(|_| unavailable("OAuth account lock failed"))?;
        if row.is_none_or(|r|r.get::<_,i64>(0)!=user.security_version) {return Ok(None);}
        let now=now_millis();
        tx.execute("DELETE FROM codelocal_beta_oauth_codes WHERE user_id=$1 AND expires_at<=$2",&[&user.id,&now])
            .await.map_err(|_| unavailable("OAuth code cleanup failed"))?;
        let count:i64=tx.query_one("SELECT COUNT(*) FROM codelocal_beta_oauth_codes WHERE user_id=$1",&[&user.id])
            .await.map_err(|_| unavailable("OAuth code quota unavailable"))?.get(0);
        if count>=64 {return Err(unavailable("OAuth code capacity reached"));}
        let code=oauth_random()?;
        tx.execute("INSERT INTO codelocal_beta_oauth_codes(code_hash,user_id,client_id,redirect_uri,challenge,resource,security_version,expires_at)
            VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
            &[&oauth_hash(&code),&user.id,&grant.client_id,&grant.redirect_uri,&grant.challenge,&grant.resource,&user.security_version,&(now+300_000)])
            .await.map_err(|_| unavailable("OAuth code insertion failed"))?;
        let audit_id=oauth_random()?;
        let detail=serde_json::json!({"clientId":grant.client_id}).to_string();
        tx.execute("INSERT INTO codelocal_audit_logs(id,user_id,event,detail,created_at)
            VALUES($1,$2,'oauth.authorized',($3::text)::jsonb,$4)",&[&audit_id,&user.id,&detail,&now])
            .await.map_err(|_| unavailable("OAuth audit failed"))?;
        tx.commit().await.map_err(|_| unavailable("OAuth code commit failed"))?;
        Ok(Some(code))
    }

    pub async fn exchange_oauth_code(&self,code:&str,grant:&OAuthGrant<'_>) -> Result<Option<String>,StoreError> {
        let mut db=self.pool.get().await.map_err(|_| unavailable("OAuth database unavailable"))?;
        let tx=db.transaction().await.map_err(|_| unavailable("OAuth exchange unavailable"))?;
        let hash=oauth_hash(code);
        // Lock the account before the code row, matching issue_oauth_code's order.
        let row=tx.query_opt("SELECT u.id,u.security_version FROM codelocal_users u
            JOIN codelocal_beta_oauth_codes c ON c.user_id=u.id WHERE c.code_hash=$1 FOR NO KEY UPDATE OF u",&[&hash])
            .await.map_err(|_| unavailable("OAuth exchange account lookup failed"))?;
        let Some(row)=row else{return Ok(None);};
        let uid:String=row.get(0);
        let version:i64=row.get(1);
        let now=now_millis();
        let consumed=tx.query_opt("DELETE FROM codelocal_beta_oauth_codes WHERE code_hash=$1
            AND user_id=$2 AND client_id=$3 AND redirect_uri=$4 AND challenge=$5 AND resource=$6
            AND security_version=$7 AND expires_at>$8 RETURNING user_id",
            &[&hash,&uid,&grant.client_id,&grant.redirect_uri,&grant.challenge,&grant.resource,&version,&now])
            .await.map_err(|_| unavailable("OAuth code consumption failed"))?;
        if consumed.is_none(){return Ok(None);}
        tx.execute("DELETE FROM codelocal_beta_oauth_access WHERE user_id=$1 AND (expires_at<=$2 OR security_version<>$3)",&[&uid,&now,&version])
            .await.map_err(|_| unavailable("OAuth access cleanup failed"))?;
        let count:i64=tx.query_one("SELECT COUNT(*) FROM codelocal_beta_oauth_access WHERE user_id=$1",&[&uid])
            .await.map_err(|_| unavailable("OAuth access quota unavailable"))?.get(0);
        if count>=128 {return Err(unavailable("OAuth access capacity reached"));}
        let token=oauth_random()?;
        tx.execute("INSERT INTO codelocal_beta_oauth_access(token_hash,user_id,client_id,resource,security_version,expires_at)
            VALUES($1,$2,$3,$4,$5,$6)",
            &[&oauth_hash(&token),&uid,&grant.client_id,&grant.resource,&version,&(now+3_600_000)])
            .await.map_err(|_| unavailable("OAuth token insertion failed"))?;
        tx.commit().await.map_err(|_| unavailable("OAuth exchange commit failed"))?;
        Ok(Some(token))
    }

    /// Resource-server identity, never derived from client-supplied account IDs.
    pub async fn oauth_access_account(&self, token: &str, resource: &str) -> Result<Option<String>, StoreError> {
        let db = self.pool.get().await.map_err(|_| unavailable("OAuth database unavailable"))?;
        let row = db.query_opt(
            "SELECT a.user_id FROM codelocal_beta_oauth_access a JOIN codelocal_users u ON u.id=a.user_id
             WHERE a.token_hash=$1 AND a.resource=$2 AND a.expires_at>$3 AND a.security_version=u.security_version",
            &[&oauth_hash(token), &resource, &now_millis()],
        ).await.map_err(|_| unavailable("OAuth token validation unavailable"))?;
        Ok(row.map(|r| r.get(0)))
    }

    pub async fn revoke_oauth_access(&self,client_id:&str,token:&str) -> Result<(),StoreError> {
        let db=self.pool.get().await.map_err(|_| unavailable("OAuth database unavailable"))?;
        db.execute("DELETE FROM codelocal_beta_oauth_access WHERE token_hash=$1 AND client_id=$2",
            &[&oauth_hash(token),&client_id]).await.map_err(|_| unavailable("OAuth token revocation failed"))?;
        Ok(())
    }
}
