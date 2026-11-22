# CodeLocal beta repository instructions

User-approved migration target: **Rust backend + native Rust CLI/runtime,
Next.js + React + TypeScript frontend**. The previous project is archived
under `old-project/` and is read-only reference material.

Do not add production features to `old-project/`.
Do not deploy the beta bootstrap to production.
Preserve API, OAuth/MCP, authorization, data, workspace and device security
contracts during phased implementation. Never bypass approval or tenant
boundaries to make a feature work.

All new work belongs to `backend/`, `cli/`, `web/`, `docs/` or dedicated
Rust crates in the Cargo workspace. Each module requires tests, contracts,
and an explicit migration acceptance gate before replacing its Go equivalent.
