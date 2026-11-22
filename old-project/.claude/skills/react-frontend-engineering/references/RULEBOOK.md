# React UI Logic Rulebook v1.0 — APPROVED

Scope: React/TypeScript, React + Vite CSR, Next.js client/server boundaries and compatible React systems. **12 approved groups**. Philosophy: Smart Separation · Smart State Management · Feature Completeness · Production Quality.

**P0:** Correct data, privacy/session boundaries, working feature behavior, no silent mock success, contract compatibility, truthful test/delivery. **P1:** Smart architecture, loading/errors, forms, accessibility, routing and risk-appropriate tests. **P2:** Measured memoization, virtualization, advanced caching and UI optimizations. Avoid installing technology without evidence; current project conventions take precedence over library preferences.

## #01 Smart Component Architecture (P1)
Split rendering, reusable hooks, feature interactions and API access when complexity requires. Do not force one hook/service/types file per component or bloat 1000-line components. Reuse installed abstractions, group by feature/domain when justified, keep explicit props and testable boundaries; avoid unjustified any/casts. For Next.js distinguish server-only, Server Component, Client Component, route handlers and actions; React/Vite remains CSR when so architected. Never rewrite rendering model to satisfy a skill.

## #02 Smart State Management (P1)
Keep **server state** in the data-fetching/cache layer, **UI state** local unless shared, **URL state** in addressable search params when relevant and **form state** in form controller. One source of truth per value. Avoid server-data duplication in global stores, derived-state useEffect loops and unbounded global Context. Use installed tools; TanStack Query, Zustand, React Hook Form, Zod are suggestions, not mandatory. Stable keys include filter/scope; cancel or ignore stale results, dedupe and invalidate after writes.

## #03 API Integration & Contract Safety (P0/P1)
Use existing API client, validate DTO/response assumptions, prefer OpenAPI generated types where supported, do not pretend guessed schemas are verified. Handle 400/422, 401, 403, 404, 409, 429, 5xx, network and timeout differently. Keep auth/session secrets server-side and private caches separated across user/tenant. Do not silently fall back to mocks after a live error or show success after failed mutation. Avoid unsafe retries for non-idempotent writes. Update query cache only after confirmed result/clear optimistic rollback, preserve old contract or coordinate backend versioning.

## #04 Smart Form Logic & Validation (P0/P1)
Use correct initial values, required/type/cross-field validation and server error mapping. Frontend validation helps UX; backend remains authoritative. Track dirty/touched/submitting, prevent unintended duplicate sends, preserve edits on failure/refetch and warn before destructive unsaved navigation where meaningful. Handle stale versions/409 without blind overwrite. Multi-step and upload forms need partial-state/error/progress recovery when supported. Prefer installed form/schema tools (React Hook Form + Zod only as an option). Declare success only on reliable API confirmation.

## #05 Smart Data Tables & Hybrid Pagination (P0/P1)
Growing lists use backend paging, not whole-database fetch/slice; offset/page-size for admin jump pages, cursor/keyset for large feeds. Query parameters (search, sort, filters, page) are validated against real API and reflected in URL where useful. Debounce high-frequency input appropriately; avoid stale request races. Provide loading, empty, error/retry, count only if contract supports, stable selection semantics (page vs all matching), safe bulk actions and export scope. Never invent missing server filter/pagination. Preserve authorized scope and avoid frontend N+1 per row.

## #06 Routing, Navigation & User Context (P0/P1)
Navigation/menu/breadcrumb routes must exist, direct deep-link refresh must work with server fallback/config. Retain list filters/scroll/page on back navigation when workflow needs it; support browser history, 404/403/error pages and avoid redirect loops. Frontend route guard is UX, not security; real authZ is backend. Do not erase unsaved form drafts inadvertently or replace router just from preference.

## #07 Authentication & Authorization UX (P0/P1)
Verify session before protected content, avoid unauthorized UI flicker. Refresh only if backend supports it, consolidate simultaneous refreshes, prevent storms and redirect loops. Distinguish 401 (authentication) from 403 (permission). Logout/account/tenant switch purges private query caches and session state. Preserve in-progress form data on re-auth only if safe; do not persist sensitive drafts irresponsibly. Changes to roles, session revocation and device policies must be reflected correctly. Never circumvent device limits or backend auth controls to make login succeed.

## #08 Async UX & Error Recovery (P0/P1)
Model idle, initial loading, background refresh, submit, success, error and unknown result when applicable. Keep existing content during refetch where appropriate; do not allow old responses to overwrite newer state. Optimistic update only when reversible with rollback and within business risk; never fake approval/payment/sensitive action success. After timeout on mutation, result may be unknown; offer status check or safe recovery rather than blind retry. Respect 429 Retry-After when given. Avoid infinite loading overlays.

