-- Isolated beta runtime preferences, not an execution authorization.
BEGIN;
CREATE TABLE IF NOT EXISTS codelocal_beta_runtime_preferences (
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    execution_mode TEXT CHECK (execution_mode IN ('safe','live')),
    max_worktrees INTEGER CHECK (max_worktrees BETWEEN 1 AND 20),
    updated_at BIGINT NOT NULL,
    PRIMARY KEY(user_id,device_id,workspace_id),
    FOREIGN KEY(user_id,device_id,workspace_id)
      REFERENCES codelocal_workspaces(user_id,device_id,workspace_id) ON DELETE CASCADE
);
COMMIT;
