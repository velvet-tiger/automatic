import { describe, expect, it } from "vitest";
import type { ActivityEntry, DriftReport } from "../types";
import {
  ACTIVITY_HISTORY_LIMIT,
  CALENDAR_WEEKS,
  activityLevel,
  agentSyncRows,
  buildActivityCalendar,
  countRecent,
  dailyCounts,
  groupActivity,
  latestSync,
  localDayKey,
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

describe("agentSyncRows", () => {
  const label = (id: string) => id.toUpperCase();

  it("marks agents missing from the report as in sync", () => {
    const report: DriftReport = {
      drifted: true,
      agents: [
        {
          agent_id: "cursor",
          agent_label: "Cursor",
          files: [{}, {}] as DriftReport["agents"][number]["files"],
        },
      ],
    };
    expect(agentSyncRows(["claude", "cursor"], label, report, false)).toEqual([
      { id: "claude", label: "CLAUDE", status: "synced", driftedFiles: 0 },
      { id: "cursor", label: "CURSOR", status: "drifted", driftedFiles: 2 },
    ]);
  });

  it("shows checking before the first report", () => {
    expect(agentSyncRows(["claude"], label, null, false)[0].status).toBe("checking");
  });

  it("shows a failed check", () => {
    expect(agentSyncRows(["claude"], label, null, true)[0].status).toBe("failed");
  });
});
