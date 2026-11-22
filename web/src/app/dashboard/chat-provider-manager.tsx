"use client";

import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { AppIcon } from "./app-icon";
import { useTranslations } from "@/lib/i18n/provider";
import styles from "./chat-provider-manager.module.css";

type ProviderProtocol = "anthropic_messages" | "chat_completions" | "responses";
type AIProvider = {
  id: string;
  name: string;
  baseUrl: string;
  protocol: ProviderProtocol | "openai_compatible";
  models: string[];
  modelLabels?: Record<string, string>;
  enabled: boolean;
  hasCredential: boolean;
  lastTestStatus?: string;
  lastTestMessage?: string;
};
type ModelDraft = { key: string; model: string; label: string };
type ChatProviderManagerProps = { open: boolean; onClose: () => void; onChanged: () => void };

const newModelDraft = (): ModelDraft => ({
  key: typeof crypto !== "undefined" && typeof crypto.randomUUID === "function" ? crypto.randomUUID() : `${Date.now()}-${Math.random()}`,
  model: "",
  label: "",
});
const normalizeProtocol = (value: AIProvider["protocol"]): ProviderProtocol => value === "responses" || value === "anthropic_messages" ? value : "chat_completions";

export function ChatProviderManager({ open, onClose, onChanged }: ChatProviderManagerProps) {
  const { t } = useTranslations();
  const closeRef = useRef<HTMLButtonElement>(null);
  const [providers, setProviders] = useState<AIProvider[]>([]);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [protocol, setProtocol] = useState<ProviderProtocol>("chat_completions");
  const [models, setModels] = useState<ModelDraft[]>([newModelDraft()]);
  const [enabled, setEnabled] = useState(true);
  const [notice, setNotice] = useState<{ kind: "ok" | "error"; text: string } | null>(null);
  const selectedProvider = useMemo(() => providers.find((provider) => provider.id === selectedId) ?? null, [providers, selectedId]);

  function resetDraft() {
    setSelectedId(null); setName(""); setBaseUrl(""); setApiKey(""); setProtocol("chat_completions"); setModels([newModelDraft()]); setEnabled(true); setNotice(null);
  }
  function editProvider(provider: AIProvider) {
    setSelectedId(provider.id); setName(provider.name); setBaseUrl(provider.baseUrl); setApiKey(""); setProtocol(normalizeProtocol(provider.protocol)); setEnabled(provider.enabled); setNotice(null);
    setModels(provider.models.length ? provider.models.map((model) => ({ key: `${provider.id}:${model}`, model, label: provider.modelLabels?.[model] ?? "" })) : [newModelDraft()]);
  }
  async function loadProviders(preferredId?: string | null) {
    setLoading(true);
    try {
      const response = await fetch("/api/v1/dashboard/ai/providers", { credentials: "include" });
      if (!response.ok) throw new Error(String(response.status));
      const data = (await response.json()) as { providers?: AIProvider[] };
      const next = Array.isArray(data.providers) ? data.providers : [];
      setProviders(next);
      const id = preferredId ?? selectedId;
      if (id) { const current = next.find((provider) => provider.id === id); if (current) editProvider(current); }
    } catch { setNotice({ kind: "error", text: t("Could not load AI providers.") }); }
    finally { setLoading(false); }
  }

  useEffect(() => {
    if (!open) return;
    const previousOverflow = document.body.style.overflow;
    const closeOnEscape = (event: KeyboardEvent) => { if (event.key === "Escape") onClose(); };
    const frame = window.requestAnimationFrame(() => { resetDraft(); void loadProviders(null); closeRef.current?.focus(); });
    document.body.style.overflow = "hidden";
    document.addEventListener("keydown", closeOnEscape);
    return () => { window.cancelAnimationFrame(frame); document.body.style.overflow = previousOverflow; document.removeEventListener("keydown", closeOnEscape); };
  }, [open]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!open) return null;

  const modelIDs = models.map((item) => item.model.trim()).filter(Boolean);
  const uniqueModelIDs = new Set(modelIDs);
  const modelsValid = modelIDs.length > 0 && modelIDs.length === models.length && uniqueModelIDs.size === modelIDs.length;
  const credentialValid = selectedProvider?.hasCredential || apiKey.trim().length > 0;
  const canSave = name.trim() !== "" && baseUrl.trim() !== "" && credentialValid && modelsValid && !saving;

  function updateModel(key: string, field: "model" | "label", value: string) {
    setModels((current) => current.map((item) => item.key === key ? { ...item, [field]: value } : item));
  }
  function removeModel(key: string) {
    setModels((current) => current.length === 1 ? [newModelDraft()] : current.filter((item) => item.key !== key));
  }
  async function saveProvider(event: FormEvent) {
    event.preventDefault(); if (!canSave) return;
    setSaving(true); setNotice(null);
    const labels = Object.fromEntries(models.flatMap((item) => item.model.trim() && item.label.trim() ? [[item.model.trim(), item.label.trim()]] : []));
    const payload: Record<string, unknown> = { name: name.trim(), baseUrl: baseUrl.trim(), protocol, models: modelIDs, modelLabels: labels, enabled };
    if (apiKey.trim()) payload.apiKey = apiKey.trim();
    try {
      const editing = Boolean(selectedId);
      const response = await fetch(editing ? `/api/v1/dashboard/ai/providers/${encodeURIComponent(selectedId!)}` : "/api/v1/dashboard/ai/providers", {
        method: editing ? "PATCH" : "POST", credentials: "include", headers: { "content-type": "application/json" }, body: JSON.stringify(payload),
      });
      const data = (await response.json().catch(() => ({}))) as { provider?: AIProvider; error?: string };
      if (!response.ok || !data.provider) throw new Error(data.error || "save_failed");
      setApiKey(""); setNotice({ kind: "ok", text: editing ? "Provider updated." : "Provider added. Models are ready in Chat." });
      await loadProviders(data.provider.id); onChanged();
    } catch (error) { setNotice({ kind: "error", text: error instanceof Error && error.message !== "save_failed" ? error.message : t("Could not save this AI provider.") }); }
    finally { setSaving(false); }
  }
  async function testProvider() {
    if (!selectedProvider || testing) return; setTesting(true); setNotice(null);
    try {
      const response = await fetch(`/api/v1/dashboard/ai/providers/${encodeURIComponent(selectedProvider.id)}/test`, { method: "POST", credentials: "include" });
      const data = (await response.json().catch(() => ({}))) as { ok?: boolean };
      if (!response.ok || !data.ok) throw new Error("test_failed");
      setNotice({ kind: "ok", text: t("Connected. Models are ready to use in Chat.") }); await loadProviders(selectedProvider.id);
    } catch { setNotice({ kind: "error", text: "CodeLocal could not verify this provider. Check URL, key and API format." }); }
    finally { setTesting(false); }
  }
  async function deleteProvider() {
    if (!selectedProvider || deleting || !window.confirm(t("Remove {name}? The encrypted key and provider settings will be deleted.", { name: selectedProvider.name }))) return;
    setDeleting(true);
    try {
      const response = await fetch(`/api/v1/dashboard/ai/providers/${encodeURIComponent(selectedProvider.id)}`, { method: "DELETE", credentials: "include" });
      if (!response.ok && response.status !== 204) throw new Error(String(response.status));
      setProviders((current) => current.filter((provider) => provider.id !== selectedProvider.id)); resetDraft(); onChanged();
    } catch { setNotice({ kind: "error", text: t("Could not remove this AI provider.") }); }
    finally { setDeleting(false); }
  }

  return <div className={styles.layer}>
    <button className={styles.backdrop} type="button" onClick={onClose} aria-label={t("Close AI providers")} />
    <section className={styles.dialog} role="dialog" aria-modal="true" aria-labelledby="ai-provider-title">
      <header className={styles.head}><div><span className={styles.eyebrow}>{t("Your AI")}</span><h2 id="ai-provider-title">AI providers</h2><p>Add your own provider, API format and exact models. CodeLocal encrypts the key before storage.</p></div><button ref={closeRef} className={styles.close} type="button" onClick={onClose}><AppIcon name="close" size={18} /></button></header>
      <div className={styles.workspace}>
        <aside className={styles.providerPane}>
          <div className={styles.paneTitle}><strong>Providers</strong><span>{providers.length}</span></div>
          <div className={styles.providerList}>{loading && !providers.length ? <div className={styles.providerEmpty}>{t("Loading…")}</div> : null}{!loading && !providers.length ? <div className={styles.providerEmpty}>No providers yet.</div> : null}{providers.map((provider) => <button key={provider.id} type="button" className={`${styles.providerItem} ${selectedId === provider.id ? styles.providerItemActive : ""}`} onClick={() => editProvider(provider)}><span className={`${styles.statusDot} ${provider.lastTestStatus === "ok" ? styles.statusOK : provider.lastTestStatus === "error" ? styles.statusError : ""}`} /><span><strong>{provider.name}</strong><small>{provider.models.length} {provider.models.length === 1 ? "model" : "models"}</small></span><AppIcon name="chevron-right" size={14} /></button>)}</div>
          <button className={styles.addProviderNav} type="button" onClick={resetDraft}><AppIcon name="plus" size={15} /> Add provider</button>
        </aside>
        <form className={styles.editor} onSubmit={saveProvider}>
          <div className={styles.editorScroll}>
            <div className={styles.editorTitle}><div><strong>{selectedProvider ? selectedProvider.name : "Add provider"}</strong><small>{selectedProvider ? "Edit provider settings and models" : "Configure a provider for Chat"}</small></div>{selectedProvider ? <label className={styles.enabledToggle}><input type="checkbox" checked={enabled} onChange={(event) => setEnabled(event.target.checked)} /><span>Enabled</span></label> : null}</div>
            {notice ? <div className={`${styles.notice} ${notice.kind === "error" ? styles.noticeError : ""}`}>{notice.text}</div> : null}
            <div className={styles.fields}>
              <label><span>Name</span><input value={name} onChange={(event) => setName(event.target.value)} placeholder="OpenAI" maxLength={80} /></label>
              <label><span>Base URL</span><input value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} placeholder="https://api.openai.com/v1" inputMode="url" /></label>
              <label className={styles.fullField}><span>API key</span><input value={apiKey} onChange={(event) => setApiKey(event.target.value)} placeholder={selectedProvider?.hasCredential ? "Saved securely — enter a new key to replace" : "••••••••••••••••"} type="password" autoComplete="off" /><small><AppIcon name="shield" size={12} /> Encrypted before storage. Never returned to the browser.</small></label>
              <label className={styles.fullField}><span>API format</span><select value={protocol} onChange={(event) => setProtocol(event.target.value as ProviderProtocol)}><option value="anthropic_messages">Anthropic Messages (/v1/messages)</option><option value="chat_completions">Chat Completions (/chat/completions)</option><option value="responses">Responses (/responses)</option></select></label>
            </div>
            <section className={styles.modelsSection}><div className={styles.modelsHead}><div><strong>Models</strong><small>Add the exact model IDs exposed by this provider.</small></div><button type="button" onClick={() => setModels((current) => [...current, newModelDraft()])}><AppIcon name="plus" size={14} /> Add model</button></div><div className={styles.modelRows}>{models.map((item, index) => <div className={styles.modelRow} key={item.key}><label><span>Model ID</span><input value={item.model} onChange={(event) => updateModel(item.key, "model", event.target.value)} placeholder={index === 0 ? "gpt-5.6-sol" : "model-id"} /></label><label><span>Display name <em>optional</em></span><input value={item.label} onChange={(event) => updateModel(item.key, "label", event.target.value)} placeholder="My coding model" /></label><button className={styles.removeModel} type="button" onClick={() => removeModel(item.key)} aria-label="Remove model"><AppIcon name="close" size={15} /></button></div>)}</div>{uniqueModelIDs.size !== modelIDs.length ? <p className={styles.validation}>Each Model ID must be unique.</p> : null}</section>
          </div>
          <footer className={styles.footer}>{selectedProvider ? <div className={styles.secondaryActions}><button type="button" onClick={() => void testProvider()} disabled={testing}>{testing ? t("Testing…") : "Test connection"}</button><button className={styles.danger} type="button" onClick={() => void deleteProvider()} disabled={deleting}>{deleting ? "Removing…" : t("Remove")}</button></div> : <span />}<button className={styles.primary} type="submit" disabled={!canSave}>{saving ? t("Saving…") : selectedProvider ? "Save changes" : "Add provider"}</button></footer>
        </form>
      </div>
    </section>
  </div>;
}
