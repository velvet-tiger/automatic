// Pure calculations behind the project Summary panel.

import type { ActivityEntry, DriftReport } from "../types";

/** Most events the backend keeps per project (`MAX_ENTRIES_PER_PROJECT`). */
export const ACTIVITY_HISTORY_LIMIT = 500;
export const CALENDAR_WEEKS = 13;
export const STAT_WINDOW_DAYS = 30;
const DAY_MS = 24 * 60 * 60 * 1000;

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

export type AgentSyncStatus = "synced" | "drifted" | "checking" | "failed";

export interface AgentSyncRow {
  id: string;
  label: string;
  status: AgentSyncStatus;
  driftedFiles: number;
}

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

/**
 * Days for the activity calendar, oldest first. The first day is a Sunday, so
 * the seven-row grid lines up by weekday.
 */
export function buildActivityCalendar(entries: ActivityEntry[], now: Date): ActivityCalendar {
  const today = startOfLocalDay(now);
  const start = addLocalDays(today, -today.getDay() - (CALENDAR_WEEKS - 1) * 7);
  const counts = countByLocalDay(entries);

  const oldest = entries.length >= ACTIVITY_HISTORY_LIMIT ? oldestEventDate(entries) : null;
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

/** Events in the last `days` days, optionally limited to one event kind. */
export function countRecent(
  entries: ActivityEntry[],
  days: number,
  now: Date,
  event?: string,
): number {
  const cutoff = now.getTime() - days * DAY_MS;
  return entries.filter((entry) => {
    if (event !== undefined && entry.event !== event) return false;
    return new Date(entry.timestamp).getTime() >= cutoff;
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

/**
 * The newest `limit` rows for the activity list. Runs of consecutive syncs
 * collapse into one row, because a save often logs two identical entries.
 */
export function groupActivity(entries: ActivityEntry[], limit: number): ActivityGroup[] {
  const groups: ActivityGroup[] = [];
  for (const entry of entries) {
    const previous = groups[groups.length - 1];
    if (previous !== undefined && entry.event === "sync" && previous.entry.event === "sync") {
      previous.count += 1;
      continue;
    }
    if (groups.length === limit) break;
    groups.push({ entry, count: 1 });
  }
  return groups;
}

/** Feature details hold an internal id and memory details repeat the label. */
export function visibleActivityDetail(entry: ActivityEntry): string {
  if (entry.event.startsWith("feature_") || entry.event.startsWith("memory_")) return "";
  return entry.detail;
}

export function latestSync(entries: ActivityEntry[]): ActivityEntry | undefined {
  return entries.find((entry) => entry.event === "sync");
}

/** One row per project agent. A drift report lists only the agents that drifted. */
export function agentSyncRows(
  agentIds: string[],
  labelFor: (id: string) => string,
  report: DriftReport | null,
  checkFailed: boolean,
): AgentSyncRow[] {
  return agentIds.map((id) => {
    const drift = report?.drifted ? report.agents.find((agent) => agent.agent_id === id) : undefined;
    let status: AgentSyncStatus = "synced";
    if (drift !== undefined) status = "drifted";
    else if (report === null) status = checkFailed ? "failed" : "checking";
    return { id, label: labelFor(id), status, driftedFiles: drift?.files.length ?? 0 };
  });
}
