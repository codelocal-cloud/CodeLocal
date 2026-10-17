# Feature Parity Before Replacement — migration playbook

Use for Razor SSR → React CSR, Vue → React, UI V1 → V3, large route rewrites, screen rebuilds and comparable migration tasks. Do not declare progress from screen counts alone.

## Pre-migration inventory (source-of-truth matrix)

Each original route and handler gets a row with:

| Field | Required evidence |
| --- | --- |
| Original route / handler | Actual route path, GET/POST/other method, parameter/query names |
| User journey | How user arrives and which task they complete |
| UI elements | Tables, fields, buttons, menus, tabs, dialogs, exports, upload/download |
| Business effect | API operation, validation, state transitions, audit |
| Auth/data scope | Role, resource/tenant/ownership and disabled/action visibility |
| Original data | Data fields, filters, pagination, sorting, default values |
| New React target | Route/component/hook/client, API endpoint contract |
| QA | Positive/negative tests, visual/keyboard/mobile states, status PASS/FAIL/NOT RUN/BLOCKED |

Check references from menus, breadcrumbs, deep links, reports and notifications. A Razor GET/POST PageModel handler is not equivalent to a pretty React route until its behavior is mapped and verified.

## Migration sequence

1. Inventory current routes + GET/POST handlers + consumers. Preserve worktree.
2. Verify existing API coverage and identify missing backend contracts with backend-engineering.
3. Implement type-safe API integration; no frontend replication of authoritative business logic.
4. Implement full interactions in new route (forms, tables, confirmations, modals, edge states).
5. Visualize using ios-inspired-ui-design without deleting actions or losing data.
6. Check parity per screen: functional, authorization, data, navigation, error and accessibility.
7. Switch or deprecate original implementation only with cutover/rollback plan and verified critical flows.

## Completion states

- **INVENTORIED:** Source behavior mapped; no claim of implementation.
- **IMPLEMENTED:** React route and logic exist; not yet validated.
- **VERIFIED:** Required real or appropriate integration tests passed, contracts matched.
- **CUTOVER:** Old entrypoints safely replaced/rerouted as intended with rollback.
- **BLOCKED:** Missing API, access, test environment or old behavior unknown.

Keep these separate from quantitative route coverage. Do not say 100% until required functional parity and release checks are observed. When functionality is intentionally retired, require an explicit approved exception and affected-consumer plan.

## Smoke test scenarios for each screen class

- List: server paging, filters, sorting, bulk, export, errors, 401/403/429.
- Form: create/edit, validation, dirty-state retention, 409 conflict, double-submit and permission.
- Detail: direct deep link, missing record, wrong tenant, navigation back.
- Workflow: permitted state transitions, reject path, auditable effects, concurrent updates.
- Media: upload/download, file policy, permissions, progress/errors.
- Dashboard: authorized API totals, zero-vs-unknown, data freshness and no fake KPIs.
- Style: viewport and critical actions at 390/768/1024/1440, low-height, zoom where relevant.

A passing mock fixture proves only fixture interactions; live backend/UAT verification remains separate.