## #09 Performance & Rendering Efficiency (P1/P2)
Architect state/render boundaries before blanket memoization. Apply memo/useCallback/useMemo, lazy/code splitting and virtualization when profiling/constraints justify them. Server pagination precedes giant list rendering. Deduplicate requests, manage bundle weight and image/font loading; do not overcache stale/wrong backend data. Respect Next.js server/client boundaries. Claim improvements only with measured baseline and outcome.

## #10 Accessibility & Interaction Logic (P1)
Semantic HTML and labels, keyboard operability, focus-visible, modal focus trap/restore and appropriate Escape behavior, accessible form error associations, keyboard tabs/menu/combobox/table interactions, ARIA feedback for meaningful async status, meaning beyond color and safe destructive confirmation. Target WCAG 2.2 AA when applicable. React owns interaction accessibility; ios-inspired-ui-design owns visual contrast/touch targets/typography.

## #11 React Testing & Interaction Quality Gate (P0/P1)
Test what the user does, not just snapshots. Component tests for forms, tabs/dialogs and validation; API integration tests for authentication, paging, failures, stale requests and mutation cache; E2E for critical workflows in available environments. Use installed Vitest/Testing Library/Playwright/MSW or equivalents; no forced stack. Mock success proves UI under a fixture, not live backend/production. Do not weaken assertions to turn FAIL into PASS. Report PASS/FAIL/NOT RUN/BLOCKED.

## #12 Feature Completeness & Safe Delivery (P0/P1)
A UI is complete only when required actions work with authorized real APIs, validation, error/loading/empty states, routing, responsive and tests as applicable. During migration, enforce **Feature Parity Before Replacement**: inventory old routes, GET/POST handlers, buttons, forms, permissions, API calls, exports, dialogs, navigation and negative paths; map to new implementations, test before cutover. Do not remove working old UI before safe parity or approved exception. Project-aware commit/push/merge/deploy; no 100% claim from route/file counts.

## Collaboration boundaries

**backend-engineering** owns server-authoritative authZ, persisted invariants, APIs, DB, transactions, audit and quotas. **react-frontend-engineering** owns frontend state, API interaction, form UX, routing, session UX and behavioral accessibility. **ios-inspired-ui-design** owns design tokens, glass/material, layout, typography, visual states, responsive and visual QA. Coordinate changes crossing boundaries; do not fake backend behavior in the browser to complete a screen.

## Risk-based Frontend Quality Gate (Option B)

- **Low:** isolated content/refactor with no behavior impact; scoped lint/type tests.
- **Medium:** CRUD/table/form; required interaction/API tests, errors, empty/loading states, typecheck.
- **High:** auth/session, permissions, financial/approval, sensitive upload, migration of critical screens; negative path, contract, race and E2E/integration checks where applicable.
- **Critical:** possible cross-tenant leak, duplicate financial mutations or broad loss of workflow; strong verification, recovery and approval before release.

Block automatic merge/deploy if P0 feature/security/contract regression, required tests FAIL/NOT RUN/BLOCKED without approved process, permissions missing, or feature parity unverified where required. May save clearly marked WIP on authorized branch; not equivalent to release.

## Agent execution workflow

1. **Inspect:** Git/worktree, instructions, UI feature/routes, stack, API contracts, existing tests and design system.
2. **Scope:** Define user journey, APIs, roles, state flow, risk level and acceptance.
3. **Design:** Smart boundaries, server/UI/URL/form state, failure paths and cache strategy.
4. **Implement:** Real actions and data, meaningful async state and compatibility; protect other agents' changes.
5. **Verify:** Typecheck/lint/build, interaction, negative paths, network/runtime, responsive with ios-inspired-ui-design.
6. **Deliver:** Authorized project-aware Git and release only after gate.
7. **Report:** Changed files, confirmed behavior, test status, dependencies, risk, Git/CI/deployment independently.

## Definition of Done

- [ ] Real user actions, routes and APIs work as required, no mock success.
- [ ] Feature parity proven for replacements/migrations or explicit approved gap.
- [ ] State, cache, session/tenant boundaries and 401/403/409/429 behavior correct.
- [ ] Forms protect edits, list pagination/filter/sort/export scoped and bounded.
- [ ] Loading/error/empty/retry, navigation and keyboard/focus behavior appropriate.
- [ ] Visual responsive/QA coordinated, required tests passed and recorded.
- [ ] No unapproved merge/deploy or inaccurate claim of completion.

**Absolute bans:** Fake buttons/CRUD, fabricated success/mock fallback, unsafe token storage, backend security bypass, hidden lost features, blanket any casts, speculative global stores, retrying high-risk mutations unsafely, silent API contract breaks, unverified PASS.