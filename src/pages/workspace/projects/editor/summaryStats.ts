// Pure calculations behind the project Summary panel.

import type { ActivityEntry, DriftReport } from "../types";

/** Mirrors `MAX_ENTRIES_PER_PROJECT` in `src-tauri/src/activity.rs`. */
export const ACTIVITY_HISTORY_LIMIT = 500;
export const CALENDAR_WEEKS = 13;
export const STAT_WINDOW_DAYS = 30;
/** Syncs further apart than this are separate work, so they keep their own rows. */
const SYNC_GROUP_GAP_MS = 60 * 60 * 1000;

export interface CalendarDay {
  key: string;
  date: Date;
  count: number;
  /** False for days older than the stored history, whose count is unknown. */
  known: boolean;
}

export interface ActivityCalendar {
  days: CalendarDay[];
  /** Oldest stored event, set only when history stops inside the calendar. */
  historySince: Date | null;
}

export interface ActivityGroup {
  entry: ActivityEntry;
  count: number;
}

export type AgentSyncStatus = "synced" | "drifted" | "checking" | "failed" | "not_set_up";

export interface AgentSyncRow {
  id: string;
  label: string;
  status: AgentSyncStatus;
  driftedFiles: number;
}

export interface SyncSummary {
  status: AgentSyncStatus;
  title: string;
  /** Share of the ring to fill, from 0 to 1. */
  share: number;
}

const SYNC_TITLES: Record<AgentSyncStatus, string> = {
  synced: "In sync",
  drifted: "Out of sync",
  checking: "Checking…",
  failed: "Not checked",
  not_set_up: "Not set up",
};

