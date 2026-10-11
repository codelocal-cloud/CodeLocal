import assert from "node:assert/strict";
import { historyCursor } from "./chat-history-cursor.ts";

assert.deepEqual(historyCursor({
  messages: [
    { id: "older-message", createdAt: 1000 },
    { id: "newer-message", createdAt: 1001 },
  ],
}), { beforeId: "older-message", beforeTime: 1000 });
for (const payload of [
  null, {}, { messages: [] }, { messages: [{ id: "", createdAt: 1000 }] },
  { messages: [{ id: "missing-time" }] },
  { messages: [{ id: "bad-time", createdAt: Number.NaN }] },
  { messages: [{ id: "negative-time", createdAt: -1 }] },
]) {
  assert.equal(historyCursor(payload), null);
}
console.log("Chat history pagination cursor checks passed.");
