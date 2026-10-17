# Backend Logic Rulebook v1.0 — APPROVED

Scope: Go, NestJS, ASP.NET Core, Next.js server/BFF and other compatible backends. Framework choice follows the actual repository. Baseline: Smart Production Standard, Scale-ready Not Over-engineered. **19 approved rule groups**.

## Priority and decision policy

- **P0 / Non-negotiable:** Security and authorization, tenant/data boundaries, data integrity, API compatibility without an approved migration, truthful evidence, safe shared/prod operations.
- **P1 / Production standard:** Testability, maintainability, validated configuration, robust errors, observability and documented project-aware delivery.
- **P2 / Context-driven:** Distributed tracing, queues, cache layers, load engineering and optimizations when evidence or requirements warrant them.
- P1/P2 may use the simplest technically suitable implementation; never bypass P0. For uncertain high-impact business semantics, stop and ask. A documented, approved exception is not permission to misstate results.

## I. Architecture, API and data

### #01 Smart Layered Architecture (P1)
Separate HTTP transport, use cases/business rules and data access when responsibilities justify separation. Thin handlers validate transport and relay identity context; domain/service layer owns invariants; persistence layer owns safe queries. Do not require a repository/interface per entity or introduce microservices before necessary. Preserve framework idioms and testable boundaries. PASS: a reader can trace every critical operation and its authorization.

### #02 API Contracts & Backward Compatibility (P0/P1)
For new APIs, consistent DTO/schema, validation, methods/status codes and machine-readable error codes. GET/HEAD must not mutate business state. Avoid direct ORM entity exposure, secret fields or unconditional 200-on-error. Preserve existing clients; breaking routes/fields/enums/events require versioning, adapters, deprecation or coordinated migration. Validate all consumers (web, mobile, BFF, automation). A uniform response wrapper is optional and must not break current contracts. 400/422 validation, 401 unauthenticated, 403 forbidden, privacy-safe 404 where relevant, 409 concurrency, 429 throttling and safe 5xx are distinct.

### #03 Database & Smart Migrations (P0)
Bind query values via parameters/safe ORM; allowlist untrusted dynamic identifiers, filter and sort keys. Apply constraints, indexes, FK, unique/nullable models and least-privilege DB access. Use versioned migrations; prefer expand → backfill/migrate → contract for mixed-version rollout. Local/authorized test DB migrations may run automatically. Shared staging/prod deployment follows project approval; destructive changes require explicit authorization and validated recovery approach. Preserve old data and make old/new versions coexist where rollout requires it. Rollback schema does not guarantee restoring discarded data.

### #04 Hybrid Pagination (P1)
Growing GetList/Search/feed/log collections must paginate at database or use another bounded server-side strategy. Administrative jump-to-page uses page/pageSize/offset; large/fast-changing streams use stable keyset/cursor. Illustrative default 25, maximum 100, overridable per endpoint—not a universal invariant. Validate all inputs, enforce tenant/row permissions *before* count/limit and use stable order with unique tie-breaker. Do not load all rows to array-slice. Total count is conditional on contract and cost. Tiny bounded lookups, aggregations and controlled exports are exceptions; define export permissions/scope separately.

### #05 Transactions, Concurrency & Risk-based Idempotency (P0)
Use transactions when business changes must commit atomically. Choose unique constraints, optimistic version, atomic conditional updates, row locks and isolation based on actual race conditions. Payment, high-risk approvals/orders, charge/debit and webhook side effects need replay/idempotency protection; ordinary CRUD need not gain artificial key tables. Do not retry non-idempotent mutations unsafely. Do not hold a DB transaction across lengthy remote calls. Use outbox/durable asynchronous handoff when DB commit and event delivery must be reliably coordinated. Test duplicates, lost updates and partial failures.

## II. Security

### #06.1 RBAC + Resource-based Authorization (P0)
Authenticate on the server, deny by default, verify role *and* per-resource/tenant/owner/branch scope. Apply to list/detail/create/update/delete/approve/export/download/bulk actions. Never trust client-provided role/userId/tenantId; enforce access at query/service boundary, prevent IDOR/BOLA. ABAC/policy engine is a contextual addition, not mandatory scaffolding. Negative-path tests cover wrong user, wrong role, cross-tenant and revoked access.

