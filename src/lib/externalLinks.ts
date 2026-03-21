import type { MouseEvent } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";

export async function openExternalUrl(url: string): Promise<void> {
  try {
    await openUrl(url);
  } catch {
    window.open(url, "_blank", "noopener,noreferrer");
  }
}

export function handleExternalLinkClick(url: string, stopPropagation = false) {
  return (event: MouseEvent<HTMLAnchorElement>) => {
    event.preventDefault();
    if (stopPropagation) event.stopPropagation();
    void openExternalUrl(url);
  };
}
