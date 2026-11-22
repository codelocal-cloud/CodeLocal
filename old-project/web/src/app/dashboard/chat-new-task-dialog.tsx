"use client";

import { useEffect, useRef } from "react";
import type { RuntimeExecutionMode } from "@/lib/contracts/runtime-settings";
import { useTranslations } from "@/lib/i18n/provider";
import { AppIcon } from "./app-icon";
import styles from "./chat-new-task-dialog.module.css";

export const GENERAL_PROJECT_KEY = "__general__";

export type ChatNewTaskProject = {
  key: string;
  name: string;
  deviceName: string;
  statusLabel: string;
  active: boolean;
};

export type ChatNewTaskModel = {
  id: string;
  label: string;
  provider: string;
};

type ChatNewTaskDialogProps = {
  open: boolean;
  projects: ChatNewTaskProject[];
  selectedProjectKey: string;
  executionMode: RuntimeExecutionMode;
  executionConfigured: boolean;
  executionLoading: boolean;
  executionError: boolean;
  models: ChatNewTaskModel[];
  selectedModel: string;
  busy: boolean;
  onProjectChange: (key: string) => void;
  onExecutionModeChange: (mode: RuntimeExecutionMode) => void;
  onModelChange: (model: string) => void;
  onClose: () => void;
  onStart: () => void;
};

