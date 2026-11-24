# Phase 21A — Live runtime transport verification

The isolated PostgreSQL/Redis integration fixture now starts an actual Axum TCP listener on an ephemeral loopback port and exercises the signed WebSocket upgrade through a real Tungstenite client. It rejects an unregistered workspace during registration, verifies an accepted registration and Redis lease, rejects a repeated signed nonce with HTTP 409, answers the application heartbeat, updates the lease with empty and restored workspace sets, and confirms an unsupported message closes the socket and clears presence. The test uses only disposable credentials and fixture workspaces.

Local compilation, 96 unit tests, formatting and Clippy passed. The new live-store socket test is part of the intentionally ignored integration fixture run in GitHub Actions; it has not yet been executed against real services for this commit.

This remains presence-only transport. Remote file, terminal, agent and tool dispatch, reconnect hardening and full production migration are not covered by the test.
