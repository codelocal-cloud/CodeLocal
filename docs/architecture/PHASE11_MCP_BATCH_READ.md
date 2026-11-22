# Phase 11 — One-use-approved MCP batch read (2026-11-22)

## Implementation

- Added local Rust MCP stdio tool `codelocal_local_read_many` with required `workspaceId` and `paths` array (1–20 paths).
- The tool reuses the bounded read engine: maximum 2 MiB per file, 8 MiB total, no ungranted roots, parent traversal, symlinks or credential paths. No network upload, writes, terminal or Git operations.
- Every batch request requires a human-reviewed `approvalId` from the local CLI inbox. Approval is tied to the exact full array of relative file names, MCP process session, workspace and tool, expires and is consumed once before any read.
- The batch is preflight-validated before files are read. If a batch includes `.env` or a similarly restricted file, the call returns an error even if the human approved it. Source content is never included in pending responses.
- The local MCP tool catalog grows from four to **five** tools: workspace list, info, read, read-many and fixed filename/text search.

## Verification

- Rust tests cover the exact batch approval/replay path and rejection of mixed sensitive-file requests; backend integration tests stay separate.
- Clippy, Rust formatting and existing CLI tests must pass before commit/push.
- This local-only MCP extension is not the Cloud MCP Gateway and is not full Go compact-tool parity.

See `docs/architecture/PHASE10_LOCAL_MCP_APPROVALS.md` for the manual review flow and expiry/permissions.