export function ChatNewTaskDialog({
  open,
  projects,
  selectedProjectKey,
  executionMode,
  executionConfigured,
  executionLoading,
  executionError,
  models,
  selectedModel,
  busy,
  onProjectChange,
  onExecutionModeChange,
  onModelChange,
  onClose,
  onStart,
}: ChatNewTaskDialogProps) {
  const { t } = useTranslations();
  const closeRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    const previousOverflow = document.body.style.overflow;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) onClose();
    };
    document.body.style.overflow = "hidden";
    document.addEventListener("keydown", closeOnEscape);
    window.requestAnimationFrame(() => closeRef.current?.focus());
    return () => {
      document.body.style.overflow = previousOverflow;
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [busy, onClose, open]);

  if (!open) return null;

  const projectSelected = Boolean(selectedProjectKey);
  const generalSelected = selectedProjectKey === GENERAL_PROJECT_KEY;
  const executionReady = generalSelected || (!executionLoading && !executionError);
  const canStart = projectSelected && executionReady && Boolean(selectedModel) && !busy;

  return (
    <div className={styles.layer}>
      <button className={styles.backdrop} type="button" onClick={busy ? undefined : onClose} aria-label={t("Close new task")} />
      <section className={styles.dialog} role="dialog" aria-modal="true" aria-labelledby="new-task-title">
        <header className={styles.head}>
          <div>
            <span className={styles.eyebrow}>{t("CodeLocal Chat")}</span>
            <h2 id="new-task-title">{t("New task")}</h2>
            <p>{t("Choose the project and how CodeLocal should work before starting.")}</p>
          </div>
          <button ref={closeRef} className={styles.close} type="button" onClick={onClose} disabled={busy} aria-label={t("Close new task")}>
            <AppIcon name="close" size={18} />
          </button>
        </header>

        <div className={styles.body}>
          <section className={styles.section}>
            <div className={styles.sectionHead}>
              <div><strong>{t("Project")}</strong><span>{t("A task stays bound to one project.")}</span></div>
            </div>
            <div className={styles.projectList}>
              <button
                className={`${styles.projectCard} ${generalSelected ? styles.selected : ""}`}
                type="button"
                aria-pressed={generalSelected}
                onClick={() => onProjectChange(GENERAL_PROJECT_KEY)}
              >
                <span className={styles.projectIcon}><AppIcon name="chat" size={18} /></span>
                <span><strong>{t("General")}</strong><small>{t("Chat without a project or coding workspace.")}</small></span>
                {generalSelected ? <AppIcon name="check" size={16} /> : null}
              </button>
              {projects.map((project) => {
                const selected = selectedProjectKey === project.key;
                return (
                  <button
                    className={`${styles.projectCard} ${selected ? styles.selected : ""}`}
                    type="button"
                    aria-pressed={selected}
                    onClick={() => onProjectChange(project.key)}
                    key={project.key}
                  >
                    <span className={styles.projectIcon}><AppIcon name="folder" size={18} /></span>
                    <span><strong>{project.name}</strong><small>{project.deviceName} · {project.statusLabel}</small></span>
                    <i className={styles.statusDot} data-active={project.active} aria-hidden="true" />
                    {selected ? <AppIcon name="check" size={16} /> : null}
                  </button>
                );
              })}
            </div>
            {!projectSelected ? <p className={styles.help}>{t("Choose a project to continue.")}</p> : null}
          </section>

          {!generalSelected && projectSelected ? (
            <section className={styles.section}>
              <div className={styles.sectionHead}>
                <div><strong>{t("Execution Mode")}</strong><span>{t("Choose how CodeLocal changes this project")}</span></div>
              </div>
              {executionLoading ? <div className={styles.loading}>{t("Loading execution mode…")}</div> : null}
              {executionError ? <div className={styles.error}>{t("Could not load execution mode.")}</div> : null}
              {!executionLoading && !executionError ? (
                <>
                  <div className={styles.executionGrid}>
                    <button
                      className={`${styles.executionCard} ${executionMode === "safe" ? styles.selected : ""}`}
                      type="button"
                      aria-pressed={executionMode === "safe"}
                      onClick={() => onExecutionModeChange("safe")}
                    >
                      <span className={styles.executionIcon}><AppIcon name="shield" size={18} /></span>
                      <span><strong>{t("Safe Workspace")}</strong><small>{t("Work in an isolated copy. Recommended for most tasks.")}</small></span>
                      {executionMode === "safe" ? <AppIcon name="check" size={16} /> : null}
                    </button>
                    <button
                      className={`${styles.executionCard} ${styles.liveCard} ${executionMode === "live" ? styles.selected : ""}`}
                      type="button"
                      aria-pressed={executionMode === "live"}
                      onClick={() => onExecutionModeChange("live")}
                    >
                      <span className={styles.executionIcon}><AppIcon name="runtime" size={18} /></span>
                      <span><strong>{t("Live Project")}</strong><small>{t("Edit the current project directly. Best for debugging a running app.")}</small></span>
                      {executionMode === "live" ? <AppIcon name="check" size={16} /> : null}
                    </button>
                  </div>
                  <p className={styles.help}>{executionConfigured ? t("Saved for this project.") : t("Choose once. CodeLocal remembers this setting for the project.")}</p>
                </>
              ) : null}
            </section>
          ) : null}

          <section className={styles.section}>
            <div className={styles.sectionHead}>
              <div><strong>{t("AI Model")}</strong><span>{t("Choose the model for this task.")}</span></div>
            </div>
            <label className={styles.modelSelect}>
              <AppIcon name="codelocal" size={17} />
              <select value={selectedModel} onChange={(event) => onModelChange(event.target.value)} disabled={busy || models.length === 0}>
                {models.length === 0 ? <option value="">{t("No AI model")}</option> : null}
                {models.map((model) => <option value={model.id} key={model.id}>{model.label} · {model.provider}</option>)}
              </select>
            </label>
          </section>

          {projectSelected ? <div className={styles.notice}><AppIcon name="connection" size={15} /><span>{t("Switching project or execution mode starts a new task.")}</span></div> : null}
        </div>

        <footer className={styles.footer}>
          <button className={styles.cancel} type="button" onClick={onClose} disabled={busy}>{t("Cancel")}</button>
          <button className={styles.start} type="button" onClick={onStart} disabled={!canStart}>
            {busy ? t("Starting…") : t("Start task")}
          </button>
        </footer>
      </section>
    </div>
  );
}
