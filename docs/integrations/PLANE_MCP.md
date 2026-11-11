# Plane + CodeLocal: recommended integration

**Architecture:** Keep Plane as a separate project-management service; use the official Plane MCP server from CodeLocal. Do not fork Plane into CodeLocal. Keep task data and Plane's DB outside CodeLocal's DB.

## Primary option: CodeLocal Tasks or Plane Cloud, CodeLocal Cloud MCP, encrypted workspace-scoped PAT

This is the default for remote AI agents and multiple user devices. It works without an online CodeLocal device and does not depend on Plane hosted OAuth callback allowlisting.

1. In **[CodeLocal Tasks](https://plane.codelocal.cloud)**, open **Profile Settings → Personal Access Tokens**, or use Workspace Settings → Access Tokens. Create a token with the minimum required permissions. Plane Cloud users follow the same flow in their Plane account.
2. Find your workspace slug in the Plane URL, e.g. `https://plane.codelocal.cloud/example-team/` → `example-team`.
3. In CodeLocal, open **Dashboard → Plugins → Explore → Plane Project Management → Install → Configure**.
4. Choose **CodeLocal Tasks (self-hosted)** (default), whose vetted endpoint is `https://planemcp-plane.up.railway.app/http/api-key/mcp`, or choose **Plane Cloud** for `https://mcp.plane.so/http/api-key/mcp`. Enter the workspace slug and paste your own token; never put it in a URL.
5. Approve the requested permissions. CodeLocal probes MCP discovery; it reports ready only if discovery works.
6. In Chat, use `list_plugin_tools` and `call_plugin_tool`; review and approve changes before any remote write.

CodeLocal stores the PAT and slug together in its account-bound encrypted Cloud credential store. Plane work items remain in Plane. CodeLocal's Cloud MCP client attaches `Authorization: Bearer <PAT>` and `x-workspace-slug: <slug>` only when sending to one of the two **exactly vetted PAT MCP endpoints**. It does not create a global administrator token.

**Migration:** Built-in Plane v0.1.0 used OAuth and v0.2.0 only supported hosted Plane. v0.3.0 supports CodeLocal Tasks and Plane Cloud with separate per-user PAT credentials. Users switching endpoints must reconnect using a token issued by that Plane instance.

## Current CodeLocal Tasks installation (Railway, self-hosted)

The active Plane Community Edition instance is now **[https://plane.codelocal.cloud](https://plane.codelocal.cloud)**.

- Railway project: [`codelocal.cloud`](https://railway.com/project/d5286319-d1c5-4b51-9022-ac3211ed8f2a), **environment `plane`** (not the existing `dev` environment)
- Services: `PlaneApp`, `PlaneDB`, `PlaneCache`, `PlaneQueue`, and **`PlaneMCP`** (official PAT-only gateway), plus object bucket `PlaneFiles`. They have separate credentials and persistent data from CodeLocal's `dev` environment.
- Public URL: `https://plane.codelocal.cloud`; Railway's `https://planeapp-plane.up.railway.app` is an emergency fallback, **not** the canonical login/invitation URL.
- PlaneApp variables: `DOMAIN_NAME=plane.codelocal.cloud`, `APP_PROTOCOL=https`, `WEB_URL=https://plane.codelocal.cloud`, `CORS_ALLOWED_ORIGINS=https://plane.codelocal.cloud`, `PORT=80`, `SITE_ADDRESS=:80`, and `ENABLE_SIGNUP=0`. Keep `SITE_ADDRESS=:80`: Plane's internal proxy listens on port 80 and Railway/Cloudflare terminate HTTPS.
- The original admin account was provisioned by Plane's boot script; find `ADMIN_PASSWORD` securely at Railway → environment `plane` → `PlaneApp` → Variables. Never paste it into chat or check it into Git.
- Only invited users should join until SMTP and the desired authentication methods are configured in Plane's God Mode. Having a public sign-in page does **not** mean anonymous users can create accounts.
- Run `PLANE_BASE_URL=https://plane.codelocal.cloud bash deploy/plane/verify.sh` after changes; then test a real admin login, inviting a test member, and a test work item.
- **MCP:** CodeLocal Cloud can connect to the dedicated self-hosted `PlaneMCP` service (PAT-authenticated, 30 tools). Each CodeLocal user supplies a separate limited token and workspace slug. Never share the Plane admin password or a universal PAT with users. Authenticated MCP tool discovery using a real user PAT still needs to be tested after creating an account/token.

The domain's Railway DNS record may display `REQUIRES_UPDATE` if Cloudflare is proxying the record and hiding its target CNAME. Check that the HTTPS site and `/api/instances/` return HTTP 200, and the certificate/ownership validation is good before changing DNS. Railway's assigned CNAME target at rollout was `8cwgxkdl.up.railway.app`. Do not disable an otherwise healthy Cloudflare proxy simply to clear a cosmetic warning.

## Alternative: independent Docker Compose deployment + local MCP

Plane CE self-hosting requires more RAM and operational work than the MCP plugin. For a separate **non-Railway** install, deploy Plane in **its own Docker Compose stack** (not inside the CodeLocal web process). Keep backups for PostgreSQL and object storage, TLS, mail, credentials, and regular upgrades.

Official self-host guide: https://developers.plane.so/self-hosting/methods/docker-compose

**Deployment preflight and verification:** run `bash deploy/plane/doctor.sh` on a separate Docker host and follow [the CodeLocal Plane self-hosting runbook](../../deploy/plane/README.md). The runbook includes resource safeguards, official installation, authenticated API checks and a no-secret-in-Git MCP registration helper. Do not install Plane on an under-provisioned CodeLocal development machine or inside the existing CodeLocal Railway service.

For a private self-hosted Plane server, use the official stdio MCP server from an **authorized** CodeLocal device. Install Python 3.10+ and `uvx` and set these variables outside the repository:

- `PLANE_API_KEY`: Plane personal/workspace access token
- `PLANE_WORKSPACE_SLUG`: workspace slug
- `PLANE_BASE_URL`: for CodeLocal Tasks, `https://plane.codelocal.cloud`

Then register local MCP:

```bash
codelocal mcp add plane --global \
  --env PLANE_API_KEY \
  --env PLANE_WORKSPACE_SLUG \
  --env PLANE_BASE_URL \
  -- uvx plane-mcp-server stdio

codelocal mcp probe plane
codelocal mcp search workitem --server plane
```

The `--env` flags reference existing environment variables without embedding token values in the MCP registry. Omit `--global` to scope registration to one project. **Local stdio needs the device running**; the hosted Cloud MCP PAT plugin does not work with a private Plane URL.

For fully unattended self-hosted installations, deploy a dedicated protected Plane MCP service next to Plane and build account-specific auth handling (never one shared team admin token for all tenants). Do not expose a local stdio server or internal Plane API to the public Internet.

## Safety and acceptance

- Each CodeLocal account connects and encrypts **its own** PAT and slug; Plane roles still determine access
- CodeLocal's MCP tool approval is preserved for writes and deletes
- Revoking the PAT in Plane or disconnecting the CodeLocal plugin blocks new requests
- Check project/work-item reads, a test work-item creation/update, approval denial, token rotation and account isolation
- Test server restart, bad slug, bad PAT, and offline CodeLocal device
- Do not consider the integration connected until MCP discovery returns real tools
- Source code passing tests is not proof of a deployed Plane instance or a live user connection

References:
- https://developers.plane.so/dev-tools/mcp-server
- https://github.com/makeplane/plane-mcp-server
