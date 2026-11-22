"use client";

import { useEffect, useState } from "react";
import { useParams } from "next/navigation";

type PendingApproval = {
  id: string;
  plugin: string;
  endpoint: string;
  tool: string;
  arguments: Record<string, unknown>;
  expiresAt: number;
};
type Account = { csrf: string };
type ApprovalResult = { status: string; result?: unknown; message?: string };

export default function PlaneMCPApprovalPage() {
  const params = useParams<{ id: string }>();
  const id = params.id;
  const [approval, setApproval] = useState<PendingApproval | null>(null);
  const [csrf, setCSRF] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<ApprovalResult | null>(null);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      if (!id || !/^[a-f0-9]{48}$/.test(id)) throw new Error("Invalid approval link.");
      const [detail, account] = await Promise.all([
        fetch("/api/v1/plugin-mcp-approvals/" + encodeURIComponent(id), { credentials: "same-origin", cache: "no-store" }),
        fetch("/api/v1/account", { credentials: "same-origin", cache: "no-store" }),
      ]);
      if (!detail.ok || !account.ok) throw new Error("Approval expired, unavailable or not owned by this account. Sign in to the same CodeLocal account.");
      const item = await detail.json() as PendingApproval;
      const identity = await account.json() as Account;
      if (item.plugin !== "plane" || !identity.csrf) throw new Error("Invalid approval or missing security token.");
      if (!cancelled) { setApproval(item); setCSRF(identity.csrf); }
    }
    void load().catch((e: unknown) => { if (!cancelled) setError(e instanceof Error ? e.message : "Could not load approval."); });
    return () => { cancelled = true; };
  }, [id]);

  async function submit(approve: boolean) {
    if (!approval || !csrf || busy || result) return;
    setBusy(true);
    setError("");
    try {
      const response = await fetch("/api/v1/plugin-mcp-approvals/" + encodeURIComponent(approval.id), {
        method: "POST",
        credentials: "same-origin",
        headers: { "Content-Type": "application/json", "X-CSRF-Token": csrf },
        body: JSON.stringify({ approve }),
      });
      const outcome = await response.json() as ApprovalResult;
      if (!response.ok) throw new Error("Approval could not be completed; refresh the provider to check its outcome.");
      setResult(outcome);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Operation failed.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <main style={{ maxWidth: 760, margin: "2rem auto", padding: "1.5rem", display: "grid", gap: "1rem" }}>
      <h1 style={{ fontSize: "1.5rem", fontWeight: 700 }}>Review Plane MCP operation</h1>
      <p>CodeLocal is requesting your approval to execute one specific Plane tool on your connected account. The action has not run yet.</p>
      {error && <p role="alert" style={{ color: "var(--color-danger, #b42318)" }}>{error}</p>}
      {approval && !result && (
        <>
          <p><strong>Plugin:</strong> {approval.plugin} · <strong>Tool:</strong> {approval.tool}</p>
          <p><strong>Endpoint:</strong> <code>{approval.endpoint}</code></p>
          <p>Expires: {new Date(approval.expiresAt).toLocaleString()}</p>
          <section aria-label="Exact tool arguments" style={{ border: "1px solid #94a3b8", borderRadius: 12, padding: 16 }}>
            <h2 style={{ fontWeight: 600, marginBottom: 8 }}>Arguments to be sent to Plane</h2>
            <pre style={{ overflowX: "auto", whiteSpace: "pre-wrap", wordBreak: "break-word", fontSize: 13 }}>{JSON.stringify(approval.arguments, null, 2)}</pre>
          </section>
          <div style={{ display: "flex", flexWrap: "wrap", gap: 12 }}>
            <button type="button" disabled={busy} onClick={() => void submit(true)} style={{ padding: "10px 18px", borderRadius: 8, backgroundColor: "#2563eb", color: "white", cursor: "pointer" }}>
              {busy ? "Processing…" : "Approve and execute"}
            </button>
            <button type="button" disabled={busy} onClick={() => void submit(false)} style={{ padding: "10px 18px", borderRadius: 8, border: "1px solid #94a3b8", cursor: "pointer" }}>
              Deny
            </button>
          </div>
        </>
      )}
      {result && (
        <section aria-live="polite">
          <p><strong>Result: {result.status}</strong></p>
          {result.message && <p>{result.message}</p>}
          {result.result !== undefined && <pre style={{ overflowX: "auto", whiteSpace: "pre-wrap", wordBreak: "break-word" }}>{JSON.stringify(result.result, null, 2)}</pre>}
          <p>You can return to ChatGPT. Check Plane before requesting any retry.</p>
        </section>
      )}
    </main>
  );
}
