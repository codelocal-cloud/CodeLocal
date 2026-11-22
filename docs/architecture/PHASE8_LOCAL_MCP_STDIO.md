# Phase 8 — Rust beta local MCP stdio read/search bridge (2026-11-22)

## What is implemented

The `codelocal mcp` command runs an **MCP JSON-RPC 2.0 stdio server** locally, using the existing beta workspace grant registry. It does **not** connect to the Cloud or open any network listener.

The initial read-only MCP tools are:

| Tool | Capability |
| --- | --- |
| `codelocal_workspace_list` | Granted workspace IDs and names (no absolute paths) |
| `codelocal_local_info` | Local metadata/SHA-256 (file/directory) |
| `codelocal_local_read` | Bounded text read and optional line range |
| `codelocal_local_search` | Git-ignore-aware search by filename or fixed text |

This uses the tested Rust `LocalReader`, with explicit local grants, sensitive-file exclusions, symlink rejection, bounded file sizes and scan result caps. JSON-RPC methods `initialize`, `ping`, `tools/list` and `tools/call` are implemented. Notifications do not produce responses. Input is limited to 256 KiB per message.

**No edit, write, terminal, git, process, agent, browser, computer, network or remote operations are exposed by this MCP server.** Phase 7 manual `local-edit` remains CLI-only pending explicit action approval and auditable sandbox policies. This is a **subset**, not 14-tool compatibility.

## Manual use

```sh
cargo build -p codelocal
# Grant an individual project (do not grant your home directory):
./target/debug/codelocal grant ./cli
./target/debug/codelocal workspaces
# Configure the AI client's local MCP server command to point at:
./target/debug/codelocal mcp
```

The MCP client is responsible for spawning the process with access to the same `CODELOCAL_BETA_STATE_DIR` as the grant operation. Connect **only a locally trusted AI client**. **Phase 10 now requires a human CLI approval for each source read/search and binds approval to one exact invocation; see `PHASE10_LOCAL_MCP_APPROVALS.md`.**

## Verification

- Rust unit tests cover initialization, JSON-RPC framing, tool discovery, read/search grant requirements, secret path denial, unknown tool refusal, and notifications.
- Direct stdio smoke confirmed that a real MCP `initialize` request produces one valid JSON-RPC response with protocol version `2025-06-18`, four read-only tools and no stdout logs.
- Rust fmt, tests and Clippy gates pass.
- No changes to Go production source, Next.js UI, or `main`.

## Remaining work

Cloud MCP Gateway, remote runtime/device tunnel, approvals, role authorization, 14 compact Go tool/action contracts, reconnect handling, streaming and audit E2E are still unimplemented. This local beta foundation must not be presented as the production MCP server.
