# API security & abuse-control checklist

This reference is normative when adding/changing backend endpoints. Values below are **illustrations, not universal production defaults**; choose through threat model, observed traffic and capacity.

## Endpoint classification

| Category | Authentication & scope | Limit mechanisms | Important tests |
| --- | --- | --- | --- |
| Login/MFA/recovery | Pre-auth; account enumeration protection | IP burst + identifier-based failure/cooldown; monitoring | valid/invalid password; shared IP; 429; retry; no enumeration |
| Public GET | Anonymous or API key | per-IP/key burst + sustained; per-day usage quota for public/high-cost reads | burst, daily limit, reset, changing headers, concurrency |
| Authenticated GetList | user/tenant/record scope | bounded page size; optional request quotas | scope-filtered count, IDOR, max page size, last page |
| Search/export/report | user/tenant + action permission | concurrency, time/budget quota, maximum export size | authorized scope, N+1, volume, cancellation, retries |
| Public mutation/webhook | signed/authenticated as appropriate | rate cap, payload cap, replay prevention/idempotency | forged signature, replay, duplicate writes |
| Upload/presign/AI endpoint | user/tenant/action scope | cost/size/quantity quotas, concurrency | unsupported files, S3 receipt, budget, secrets |

## Decision process for rate limits

1. Identify the resource being protected: account security, expensive DB query, third-party API budget or bandwidth.
2. Choose **partition key** deliberately: API key/user/tenant + source IP where useful; do not trust `X-Forwarded-For` outside configured trusted proxies.
3. Define **burst limit**, **sustained limit**, and (where needed) a **daily or rolling-24h quota**; specify UTC window vs rolling window, concurrency and cost weighting.
4. Use a shared atomic store/gateway across replicas. Test atomic consumption and reset under parallel requests.
5. Exempt only explicitly documented probes/internal authenticated paths. Use HTTP 429 with Retry-After and a safe error envelope. Ensure metrics and logs contain no secrets.
6. Model shared users (NAT, organizations), reverse proxies, per-account lockout abuse and deployment restarts. Design legitimate recovery.
7. Validate quota pricing and business needs rather than copying arbitrary numbers from other apps.

## Server safety

- Positive allowlist of validation rules; strict authZ per request AND per record; filter before COUNT.
- Use prepared SQL/ORM bindings, explicit identifier allowlists, safe escaping only for display, strict input size limits.
- Cookies: HttpOnly, Secure on HTTPS, SameSite as suitable + CSRF on state change.
- Tokens: verify issuer/audience/signature/expiry/rotation with vetted packages.
- Uploaded files: magic-byte checks where feasible, extension/size bounds, malware scanning if threat model requires, permission on download, presigned links scoped and short-lived.
- Do not serialize credential, PII or record fields without explicit permission. Do not leak secrets via errors/logs.
- Use transactions/idempotency keys where duplicate writes could be dangerous; audit actor, action, target and trace ID securely.

## Minimum negative-path API review

No auth -> 401; wrong role/record -> 403 or privacy-safe 404; invalid input -> 400/422; competing write -> 409; over quota -> 429; timeout/upstream outage -> suitable 5xx/503; unsupported duplicate/replay -> safe idempotent response/rejection. Never return 200 as a blanket catch handler.
