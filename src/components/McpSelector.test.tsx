import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { McpSelector } from "./McpSelector";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

interface Rect {
  top: number;
  bottom: number;
}

// happy-dom has no layout engine, so each element reports the rect the test gives it.
function stubLayout(scroller: Rect, anchor: Rect, menuHeight: number) {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const rect = this.dataset.testid === "scroller" ? scroller : anchor;
    return { ...rect, left: 0, right: 0, width: 0, height: rect.bottom - rect.top, x: 0, y: rect.top, toJSON: () => ({}) };
  });
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(menuHeight);
}

describe("McpSelector", () => {
  afterEach(() => vi.restoreAllMocks());

  // VEL-456: the picker sits last in the profile editor, so its menu must flip
  // above the button. A hard-coded top-full collapsed the flipped menu.
  it("opens the add menu above the button near the bottom of a scroll area", () => {
    stubLayout({ top: 80, bottom: 600 }, { top: 560, bottom: 580 }, 288);
    render(
      <div data-testid="scroller" style={{ overflowY: "auto" }}>
        <McpSelector servers={[]} availableServers={["github", "linear"]} onAdd={() => {}} onRemove={() => {}} />
      </div>,
    );

    fireEvent.click(screen.getByText("Add from Library"));

    const menu = screen.getByPlaceholderText("Search MCP servers...").closest("div")!.parentElement!;
    expect(menu.className).toContain("bottom-full mb-1");
    expect(menu.className).not.toContain("top-full");
    expect(screen.getByText("github")).toBeInTheDocument();
  });
});