/** Events are stored in UTC, but a day on the calendar is the user's local day. */
export function localDayKey(date: Date): string {
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${date.getFullYear()}-${month}-${day}`;
}

function startOfLocalDay(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

function addLocalDays(date: Date, days: number): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + days);
}

function countByLocalDay(entries: ActivityEntry[]): Map<string, number> {
  const counts = new Map<string, number>();
  for (const entry of entries) {
    const date = new Date(entry.timestamp);
    if (Number.isNaN(date.getTime())) continue;
    const key = localDayKey(date);
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  return counts;
}

/** The oldest parseable event. Entries arrive newest first, but this does not rely on it. */
function oldestEventDate(entries: ActivityEntry[]): Date | null {
  let oldest: Date | null = null;
  for (const entry of entries) {
    const date = new Date(entry.timestamp);
    if (Number.isNaN(date.getTime())) continue;
    if (oldest === null || date < oldest) oldest = date;
  }
  return oldest;
}

/** Oldest stored event, or null when the limit has not discarded any history. */
export function historyStart(entries: ActivityEntry[]): Date | null {
  return entries.length >= ACTIVITY_HISTORY_LIMIT ? oldestEventDate(entries) : null;
}

/** Where history starts, when that falls inside the last `days` days. */
export function truncatedWindowStart(entries: ActivityEntry[], days: number, now: Date): Date | null {
  const start = historyStart(entries);
  return start !== null && start > windowStart(days, now) ? start : null;
}

function windowStart(days: number, now: Date): Date {
  return addLocalDays(startOfLocalDay(now), 1 - days);
}

/**
 * Days for the activity calendar, oldest first. The first day is a Sunday, so
 * the seven-row grid lines up by weekday.
 */
export function buildActivityCalendar(entries: ActivityEntry[], now: Date): ActivityCalendar {
  const today = startOfLocalDay(now);
  const start = addLocalDays(today, -today.getDay() - (CALENDAR_WEEKS - 1) * 7);
  const counts = countByLocalDay(entries);

  const oldest = historyStart(entries);
  const historySince = oldest !== null && oldest > start ? oldest : null;
  const firstKnownDay = historySince === null ? start : startOfLocalDay(historySince);

  const days: CalendarDay[] = [];
  for (let date = start; date <= today; date = addLocalDays(date, 1)) {
    const key = localDayKey(date);
    days.push({ key, date, count: counts.get(key) ?? 0, known: date >= firstKnownDay });
  }
  return { days, historySince };
}

/** Daily event counts for the last `days` days, oldest first, ending today. */
export function dailyCounts(entries: ActivityEntry[], days: number, now: Date): number[] {
  const today = startOfLocalDay(now);
  const counts = countByLocalDay(entries);
  return Array.from({ length: days }, (_, index) => {
    const date = addLocalDays(today, index - days + 1);
    return counts.get(localDayKey(date)) ?? 0;
  });
}

/**
 * Events in the last `days` local days, optionally limited to one event kind.
 * Covers the same days as `dailyCounts`, so a tile matches its sparkline.
 */
export function countRecent(
  entries: ActivityEntry[],
  days: number,
  now: Date,
  event?: string,
): number {
  const start = windowStart(days, now).getTime();
  const end = addLocalDays(startOfLocalDay(now), 1).getTime();
  return entries.filter((entry) => {
    if (event !== undefined && entry.event !== event) return false;
    const time = new Date(entry.timestamp).getTime();
    return time >= start && time < end;
  }).length;
}

/** Calendar shade from 0 (no activity) to 4 (busiest). */
export function activityLevel(count: number): 0 | 1 | 2 | 3 | 4 {
  if (count <= 0) return 0;
  if (count <= 2) return 1;
  if (count <= 5) return 2;
  if (count <= 10) return 3;
  return 4;
}

function previousSyncIsClose(newer: ActivityEntry | undefined, older: ActivityEntry): boolean {
  if (newer === undefined || newer.event !== "sync" || older.event !== "sync") return false;
  const gap = new Date(newer.timestamp).getTime() - new Date(older.timestamp).getTime();
  return Math.abs(gap) <= SYNC_GROUP_GAP_MS;
}

/**
 * The newest `limit` rows for the activity list. Consecutive syncs close
 * together collapse into one row, because a save often logs two identical
 * entries.
 */
export function groupActivity(entries: ActivityEntry[], limit: number): ActivityGroup[] {
  const groups: ActivityGroup[] = [];
  let lastInRun: ActivityEntry | undefined;
  for (const entry of entries) {
    const previous = groups[groups.length - 1];
    if (previous !== undefined && previousSyncIsClose(lastInRun, entry)) {
      previous.count += 1;
    } else {
      if (groups.length === limit) break;
      groups.push({ entry, count: 1 });
    }
    lastInRun = entry;
  }
  return groups;
}

/** Feature details hold an internal id. Memory key details repeat the label. */
export function visibleActivityDetail(entry: ActivityEntry): string {
  if (entry.event.startsWith("feature_")) return "";
  if (entry.event === "memory_stored" || entry.event === "memory_deleted") return "";
  return entry.detail;
}

export function latestSync(entries: ActivityEntry[]): ActivityEntry | undefined {
  return entries.find((entry) => entry.event === "sync");
}

/** Names the drift that belongs to the project, not to any one agent. */
export function projectLevelDrift(report: DriftReport | null): string[] {
  if (report === null || !report.drifted) return [];
  const kinds: string[] = [];
  if ((report.instruction_conflicts?.length ?? 0) > 0) kinds.push("instruction files");
  if ((report.custom_conflicts?.length ?? 0) > 0) kinds.push("custom assets");
  if (report.gitignore) kinds.push(".gitignore");
  return kinds;
}

function overallStatus(
  report: DriftReport | null,
  checkFailed: boolean,
  setUp: boolean,
): AgentSyncStatus {
  if (!setUp) return "not_set_up";
  if (report === null) return checkFailed ? "failed" : "checking";
  return report.drifted ? "drifted" : "synced";
}

/** One row per project agent. A drift report lists only the agents that drifted. */
export function agentSyncRows(
  agentIds: string[],
  labelFor: (id: string) => string,
  report: DriftReport | null,
  checkFailed: boolean,
  setUp: boolean,
): AgentSyncRow[] {
  const overall = overallStatus(report, checkFailed, setUp);
  return agentIds.map((id) => {
    const drift = report?.drifted ? report.agents.find((agent) => agent.agent_id === id) : undefined;
    const status = overall === "drifted" && drift === undefined ? "synced" : overall;
    return { id, label: labelFor(id), status, driftedFiles: drift?.files.length ?? 0 };
  });
}

/**
 * The project's overall state. Project-level drift counts as one more part
 * of the ring, so it never reads as complete while the title says out of sync.
 */
export function summarizeSync(
  rows: AgentSyncRow[],
  report: DriftReport | null,
  checkFailed: boolean,
  setUp: boolean,
): SyncSummary {
  const status = overallStatus(report, checkFailed, setUp);
  const title = SYNC_TITLES[status];
  if (status === "synced") return { status, title, share: 1 };
  if (status !== "drifted") return { status, title, share: 0 };
  const synced = rows.filter((row) => row.status === "synced").length;
  const parts = rows.length + (projectLevelDrift(report).length > 0 ? 1 : 0);
  return { status, title, share: parts === 0 ? 0 : synced / parts };
}

export function syncStatusLabel(status: AgentSyncStatus): string {
  return SYNC_TITLES[status];
}
