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

**Do not interpret the 13 passing tests as feature parity.** Only two narrowly scoped existing capabilities have initial implementation.
