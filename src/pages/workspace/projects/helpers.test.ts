import { describe, it, expect } from "vitest";
import {
  checkFailureNotices,
  driftChecked,
  driftCheckFailed,
  isCheckFailed,
  isDrifted,
  isInSync,
} from "./helpers";

describe("drift check state", () => {
  it("never reads a failed check as in sync or drifted", () => {
    const failed = driftCheckFailed("MCP server 'github' could not be read: locked");
    expect(failed).toEqual({ status: "failed", error: "MCP server 'github' could not be read: locked" });
    expect(isCheckFailed(failed)).toBe(true);
    expect(isInSync(failed)).toBe(false);
    expect(isDrifted(failed)).toBe(false);
  });

  it("keeps the message of an Error and of other rejection values", () => {
    expect(driftCheckFailed(new Error("boom"))).toEqual({ status: "failed", error: "boom" });
    expect(driftCheckFailed(42)).toEqual({ status: "failed", error: "42" });
  });

  it("tells a checked project apart from one not checked yet", () => {
    expect(isInSync(driftChecked(false))).toBe(true);
    expect(isDrifted(driftChecked(true))).toBe(true);
    expect(isInSync(undefined)).toBe(false);
    expect(isDrifted(undefined)).toBe(false);
    expect(isCheckFailed(undefined)).toBe(false);
  });
});

describe("checkFailureNotices", () => {
  it("shows nothing when both checks ran", () => {
    expect(checkFailureNotices(null, null)).toEqual([]);
  });

  it("shows one line when both checks failed for the same reason", () => {
    expect(checkFailureNotices("same", "same")).toEqual([
      { title: "Could not check this project for changes or problems.", error: "same" },
    ]);
  });

  it("shows each failure when the reasons differ", () => {
    expect(checkFailureNotices("a", "b")).toEqual([
      { title: "Could not check this project for changes.", error: "a" },
      { title: "Could not check this project for problems.", error: "b" },
    ]);
    expect(checkFailureNotices(null, "b")).toEqual([
      { title: "Could not check this project for problems.", error: "b" },
    ]);
  });
});
