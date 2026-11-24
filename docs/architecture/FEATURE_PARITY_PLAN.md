# CodeLocal Beta — 100% functional-parity migration plan

Baseline: `origin/main` commit `365d372f37efda76ee43791a6f56d7be3f41e24f` (2026-11-22 audit).
Status: **inventory + implementation plan only**; feature parity has **not** been implemented.
Decision: **KEEP the exact existing Next.js UI and user journeys; replace only the Go backend/CLI/local runtime with Rust**. No UI redesign or feature deletion. Current `web/` has been restored from `main`; only runtime/API parity is pending.

## Source of truth and audit limitations

- Reference source: `old-project/`, archived from the exact baseline above, 1,304 tracked files plus manifest. **Active frontend `web/` is restored from that same baseline**; do not rewrite its pages or components.
- 52 route files named `page.tsx` under `old-project/web/src/app/`; only 1 Next route handler (`/healthz`) exists in this archived app, since Go owns the public API and trust boundary.
- Existing app entrypoints: `old-project/cmd/codelocal`, `cmd/codelocal-cloud`, `cmd/codelocal-reviewer`, `cmd/computerhelper`, `cmd/computernative`, `cmd/release`.
- API definitions: `old-project/internal/cloudserver` + `internal/webauth` + `internal/oauth` + `internal/cloudmcp`. The simple `mux.Handle` text-search count **is not** a verified number of unique endpoints.
- MCP gateway and tool contracts: `old-project/internal/mcpgateway/compact_tools.go`, `automation_tools.go`, `social_tool.go`, `blog_tool.go` and `server.go`.
- Public shipping scope: `old-project/STATUS.md`. Cloud/web features, internal code, Enterprise references and aspirational docs must not automatically be marked as shipped.
- This is a **static repository audit**, not a production behavior/UX audit. Collect screenshots, real API schemas, migrations, permission matrices and golden traces before claiming equivalence.

### Three BA cases

1. **A — preserve current Next.js frontend + rebuild backend/client (SELECTED):** keep existing URLs, labels, route/UI components, forms and states; implement Rust Cloud/CLI/runtime behind exactly compatible API/authorization/MCP contracts.
2. **B — parity with non-breaking internal refactor (LATER):** same externally observable behavior, but improve internal layering, data access, typing and performance once A gates pass.
3. **C — UX/product redesign (DEFERRED):** change IA and flow only under a separate, explicitly approved specification after A is complete.

## Existing web feature inventory

Status notation: **R** = route/UI and/or server logic found in archived repository, not proof of production readiness. **UI source is already restored in `web/`; it must not be rebuilt.**
All rows must reach **P** = verified integration of the preserved Next.js web with real Rust endpoints, persisted data and regression evidence.

