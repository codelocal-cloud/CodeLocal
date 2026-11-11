# Plane Cloud MCP in ChatGPT through CodeLocal — implementation plan

**Date:** 2026-11-11
**Owner:** CodeLocal backend / MCP Gateway
**User goal:** Use the existing Plane self-hosted Cloud plugin inside the ChatGPT CodeLocal connector and create the mobile-first ChatNet/SocCO Sprint 1 backlog without importing CSV manually.

## Root cause

- CodeLocal's browser Plugins dashboard establishes a user-scoped, encrypted Cloud Plane connection at `plane.codelocal.cloud` through `PlaneMCP` and discovers 30 tools.
- The ChatGPT CodeLocal MCP tool `mcp(action=list|search|info|call)` previously delegated exclusively to local device-side MCP Hub. It therefore listed local Blender/Penpot but could not see a Cloud Plane connection.
- Local MCP Hub must **not** receive or persist the Plane PAT from another user's Cloud credential store.

## Plan / gates

1. **Inspect & preserve** — confirm current ChatNet `main` baseline `bbd5f80`, preserve its dirty local checkout; implement CodeLocal changes in an isolated Git worktree.
2. **Bridge discovery** — keep CodeLocal's existing 14-tool ChatGPT surface and its legacy `mcp` action schema; add synthetic `cloud:plane` visible when that exact OAuth user has a ready Cloud Plane connection. Allow `mcp(action=search|info,server=cloud:plane)` to discover live tools directly in CodeLocal Cloud.
3. **Bridge execution** — require authenticated ChatGPT MCP user, trusted exact Plane PAT endpoint, ready user-owned connection, encrypted credential and valid workspace slug. Revalidate connection version before execution. Require **explicit MCP client elicitation** of each exact operation/arguments, or show an authenticated CodeLocal approval link when elicitation is unsupported; persist & consume backend one-shot approval. Never trust `readOnlyHint`, AI-authored approval tokens, shared admin PATs or a device's full-access setting.
4. **Verification** — tests for legacy MCP ABI, alias scoping, endpoint allowlist and approval fail-closed; full Go suite, vet, repo checks and deployment staging/main CI. Check live `mcp(action=list)` and `mcp(action=search,server=cloud:plane)` from ChatGPT. Execute a real Plane mutation only when the client supports elicitation and the user accepts.
5. **Mobile sprint tasks** — create/check the 22 prepared `S1-MOB-01`…`S1-MOB-22` work items on **SocCo**, link to Sprint 1 after confirming the target project/cycle. Detect existing IDs first; do not duplicate or invent owners/due dates. Scope only mobile Web/PWA iOS/Android: auth, chat, groups, voice/media, push and QA. **Exclude Marketplace, Point/Coin, checkout and desktop release.**

## One-confirmation Sprint 1 bulk workflow

CodeLocal also exposes a **bounded bridge-only** `batch_create_sprint` tool under `cloud:plane`. This is not a native Plane tool. It accepts the exact project UUID, cycle name, owner UUID, and 1–25 items with unique keys, descriptions, and priorities. It creates or reuses the matching cycle, reads by external source/id before creating work items, attaches them to the cycle and verifies membership.

One authenticated browser approval displays all arguments before any action. The work is then performed with the existing per-account encrypted Plane PAT; the PAT never leaves CodeLocal Cloud. If any call has an unknown outcome, the batch **stops immediately**, reports the created or existing subset and never retries a mutation automatically. Do not call the tool again until existing item IDs have been inspected. This workflow is restricted to the already-trusted Plane PAT endpoint and same authenticated owner, and it does not enable arbitrary batch tool execution.

## Connected-owner direct execution (2026-11-11)

The owner of an already-connected, ready Plane Cloud PAT connection can now call a **small explicit allowlist** without a second CodeLocal browser approval. This is intentional: connecting a Plane account and requesting operations from ChatGPT is the standard authorization for routine work. The following actions are allowed directly: `project` read/list; `cycle` read/list/create and **add-only** work item membership; `workitem` read/search/create and content/progress update with a restricted set of fields; `member/label/state/module` read; and the schema-validated `batch_create_sprint` workflow (at most 25 keyed work items). The caller must remain the same authenticated CodeLocal account, the origin must be the allowlisted Plane MCP PAT endpoint, and the server uses the encrypted per-user bearer without exporting it.

The direct path **does not** approve `delete`, `archive`, membership removal, assignee/permission changes, project administration, or arbitrary unknown tools/actions. Those still use the owner confirmation path and one-shot approval storage. The policy is defined server-side by exact tool + action + scoped argument checks, **never** by tool descriptions, readOnly annotations, local device Full Access, or AI-supplied approval tokens. All direct calls are audited, validated against upstream tool JSON schemas, and never automatically replayed following an unknown outcome. For batch calls, partial results are returned for inspection instead of automatically retrying mutations.

This change intentionally does **not** alter the local CodeLocal runtime's separate workspace access controls, device privileges, or Git/terminal safety rules.

## Definition of Done

- **Bridge tests** pass and unchanged local extension routing remains compatible with previously connected clients.
- The same authenticated ChatGPT CodeLocal account can discover Plane tools using `server=cloud:plane`, regardless of MacBook online state.
- No Plane PAT/token appears in MCP responses, source code, logs or local MCP config.
- A mutating operation requires an actual **user** confirmation: MCP elicitation where available, otherwise the linked CodeLocal browser approval page, bound to the originating user's account. No confirmation means **no dispatch**. Calls are at-most-once with an outcome-unknown warning on failure.
- Railway deployment is verified separately from Git commit/push. A successful 30-tool discovery alone does **not** establish successful write access.
- Sprint 1 tasks are only marked **created** after listing them back from Plane with their IDs. If host-side elicitation is unsupported, report the exact blocker; do not pretend import or creation succeeded.

## Usage after deployment

1. `mcp(action=list)` → inspect the appended `cloudServers` entry for `cloud:plane`. This may appear even when local MCP servers are offline.
2. `mcp(action=search,server=cloud:plane,query="")` → current live Plane tool schemas, bounded to 50.
3. `mcp(action=info,server=cloud:plane,tool=<exact tool name>)` → input schema.
4. `mcp(action=call,server=cloud:plane,tool=<exact tool name>,arguments={...})` → user confirmation via MCP elicitation or a returned CodeLocal approval URL; one approved Cloud execution. The fallback URL expires in 5 minutes, is visible only to the same signed-in account, and uses CSRF + session freshness checks.

**Limitations:** ChatGPT MCP client support for MCP elicitation is not guaranteed. When unsupported, CodeLocal returns an owner-only approval URL; the tool does not run until the user approves it. The chat must inspect Plane before attempting retries after an unknown result. This implementation deliberately avoids synchronizing Cloud PATs to the local MCP Hub.
