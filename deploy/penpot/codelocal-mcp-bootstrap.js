(() => {
  "use strict";

  const MESSAGE_TYPE = "codelocal:penpot-mcp-token";
  const REQUEST_TYPE = "codelocal:penpot-mcp-token-request";
  const ALLOWED_PARENT_ORIGINS = new Set([
    "https://codelocal.cloud",
    "http://localhost:3000",
    "http://localhost:3333",
  ]);

  if (window.parent === window) return;

  // OIDC redirects can strip or replace document.referrer inside the iframe.
  // Accept only a verified parent-window handshake from an allowed origin.
  let parentOrigin = "";
  try {
    const referrerOrigin = new URL(document.referrer).origin;
    if (ALLOWED_PARENT_ORIGINS.has(referrerOrigin)) parentOrigin = referrerOrigin;
  } catch {
    // The trusted parent may still establish its origin via postMessage.
  }

  let provisioning = false;
  let deliveredToken = "";

  async function rpc(command, body) {
    const response = await fetch(`/api/rpc/command/${command}`, {
      method: "POST",
      credentials: "include",
      cache: "no-store",
      headers: {
        Accept: "application/json",
        "Content-Type": "application/json",
      },
      body: JSON.stringify(body),
    });
    if (!response.ok) throw new Error("Penpot token lookup unavailable");
    return response.json();
  }

  function tokenFromURL(value) {
    if (typeof value !== "string" || !value) return "";
    try {
      return new URL(value, window.location.origin).searchParams.get("userToken")?.trim() || "";
    } catch {
      return "";
    }
  }

  function extractToken(value) {
    if (!value || typeof value !== "object" || Array.isArray(value)) return "";
    const record = value;
    for (const key of ["token", "accessToken", "access-token", "mcpToken", "mcp-token", "currentMcpToken"]) {
      const candidate = record[key];
      if (typeof candidate === "string" && candidate.trim().length >= 20) return candidate.trim();
    }
    for (const key of ["url", "serverUrl", "server-url", "mcpUrl", "mcp-url"]) {
      const candidate = tokenFromURL(record[key]);
      if (candidate) return candidate;
    }
    for (const key of ["result", "data"]) {
      const candidate = extractToken(record[key]);
      if (candidate) return candidate;
    }
    return "";
  }

  async function provision(forceDeliver = false) {
    if (!parentOrigin || provisioning || document.visibilityState === "hidden") return;
    provisioning = true;
    try {
      let token = extractToken(await rpc("get-current-mcp-token", {}));
      if (!token) {
        token = extractToken(await rpc("create-access-token", {
          name: "CodeLocal MCP",
          type: "mcp",
        }));
      }
      if (!token || (!forceDeliver && token === deliveredToken)) return;
      deliveredToken = token;
      window.parent.postMessage({ type: MESSAGE_TYPE, token }, parentOrigin);
    } catch {
      // Failed lookups must not rotate the account's existing MCP key.
    } finally {
      provisioning = false;
    }
  }

  window.addEventListener("message", (event) => {
    if (event.source !== window.parent || event.data?.type !== REQUEST_TYPE ||
        !ALLOWED_PARENT_ORIGINS.has(event.origin)) return;
    parentOrigin = event.origin;
    // Recheck the live session after the trusted parent completes its handshake.
    void provision(true);
  });

  void provision();
  window.setTimeout(() => void provision(), 2500);
  window.addEventListener("focus", () => void provision());
  window.addEventListener("hashchange", () => void provision());
  document.addEventListener("visibilitychange", () => void provision());
  window.setInterval(() => void provision(), 30000);
})();
