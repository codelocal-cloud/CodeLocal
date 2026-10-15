"use client";

import { AppIcon } from "./app-icon";
import { PluginApproval } from "./plugin-approval";
import { useTranslations } from "@/lib/i18n/provider";
import type { MessageKey } from "@/lib/i18n/messages";
import styles from "./chat-action-summary.module.css";

export type ChatToolCall = {
  id: string;
  name: string;
  arguments: string;
  result?: string;
  durationMs?: number;
  status: "running" | "done" | "error" | "approval_required";
};

const actionLabels: Partial<Record<string, MessageKey>> = {
  list_workspaces: "View workspaces",
  list_devices: "Check devices",
  search_project_brain: "Search Project Brain",
  get_workspace_detail: "Read project",
  list_project_files: "View source code",
  read_project_file: "Read file",
  search_project_code: "Search code",
  edit_project_file: "Edit code",
  write_project_file: "Update file",
  apply_project_patch: "Apply changes",
  run_project_command: "Run command",
  poll_project_command: "Wait for command",
  verify_project_changes: "Verify changes",
};

function actionLabel(name: string, t: ReturnType<typeof useTranslations>["t"]) {
  const key = actionLabels[name];
  return key ? t(key) : name.replaceAll("_", " ");
}

function durationLabel(durationMs: number) {
  if (durationMs < 1000) return `${durationMs}ms`;
  if (durationMs < 60_000) return `${(durationMs / 1000).toFixed(durationMs < 10_000 ? 1 : 0)}s`;
  const minutes = Math.floor(durationMs / 60_000);
  const seconds = Math.round((durationMs % 60_000) / 1000);
  return seconds ? `${minutes}m ${seconds}s` : `${minutes}m`;
}

type ActionPermission = { status: string; command: string; reason: string; riskLevel: string; canApproveOnce: boolean };

function actionPermission(raw?: string): ActionPermission | null {
  if (!raw) return null;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object") return null;
    const outer = parsed as Record<string, unknown>;
    const nested = outer.result;
    const data = nested && typeof nested === "object" ? nested as Record<string, unknown> : outer;
    if (data.status !== "approval_required" && data.status !== "blocked") return null;
    const token = typeof data.approvalToken === "string" ? data.approvalToken : "";
    const expiresAt = typeof data.expiresAt === "number" ? data.expiresAt : 0;
    return {
      status: String(data.status),
      command: typeof data.command === "string" ? data.command : "",
      reason: typeof data.reason === "string" ? data.reason : "",
      riskLevel: typeof data.riskLevel === "string" ? data.riskLevel : "",
      canApproveOnce: data.status === "approval_required" && /^[a-f0-9]{32}\.[a-f0-9]{64}$/.test(token) && expiresAt > Date.now(),
    };
  } catch {
    return null;
  }
}

