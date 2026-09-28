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
 * project. Keys match first, then names exactly, then names ignoring case,
 * which mirrors the backend resolver. Returns null when nothing matches.
 */
export function resolveProjectKey(
  summaries: readonly ProjectSummary[],
  identifier: string,
): string | null {
  const byKey = summaries.find((s) => s.local_key !== "" && s.local_key === identifier);
  if (byKey) return projectKeyOf(byKey);
  const byName = summaries.find((s) => s.name === identifier);
  if (byName) return projectKeyOf(byName);
  const lower = identifier.toLowerCase();
  const byNameCi = summaries.find((s) => s.name.toLowerCase() === lower);
  return byNameCi ? projectKeyOf(byNameCi) : null;
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
