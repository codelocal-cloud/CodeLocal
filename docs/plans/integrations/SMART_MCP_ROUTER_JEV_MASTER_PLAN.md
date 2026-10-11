# Smart MCP Router — Jev Reranking & Context Budget Master Plan

Status: PROPOSED / DOCUMENTED ONLY (not implemented)
Decision date: 2026-10-11
Owner domains: Plugin Platform, MCP Hub, Dashboard Chat, Project Brain
Baseline: origin/main at 99e08e0
Implementation: separate feature branch after design review; do not treat this plan as a shipped feature.

## 0. Executive decision

Build a provider-neutral Smart MCP Router above the existing Plugin registry and MCP Hub, not another MCP gateway. Start with hard eligibility rules and existing lexical/schema search, add hybrid retrieval only when measured, and evaluate TypeSafe Jev as an **optional rerank/gate provider** in shadow mode before any live routing. Default must remain useful without Jev, an embedding vendor, or network access.

Primary outcomes: select the right installed/connected/authorized tools for the intent, avoid unnecessary schema/result text in the model context, improve tool selection reliability, and preserve the CodeLocal security contract.

**Important current-state correction:** CodeLocal already exposes a compact public MCP surface and lazy extension discovery (mcp.search -> mcp.info -> mcp.call). Hundreds of installed MCP tools are **not** all preloaded as first-party ChatGPT schemas. This project must measure marginal improvements to search quality, discovery payloads, unnecessary info/call turns and Dashboard Chat tool exposure. It must not claim savings from removing an already absent "500 tool schema" prompt.

## 1. Scope and non-goals

In scope:
- Installed Plugin/App/MCP discovery, Cloud + Local runtime choice, ranking, just-in-time schema, result context budget, opt-in Jev adapter, bounded cache, observability, eval harness.
- Dashboard Chat internal Plugin discovery and the existing compact public mcp search/info/call contract.
- Plugin marketplace suggestions as a **separate** catalog discovery surface, clearly distinguished from tools that can actually execute.
- Optional later reuse of the retrieval framework for Project Brain code context, but not in this initial MCP rollout.

Out of scope:
- A new general-purpose LLM router, a replacement for internal/mcphub, new top-level MCP tools per Plugin, automatic installs, automatic authorizations, prompt-history compression, or a promise to increase ChatGPT subscription limits.
- Trusting model/re-ranker scores as authorization, shipping customer content to a third party by default, or mutating plugin grants/credentials during discovery.

## 2. Existing implementation / ownership to reuse

| Current source | Existing responsibility | Planned integration |
| --- | --- | --- |
| internal/mcphub | Local registry, probe, lexical/schema catalog search, exact info, invocation | Keep as authoritative Local execution/discovery adapter; wrap search candidates |
| internal/localclient/engine.go | mcp_search_tools, mcp_tool_info, mcp_call dispatch | Keep unchanged externally; optionally delegate ranked search under flag |
| internal/mcpgateway/compact_tools.go | Compact model-visible domain tool surface | **No schema proliferation** |
| internal/cloudserver/plugin_chat_tools.go | Dashboard Chat list_plugin_tools, call_plugin_tool, Cloud vs Local discovery, approval | Avoid bulk discovery results; select targeted candidates; preserve approval flow |
| internal/cloudmcp | Cloud MCP discovery/execution | Feed permitted Cloud candidates, not credentials |
| internal/plugins, internal/cloud | Plugin metadata, install/connection state and trust/version data | Resolve eligibility from the canonical store, not a new parallel registry |
| internal/projectbrain, internal/project | Task and code context | Future extension only; do not cross-contaminate tenant/project data |

Existing documents are authoritative on their own boundaries:
- docs/plans/integrations/PLUGIN_PLATFORM_MASTER_PLAN.md
- docs/plans/integrations/MCP_HUB.md
- docs/plans/integrations/MCP_TOOL_SURFACE_PLAN.md
- docs/integrations/MCP_COMPATIBILITY.md
- docs/plans/benchmarking/CODELOCAL_BENCHMARK_PLAN.md
- docs/architecture/SECURITY_AND_PRIVACY.md

Validate actual implementation state against the repository at the start of each phase; planned components may not yet be fully deployed.

## 3. User journeys / BA acceptance cases

**UC-01: Find an installed tool.** User asks to create a GitHub issue. Registry filters tools to eligible GitHub connection(s), semantic/lexical retrieval proposes candidates, optional reranker sorts them, the client retrieves the exact input schema only for a chosen tool, existing approval gates the write.

**UC-02: Many installed tools.** With 30 MCP servers and 500 available extension tools, client still sees compact CodeLocal tool schema(s), not 500 schemas. Candidate result is a bounded shortlist; no global tool-catalog dump in a prompt. Figures are fixture sizes, not claimed production measurements.