function safeApprovalResult(raw: string) {
  // A one-time approval token is a capability. Never print it in the chat UI.
  return raw.replace(/("approvalToken"\s*:\s*")[^"]+(")/g, '$1[HIDDEN]$2');
}

function summaryState(actions: ChatToolCall[]) {
  if (actions.some((action) => action.status === "approval_required")) return "approval";
  if (actions.some((action) => action.status === "running")) return "running";
  if (actions.some((action) => action.status === "error")) return "error";
  return "done";
}

function summaryLabel(actions: ChatToolCall[], state: ReturnType<typeof summaryState>, t: ReturnType<typeof useTranslations>["t"]) {
  if (state === "approval") return t("Waiting for approval");
  if (state === "running") return t("{count} steps running", { count: actions.filter((action) => action.status === "running").length });
  if (state === "error") {
    const failed = actions.filter((action) => action.status === "error").length;
    return t("{done} complete · {failed} errors", { done: actions.length - failed, failed });
  }
  return t("{count} steps completed", { count: actions.length });
}

export function ChatActionSummary({ actions, busy, onApprove, onApproveOnce, onDecline }: { actions: ChatToolCall[]; busy: boolean; onApprove: () => void; onApproveOnce: (action: ChatToolCall) => void; onDecline: (action: ChatToolCall) => void }) {
  const { t } = useTranslations();
  const state = summaryState(actions);
  const durationMs = actions.reduce((total, action) => total + Math.max(0, action.durationMs || 0), 0);
  const approvals = actions.filter((action) => action.status === "approval_required");
  const stateLabel = state === "approval" ? t("Approval required") : state === "error" ? t("Completed with errors") : state === "running" ? t("Running") : t("Completed");
  const label = summaryLabel(actions, state, t);

  return (
    <div className={styles.actionGroup} data-state={state}>
      <details className={styles.disclosure}>
        <summary role="button" aria-label={`${label}${durationMs > 0 ? `, ${durationLabel(durationMs)}` : ""}`}>
          <span className={styles.stateIcon} aria-hidden="true">{state === "done" ? <AppIcon name="check" size={13} /> : <i />}</span>
          <span className={styles.summaryCopy} aria-live="polite">
            <strong>{label}</strong>
            <small>{stateLabel}{durationMs > 0 ? ` · ${durationLabel(durationMs)}` : ""}</small>
          </span>
          <AppIcon className={styles.chevron} name="chevron-right" size={14} />
        </summary>
        <ol className={styles.timeline}>
          {actions.map((action) => { const permission = actionPermission(action.result); return (
            <li key={action.id} data-state={action.status}>
              <span className={styles.timelineDot} aria-hidden="true" />
              <div className={styles.actionCopy}>
                <strong>{actionLabel(action.name, t)}</strong>
                <small>{action.status === "approval_required" ? t("Needs permission") : action.status === "error" ? t("Error") : action.status === "running" ? t("Running") : t("Completed")}{action.durationMs ? ` · ${durationLabel(action.durationMs)}` : ""}</small>
                {permission?.reason ? <small className={styles.policyReason} role="status">{permission.status === "blocked" ? "Security policy blocked this action: " : "Approval needed: "}{permission.reason}</small> : null}
                {action.arguments || action.result ? (
                  <details className={styles.actionData}>
                    <summary>{t("Details")}</summary>
                    {action.arguments ? <code>{action.arguments}</code> : null}
                    {action.result ? <code>{safeApprovalResult(action.result).slice(0, 800)}{safeApprovalResult(action.result).length > 800 ? "…" : ""}</code> : null}
                  </details>
                ) : null}
              </div>
            </li>
          ); })}
        </ol>
      </details>
      {approvals.map((action) => {
        if (action.name === "call_plugin_tool") {
          let request: { approvalId?: unknown; plugin?: unknown; connection?: unknown; tool?: unknown; arguments?: unknown } = {};
          try { request = JSON.parse(action.result || "{}"); } catch { /* Missing approval data must not enable execution. */ }
          if (typeof request.approvalId !== "string" || !/^[a-f0-9]{48}$/.test(request.approvalId)) return null;
          return <PluginApproval key={`${action.id}-approval`} id={request.approvalId} busy={busy} />;
        }
        const permission = actionPermission(action.result);
        return <div className={styles.approvalRow} key={`${action.id}-approval`}>
          <span><AppIcon name="shield" size={15} /><strong>{actionLabel(action.name, t)}</strong>{permission?.command ? <small className={styles.approvalCommand}>{permission.command}</small> : null}<small>{permission?.reason || t("Needs permission to continue")}</small><small>Allow once applies only to this command; Full access changes the whole workspace.</small></span>
          <div className={styles.approvalChoices}>
            {permission?.canApproveOnce ? <button type="button" className={styles.allowOnce} onClick={() => {
              if (window.confirm(`Allow this command once?\n\n${permission.command || action.name}\n\n${permission.reason}`)) onApproveOnce(action);
            }} disabled={busy} title="Approve this command once only">Allow once</button> : null}
            <button type="button" onClick={() => onDecline(action)} disabled={busy}>Deny</button>
            <button type="button" onClick={onApprove} disabled={busy} title="Change workspace-wide permission">{t("Full access")}</button>
          </div>
        </div>;
      })}
    </div>
  );
}
