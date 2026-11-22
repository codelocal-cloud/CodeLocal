-- Rust CodeLocal beta workspace catalog. Do not apply to production.
-- The public catalog contains no local machine filesystem paths.
BEGIN;
CREATE TABLE IF NOT EXISTS codelocal_workspaces (
    user_id TEXT NOT NULL REFERENCES codelocal_users(id) ON DELETE CASCADE,
    device_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    workspace_name TEXT NOT NULL,
    managed BOOLEAN NOT NULL DEFAULT FALSE,
    system_app BOOLEAN NOT NULL DEFAULT FALSE,
    created_at BIGINT NOT NULL,
    last_seen_at BIGINT NOT NULL,
    PRIMARY KEY(user_id, device_id, workspace_id)
);
CREATE INDEX IF NOT EXISTS idx_beta_workspaces_recent
    ON codelocal_workspaces(user_id, last_seen_at DESC);
COMMIT;
