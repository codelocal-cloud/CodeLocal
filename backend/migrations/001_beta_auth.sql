-- CodeLocal Rust BETA-ONLY auth bootstrap.
-- NEVER run on production. Use a separate database named *beta*.
-- Minimal subset of Go codelocal_schema_migrations v1, v6, v41, v42.
-- This is NOT the complete Go migration plan and must not be used to migrate existing data.

BEGIN;

CREATE TABLE IF NOT EXISTS codelocal_users (
    id TEXT PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    password_salt TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    referral_code TEXT NOT NULL,
    referred_by_code TEXT,
    password_changed_at BIGINT NOT NULL DEFAULT 0,
    security_version BIGINT NOT NULL DEFAULT 1
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_codelocal_users_referral_code_ci
    ON codelocal_users ((UPPER(referral_code)));

CREATE TABLE IF NOT EXISTS codelocal_audit_logs (
    id TEXT PRIMARY KEY,
    user_id TEXT REFERENCES codelocal_users(id) ON DELETE SET NULL,
    event TEXT NOT NULL,
    device_id TEXT,
    workspace_id TEXT,
    detail JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_codelocal_audit_user
    ON codelocal_audit_logs(user_id, created_at DESC);

COMMIT;
