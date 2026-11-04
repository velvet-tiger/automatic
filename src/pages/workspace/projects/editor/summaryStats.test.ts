import { describe, expect, it } from "vitest";
import type { ActivityEntry, DriftReport, GitActivity, GitCommit } from "../types";
import {
  ACTIVITY_HISTORY_LIMIT,
  CALENDAR_WEEKS,
  activityLevel,
  agentSyncRows,
  buildActivityCalendar,
  commitWindowStats,
  countRecent,
  dailyCounts,
  gitHistoryStart,
  groupActivity,
  historyStart,
  latestSync,
  localDayKey,
  projectLevelDrift,
  summarizeSync,
  truncatedWindowStart,
  visibleActivityDetail,
} from "./summaryStats";

// Wednesday 4 November 2026, local time.
const NOW = new Date(2026, 10, 4, 12, 0, 0);

function entry(timestamp: Date | string, event = "project_updated", detail = ""): ActivityEntry {
  return {
    id: 1,
    project: "kelpie",
    event,
    label: event,
    detail,
    timestamp: typeof timestamp === "string" ? timestamp : timestamp.toISOString(),
  };
}

describe("buildActivityCalendar", () => {
  it("starts on a Sunday and ends today", () => {
    const { days } = buildActivityCalendar([], NOW);
    expect(days[0].date.getDay()).toBe(0);
    expect(days[days.length - 1].key).toBe(localDayKey(NOW));
    expect(days.length).toBe((CALENDAR_WEEKS - 1) * 7 + NOW.getDay() + 1);
  });

  it("counts events on the local day at both ends of the day", () => {
    const entries = [
      entry(new Date(2026, 10, 3, 0, 0, 1)),
      entry(new Date(2026, 10, 3, 23, 59, 59)),
      entry(new Date(2026, 10, 4, 0, 0, 1)),
    ];
    const { days } = buildActivityCalendar(entries, NOW);
    const byKey = new Map(days.map((day) => [day.key, day.count]));
    expect(byKey.get("2026-11-03")).toBe(2);
    expect(byKey.get("2026-11-04")).toBe(1);
  });

  it("ignores events with an unreadable timestamp", () => {
    const { days } = buildActivityCalendar([entry("not a date")], NOW);
    expect(days.every((day) => day.count === 0)).toBe(true);
  });

  it("treats every day as known when history is below the limit", () => {
    const { days, historySince } = buildActivityCalendar([entry(new Date(2026, 10, 1))], NOW);
    expect(historySince).toBeNull();
    expect(days.every((day) => day.known)).toBe(true);
  });

  it("marks days before truncated history as unknown", () => {
    const oldest = new Date(2026, 9, 20, 15, 0, 0);
    const entries = Array.from({ length: ACTIVITY_HISTORY_LIMIT }, () => entry(oldest));
    const { days, historySince } = buildActivityCalendar(entries, NOW);
    expect(historySince).toEqual(oldest);
    const byKey = new Map(days.map((day) => [day.key, day.known]));
    expect(byKey.get("2026-10-19")).toBe(false);
    expect(byKey.get("2026-10-20")).toBe(true);
  });

  it("does not report truncation when full history reaches past the calendar", () => {
    const entries = Array.from({ length: ACTIVITY_HISTORY_LIMIT }, () =>
      entry(new Date(2026, 0, 1)),
    );
    expect(buildActivityCalendar(entries, NOW).historySince).toBeNull();
  });
});

describe("dailyCounts", () => {
  it("returns oldest first and ends today", () => {
    const entries = [entry(new Date(2026, 10, 4, 1)), entry(new Date(2026, 10, 2, 1))];
    expect(dailyCounts(entries, 3, NOW)).toEqual([1, 0, 1]);
  });
});

describe("countRecent", () => {
  const entries = [
    entry(new Date(2026, 10, 3), "memory_stored"),
    entry(new Date(2026, 10, 2), "sync"),
    entry(new Date(2026, 8, 1), "memory_stored"),
  ];

  it("counts every event in the window", () => {
    expect(countRecent(entries, 30, NOW)).toBe(2);
  });

  it("filters to one event kind", () => {
    expect(countRecent(entries, 30, NOW, "memory_stored")).toBe(1);
  });

  it("covers the same days as the sparkline", () => {
    const edge = [
      entry(new Date(2026, 9, 6, 0, 0, 1)),
      entry(new Date(2026, 9, 5, 23, 59, 59)),
      entry(new Date(2026, 10, 4, 23, 59, 0)),
      entry(new Date(2026, 10, 5, 0, 0, 1)),
    ];
    const sparkline = dailyCounts(edge, 30, NOW).reduce((sum, count) => sum + count, 0);
    expect(countRecent(edge, 30, NOW)).toBe(sparkline);
    expect(sparkline).toBe(2);
  });
});