### #06.2 Policy-based Rate Limit & Quotas (P0/P1)
Policy classes: AUTH (login/OTP/recovery), PUBLIC, AUTHENTICATED, HEAVY (search/export/bulk), METERED (AI/SMS/cost). Define burst/sustained/concurrency/day or cost ceilings by risk and traffic; never invent one universal number. Keys may combine trusted IP, normalized account, user, tenant, API key. Shared atomic counters/gateway are required for globally coherent quotas on multi-node deployments; trust proxy headers only from configured proxies. Return 429 and Retry-After when possible, monitor rejects and decide fail-open/closed deliberately by endpoint risk. Test reset semantics, concurrency and abuse. Daily quotas do not replace burst protection.

### #06.3 Risk-based Audit + Selective Data History (P0/P1)
Distinguish technical logs, security audit and business audit. Persist actor/tenant/action/resource/time/result/requestId and selected before/after changes for consequential operations, not indiscriminate full snapshots. Never record passwords, OTP, tokens or unnecessary PII. Important audit must be durable and consistent with business effects; choose append-only/tamper resistance as needed. Protect viewer rights, archive/retention, pagination and sensitivity. Audit is not a financial ledger and is not an automatic restore mechanism.

### Shared security baseline (P0)
Server-side input validation, XSS/CSRF defenses for applicable cookie mutations, correctly verified tokens and secure session lifecycle, safe cookies, CORS/CSP/security headers, SSRF controls, safe upload/download access, outbound constraints, secret segregation/encryption per data classification. Never temporarily disable auth, authorization, rate limits or validation to get a test green.

## III. Quality and operations

### #07 Logging, Monitoring & Observability (P1)
Structured timestamp/severity/service/requestId/traceId logs, redacted secrets/PII, no repeated duplicate exception records. Monitor request/error/latency, dependencies, DB/queue as relevant. Distinguish liveness/readiness and use meaningful alert severity, retention and access. Add tracing, SLOs, Grafana/Loki/Prometheus/OTel only where warranted, not by default.

### #08 Error Handling & Resilience (P0/P1)
Typed/domain vs infrastructure errors; safe client error/status and actionable server diagnostics. Never swallow exceptions or return success after failed writes. Timeouts, cancellation, bounded retry/backoff+jitter only for transient and safely idempotent work; prevent stacked retries/retry storms. If a secondary task fails after a primary commit, report actual primary state; choose outbox/reconciliation when necessary. No unsafe silent fallbacks or infinite hangs. Treat ambiguous post-write timeouts as unknown state until verified.

### #09 Risk-based Tests & Quality Gates (P0/P1)
Build success is not business success. Unit-test invariants, integration-test real DB/transactions/migrations and API boundaries, contract-test impacted clients, negative-test authZ/tenant scope/injection and concurrency/idempotency where risk exists. Benchmark/load-test bottlenecks when needed. Coverage percentage alone is not quality. Never weaken assertions to pass. Report observed PASS/FAIL/NOT RUN/BLOCKED.

### #10 Scale-ready, Not Over-engineered (P1/P2)
Prefer modular monolith, stateless horizontal API scaling where suitable, safe pool/index/query limits, caching with TTL+invalidation and tenant isolation, bounded concurrency/timeouts/backpressure. Add Redis, background queue, object storage or microservices for clear constraints, not speculative complexity. Measure before claiming gains; document triggers for future scale changes.

## IV. Engineering and release safety

### #11 Clean Code & Maintainability (P1)
Consistent naming and small comprehensible modules, avoid duplicated business meaning, needless helpers/interfaces and speculative dependencies. Root-cause recurrent failures rather than stacking bypasses and fallback hacks. No dead-code accumulation, hardcoded secrets or redundant configuration. Comments explain non-obvious decisions; docs track meaningful architecture changes.

### #12 Configuration & External Integrations (P0/P1)
Validate essential config at startup, keep env/secret management server-side, update examples without copying secrets. Do not introduce env flags to conceal a business bug. External integrations (AI, SMS, email, storage, payment) require timeout, safe failure, retries only when justified, quotas/cost protection and observability. Never masquerade as a successful live service when upstream is broken.

### #13 Project-aware Auto Delivery (P0/P1)
Inspect Git status, branch policy, instructions, staged/unstaged/untracked work before edits. Commit/push/merge autonomously only where project profile/policy and actual permissions authorize it and required gates pass. WIP commits/branches may preserve unfinished work without being represented as releasable. No force push, destructive reset, blind conflict overwrite, or assumed default branch. Prod release follows explicit project process; verify local commit, remote push, merge, CI and actual deploy separately.

