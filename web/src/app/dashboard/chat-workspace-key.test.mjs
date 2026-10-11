import assert from "node:assert/strict";
import { projectTreeOpen, threadBelongsToWorkspace, threadBoundWorkspace, threadCreationPayload, toggleExpandedProject, workspaceIdentityKey } from "./chat-workspace-key.ts";

const workspace = { deviceId: "device-1", workspaceId: "workspace-1" };
const key = workspaceIdentityKey(workspace);

assert.equal(key, "device-1::workspace-1");
assert.equal(threadBelongsToWorkspace({ workspaceKey: key }, workspace), true);
assert.equal(threadBelongsToWorkspace({ workspaceKey: "device-1::workspace-2" }, workspace), false);
assert.equal(threadBelongsToWorkspace({}, workspace), false);

const other = { deviceId: "device-2", workspaceId: "workspace-2" };
assert.deepEqual(threadBoundWorkspace({workspaceKey:key}, [other, workspace]), workspace);
assert.equal(threadBoundWorkspace({workspaceKey:"device-9::missing"}, [workspace, other]), undefined);
assert.equal(threadBoundWorkspace({}, [workspace]), undefined);
assert.equal(threadBoundWorkspace(undefined, [workspace]), undefined);

const initiallyOpen = new Set([key]);
assert.equal(projectTreeOpen(key, "", initiallyOpen), true);
const collapsed = toggleExpandedProject(initiallyOpen, key);
assert.equal(projectTreeOpen(key, "", collapsed), false);
assert.equal(projectTreeOpen(key, "search", collapsed), true);
const reopened = toggleExpandedProject(collapsed, key);
assert.equal(projectTreeOpen(key, "", reopened), true);

assert.deepEqual(threadCreationPayload("auto", key), { model: "auto", workspaceKey: "device-1::workspace-1" });
assert.deepEqual(threadCreationPayload("gpt-5", ""), { model: "gpt-5", workspaceKey: "" });