| ID | Feature / current routes | Functions to preserve | Source evidence | Baseline | Rust/API parity |
| --- | --- | --- | --- | --- | --- |
| WEB-01 | Landing `/` | Product introduction, documentation/installation links, responsive presentation | `web/src/app/page.tsx` | R | UI retained; API pending |
| WEB-02 | Signup/login `/signup`, `/register`, `/signup/verify`, `/login`, `/logout` | Form validation, email verification, sessions, errors, redirect state | `web/src/app/signup`, `login`, `internal/webauth` | R | UI retained; API pending |
| WEB-03 | Password recovery `/forgot-password`, `/reset-password` | Reset email/token lifecycle, password change, rate limits | `web/src/app/forgot-password`, `reset-password`, `internal/webauth/password_reset.go` | R | UI retained; API pending |
| WEB-04 | MCP OAuth `/authorize` | OAuth consent and client authorization, redirects, protections | `web/src/app/authorize`, `internal/oauth` | R | UI retained; API pending |
| WEB-05 | Dashboard `/dashboard` | Workspace/device metrics, usage, recent activity, quick actions | `dashboard/overview-live.tsx`, `cloudserver/dashboard_api.go` | R | UI retained; account-scoped Overview GET with PG device/workspace/usage and Redis online status implemented; real-store CI and Skills parity pending |
| WEB-06 | Chat `/chat` | Providers/model selection, chat streaming, history/threads, context, tool output, media upload | `dashboard/dashboard-chat.tsx`, `cloudserver/dashboard_chat_*` | R (construction notice) | UI retained; API pending |
| WEB-07 | Projects `/dashboard/workspaces` | Device/workspace list, states, remove/revoke operations, detail data | `dashboard/workspaces`, `cloudserver/dashboard_resources_api.go` | R | UI retained; beta workspace catalog + signed metadata sync, E2E pending |
| WEB-08 | Devices `/dashboard/devices` and `/pair/approve` | Pair flow, online presence, device revoke, permissions | `dashboard/devices`, `pair/approve`, `cloudserver/server.go` | R | UI retained; API pending |
| WEB-09 | MCP connections `/dashboard/connect` | Endpoint copy, OAuth setup instructions, client configuration | `dashboard/connect` | R | UI retained; API pending |
| WEB-10 | Project Brain `/dashboard/knowledge` | Knowledge graph and health, durable intelligence views | `dashboard/knowledge`, `cloudserver/knowledge_*` | R | UI retained; API pending |
| WEB-11 | Code Graph `/dashboard/code-graph` | Code structure, graph query and UI exploration | `dashboard/code-graph`, `cloudserver/code_graph_*` | R | UI retained; API pending |
| WEB-12 | Skills `/dashboard/skills` | Browse, import, publish, user enable/disable, ratings, admin evaluation/promote/rollback | `dashboard/skills`, `cloudserver/skill_routes.go` | R | UI retained; API pending |
| WEB-13 | Plugins `/dashboard/plugins`, `/dashboard/plugins/approve/[id]` | Explore/installed, install/remove, local/cloud connections, credentials, OAuth, approval | `dashboard/plugins`, `cloudserver/plugin_routes.go` | R | UI retained; API pending |
| WEB-14 | Penpot Design `/dashboard/design` | Embedded Penpot, SSO, workspace connection, fallback link | `dashboard/design`, `internal/penpot` | R | UI retained; API pending |
| WEB-15 | Usage `/dashboard/usage` | MCP calls and estimated token statistics | `dashboard/usage`, `cloudserver/usage_api.go` | R | UI retained; Rust account-scoped Usage GET/PG E2E passed on ae18c36; actual MCP/Chat event producers pending |
| WEB-16 | Settings `/dashboard/settings` | Runtime configuration, execution modes, secrets, decision/provider settings, navigation hub | `dashboard/settings`, `cloudserver/runtime_settings_api.go` | R | UI retained; API pending |
| WEB-17 | Security `/dashboard/security` | Security settings, sessions, passwords, device/access controls | `dashboard/security`, `internal/webauth` | R | UI retained; API pending |
| WEB-18 | Account `/dashboard/account` | User profile/preferences, account details, signout | `dashboard/account`, `cloudserver/account_api.go` | R | UI retained; API pending |
| WEB-19 | Admin `/dashboard/admin` | Privileged system and moderation/skill administration, strict RBAC | `dashboard/admin`, `cloudserver/admin*` APIs | R | UI retained; API pending |
| WEB-20 | Blogs `/dashboard/blogs`, posts, series; `/blog*` and `/blogs*` public routes | Editor, draft/update/publish/unpublish/delete, series, tag/category views, distribution | `dashboard/blogs`, `cloudserver/blog_routes.go` | R | UI retained; API pending |
| WEB-21 | Forums `/dashboard/forums`, `/forums`, topic details | Create/view topics, comments, voting, moderation, permissions | `dashboard/forums`, `cloudserver/forum_routes.go` | R | UI retained; API pending |
| WEB-22 | Shots/media `/dashboard/shots`, `/s/[shareID]` | Media prepare/upload/finalize, variants, public/private sharing, revoke | `dashboard/shots`, `cloudserver/media_asset_routes.go` | R | UI retained; API pending |
| WEB-23 | Invite `/dashboard/invite` | Invite/referral view and available account-scoped actions | `dashboard/invite`, `cloudserver/dashboard_api.go` | R | UI retained; API pending |
| WEB-24 | Public/support/trust `/support`, `/terms`, `/privacy`, `/security`, `/users/[authorID]` | Public content, author profiles and SEO/metadata | `web/src/app` public pages | R | UI retained; API pending |
| WEB-25 | Cross-cutting shell | Sidebar/nav rail, mobile drawers, loading/empty/error states, language selection, accessibility, route guards | `dashboard/dashboard-nav.tsx`, `dashboard-sidebar.tsx`, `web/src/lib/i18n` | R | UI retained; API pending |

