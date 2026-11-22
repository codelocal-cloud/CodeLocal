import type { ReactNode } from "react";

// Exact-name matching prevents unrelated plugins from displaying a trusted service logo.
function artwork(name: string): ReactNode {
  const key = name.toLowerCase().replace(/[^a-z0-9]/g, "");
  if (key === "github") return <path fill="currentColor" d="M12 2a10 10 0 0 0-3.16 19.49c.5.09.69-.21.69-.48v-1.7C6.73 19.91 6.14 18 6.14 18c-.46-1.17-1.13-1.48-1.13-1.48-.92-.63.07-.62.07-.62 1.02.07 1.56 1.04 1.56 1.04.9 1.55 2.36 1.1 2.94.85.09.35-1.1.63-1.35-2.25-.26-4.61-1.13-4.61-5.01 0-1.11.4-2.02 1.04-2.73-.11-.25-.45-1.29.1-2.69 0 0 .84-.27 2.75 1.04a9.51 9.51 0 0 1 5 0c1.91-1.31 2.75-1.04 2.75-1.04.55 1.4.21 2.44.1 2.69.65.71 1.04 1.62 1.04 2.73 0 3.89-2.37 4.75-4.63 5 .36.31.68.92.68 1.85v2.75c0 .27.18.58.7.48A10 10 0 0 0 12 2Z" />;
  if (key === "gmail") return <><path d="M3 7v11" stroke="#4285F4" strokeWidth="3.3" strokeLinecap="round"/><path d="m3 7 9 7 9-7" fill="none" stroke="#EA4335" strokeWidth="3.3" strokeLinecap="round" strokeLinejoin="round"/><path d="M21 7v11" stroke="#34A853" strokeWidth="3.3" strokeLinecap="round"/></>;
  if (key === "googledrive" || key === "drive") return <><path d="m9 3 4.2 0L22 18h-4.4Z" fill="#FFBA00"/><path d="M9 3 2 16.5l3 4.5L13.2 7Z" fill="#22AA66"/><path d="m5 21 3-4.5H22L19.1 21Z" fill="#4285F4"/></>;
  if (key === "figma") return <><rect x="4" y="2" width="9" height="7" rx="3.5" fill="#F24E1E"/><rect x="12" y="2" width="9" height="7" rx="3.5" fill="#FF7262"/><rect x="4" y="9" width="9" height="7" rx="3.5" fill="#A259FF"/><circle cx="16.5" cy="12.5" r="4.5" fill="#1ABCFE"/><rect x="4" y="16" width="9" height="7" rx="3.5" fill="#0ACF83"/></>;
  if (key === "slack") return <><path d="M9 2a2 2 0 0 1 2 2v6H7V4a2 2 0 0 1 2-2Z" fill="#36C5F0"/><path d="M15 22a2 2 0 0 1-2-2v-6h4v6a2 2 0 0 1-2 2Z" fill="#2EB67D"/><path d="M2 15a2 2 0 0 1 2-2h6v4H4a2 2 0 0 1-2-2Z" fill="#E01E5A"/><path d="M22 9a2 2 0 0 1-2 2h-6V7h6a2 2 0 0 1 2 2Z" fill="#ECB22E"/></>;
  if (key === "zoom") return <><rect x="3" y="7" width="12" height="10" rx="3" fill="currentColor"/><path d="m15 10 6-4v12l-6-4Z" fill="currentColor"/></>;
  if (key === "googletasks") return <><circle cx="12" cy="12" r="9" fill="#fff"/><path d="m7 12 3 3 7-7" fill="none" stroke="#2B78F6" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round"/></>;
  if (key === "shopify") return <><path d="M5 6 19 4l2 16-16 2Z" fill="#8DD251"/><path d="M9 7c0-3 5-5 6 0" fill="none" stroke="#eaf9d5" strokeWidth="2"/><path d="M14.5 10.5c-4-2-7 4-2 5 3 1 1 3-3 2" fill="none" stroke="#fff" strokeWidth="2.5"/></>;
  if (key === "notion") return <><path d="M3 5 18 3l3 3v14L6 22l-3-3Z" fill="currentColor"/><path d="M8 18V8l8 10V8" stroke="#fff" strokeWidth="2" fill="none"/></>;
  if (key === "googleads") return <><path d="M4 17 12 3h4l-9 17H3Z" fill="#4285F4"/><path d="m12 3 9 15h-6L9 8Z" fill="#FBBC04"/><circle cx="18.5" cy="18.5" r="3.5" fill="#34A853"/></>;
  return null;
}

/** Brand marks are decorative only; labels and connection state come from the live plugin API. */
export function PluginBrandIcon({ name }: { name: string }) {
  const mark = artwork(name);
  if (!mark) return <span aria-hidden="true">{name.slice(0, 2).toUpperCase()}</span>;
  return <svg aria-hidden="true" focusable="false" viewBox="0 0 24 24" width="30" height="30" fill="none">{mark}</svg>;
}