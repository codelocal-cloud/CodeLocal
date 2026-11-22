# Phase 6 — Rust local search files/text (2026-11-22)

## Delivered in `beta`

- `codelocal local-search-files <workspace-id> <query> [maxResults]`: searches workspace-relative filenames and returns `matches`, `truncated`, `scannedEntries`, `includeIgnored=false`.
- `codelocal local-search-text <workspace-id> <fixed-string> [maxResults]`: fixed UTF-8 substring search (no regex/shell) returning Go-style `relative/path:line:column:text` matches, `truncated` and scan metrics.
- Traversal uses the Rust `ignore` crate with `.gitignore`, `.ignore`, git excludes and Git global ignores; hidden, portable `.codelocal/rules` and `skills` remain eligible unless ignored.
- Security is mandatory: workspace grant identity, sensitive-path exclusions, no directory symlinks, file opens through the existing `openat/O_NOFOLLOW` boundary, and no Cloud upload or remote execution.
- Caps: 10,000 visited entries; depth 24; 200 results; 2 MiB/file; 64 MiB aggregate text scan. Results are intentionally **fixed-string only** and `includeIgnored=true` is not supported in this phase. A result set can be truncated by caps.

## Verified

- Workspace Rust tests and Clippy `-D warnings` pass, including tests for `.gitignore`, secrets, binary/oversized files, symlinks, duplicate results, UTF-8 and result limits.
- CLI smoke on isolated beta workspace: `grant ./cli`, `local-search-files ... .rs 8`, `local-search-text ... LocalReader 4`; returns correct relative matches and truncation.
- Go production runtime, Next.js UI, `main` and production data are unchanged.
- Signed Cloud/runtime MCP search is **not implemented yet**. This phase only creates the secure local foundation; it does not claim 14-tool parity.

## Next modules

1. Hash-checked local edit/replace with safe atomic write and approval boundary (never remote-enabled prematurely).
2. Rust MCP transport/gateway, device/nonce/workspace permission enforcement and local runtime process.
3. Full Go service/domain parity, end-to-end PostgreSQL/Redis and security/staging gates.

This phase is committed and pushed to GitHub `beta` only after testing; do not deploy until the full parity/security gates pass.