**Critical distinction:** existing Chat UI includes `ConstructionNotice` and legacy docs flag preview limitations. Parity means reproducing **documented and observed existing behavior**; do not falsely change a limited feature to “fully supported” or silently remove the UI.

## Core Cloud/CLI/runtime/MCP inventory

| ID | Current capability and acceptance boundary | Archived source | New owner | Priority |
| --- | --- | --- | --- | --- |
| CORE-01 | Authentication: signup/verification, login/logout, password reset, cookie/session/CSRF, OAuth/OIDC and reauth | `internal/webauth`, `internal/oauth` | Rust identity/auth module | P0 — Rust session/forms/OTP source implemented; real PG/Redis login/account/logout E2E passed in CI on 49b7e12; email delivery/OAuth/re-auth pending |
| CORE-02 | Multi-user account isolation, authorization, admin guards, rate limiting, audit | `internal/cloud`, `cloudserver`, `security` | Rust identity + policy | P0 |
| CORE-03 | Cloud storage, PostgreSQL schema/data migrations, domain stores and transactional operations | `internal/cloud` | Rust storage/domain modules | P0 |
| CORE-04 | Pairing handshake, device identity, signed requests, credential lifecycle and presence | `internal/deviceauth`, `cloudserver/runtime_ws.go` | Rust cloud + local runtime | P0 — pair/claim, Ed25519/Redis anti-replay WS presence+heartbeat/expiry implemented; real-store pairing/workspace/lease E2E passed in CI on 49b7e12; activation/revocation queue pending |
| CORE-05 | Account-scoped device/workspace routing, sync, reconnect, revocation | `internal/runtime`, `localclient`, `workspace` | Rust runtime transport | P0 — signed workspace metadata, foreground WS presence/reconnect and account-scoped online status implemented; secure command routing/revocation acknowledgment pending |
| CORE-06 | Remote MCP HTTPS, OAuth, public discovery, tool schemas, error shape and stream semantics | `internal/mcpgateway`, `internal/cloudmcp` | Rust MCP gateway | P0 |
| CORE-07 | CLI startup, login/logout/switch account, status/stop, workspace grant/ungrant/list | `cmd/codelocal` | Rust CLI | P0 — grant/list/revoke/status, signed pairing/sync/logout, local MCP stdio with one-use human review and foreground WS presence runtime implemented; service installer/stop/supervision and privileged routing pending |
| CORE-08 | Workspace file boundaries, sensitive file exclusion, symlink/traversal protection, hash-safe writes | `internal/localfs`, `internal/security`, `internal/editing` | Rust local engine | P0 — read/info/range/many, gitignore-aware search and SHA-256-guarded local exact edits tested on Unix; Windows secure writes, cross-process CAS, remote MCP and approvals pending |
| CORE-09 | Git read/write, terminal preflight, guarded host execution, PTY/process life cycle, audits/idempotency | `internal/process`, `internal/approval`, `internal/taskexecution` | Rust local engine | P0 |
| CORE-10 | Project mapping, code search, dependency lookup, semantic symbols/LSP, call/import graph | `internal/project`, `internal/lsp`, `internal/repository` | Rust intelligence | P1 |
| CORE-11 | Project Brain, sanitized memory, Experience verification/promotion, learned skills, knowledge graph, health | `internal/projectbrain`, `memory`, `learnedskills`, `cloud` | Rust intelligence + storage | P1 |
| CORE-12 | AI chat: provider catalog/BYOK, streaming, threads/history, model router, runtime tools, media | `internal/cloudserver/dashboard_chat_*`, `internal/decision*` | Rust chat/orchestration | P1 |
| CORE-13 | Bounded multi-step agents, checkpoints, recovery, verification, orchestration | `internal/orchestration`, `internal/taskstate`, `internal/toolprogram`, `mcpgateway` | Rust agent engine | P1 |
| CORE-14 | Optional browser automation and Computer Use, permission checks and media capture | `internal/automation`, `cmd/computerhelper`, `cmd/computernative` | Rust controller + OS-specific bridge | P1 |
| CORE-15 | MCP extension hub, local stdio/remote connection management, cloud:plane, tool discovery/invocation approval | `internal/mcphub`, `mcpconfig`, `mcpgateway` | Rust extension hub | P1 — read-only local MCP stdio (5 tools including approved batch read) with per-request human review and one-use execution available; Cloud OAuth/approval gateway and extension hub pending |
| CORE-16 | Plugin catalog/install/remove, local/cloud credential handling, approvals, OAuth and connection state | `internal/plugins`, `cloudserver/plugin_*` | Rust plugin service | P2 |
| CORE-17 | Skill catalog/import/publish/rating, evaluations, promotions and rollback | `internal/skills`, `cloudserver/skill_*` | Rust skills service | P2 |
| CORE-18 | Design/Penpot MCP and WebSocket/SSO integration | `internal/penpot`, `cloudserver/server.go` | Rust integration service | P2 |
| CORE-19 | Blog/forum/social, media assets/shots, public author pages, email and admin | `internal/social`, `cloudserver/{blog,forum,media}*`, `mailer` | Rust content services | P2 |
| CORE-20 | Usage metrics, model/provider and decision preferences, admin and invitations | `internal/usage`, `decision*`, `cloudserver` | Rust account/metrics | P2 — real PG Usage DTO/query/event dedup source added; provider/MCP producers, preferences/admin/invites pending |
| CORE-21 | npm stable/beta releases, bundled native binaries, updater, macOS/Windows/Linux, install/uninstall and diagnostics | `cmd/release`, `internal/clientupdate`, `scripts`, `.github/workflows` | Rust CLI release pipeline | P2 |
| CORE-22 | Health/metrics, rate limits, migrations, feature flags, WebSocket routing, deploy rollback | `internal/observability`, `featureflag`, `deploy` | Rust operations | P2 |

