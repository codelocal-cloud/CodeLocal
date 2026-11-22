"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslations } from "@/lib/i18n/provider";
import styles from "./public-build-notice.module.css";

const STORAGE_KEY = "codelocal-public-build-notice-last-shown-v1";
const DISPLAY_INTERVAL_MS = 60 * 60 * 1000;

type Copy = {
  eyebrow: string;
  title: string;
  body: string;
  body2: string;
  fork: string;
  feedback: string;
  close: string;
};

const copyByLocale: Record<string, Copy> = {
  en: {
    eyebrow: "Public build notice",
    title: "CodeLocal is still being opened up.",
    body: "codelocal.cloud is the CodeLocal build we currently run for people to try. The source on GitHub is not complete yet.",
    body2: "We do not copy the Enterprise codebase into the public repository. Each capability is implemented again using the proven Enterprise edition as a reference, so the open-source version is easier to understand, audit, fork and run independently. You are welcome to fork it and help us improve it in the Forums.",
    fork: "Fork on GitHub",
    feedback: "Feedback on Forums",
    close: "Continue",
  },
  vi: {
    eyebrow: "Thông báo về bản public",
    title: "CodeLocal vẫn đang trong quá trình mở mã nguồn.",
    body: "codelocal.cloud là bản CodeLocal đang chạy để mọi người dùng thử. Mã nguồn trên GitHub hiện chưa đầy đủ.",
    body2: "Chúng tôi không copy nguyên code Enterprise sang bản public. Thay vào đó, từng tính năng được viết lại dựa trên phiên bản Enterprise đã được kiểm chứng, để bản mã nguồn mở dễ hiểu, dễ audit, dễ fork và tự triển khai hơn. Mọi người có thể fork và góp ý trên Forums để cùng hoàn thiện CodeLocal.",
    fork: "Fork trên GitHub",
    feedback: "Góp ý trên Forums",
    close: "Tiếp tục",
  },
  "zh-Hans": {
    eyebrow: "公开版本说明",
    title: "CodeLocal 仍在逐步开放源代码。",
    body: "codelocal.cloud 是目前供大家体验的 CodeLocal 版本。GitHub 上的公开源码目前还不完整。",
    body2: "我们不会把 Enterprise 代码库直接复制到公开仓库。每项功能都会以已经过验证的 Enterprise 版本为参考重新实现，让开源版本更容易理解、审计、Fork 和独立部署。欢迎 Fork 并在 Forums 中一起完善 CodeLocal。",
    fork: "在 GitHub 上 Fork",
    feedback: "前往 Forums 反馈",
    close: "继续",
  },
  hi: {
    eyebrow: "पब्लिक बिल्ड सूचना",
    title: "CodeLocal का ओपन-सोर्स संस्करण अभी विकसित हो रहा है।",
    body: "codelocal.cloud वह CodeLocal build है जिसे हम अभी लोगों को इस्तेमाल करके देखने के लिए चला रहे हैं। GitHub पर public source अभी पूरा नहीं है।",
    body2: "हम Enterprise codebase को सीधे public repository में copy नहीं करते। हर capability को proven Enterprise edition को reference मानकर दोबारा implement किया जाता है, ताकि open-source version समझने, audit करने, fork करने और independently deploy करने में आसान हो। आप इसे fork करके Forums पर feedback दे सकते हैं।",
    fork: "GitHub पर Fork करें",
    feedback: "Forums पर Feedback दें",
    close: "जारी रखें",
  },
};

function readLastShown() {
  try {
    const value = Number(window.localStorage.getItem(STORAGE_KEY));
    return Number.isFinite(value) && value > 0 ? value : 0;
  } catch {
    return 0;
  }
}

function rememberShown(timestamp: number) {
  try {
    window.localStorage.setItem(STORAGE_KEY, String(timestamp));
  } catch {
    // Storage can be unavailable in strict privacy modes. The in-memory timer still prevents a loop.
  }
}

export function PublicBuildNotice() {
  const { locale } = useTranslations();
  const copy = useMemo(() => copyByLocale[locale] ?? copyByLocale.en, [locale]);
  const dialogRef = useRef<HTMLDialogElement>(null);
  const timerRef = useRef<number | null>(null);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    const schedule = (delay: number) => {
      if (timerRef.current !== null) window.clearTimeout(timerRef.current);
      timerRef.current = window.setTimeout(() => {
        const now = Date.now();
        rememberShown(now);
        setOpen(true);
        schedule(DISPLAY_INTERVAL_MS);
      }, delay);
    };

    const elapsed = Date.now() - readLastShown();
    schedule(elapsed >= DISPLAY_INTERVAL_MS ? 0 : DISPLAY_INTERVAL_MS - elapsed);

    return () => {
      if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    };
  }, []);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  }, [open]);

  return (
    <dialog
      ref={dialogRef}
      className={styles.dialog}
      aria-labelledby="public-build-notice-title"
      onCancel={(event) => {
        event.preventDefault();
        setOpen(false);
      }}
      onClose={() => setOpen(false)}
    >
      <div className={styles.card}>
        <div className={styles.eyebrow}>{copy.eyebrow}</div>
        <h2 id="public-build-notice-title">{copy.title}</h2>
        <p>{copy.body}</p>
        <p>{copy.body2}</p>
        <div className={styles.actions}>
          <a className={styles.primary} href="https://github.com/codelocal-cloud/codelocal" target="_blank" rel="noreferrer">{copy.fork}</a>
          <a className={styles.secondary} href="https://codelocal.cloud/forums">{copy.feedback}</a>
        </div>
        <button className={styles.close} type="button" onClick={() => setOpen(false)}>{copy.close}</button>
      </div>
    </dialog>
  );
}
