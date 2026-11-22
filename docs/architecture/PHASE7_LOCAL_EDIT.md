# Phase 7 — Rust local hash-safe exact edit (2026-11-22)

## Implemented on `beta`

- `codelocal local-edit <workspace-id> <path> <expected-sha256> <old> <new> [--all]` provides a **manual, local-only** exact text replacement command.
- Requires an existing locally granted beta workspace and a mandatory, valid SHA-256 of the existing file. Missing/mismatching hashes fail without modifying the file.
- Rejects ambiguous old text unless `--all` is supplied, prevents empty old text, preserves CRLF line endings for Go-style LF inputs, and limits input/output to 2 MiB. The response reports before/after hash, replacements and final bytes.
- The same sensitive filename/prefix policy as `local-read` is enforced. Symlinks, traversals, non-regular files and out-of-workspace paths are denied. No file creation, arbitrary overwrite or batch edits are enabled.
- On Unix, every parent directory is opened with `openat(O_NOFOLLOW|O_DIRECTORY)`. The output is written to a fresh securely randomized temporary file **inside the verified parent directory descriptor**. Original permission bits are preserved; the file is fsynced and promoted with `renameat` on the same directory descriptor; the directory is fsynced. On failure, temporary files are removed by an RAII guard.
- Before promotion, an independent second open compares the original inode/dev and full file SHA-256. This detects typical concurrent modifications; **it is not a fully linearizable CAS against non-cooperative external writers**, which could still race the final compare and rename. Remote/MCP edit remains **disabled**, pending an approval/locking protocol.
- Windows edit fails closed, pending safe handle-based replacement/reparse-point handling. Read-only CLI functionality remains available where previously supported.

## Verification

- Unit tests cover file edits, permission preservation, exact SHA-256, mismatch/ambiguous text refusal without mutations, sensitive/symlink exclusions, CRLF normalization, size limits and temporary-file cleanup.
- Rust `cargo fmt`, `cargo test`, `cargo clippy --all-targets -- -D warnings` passed before commit.
- No changes to Next.js, production Go code or `main`; this phase does not expose network execution.
- Real Cloud/MCP edit, per-action user approvals and audit persistence remain outstanding.

## Next

1. Approval-gated, session-bound Rust local runtime and MCP bridge.
2. Cross-process write serialization, Windows secure equivalents and comprehensive E2E.
3. Remaining Go feature parity and production readiness gates.

This is a narrow functional milestone, not a claim that CodeLocal is 100% Rust-ready.
