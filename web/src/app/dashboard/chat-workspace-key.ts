export type WorkspaceIdentity = {
  deviceId: string;
  workspaceId: string;
};

export type ThreadIdentity = {
  workspaceKey?: string;
};

export function workspaceIdentityKey(workspace: WorkspaceIdentity) {
  return `${workspace.deviceId}::${workspace.workspaceId}`;
}

export function threadBelongsToWorkspace(thread: ThreadIdentity, workspace: WorkspaceIdentity) {
  return thread.workspaceKey === workspaceIdentityKey(workspace);
}

// Build the execution request from the immutable thread binding, never from
// temporarily stale project-picker state during a thread change.
export function threadBoundWorkspace<T extends WorkspaceIdentity>(thread: ThreadIdentity | undefined, workspaces: readonly T[]): T | undefined {
  if (!thread?.workspaceKey) return undefined;
  return workspaces.find((workspace) => workspaceIdentityKey(workspace) === thread.workspaceKey);
}

export function projectTreeOpen(key: string, query: string, expanded: ReadonlySet<string>) {
  return query.trim().length > 0 || expanded.has(key);
}

export function toggleExpandedProject(current: ReadonlySet<string>, key: string) {
  const next = new Set(current);
  if (next.has(key)) next.delete(key);
  else next.add(key);
  return next;
}

export function threadCreationPayload(model: string, workspaceKey: string) {
  return { model, workspaceKey };
}
