export type ChatHistoryCursor = { beforeTime: number; beforeId: string };

// History is returned oldest-first. The cursor must reference the first row,
// never the last, or consecutive pages can skip or repeat messages.
export function historyCursor(value: unknown): ChatHistoryCursor | null {
  if (!value || typeof value !== "object") return null;
  const records = (value as { messages?: unknown }).messages;
  if (!Array.isArray(records) || !records.length) return null;
  const first = records[0] as { id?: unknown; createdAt?: unknown } | null;
  return first && typeof first.id === "string" && first.id.trim() && typeof first.createdAt === "number" && Number.isSafeInteger(first.createdAt) && first.createdAt > 0
    ? { beforeId: first.id, beforeTime: first.createdAt }
    : null;
}
