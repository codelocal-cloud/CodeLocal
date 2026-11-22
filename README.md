# CodeLocal beta — keep Next.js, rewrite backend and local client in Rust

This branch `beta` starts from `main` commit `365d372`. **The existing Next.js web product is preserved**, not redesigned. Our migration replaces the Go Cloud/backend, MCP gateway, CLI and machine runtime with Rust **behind the current web/API contracts**.

| Directory | Ownership | Status |
| --- | --- | --- |
| `web/` | Existing CodeLocal Next.js 16 / React 19 product, copied unchanged from `main` | Source preserved; needs Rust backend |
| `backend/` | New Rust Axum/Tokio cloud API/MCP backend | CSRF, isolated PostgreSQL/Redis auth adapters, login/account/logout handlers (not yet database E2E verified); other services pending |
| `cli/` | New Rust CLI and future native machine runtime | Beta-isolated workspace grant/list/revoke/status; remote runtime pending |
| `old-project/` | Frozen complete Go+web source from `main`, for compatibility, security and migration reference | Archive only; never deploy |
| `docs/architecture/` | Parity inventory, contract requirements and phased migration | Active plan |

**Not production-ready.** Existing Next.js screens make requests against the historical API contract; almost all of those endpoints are **not yet implemented** in Rust. Login/account/logout handlers require separately provisioned **local isolated beta** PostgreSQL/Redis and are **not yet proven against live databases**. Registration/OTP, OAuth, Chat, Devices, MCP and Project Brain remain pending.

## Local development

```sh
# Terminal 1: Rust backend
cargo run -p codelocal-backend
# GET http://127.0.0.1:8080/healthz
# GET http://127.0.0.1:8080/api/v1/system/status
# GET http://127.0.0.1:8080/api/v1/auth/csrf

# Terminal 2: existing Next.js product
cd web
npm ci
CODELOCAL_BACKEND_URL=http://127.0.0.1:8080 npm run dev

# Terminal 3: Rust CLI workspace features (beta-only state)
cargo run -p codelocal -- --version
cargo run -p codelocal -- grant ./cli
cargo run -p codelocal -- workspaces
cargo run -p codelocal -- ungrant ./cli
```

`CODELOCAL_BACKEND_URL` is the **server-only** destination used by the existing Next.js proxy. The Rust backend will implement the existing `/api/v1/*` and authentication contracts incrementally. Do not put private credentials into `NEXT_PUBLIC_*` variables.

### Optional: isolated beta authentication test

**Never point these commands to an existing production database or Redis instance.** The beta Rust backend intentionally refuses non-loopback PostgreSQL/Redis, and PostgreSQL's database name must contain `beta`. To prepare a local test database:

```sh
# Provision a separate PostgreSQL database/account yourself (e.g. codelocal_beta).
# Only on the isolated beta database; migrations are never automatic:
psql -h 127.0.0.1 -U beta_user -d codelocal_beta -v ON_ERROR_STOP=1 -f backend/migrations/001_beta_auth.sql

# Export real values in your own terminal, never commit them to Git:
export CODELOCAL_BETA_DATABASE_URL='postgres://beta_user:<secret>@127.0.0.1:5432/codelocal_beta'
export CODELOCAL_BETA_REDIS_URL='redis://127.0.0.1:6379/15'
export CODELOCAL_BETA_SECURITY_BINDING_SECRET='<unique random secret longer than 32 chars>'
cargo run -p codelocal-backend

# Optional integration gate (writes and removes a single temporary beta test user):
cargo test -p codelocal-backend isolated_beta_login_account_logout_real_stores -- --ignored
```

The Redis URL should point to a dedicated test Redis or isolated database. Session keys are deliberately namespaced `codelocal:beta:session:*` rather than shared with the Go production runtime. For Go password parity, Rust verifies the exact scrypt parameters `N=16384, r=8, p=1, dkLen=64`. The first endpoint group is `POST /login`, `GET /api/v1/account` and `POST /logout`, all backed by real stores; incomplete/missing infrastructure returns an error rather than a fake successful sign-in. **Email signup/OTP and full security signal/IP rate-limit parity are not done**.

## Gates

- Rust: `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`
- Existing web: `npm run check --prefix web` (lint, TypeScript, UI tests and production build)
- Security and compatibility: OAuth/PKCE, cookie/CSRF, tenant and workspace boundaries, device pairing, local approvals, MCP tool schemas and error/streaming parity.
- Before deployment: real backend E2E, migration dry-run, multiple devices/AI clients, releases and rollback testing.

**Policy:** Keep `main`/production untouched; do not claim a feature is migrated until there is a real Rust implementation with test evidence.

See [Migration architecture](docs/architecture/MIGRATION.md), [Feature inventory and plan](docs/architecture/FEATURE_PARITY_PLAN.md) and [Phase 1 execution report](docs/architecture/IMPLEMENTATION_PROGRESS.md).
