# Plane MCP in CodeLocal

CodeLocal can use Plane's official Model Context Protocol server. The integration adds **Plane Project Management** to Dashboard → Plugins → Explore as an opt-in Cloud OAuth plugin. This is separate from deploying a Plane project-management instance.

## Plane Cloud: connect the built-in plugin

1. Deploy the CodeLocal code changes, sign in, and open **Dashboard → Plugins → Explore**.
2. Install **Plane Project Management**, then select **Configure**.
3. Review the read/write/delete permissions and choose **Sign in with OAuth**.
4. Sign in to Plane, choose the desired workspace and finish authorization.
5. Confirm the connection lists discovered MCP tools before using it in Chat.
6. Approve write/destructive tool calls individually when asked by CodeLocal.

Endpoint: `https://mcp.plane.so/http/mcp`. This uses Streamable HTTP and OAuth. It deliberately does **not** auto-install or auto-authorize accounts.

**Important:** Plane's hosted OAuth server uses a callback URI allowlist. CodeLocal's `/api/v1/plugins/oauth/callback` may not be allowlisted by the hosted service. If authorization is rejected, ask the Plane MCP service operator to allowlist the exact CodeLocal callback or deploy an MCP server configured with `PLANE_OAUTH_ALLOWED_REDIRECT_URIS`. Do **not** treat adding the plugin to the catalog as a working OAuth connection.

## Plane Community Edition / self-hosted: local MCP Hub

The official Python server works against a private Plane instance through stdio. It requires Python 3.10+ and `uvx` (provided by [uv](https://docs.astral.sh/uv/)). Start CodeLocal from an environment that contains the following variable **names and values**, configured outside the repository:

- `PLANE_API_KEY`: a private Plane personal or workspace token
- `PLANE_WORKSPACE_SLUG`: the Plane workspace slug
- `PLANE_BASE_URL`: your Plane instance URL, e.g. `https://plane.example.com` (optional for Plane Cloud)

Register the server in CodeLocal from your authorized workspace, after ensuring those variables are available to the CodeLocal runtime:

```bash
codelocal mcp add plane --global \
  --env PLANE_API_KEY \
  --env PLANE_WORKSPACE_SLUG \
  --env PLANE_BASE_URL \
  -- uvx plane-mcp-server stdio

codelocal mcp probe plane
codelocal mcp search workitem --server plane
```

The `--env` values above are references to existing environment variables; do not put tokens into command arguments, Git, logs or MCP registry files. If using the default hosted API, omit the `--env PLANE_BASE_URL` flag and the variable.

To limit access to a single CodeLocal project, omit `--global` and run the command from that project directory. Local stdio requires a connected, authorized CodeLocal device; unlike Cloud, it cannot operate while that device is offline. Configure access and approvals according to the local MCP policy.

## Other hosted option: PAT

Plane also provides `https://mcp.plane.so/http/api-key/mcp` for headless integrations, which requires both `Authorization: Bearer <PAT>` and `x-workspace-slug: <slug>`. **The built-in Cloud OAuth template does not implement this two-header PAT flow.** For PAT use a custom MCP connection that securely supports both headers or a local stdio server; do not paste PATs into public endpoint URLs.

## Release acceptance checklist

- [ ] Manifest validates and Plane appears in Plugins → Explore, not Installed by default
- [ ] OAuth callback actually completes against the chosen Plane server; connection discovers tools
- [ ] Read calls list projects and work items from the authorized workspace only
- [ ] A test work item can be created/updated only after the normal CodeLocal approval
- [ ] Rejected or expired approvals do not execute writes; disconnected accounts cannot call tools
- [ ] Missing credentials, offline local runtime and unsupported callbacks show errors, not false success
- [ ] No API keys, OAuth tokens or real customer work items are committed to the repository

References: https://developers.plane.so/dev-tools/mcp-server and https://github.com/makeplane/plane-mcp-server
