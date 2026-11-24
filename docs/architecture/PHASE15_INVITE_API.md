# Phase 15 — Account-scoped Rust Invite/Referral API (2026-11-24)

## What is implemented

`GET /api/v1/invite` now serves the unchanged Next.js page at `/dashboard/invite`, using the same core DTO as archived Go.

- Requires authenticated session and the active account's actual `referralCode`. The client cannot supply an arbitrary referral code to enumerate unrelated users. HTTP 401 for anonymous access.
- The invite link is built from the explicitly configured and validated `CODELOCAL_BETA_PUBLIC_BASE_URL` (HTTPS or loopback HTTP) and the user's ASCII referral code. Missing/invalid public beta origin returns 503 rather than linking to production or an attacker-provided host.
- PostgreSQL reads only direct invitees whose normalized `referred_by_code` matches the authenticated account's referral code. Email output is **masked** with the archived Go rule, and IDs/full emails/passwords are never returned.
- Invited account device credentials are looked up in PostgreSQL in one scoped query, excluding revoked devices. Redis `MGET` checks existing **beta** runtime-presence leases; an invitee appears `runtime_online` only when a lease is present. Status otherwise is `offline`, and `activeCount` reflects real currently online invitees.
- `invitedBy` uses the recorded inviter code or `Root account`. `directCount`, `activeCount`, `members[].emailMasked/initial/joinedAt/status` and `inviteLink` match the existing Next.js contract.
- Requests fail closed on PostgreSQL/Redis errors. The beta route currently caps an invitation list at 500 and total associated devices at 3000; it returns 503 instead of claiming a truncated member count. Proper pagination is pending.

## Testing and safety gates

- Local Rust tests cover referral-code validation, privacy-safe email masks/initials, exact link DTO and no leaked account/device secrets, plus an anonymous 401.
- Real PostgreSQL+Redis integration test (GitHub Actions): after logging in as the inviter, confirm empty invite list, insert a separate referred user, confirm one masked/offline invitee, pair the invitee's device, establish beta runtime lease, verify online count/status, clear presence, verify offline status and revoke. Verify unrelated referral codes cannot enumerate the inviter's members.
- No fake `mcp_active` activity: Cloud MCP active-session tracking from Go has **not** been ported. The only active status currently emitted by Rust is `runtime_online`, backed by the Redis device lease. Some real MCP-only activity will remain untracked until Cloud MCP porting; this is explicitly partial parity.
- This uses no production user data or service, and does not modify the original Next.js frontend, archived Go, or GitHub main.

## Remaining parity

Port global MCP-online signals, invitation pagination, runtime aggregate performance tuning, full referral activity, Cloud MCP authorization and release validation. A successful Invite CI run is not evidence that the rest of the Go backend is already ported.
