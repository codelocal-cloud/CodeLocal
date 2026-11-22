# Phase 9 — Signed Rust runtime WebSocket presence (2026-11-22)

## Shipped code scope

This is an **isolated beta runtime-presence milestone**, not production-ready remote execution. The existing Next.js user interface is unchanged.

- Backend Rust now provides `GET /api/client/runtime/ws`. Upgrade is granted only after a valid, non-revoked beta device credential, Ed25519 signature covering GET, the exact path, timestamp, fresh nonce and empty request body. Redis consumes each nonce atomically before upgrade.
- Device registers with the archived Go-compatible `runtime_register` envelope: `credentialId`, `credentialSecret`, `deviceId`, `clientVersion` and `workspaceIds`. The server requires both credential and device identity match and checks that workspace IDs are present in the **same user/device's** PostgreSQL workspace catalog.
- Redis manages a 45-second runtime presence lease per credential with a cryptographically random per-connection lease ID; stale connections cannot refresh or delete a newer connection's presence. Redis failures fail closed. Only `runtime_pong` and `runtime_sync` renew a lease, after the server rechecks revocation and workspace permissions; unsupported event/activation messages close the socket and execute nothing.
- Server sends `runtime_registered` and `runtime_ping`. A missed 35-second heartbeat, revoked credential, bad workspace ID, invalid message or disconnected socket clears/lets expire the presence lease.
- Dashboard `GET /api/v1/devices` and `GET /api/v1/workspaces` use real Redis presence. Only authorized registered workspace IDs are marked `active`/online. A Redis outage returns an error rather than inventing offline/online state. Revoked devices remain revoked even if a stale TTL lease exists.
- CLI Rust `codelocal` or `codelocal runtime` runs the signed WebSocket presence client in foreground; `codelocal runtime --once` tests a single connection. CLI uses its **beta-only** private Ed25519 key and device credential, syncs workspace IDs/names first over the signed HTTP endpoint, reconnects with bounded exponential backoff, and responds to runtime ping. Ctrl+C stops the foreground process.

## Verify

```sh
# Low-footprint test/debug options are useful on disk-constrained Macs
CARGO_INCREMENTAL=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --workspace --offline
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 cargo clippy --workspace --all-targets --offline -- -D warnings
cargo fmt --all -- --check
npm run check --prefix web
```

A real-store end-to-end test has **not run** because the connected Mac's Docker daemon was unavailable. Unit tests cover signed headers/canonicalization, registration validation, workspace ID sanitization, stale lease payload rejection and fail-closed malformed upgrades. Actual Redis lease CAS, WebSocket handshake, reconnection and dashboard transitions require a running isolated beta PostgreSQL/Redis stack and at least two paired devices.

The Mac originally ran out of free disk space while compiling new WebSocket dependencies. Only generated Cargo package build artifacts were cleaned using `cargo clean -p codelocal` and `cargo clean -p codelocal-backend`; source code, Git state, runtime identities and production assets were not removed.

## Hard boundaries and next gates

- **No remote file/terminal/MCP execution:** the websocket is limited to presence and workspace registration; command/event/activation payloads are rejected. Four local read-only MCP tools still operate only through explicitly launched local stdio.
- This does not implement Go's activation queues, revocation acknowledgement, distributed event delivery, live multi-client concurrency tests, approval tokens, privileged actions, or native service/installer.
- Signed-request replay prevention has Redis TTL but E2E has not confirmed cross-instance behavior; verify under load and with an explicit revocation scenario.
- The client currently requires metadata synchronization to succeed before connecting. Long-running re-connect and online/offline UI flows require UAT with real stores.
- Remaining: remote MCP gateway + action approval, full user-facing APIs, Project Brain, plugins, mobile and cross-platform runtime/installer, OAuth/PKCE, and staging rollout/rollback gates.

Do not merge/deploy to `main` before the complete feature parity and security gates are met. Development changes should be committed and pushed incrementally to GitHub `beta` after local checks pass.
