import { describe, it, expect } from "vitest";
import type { ProjectSummary } from "../pages/workspace/projects/types";
import {
  membersWithoutProject,
  projectKeyOf,
  projectLabelFor,
  projectLabels,
  projectLabelText,
  projectNameForKey,
  projectsNamed,
  resolveProjectKey,
  rowProjectKey,
  sharedNameNote,
  shortFolder,
} from "./projectIdentity";

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

describe("projectLabels", () => {
  const at = (name: string, local_key: string, directory: string): ProjectSummary => ({
    name,
    local_key,
    id: `id-${local_key}`,
    directory,
  });

  it("shows a unique name exactly as it is", () => {
    const labels = projectLabels([at("api", "k1", "/w/api"), at("web", "k2", "/w/web")]);
    expect(projectLabelFor(labels, "k1")).toEqual({ name: "api", hint: null });
    expect(projectLabelText(projectLabelFor(labels, "k2"))).toBe("web");
  });

  it("adds the shortest folder part that tells two same-named projects apart", () => {
    const labels = projectLabels([
      at("website", "k1", "/Users/me/consultmed/website"),
      at("website", "k2", "/Users/me/_active/website"),
      at("api", "k3", "/Users/me/api"),
    ]);
    expect(projectLabelText(projectLabelFor(labels, "k1"))).toBe("website · consultmed/website");
    expect(projectLabelText(projectLabelFor(labels, "k2"))).toBe("website · _active/website");
    expect(projectLabelFor(labels, "k3").hint).toBeNull();
  });

  it("uses one folder level when the folders already differ", () => {
    const labels = projectLabels([at("website", "k1", "/a/site-a"), at("website", "k2", "/a/site-b")]);
    expect(projectLabelFor(labels, "k1").hint).toBe("site-a");
    expect(projectLabelFor(labels, "k2").hint).toBe("site-b");
  });

  it("picks a depth per project when three share a name", () => {
    const labels = projectLabels([
      at("website", "k1", "/w/consultmed/website"),
      at("website", "k2", "/w/_active/website"),
      at("website", "k3", "/w/old/_active/website"),
    ]);
    expect(projectLabelFor(labels, "k1").hint).toBe("consultmed/website");
    expect(projectLabelFor(labels, "k2").hint).toBe("w/_active/website");
    expect(projectLabelFor(labels, "k3").hint).toBe("old/_active/website");
  });

  it("shows the full folder when one folder is a suffix of another", () => {
    const labels = projectLabels([at("website", "k1", "/a/website"), at("website", "k2", "/x/a/website")]);
    expect(projectLabelFor(labels, "k1").hint).toBe("/a/website");
    expect(projectLabelFor(labels, "k2").hint).toBe("x/a/website");
  });

  it("treats names that differ only by case as the same name", () => {
    const labels = projectLabels([at("Website", "k1", "/one/website"), at("website", "k2", "/two/website")]);
    expect(projectLabelText(projectLabelFor(labels, "k1"))).toBe("Website · one/website");
    expect(projectLabelText(projectLabelFor(labels, "k2"))).toBe("website · two/website");
  });

  it("labels a project with no folder and falls back for unknown keys", () => {
    const labels = projectLabels([at("site", "k1", ""), at("site", "k2", "/w/site")]);
    expect(projectLabelFor(labels, "k1").hint).toBe("no folder");
    expect(projectLabelFor(labels, "k2").hint).toBe("site");
    expect(projectLabelFor(labels, "gone", "orphan")).toEqual({ name: "orphan", hint: null });
    expect(projectLabelFor(labels, "gone")).toEqual({ name: "gone", hint: null });
  });

  it("does not resolve a name two projects share", () => {
    const shared = [at("site", "k1", "/a/site"), at("SITE", "k2", "/b/site")];
    expect(resolveProjectKey(shared, "site")).toBeNull();
    expect(resolveProjectKey(shared, "k2")).toBe("k2");
  });
});

describe("shared names in the Add Project wizard", () => {
  const at = (name: string, local_key: string, directory: string): ProjectSummary => ({
    name,
    local_key,
    id: `id-${local_key}`,
    directory,
  });
  const all = [
    at("website", "k1", "/Users/me/consultmed/website"),
    at("Website", "k2", "/Users/me/_active/website"),
    at("api", "k3", "/Users/me/api"),
  ];

  it("finds other projects with the name, ignoring case and the wizard's own stub", () => {
    expect(projectsNamed(all, " WEBSITE ").map((s) => s.local_key)).toEqual(["k1", "k2"]);
    expect(projectsNamed(all, "website", "k2").map((s) => s.local_key)).toEqual(["k1"]);
    expect(projectsNamed(all, "")).toEqual([]);
    expect(projectsNamed(all, "docs")).toEqual([]);
  });

  it("shortens a folder to its last two parts", () => {
    expect(shortFolder("/Users/me/consultmed/website")).toBe("…/consultmed/website");
    expect(shortFolder("/website")).toBe("/website");
  });

  it("says one other project shares the name, with its folder", () => {
    expect(sharedNameNote("website", projectsNamed(all, "website", "k2"))).toBe(
      "Another project is also called \u201cwebsite\u201d (…/consultmed/website). You can keep this name.",
    );
    expect(sharedNameNote("site", [at("site", "k9", "")])).toBe(
      "Another project is also called \u201csite\u201d. You can keep this name.",
    );
  });

  it("counts several and says nothing when the name is free", () => {
    expect(sharedNameNote("website", projectsNamed(all, "website"))).toBe(
      "2 other projects are also called \u201cwebsite\u201d. You can keep this name.",
    );
    expect(sharedNameNote("docs", [])).toBeNull();
  });
});

describe("membersWithoutProject", () => {
  const summaries: ProjectSummary[] = [
    { name: "site", local_key: "k1", id: "id-a", directory: "/a/site" },
    { name: "site", local_key: "k2", id: "id-a", directory: "/b/site" },
    { name: "site", local_key: "k3", id: "id-c", directory: "/c/site" },
  ];

  it("drops every checkout of the project and keeps a same-named other project", () => {
    expect(membersWithoutProject(["k1", "k2", "k3", "ghost"], "k1", summaries)).toEqual(["k3", "ghost"]);
  });

  it("drops only the key itself for an unknown project", () => {
    expect(membersWithoutProject(["ghost", "k3"], "ghost", summaries)).toEqual(["k3"]);
  });
});
