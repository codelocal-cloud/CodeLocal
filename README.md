# CodeLocal Rust beta — keep the existing Next.js frontend

This `beta` branch preserves the exact CodeLocal Next.js 16 / React 19 frontend from `main` commit `365d372`, while migrating the Go backend, MCP gateway and local client into Rust. The legacy source is frozen in `old-project/` (read-only reference). Production and `main` are not changed.

## Actual migration state

| Component | Result |
| --- | --- |
| `web/` | Original 52 Next.js page routes and UI preserved; full web check/build passes |
| `cli/` | Rust beta registry, Ed25519 pairing, signed workspace sync/logout, bounded local file read/search/edit, human-approved, one-use local MCP stdio read/search/batch read (5 tools) and foreground signed WebSocket device presence; no remote host execution |
| `backend/` | Rust beta auth/session/OTP password reset, device pairing/revocation, signed workspace sync, presence WebSocket, PostgreSQL Usage API, Overview and Invite/Referral. **Overview real-store CI passed on 8ddc87c; Invite E2E pending its own CI run** |
| `backend/src/device_signing.rs` | Go-compatible Ed25519 verifier and one-time Redis nonce now gate workspace sync and runtime WebSocket upgrade; no privileged file/terminal/MCP endpoint |
| `backend/compose.beta.yml` | Isolated loopback PostgreSQL+Redis test stack (Docker daemon must be running) |
| `backend/migrations/` | Isolated beta auth, device/pairing, workspace and usage SQL (001–004), never applied to production |
| `docs/architecture/` | Contract parity plan, migration waves and execution reports |

**This beta is NOT production-ready.** Signed workspace metadata sync and a **presence-only** WebSocket runtime are implemented in Rust; remote workspace activation, privileged command dispatch, full revocation protocol, 14 MCP tool behaviors, Plugins, Project Brain, chat, knowledge, social and many other services are not yet ported. A route exists only if it implements a real slice of its domain. No authenticated state or device presence is fabricated.

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
cargo run -p codelocal -- workspaces # copy the workspace ID
cargo run -p codelocal -- local-read <workspace-id> src/lib.rs 1 10
cargo run -p codelocal -- local-search-files <workspace-id> '.rs' 20
cargo run -p codelocal -- local-search-text <workspace-id> 'LocalReader' 20
# Use the file's SHA-256 from local-info before making a local-only edit:
cargo run -p codelocal -- local-edit <workspace-id> src/lib.rs <expected-sha256> 'old text' 'new text'
# Start read-only MCP stdio; file reads/search now REQUIRE human approval:
cargo run -p codelocal -- mcp
# In another terminal, review exact request/tool/arguments before granting it once:
cargo run -p codelocal -- approvals
cargo run -p codelocal -- approvals approve <approval-id>
# Or reject: cargo run -p codelocal -- approvals deny <approval-id>
cargo run -p codelocal -- sync  # signed metadata only; no paths or remote host execution
cargo run -p codelocal -- runtime --once # one signed WebSocket registration check
cargo run -p codelocal -- runtime        # foreground presence + heartbeat; Ctrl+C to stop
# When testing is finished: cargo run -p codelocal -- logout
```

A fresh Compose PostgreSQL volume executes `001_beta_auth.sql`, `002_beta_devices.sql`, `003_beta_workspaces.sql` and `004_beta_usage.sql` in order. Existing volumes do not automatically replay migrations. This is an **isolated local-only subset** of Go migrations and must never be applied to the existing production database. The beta auth store refuses non-loopback database hosts and databases not named with `beta`.

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

**On the connected Mac, Docker still returns `EOF`, so real PG+Redis E2E is not runnable locally.** GitHub Actions `beta` provisions disposable PostgreSQL 17/Redis 7; its full Rust/Web/real-store E2E passed on commit `ae18c36`, including account-scoped Usage API. New domains require CI success on their own commit.

The service uses **beta-specific** environment variables for DB, Redis, security binding, public URL and Resend mail delivery to avoid accidentally using production credentials. Email OTP uses a 10-minute pending record, 6-digit code, hash, limited attempts and actual delivery; if mail is unconfigured it returns 503 rather than claiming email sent. Password reset increments `securityVersion` to invalidate older browser sessions.

Device pairing still does not enable privileged local operations. Signed request authentication, user-device ownership and Redis replay prevention protect workspace metadata sync and WebSocket presence only; explicit per-action approval and authorization gates must be completed before exposing remote MCP/file/terminal execution.

## Release safety

Do not modify or deploy the frozen Go archive. Do not merge into `main`, deploy to Railway, publish npm CLI, migrate existing credentials, or use production database snapshots until the full compatibility and security gates pass. Consult [feature parity plan](docs/architecture/FEATURE_PARITY_PLAN.md), [architecture](docs/architecture/MIGRATION.md), [Phase 1–2 progress](docs/architecture/IMPLEMENTATION_PROGRESS.md) and [Phase 3 auth/devices report](docs/architecture/PHASE3_AUTH_DEVICES.md) and [Phase 4 signed workspaces](docs/architecture/PHASE4_SIGNED_WORKSPACES.md) and [Phase 5 local file reads](docs/architecture/PHASE5_LOCAL_READ.md) and [Phase 6 local search](docs/architecture/PHASE6_LOCAL_SEARCH.md) and [Phase 7 local edit](docs/architecture/PHASE7_LOCAL_EDIT.md) and [Phase 8 local MCP stdio](docs/architecture/PHASE8_LOCAL_MCP_STDIO.md) and [Phase 9 signed runtime presence](docs/architecture/PHASE9_RUNTIME_PRESENCE.md) and [Phase 10 local MCP approvals](docs/architecture/PHASE10_LOCAL_MCP_APPROVALS.md) and [Phase 11 MCP batch read](docs/architecture/PHASE11_MCP_BATCH_READ.md) and [Phase 12 beta database CI](docs/architecture/PHASE12_BETA_CI_E2E.md) and [Phase 13 Usage API](docs/architecture/PHASE13_USAGE_API.md) and [Phase 14 Dashboard Overview](docs/architecture/PHASE14_DASHBOARD_OVERVIEW.md) and [Phase 15 Invite API](docs/architecture/PHASE15_INVITE_API.md).