describe("activityLevel", () => {
  it.each([
    [0, 0],
    [1, 1],
    [2, 1],
    [3, 2],
    [6, 3],
    [11, 4],
  ])("maps %i events to level %i", (count, level) => {
    expect(activityLevel(count)).toBe(level);
  });
});

describe("groupActivity", () => {
  it("merges consecutive syncs only", () => {
    const entries = [
      entry(NOW, "sync"),
      entry(NOW, "sync"),
      entry(NOW, "skill_added"),
      entry(NOW, "skill_added"),
      entry(NOW, "sync"),
    ];
    expect(groupActivity(entries, 10).map((group) => group.count)).toEqual([2, 1, 1, 1]);
  });

  it("keeps syncs apart when they are far apart in time", () => {
    const entries = [
      entry(new Date(2026, 10, 4, 12, 0), "sync"),
      entry(new Date(2026, 10, 4, 11, 30), "sync"),
      entry(new Date(2026, 10, 3, 9, 0), "sync"),
    ];
    expect(groupActivity(entries, 10).map((group) => group.count)).toEqual([2, 1]);
  });

  it("measures the gap from the previous sync in a run", () => {
    const entries = [
      entry(new Date(2026, 10, 4, 12, 0), "sync"),
      entry(new Date(2026, 10, 4, 11, 20), "sync"),
      entry(new Date(2026, 10, 4, 10, 40), "sync"),
    ];
    expect(groupActivity(entries, 10).map((group) => group.count)).toEqual([3]);
  });

  it("keeps counting a sync run at the limit", () => {
    const entries = [entry(NOW, "skill_added"), entry(NOW, "sync"), entry(NOW, "sync")];
    const groups = groupActivity(entries, 2);
    expect(groups).toHaveLength(2);
    expect(groups[1].count).toBe(2);
  });
});

describe("visibleActivityDetail", () => {
  it("hides feature ids and repeated memory keys", () => {
    expect(visibleActivityDetail(entry(NOW, "feature_created", "e7dcf9d1"))).toBe("");
    expect(visibleActivityDetail(entry(NOW, "memory_stored", "key"))).toBe("");
  });

  it("keeps the size of a memory clear", () => {
    expect(visibleActivityDetail(entry(NOW, "memory_cleared", "3 entries"))).toBe("3 entries");
  });

  it("keeps other details", () => {
    expect(visibleActivityDetail(entry(NOW, "sync", "42 files"))).toBe("42 files");
  });
});

describe("latestSync", () => {
  it("finds the newest sync", () => {
    const entries = [entry(NOW, "skill_added"), entry(NOW, "sync", "42 files")];
    expect(latestSync(entries)?.detail).toBe("42 files");
  });

  it("returns undefined when there is none", () => {
    expect(latestSync([])).toBeUndefined();
  });
});

describe("history limit", () => {
  const full = (date: Date) => Array.from({ length: ACTIVITY_HISTORY_LIMIT }, () => entry(date));

  it("reports no start below the limit", () => {
    expect(historyStart([entry(new Date(2026, 10, 1))])).toBeNull();
  });

  it("reports the start when it cuts into the window", () => {
    const start = new Date(2026, 9, 25);
    expect(truncatedWindowStart(full(start), 30, NOW)).toEqual(start);
  });

  it("stays silent when history reaches past the window", () => {
    expect(truncatedWindowStart(full(new Date(2026, 8, 1)), 30, NOW)).toBeNull();
  });
});

