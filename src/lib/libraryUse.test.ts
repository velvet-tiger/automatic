import { beforeEach, describe, expect, it } from "vitest";
import { invokeMock, mockInvoke, resetInvokeMock } from "../test/tauriMock";
import {
  importExistingProject,
  libraryUseSummary,
  startsReadOnly,
  type LibraryUseGroup,
} from "./libraryUse";

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
      "Uses 5 items from your library, including 1 MCP server that can hold a stored sign-in or stored values and 2 hooks.",
    );
  });

  it("reports names that were not checked", () => {
    expect(libraryUseSummary([group({ total: 1, not_checked: 3 })])).toBe(
      "Uses 1 item from your library. 3 more names not checked.",
    );
  });
});

describe("startsReadOnly", () => {
  const readable = (read_only_hint: boolean, groups: LibraryUseGroup[] = []) =>
    ({ kind: "Readable", read_only_hint, groups }) as const;

  it("offers normal mode only after a full check with no hint", () => {
    expect(startsReadOnly(readable(false))).toBe(false);
    expect(startsReadOnly(readable(true))).toBe(true);
    expect(startsReadOnly(readable(false, [group({ not_checked: 1 })]))).toBe(true);
  });

  it("starts read-only when the check did not finish", () => {
    expect(startsReadOnly(null)).toBe(true);
    expect(startsReadOnly(undefined)).toBe(true);
    expect(startsReadOnly({ kind: "Unreadable" })).toBe(true);
    expect(startsReadOnly({ kind: "NotChecked" })).toBe(true);
  });
});

describe("importExistingProject", () => {
  beforeEach(() => resetInvokeMock());

  it("sends the chosen mode under the name the command reads", async () => {
    mockInvoke({ import_existing_project: { name: "x", local_key: "k", read_only: true } });

    await importExistingProject("/repo", true);

    expect(invokeMock).toHaveBeenCalledWith("import_existing_project", {
      directory: "/repo",
      readOnly: true,
    });
  });
});
