# CodeLocal Rust beta — Phase 3 execution report

## Phase 3 — Email account flows, device pairing, signature contracts (2026-11-22)

**Implemented in Rust source, code-compiled and unit-tested, not yet real datastore E2E:**

- `backend/src/signup.rs`: Existing Next.js signup POST, 6-digit email OTP, 10-minute pending lifetime, code-hash scoping, 5-attempt lockout, invite-only referral checks, verified-user creation only after OTP, browser session after confirmation, context GET and redirect/error response compatibility.
- `backend/src/password_reset.rs`: Forgot-password request, reset context, OTP validation and finalize, authenticated password change; password update increments security version, invalidating previous sessions. Same Next.js forms and paths.
- `backend/src/mailer.rs`: Resend API adapter, TLS-only endpoint, short timeout, idempotency, separate beta-only credentials; no pretending email was sent when credentials or provider unavailable.
- `backend/src/store_auth_flows.rs`: Beta-namespace Redis rate limits, pending signup/reset storage/attempts/locks; invite validation, audited user creation and password-version update in PostgreSQL.
- `backend/src/pair.rs`, `backend/src/store_devices.rs`: Device pairing start/approve/claim, signed-credential fields and hash validation, PG transaction/row lock/retry-safe claim, device credential lookup, active/revoked status and user-scoped management.
- `backend/src/devices.rs`: Real current-account device list matching Next.js DTO and authenticated CSRF-protected device revoke. Online status deliberately remains **offline** until the runtime presence system is ported; never send fabricated online state.
- `backend/src/device_signing.rs`: Go-compatible Ed25519 request canonicalization and verification, clock-skew validation, modified body/query rejection; **not wired to privileged endpoints** until replay nonce and local workspace permissions are fully ported.
- `backend/src/public_url.rs`: Beta-only public-origin validation (HTTPS or loopback HTTP), no userinfo/path/query. Separate beta secrets and beta-only URLs.
- `cli/src/pair.rs`: Rust CLI `login`/`pair` and `login --force`: explicit beta API origin and handshake validation before pairing, 10-minute browser approval loop with retry-safe claim, verify new credentials using `POST /api/client/auth/check`, atomic private (0600) beta-only file storage, stable beta device ID, and symlink refusal. This does not implement a daemon or privileged remote operations.
- `backend/migrations/002_beta_devices.sql`: Dedicated beta pairing/device schema, applied by Compose on fresh isolated PG volume, never production.
- `backend/compose.beta.yml`: Loopback-only Postgres 17 and Redis 7, fresh test database named `codelocal_beta`; Docker Compose config verified.

### Verification and blockers

- Last compiled result: Rust `cargo test --workspace --offline` 39 passed (9 CLI + 30 backend), 1 ignored real-store integration; Rust format and Clippy pass. No DB-backed pairing run yet.
- Preserved Next.js check/build was verified previously; no `web/` modifications in this Phase.
- Attempted to boot isolated Docker Compose, but Docker daemon/image pull returned `unable to get image 'redis:7-alpine': unexpected end of JSON input`. **PostgreSQL/Redis beta E2E was not executed**. Do not claim DB-integrated signup/login/OTP/pairing works until rerun with a healthy daemon.
- No real Resend beta credentials were provided, so no email was sent; Resend adapter has not been integration tested.
- No production DB, user credentials, deployed Cloud, npm publication, or `main` branch was touched.

### Remaining compatibility/security work before production

1. Isolated beta DB/Redis E2E (including auth, OTP, paired credential retry, device revoke, session invalidation). Test schema/migration compatibility against anonymized data copy.
2. OAuth 2.1/OIDC PKCE, MCP gateway and 14 compact MCP tool/action schemas with authorization, abuse throttles, stream compatibility and tenant boundaries.
3. Rust native CLI pairing/login persistence, background local runtime, signed authenticated device transport, nonce replay protection, WebSocket state and workspace synchronization.
4. Full project agent, Project Brain and code graph, MCP plugins/skills, chat, blog/forum/media/admin and all existing Next.js API endpoints.
5. End-to-end security verification on Mac, Windows, Linux, multiple devices and revocation flows; staging + rollback.

This Phase is a **functional subset of the overall migration**, not final CodeLocal 100% parity.