**UC-03: Missing/disconnected tool.** Return explicit "connect required", "local device offline" or "not installed" status and a safe next action. Never pretend tool execution succeeded or silently install.

**UC-04: Local versus Cloud.** Choose only eligible deployment targets: exact connected account, workspace, device, supported transport and requested data sensitivity. A reranker may compare eligible choices but cannot widen the set.

**UC-05: Explicit user selection.** @plugin, chosen server/tool, pinned target, or requested manual routing takes precedence over model ranking subject to hard policy; no silent substitution across credentials/devices.

**UC-06: Ambiguous / no good match.** Show a shortlist or ask for clarification/return abstain, rather than confidently invoking the wrong write/delete tool.

**UC-07: External model offline/error.** Deterministic retrieval continues under a bounded latency budget; optional Jev failure cannot break core MCP discovery.

**UC-08: Marketplace browsing.** Search public Plugin catalog for **suggestions** with install/permission information. Do not put uninstalled tools in callable runtime candidates or assume ratings equal safety.

**UC-09: Prompt injection via tool metadata.** A malicious description is data only. It cannot change system rules, force installation, reveal secrets, or skip approvals.

**UC-10: Dashboard Chat tool results.** Return a short typed preview and stable reference when possible. Full/binary/structured results remain retrievable on demand and preserve MCP content semantics; do not lose required evidence merely to hit a token budget.

## 4. Reference architecture

~~~text
ChatGPT / Codex / Claude (compact CodeLocal MCP)      CodeLocal Dashboard Chat
                  |                                              |
                  +-------------- routing request ---------------+
                                         |
                               Smart MCP Resolver
                                         |
                        tenant + workspace + policy gate
                                         |
                    installation / connection / runtime readiness
                                         |
                  local MCP Hub + Cloud MCP candidate catalog
                                         |
                 lexical/schema baseline (+ optional hybrid search)
                                         |
                optional Jev rerank + abstain (SHADOW first)
                                         |
               budgeted candidates + reasons + exact tool refs
                                         |
                lazy schema fetch -> existing approval/security
                                         |
                   existing Cloud or Local MCP execution
                                         |
                       bounded result / fetch full on demand
~~~

Authorization happens **both** before candidate exposure (eligibility/tenant checks) and at execution time using current policy and the exact tool/arguments. A search result is never an authorization token.

## 5. Router contract (internal, conceptual)

Input:
- authenticated user and tenant identity, exact workspaceKey/session, client surface, task intent (minimal/sanitized), optional explicitly selected Plugin/connection;
- requested target type (cloud or local:<device>), allowed installed scopes, maxCandidates, maxMetadataTokens, deadline, privacy mode;
- catalog generation/version and user-enabled capability filters.

Output:
- ordered {pluginId, appId, connectionId, executionTarget, server, tool, schemaRef, readiness, score, reasonCode};
- abstain/clarify/connect-required reason where appropriate;
- retrievalSource, rerankerUsed, fallbackReason, catalogVersion and measured latency/token-estimate metadata;
- never include credentials, local secret paths, bearer headers, private tool results or hidden tools.

Exact schema remains resolved by the existing info/discovery path at use time. The gateway must recheck connection version, scope, current permissions and approval before actual invocation (TOCTOU protection). Keep the existing external mcp actions, JSON input contracts and historical clients compatible.

Do not use arbitrary LLM-generated scores as hard grants or as proof of server/tool availability.

## 6. Retrieval and scoring design

1. **Hard filters (non-ML):** authenticated tenant, active installation, enablement/routing mode, user/workspace membership, actual connection/readiness, transport compatibility, runtime availability, declared capabilities and policy. Surface disabled/unavailable reasons separately when UX needs them.
2. **Candidate retrieval:** current lexical/schema search first; later hybrid BM25 + embeddings for language/paraphrase robustness. Use stable names/descriptions/input-schema summaries and capability tags; exclude secrets and tool outputs.
3. **Reranking:** Jev adapter scores candidate relevance against concise intent, optionally abstains if no useful match. Compare against deterministic and hybrid baselines. Never use Jev for security authorization or factual tool schema creation.
4. **Diversity + constraints:** deduplicate aliases and near-identical versions; respect explicit target/pins; prefer readiness and user choice over vendor score.
5. **Budget compiler:** choose a bounded list of metadata, preserve toolRef and why it matched; fetch exact schema only when selected. Avoid truncating schema in ways that alter validation.
6. **Tool results:** preserve original MCP responses behind exact references; summaries/previews are optional and lossless full retrieval is possible. In particular, errors and unknown outcomes must not be summarized away.

Use catalog version/content hash for incremental reindexing and cache invalidation on install/uninstall, version, permission/grant, connection health, credential refresh, workspace membership or runtime status changes. Results must never be shared across tenants, users, scopes or incompatible policy versions.

