import { describe, it, expect } from "vitest";
import { extraFolderState, folderName, folderParent, groupFolders, savedStatus } from "./writableFolders";
import type { WritableFoldersFindings } from "./types";

function findings(overrides: Partial<WritableFoldersFindings> = {}): WritableFoldersFindings {
  return {
    wanted: [],
    uncovered: [],
    missing: [],
    target: { kind: "local" },
    blocked_by: null,
    ask_for_user_settings: false,
    paste_json: "{}",
    unreadable: [],
    groups_switched_off: [],
    last_sync_errors: [],
    refused: [],
    left_in_tracked_local: [],
    ...overrides,
  };
}

describe("folderName and folderParent", () => {
  it("names a folder by its last part", () => {
    expect(folderName("/Users/me/work/cli")).toBe("cli");
    expect(folderName("/Users/me/work/cli/")).toBe("cli");
    expect(folderName("C:\\work\\cli")).toBe("cli");
  });

  it("shows the parent with home as ~", () => {
    expect(folderParent("/Users/me/work/cli", "/Users/me")).toBe("~/work");
    expect(folderParent("/Users/me/cli", "/Users/me/")).toBe("~");
    expect(folderParent("/Volumes/Shared/design", "/Users/me")).toBe("/Volumes/Shared");
    expect(folderParent("/Users/meg/cli", "/Users/me")).toBe("/Users/meg");
    expect(folderParent("/Users/me/work/cli", null)).toBe("/Users/me/work");
  });
});

describe("extraFolderState", () => {
  const f = findings({
    missing: [
      { path: "/gone", source: { kind: "extra" } },
      { path: "/member", source: { kind: "group_member", group: "g", project: "p" } },
    ],
    refused: [{ path: "/Users/me/.ssh", source: { kind: "extra" }, reason: "it holds credentials" }],
  });

  it("matches only the project's own extra folders", () => {
    expect(extraFolderState("/gone", f)).toEqual({ kind: "missing" });
    expect(extraFolderState("/member", f)).toEqual({ kind: "ok" });
    expect(extraFolderState("/Users/me/.ssh", f)).toEqual({ kind: "refused", reason: "it holds credentials" });
    expect(extraFolderState("/fine", f)).toEqual({ kind: "ok" });
    expect(extraFolderState("/gone", null)).toEqual({ kind: "ok" });
  });
});

describe("groupFolders", () => {
  it("lists members per group, then the groups that are switched off", () => {
    const lines = groupFolders(
      findings({
        wanted: [
          { path: "/b", source: { kind: "group_member", group: "Team", project: "web" } },
          { path: "/a", source: { kind: "group_member", group: "Team", project: "cli" } },
          { path: "/x", source: { kind: "extra" } },
        ],
        missing: [{ path: "/m", source: { kind: "group_member", group: "Apps", project: "ios" } }],
        groups_switched_off: ["Clients"],
      }),
    );
    expect(lines).toEqual([
      { group: "Apps", projects: ["ios"], switchedOn: true },
      { group: "Team", projects: ["cli", "web"], switchedOn: true },
      { group: "Clients", projects: [], switchedOn: false },
    ]);
  });
});

describe("savedStatus", () => {
  const wanted = [{ path: "/a", source: { kind: "extra" } as const }];

  it("says nothing when there are no folders", () => {
    expect(savedStatus(findings())).toBeNull();
  });

  it("names where the folders went", () => {
    expect(savedStatus(findings({ wanted }))).toEqual({ text: "Saved for this project only", ok: true });
    expect(savedStatus(findings({ wanted, target: { kind: "user", reason: "silent" } }))?.text).toBe(
      "Saved for all your Claude Code sessions",
    );
    expect(savedStatus(findings({ wanted, target: { kind: "needs_consent", reason: "tracked" } }))?.ok).toBe(false);
  });

  it("reports a failed sync first", () => {
    expect(savedStatus(findings({ wanted, last_sync_errors: ["disk full"] }))?.text).toMatch(/last sync failed/);
  });
});
