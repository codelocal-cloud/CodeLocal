import type { KeyboardEvent } from "react";

// Keep keyboard navigation inside the active modal, including on mobile with
// hardware keyboards. Do not steal focus during normal form field edits.
export function trapChatDialogFocus(event: KeyboardEvent<HTMLElement>) {
  if (event.key !== "Tab") return;
  const focusable = Array.from(event.currentTarget.querySelectorAll<HTMLElement>(
    'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
  )).filter((item) => item.getClientRects().length > 0 && item.getAttribute("aria-hidden") !== "true");
  if (!focusable.length) return;
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  const focused = document.activeElement;
  if (event.shiftKey && (focused === first || !event.currentTarget.contains(focused))) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && (focused === last || !event.currentTarget.contains(focused))) {
    event.preventDefault();
    first.focus();
  }
}
