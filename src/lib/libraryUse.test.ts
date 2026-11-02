import { describe, expect, it } from "vitest";
import { libraryUseSummary, type LibraryUseGroup } from "./libraryUse";

const group = (over: Partial<LibraryUseGroup>): LibraryUseGroup => ({
  kind: "skill",
  total: 0,
  flagged_total: 0,
  names: [],
  not_checked: 0,
  ...over,
});

describe("libraryUseSummary", () => {
  it("returns nothing when no library item is used", () => {
    expect(libraryUseSummary([])).toBeNull();
  });

  it("names the risky items", () => {
    const line = libraryUseSummary([
      group({ kind: "mcp_server", total: 2, flagged_total: 1 }),
      group({ kind: "hook", total: 2, flagged_total: 2 }),
      group({ kind: "skill", total: 1 }),
    ]);
    expect(line).toBe(
      "Uses 5 items from your library, including 1 MCP server with stored values and 2 hooks.",
    );
  });

  it("reports names that were not checked", () => {
    expect(libraryUseSummary([group({ total: 1, not_checked: 3 })])).toBe(
      "Uses 1 item from your library. 3 more names not checked.",
    );
  });
});
