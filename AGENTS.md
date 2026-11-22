# CodeLocal beta repository instructions

**Approved direction (2026-11-22): preserve the existing Next.js frontend and rebuild the Go backend/CLI/local runtime in Rust.**

- `web/` is copied from `main` commit `365d372`. Treat its route tree, existing components, UI/UX, DTOs and user flows as **frozen parity contracts**, not a blank-slate design canvas. Avoid visual/UX redesign.
- `backend/` owns Rust cloud/server/API/OAuth/MCP/domain logic; `cli/` and future Rust crates own local runtime, CLI, controlled execution and OS bridges.
- `old-project/` is a read-only historical source snapshot to extract behavior, schema, tests and security contracts. Never deploy it or add production changes to it.
- Maintain current `/api/v1/*`, `/api/collective/*`, auth and consent POST, `/mcp`, `/client` and session/cookie/CSRF semantics. Do not weaken authorization or approval gates.
- Use real Rust implementations and state; never pretend new endpoints are production-ready with mocks or fake telemetry.
- For `web/`, run `npm run check --prefix web`; for Rust run fmt/test/clippy and targeted security/contract tests. For real parity, also run E2E against beta Rust services.
- Keep `main` and current production deployments unchanged until an approved cutover.

See `docs/architecture/MIGRATION.md` and `docs/architecture/FEATURE_PARITY_PLAN.md`.
