-- Isolated beta, encrypted secret storage. No plaintext or execution material.
BEGIN;
CREATE TABLE IF NOT EXISTS codelocal_beta_runtime_secrets (
    user_id TEXT NOT NULL REFERENCES codelocal_users(id) ON DELETE CASCADE,
    device_id TEXT,
    workspace_id TEXT,
    name TEXT NOT NULL CHECK (name ~ '^[A-Za-z_][A-Za-z0-9_]{0,127}$'),
    nonce BYTEA NOT NULL CHECK (octet_length(nonce)=12),
    ciphertext BYTEA NOT NULL CHECK (octet_length(ciphertext) BETWEEN 17 AND 4112),
    key_id TEXT NOT NULL CHECK (length(key_id)=64),
    updated_at BIGINT NOT NULL,
    UNIQUE NULLS NOT DISTINCT (user_id,device_id,workspace_id,name),
    CHECK (workspace_id IS NULL OR device_id IS NOT NULL),
    FOREIGN KEY(user_id,device_id) REFERENCES codelocal_devices(user_id,device_id) ON DELETE CASCADE,
    FOREIGN KEY(user_id,device_id,workspace_id)
      REFERENCES codelocal_workspaces(user_id,device_id,workspace_id) ON DELETE CASCADE
);
COMMIT;
