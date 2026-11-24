# Phase 22A — Local project structure inventory

Rust beta implements `codelocal project-map <workspace-id>` and the sixth local stdio MCP tool `codelocal_project_map`. Both require a current workspace grant; MCP additionally requires the existing exact-request, one-use human approval. The map contains filename-based language counts, visible files/directories, candidate manifests and entrypoints, and bounded relative path samples. It does not read source contents, execute build scripts, transmit files to Cloud, or expose absolute paths.

The existing gitignore-aware walker and sensitive path filtering are reused. Files are reopened through the local read boundary before counting. Scan limits are 10,000 entries and depth 24; samples are limited to 64 per category and 1,024 bytes per sampled path. Truncation is explicitly reported. Language and entrypoint detection are heuristics, not semantic analysis or an exact replacement for the Go project engine.

Local validation: 100 Rust tests passed, Clippy passed. Tests cover manifest/language detection, sample bounds, sensitive/ignored paths, and exact one-use approval. GitHub CI on the new commit is required for acceptance. Semantic symbols, LSP, dependency and call graphs, Project Brain durable knowledge, remote context transport and full compact MCP compatibility remain pending.
