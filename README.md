# CodeLocal beta — Rust + Next.js rebuild

This is a **clean-slate replacement** for the old Go/TypeScript CodeLocal codebase.

- `backend/` — native Rust API service (Axum + Tokio).
- `cli/` — native Rust CLI entrypoint (bootstrap only).
- `web/` — brand-new Next.js App Router interface.
- `old-project/` — archived source snapshot of the original Git-tracked repository, kept for contract reference and rollback.
- `docs/` — architecture, migration checklist, and compatibility gates.
- `migration/` — repeatable archive procedure.

**Important:** This is a bootstrap, **not a production-ready replacement**.
The Rust code does not yet implement login, OAuth, MCP, tool execution,
Project Brain, database persistence, device pairing or billing. The old
production environment must continue using its existing deploy until
migration acceptance gates pass.

## Run locally

Rust API:

```sh
cargo run -p codelocal-backend
# GET http://127.0.0.1:8080/healthz
# GET http://127.0.0.1:8080/api/v1/system/status
```

Frontend:

```sh
cd web
npm install
npm run dev
# http://localhost:3000
```

Rust CLI:

```sh
cargo run -p codelocal -- --version
```

See `docs/architecture/MIGRATION.md` before implementing production behavior.
