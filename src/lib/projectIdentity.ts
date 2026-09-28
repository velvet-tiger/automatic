/**
 * Project identity helpers for the frontend.
 *
 * The frontend addresses a project by its `local_key` (the identity of one
 * checkout on this machine). Names are display labels only. Every backend
 * command that takes a project accepts either a `local_key` or a name, so a
 * name is still a valid fallback for a project that has no key yet, or for a
 * row that names an unregistered (orphaned) project.
 *
 * See automatic-meta/general/plans/projects/project-identity.md, stage 3b.
 */
import { invoke } from "@tauri-apps/api/core";
import type { ProjectSummary } from "../pages/workspace/projects/types";
import { migrateProjectStorageOnce } from "./projectStorageMigration";

/**
 * The identifier the frontend uses for a project: its `local_key`, or its
 * name while the startup backfill has not minted a key yet.
 */
export function projectKeyOf(summary: ProjectSummary): string {
  return summary.local_key !== "" ? summary.local_key : summary.name;
}

/**
 * The identifier for a backend row that names a project. Rows carry the
 * display name in `project` and the `local_key` when the project is
 * registered; orphaned rows fall back to the name.
 */
export function rowProjectKey(row: { project: string; local_key?: string }): string {
  return row.local_key !== undefined && row.local_key !== "" ? row.local_key : row.project;
}

/**
 * Resolve an identifier (a `local_key` or a name) to the key of a known
 * project. Keys match first, then a name only one project has (ignoring
 * case), which mirrors the backend resolver. Returns null when nothing
 * matches, or when several projects share the name: guessing would open
 * the wrong one.
 */
export function resolveProjectKey(
  summaries: readonly ProjectSummary[],
  identifier: string,
): string | null {
  const byKey = summaries.find((s) => s.local_key !== "" && s.local_key === identifier);
  if (byKey) return projectKeyOf(byKey);
  const lower = identifier.toLowerCase();
  const named = summaries.filter((s) => s.name.toLowerCase() === lower);
  return named.length === 1 ? projectKeyOf(named[0]!) : null;
}

/** Display name for a project key, or null when the key is unknown. */
export function projectNameForKey(
  summaries: readonly ProjectSummary[],
  key: string,
): string | null {
  const match = summaries.find((s) => projectKeyOf(s) === key);
  return match ? match.name : null;
}

/**
 * Group members (as `read_group` returns them: `local_key`s, or stored
 * values no project claims) without the project `key` belongs to. A group
 * stores the project `id`, so every checkout of that project goes, not
 * just `key`: leaving another checkout's key would save the id back.
 */
export function membersWithoutProject(
  members: readonly string[],
  key: string,
  summaries: readonly ProjectSummary[],
): string[] {
  const project = summaries.find((s) => projectKeyOf(s) === key);
  const id = project?.id ?? "";
  const sameProject = new Set(
    id === "" ? [key] : summaries.filter((s) => s.id === id).map(projectKeyOf).concat(key),
  );
  return members.filter((member) => !sameProject.has(member));
}

/**
 * How to show one project: its name, plus a folder hint when another
 * project has the same name. `hint` is null when the name is unique.
 */
export interface ProjectLabel {
  name: string;
  hint: string | null;
}

/** Split a folder path into its non-empty components (`/` or `\`). */
function pathParts(directory: string): string[] {
  return directory.split(/[\\/]+/).filter((part) => part !== "");
}

/** The last `depth` components of `parts` (all of them when fewer), joined with `/`. */
function trailingSuffix(parts: readonly string[], depth: number): string {
  return parts.slice(Math.max(0, parts.length - depth)).join("/");
}

/**
 * The shortest trailing part of `own`'s folder that no other project in
 * `peers` shares at the same depth. When every trailing part of `own` is
 * shared (its folder is a suffix of another's), the full folder is shown.
 * Compared exactly, since two folders can differ only by case on a
 * case-sensitive disk.
 */
