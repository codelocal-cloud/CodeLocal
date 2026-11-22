# CodeLocal beta — Phase 4 signed workspace sync (2026-11-22)

## Scope completed in Rust source

- CLI beta pairing now **requires** server-advertised `deviceSigning=true`, creates a fresh Ed25519 keypair per successful re-pair attempt, includes `devicePublicKey` in retry-safe pairing claims, and keeps the signing seed in the beta-only private credential file (`0600` Unix). It refuses to simulate signed pairing against older servers.
- `codelocal logout` signs a POST to `/api/client/auth/logout`, requiring a non-revoked device credential, Ed25519 verification and a fresh Redis nonce. The server revokes the credential; CLI deletes the local beta credential only after positive server acknowledgement. Local workspace grants are preserved; network/error responses do not falsely claim success.
- `codelocal sync` reads **only local grants** from the beta registry; sends workspace ID/name, not `localPath`. Requires explicit `CODELOCAL_BETA_API_URL` and a beta credential from the same origin; rejects missing/invalid local keys and mismatched account origins.
- `POST /api/client/workspaces/sync` matches the Go URL, requires a non-revoked matching credential, verifies request Ed25519 signature across method/URI/timestamp/nonce/body SHA-256, and uses Redis `SET NX EX` to consume nonce once per credential. Invalid signatures do not consume a nonce. Redis outage fails closed. This endpoint does **not** execute any local code or file operation.
- PostgreSQL update is transactional and holds a row lock on the device; its revocation state is checked again inside the transaction. Workspace metadata is replaced atomically (including stale entries); device timestamps are updated. Input is bounded, duplicate workspace IDs and forged managed/system-app flags are rejected.
- `GET /api/v1/workspaces` returns the existing Next.js `WorkspacesResource` DTO using real account-scoped metadata, hiding revoked devices. **All workspaces are explicitly offline** because WebSocket transport/presence is not implemented.
- `backend/migrations/003_beta_workspaces.sql` and `backend/compose.beta.yml` provide isolated schema/bootstrap for metadata (no local file paths). The store refuses to boot with missing schemas.

## Security / limitations

- The signature gate currently protects **only workspace metadata sync**. It is not yet reusable for a privileged terminal/filesystem/MCP operation; those routes remain unimplemented. The backend's email OTP/auth is also still waiting for real-store E2E.
- `POST /api/client/auth/check` remains a credential-only, read-only check per the archived Go semantics. It does not grant host operations.
- Not yet included: WebSocket session transport, live presence, device runtime daemon, scheduling/retry on reconnect, distributed cross-device routing, workspace revoke acknowledgement, OAuth/PKCE, Project Brain and all 14 MCP tools.
- Metadata sync intentionally returns empty `knowledge`, `portableSkills` and `runtimeSettings` rather than fabricated settings. This is a **subset of Go's full sync payload**, not full API parity.
- No production credentials, server, `main`, or legacy `old-project/` source were modified.

## Verification

- `cargo fmt --all -- --check`, `cargo test --workspace --offline`, `cargo clippy --workspace --all-targets --offline -- -D warnings` pass.
- Unit tests cover Ed25519 canonical/signature verification, body/query tampering, clock skew, bad nonce, workspace validation/duplicate input and local credential protection.
- **Not E2E verified:** Docker daemon still returns `EOF` when querying services; on this Mac `redis-server` is not installed. Isolated PostgreSQL/Redis startup, full signed HTTP/SQL execution, real emails and pairing through an actual browser remain unverified.
- The original Next.js test/build suite passed in Phase 3; this phase does not modify `web/`.

## Reproduce once Docker is healthy

```sh
docker compose -f backend/compose.beta.yml up -d --wait
export CODELOCAL_BETA_DATABASE_URL='postgres://codelocal_beta:local_beta_only_change_me@127.0.0.1:55432/codelocal_beta'
export CODELOCAL_BETA_REDIS_URL='redis://127.0.0.1:56379/0'
export CODELOCAL_BETA_SECURITY_BINDING_SECRET='<new random value of at least 32 characters>'
export CODELOCAL_BETA_PUBLIC_BASE_URL='http://127.0.0.1:3100'
export CODELOCAL_BETA_API_URL='http://127.0.0.1:8080'

# Another terminal: API, frontend and explicit browser pairing (account already required)
cargo run -p codelocal-backend
CODELOCAL_BACKEND_URL=http://127.0.0.1:8080 npm run dev --prefix web -- --port 3100
cargo run -p codelocal -- login
cargo run -p codelocal -- grant ./cli
cargo run -p codelocal -- sync
cargo run -p codelocal -- status

# Test suite with real isolated PG + Redis
cargo test -p codelocal-backend isolated_beta_login_account_logout_real_stores -- --ignored
```

Never point `CODELOCAL_BETA_*` at production or merge this migration into `main` before full staging E2E and rollback verification.
