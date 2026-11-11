# Plane + CodeLocal: recommended integration

**Architecture:** Keep Plane as a separate project-management service; use the official Plane MCP server from CodeLocal. Do not fork Plane into CodeLocal. Keep task data and Plane's DB outside CodeLocal's DB.

## Primary option: Plane Cloud, CodeLocal Cloud MCP, encrypted workspace-scoped PAT

This is the default for remote AI agents and multiple user devices. It works without an online CodeLocal device and does not depend on Plane hosted OAuth callback allowlisting.

1. In Plane, open **Profile Settings → Personal Access Tokens**. Create a token limited to the account/role required. Prefer a workspace token where available.
2. Find your workspace slug in the Plane URL, e.g. `https://app.plane.so/example-team/` → `example-team`.
3. In CodeLocal, open **Dashboard → Plugins → Explore → Plane Project Management → Install → Configure**.
4. Use the prefilled official HTTPS MCP endpoint `https://mcp.plane.so/http/api-key/mcp`, enter the workspace slug and paste the token. Do **not** paste the token in a URL.
5. Approve the requested permissions. CodeLocal probes MCP discovery; it reports ready only if discovery works.
6. In Chat, use `list_plugin_tools` and `call_plugin_tool`; review and approve changes before any remote write.

CodeLocal stores the PAT and slug together in its account-bound encrypted Cloud credential store. Plane work items remain in Plane. CodeLocal's Cloud MCP client attaches `Authorization: Bearer <PAT>` and `x-workspace-slug: <slug>` only when sending to the exact official Plane PAT MCP endpoint. It does not make a global authorization token.

**Migration:** The previous built-in Plane v0.1.0 used OAuth. Users must reconnect with a Plane access token to use v0.2.0. Existing Plane OAuth credentials are not converted.

## Self-hosted Plane Community Edition: independent Docker deployment + local MCP

Plane CE self-hosting requires more RAM and operational work than the MCP plugin. Deploy Plane in **its own Docker Compose stack on a long-running server** (not inside the CodeLocal web process). Keep backups for PostgreSQL and object storage, TLS, mail, credentials, and regular upgrades.

Official self-host guide: https://developers.plane.so/self-hosting/methods/docker-compose

**Deployment preflight and verification:** run `bash deploy/plane/doctor.sh` on a separate Docker host and follow [the CodeLocal Plane self-hosting runbook](../../deploy/plane/README.md). The runbook includes resource safeguards, official installation, authenticated API checks and a no-secret-in-Git MCP registration helper. Do not install Plane on an under-provisioned CodeLocal development machine or inside the existing CodeLocal Railway service.

For a private self-hosted Plane server, use the official stdio MCP server from an **authorized** CodeLocal device. Install Python 3.10+ and `uvx` and set these variables outside the repository:

- `PLANE_API_KEY`: Plane personal/workspace access token
- `PLANE_WORKSPACE_SLUG`: workspace slug
- `PLANE_BASE_URL`: the self-hosted Plane URL, e.g. `https://plane.example.com`

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
