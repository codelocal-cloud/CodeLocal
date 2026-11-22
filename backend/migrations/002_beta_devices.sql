-- CodeLocal Rust BETA ONLY: device pairing and credentials, Go-compatible columns.
-- Apply only after 001_beta_auth.sql on a separately provisioned codelocal_beta DB.
BEGIN;

CREATE TABLE IF NOT EXISTS codelocal_pairings (
    pairing_id TEXT PRIMARY KEY,
    code TEXT NOT NULL,
    device_id TEXT NOT NULL,
    device_name TEXT NOT NULL,
    user_id TEXT REFERENCES codelocal_users(id) ON DELETE CASCADE,
    created_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    approved_at BIGINT,
    claimed_at BIGINT
);
CREATE INDEX IF NOT EXISTS idx_codelocal_pairings_expires
    ON codelocal_pairings(expires_at);

CREATE TABLE IF NOT EXISTS codelocal_devices (
    credential_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES codelocal_users(id) ON DELETE CASCADE,
    device_id TEXT NOT NULL,
    device_name TEXT NOT NULL,
    public_key TEXT,
    secret_hash TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    last_seen_at BIGINT NOT NULL,
    revoked_at BIGINT,
    UNIQUE(user_id, device_id)
);
CREATE INDEX IF NOT EXISTS idx_codelocal_devices_user
    ON codelocal_devices(user_id);

COMMIT;
