import type { MouseEvent } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";

/** The only schemes the app hands to the system browser or mail client. */
const ALLOWED_SCHEMES: ReadonlySet<string> = new Set(["http:", "https:", "mailto:"]);

/**
 * Whether `url` is an absolute URL with a scheme Automatic will open. Links
 * reach the opener from remote feeds, catalogues and dev-server logs, so a
 * `file:`, `javascript:` or custom-scheme URL must never get through.
 */
export function isOpenableExternalUrl(url: string): boolean {
  let parsed: URL;
  try {
    parsed = new URL(url.trim());
  } catch {
    return false;
  }
  return ALLOWED_SCHEMES.has(parsed.protocol);
}

/**
 * Open `url` in the user's browser or mail client. A URL with any other
 * scheme is refused. The `window.open` fallback covers an opener failure only
 * after the scheme check, so it cannot bypass the opener's own scope.
 */
export async function openExternalUrl(url: string): Promise<void> {
  if (!isOpenableExternalUrl(url)) {
    console.warn(`Refused to open a link with an unsupported scheme: ${url}`);
    return;
  }
  const target = url.trim();
  try {
    await openUrl(target);
  } catch (err: unknown) {
    console.warn(`The system opener failed for ${target}, opening in a new window instead`, err);
    window.open(target, "_blank", "noopener,noreferrer");
  }
}

export function handleExternalLinkClick(url: string, stopPropagation = false) {
  return (event: MouseEvent<HTMLAnchorElement>) => {
    event.preventDefault();
    if (stopPropagation) event.stopPropagation();
    void openExternalUrl(url);
  };
}
