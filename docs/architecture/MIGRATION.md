# CodeLocal beta — migration architecture and acceptance gates

## Mandate

Rebuild the **entire CodeLocal application**: Go backend/CLI/runtime to Rust,
and the frontend into a fresh Next.js + React + TypeScript App Router project.
Do not copy old business logic unreviewed or silently leave Go serving
production routes. Preserve `old-project/` solely for contract extraction,
tests, security comparisons and rollback. Do not deploy it as part of beta.

## Target boundaries

```text
AI clients (ChatGPT / Codex / Claude)
     ↕ MCP HTTPS / OAuth
Rust Cloud API + MCP Gateway (backend/)
     ↕ authenticated transport
Rust Device / Local Agent (cli/ + future crates/)
     ↕ explicit user authorization & approvals
Local projects / Git / terminal / LSP / tools
     ↕ only permitted, redacted intelligence
Project Brain / persistent state / services
     ↕ HTTPS
Next.js Control Plane (web/)
```

Rust implementation decisions: Tokio runtime, Axum HTTP API, strict
typed contracts, parameterized database access, explicit authorization
boundaries. Postgres/Redis/storage, queues and WebSocket transport must be
designed from observed legacy contracts before dependencies are finalized.

Next.js is a separate frontend consuming Rust-backed APIs; backend
authorization is mandatory for all sensitive operations. Do not
reimplement enforcement in React alone.

## Three migration cases

1. **Infrastructure/bootstrap (now).** Archive old tracked source and
   create independent Rust workspace + Next.js. Provide health/status
   endpoints and dev/build basics. Zero legacy production functionality
   is claimed.
2. **Parity migration.** Extract exact API/MCP/DB/session/CLI contracts
   from old-project; implement Rust equivalents with tests, compatibility
   adapters where necessary, data migrations and traceable coverage.
3. **Cutover.** Deploy isolated beta services and test multi-device
   workflows end-to-end. Compare against main, gate security, run data
   migrations with rollback, then enable controlled traffic only after
   acceptance.

## Modules and minimum acceptance requirements

| Domain | Required acceptance checks |
| --- | --- |
| Identity + auth | OAuth, sessions, tokens, CSRF, same-site, replay resistance |
| Cloud + tenancy | account isolation, RBAC, rate limits, validation |
| MCP gateway | discovery, request/response compatibility, streaming/errors |
| Local runtime | encrypted pairing, connect/reconnect, device ownership |
| Workspace/files | authorization, symlink escapes, binary/secret denial |
| Git/terminal | user approvals, command policy, audit and cancellation |
| Project Brain | persistence, LSP indexes, memory and knowledge parity |
| Computer/browser | permissions, device isolation, bounded controls |
| Extensions/plugins | discovery, invocation ACL, explicit approvals |
| Frontend | new design system, accessibility, responsive UI, page parity |
| CLI/releases | install/update/uninstall, cross-platform binary releases |
| Operations | observability, schema migration, zero-downtime rollback |

## Rules

- Keep `main` and live production deployments unchanged.
- Rust API routes are additive only until equivalent tests prove parity.
- Do not migrate customer plaintext, secrets or production data by copying
  local environment files into Git.
- Require green `cargo fmt --check`, `cargo test`, `cargo clippy`,
  Next.js lint/typecheck/build and API contract tests before promotion.
- A green health endpoint is **not** proof that identity, MCP or devices work.
- Document missing parity openly; no silent stubs that claim success.

## Current milestone

**Bootstrap started:** Git source archived; Rust HTTP and CLI entrypoints,
Next.js shell and API connection status created. Product-specific endpoints,
storage and all security-sensitive functionality are still pending.
