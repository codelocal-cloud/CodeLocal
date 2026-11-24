# Phase 18 — Encrypted beta Runtime Secrets (2026-11-24)

The preserved Next.js Runtime Settings page can now create, replace and delete secrets via POST /api/v1/runtime/settings/secret. Global, Device and Workspace scopes are supported. GET /api/v1/runtime/settings exposes only names and configured:true references, with effective references merged from global to device to workspace. Existing execution-mode and worktree preferences remain separate.

## Security model

- AES-256-GCM authenticated encryption via ring; random 96-bit nonce per write; associated data binds the ciphertext to the exact account, device, workspace and name. A SHA-256 key identifier supports identifying the encrypting key. No plaintext is stored in PostgreSQL, returned by dashboard APIs or placed in audit details. The submitted secret string and temporary key input are explicitly zeroized where controlled by this code; this is not a guarantee of complete process-memory erasure.
- Set a dedicated, independently generated 32-byte key as 64 hex characters in CODELOCAL_BETA_RUNTIME_SECRET_KEY_HEX. Never reuse the session binding secret, commit the key, or use a CI key outside CI. Missing/invalid key causes writes of new secret values to fail closed. Deletion and metadata reads do not require decryption. Keep the key securely backed up; changing it does not migrate existing encrypted rows. Key rotation and recovery tooling are pending.
- Browser mutations require a valid bound session, CSRF and no active risk flag. Ownership is verified within the write transaction; account/device/workspace locks serialize writes, and audit is atomic with the change. Limits: 64 names per layer, 4096 bytes per value and ASCII environment-style names of at most 128 characters.
- Migration 006 uses nullable composite foreign keys and PostgreSQL 17 unique NULLS NOT DISTINCT constraints. Workspace removal cascades its secrets. Revoking or newly pairing a device deletes its device/workspace secrets in the same database transaction; global account secrets remain.
- This phase only stores encrypted values and resolves metadata. There is **no decryption endpoint, remote secret delivery, arbitrary config injection, or remote execution**. The beta key is read on each set; metadata does not certify that a key remains available. Safe/live preferences and secret settings still return runtimeApplied:false.

## Verification and operation

Apply 006_beta_runtime_secrets.sql explicitly to existing isolated beta volumes; a fresh Compose/CI database initializes migrations 001–006 in order. GitHub CI generates a disposable random encryption key only for the real-store test. Unit tests exercise randomized authenticated encryption, tampering, wrong keys, cross-scope binding, and input limits. The integration test adds secret set/replace/delete, ciphertext checks, metadata inheritance, CSRF, cross-account isolation, audit redaction, and workspace/device cleanup. Actual PostgreSQL/Redis CI must pass before claiming real-store acceptance.

No changes to Next.js, frozen Go archive, production or main.
