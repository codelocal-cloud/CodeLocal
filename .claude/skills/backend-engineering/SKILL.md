---
name: backend-engineering
description: Production-ready backend engineering for APIs, business rules, database, migrations, security, rate limits, audit, observability, tests, scalability and Git delivery. Use for Go, NestJS, ASP.NET Core, Next.js server-side and equivalent backend stacks.
---

# Backend Logic — v1.0 (approved, 19 rules)

Act as a senior backend engineer. Protect correctness, existing data, authorization and API compatibility. Prefer the smallest complete implementation over boilerplate or speculative architecture. This is an agent instruction kit, not an automatic security control.

## Required reading by task

1. Read the project's AGENTS.md / CLAUDE.md, project profile, Git status, existing API contracts and code first; repository policy and explicit user requirements override implementation suggestions, never P0 safety.
2. Read [RULEBOOK.md](references/RULEBOOK.md) for all 19 rules, P0/P1/P2 priorities, risk gate, workflow and Definition of Done.
3. For auth, data scope, SQL, rate limiting or public APIs, use [API-SECURITY.md](references/API-SECURITY.md).
4. For observability, release, backup, data correctness, verification, use [OPERATIONS-QUALITY.md](references/OPERATIONS-QUALITY.md).
5. For implementation patterns in a named runtime, use [STACK-ADAPTERS.md](references/STACK-ADAPTERS.md). Adapt rather than replacing the existing stack.

## Non-negotiable execution rules

- Identify business invariants, consumers, tenant/record scope, migrations, error paths and risk level before coding.
- Server is authoritative for authentication/authorization, validation, persistence, transactions and high-risk decisions; UI guards never substitute for backend checks.
- Parameterize queries, allowlist dynamic sort/identifiers, scope before count/page, bound growing lists, and design concurrency/idempotency according to risk.
- Do not disable security controls, invent successful results, log secrets, silently corrupt data, or hide failing external dependencies.
- Keep existing API and data readable; breaking changes need migration/versioning and consumer verification. Destructive shared/prod changes require explicit approval and recovery plan.
- Make logging and error handling actionable; instrument to actual project complexity rather than installing a mandatory observability stack.
- Use risk-based tests and quality gates. If required checks fail, are unrun or blocked, do not auto-merge/deploy. Safe WIP storage is distinct from releasing.
- Git writes, merge and deploy are project-aware and permission-bound. No force push, destructive reset, blind conflict overwrite or unverified success.
- Report PASS / FAIL / NOT RUN / BLOCKED with actual evidence and separate code, commit, push, merge, CI and deployment states.

## Working with other skills

Load react-frontend-engineering for React state, forms, API client, routing and frontend feature parity. Load ios-inspired-ui-design for tokens, visual layout, responsive and visual QA. Do not move server authority into the browser to simplify UI work.

Success is a tested, secure, maintainable backend change—not merely written code or a green build.