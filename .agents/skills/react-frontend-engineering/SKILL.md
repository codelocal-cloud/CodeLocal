---
name: react-frontend-engineering
description: Complete production React/TypeScript feature behavior—components, server/UI/URL/form state, typed API integration, forms, tables, pagination, routing, session UX, async recovery, accessibility interactions, tests and migration parity. Works with React Vite and Next.js.
---

# React UI Logic — v1.0 (approved, 12 rules)

Act as a senior React engineer accountable for **working features**, not screenshot-shaped mockups. Use Smart Separation, Smart State Management, API Contract Safety and Risk-based Feature Completeness.

## Read relevant references

- [RULEBOOK.md](references/RULEBOOK.md): authoritative 12 rules, priorities, workflow, testing, delivery and Definition of Done.
- [API-STATE-FORMS.md](references/API-STATE-FORMS.md): server/UI/URL/form state, API auth, retries, forms and tables.
- [FEATURE-PARITY.md](references/FEATURE-PARITY.md): migration screen inventory, parity matrix, acceptance tests, safe cutover.

## Agent behavior

1. Inspect actual stack, router, components, API client, state/form libraries, authorization, existing workflows and repo instructions before editing. Preserve Git/other-agent work.
2. Separate render, interactions and data access only to the level complexity warrants; no compulsory service/hook for every component.
3. Keep server state in existing data-fetching solution, local UI state local, URL state in route/search params where useful and form state in forms. Avoid duplicate sources of truth.
4. Use actual API contracts, typed DTOs and safe cache boundaries across users/tenants. Do not fake success, swallow errors, bypass auth or silently substitute mock data.
5. Forms need correct validation, submit/error/dirty/concurrency handling; bounded GetList must be server-paginated, with meaningful filters, sorting, exports and scopes.
6. Handle 401 vs 403, session recovery, stale requests, timeouts, ambiguous writes, loading/empty/error/retry and route navigation deliberately.
7. Accessibility interaction (focus, keyboard, ARIA, dialog behavior) is functional engineering. Visual tokens/responsive are owned by ios-inspired-ui-design.
8. For migration, inventory routes/screens/actions/handlers/API/permissions/states and prove parity before replacing old implementation. Generated routes alone are not complete features.
9. Test user behavior and negative paths, classify risk, stop auto-merge/deploy on P0 or required checks not passing. Report PASS/FAIL/NOT RUN/BLOCKED and distinguish Git/deployment states.

Load backend-engineering if authoritative business rules, API contracts, database, authorization or server changes arise. Load ios-inspired-ui-design for visual structure and responsive QA. Follow the actual project stack; no blanket mandate for TanStack Query, Zustand, React Hook Form, Zod or a specific router.