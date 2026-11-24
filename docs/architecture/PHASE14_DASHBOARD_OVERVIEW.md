# Phase 14 — Account-scoped Rust Dashboard Overview (2026-11-24)

## Implemented

`GET /api/v1/dashboard/overview` now provides the exact core DTO required by the **unchanged** `web/src/lib/contracts/dashboard.ts` and the `/dashboard` overview page. It is protected by the existing beta session, bound browser device cookie, user agent and account lookup.

- `user.email` and `user.isAdmin` come from the authenticated account and the explicitly configured `CODELOCAL_BETA_ADMIN_EMAILS`.
- `devices.paired` and `devices.online` use real account-scoped PostgreSQL device rows plus live Redis runtime presence. Revoked devices are never counted as paired or online, even if an old Redis presence lease has not expired yet.
- `workspaces.total/active/sleeping/offline/recent` use account-scoped PostgreSQL workspace rows and *only* runtime presence from that account's non-revoked devices. A workspace is active only if its exact ID is present in the device's current Redis lease. At most five recent rows are returned (most recent first), with name, ID, device label and real last-seen timestamp. There is no simulated "sleeping" state; `sleeping:0` reflects that this native-runtime state is not yet implemented in Rust.
- `usage.available` and `last24h/last30d/allTime` derive from real account-scoped PostgreSQL aggregated MCP events (Phase 13). `available:true` means the beta telemetry database query succeeded, **not** that unported Cloud MCP/Chat producers have been enabled.
- HTTP 401 for unauthenticated access, 503 for datastore/presence failures and a strict `Cache-Control: no-store` on successful responses. No database credentials, pairing secrets, lease IDs, project paths or device public keys are returned.
- The current workspace catalog is bounded by 1000 rows; an overview with 1000 rows is refused with 503 instead of claiming an inaccurate complete count. Full pagination and large-account aggregates remain pending.

## Real-store CI acceptance

The existing disposable **PostgreSQL 17 + Redis 7** GitHub Actions job now validates:
1. Account with an authenticated session and no devices: real zero counters, no fabricated Skills.
2. Persisted, idempotent MCP usage is reflected in Overview.
3. Pairing + workspace sync + Redis runtime presence: online device, active workspace and recent project.
4. Lease cleanup: same workspace becomes offline, device online count becomes zero.
5. Device revocation + workspace removal: revoked device is excluded without losing account-scoped usage.
6. Anonymous GET returns 401. Unit tests separately verify DTO fields, absence of credentials and that missing Redis presence is not mistaken for active.

The **Skills** section of Go's Overview is deliberately omitted: the current Next.js type guard and UI do not depend on it and Rust has not ported the Go skills engine. Sending an empty "working" skills catalog would be misleading. Full Go endpoint parity requires porting Skills, sleeping runtime state, workspace catalog pagination and telemetry producers.

No edits to `web/`, frozen `old-project/`, GitHub `main`, or production infrastructure.
