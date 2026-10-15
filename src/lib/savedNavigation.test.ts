import { describe, expect, it } from "vitest";
import { FALLBACK_SECTION, FALLBACK_TAB, migrateSavedSection, migrateSavedTab } from "./savedNavigation";

const KNOWN_TABS = new Set(["projects", "skills", "skill-store", "settings", "sync"]);
const SECTIONS = ["workspace", "library", "discover", "tools"] as const;

describe("migrateSavedTab", () => {
  it("keeps a tab the app still renders", () => {
    expect(migrateSavedTab("skills", KNOWN_TABS)).toBe("skills");
    expect(migrateSavedTab("settings", KNOWN_TABS)).toBe("settings");
  });

  it("sends the removed Getting Started tab to Projects", () => {
    expect(migrateSavedTab("getting-started", KNOWN_TABS)).toBe("projects");
  });

  it("sends older removed tabs to their replacements", () => {
    expect(migrateSavedTab("dashboard", KNOWN_TABS)).toBe("projects");
    expect(migrateSavedTab("activity", KNOWN_TABS)).toBe("projects");
    expect(migrateSavedTab("configuration", KNOWN_TABS)).toBe("projects");
    expect(migrateSavedTab("support", KNOWN_TABS)).toBe("settings");
  });

  it("falls back to Projects for a missing or unknown tab", () => {
    expect(migrateSavedTab(null, KNOWN_TABS)).toBe(FALLBACK_TAB);
    expect(migrateSavedTab("", KNOWN_TABS)).toBe(FALLBACK_TAB);
    expect(migrateSavedTab("tab-from-a-newer-build", KNOWN_TABS)).toBe(FALLBACK_TAB);
    expect(migrateSavedTab("constructor", KNOWN_TABS)).toBe(FALLBACK_TAB);
  });
});

describe("migrateSavedSection", () => {
  it("keeps a section that still exists", () => {
    expect(migrateSavedSection("library", SECTIONS)).toBe("library");
  });

  it("sends the removed Start section to Workspace", () => {
    expect(migrateSavedSection("start", SECTIONS)).toBe("workspace");
  });

  it("sends the old Marketplace and Community sections to Discover", () => {
    expect(migrateSavedSection("marketplace", SECTIONS)).toBe("discover");
    expect(migrateSavedSection("community", SECTIONS)).toBe("discover");
  });

  it("falls back to Workspace for a missing or unknown section", () => {
    expect(migrateSavedSection(null, SECTIONS)).toBe(FALLBACK_SECTION);
    expect(migrateSavedSection("nowhere", SECTIONS)).toBe(FALLBACK_SECTION);
    expect(migrateSavedSection("constructor", SECTIONS)).toBe(FALLBACK_SECTION);
  });

  it("throws when the fallback section is not known", () => {
    expect(() => migrateSavedSection("start", ["library"] as const)).toThrow(/workspace/);
  });
});
