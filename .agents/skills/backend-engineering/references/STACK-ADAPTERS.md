# Framework adapters (choose what the repository uses)

This reference is a **mapping**, not a command to install or migrate frameworks. If the user says “Net.js”, confirm if they mean **NestJS (Node/TypeScript)** or **ASP.NET (.NET)** only when their choice changes implementation.

| Concern | Next.js (Route Handler/BFF) | NestJS | Go | ASP.NET Core |
| --- | --- | --- | --- | --- |
| Input schema | Zod/Valibot or existing validated schema server-side | DTO + global ValidationPipe, whitelist/reject extras | Decode/validate with explicit size ceilings, custom types | Model validation / endpoint filters |
| Authentication | secure session validation in handlers/actions | guards + strategy/session | middleware/context identity | authentication/authorization policies |
| Record permissions | check tenant/owner in DB call | guard/service + scoped repository | scoped queries with identity in context | service/endpoints with filtered DB access |
| SQL injection | ORM bind or prepared statements | safe ORM/query builder, never untrusted raw SQL | `database/sql`, `sqlx`, pgx params | EF parameterization/ADO.NET params |
| Server pagination | validate query + scoped DB LIMIT/cursor | typed query DTO + scoped repository | query params + scoped SQL | endpoint filter/query object + scoped SQL |
| Rate limiting | gateway/edge or shared limiter; not per-instance alone | throttler/gateway with shared Redis or equivalent | middleware with shared backend | built-in limiter with shared quota for multi-replica |
| CSRF | appropriate for cookie-backed mutations/server actions | appropriate for cookie-backed mutations | implement for cookie-backed mutations | antiforgery for cookie-backed mutations |
| Test | route/integration & browser | unit/e2e (HTTP app) | `go test`, `httptest`, race where needed | xUnit/integration/TestServer |

## Architecture

- Next.js may serve UI and BFF. If a dedicated backend exists, keep secrets and durable business authorization on the backend. Do not make browser → database direct calls.
- NestJS: keep controller thin; service owns business invariants; persistence layer owns safe queries; share types/contracts intentionally, not by leaking ORM entities.
- Go: small packages organized by capability; pass context through DB/external calls, close rows, check errors, use timeouts and cancellation. Avoid over-frameworking.
- ASP.NET Core: separate DTO/endpoint logic and persistence as the project grows; don't create a large clean-architecture scaffold for a two-route service.
- Shared quotas across multiple Next.js/Go/NestJS/.NET instances should rely on a centralized store or managed API gateway. Rate limit middleware without correct persistence does **not** guarantee global or daily caps.