### #14 Backup, Recovery & Data Protection (P0)
Existence of backup is not evidence of restore capability. Prior to destructive or irreversible changes, establish tested/reasonable recovery, RPO/RTO when required, and approval. Schema rollback differs from data recovery. No unauthorized destructive actions against shared/prod data.

### #15 API & Data Compatibility (P0)
Check routes, DTOs, events, enums, serialized data, database read/write compatibility, running old/new app versions and dependent consumers. Coordinate dual reads/writes or expand/contract where necessary. Remove deprecated schema/endpoints only after migration and dependency evidence.

### #16 Dependency & Supply-chain Security (P1)
Prefer installed, maintained and compatible dependencies. Review source, vulnerability advisories, version/lockfile and licenses when applicable. Never bypass serious security findings for a green build or execute untrusted installation scripts with secrets.

### #17 Data Correctness & Domain Invariants (P0)
Use decimal/integer minor units for money by domain; define units, rounding, timezones and precision. Distinguish null/absent/zero/unknown. Enforce valid state transitions, import/export validation and database constraints. Ask for clarification rather than invent rules governing money, identity, permissions or destructive effects.

### #18 Release Safety & Rollback (P0/P1)
Risk-dependent pre/post deployment checks, controlled cutover and recovery; code rollback cannot resurrect lost DB rows. Do not deploy production solely because push succeeded. Respect deployment approvals, required tests and state of CI.

### #19 Truthful Verification & Evidence (P0)
PASS = executed and passed; FAIL = executed and failed; NOT RUN = not executed; BLOCKED = unable due to a prerequisite/permission. Explicitly distinguish edited, built, tested, committed, pushed, merged, CI-passed, deployed and real user-verified. Mock passing is not live API proof. Do not claim completion from intentions or tool invocation alone.

## V. Risk-based Quality Gate — Option B (approved)

- **Low:** Docs or narrowly scoped refactor; relevant structural/type checks.
- **Medium:** Conventional CRUD/API; relevant build, unit/integration/contract tests.
- **High:** Auth, tenant scope, confidential data, payment, approval, schema migrations; negative-path security/data/concurrency tests, recovery and contracts as applicable.
- **Critical:** Large-scale data loss/leak/financial loss risk; stronger checks, staged rollout, approval and recovery verification.

**Block auto merge/deploy** for P0 regressions; required FAIL, NOT RUN or BLOCKED without approved exception process; unapproved destructive migration; unplanned breaking contract; or missing permissions. WIP preservation on allowed branches is not release. Do not remove tests or disable protections to manufacture PASS.

## VI. Agent execution protocol

1. **Inspect** repo, instructions, Git worktree, actual stack, endpoint-to-consumer mapping, schema, history, tests.
2. **Classify** task risk and affected P0/P1/P2 rules, scope and authorization.
3. **Design** the smallest correct solution and failure/recovery strategy; ask on blocking/high-impact ambiguity.
4. **Implement** root-cause fix, migrations/tests/docs when relevant, preserving unrelated changes.
5. **Verify** relevant build/type/lint, unit/integration/security/contract, negative paths and data safety.
6. **Deliver** using project-authorized Git/release process only after quality gate.
7. **Report** files, outcomes, exact checks, risk, unknowns, Git/push/merge/CI/deploy evidence separately.

## VII. Definition of Done

- [ ] Business invariants, negative paths and scope correct.
- [ ] Existing API/data contracts preserved or migrated safely.
- [ ] Server-side auth/authZ, input validation, confidential data controls intact.
- [ ] Transactions, concurrency/idempotency, pagination and DB changes appropriate.
- [ ] Audit, operational logs, errors, retries and external integrations safe.
- [ ] Focused quality gates passed; failures/unrun checks explicitly identified.
- [ ] Git/release actions within permission and verified separately.
- [ ] No known P0 outstanding, no fabricated production success.

## VIII. Absolute prohibitions

Never bypass authentication/authorization, silently delete or reset data, expose secrets, hide upstream failures using mock success, swallow exceptions, weaken tests, overwrite others' work, force push, auto-deploy unapproved production changes or claim unobserved testing/delivery. Coordinate UI logic with react-frontend-engineering and presentation with ios-inspired-ui-design.