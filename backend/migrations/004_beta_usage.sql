-- CodeLocal Rust BETA-ONLY usage telemetry schema.
-- This is a subset of the Go usage migrations (rollup + provider chat).
-- Apply to disposable *beta* PostgreSQL only.
BEGIN;

CREATE TABLE IF NOT EXISTS codelocal_mcp_usage (
    user_id TEXT NOT NULL REFERENCES codelocal_users(id) ON DELETE CASCADE,
    bucket_start BIGINT NOT NULL,
    device_id TEXT NOT NULL DEFAULT '',
    workspace_id TEXT NOT NULL DEFAULT '',
    tool TEXT NOT NULL,
    calls BIGINT NOT NULL DEFAULT 0 CHECK (calls >= 0),
    input_bytes BIGINT NOT NULL DEFAULT 0 CHECK (input_bytes >= 0),
    output_bytes BIGINT NOT NULL DEFAULT 0 CHECK (output_bytes >= 0),
    input_tokens_est BIGINT NOT NULL DEFAULT 0 CHECK (input_tokens_est >= 0),
    output_tokens_est BIGINT NOT NULL DEFAULT 0 CHECK (output_tokens_est >= 0),
    PRIMARY KEY(user_id,bucket_start,device_id,workspace_id,tool)
);
CREATE INDEX IF NOT EXISTS idx_beta_mcp_usage_user_time
    ON codelocal_mcp_usage(user_id,bucket_start DESC);

-- Each MCP invocation is deduplicated before incrementing the rollup.
-- Separate beta table ensures retries never double count token estimates.
CREATE TABLE IF NOT EXISTS codelocal_beta_mcp_usage_events (
    event_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES codelocal_users(id) ON DELETE CASCADE,
    created_at BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_beta_mcp_events_user_time
    ON codelocal_beta_mcp_usage_events(user_id,created_at DESC);

CREATE TABLE IF NOT EXISTS codelocal_dashboard_chat_usage (
    event_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES codelocal_users(id) ON DELETE CASCADE,
    thread_id TEXT,
    model TEXT NOT NULL,
    protocol TEXT NOT NULL,
    input_tokens BIGINT NOT NULL CHECK(input_tokens >= 0),
    output_tokens BIGINT NOT NULL CHECK(output_tokens >= 0),
    total_tokens BIGINT NOT NULL CHECK(total_tokens >= 0),
    created_at BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_beta_chat_usage_user_time
    ON codelocal_dashboard_chat_usage(user_id,created_at DESC);
CREATE INDEX IF NOT EXISTS idx_beta_chat_usage_thread_time
    ON codelocal_dashboard_chat_usage(user_id,thread_id,created_at DESC);

COMMIT;
