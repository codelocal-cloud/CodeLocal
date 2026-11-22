# Phase 5 — Rust local file read foundation (2026-11-22)

## Goal and implementation

Keep the **unchanged Next.js UI** while replacing the old Go native localfs read boundary with Rust in `cli/src/localfs.rs`. This phase deliberately provides **manual, local-only file access** behind an explicit beta workspace grant. It does **not** expose a Cloud, WebSocket or MCP tool endpoint.

Implemented in the Rust CLI:

| Command | Input | Behavior |
| --- | --- | --- |
| `local-info <workspace-id> <relative-path>` | Existing granted workspace and file/directory | Return bounded file metadata and SHA-256 for eligible small files |
| `local-read <workspace-id> <relative-path>` | Granted workspace and relative text path | Return full content if 2 MiB or less, with metadata and SHA-256 |
| `local-read <workspace-id> <relative-path> <start> <end>` | 1-based inclusive lines | Return selected lines, file-wide hash, and line count |
| `local-read-many <workspace-id> <path...>` | At most 20 explicit relative files | Preflight all entries, return ordered files, fail if a file exceeds 2 MiB or batch exceeds 8 MiB |

The local-read result follows a **subset** of the archived Go read response structure (`path`, `size`, `mtimeMs`, `isFile`, `isDirectory`, `hash`, `binary`, `content`, `startLine`, `endLine`, `totalLines`). File-ignored status is **intentionally omitted** until full Git ignore semantics are ported; no false `ignored=false` is emitted.

## Security boundaries

1. The Rust reader receives the exact `workspaceId` from `~/.codelocal-beta/workspaces.json` (or `CODELOCAL_BETA_STATE_DIR`), validates the persisted canonical root and workspace ID, and rechecks that the root itself is not a credential directory, filesystem root or entire home directory. A modified local registry cannot trivially grant `.ssh`.
2. It rejects absolute paths, parent traversal, Windows-style backslashes, path prefixes, private runtime folders, `.git`, `.ssh`, `.aws`, `.env*`, private-key and common credential filenames; benign portable `.codelocal/project.json`, `.codelocal/quality.json`, `rules/` and `skills/` can be inspected subject to the same sensitive-file checks.
3. On Unix, directory descriptors are walked with Rustix `openat` plus `O_NOFOLLOW` for **every child component**; final targets are opened with `O_NOFOLLOW|O_NONBLOCK`. This avoids the common canonicalize-then-open symlink race under the granted root.
4. On non-Unix platforms the fallback rejects symlink components and checks canonical containment but is **not yet OS-level race-proof**. Do not expose this fallback to network/MCP calls until an equivalent Windows handle-based implementation and tests exist.
5. Only regular files can be read. Reads use a bounded OS file handle and refuse oversized/growing files. Binary/invalid UTF-8 data is not emitted as text. No file content or paths are uploaded to Cloud by these commands.
6. **No local or remote write/edit/terminal privileges are added.** Permission checks for the future 14 compact MCP tool actions require independent user approvals, audits, signed device transport and workspace policy.

## Checks performed

- `cargo fmt --all -- --check`, `cargo test --workspace --offline`, `cargo clippy --workspace --all-targets --offline -- -D warnings`.
- Unit tests cover schema/line ranges and CRLF, stable SHA-256, binary input, oversized files, batch validation, sensitive filenames, tampered registry, revoked grants, internal CodeLocal runtime state, parent traversal, symlinks inside/outside the workspace.
- Live CLI smoke using an **isolated beta registry**: granted `./cli`, then `local-read` 1–4, `local-info`, `local-read-many` on `src/lib.rs` / `Cargo.toml`. Returned expected local content/hash/metadata. Attempted traversal was blocked by local CodeLocal terminal guard; direct unit tests independently verify the reader's own refusal.
- No Next.js source changes. No Go archive modifications. No production credentials, database or remote execution touched.

## Manual example

```sh
# Choose a throwaway test workspace/state; never use production Go ~/.codelocal.
CODELOCAL_BETA_STATE_DIR=target/localread-example cargo run -p codelocal -- grant ./cli
CODELOCAL_BETA_STATE_DIR=target/localread-example cargo run -p codelocal -- workspaces
# Replace <id> below with the ID shown by workspaces.
CODELOCAL_BETA_STATE_DIR=target/localread-example cargo run -p codelocal -- local-info <id> src/lib.rs
CODELOCAL_BETA_STATE_DIR=target/localread-example cargo run -p codelocal -- local-read <id> src/lib.rs 1 10
CODELOCAL_BETA_STATE_DIR=target/localread-example cargo run -p codelocal -- local-read-many <id> src/lib.rs Cargo.toml
```

## Outstanding before complete Go parity

- Git ignore matching, directory listings and recursive bounded `search files/text`, advanced read concurrency and file change/snapshot semantics.
- Filesystem editing, hash-safe writes, patch/apply, source formatters, approvals and audit.
- Windows race-free equivalent of Unix descriptor traversal.
- CLI local runtime/service and signed MCP remote read routing; **the local-only commands are not connected to the Cloud API**.
- PostgreSQL/Redis real-store E2E still blocked by the local Docker daemon `EOF` failure documented in Phase 4.

This is a **verified read-only foundation**, not a claim that the 14 MCP tools or full Go-to-Rust migration are complete.
