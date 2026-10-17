# Backend operations, quality and safe delivery — actionable checklist

Use with [RULEBOOK.md](RULEBOOK.md) rules #07–#19. Project-aware defaults: do not install production tooling without a justified need.

## Error taxonomy & resilience

| Case | HTTP/client behavior | Server action |
| --- | --- | --- |
| Invalid schema/field | 400/422 with field/code | Log non-sensitive context; avoid stack to client |
| Missing authentication | 401; recover safely | Verify session/token, protect enumeration |
| Forbidden resource | 403 or privacy-safe 404 | Check role + resource/tenant ownership |
| Conflict/outdated version | 409; prompt reload/reconcile | Conditional update/version checks |
| Throttle/cost cap | 429; Retry-After when known | Metrics, atomic accounting where required |
| Dependency outage | Safe 502/503/504 or equivalent | Timeout, classify transient, bounded safe retry |
| Unknown application failure | 500 with trace ID | One structured error log, alert based on impact |

Do not return 200 for failed mutation. Timeout on a non-idempotent write means **unknown outcome**, not automatic failure and blind reissue. External secondary effects require explicit durable dispatch/compensation strategy where business impact warrants.

## Audit vs logs

- Request/technical logs: short operational trace, redact PII/secrets and avoid duplicate exception logging.
- Security audit: authentication/session/role/permission changes, abuse, high-risk access.
- Business history: consequential approve, delete, status change, financial events and selected field diffs.
- Important audit storage must be durable and appropriately atomic with business effects; technical telemetry may be best-effort.
- Retention, archive, access review, tamper resistance and restoration are separate decisions.

## Migration & backup decision table

| Change | Minimum |
| --- | --- |
| Add nullable field/index in local/test | Reversible migration, tests, query impact |
| Add NOT NULL/change enum on shared deployment | Expand/backfill/contract plan, read/write compatibility |
| Drop data/table or destructive transform | Explicit permission, verified recovery, client impact, cutover plan |
| Mixed-version rollout | Old/new readers and writers compatible throughout |
| Prod migration | Approved release gate, backup/recovery evidence and post-checks |

"Rollback migration succeeded" is **not** proof user data was recovered. Production writes require explicit policy/grant.

## Quality & release gates

1. Inspect repo permissions and Git state; identify tests and deploy targets.
2. Low: focused formatting/type checks. Medium: API and business unit/integration/contract checks.
3. High: auth, tenant, transaction/concurrency, migration negative paths + data-recovery controls when applicable.
4. Critical: explicit approvals and staged/controlled release with measurable rollback/recovery strategy.
5. FAIL / important NOT RUN / BLOCKED means no autonomous merge or deploy; safe WIP on allowed branch can be saved.
6. Check results separately: code edited, local build/test, commit hash, remote branch, merged target, CI status, deployment health and user acceptance. Never infer one from another.

## Scale readiness

- Profile DB query plans and pagination before adding cache.
- Use stateless services and shared quotas when multi-node semantics need them.
- Timeouts, cancellation, bounded queues/backpressure, connection-pool and retry budgets.
- Introduce tracing/Redis/queue/circuit breaker by measured need; note triggers and trade-offs.
- Money uses domain-appropriate exact representations, time and timezone semantics are explicit.

## Handoff evidence template

Scope: ...
Risk (Low/Medium/High/Critical): ...
PASS: command/test + observed outcome ...
FAIL: command/test + observed outcome ...
NOT RUN: reason ...
BLOCKED: prerequisite/permission ...
Changed files: ...
API/data/security compatibility: ...
Git working tree / commit / push / merge: (each state independently) ...
CI / deployed service / live UAT: (each separately, NOT RUN if not checked) ...
Remaining risks and recovery plan: ...
