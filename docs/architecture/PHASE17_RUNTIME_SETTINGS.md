# Phase 17 — Rust workspace runtime preferences (2026-11-24)

Added the existing Next.js Settings contract to the isolated Rust beta:
- GET /api/v1/runtime/settings for global, owned device and owned workspace scopes.
- POST /api/v1/runtime/settings/execution: workspace-only safe/live preference.
- POST /api/v1/runtime/settings/config: workspace-only CODELOCAL_MAX_WORKTREES, integer 1–20 or delete (default UI value 3).

The settings are real PostgreSQL records, not in-memory mock values. Migration 005 creates a table keyed by account/device/workspace with a foreign key to the registered workspace. Removing a workspace deletes its preferences. The read checks live non-revoked device ownership. Writes require the existing authenticated browser session, CSRF and no active risk flag, lock the owned device/workspace, and commit the change and audit record atomically. Concurrent mode and limit updates preserve the other field. Global/device scopes expose only the supported default/empty preference model.

Responses explicitly say runtimeApplied:false: saving a preference is NOT execution permission, changing the active runtime mode, or worktree cleanup. The native runtime does not consume these preferences yet. Arbitrary environment variables, encrypted secrets, system projects, inheritance of general configuration and remote application are not implemented by this phase. Existing Next.js UI and Go archive are unchanged.

## Verification

Unit tests validate the DTO, target normalization, bounded worktree limit, unsupported key rejection, anonymous HTTP denial and form parsing. The real-store CI test now exercises default settings, invalid CSRF, invalid limit, save/read mode and limit, cross-account denial, delete/reset, and removed-workspace denial. It requires migration 005 on the disposable beta database. Real-store acceptance depends on a successful CI run; no production database is used.

Existing beta development volumes need the reviewed 005 migration applied explicitly; no automatic migration or destructive cleanup.
