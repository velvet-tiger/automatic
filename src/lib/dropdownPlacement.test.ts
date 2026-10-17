import { describe, it, expect } from "vitest";
import { chooseDropdownSide } from "./dropdownPlacement";

const bounds = { top: 0, bottom: 800 };

describe("chooseDropdownSide", () => {
  it("opens below when the menu fits", () => {
    expect(chooseDropdownSide({ top: 100, bottom: 120 }, bounds, 288)).toBe("below");
  });

  it("flips above when the menu does not fit below and there is room above", () => {
    expect(chooseDropdownSide({ top: 700, bottom: 720 }, bounds, 288)).toBe("above");
  });

  it("stays below when there is less room above than below", () => {
    expect(chooseDropdownSide({ top: 20, bottom: 40 }, bounds, 900)).toBe("below");
  });

  it("counts the gap between the trigger and the menu", () => {
    // 288px of space below, but the menu plus its 4px gap needs 292px.
    expect(chooseDropdownSide({ top: 472, bottom: 512 }, bounds, 288)).toBe("above");
  });

  it("uses the scroll container's edge, not the viewport, as the bottom", () => {
    expect(chooseDropdownSide({ top: 500, bottom: 520 }, { top: 80, bottom: 600 }, 288)).toBe("above");
  });
});
