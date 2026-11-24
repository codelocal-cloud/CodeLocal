-- Beta-only OAuth public client registration and authorization code grants.
BEGIN;
CREATE TABLE IF NOT EXISTS codelocal_beta_oauth_clients (
    client_id TEXT PRIMARY KEY,
    client_name TEXT NOT NULL,
    redirect_uris TEXT[] NOT NULL CHECK (cardinality(redirect_uris) BETWEEN 1 AND 8),
    created_at BIGINT NOT NULL
);
CREATE TABLE IF NOT EXISTS codelocal_beta_oauth_codes (
    code_hash TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES codelocal_users(id) ON DELETE CASCADE,
    client_id TEXT NOT NULL REFERENCES codelocal_beta_oauth_clients(client_id) ON DELETE CASCADE,
    redirect_uri TEXT NOT NULL,
    challenge TEXT NOT NULL,
    resource TEXT NOT NULL,
    security_version BIGINT NOT NULL,
    expires_at BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_beta_oauth_codes_user ON codelocal_beta_oauth_codes(user_id,expires_at);
CREATE TABLE IF NOT EXISTS codelocal_beta_oauth_access (
    token_hash TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES codelocal_users(id) ON DELETE CASCADE,
    client_id TEXT NOT NULL REFERENCES codelocal_beta_oauth_clients(client_id) ON DELETE CASCADE,
    resource TEXT NOT NULL,
    security_version BIGINT NOT NULL,
    expires_at BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_beta_oauth_access_user ON codelocal_beta_oauth_access(user_id,expires_at);
COMMIT;
