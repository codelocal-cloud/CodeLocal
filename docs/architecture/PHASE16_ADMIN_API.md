# Phase 16 — Rust administrator read API (2026-11-24)

Implemented the existing Next.js `GET /api/v1/admin` contract on isolated Rust beta. No changes to the Next.js UI, archived Go or production.

## Behavior and authorization

- Uses the authenticated browser session and its actual account, with an explicit server-side `CODELOCAL_BETA_ADMIN_EMAILS` allowlist. Anonymous callers receive 401, ordinary accounts receive 403. A request header or referral code cannot confer administrative access.
- Admin requests with the existing active session-risk flag receive 403 `reauthentication_required`. This is the existing beta risk-window mechanism, not a complete step-up authentication implementation.
- Queries PostgreSQL for real users, invitation totals, most recent non-revoked device activity and exact last recorded MCP event timestamp. Returns only the fields in the admin dashboard contract, not password hashes, credential material or raw sessions.
- Redis leases determine `runtimeActive` for non-revoked devices. No simulated `mcpActive` or `usingMcpNow`: live MCP invocation signals are still unported.
- A successful privileged read requires a persisted `admin.users_viewed` audit event (actor ID and row count), and the response is marked `Cache-Control: no-store`.
- Bounded read: at most 500 accounts and 3000 associated active device rows, matching the reused presence query guard. Larger catalogs return 503 rather than silently truncating the totals; server-side pagination remains pending.

## Acceptance

Local Rust workspace tests and Clippy pass (83 tests; one real-store integration test is run in the GitHub beta CI job). The extended integration test checks ordinary-user denial despite an asserted admin header, allowed admin read, live guest presence, referral count, lack of credential fields, historical MCP timestamp, revoked-device offline state, audit events, and denial after browser user-agent change. CI must pass for the new commit before claiming real-store acceptance.

This is the read-only administrator user overview, not full moderation, skills administration, OAuth, MCP execution, or production readiness.
