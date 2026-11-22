# CodeLocal beta — Rust backend and local client migration, preserving Next.js

## Approved direction (2026-11-22)

**Do not rewrite the frontend.** Retain `web/` from `main` commit `365d372` with its existing routes, components, responsive layout, forms, navigation and Next.js App Router configuration. Only change frontend code if required for a demonstrated backend compatibility issue. Preserve external API contracts so frontend changes are minimal.

Rewrite the following **in Rust**:

1. Cloud/server: identity, API, persistence, OAuth/OIDC, MCP gateway, device/workspace routing, usage, plugin/skill/content services.
2. Local client/runtime: CLI, device login/pairing, persistent agent, filesystem, Git, guarded terminal, local MCP hub, browser/computer orchestration.
3. System integrations: DB migrations, WebSocket transports, permissions, OS-native bridges and cross-platform releases.

The full old source, including the original web app, remains in `old-project/` for reference and rollback. Do not serve or ship old Go code from `beta`. Continue operating `main` in its existing deployment until cutover is explicitly approved.

## Architecture and security authorities

```text
ChatGPT / Codex / Claude / other MCP clients
          ↕ HTTPS OAuth + MCP (contract preserved)
Rust Cloud API & MCP Gateway (backend/)
          ↕ signed/authenticated device transport
Rust CLI + local native runtime (cli/ + future crates/)
          ↕ explicit workspace access and approvals
Local Git, files, processes, LSP, browser/computer bridges

Existing Next.js web/ (preserved)
          ↕ same-origin /api/v1/* and mutations
Rust Cloud API (source of truth for auth, data, policy)
```

Next.js owns presentation, not authorization, sessions or business rules. Preserve the public URL namespace, status codes, DTOs, cookie/CSRF semantics, OAuth and MCP tool definitions. Existing `web/next.config.ts` proxies `/api/v1/*` and marked mutations to `CODELOCAL_BACKEND_URL`. In beta, point this env var to the Rust API. **Most routes are not yet implemented**, so runtime E2E is expected to fail until the corresponding Rust module is ported.

Security gates: tenant isolation, credential redaction, PKCE, session rotation, CSRF/rate limits, device ownership, workspace path/symlink confinement, non-bypassable approvals, audit and explicit revocation. Do not falsely describe guarded host-shell policy as OS sandboxing.

## Three cases (BA)

- **A — selected: keep existing UI; rebuild backend and local client.** Use unchanged Next components, route tree and user flows; implement Rust behind matching contracts.
- **B — deferred: targeted frontend bug fixes.** After backend parity, address proven breakages without changing information architecture.
- **C — deferred: UI/product redesign.** Separate future design proposal, with explicit approval.

## Execution waves

| Wave | Concrete implementation | Gate |
| --- | --- | --- |
| 0 (now) | Restore `web/` byte-for-byte from `main`, capture 52 Next page routes, API/MCP/CLI baseline and CI checks | `git diff main -- web` empty; `npm run check --prefix web` |
| 1 | Rust persistence, identity, signup/login/verify/reset, sessions, CSRF, OAuth/OIDC, account scope | Existing login/authorize forms work with real Rust; negative security tests |
| 2 | Rust device registration/pairing, signed local transport, presence, workspaces, CLI grant/list/status | Existing Devices/Projects pages and CLI communicate with Rust |
| 3 | Rust `/mcp`, 14 compact tool schemas/actions, local runtime filesystem/Git/terminal/process/approval | Existing ChatGPT/Codex/Claude connections pass compatibility tests |
| 4 | Rust Project Brain, code graph/LSP, context, skills, agents, browser/computer, extension hub/Plugins | Real execution, durable evidence and security regression |
| 5 | Rust chat/streaming/providers, media/shots, blog/forum/social, admin, usage, Penpot/SSO, settings | Full existing Next.js feature journeys against Rust |
| 6 | npm/native client release, Rust cloud staging, DB migration dry-run, reliability/performance, canary and rollback | E2E parity, cross-platform verification, explicit go/no-go |

**Implementation order is driven by dependencies:** auth before private APIs; pairing before remote local execution; permission framework before file/terminal/browser actions. Preserve user data with explicit schema transformations and verified rollback; never commit secrets/production snapshots.

## Per-feature definition of done

- Current Next UI and routes still render with authentic data; no synthetic success or fake telemetry.
- Rust endpoints exactly satisfy the old request/response/error/pagination/streaming contract or an explicit compatibility adapter exists.
- Authorization/security tests include malicious and cross-account requests, CSRF rejection, bad pairing, revoked devices, path escape, unsafe command approvals.
- CLI commands, version/help and exit semantics match current release where applicable; MCP names, schemas and tool errors are compatible.
- Web lint/typecheck/UI tests/build, Rust fmt/test/clippy, integration/E2E and deploy rollback are green.
- Matrix in `FEATURE_PARITY_PLAN.md` links each source feature with actual tests and implementation evidence.

## Current status

- Preserved the existing `web/` source (52 page routes) from `main` on `beta`.
- New Rust backend has only health/bootstrap status routes and the Rust CLI is a starter. **Business functionality is not migrated yet.**
- Live `main` and production remain untouched. Do not push/deploy until separately requested.
