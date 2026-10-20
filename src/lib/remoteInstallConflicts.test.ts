import { describe, it, expect } from "vitest";
import {
  conflictKey,
  describeConflict,
  indexConflicts,
  initialSelection,
  isGroupSelected,
  isSelectable,
  overridesFor,
  toggleGroupSelection,
  withoutConflicts,
  type SourceConflict,
} from "./remoteInstallConflicts";

function conflict(partial: Partial<SourceConflict>): SourceConflict {
  return {
    resource_type: "rule",
    resource_name: "style",
    existing_source: "local",
    existing_name: "style",
    blocked: false,
    reason: "Replaces your own 'style'",
    ...partial,
  };
}

const groups = [
  { key: "rules" as const, items: [{ name: "style" }, { name: "fresh" }] },
  { key: "mcp_servers" as const, items: [{ name: "automatic" }, { name: "github" }] },
];

const conflicts = indexConflicts([
  conflict({}),
  conflict({
    resource_type: "mcp_server",
    resource_name: "automatic",
    existing_source: "built-in",
    existing_name: "automatic",
    blocked: true,
    reason: "'automatic' is built-in and cannot be replaced",
  }),
  conflict({
    resource_type: "mcp_server",
    resource_name: "github",
    existing_source: "source:acme/tools",
    existing_name: "github",
  }),
]);

describe("remote install conflicts", () => {
  it("keys a resource by its singular type", () => {
    expect(conflictKey("mcp_servers", "github")).toBe("mcp_server:github");
    expect(conflictKey("skills", "x")).toBe("skill:x");
  });

  it("starts with only the resources that replace nothing selected", () => {
    const selection = initialSelection(groups, conflicts);
    expect([...selection.rules]).toEqual(["fresh"]);
    expect([...selection.mcp_servers]).toEqual([]);
  });

  it("never lets a blocked resource be selected", () => {
    expect(isSelectable("mcp_servers", "automatic", conflicts)).toBe(false);
    expect(isSelectable("mcp_servers", "github", conflicts)).toBe(true);
    expect(isSelectable("rules", "fresh", conflicts)).toBe(true);
  });

  it("confirms only the selected conflicts that are not blocked", () => {
    const selection = {
      rules: new Set(["style", "fresh"]),
      mcp_servers: new Set(["automatic", "github"]),
    };
    expect(overridesFor(selection, conflicts)).toEqual(["mcp_server:github", "rule:style"]);
    expect(overridesFor({ rules: new Set(["fresh"]) }, conflicts)).toEqual([]);
  });

  it("drops conflicting resources from a selection", () => {
    const selection = withoutConflicts(
      { rules: new Set(["style", "fresh"]), mcp_servers: new Set(["github"]) },
      conflicts,
    );
    expect([...selection.rules]).toEqual(["fresh"]);
    expect([...selection.mcp_servers]).toEqual([]);
  });

  it("says what a replacement would replace", () => {
    expect(describeConflict(conflict({}))).toBe("Replaces your own version");
    expect(
      describeConflict(conflict({ existing_source: "source:acme/tools" })),
    ).toBe("Replaces the version installed from acme/tools");
    expect(
      describeConflict(conflict({ blocked: true, reason: "'automatic' is built-in and cannot be replaced" })),
    ).toBe("Cannot install: 'automatic' is built-in and cannot be replaced");
  });

  it("selects only conflict-free items from a group click", () => {
    const items = groups[1].items;
    const first = toggleGroupSelection("mcp_servers", items, new Set(), conflicts);
    expect([...first]).toEqual([]);
    const rules = toggleGroupSelection("rules", groups[0].items, new Set(), conflicts);
    expect([...rules]).toEqual(["fresh"]);
    expect(isGroupSelected("rules", groups[0].items, rules, conflicts)).toBe(true);
    // A second click clears the group, the confirmed conflict too.
    const cleared = toggleGroupSelection("rules", groups[0].items, new Set(["fresh", "style"]), conflicts);
    expect([...cleared]).toEqual([]);
    // A conflict the user already picked stays when the group fills in.
    const kept = toggleGroupSelection("rules", groups[0].items, new Set(["style"]), conflicts);
    expect([...kept].sort()).toEqual(["fresh", "style"]);
  });
});
