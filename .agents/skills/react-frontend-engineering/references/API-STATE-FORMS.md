# React state, API, forms, tables and session — implementation checklist

Read with [RULEBOOK.md](RULEBOOK.md). This is an execution checklist, not a mandate to install a library.

## State ownership matrix

| Category | Source of truth | Frequent failure | Safe pattern |
| --- | --- | --- | --- |
| Server data | Query/cache layer, backend | Global duplicate cache | Stable scoped query keys, invalidation |
| UI control | Local component/feature state | Global everything | Lift only shared interactions |
| Navigation | URL/search params where meaningful | Filters lost on refresh | Validated parse/serialize, history tests |
| Form | Form controller/local draft | Refetch erases dirty fields | Track dirty and reconcile deliberately |
| Session | Backend session/account scope | 401/403 confusion, cache leaks | Auth lifecycle, purge private caches on switch |

Next.js Server Components and route handlers must retain server-only credentials; React/Vite browser code cannot be trusted with privileged secrets. Never assume HTTP success without payload/contract validation where meaningful.

## API request & mutation flow

1. Define typed request/response from an actual backend contract.
2. Include authenticated scope and all list filters/search/sort/page in query keys where applicable.
3. Abort, guard or ignore stale responses; do not let older request replace later state.
4. Distinguish initial loading and background refresh; preserve existing table content when appropriate.
5. On mutation: disable duplicate submit where appropriate → call API → verify confirmed outcome → update or invalidate relevant cache → show success.
6. On 401 attempt project-supported safe re-auth/refresh, not infinite redirect. On 403 show denial and do not logout automatically. On 409 show conflict/reload. On 422 map field errors. On 429 respect retry guidance.
7. For risky mutations after network timeout, result may be unknown; use status lookup or server idempotency before retry.
8. Optimistic changes require allowed business risk, rollback and explicit reconciliation. Never optimistic-declare payment/approval finality.
9. Account/tenant switch and logout invalidate/clear all private cached records and derived UI state.

## Form acceptance

- Default values, existing record load, server field errors, submitting/disabled states.
- Validations (required, bounds, cross-field), dirty/touched/error descriptions and focus.
- Failed API keeps input; refetch does not overwrite unsaved changes.
- Save/cancel behavior and unsaved changes warning as needed.
- Conflicts detect record versions and refuse blind overwrite.
- Multi-step forms and upload handle partial fail, size/type, progress and safe cancellation.

## Table acceptance

- Server paging (not slicing one server page locally), stable sort, search debounce, filters and URL sync when relevant.
- Correct page reset when filter changes; empty, initial load, background refresh, error/retry and 403.
- Selection semantics: selected current page vs all filtered result; confirmations for bulk/destructive actions.
- Export semantics: authorized records, chosen scope, large export as background job if backend requires.
- Cursor feeds may not know total count; UI must not fabricate a page count.

## Authentication session UX

- Initial session check before protected data render; no auth flash.
- Refresh single-flight when supported, bounded handling on revocation/expiry.
- No device-limit bypass, no default disabling of session control.
- Keep non-sensitive unsaved drafts through re-auth only when safe; clear private state on user/tenant switch.
- Verify logout, 401, 403 and cache isolation with negative tests.

## Test examples (pick relevant)

- Two search requests return in reverse order: latest query remains displayed.
- GET list returns pageSize=25/totalCount=250: UI pages server results, not those 25.
- POST response 409: editor retains fields, displays conflict and does not overwrite.
- Token expires with three requests: one refresh or controlled expiry path, no loop.
- Tenant/account switch: cached previous tenant data is never displayed.
- Button hidden by permission is not counted as server authorization.
- Network timeout after financial POST: UI does not auto-resend blindly.
