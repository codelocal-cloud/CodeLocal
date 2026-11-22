# CodeLocal beta — Phase 1 execution report

Date: 2026-11-22. Source baseline: `main` commit `365d372`.
Current milestone: **started P0 migration**, not full feature parity.

## Implemented in Rust

### CLI native workspace registry (`cli/src/workspace.rs`)

- Commands: `codelocal .`, `codelocal <directory>`, `grant`/`add`, `ungrant`/`remove`, `workspaces`, `status`, `--help` and `--version`.
- Same workspace JSON keys and ID algorithm as Go: `workspaceId`, `workspaceName`, `localPath`, `grantedAt`, optional flags, `<name>-<sha256(canonicalPath)[:10]>`.
- Grants require an existing canonical directory and reject whole home/root and known credential/config paths. Symlink targets are canonicalized before policy validation.
- State stored atomically and privately (Unix 0700 dir / 0600 files) with an exclusive beta lock and bounded lock wait; grant/ungrant never change project contents.
- **Isolated state by default:** `~/.codelocal-beta/workspaces.json`. Override with `CODELOCAL_BETA_STATE_DIR`. This deliberately **does not** migrate or overwrite the Go runtime's `~/.codelocal/workspaces.json` until a full compatibility/data-migration review.
- `status` explicitly reports that remote runtime, pairing and MCP remain unimplemented. The CLI intentionally exits with an error for unfinished commands.
- No device startup, no Cloud sync, no sensitive action execution yet.

### Cloud API form-security contract (`backend/src/auth.rs`)

- Real `GET /api/v1/auth/csrf` for unchanged Next.js forms.
- 32 cryptographically generated random bytes, unpadded base64url 43-character token, Go-compatible `{"csrf":"..."}`.
- Reuses a syntactically valid existing `codelocal_csrf` cookie; replaces missing/invalid cookies.
- Sets `HttpOnly`, `Path=/`, `SameSite=Lax`, `Max-Age=2592000`, `Cache-Control: no-store`, and `Secure` under configured HTTPS/production environment.
- No fake sessions or authorization. Login/signup/verification, CSRF POST verification, OAuth, persistence and RBAC remain pending.

## Verification

- Rust `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`.
- 6 workspace registry tests + 3 CSRF contract tests + 4 Rust foundation tests (13 total).
- CLI smoke test in a disposable `target/cli-beta-smoke` state: grant, list, status, revoke, list; success.
- Live HTTP smoke test: Rust backend `GET /api/v1/auth/csrf` responded `200`, expected JSON and cookie.
- Existing Next.js dev server rewrote `GET /api/v1/auth/csrf` to Rust and received `200` with the same contract.
- Existing Next.js code was not changed in this slice; previous full `npm run check --prefix web` passed (one preexisting ESLint image warning).
- Local test services terminated. No Git push, no production deployment, no existing user credentials touched.

## Next P0 tasks (not completed)

1. Match Go `internal/cloud` persistence schema and establish Rust PostgreSQL migrations/access with rollback and tenant isolation.
2. Implement real email signup/login/password verification, user/session store, password scrypt compatibility, CSRF checks for POST, cookie rotation, security-version checks, audit and rate limits.
3. Implement OAuth PKCE, `/mcp` authenticated routing and device pairing/signing with compatible service-level errors.
4. Implement background Rust local runtime with cloud transport, CLI `login`/`status`/`stop`, device/workspace sync, reconnection and revocation.
5. Implement restricted local file operations, approvals, Git, terminal and other public MCP actions with security negative tests.
6. Run E2E on separate beta Cloud/database and at least two clients/devices before any migration of old local state or cutover.

**Historical Phase 1 note:** 13 passing tests covered only CSRF and local workspace registry. More scoped Rust auth code follows below; it does not establish complete feature parity.

## Phase 2 — Rust beta authentication data plane (2026-11-22)

**Implemented source, not yet production-ready and not yet end-to-end validated against running PostgreSQL + Redis:**

- `backend/src/store.rs` — explicit opt-in, isolated PostgreSQL user repository matching Go's `codelocal_users` fields, Redis session JSON with `securityVersion`, revoke, 30-day TTL, atomic 12-attempt/10-minute account throttle. Authenticated state is prefixed `codelocal:beta:session:*` and cannot read Go production sessions.
- `backend/migrations/001_beta_auth.sql` — reviewed manual PostgreSQL bootstrap for **beta-only** user and audit tables, using Go-compatible column names. Startup checks existing table columns and does not run migrations or create users silently.
- `backend/src/password.rs` — Go-compatible scrypt verification (`N=16384,r=8,p=1,64 bytes`) with constant-time comparison; cross-language deterministic golden fixture.
- `backend/src/auth_session.rs` — `POST /login`, `GET /api/v1/account`, `POST /logout`; uses real beta stores only, pre-auth cookie/Form CSRF equality, session-bound logout CSRF, unguessable session tokens, same-origin redirects, device/agent binding, security-version checks, credential redaction and no fake authenticated responses.
- `backend/src/main.rs` starts the API with a verified optional beta store; unconfigured and partial-store setups fail closed. Only localhost PostgreSQL/Redis URLs are permitted until verified TLS support is added, and database names must contain `beta`.
- `backend/src/store.rs` contains an **explicitly ignored** real-store integration test that seeds/removes a temporary user, performs CSRF→login→account→logout and verifies session revocation. Run only with separate local beta PG/Redis services provisioned.
- No Next.js UI source or production Go data modified.

### Explicit limits / unresolved gates

1. Live PostgreSQL and Redis integration test has **not** been executed on connected Mac because a vetted disposable DB/Redis stack is not running/provisioned for this task.
2. Auth still lacks email signup/OTP, password reset, OAuth, public proxy cookie/security integration, per-IP rate limits and verified network-prefix binding. Account throttle and device/agent hashes exist, but **security parity is incomplete**.
3. Existing Go Redis sessions are intentionally not shared in beta. Production cookie/session and credential migration require a separate compatibility and security gate.
4. Admin email selection uses explicit `CODELOCAL_BETA_ADMIN_EMAILS` (no automatic privileged default); full RBAC/tenancy and session risk handling still require review.
5. Rust API does not yet claim general backend readiness. Do not deploy beta to a public endpoint or put beta auth behind a production domain.

Next: fully isolated Postgres/Redis E2E; signup verification + scrypt-compatible password reset; audited OAuth/PKCE and device pairing; remaining 14 MCP tools with permission boundaries.
