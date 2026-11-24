# Phase 22D — Local index MCP access

The existing bounded lexical workspace index is now accessible through two additional beta local stdio MCP tools:

- `codelocal_symbol_search`: `workspaceId`, exact ASCII identifier `query`, optional `maxResults` (1–200, default 100).
- `codelocal_dependency_map`: `workspaceId`, optional `maxResults` (1–200, default 100).

Both require a current workspace grant and a human-reviewed, exact-argument, session-bound, single-use approval. The existing index's traversal, source size, sensitive path and output limits remain unchanged. The tools return lexical candidates, not compiler-resolved symbols or semantic dependency graphs. The local experimental catalog contains eight tools, not the fourteen compact production tools.

Regression coverage includes catalog presence, approval before execution, changed-argument rejection, successful approved lookup, sensitive-file exclusion, and consumed-ticket rejection. The existing real-store integration test remains separate. Cloud routing, persistent indexes, semantic/LSP support and production parity are not implemented by this slice. Next.js, archived Go and production main are unchanged.
