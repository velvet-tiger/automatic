import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const openUrl = vi.fn<(url: string) => Promise<void>>();
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: (url: string) => openUrl(url) }));

import { isOpenableExternalUrl, openExternalUrl } from "./externalLinks";

describe("isOpenableExternalUrl", () => {
  it.each(["https://tryautomatic.app", "http://localhost:5173/", "mailto:someone@example.com", "  https://x.io  "])(
    "allows %s",
    (url) => {
      expect(isOpenableExternalUrl(url)).toBe(true);
    },
  );

  it.each([
    "javascript:alert(1)",
    "JavaScript:alert(1)",
    "file:///Applications/Calculator.app",
    "vscode://file/etc/passwd",
    "data:text/html,<script>alert(1)</script>",
    "tel:123",
    "/relative/path",
    "",
    "not a url",
  ])("refuses %s", (url) => {
    expect(isOpenableExternalUrl(url)).toBe(false);
  });
});

describe("openExternalUrl", () => {
  const windowOpen = vi.fn();

  beforeEach(() => {
    openUrl.mockReset();
    windowOpen.mockReset();
    vi.stubGlobal("window", { open: windowOpen });
    vi.spyOn(console, "warn").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("opens an allowed URL with the system opener", async () => {
    openUrl.mockResolvedValue();
    await openExternalUrl("https://tryautomatic.app");
    expect(openUrl).toHaveBeenCalledWith("https://tryautomatic.app");
    expect(windowOpen).not.toHaveBeenCalled();
  });

  it("falls back to window.open for an allowed URL the opener fails on", async () => {
    openUrl.mockRejectedValue(new Error("no handler"));
    await openExternalUrl("https://tryautomatic.app");
    expect(windowOpen).toHaveBeenCalledWith("https://tryautomatic.app", "_blank", "noopener,noreferrer");
  });

  it("opens nothing for a refused scheme", async () => {
    await openExternalUrl("javascript:alert(1)");
    await openExternalUrl("file:///etc/passwd");
    expect(openUrl).not.toHaveBeenCalled();
    expect(windowOpen).not.toHaveBeenCalled();
  });
});
