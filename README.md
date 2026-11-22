# CodeLocal beta — keep Next.js, rewrite backend and local client in Rust

This branch `beta` starts from `main` commit `365d372`. **The existing Next.js web product is preserved**, not redesigned. Our migration replaces the Go Cloud/backend, MCP gateway, CLI and machine runtime with Rust **behind the current web/API contracts**.

| Directory | Ownership | Status |
| --- | --- | --- |
| `web/` | Existing CodeLocal Next.js 16 / React 19 product, copied unchanged from `main` | Source preserved; needs Rust backend |
| `backend/` | New Rust Axum/Tokio cloud API/MCP backend | Health/status + real CSRF GET contract; other auth/services pending |
| `cli/` | New Rust CLI and future native machine runtime | Beta-isolated workspace grant/list/revoke/status; remote runtime pending |
| `old-project/` | Frozen complete Go+web source from `main`, for compatibility, security and migration reference | Archive only; never deploy |
| `docs/architecture/` | Parity inventory, contract requirements and phased migration | Active plan |

**Not production-ready.** Existing Next.js screens make requests against the historical API contract; almost all of those endpoints are **not yet implemented** in Rust. A successful Next.js build or Rust health check does **not** mean login, Chat, Devices, MCP, Project Brain or other features work.

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

## Gates

- Rust: `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`
- Existing web: `npm run check --prefix web` (lint, TypeScript, UI tests and production build)
- Security and compatibility: OAuth/PKCE, cookie/CSRF, tenant and workspace boundaries, device pairing, local approvals, MCP tool schemas and error/streaming parity.
- Before deployment: real backend E2E, migration dry-run, multiple devices/AI clients, releases and rollback testing.

**Policy:** Keep `main`/production untouched; do not claim a feature is migrated until there is a real Rust implementation with test evidence.

See [Migration architecture](docs/architecture/MIGRATION.md), [Feature inventory and plan](docs/architecture/FEATURE_PARITY_PLAN.md) and [Phase 1 execution report](docs/architecture/IMPLEMENTATION_PROGRESS.md).
