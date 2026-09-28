import { describe, it, expect } from "vitest";
import type { ProjectSummary } from "../pages/workspace/projects/types";
import { projectKeyOf, projectNameForKey, resolveProjectKey, rowProjectKey } from "./projectIdentity";

const summary = (name: string, local_key: string): ProjectSummary => ({
  name,
  local_key,
  id: `id-${name}`,
  directory: `/work/${name}`,
});

const list = [summary("App", "key-app"), summary("web", "key-web"), summary("pending", "")];

describe("projectIdentity", () => {
  it("uses the local_key, or the name while none is minted", () => {
    expect(projectKeyOf(list[0]!)).toBe("key-app");
    expect(projectKeyOf(list[2]!)).toBe("pending");
  });

  it("resolves keys first, then names, then names ignoring case", () => {
    expect(resolveProjectKey(list, "key-web")).toBe("key-web");
    expect(resolveProjectKey(list, "App")).toBe("key-app");
    expect(resolveProjectKey(list, "app")).toBe("key-app");
    expect(resolveProjectKey(list, "pending")).toBe("pending");
    expect(resolveProjectKey(list, "gone")).toBeNull();
  });

  it("keys a backend row by local_key, falling back to the name", () => {
    expect(rowProjectKey({ project: "App", local_key: "key-app" })).toBe("key-app");
    expect(rowProjectKey({ project: "orphan" })).toBe("orphan");
    expect(rowProjectKey({ project: "orphan", local_key: "" })).toBe("orphan");
  });

  it("finds the display name for a key", () => {
    expect(projectNameForKey(list, "key-web")).toBe("web");
    expect(projectNameForKey(list, "nope")).toBeNull();
  });
});
