# Phase 10 — Human approval for local Rust MCP reads (2026-11-22)

## Implemented

The existing `codelocal mcp` stdio process now **fails closed by default** for the file-access MCP tools: `codelocal_local_info`, `codelocal_local_read`, `codelocal_local_search`, and (since Phase 11) `codelocal_local_read_many`. `codelocal_workspace_list` remains a metadata-only tool returning workspace IDs and names (not local paths).

1. The AI client calls `tools/call` with an exact tool name and arguments, including a currently granted `workspaceId`.
2. Instead of reading content immediately, Rust returns an MCP text result with `status:approval_required`, `executionStarted:false`, an unpredictable `approvalId` and expiry time.
3. The human opens another terminal and runs `codelocal approvals`, reviews the tool, workspace ID and exact JSON arguments, then explicitly issues `codelocal approvals approve <id>` or `codelocal approvals deny <id>`. The local AI client has **no approval tool**.
4. The client resubmits the **exact same** `tools/call` with `params.approvalId`. The request is bound to a random MCP process session, exact arguments, tool and workspace, and executes at most once. Replays, mutated arguments, a different client session or denied/expired tickets are rejected.
5. Every tool still checks the live workspace grant and the existing sensitive-file policy **after approval**. An approved `.env` request remains forbidden.

The beta-only approval inbox lives at `~/.codelocal-beta/mcp-approvals.json` (or `CODELOCAL_BETA_STATE_DIR`). On Unix it uses mode 0700 for its containing state directory, mode 0600 for files, a local exclusive lock and atomic temp-file replacements, and refuses symlinks and corrupted state. The inbox is bounded to 32 tickets / 128 KiB; submitted arguments are bounded to 512 bytes. Pending tickets expire after 5 minutes, approved tickets after 2 minutes. A ticket is consumed *before* the read executes, not afterwards.

## Example

```sh
# Terminal A: launch trusted AI client and configure the Rust MCP command
codelocal mcp

# Terminal B: inspect pending requests and choose one exact ID to decide
codelocal approvals
codelocal approvals approve 0123456789abcdef0123456789abcdef
# or
codelocal approvals deny 0123456789abcdef0123456789abcdef
```

IDs are randomly issued: the example above is **not** a usable approval ID. The AI client must retry with `params.approvalId` without modifying `arguments`.

## Safety boundaries

- **No remote MCP**, write/edit, terminal, git, process, browser or agent operation is approved or exposed by this module.
- The approval is local to the signed-in OS user's beta state. A malicious process already executing with that same OS account could invoke the CLI directly; this is **not** a sandbox or OS-level security boundary. Use only trusted local MCP clients.
- Windows approval storage fails closed until Windows ACL and reparse-point protections are implemented and tested.
- Full Go parity still requires Cloud MCP OAuth/PKCE, per-user authorization, server-side durable approvals, 14 tool/action contracts, response streaming, and local runtime permission-to-execute mechanisms.
- PostgreSQL/Redis beta E2E remains unverified due to the connected Mac's Docker daemon issue. This phase's tests exercise local filesystem approval semantics without depending on production services.

## Verification

`cargo fmt --all -- --check`, Rust workspace tests and Clippy are required before committing/pushing to GitHub `beta`. Tests cover exact argument/session binding, expiry/refusal, single-use replay, denial, file permissions, symlink refusal, secret-file access refusal even after approval, and unknown privileged tools returning errors.