describe("sync status", () => {
  const label = (id: string) => id.toUpperCase();
  const files = (count: number) =>
    Array.from({ length: count }, () => ({})) as DriftReport["agents"][number]["files"];
  const conflictOnly: DriftReport = {
    drifted: true,
    agents: [],
    gitignore: {} as DriftReport["gitignore"],
  };

  it("marks agents missing from the report as in sync", () => {
    const report: DriftReport = {
      drifted: true,
      agents: [{ agent_id: "cursor", agent_label: "Cursor", files: files(2) }],
    };
    const rows = agentSyncRows(["claude", "cursor"], label, report, false, true);
    expect(rows).toEqual([
      { id: "claude", label: "CLAUDE", status: "synced", driftedFiles: 0 },
      { id: "cursor", label: "CURSOR", status: "drifted", driftedFiles: 2 },
    ]);
    expect(summarizeSync(rows, report, false, true)).toEqual({
      status: "drifted",
      title: "Out of sync",
      share: 0.5,
    });
  });

  it("does not fill the ring for conflict-only drift", () => {
    const rows = agentSyncRows(["claude"], label, conflictOnly, false, true);
    expect(rows[0].status).toBe("synced");
    expect(projectLevelDrift(conflictOnly)).toEqual([".gitignore"]);
    expect(summarizeSync(rows, conflictOnly, false, true)).toEqual({
      status: "drifted",
      title: "Out of sync",
      share: 0.5,
    });
  });

  it("names every kind of project-level drift", () => {
    const report = {
      drifted: true,
      agents: [],
      instruction_conflicts: [{}],
      custom_conflicts: [{}],
    } as unknown as DriftReport;
    expect(projectLevelDrift(report)).toEqual(["instruction files", "custom assets"]);
  });

  it("ignores conflicts on a clean report", () => {
    expect(projectLevelDrift({ drifted: false, agents: [] })).toEqual([]);
  });

  it("shows checking for a null report", () => {
    const rows = agentSyncRows(["claude"], label, null, false, true);
    expect(rows[0].status).toBe("checking");
    expect(summarizeSync(rows, null, false, true)).toEqual({
      status: "checking",
      title: "Checking…",
      share: 0,
    });
  });

  it("shows a failed check", () => {
    const rows = agentSyncRows(["claude"], label, null, true, true);
    expect(rows[0].status).toBe("failed");
    expect(summarizeSync(rows, null, true, true).title).toBe("Not checked");
  });

  it("shows not set up over any report when setup is missing", () => {
    const report: DriftReport = { drifted: false, agents: [] };
    const rows = agentSyncRows(["claude"], label, report, false, false);
    expect(rows[0].status).toBe("not_set_up");
    expect(summarizeSync(rows, report, false, false)).toEqual({
      status: "not_set_up",
      title: "Not set up",
      share: 0,
    });
  });
});

function commit(timestamp: Date, author = "Sam", insertions = 1, deletions = 0): GitCommit {
  return {
    hash: `${timestamp.getTime()}${author}`,
    timestamp: timestamp.toISOString(),
    author,
    subject: "change",
    insertions,
    deletions,
    touches_agent_config: false,
  };
}

function gitActivity(commits: GitCommit[], truncated: boolean): GitActivity {
  return {
    repo: { branch: "main", uncommitted_files: 0, ahead: null, behind: null, has_commits: true },
    commits,
    truncated,
    history_days: 98,
  };
}

describe("commitWindowStats", () => {
  it("totals commits, authors and lines inside the window only", () => {
    const commits = [
      commit(new Date(2026, 10, 4, 9), "Sam", 10, 2),
      commit(new Date(2026, 10, 1, 9), "Alex", 5, 5),
      commit(new Date(2026, 10, 1, 10), "Sam", 1, 0),
      commit(new Date(2026, 8, 1), "Old", 100, 100),
    ];
    expect(commitWindowStats(commits, 30, NOW)).toEqual({
      commits: 3,
      authors: 2,
      insertions: 16,
      deletions: 7,
    });
  });

  it("matches the sparkline at the window edge", () => {
    const edge = [commit(new Date(2026, 9, 6, 0, 0, 1)), commit(new Date(2026, 9, 5, 23, 59))];
    const sparkline = dailyCounts(edge, 30, NOW).reduce((sum, count) => sum + count, 0);
    expect(commitWindowStats(edge, 30, NOW).commits).toBe(sparkline);
  });
});

describe("gitHistoryStart", () => {
  it("is the oldest commit only when the cap cut history short", () => {
    const commits = [commit(new Date(2026, 10, 3)), commit(new Date(2026, 9, 20))];
    expect(gitHistoryStart(gitActivity(commits, true))).toEqual(new Date(2026, 9, 20));
    expect(gitHistoryStart(gitActivity(commits, false))).toBeNull();
  });

  it("marks calendar days before a truncated history as unknown", () => {
    const commits = [commit(new Date(2026, 10, 3)), commit(new Date(2026, 9, 20))];
    const { days, historySince } = buildActivityCalendar(commits, NOW, new Date(2026, 9, 20));
    expect(historySince).toEqual(new Date(2026, 9, 20));
    expect(days.find((d) => d.key === "2026-10-19")?.known).toBe(false);
    expect(days.find((d) => d.key === "2026-10-20")?.known).toBe(true);
    expect(truncatedWindowStart(commits, 30, NOW, new Date(2026, 9, 20))).toEqual(new Date(2026, 9, 20));
  });

  it("ignores the activity log limit for commits", () => {
    const many = Array.from({ length: 600 }, () => commit(new Date(2026, 10, 3)));
    expect(buildActivityCalendar(many, NOW, null).historySince).toBeNull();
  });
});
