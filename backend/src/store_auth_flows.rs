
impl AuthStore {
    /// Atomic fixed-window throttle with hashed subject. Independent beta namespace.
    pub async fn rate_limit(&self, scope: &'static str, subject: &str, limit: i64, seconds: u64) -> Result<bool, StoreError> {
        if subject.is_empty() || subject.len() > 1024 || limit <= 0 || seconds == 0 {
            return Err(unavailable("invalid rate limiter parameters"));
        }
        const LUA: &str = "local n=redis.call('INCR',KEYS[1]); if n==1 then redis.call('EXPIRE',KEYS[1],ARGV[1]) end; return n";
        let key = format!(
            "codelocal:beta:rate:{scope}:{}",
            hex::encode(Sha256::digest(subject.as_bytes()))
        );
        let mut conn = self.redis.get_multiplexed_async_connection().await
            .map_err(|_| unavailable("beta Redis unavailable"))?;
        let count: i64 = redis::cmd("EVAL").arg(LUA).arg(1).arg(&key).arg(seconds)
            .query_async(&mut conn).await.map_err(|_| unavailable("beta rate limit unavailable"))?;
        Ok(count <= limit)
    }

    pub async fn referral_allowed(&self, code: &str, email: &str, primary_admin: Option<&str>) -> Result<bool, StoreError> {
        let code = code.trim().to_ascii_uppercase();
        if code == "MMON" {
            return Ok(primary_admin.is_some_and(|admin| admin.eq_ignore_ascii_case(email)));
        }
        if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()) {
            return Ok(false);
        }
        let client = self.pool.get().await.map_err(|_| unavailable("beta PostgreSQL unavailable"))?;
        let found = client.query_opt(
            "SELECT id FROM codelocal_users WHERE UPPER(referral_code)=$1",
            &[&code],
        ).await.map_err(|_| unavailable("beta referral query failed"))?;
        Ok(found.is_some())
    }

    /// Account is created only after a verified 6-digit code; never seed a privileged user.
    pub async fn create_verified_user(&self, email: &str, hash: &str, salt: &str, invite: &str) -> Result<User, StoreError> {
        if email.len() > 254 || hash.len() != 86 || salt.len() > 64 {
            return Err(unavailable("invalid beta user data"));
        }
        let client = self.pool.get().await.map_err(|_| unavailable("beta PostgreSQL unavailable"))?;
        for _ in 0..10 {
            let mut id_bytes = [0u8; 16];
            let mut referral_bytes = [0u8; 6];
            getrandom::fill(&mut id_bytes).map_err(|_| unavailable("secure random unavailable"))?;
            getrandom::fill(&mut referral_bytes).map_err(|_| unavailable("secure random unavailable"))?;
            const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
            let code = referral_bytes.iter().map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char).collect::<String>();
            let id = hex::encode(id_bytes);
            let now = now_millis();
            let normalized_invite = invite.trim().to_ascii_uppercase();
            let row = client.query_opt(
                "INSERT INTO codelocal_users(id,email,password_hash,password_salt,referral_code,referred_by_code,created_at) \
                 VALUES($1,$2,$3,$4,$5,NULLIF($6,''),$7) \
                 ON CONFLICT DO NOTHING RETURNING id,email,password_hash,password_salt,password_changed_at,security_version,referral_code,referred_by_code,created_at",
                &[&id, &email, &hash, &salt, &code, &normalized_invite, &now],
            ).await;
            match row {
                Ok(Some(row)) => return Ok(User {
                    id: row.get(0), email: row.get(1), password_hash: row.get(2),
                    password_salt: row.get(3), password_changed_at: row.get(4),
                    security_version: row.get(5), referral_code: row.get(6),
                    referred_by_code: row.get(7), created_at: row.get(8),
                }),
                Ok(None) => {
                    if self.by_email(email).await?.is_some() {
                        return Err(unavailable("EMAIL_ALREADY_REGISTERED"));
                    }
                }
                Err(_) => {
                    // Unique constraints might race with registration. Do not leak DB errors.
                    if self.by_email(email).await?.is_some() {
                        return Err(unavailable("EMAIL_ALREADY_REGISTERED"));
                    }
                    return Err(unavailable("beta account insert failed"));
                }
            }
        }
        Err(unavailable("unable to allocate beta account"))
    }

    pub async fn update_password(&self, id: &str, new_hash: &str, new_salt: &str) -> Result<i64, StoreError> {
        let client = self.pool.get().await.map_err(|_| unavailable("beta PostgreSQL unavailable"))?;
        let changed_at = now_millis();
        let row = client.query_opt(
            "UPDATE codelocal_users SET password_hash=$2,password_salt=$3,password_changed_at=$4,security_version=security_version+1 WHERE id=$1 RETURNING security_version",
            &[&id, &new_hash, &new_salt, &changed_at],
        ).await.map_err(|_| unavailable("beta password update failed"))?;
        row.map(|row| row.get(0)).ok_or_else(|| unavailable("USER_NOT_FOUND"))
    }

    pub async fn pending_put<T: Serialize>(&self, kind: PendingKind, token: &str, value: &T, ttl_secs: u64) -> Result<(), StoreError> {
        if !valid_pending_token(token) || ttl_secs == 0 {
            return Err(unavailable("invalid pending operation"));
        }
        let raw = serde_json::to_string(value).map_err(|_| unavailable("pending serialize failed"))?;
        let mut conn = self.redis.get_multiplexed_async_connection().await
            .map_err(|_| unavailable("beta Redis unavailable"))?;
        let _: () = conn.set_ex(kind.key(token), raw, ttl_secs).await
            .map_err(|_| unavailable("beta pending storage failed"))?;
        Ok(())
    }

    pub async fn pending_get<T: serde::de::DeserializeOwned>(&self, kind: PendingKind, token: &str) -> Result<Option<(T, i64)>, StoreError> {
        if !valid_pending_token(token) {
            return Ok(None);
        }
        let mut conn = self.redis.get_multiplexed_async_connection().await
            .map_err(|_| unavailable("beta Redis unavailable"))?;
        // GET+TTL under one Lua script to avoid an expired pending key racing a TTL check.
        const LUA: &str = "local v=redis.call('GET',KEYS[1]); if not v then return {false,-2} end; return {v,redis.call('TTL',KEYS[1])}";
        let result: (Option<String>, i64) = redis::cmd("EVAL").arg(LUA).arg(1).arg(kind.key(token))
            .query_async(&mut conn).await.map_err(|_| unavailable("beta pending lookup failed"))?;
        match result {
            (Some(raw), ttl) if ttl > 0 => Ok(serde_json::from_str(&raw).ok().map(|v| (v, ttl))),
            _ => Ok(None),
        }
    }

    pub async fn pending_attempt(&self, kind: PendingKind, token: &str, max: i64) -> Result<bool, StoreError> {
        if !valid_pending_token(token) {
            return Ok(false);
        }
        const LUA: &str = r#"local key=KEYS[1]; if redis.call('EXISTS',key)==0 then return 0 end
            local count=redis.call('INCR',KEYS[2]); local ttl=redis.call('PTTL',key);
            if ttl>0 then redis.call('PEXPIRE',KEYS[2],ttl) end;
            if count>=tonumber(ARGV[1]) then redis.call('DEL',key,KEYS[2]); return 0 end
            return 1"#;
        let mut conn = self.redis.get_multiplexed_async_connection().await
            .map_err(|_| unavailable("beta Redis unavailable"))?;
        let ok: i64 = redis::cmd("EVAL").arg(LUA).arg(2).arg(kind.key(token))
            .arg(kind.attempt_key(token)).arg(max)
            .query_async(&mut conn).await.map_err(|_| unavailable("beta pending limit unavailable"))?;
        Ok(ok == 1)
    }

    pub async fn pending_lock(&self, kind: PendingKind, token: &str) -> Result<bool, StoreError> {
        if !valid_pending_token(token) {
            return Ok(false);
        }
        let mut conn = self.redis.get_multiplexed_async_connection().await
            .map_err(|_| unavailable("beta Redis unavailable"))?;
        let locked: Option<String> = redis::cmd("SET")
            .arg(kind.lock_key(token)).arg("1").arg("EX").arg(30).arg("NX")
            .query_async(&mut conn).await
            .map_err(|_| unavailable("beta pending lock unavailable"))?;
        Ok(locked.as_deref() == Some("OK"))
    }

    pub async fn pending_delete(&self, kind: PendingKind, token: &str) -> Result<(), StoreError> {
        if !valid_pending_token(token) {
            return Ok(());
        }
        let mut conn = self.redis.get_multiplexed_async_connection().await
            .map_err(|_| unavailable("beta Redis unavailable"))?;
        let _: i64 = redis::cmd("DEL")
            .arg(kind.key(token)).arg(kind.attempt_key(token)).arg(kind.lock_key(token))
            .query_async(&mut conn).await.map_err(|_| unavailable("beta pending delete failed"))?;
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub enum PendingKind { Signup, Reset }
impl PendingKind {
    fn key(self, token: &str) -> String {
        let namespace = match self {
            Self::Signup => "signup-verification",
            Self::Reset => "password-reset",
        };
        format!("codelocal:beta:{namespace}:{token}")
    }
    fn attempt_key(self, token: &str) -> String { format!("{}:attempts", self.key(token)) }
    fn lock_key(self, token: &str) -> String { format!("{}:finalize", self.key(token)) }
}
pub fn valid_pending_token(token: &str) -> bool {
    (24..=128).contains(&token.len()) && token.bytes().all(|b| b.is_ascii_alphanumeric() || b==b'_' || b==b'-')
}
