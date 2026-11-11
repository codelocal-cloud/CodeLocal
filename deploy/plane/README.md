# Plane Community Edition — CodeLocal Tasks

**Canonical URL:** [https://plane.codelocal.cloud](https://plane.codelocal.cloud)

**Railway:** [project `codelocal.cloud`](https://railway.com/project/d5286319-d1c5-4b51-9022-ac3211ed8f2a) → **environment `plane`**.

The November 11, 2026 deployment uses Plane CE AIO `v1.4.3`, service `PlaneApp`, separate `PlaneDB` (PostgreSQL), `PlaneCache` (Redis), `PlaneQueue` (RabbitMQ), **`PlaneMCP`** (PAT-only MCP gateway) and bucket `PlaneFiles`. The pre-existing `dev` environment remains separate. `planeapp-plane.up.railway.app` is a fallback only.

This folder contains **operator checks and MCP onboarding**, not a fork of Plane. Never share CodeLocal's production database, Redis, secrets, or persistent volumes with Plane.

## Current Railway deployment and acceptance

In Railway open environment `plane` → `PlaneApp` → Variables:

- `DOMAIN_NAME=plane.codelocal.cloud`
- `APP_PROTOCOL=https`
- `WEB_URL=https://plane.codelocal.cloud`
- `CORS_ALLOWED_ORIGINS=https://plane.codelocal.cloud`
- `PORT=80` and `SITE_ADDRESS=:80` (**keep** these; the container reverse proxy listens on port 80 while Railway terminates TLS)
- `ENABLE_SIGNUP=0` (invite only, after SMTP/verification is working)

Initial admin login credentials are in the `PlaneApp` service variables. Change the generated password after signing in and **never expose it in Git or chat**. Configure SMTP and test password recovery/invitation emails before onboarding staff.

The Railway domain is mapped to `PlaneApp:80`; Cloudflare's proxy can hide the CNAME and trigger a cosmetic `REQUIRES_UPDATE` from Railway. The assigned Railway CNAME target was `8cwgxkdl.up.railway.app` on 2026-11-11; do not alter a healthy proxy or delete its DNS record simply to remove the warning. Verify domain ownership, valid HTTPS, and both website and API HTTP 200 first.

```bash
PLANE_BASE_URL=https://plane.codelocal.cloud bash deploy/plane/verify.sh
```

A successful public check does not confirm a login, a workspace invitation, a real task create/update, backups or MCP connectivity. Test these separately.


## 1. Optional: prepare a standalone Docker host

Use a **separate Linux server** running Docker Compose. Plane's official minimum is 2 vCPU, 4 GiB RAM; 8 GiB is recommended for production. Reserve at least 20 GiB of free disk for the stack, logs and growth, plus a backup destination.

Run from this repository's root:

```bash
bash deploy/plane/doctor.sh
```

A FAIL is a hard stop: do not delete unrelated Docker images, databases or volumes to make room. On the current MacBook, Docker has ~7.75 GiB allocated and only ~11 GiB disk space was available when checked (2026-11-11). It is **not safe** to start the full stack there.

## 2. Optional: install the official Plane Community Docker stack

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
export PLANE_BASE_URL=https://plane.codelocal.cloud
export PLANE_WORKSPACE_SLUG=example-team
read -rs PLANE_API_KEY && export PLANE_API_KEY
bash deploy/plane/verify.sh
bash deploy/plane/register-mcp.sh
```

This checks real Plane HTTP and REST authentication before registering the MCP client with `uvx plane-mcp-server stdio`. The script stores only **references to** `PLANE_API_KEY`, `PLANE_BASE_URL`, and `PLANE_WORKSPACE_SLUG` in CodeLocal. The secrets must remain available to the CodeLocal runtime environment when the MCP tool launches; a variable exported in a transient shell may not propagate to an already-running background runtime. Ensure the service manager supplies secrets securely.

**Important:** The built-in CodeLocal **Cloud Plane plugin** is currently hard-restricted to Plane's hosted `mcp.plane.so` PAT endpoint. It does **not** connect to a private self-hosted Plane URL. Local CodeLocal MCP Hub stdio works with self-hosted Plane and requires the paired device to stay online. A 24/7 self-hosted remote MCP service would be a separate, access-controlled deployment.

## 4. Railway operations and costs

The live Plane instance already runs in Railway project `codelocal.cloud`, but in a **separate `plane` environment** with distinct Plane-named services/storage. It is **not** a replacement for the pre-existing `dev` environment and should not depend on those database credentials.

At the first November 11, 2026 check, Railway usage was $48.68 of a shared $100 hard limit. This amount can change, and Plane's always-on service usage contributes to the shared quota. Monitor project spending and avoid raising hard limits or deleting other workloads automatically.

The earlier `codelocal-plane` test project, if still present, is a separate Plane instance and database. Do **not** delete it until you have verified that any work items, user accounts and uploads worth retaining have been migrated or intentionally abandoned. Deleting the project does not migrate content into the current Plane.

For support and maintenance: use the `plane` environment, check `PlaneApp` deployment health, monitor database volume/storage, configure SMTP, schedule backups and test restore. Rotate admin/API credentials and review access permissions regularly.

CodeLocal's Plane plugin v0.3.0 supports both the vetted self-hosted `PlaneMCP` gateway (`https://planemcp-plane.up.railway.app/http/api-key/mcp`) and the Plane Cloud PAT endpoint. The gateway runs the official Plane MCP image with only the header-authenticated transport enabled (see [mcp/README.md](mcp/README.md)). Each CodeLocal account must provide its own Plane-issued PAT and workspace slug. Confirm with real authenticated tool discovery before claiming end-to-end readiness.
