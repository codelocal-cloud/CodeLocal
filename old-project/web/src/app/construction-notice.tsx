"use client";

import { useEffect, useRef } from "react";
import { useTranslations } from "@/lib/i18n/provider";
import styles from "./construction-notice.module.css";

export function ConstructionNotice() {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const { t } = useTranslations();

  useEffect(() => {
    const dialog = dialogRef.current;
    if (dialog && !dialog.open) dialog.showModal();
  }, []);

  return (
    <dialog
      ref={dialogRef}
      className={styles.dialog}
      aria-labelledby="construction-notice-title"
      aria-describedby="construction-notice-description"
    >
      <div className={styles.card}>
        <div className={styles.icon} aria-hidden="true">!</div>
        <div className={styles.copy}>
          <div className={styles.eyebrow}>{t("Preview notice")}</div>
          <h2 id="construction-notice-title">{t("Feature under construction")}</h2>
          <p id="construction-notice-description">
            {t("This feature is currently under construction and acceptance testing. Some flows are being separated and reviewed again, so errors may occur while you use it.")}
          </p>
        </div>
        <button type="button" className={styles.button} onClick={() => dialogRef.current?.close()}>
          {t("I understand")}
        </button>
      </div>
    </dialog>
  );
}
