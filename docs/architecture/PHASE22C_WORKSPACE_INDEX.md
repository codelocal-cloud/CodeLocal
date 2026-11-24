# Phase 22C — Workspace symbol search and import candidates

New local CLI commands:

- `codelocal symbol-search <workspace-id> <name> [limit]` searches case-sensitive exact ASCII declaration names, returning relative path, language, kind and line.
- `codelocal dependency-map <workspace-id> [limit]` collects import references and reports unresolved, candidate or ambiguous edges. Only JS/TS relative imports have path candidates: explicit source filenames, extensionless source files and index files. Resolution is restricted to files indexed within the current workspace; parent traversal beyond the workspace, sensitive paths, and external packages cannot resolve. No compiler alias, package export, Rust/Go/Python module resolution, semantic graph or call graph is claimed.

Both commands use the existing grant, gitignore-aware deterministic traversal and no-follow file access. Limits: 10,000 visited entries, depth 24, 512 indexed source files, 16 MiB cumulative read budget (overflow probes can read one extra byte), 256 KiB per file, 200 candidates per category per file, and up to 200 returned matches or edges. Source paths longer than 1024 bytes, unsupported encodings, inaccessible files and oversized sources are skipped with partial-scan metadata where applicable. Source contents are not emitted or transmitted; analysis remains local and lexical. A scan can be incomplete, and a candidate is not proof of a compiler-resolved edge. No persistent index or automatic scanning is added.

104 local Rust unit tests, formatting, Clippy and diff checks passed. Tests verify exact declaration lookup, ignored and oversized inputs, deterministic candidate order, ambiguity, unresolved external and escaped paths, and bounded results. Real-store E2E remains CI-only. The MCP approval catalog, Next.js, archived Go, main and production are unchanged. Semantic/LSP indexing, persistent Project Brain, Cloud graph endpoints and remote context tools remain pending.