### CLI commands to preserve

Exact baseline help/entrypoint is `old-project/cmd/codelocal/main.go`. Required journeys:
- `codelocal`, `codelocal .`, `codelocal <path>`, `--help`, `--version`
- `login`, `login --force`, `logout`, `switch-account`, `pair`
- `setup`, `doctor`, `status`, `stop`
- `grant`, `ungrant`, `workspaces`
- `agent on`, `agent off`, `agent status`
- `approvals list/revoke/reset`
- `mcp add/list/info/probe/search/remove` (stdio and URL transports)
- `reset --all`, `uninstall --all` and interactive confirmation semantics.

### Public compact MCP contract

The actual `compactToolDefinitions()` return list in the baseline has **14 named public tools**; each tool exposes multiple actions. Preserve exact schemas, required arguments, error/approval responses and compatibility behavior:

1. `workspace` — list/select/info, access/execution modes, memories/skills, devices, approvals/security.
2. `context` — project info/map/instructions, ranked task context, dependencies and LSP actions.
3. `agent` — bounded action plan, continuation, verification and checkpoints.
4. `read` — info/file/range/many.
5. `search` — files/text.
6. `edit` — write/replace/patch/apply/format.
7. `verify` — snapshot/changes.
8. `git` — status/diff/log/show/blame/history/stage/unstage/commit/push.
9. `terminal` — preflight/history/run/start/PTY/process management/artifact publishing.
10. `mcp` — local and permitted cloud extensions discovery/call.
11. `social` — social/forum operations exposed by main.
12. `blog` — blog operations exposed by main.
13. `browser` — optional isolated browser operations.
14. `computer` — optional desktop/mobile OS control actions subject to permissions.

Do not substitute new tool names/schema for existing clients until a compatibility/versioned transition is explicitly accepted.

## Implementation waves and dependencies

