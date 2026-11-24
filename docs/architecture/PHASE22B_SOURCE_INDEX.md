# Phase 22B — Local lexical source index

Rust beta provides `codelocal source-index <workspace-id> <file>` for a single granted source file. It reports bounded lexical symbol candidates and module/import references with line numbers for Rust, Go, Python, TypeScript and JavaScript. The existing workspace-relative path validation and no-follow file-open boundary are reused. Inputs are capped at 256 KiB and results at 200 declarations and 200 imports per file, with explicit truncation. No file contents or source snippets are returned.

This is heuristic line-based analysis, not an AST, semantic resolver, or resolved dependency graph. It may miss multiline declarations, language-specific syntax and complex imports, and can produce false positives in multiline strings/comments. Module references and symbol names are source-derived metadata. It is local-only and does not upload data or run project code.

MCP integration remains pending: the approval allowlist update was blocked, so the incomplete MCP changes were reverted. The existing six-tool catalog and approval policy remain unchanged.

Local validation: 102 Rust unit tests passed, formatting, Clippy and diff checks passed. The real PostgreSQL/Redis integration test remains CI-only. GitHub CI on this commit is required before acceptance. Full semantic indexing, workspace-wide queries, dependency resolution, LSP, durable Project Brain and remote execution remain pending.
