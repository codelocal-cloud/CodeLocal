# Plane Community Edition MCP gateway

This is a minimal Railway deployment wrapper around the official `makeplane/plane-mcp-server` image. It starts **only the header/PAT authenticated MCP** server at `/http/api-key/mcp` and deliberately does **not** initialize the OAuth transport; the CE instance cannot supply Plane OAuth application credentials.

Railway service: `PlaneMCP`, project `codelocal.cloud`, environment `plane`.

Variables:

- `PLANE_BASE_URL=https://plane.codelocal.cloud`
- `PLANE_INTERNAL_BASE_URL=https://plane.codelocal.cloud`
- `PORT=8211`
- `LOG_PAYLOADS=false`

Every connection **must** send `Authorization: Bearer <PAT>` and `x-workspace-slug: <workspace>`. Never put a shared administrator PAT in the Railway environment, the repo, URL, or per-user defaults. The gateway validates each user's PAT and workspace with the Plane API, and Plane permissions still apply.

Keep `PlaneMCP` accessible over HTTPS. An unauthenticated request to `/http/api-key/mcp` must fail with 401/403; successful authentication and tool discovery must be verified using a separate limited token.

CodeLocal Cloud's Plane connector only permits exact allowlisted hosted Plane and CodeLocal PlaneMCP endpoints and encrypts each user's PAT separately.
