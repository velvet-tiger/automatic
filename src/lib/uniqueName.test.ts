import { describe, it, expect } from "vitest";
import { nextAvailableName } from "./uniqueName";

describe("nextAvailableName", () => {
  it("returns the base when nothing collides", () => {
    expect(nextAvailableName("linear-copy", ["linear", "sentry"])).toBe("linear-copy");
  });

  it("appends -2 when the base is taken", () => {
    expect(nextAvailableName("linear-copy", ["linear", "linear-copy"])).toBe("linear-copy-2");
  });

  it("skips every taken suffix in order", () => {
    const existing = ["linear-copy", "linear-copy-2", "linear-copy-3"];
    expect(nextAvailableName("linear-copy", existing)).toBe("linear-copy-4");
  });

  it("treats case variants as collisions", () => {
    expect(nextAvailableName("linear-copy", ["Linear-Copy"])).toBe("linear-copy-2");
    expect(nextAvailableName("linear", ["LINEAR"], { startAt: 2 })).toBe("linear-2");
  });

  it("never returns the bare base when startAt is given", () => {
    expect(nextAvailableName("linear", [], { startAt: 2 })).toBe("linear-2");
    expect(nextAvailableName("linear", ["linear"], { startAt: 2 })).toBe("linear-2");
    expect(nextAvailableName("linear", ["linear", "linear-2"], { startAt: 2 })).toBe("linear-3");
  });

  it("accepts any iterable of existing names", () => {
    expect(nextAvailableName("a", new Set(["a", "a-2"]))).toBe("a-3");
  });
});
