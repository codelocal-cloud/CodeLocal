# Plane Community Edition — isolated self-hosting for CodeLocal

This folder contains **operator checks and MCP onboarding**, not a fork of Plane. Never share CodeLocal's production database, Redis, secrets, or persistent volumes with Plane.

## 1. Choose a host

Use a **separate Linux server** running Docker Compose. Plane's official minimum is 2 vCPU, 4 GiB RAM; 8 GiB is recommended for production. Reserve at least 20 GiB of free disk for the stack, logs and growth, plus a backup destination.

Run from this repository's root:

```bash
bash deploy/plane/doctor.sh
```

A FAIL is a hard stop: do not delete unrelated Docker images, databases or volumes to make room. On the current MacBook, Docker has ~7.75 GiB allocated and only ~11 GiB disk space was available when checked (2026-11-11). It is **not safe** to start the full stack there.

## 2. Install the *official* Plane Community stack on the dedicated host

Do not run third-party or unreviewed `curl | bash` scripts. Follow the official installer:

```bash
mkdir -p "$HOME/plane-selfhost"
cd "$HOME/plane-selfhost"
curl -fsSL -o setup.sh https://github.com/makeplane/plane/releases/latest/download/setup.sh
less setup.sh                     # review before executing
chmod 700 setup.sh
./setup.sh                       # choose Install (Community Edition)
```

The installer creates `plane-app/plane.env` and Docker Compose definitions. Check the generated environment before starting. Set a real domain, public `WEB_URL`, HTTPS origin in `CORS_ALLOWED_ORIGINS`, and unique secret keys for the API, live server, Postgres, RabbitMQ and S3/MinIO. Never commit `plane.env`, tokens or uploaded files. Do not expose Postgres, Redis, RabbitMQ or MinIO directly to the internet.

For production, configure:
- HTTPS and a DNS name such as `tasks.example.com`
- external/managed PostgreSQL and S3-compatible storage where possible; scheduled **restore-tested** backups
- SMTP, outbound email, rate limiting, disk and RAM monitoring, resource quotas
- restricted admin bootstrap; do **not** leave `/god-mode/` unclaimed on a public domain
- pinned image versions, maintenance and upgrade plan

References: [Plane Docker Compose guide](https://developers.plane.so/self-hosting/methods/docker-compose) and [official environment reference](https://developers.plane.so/self-hosting/govern/environment-variables).

## 3. Verify live Plane and connect its MCP server to CodeLocal

Finish Plane's initial admin/workspace setup yourself through its HTTPS UI. Generate a **least-privilege workspace access token**, then export these variables locally in your authorized CodeLocal runtime (do not paste them in chat):

```bash
export PLANE_BASE_URL=https://tasks.example.com
export PLANE_WORKSPACE_SLUG=example-team
read -rs PLANE_API_KEY && export PLANE_API_KEY
bash deploy/plane/verify.sh
bash deploy/plane/register-mcp.sh
```

This checks real Plane HTTP and REST authentication before registering the MCP client with `uvx plane-mcp-server stdio`. The script stores only **references to** `PLANE_API_KEY`, `PLANE_BASE_URL`, and `PLANE_WORKSPACE_SLUG` in CodeLocal. The secrets must remain available to the CodeLocal runtime environment when the MCP tool launches; a variable exported in a transient shell may not propagate to an already-running background runtime. Ensure the service manager supplies secrets securely.

**Important:** The built-in CodeLocal **Cloud Plane plugin** is currently hard-restricted to Plane's hosted `mcp.plane.so` PAT endpoint. It does **not** connect to a private self-hosted Plane URL. Local CodeLocal MCP Hub stdio works with self-hosted Plane and requires the paired device to stay online. A 24/7 self-hosted remote MCP service would be a separate, access-controlled deployment.

## 4. Railway option (requires a budget decision)

Railway also has a [community-maintained Plane template](https://railway.com/deploy/plane-with-rabbitmq) including Postgres, Redis, RabbitMQ and MinIO. It is **not an official verified Plane template**. Review all images, variables, admin bootstrap and storage mounts before using it.

At the November 11, 2026 check, the connected Railway workspace had spent **$48.68 of a $100 hard limit** in the current cycle. Starting five or more additional always-on services could hit that shared cap and affect **other Railway projects**. Do not deploy this template into the existing `codelocal.cloud` Railway project, and do not raise workspace spending limits automatically.

Once a separate server or spending budget has been approved, deploy to a dedicated Plane project and confirm site availability, workspace admin ownership, PAT, MCP probe, test work-item creation, backup restore and uptime. A code merge or passing CI does **not** imply the Plane server is live.