## 7. Jev provider adapter / privacy

Jev is a **candidate**, not a dependency or a pre-approved external processor. Verify current vendor contract, output schema, availability, pricing, retention and data processing terms during implementation. Do not hardcode an unverified per-token price or assume the vendor is cheaper end-to-end.

Modes:
- off: deterministic/hybrid search only (production safe default);
- shadow: call Jev for evaluation but keep baseline results and execution unchanged;
- canary: Jev may reorder only already-eligible candidates under strict budgets;
- enabled: conditional per workspace/org rollout after acceptance gates.

Privacy default: no external reranking for private workspace intents unless informed explicit opt-in and policy allow it. Strip user IDs, exact workspace paths, private repo names, tokens, connection URLs, source code, provider messages, tool arguments/results and other sensitive fields. Tool descriptions can themselves be private; route only approved public/sanitized metadata. If sanitization would destroy required meaning, skip Jev.

Treat remote reranking as **data egress** subject to enterprise policy, audit and retention rules. Use configured credentials via existing secret provider; redact logs; enforce timeouts, backoff, circuit breaker and quotas. Never retry a non-idempotent MCP call when rerank/discovery times out.

## 8. Latency, token and cost accounting

Measure **end-to-end task cost**, not just a short prompt:
- model input/output + reasoning token usage when provider exposes it;
- MCP tool definition / list / info / result tokens;
- search/index/embedding/rerank provider calls and spend;
- cold and warm p50/p95 latency, cache hit/miss;
- extra follow-up tools, retries, wrong-tool invocations and completion quality.

Show separate metrics for (a) routing/discovery context and (b) overall conversation/task token spend. A short candidate list does not itself guarantee savings if it creates more discovery turns or incorrect selections.

Evaluation gates (targets to validate, not factual claims):
- 0 tenant isolation, authorization, approval, secret-egress or destructive-action regressions;
- no degradation of critical-tool recall on seeded high-risk tests; non-inferior task completion against baseline;
- target >=20% lower routing/discovery prompt-token overhead on large-catalog fixtures when Jev is enabled, **including** overhead of extra selection turns; report smaller/no gains honestly;
- Jev incremental end-to-end p95 latency target <=500 ms on warm runs or disable for latency-sensitive paths; configurable deadline and immediate fallback;
- positive net economic savings only after counting Jev + embeddings + infrastructure; otherwise do not enable by default.

All targets must be measured with repeatable fixtures and documented methodology. Do not advertise an "85% token reduction" absent project-specific data.

## 9. Data model and interface strategy

Prefer existing Plugin installations, App connections, immutable manifest/tool catalogs, runtime health and user grant tables as source of truth.

Derived data (rebuildable, versioned; no secrets):
- tool_search_index: tenant/scope-qualified reference + tool identity + metadata fingerprint + embedding/index versions if chosen;
- tool_router_cache: tenant/workspace/connection/policy/catalog fingerprint, intent hash, candidate refs, expiry;
- tool_router_eval_runs: sanitized benchmark-only aggregates and reproducible fixture revision.

Do not create new DB tables until the cache/index lifetime and storage location have been justified. Local-only tool catalogs must remain local unless a user explicitly enables metadata sync under the security policy. Provider failure must not corrupt canonical Plugin/MCP records.

## 10. Deployment / UX

- Keep /dashboard/connect as the connection guide; do **not** turn it into the Plugin marketplace.
- Keep /dashboard/plugins as installation, connection and management; show opt-in "Smart tool selection" settings there only when UX is ready.
- Cloud/Local execution choice appears after user initiates install/connect or explicitly selects a target, not as an unexplained auto-pick.
- Internal debug/observability screen can show chosen candidate, eligibility, score source, cache hit, fallback and sanitized latency; do not reveal private connection metadata to other users.
- Preserve native tool search of provider clients when it works. Client adapter may skip Jev entirely if host-native lazy tool loading already yields equal/better outcomes.
- Feature flags: smart_mcp_router, smart_mcp_jev_shadow, smart_mcp_jev_canary, smart_mcp_result_budget, with immediate kill switch. Names are proposals pending config convention review.

## 11. Rollout checklist, in dependency order

### P0 — Baseline, security design, fixtures (no live routing changes)
- [ ] Inventory current Go MCP Hub ranking, Cloud MCP discovery and Dashboard Chat result payloads on fresh main.
- [ ] Document permission/tenant/connection gates, data-egress policy and target-specific readiness semantics.
- [ ] Add fixtures for 1/30/100 MCP servers, 10/500/2,000 tools, offline runtime, disconnected OAuth, mixed Cloud/Local, ambiguous tasks, adversarial descriptions and duplicate aliases.
- [ ] Benchmark baseline tool recall@k, MRR/nDCG, abstain accuracy, user approval outcomes, task success, token overhead, p50/p95, costs.
- [ ] Approval gate: security owner + product owner sign off on threat model and benchmark fixture coverage.

