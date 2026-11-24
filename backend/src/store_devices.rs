// Cloud device pairing v1: transactional approval/claim and credential ownership.
// Included from store.rs; shares the same isolated beta pool.

#[derive(Debug, Clone)]
pub struct PairingRecord {
    pub pairing_id: String,
    pub code: String,
    pub device_id: String,
    pub device_name: String,
    pub user_id: Option<String>,
    pub created_at: i64,
    pub expires_at: i64,
    pub approved_at: Option<i64>,
    pub claimed_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct DeviceRecord {
    pub credential_id: String,
    pub user_id: String,
    pub device_id: String,
    pub device_name: String,
    pub public_key: Option<String>,
    pub secret_hash: String,
    pub revoked_at: Option<i64>,
    pub created_at: i64,
    pub last_seen_at: i64,
}

impl PairingRecord {
    fn from_row(row: tokio_postgres::Row) -> Self {
        Self {
            pairing_id: row.get(0), code: row.get(1), device_id: row.get(2),
            device_name: row.get(3), user_id: row.get(4), created_at: row.get(5),
            expires_at: row.get(6), approved_at: row.get(7), claimed_at: row.get(8),
        }
    }
}
impl DeviceRecord {
    fn from_row(row: tokio_postgres::Row) -> Self {
        Self {
            credential_id: row.get(0), user_id: row.get(1), device_id: row.get(2),
            device_name: row.get(3), public_key: row.get(4), secret_hash: row.get(5),
            revoked_at: row.get(6), created_at: row.get(7), last_seen_at: row.get(8),
        }
    }
}

const PAIRING_COLS: &str =
    "pairing_id,code,device_id,device_name,user_id,created_at,expires_at,approved_at,claimed_at";
const DEVICE_COLS: &str =
    "credential_id,user_id,device_id,device_name,public_key,secret_hash,revoked_at,created_at,last_seen_at";

impl AuthStore {
    pub async fn create_pairing(&self, device_id: &str, device_name: &str, code: &str) -> Result<PairingRecord, StoreError> {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).map_err(|_| unavailable("secure randomness unavailable"))?;
        let id = hex::encode(bytes);
        let now = now_millis();
        let expiry = now + 600_000;
        let client = self.pool.get().await.map_err(|_| unavailable("beta PostgreSQL unavailable"))?;
        let row = client.query_one(
            "INSERT INTO codelocal_pairings(pairing_id,code,device_id,device_name,created_at,expires_at) VALUES($1,$2,$3,$4,$5,$6) RETURNING pairing_id,code,device_id,device_name,user_id,created_at,expires_at,approved_at,claimed_at",
            &[&id, &code, &device_id, &device_name, &now, &expiry],
        ).await.map_err(|_| unavailable("beta pair start failed"))?;
        Ok(PairingRecord::from_row(row))
    }

