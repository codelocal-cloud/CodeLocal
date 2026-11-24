# Phase 20C — Measured Cloud MCP usage

Successful workspace/list tool calls now record real JSON argument and result payload sizes in the existing PostgreSQL usage rollup. Estimated tokens use ceiling(Unicode scalar count / 4), matching the legacy approximation, not AI provider billing. Each server invocation gets an independent random event ID; the existing transactional event deduplication remains in force. Failed authentication, unsupported tools, notifications, initialization and catalog failures are not recorded as successful calls. A usage-write failure returns a JSON-RPC internal error rather than claiming a metered successful invocation. This does not deduplicate client retries across distinct invocations.

The real-store integration fixture covers exact measured argument/result estimates, a separate account's registered workspace remaining invisible, a live owned workspace list, and the resulting dashboard counters. A historical MCP event is not evidence of an active connection; provider-reported chat and active MCP session tracking remain unported. Only read-only workspace metadata is available remotely. No remote execution or approval changes are introduced.

96 local Rust unit tests and Clippy passed. GitHub PostgreSQL/Redis E2E and Next.js CI must still pass on the commit. The original Next.js UI, main and production remain unchanged.
