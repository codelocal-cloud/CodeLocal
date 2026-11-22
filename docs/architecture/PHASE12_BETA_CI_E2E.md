# Phase 12 — Rust beta PostgreSQL/Redis E2E on GitHub Actions (2026-11-22)

## CI design

The existing beta-only `.github/workflows/ci.yml` now includes a third job, `rust-beta-integration`, alongside Rust unit/Clippy and Next.js checks.

- GitHub-hosted Ubuntu runner provisions disposable **PostgreSQL 17** and **Redis 7** service containers. Their ports are mapped to the runner loopback address.
- All credentials are test-only constants. Environment keys are the deliberately separate `CODELOCAL_BETA_*` values; PostgreSQL database name is `codelocal_beta`, which passes the Rust beta isolation guard. No production data, token, remote database or existing server is referenced.
- Before testing, CI applies exactly the reviewed `001_beta_auth.sql`, `002_beta_devices.sql`, `003_beta_workspaces.sql` migration sequence using `psql -v ON_ERROR_STOP=1`.
- CI executes the previously ignored but real-store `isolated_beta_login_account_logout_real_stores` test with `--ignored`. This covers auth session cookies and user account, pairing approval/claim, device credential checks, workspace registration/delete, Redis nonce replay protection, runtime presence lease replacement/refresh/cleanup, credential revoke and logout.
- The job is triggered by pushes and pull requests to **`beta`** and by manual workflow dispatch. It does not trigger a Railway deploy, npm publish or production migration.

## Gate interpretation

A green Rust unit test suite **does not** prove the Postgres/Redis integration job is green. The integration workflow must actually run and pass on GitHub before checking off the real-store E2E milestone. A locally parsed workflow YAML is not equivalent to a successful CI run.

The connected Mac still cannot obtain an active Docker daemon (`docker info` returns `EOF`); the GitHub Actions runner supplies container services without relying on this Mac.

If CI fails, investigate the workflow logs and correct Rust schema/queries on `beta`; do not bypass the test or point the runner at production.