    pub async fn get_pairing(&self, id: &str) -> Result<Option<PairingRecord>, StoreError> {
        if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) { return Ok(None); }
        let client = self.pool.get().await.map_err(|_| unavailable("beta PostgreSQL unavailable"))?;
        let row = client.query_opt(
            &format!("SELECT {PAIRING_COLS} FROM codelocal_pairings WHERE pairing_id=$1"),
            &[&id],
        ).await.map_err(|_| unavailable("beta pairing lookup failed"))?;
        Ok(row.map(PairingRecord::from_row))
    }

    pub async fn approve_pairing(&self, id: &str, code: &str, user_id: &str) -> Result<Option<PairingRecord>, StoreError> {
        let now = now_millis();
        let client = self.pool.get().await.map_err(|_| unavailable("beta PostgreSQL unavailable"))?;
        let row = client.query_opt(
            &format!("UPDATE codelocal_pairings SET user_id=$3,approved_at=$4 WHERE pairing_id=$1 AND code=$2 AND expires_at>$4 AND approved_at IS NULL AND claimed_at IS NULL AND user_id IS NULL RETURNING {PAIRING_COLS}"),
            &[&id, &code, &user_id, &now],
        ).await.map_err(|_| unavailable("beta pair approval failed"))?;
        Ok(row.map(PairingRecord::from_row))
    }

    /// Retry-safe, single-use claim, under SQL row lock.
    pub async fn claim_pairing(&self, id: &str, code: &str, credential_id: &str, secret_hash: &str, public_key: Option<&str>)
        -> Result<Option<DeviceRecord>, StoreError> {
        let mut client = self.pool.get().await.map_err(|_| unavailable("beta PostgreSQL unavailable"))?;
        let tx = client.transaction().await.map_err(|_| unavailable("beta pairing transaction failed"))?;
        let row = tx.query_opt(
            &format!("SELECT {PAIRING_COLS} FROM codelocal_pairings WHERE pairing_id=$1 AND code=$2 FOR UPDATE"),
            &[&id, &code],
        ).await.map_err(|_| unavailable("beta pair lookup failed"))?;
        let Some(pair) = row.map(PairingRecord::from_row) else {
            return Ok(None);
        };
        let Some(user_id) = pair.user_id else { return Ok(None); };
        if pair.approved_at.is_none() {
            return Ok(None);
        }
        if pair.claimed_at.is_some() {
            // Idempotent response retry only with same credential hash/public key,
            // and only while that credential remains the current active one.
            let existing = tx.query_opt(
                &format!("SELECT {DEVICE_COLS} FROM codelocal_devices WHERE user_id=$1 AND device_id=$2 AND credential_id=$3"),
                &[&user_id, &pair.device_id, &credential_id],
            ).await.map_err(|_| unavailable("beta device lookup failed"))?;
            let Some(device) = existing.map(DeviceRecord::from_row) else { return Ok(None) };
            if device.revoked_at.is_some() ||
               !bool::from(subtle::ConstantTimeEq::ct_eq(device.secret_hash.as_bytes(), secret_hash.as_bytes())) ||
               device.public_key.as_deref() != public_key {
                return Ok(None);
            }
            return Ok(Some(device));
        }
        if pair.expires_at <= now_millis() {
            return Ok(None);
        }
        let now = now_millis();
        let public_key = public_key.filter(|key| !key.is_empty());
        tx.execute("UPDATE codelocal_pairings SET claimed_at=$2 WHERE pairing_id=$1", &[&id, &now])
            .await.map_err(|_| unavailable("beta pair claim update failed"))?;
        let row = tx.query_one(
            &format!("INSERT INTO codelocal_devices(credential_id,user_id,device_id,device_name,public_key,secret_hash,created_at,last_seen_at) \
            VALUES($1,$2,$3,$4,$5,$6,$7,$7) ON CONFLICT(user_id,device_id) DO UPDATE SET \
            credential_id=EXCLUDED.credential_id, device_name=EXCLUDED.device_name,public_key=EXCLUDED.public_key,secret_hash=EXCLUDED.secret_hash,created_at=EXCLUDED.created_at,last_seen_at=EXCLUDED.last_seen_at,revoked_at=NULL \
            RETURNING {DEVICE_COLS}"),
            &[&credential_id, &user_id, &pair.device_id, &pair.device_name, &public_key, &secret_hash, &now],
        ).await.map_err(|_| unavailable("beta device credential claim failed"))?;
        // New credentials must never inherit device/workspace secrets from
        // an earlier pairing. Preserve global account-owned secrets only.
        tx.execute(
            "DELETE FROM codelocal_beta_runtime_secrets WHERE user_id=$1 AND device_id=$2",
            &[&user_id,&pair.device_id],
        ).await.map_err(|_| unavailable("beta stale device secret cleanup failed"))?;
        tx.commit().await.map_err(|_| unavailable("beta pairing transaction commit failed"))?;
        Ok(Some(DeviceRecord::from_row(row)))
    }

    pub async fn authenticate_device(&self, credential_id: &str, candidate_hash: &str) -> Result<Option<DeviceRecord>, StoreError> {
        if credential_id.len() > 128 || !credential_id.starts_with("cld_") || candidate_hash.len() != 64 {
            return Ok(None);
        }
        let client = self.pool.get().await.map_err(|_| unavailable("beta PostgreSQL unavailable"))?;
        let row = client.query_opt(
            &format!("SELECT {DEVICE_COLS} FROM codelocal_devices WHERE credential_id=$1"),
            &[&credential_id],
        ).await.map_err(|_| unavailable("beta device auth lookup failed"))?;
        let Some(device) = row.map(DeviceRecord::from_row) else { return Ok(None); };
        if device.revoked_at.is_some()
            || !bool::from(subtle::ConstantTimeEq::ct_eq(
                device.secret_hash.as_bytes(),
                candidate_hash.as_bytes(),
            ))
        {
            return Ok(None);
        }
        Ok(Some(device))
    }


    pub async fn list_devices(&self, user_id: &str) -> Result<Vec<DeviceRecord>, StoreError> {
        let client = self.pool.get().await.map_err(|_| unavailable("beta PostgreSQL unavailable"))?;
        let rows = client.query(
            &format!("SELECT {DEVICE_COLS} FROM codelocal_devices WHERE user_id=$1 ORDER BY last_seen_at DESC"),
            &[&user_id],
        ).await.map_err(|_| unavailable("beta device listing failed"))?;
        Ok(rows.into_iter().map(DeviceRecord::from_row).collect())
    }

    pub async fn revoke_device(&self, user_id: &str, credential_id: &str) -> Result<bool, StoreError> {
        let mut client = self.pool.get().await.map_err(|_| unavailable("beta PostgreSQL unavailable"))?;
        let tx = client.transaction().await.map_err(|_| unavailable("beta revoke transaction failed"))?;
        let row = tx.query_opt(
            "UPDATE codelocal_devices SET revoked_at=$3 WHERE user_id=$1 AND credential_id=$2 AND revoked_at IS NULL RETURNING device_id",
            &[&user_id,&credential_id,&now_millis()],
        ).await.map_err(|_| unavailable("beta device revoke failed"))?;
        let Some(row)=row else {return Ok(false);};
        let device_id:String = row.get(0);
        tx.execute(
            "DELETE FROM codelocal_beta_runtime_secrets WHERE user_id=$1 AND device_id=$2",
            &[&user_id,&device_id],
        ).await.map_err(|_| unavailable("beta revoked device secret cleanup failed"))?;
        tx.commit().await.map_err(|_| unavailable("beta revoke commit failed"))?;
        Ok(true)
    }
}
