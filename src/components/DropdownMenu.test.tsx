import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { DropdownMenu } from "./DropdownMenu";

interface Rect {
  top: number;
  bottom: number;
}

// happy-dom has no layout engine, so each element reports the rect the test gives it.
function stubLayout(rects: Record<string, Rect>, menuHeight: number) {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const rect = rects[this.dataset.testid ?? ""] ?? { top: 0, bottom: 0 };
    return { ...rect, left: 0, right: 0, width: 0, height: rect.bottom - rect.top, x: 0, y: rect.top, toJSON: () => ({}) };
  });
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(menuHeight);
}

function renderMenu() {
  render(
    <div data-testid="scroller" style={{ overflowY: "auto" }}>
      <div data-testid="anchor" className="relative">
        <DropdownMenu className="absolute right-0 w-72">
          <span>menu body</span>
        </DropdownMenu>
      </div>
    </div>,
  );
  return screen.getByText("menu body").parentElement!;
}

describe("DropdownMenu", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("opens below the trigger when it fits in the scroll container", () => {
    stubLayout({ scroller: { top: 80, bottom: 600 }, anchor: { top: 100, bottom: 120 } }, 288);
    const menu = renderMenu();
    expect(menu.className).toContain("top-full mt-1");
    expect(menu.className).not.toContain("bottom-full");
  });

  it("flips above the trigger when the scroll container's bottom edge is too close", () => {
    stubLayout({ scroller: { top: 80, bottom: 600 }, anchor: { top: 560, bottom: 580 } }, 288);
    const menu = renderMenu();
    expect(menu.className).toContain("bottom-full mb-1");
    expect(menu.className).not.toContain("top-full");
  });
});
