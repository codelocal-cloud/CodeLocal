# Phase 20B — Device credential and grant lifecycle hardening

- Ed25519 verification now rejects weak public keys and uses strict verification; pairing rejects invalid or weak encoded keys.
- Runtime presence and signed metadata sync use a shared workspace ID validator, rejecting dot segments and duplicate presence IDs.
- Re-pairing with a new credential or revoking a credential clears old workspace registrations and scoped secrets transactionally. Workspace preferences are removed by FK cascade. A new credential must sync its own grants; a retry of the same pairing claim preserves the current grants. Global secrets remain account-owned.
- 95 local Rust tests passed; Clippy with warnings denied passed. The real-store CI fixture tests same-claim retry, credential replacement, stale credential rejection, fresh grant sync, reset preferences and revocation with a registered workspace. CI acceptance is pending.
- This is not remote execution authorization. Runtime lease TTL and credential checks remain separate from local approval and future command routing. Existing Next.js, main and production are unchanged.