| Wave | Execution scope | Deliverable / release gate |
| --- | --- | --- |
| 0 — Baseline capture + Next restoration | Preserve existing `web/` from `main`, inventory 52 page routes, HTTP registrations, 14 MCP schemas/actions, CLI help, DB migrations, and screenshots/flows | `web/src` and web package/config match baseline; old UI check/test/build pass; freeze contracts and golden traces |
| 1 — Platform security | Rust Axum/Tokio Cloud, database access/migrations, identity/session/OAuth, CSRF, RBAC, pairing, device+workspace authorization | Auth contract+negative/security tests; no protected data leakage |
| 2 — Local runtime + gateway | Rust CLI/daemon, sync/WS, MCP discovery/invoke, filesystem, Git, terminal, approvals, process and reconnection | Old AI client connecting to new beta and doing real work with permission gates |
| 3 — Existing browser integration | Keep current Next.js unchanged; wire its existing auth, Dashboard, Chat, Projects, Devices, MCP Connect, Account/Security/Settings/Usage to real Rust API | No new UI rewrite; real-data API and E2E journeys pass |
| 4 — Intelligence + extensions | Project Brain, Code Graph, Skills, Plugins/MCP hub, Agent, browser/computer, Design/Penpot | Verified persistence and sandbox/approval tests; multi-machine flows |
| 5 — Content + admin | Blogs, Forums, Shots/media, Invite, Admin, public pages, SEO, internationalization | CRUD/media/moderation/security E2E parity |
| 6 — Release and cutover | npm/native releases, migration dry-run, beta staging deployment, security/performance, release/rollback exercises | All parity gates green; separate authorization to promote production |

Implement Rust API families in parallel as contracts allow. **Do not recreate existing Next.js route families or visual components.** Only make minimal, separately justified web compatibility fixes; never use fake data, mock success or pretend endpoints are operational.

## Required acceptance for every feature

Each WEB/CORE row is complete only when all applicable assertions pass:

- **Visual/UX:** leave existing Next.js route/UI components untouched except necessary and tested integration fixes; baseline labels, navigation, breakpoints, forms and states remain identical.
- **Behavior:** reads/writes use actual Rust backend, data persists and the same user journey produces compatible observable results.
- **Contract:** request/response/status/error, paging/filtering, streaming, API auth, MCP action schema and CLI exit behavior match documented and captured baseline, or an approved compatibility adapter exists.
- **Security:** RBAC/account boundaries, CSRF, OAuth PKCE/token lifetime, device ownership, path traversal/symlink denials, secret redaction, approvals and revocation have automated negative tests.
- **Data:** migration checks on a non-production copy, record counts/integrity, no credentials moved through Git, rollback rehearsed.
- **Quality:** Rust fmt/test/clippy; Next lint/typecheck/build; integration/E2E tests; release smoke tests on applicable OSs.
- **Evidence:** maintain explicit links to source route, new route, tests, screenshots, changelog and remaining deviations; no percentage without denominator/coverage.

A feature is **NOT done** when the page merely renders, a Rust endpoint answers `200`, or a provider returns synthetic data.

## Baseline caveats to retain

- `old-project/STATUS.md` explicitly warns that old terminal security is application-level host-policy, **not an OS-enforced sandbox**.
- Windows PTY and cross-language semantic analysis have documented parity limitations.
- Remote MCP continuity depends on Cloud; do not claim fully offline remote flow.
- Code in an Enterprise/reference/preview folder is not proof it ships in the public single-user edition.
- Existing Next.js UI source is restored, but Rust backend/client remain **bootstrap only**. As of this audit, no old business-domain feature has passed runtime parity acceptance.

## Next concrete tasks

1. Preserve existing Next.js (already restored); record full HTTP/MCP/CLI source manifests and mark shipped/preview/unknown from real old-stack tests.
2. Capture screenshots and critical workflow fixtures from an isolated main/reference environment (do not use production credentials in test artifacts).
3. Define Rust endpoint contracts and persistence schema, starting with identity/session and device pairing.
4. Write negative security and E2E tests first; then port modules wave by wave with mapping and proof.
5. Keep `main` and production untouched; do not push/deploy/cut over until separately authorized.
