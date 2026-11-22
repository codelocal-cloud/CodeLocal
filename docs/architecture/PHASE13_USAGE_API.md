# Phase 13 — Rust account-scoped Usage API (2026-11-22)

## Scope

Ported **`GET /api/v1/usage`** to the Rust beta backend without changing the original Next.js UI or its `web/src/lib/contracts/usage.ts` validator.

- Requires an authenticated Rust beta browser session with the same device and User-Agent binding as the account API. Unauthenticated calls return **401**, store outages return **503** with no fake usage response, and successful replies set `Cache-Control: no-store`.
- Returns the legacy Go DTO: `estimated:true`, `scope:"MCP payload only; not full AI model/provider billing tokens"`, `last1h/last24h/last30d/allTime` estimated MCP calls/tokens, plus `mcp.source:"payload_estimated"` and `webChat.source:"provider_reported"` with separate provider-reported token windows.
- Only aggregates **real PostgreSQL events belonging to the currently authenticated user**. An empty window is zero **recorded** events, not synthetic usage or a claim that unported model services were measured.
- Added isolated `004_beta_usage.sql`: Go-compatible hour-bucket MCP usage table and provider-reported chat usage table; a beta-only event ID table prevents replays from double-counting MCP rollups. The migration is provisioned in local Compose on a **fresh** beta PG volume, and applied by GitHub Actions CI after `001..003`.
- Internal Rust `record_mcp_usage` and `record_chat_usage` operations validate metrics and event identity, write idempotently into PostgreSQL and are not exposed as public mutation routes. **They are only called by beta integration tests now.** Actual MCP Cloud and AI Chat provider event producers are not yet migrated, so real users won't see their unported historical activity in this isolated beta database.
- Usage summaries use bucket-aligned MCP estimates and timestamped exact provider chat events, matching the Go semantics. No production DB or source code is read.

## Verification

- Local Rust fmt, unit tests and Clippy `-D warnings` pass.
- Tests check anonymous 401, DTO shape against Next.js, safe time bucketing and event input validation.
- The GitHub Actions **real PostgreSQL 17 + Redis 7** E2E was extended to cover an initially empty user, one persisted MCP usage event with a duplicate retry, one idempotent provider chat event, authenticated HTTP GET usage DTO, per-user isolation and session-based security. This **must pass on the pushed commit** to be considered DB-integrated complete.
- Next.js is preserved; no files in `web/`, `old-project/`, `main` or production were changed.

## Pending full Go parity

Port the actual remote MCP tool invocation producer and provider Chat streaming usage producer to call these trusted recording operations; usage does not include tokens or billing figures missing from those systems. Full usage/dashboard cross-domain parity and migration from Go production data are not complete.

Do **not** treat passing tests for this one API as completion of CodeLocal's full Rust migration.