### P1 — Deterministic resolver and budgeted discovery (no Jev)
- [ ] Reuse existing registry/catalog APIs; centralize eligibility and readiness checks without weakening current execution checks.
- [ ] Add stable toolRef and bounded shortlist metadata; keep exact schema fetch on demand.
- [ ] Avoid Dashboard Chat's broad default 50-tool result for intent-specific discovery while keeping manual full inventory possible.
- [ ] Introduce scope-safe versioned cache, invalidation, logs and off/rollback flag.
- [ ] Add unit/integration tests for permission changes, installation updates, offline device, and backward compatibility.
- [ ] Gate: baseline or better quality; no security regressions; measurable overhead reduction.

### P2 — Hybrid retrieval and Jev shadow experiment
- [ ] Decide whether embedding actually beats lexical/schema baseline on Vietnamese + English queries and mixed developer phrasing.
- [ ] Define vendor-neutral Reranker interface and optional TypeSafe Jev adapter with sanitized request/response validation.
- [ ] Verify vendor billing, latency, DPA/retention, production limits and egress policy before any customer data path.
- [ ] Run shadow scoring only; log sanitized aggregate comparisons without changing tool selection or execution.
- [ ] Compare baseline / hybrid / hybrid+Jev / client-native tool search where available, on identical frozen tasks.
- [ ] Gate: measurable ranking gain or economic/token improvement without recall/security regressions; otherwise keep Jev off.

### P3 — Canary in Cloud + Local with safe fallback
- [ ] Enable only for explicit consenting test users/workspaces; separate Cloud and Local target tests.
- [ ] Respect explicit user-pinned tool/provider and current per-action approval.
- [ ] Add strict timeouts, quota, fallback, provider disable and immediate kill switch.
- [ ] Run regression suite for malicious metadata, private workspace, stale catalogs, approval race and tool response fidelity.
- [ ] Gate: task quality non-inferiority, no policy bypass, within latency/cost budgets, successful rollback drill.

### P4 — Marketplace recommendations and optional Project Brain integration
- [ ] Separate installed-tool routing from "find new Plugin" recommendation index/UX.
- [ ] Explain why recommendation matched, permissions and trust state; install/connect must be explicit.
- [ ] Consider reusing candidate-ranking interfaces for task-specific code context **only** after MCP metrics and security are proven.
- [ ] Publish benchmark report and review whether default-on is warranted.

## 12. Test matrix and non-negotiable acceptance

Unit: deterministic filters, scoring normalization, tie-breakers, abstain, cache keys, invalidation, payload/token budget.

Integration: Local stdio/HTTP MCP, Cloud MCP, multi-workspace, different tenants, offline device, stale connection, version change, OAuth reconnect, manual routing, toolInfo schema checksum, concurrent install/uninstall, cancel/timeout.

Security: prompt injection in tool name/description/schema; secrets in metadata; XSS/SSRF via Plugin endpoint metadata; cross-tenant/permission leaks; write/delete approval bypass; stale approval after tool/argument changes; error/outcome-unknown no automatic replay.

E2E: ChatGPT through compact mcp facade, Dashboard Chat installed/disconnected plugin workflow, tool call approval and tool result integrity; model-native tool search parity.

Definition of Done: docs and threat model reviewed; deterministic fallback always works; feature flag and rollback tested; metrics exported; no regressions; benchmark report with raw aggregated reproducible cases; only then consider default enablement.

## 13. Engineering handoff and change control

- Create an implementation branch from a fresh main; separate P0/P1/P2/P3 PRs with focused tests and rollout notes. Do not modify the user's unrelated worktree changes.
- Prefer a cohesive resolver package/interface (final package location decided after dependency review) instead of new ad hoc logic in cloudserver or a parallel MCP Hub.
- Security/policy/approval remains authoritative; no new public tool action unless contract review is explicit.
- Each PR updates this document's checklist and date/status, plus docs/plans/integrations/PLUGIN_PLATFORM_MASTER_PLAN.md when architectural decisions change.
- Next action after this **documentation-only** commit: P0 baseline/fixtures and privacy threat model; **no live Jev integration has been implemented by this document**.

## 14. Decision log

- 2026-10-11 — Product request: optimize CodeLocal MCP marketplace/tool routing with Jev; preserve docs so the plan is not lost.
- 2026-10-11 — Architecture: provider-neutral router above existing Plugin/MCP, eligibility before reranking, Jev optional/shadow-first, fallback always, security non-negotiable.
- 2026-10-11 — Scope: documentation and implementation plan only. Benchmark before claiming token/cost savings.
