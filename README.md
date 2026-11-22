# CodeLocal Rust beta — keep the existing Next.js frontend

This `beta` branch preserves the exact CodeLocal Next.js 16 / React 19 frontend from `main` commit `365d372`, while migrating the Go backend, MCP gateway and local client into Rust. The legacy source is frozen in `old-project/` (read-only reference). Production and `main` are not changed.

## Actual migration state

| Component | Result |
| --- | --- |
| `web/` | Original 52 Next.js page routes and UI preserved; full web check/build passes |
| `cli/` | Rust beta registry + Ed25519 device pairing (`login`), signed workspace metadata sync (`sync`) and signed credential revocation (`logout`); no remote host execution |
| `backend/` | Rust beta auth/session/OTP/password reset, device pairing and revocation, signed Ed25519 workspace sync with Redis anti-replay, and account-scoped workspace catalog. **Real-store end-to-end testing pending** |
| `backend/src/device_signing.rs` | Go-compatible Ed25519 verifier now enforced for workspace metadata sync, with Redis nonce replay protection; no privileged file/terminal/MCP endpoint |
| `backend/compose.beta.yml` | Isolated loopback PostgreSQL+Redis test stack (Docker daemon must be running) |
| `backend/migrations/` | Isolated beta user/audit, device/pairing and workspace metadata SQL. **Never applied to production** |
| `docs/architecture/` | Contract parity plan, migration waves and execution reports |

**This beta is NOT production-ready.** Workspace metadata synchronization now has a signed anti-replay gate, but the Cloud native-runtime WebSocket transport, full workspace authorization/revocation handshake, 14 MCP tool behaviors, Plugins, Project Brain, chat, knowledge, social and many other services are not yet ported. A route exists only if it implements a real slice of its domain. There is no fake authenticated state or fake device presence.

## Run the isolated beta stack

```sh
# Docker daemon must be running. This stack binds only to loopback:
docker compose -f backend/compose.beta.yml up -d --wait

# Export in your shell. Values below are ONLY local test credentials.
export CODELOCAL_BETA_DATABASE_URL='postgres://codelocal_beta:local_beta_only_change_me@127.0.0.1:55432/codelocal_beta'
export CODELOCAL_BETA_REDIS_URL='redis://127.0.0.1:56379/0'
export CODELOCAL_BETA_SECURITY_BINDING_SECRET='<set-unique-random-secret-at-least-32-characters>'
export CODELOCAL_BETA_PUBLIC_BASE_URL='http://127.0.0.1:3100'
export CODELOCAL_BETA_API_URL='http://127.0.0.1:8080'
# Optional only for delivery tests: set separate beta Resend API key/sender.
# export CODELOCAL_BETA_RESEND_API_KEY='<beta-Resend-key>'
# export CODELOCAL_BETA_EMAIL_FROM='CodeLocal Beta <beta@example.com>'
# Configure this explicitly to allow the bootstrap MMON referral:
# export CODELOCAL_BETA_ADMIN_EMAILS='admin@example.com'

# API on port 8080:
cargo run -p codelocal-backend

# Existing Next.js app on port 3100, in a separate terminal:
CODELOCAL_BACKEND_URL=http://127.0.0.1:8080 npm run dev --prefix web -- --port 3100

# Native Rust CLI interactive pairing (needs functioning API+DB and browser approval):
CODELOCAL_BETA_API_URL=http://127.0.0.1:8080 cargo run -p codelocal -- login
cargo run -p codelocal -- status
cargo run -p codelocal -- grant ./cli
cargo run -p codelocal -- sync  # signed metadata only; no paths or remote host execution
# When testing is finished: cargo run -p codelocal -- logout
```

A fresh Compose PostgreSQL volume executes `001_beta_auth.sql`, `002_beta_devices.sql`, and `003_beta_workspaces.sql` in order. Existing volumes do not automatically replay migrations. This is an **isolated local-only subset** of Go migrations and must never be applied to the existing production database. The beta auth store refuses non-loopback database hosts and databases not named with `beta`.

```sh
cargo fmt --all -- --check
cargo test --workspace --offline
cargo clippy --workspace --all-targets --offline -- -D warnings
npm run check --prefix web

# Real beta-store E2E (intentionally ignored without services):
cargo test -p codelocal-backend isolated_beta_login_account_logout_real_stores -- --ignored

# Stop isolated containers; volumes retained for inspection:
docker compose -f backend/compose.beta.yml down
```

**On the connected Mac, Docker Compose configuration validated, but the daemon still returned `EOF` when queried; real PG+Redis E2E has not run.** The Rust unit tests and frontend build do not replace this gate.

The service uses **beta-specific** environment variables for DB, Redis, security binding, public URL and Resend mail delivery to avoid accidentally using production credentials. Email OTP uses a 10-minute pending record, 6-digit code, hash, limited attempts and actual delivery; if mail is unconfigured it returns 503 rather than claiming email sent. Password reset increments `securityVersion` to invalidate older browser sessions.

Device pairing still does not enable privileged local operations. The signed request gate, user-device ownership check and replay protection protect metadata sync only; explicit workspace permission and remote runtime approval gates must be completed before exposing MCP/file/terminal execution.

## Release safety

Do not modify or deploy the frozen Go archive. Do not merge into `main`, deploy to Railway, publish npm CLI, migrate existing credentials, or use production database snapshots until the full compatibility and security gates pass. Consult [feature parity plan](docs/architecture/FEATURE_PARITY_PLAN.md), [architecture](docs/architecture/MIGRATION.md), [Phase 1–2 progress](docs/architecture/IMPLEMENTATION_PROGRESS.md) and [Phase 3 auth/devices report](docs/architecture/PHASE3_AUTH_DEVICES.md) and [Phase 4 signed workspaces](docs/architecture/PHASE4_SIGNED_WORKSPACES.md).
