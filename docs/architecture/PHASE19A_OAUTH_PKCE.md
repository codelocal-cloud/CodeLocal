# Phase 19A — OAuth authorization-code and PKCE S256 (2026-11-24)

## Implemented

- OAuth authorization-server metadata at /.well-known/oauth-authorization-server advertises only implemented grants.
- POST /register registers bounded public clients with exact callback URIs. HTTPS and loopback HTTP only; credentials, fragments, reserved callback parameters, private-use URI schemes and insecure external HTTP are rejected.
- GET /api/v1/oauth/authorize-context serves the existing Next.js consent screen. POST /authorize verifies the bound browser session, risk flag, CSRF, registered redirect URI, S256 challenge, scope and the configured beta resource. It issues a random authorization code expiring in five minutes and records a consent audit.
- POST /token exchanges a valid code once for a one-hour opaque access token. The transaction checks the account security version, client, exact redirect, resource, expiry and PKCE proof. Invalid proof does not consume the code; successful consumption and token creation are atomic.
- Codes and access tokens are persisted as SHA-256 hashes only, in isolated beta migration 007. POST /revoke deletes an access token for its bound public client. Responses are not cacheable.
- Public registration and exchanges have Redis-backed throttles and bounded database quotas. Global throttles are conservative beta protections, not a production-grade per-IP abuse-control system.

## Deliberate limitations

Only authorization_code and mcp:tools are supported. Refresh rotation, offline_access, OIDC, private-use callback schemes, resource-server bearer authorization, protected-resource metadata and the Cloud MCP transport are pending. Issuing a token does not enable any MCP tool or remote action yet. The Next.js consent UI is preserved. Client registration supports the standard redirect_uris array; legacy singular redirect registration is not supported yet.

## Acceptance

Local unit tests verify the RFC 7636 S256 vector, verifier/challenge and callback policy. The expanded real-store test covers public registration, consent context, CSRF rejection, mismatched callback and audience, wrong proof, successful exchange, replay rejection, hash-only token storage and revocation. Run GitHub Actions against the exact commit for acceptance. Migration 007 must be applied explicitly to existing isolated beta database volumes. No production migration is performed.