function distinguishingHint(own: ProjectSummary, peers: readonly ProjectSummary[]): string {
  if (own.directory === "") return "no folder";
  const parts = pathParts(own.directory);
  const others = peers.filter((p) => p !== own).map((p) => pathParts(p.directory));
  for (let depth = 1; depth <= parts.length; depth++) {
    const mine = trailingSuffix(parts, depth);
    if (others.every((o) => trailingSuffix(o, depth) !== mine)) return mine;
  }
  return own.directory;
}

/**
 * Display labels for every project, keyed by `projectKeyOf`. A name that
 * only one project has (ignoring case) is shown as it is. Where a name
 * repeats, each project also gets a hint: the shortest trailing part of its
 * folder that tells it apart from the others with that name, for example
 * `consultmed/website` next to `_active/website`.
 */
export function projectLabels(summaries: readonly ProjectSummary[]): Map<string, ProjectLabel> {
  const byName = new Map<string, ProjectSummary[]>();
  for (const summary of summaries) {
    const lower = summary.name.toLowerCase();
    const group = byName.get(lower);
    if (group) group.push(summary);
    else byName.set(lower, [summary]);
  }
  const labels = new Map<string, ProjectLabel>();
  for (const summary of summaries) {
    const peers = byName.get(summary.name.toLowerCase()) ?? [summary];
    labels.set(projectKeyOf(summary), {
      name: summary.name,
      hint: peers.length > 1 ? distinguishingHint(summary, peers) : null,
    });
  }
  return labels;
}

/**
 * The label for `key` from `labels`, or the plain `fallbackName` (the key
 * itself when absent) for a project the list does not know.
 */
export function projectLabelFor(
  labels: ReadonlyMap<string, ProjectLabel>,
  key: string,
  fallbackName?: string,
): ProjectLabel {
  return labels.get(key) ?? { name: fallbackName ?? key, hint: null };
}

/**
 * Projects named `name` (ignoring case, after trimming), leaving out the
 * one whose key is `excludeKey`. Used to tell the user that a name is
 * shared. Sharing a name is allowed.
 */
export function projectsNamed(
  summaries: readonly ProjectSummary[],
  name: string,
  excludeKey: string | null = null,
): ProjectSummary[] {
  const lower = name.trim().toLowerCase();
  if (lower === "") return [];
  return summaries.filter((s) => s.name.toLowerCase() === lower && projectKeyOf(s) !== excludeKey);
}

/** A folder shortened for a sentence: its last two parts after `…/`, or the whole path when shorter. */
export function shortFolder(directory: string): string {
  const parts = pathParts(directory);
  return parts.length <= 2 ? directory : `…/${trailingSuffix(parts, 2)}`;
}

/**
 * A muted note for the Add Project wizard when `others` (see
 * `projectsNamed`) already use `name`. Null when nobody does. The note
 * informs; it never blocks, because projects may share a name.
 */
export function sharedNameNote(name: string, others: readonly ProjectSummary[]): string | null {
  const shown = name.trim();
  if (others.length === 0) return null;
  if (others.length > 1) {
    return `${others.length} other projects are also called \u201c${shown}\u201d. You can keep this name.`;
  }
  const folder = others[0]!.directory;
  const where = folder === "" ? "" : ` (${shortFolder(folder)})`;
  return `Another project is also called \u201c${shown}\u201d${where}. You can keep this name.`;
}

/** A label as one line of plain text: `name · hint`, or the name alone. */
export function projectLabelText(label: ProjectLabel): string {
  return label.hint === null ? label.name : `${label.name} · ${label.hint}`;
}

/**
 * Fetch every registered project as `{local_key, id, name, directory}`.
 *
 * The first successful load also moves localStorage entries that still name
 * projects over to their keys (see `projectStorageMigration.ts`), so every
 * consumer that reads those entries after loading summaries sees keys.
 */
export async function loadProjectSummaries(): Promise<ProjectSummary[]> {
  const summaries = await invoke<ProjectSummary[]>("get_project_summaries");
  if (!Array.isArray(summaries)) {
    throw new Error("get_project_summaries did not return a list");
  }
  try {
    migrateProjectStorageOnce(window.localStorage, summaries);
  } catch (err: unknown) {
    // localStorage can be unavailable (private mode, quota). Stored UI
    // preferences are not worth failing the project list over.
    console.warn("Project localStorage migration skipped:", err);
  }
  return summaries;
}
