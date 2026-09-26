import { describe, it, expect } from "vitest";
import { resolveGroupDrop, UNGROUPED_DROP_TARGET } from "./groupDrop";

describe("resolveGroupDrop", () => {
  it("does nothing without a target or on the source group", () => {
    expect(resolveGroupDrop("a", null, false)).toEqual({ kind: "none" });
    expect(resolveGroupDrop("a", "a", true)).toEqual({ kind: "none" });
  });

  it("moves between groups on a plain drag", () => {
    expect(resolveGroupDrop("a", "b", false)).toEqual({ kind: "move", from: "a", to: "b" });
  });

  it("adds without leaving the source group when the modifier is held", () => {
    expect(resolveGroupDrop("a", "b", true)).toEqual({ kind: "add", to: "b" });
  });

  it("adds an ungrouped project to the target group", () => {
    expect(resolveGroupDrop(null, "b", false)).toEqual({ kind: "add", to: "b" });
  });

  it("removes from every group when dropped on Other Projects", () => {
    expect(resolveGroupDrop("a", UNGROUPED_DROP_TARGET, false)).toEqual({ kind: "remove-all" });
    expect(resolveGroupDrop("a", UNGROUPED_DROP_TARGET, true)).toEqual({ kind: "remove-all" });
  });

  it("ignores an ungrouped project dropped back on Other Projects", () => {
    expect(resolveGroupDrop(null, UNGROUPED_DROP_TARGET, false)).toEqual({ kind: "none" });
  });
});
