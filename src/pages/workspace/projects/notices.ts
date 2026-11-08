import { checkFailureNotices } from "./helpers";
import type {
  DriftReport,
  Project,
  ProjectNotice,
  ProjectNoticeSeverity,
  ProjectProblem,
  ProjectProblemsReport,
} from "./types";

/** Everything the project editor knows that can raise a notice. */
export interface ProjectNoticeInputs {
  project: Project;
  /** local_key of the open project, or null while none is stored yet. */
  selectedKey: string | null;
  isCreating: boolean;
  /** The project has unsaved edits. Problems and sync notes describe the saved state, so they wait. */
  dirty: boolean;
  readOnly: boolean;
  driftReport: DriftReport | null;
  driftCheckError: string | null;
  problemsReport: ProjectProblemsReport | null;
  problemsCheckError: string | null;
  /**
   * The project the four check values above belong to. The editor clears
   * them in an effect after a project switch, so for one render they still
   * describe the previous project. They are ignored until this matches
   * `selectedKey`.
   */
  checksKey: string | null;
}

export const NOTICE_SEVERITIES: readonly ProjectNoticeSeverity[] = ["problem", "warning", "notice"];

function waitingFor(pending: string[] | undefined, ignored: string[] | undefined): string[] {
  const skip = ignored ?? [];
  return (pending ?? []).filter((entry) => !skip.includes(entry));
}

/**
 * A problem's identity across checks. It must not depend on the problem's
 * position, or fixing one problem would make the next look new and reopen a
 * hidden list. Some titles change with the number of servers ("MCP server"
 * or "MCP servers"), so the plural is folded away.
 */
function problemKey(problem: ProjectProblem): string {
  return `problem:${problem.kind}:${problem.title.toLowerCase().replace(/\bservers\b/g, "server")}`;
}

/**
 * Build the notice list for the open project, most urgent first. Each rule
 * for when a notice shows is the rule its separate banner used before the
 * list replaced them. Pure: reads nothing and changes nothing.
 */
export function collectProjectNotices(inputs: ProjectNoticeInputs): ProjectNotice[] {
  const { project, selectedKey, isCreating, dirty, readOnly } = inputs;
  if (isCreating) return [];
  const current = inputs.checksKey === selectedKey;
  const driftReport = current ? inputs.driftReport : null;
  const problemsReport = current ? inputs.problemsReport : null;
  const driftCheckError = current ? inputs.driftCheckError : null;
  const problemsCheckError = current ? inputs.problemsCheckError : null;

  const notices: ProjectNotice[] = [];
  const checked = Boolean(project.directory) && project.agents.length > 0;
  const hasKey = Boolean(selectedKey);

  if (project.directory_missing) {
    notices.push({
      id: "missing_directory",
      severity: "problem",
      count: 1,
      source: { kind: "missing_directory", directory: project.directory },
    });
  }

  // A failed check is shown even with unsaved edits: the user must be able
  // to see that the checks are not running.
  if (checked) {
    for (const failure of checkFailureNotices(driftCheckError, problemsCheckError)) {
      notices.push({
        id: `check_failure:${failure.title}`,
        severity: "problem",
        count: 1,
        source: { kind: "check_failure", title: failure.title, error: failure.error },
      });
    }
  }

  const report = problemsReport;
  if (checked && !dirty && report?.has_problems) {
    const seen = new Map<string, number>();
    report.problems.forEach((problem) => {
      const key = problemKey(problem);
      const repeat = seen.get(key) ?? 0;
      seen.set(key, repeat + 1);
      notices.push({
        id: `${key}#${repeat}`,
        severity: "problem",
        count: 1,
        source: { kind: "problem", problem },
      });
    });
  }

  // Drift describes files on disk, not the edits in memory, so it ignores
  // `dirty`. A read-only project still shows it, as information.
  if (checked && driftReport?.drifted) {
    notices.push({ id: "drift", severity: "warning", count: 1, source: { kind: "drift", report: driftReport } });
  }

  if (hasKey && project.local_awaiting_confirmation === true) {
    notices.push({ id: "pending_registration", severity: "warning", count: 1, source: { kind: "pending_registration" } });
  }

  const profiles = waitingFor(project.pending_profiles, project.local_ignored_profiles);
  if (hasKey && !readOnly && profiles.length > 0) {
    notices.push({
      id: "pending_profiles",
      severity: "warning",
      count: profiles.length,
      source: { kind: "pending_profiles", names: profiles },
    });
  }

  const contexts = waitingFor(project.pending_contexts, project.local_ignored_contexts);
  if (hasKey && contexts.length > 0) {
    notices.push({
      id: "pending_contexts",
      severity: "warning",
      count: contexts.length,
      source: { kind: "pending_contexts", slugs: contexts },
    });
  }

  if (project.mode === "silent" && project.directory) {
    notices.push({ id: "silent_mode", severity: "notice", count: 1, source: { kind: "silent_mode", directory: project.directory } });
  }

  const notes = report?.notes ?? [];
  if (checked && !dirty && notes.length > 0) {
    notices.push({ id: "sync_notes", severity: "notice", count: 1, source: { kind: "sync_notes", notes } });
  }

  // Stable sort: within one severity the order above is kept.
  return notices.sort((a, b) => NOTICE_SEVERITIES.indexOf(a.severity) - NOTICE_SEVERITIES.indexOf(b.severity));
}

/** How many rows of each severity the list shows. */
export function countNotices(notices: ProjectNotice[]): Record<ProjectNoticeSeverity, number> {
  const counts: Record<ProjectNoticeSeverity, number> = { problem: 0, warning: 0, notice: 0 };
  for (const notice of notices) counts[notice.severity] += notice.count;
  return counts;
}

/** The ids of the problems in the list, which the user saw when they hid it. */
export function problemIds(notices: ProjectNotice[]): string[] {
  return notices.filter((n) => n.severity === "problem").map((n) => n.id);
}

/**
 * Whether a list the user hid stays hidden. It opens again as soon as a
 * problem appears that was not in the list when the user hid it.
 */
export function staysHidden(seenProblemIds: string[], notices: ProjectNotice[]): boolean {
  return problemIds(notices).every((id) => seenProblemIds.includes(id));
}

const HIDDEN_KEY_PREFIX = "automatic.project_notices.hidden.";

/**
 * The problem ids stored when the user hid the list for this project on this
 * machine, or null when the list is open. A convenience only: any storage
 * failure means "open", so a notice is never hidden by accident.
 */
export function readHiddenNotices(projectKey: string): string[] | null {
  try {
    const raw = localStorage.getItem(HIDDEN_KEY_PREFIX + projectKey);
    if (raw === null) return null;
    const parsed: unknown = JSON.parse(raw);
    if (Array.isArray(parsed) && parsed.every((id): id is string => typeof id === "string")) return parsed;
  } catch (err) {
    console.warn("Could not read the hidden state of the notice list:", err);
  }
  return null;
}

/** Store the hidden state for this project, or clear it when `seen` is null. */
export function writeHiddenNotices(projectKey: string, seen: string[] | null): void {
  try {
    if (seen === null) localStorage.removeItem(HIDDEN_KEY_PREFIX + projectKey);
    else localStorage.setItem(HIDDEN_KEY_PREFIX + projectKey, JSON.stringify(seen));
  } catch (err) {
    console.warn("Could not store the hidden state of the notice list:", err);
  }
}
