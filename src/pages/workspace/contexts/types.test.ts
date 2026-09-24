import { describe, it, expect } from "vitest";
import { deriveSlug, isValidSlug, providingGroup, sourceProblem, uniqueSourceId, type ContextSource } from "./types";
import { uniqueContextSlug } from "./NewContextDialog";

const doc = (id: string): ContextSource => ({ id, kind: "documentation", display_name: "", description: "" });

describe("context helpers", () => {
  it("uses the backend slug grammar", () => {
    for (const ok of ["a", "payments", "a.b_c-d", "0day"]) expect(isValidSlug(ok)).toBe(true);
    for (const bad of ["", ".", "-a", "A", "a/b", "a b", "a".repeat(129)]) expect(isValidSlug(bad)).toBe(false);
  });

  it("derives slugs and keeps them unique", () => {
    expect(deriveSlug("Payments Domain!")).toBe("payments-domain");
    expect(deriveSlug("!!!")).toBe("");
    expect(uniqueContextSlug("Payments", ["payments", "payments-2"])).toBe("payments-3");
    expect(uniqueContextSlug("!!!", [])).toBe("context");
    expect(uniqueSourceId("Public API", [doc("public-api")])).toBe("public-api-2");
  });

  it("checks what the user entered for linked material", () => {
    const local: ContextSource = { id: "x", kind: "local", display_name: "", description: "", config: { path: "docs" } };
    expect(sourceProblem(local, [])).toMatch(/absolute/);
    const url: ContextSource = { id: "x", kind: "url", display_name: "", description: "", config: { url: "https://u:p@example.com/x", ttl_secs: 0 } };
    expect(sourceProblem(url, [])).toMatch(/user name or password/);
    const cloud: ContextSource = { id: "x", kind: "cloud", display_name: "", description: "", config: { source_id: "ctx_1" } };
    expect(sourceProblem(cloud, [])).toMatch(/src_/);
  });

  it("finds the providing group", () => {
    expect(providingGroup({ b: ["x"], a: ["x", "y"] }, "x")).toBe("a");
    expect(providingGroup(undefined, "x")).toBeNull();
  });
});
