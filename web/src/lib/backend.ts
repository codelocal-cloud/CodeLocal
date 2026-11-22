export type BackendStatus = {
  service: string;
  state: string;
  version: string;
};

const backendUrl = process.env.BACKEND_INTERNAL_URL ?? "http://127.0.0.1:8080";

export async function getBackendStatus(): Promise<BackendStatus | null> {
  try {
    const response = await fetch(new URL("/api/v1/system/status", backendUrl), {
      cache: "no-store",
      signal: AbortSignal.timeout(1500),
    });
    if (!response.ok) return null;
    const data: unknown = await response.json();
    if (!data || typeof data !== "object") return null;
    const candidate = data as Partial<BackendStatus>;
    if (typeof candidate.service !== "string" ||
        typeof candidate.state !== "string" ||
        typeof candidate.version !== "string") return null;
    return {
      service: candidate.service,
      state: candidate.state,
      version: candidate.version,
    };
  } catch {
    return null;
  }
}
