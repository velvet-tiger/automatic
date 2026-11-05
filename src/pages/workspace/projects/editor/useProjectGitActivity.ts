import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { GitActivity } from "../types";
import { checkErrorText } from "../helpers";

export type ProjectGitState =
  | { status: "loading" }
  /** The folder is not in a git repository, or git is not installed. */
  | { status: "none" }
  | { status: "failed"; error: string }
  | { status: "loaded"; activity: GitActivity };

const IDLE: ProjectGitState = { status: "none" };

/**
 * The last result for each project, kept for the life of the window. Opening
 * a project again shows it at once, with its final layout, while a fresh read
 * runs. The values are the backend's replies, so a stale entry is only ever
 * shown until that read returns.
 */
const lastKnown = new Map<string, ProjectGitState>();
/** Projects with a read running. Shared, so a remount cannot start a second. */
const inFlight = new Set<string>();
/** Projects asked for again while their read ran. */
const pending = new Set<string>();
/** Mounted hooks, told when any project's result changes. */
const listeners = new Set<() => void>();

/** Test support: forget every cached result and running read. */
export function clearGitActivityCache(): void {
  lastKnown.clear();
  inFlight.clear();
  pending.clear();
}

/**
 * Read `key`'s history into the cache. One read per project runs at a time.
 * A request made while one runs is held and made once that read finishes,
 * so a burst of focus events costs at most one more read and a save is never
 * answered with a read that started before it.
 */
async function loadGitActivity(key: string): Promise<void> {
  if (inFlight.has(key)) {
    pending.add(key);
    return;
  }
  inFlight.add(key);
  try {
    const reply: unknown = await invoke("get_project_git_activity", { project: key });
    lastKnown.set(key, gitStateFromReply(reply));
  } catch (err: unknown) {
    console.error("Failed to load git activity:", err);
    lastKnown.set(key, { status: "failed", error: checkErrorText(err) });
  } finally {
    inFlight.delete(key);
  }
  for (const listener of listeners) listener();
  if (pending.delete(key)) void loadGitActivity(key);
}

function isGitActivity(value: unknown): value is GitActivity {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as Partial<GitActivity>;
  return Array.isArray(candidate.commits)
    && typeof candidate.repo === "object" && candidate.repo !== null
    && typeof candidate.truncated === "boolean"
    && typeof candidate.history_days === "number"
    && typeof candidate.line_stats === "boolean";
}

const SAFE_DIRECTORY_COMMAND = "git config --global --add safe.directory ";

/**
 * Splits a git error into its explanation and the command that fixes it.
 * When git refuses a folder another user owns, the backend puts git's own
 * `safe.directory` command on the error's last line. `command` is null for
 * every other error.
 */
export function splitFixCommand(error: string): { message: string; command: string | null } {
  const lines = error.split("\n");
  const last = lines[lines.length - 1].trim();
  if (lines.length < 2 || !last.startsWith(SAFE_DIRECTORY_COMMAND)) {
    return { message: error, command: null };
  }
  return { message: lines.slice(0, -1).join("\n").trim(), command: last };
}

/** Checks the backend's reply, since a malformed one would break the charts. */
export function gitStateFromReply(reply: unknown): ProjectGitState {
  if (reply === null) return { status: "none" };
  if (isGitActivity(reply)) return { status: "loaded", activity: reply };
  return { status: "failed", error: "The git history reply was not in the expected format." };
}

/**
 * Loads a project's git history for the Summary.
 *
 * `projectKey` is the project's `local_key`, the one identifier that never
 * matches two checkouts. It is null while a new project is being created and
 * has no saved record. The backend reads the saved folder, so loads follow
 * the saved project: `refreshKey` changes when the project's newest activity
 * row does, which a save writes. The window regaining focus also reloads, so
 * the uncommitted count follows work done in another app. Results are cached
 * by project, so a reply for a project that is no longer shown is kept but
 * not displayed.
 */
export function useProjectGitActivity(
  projectKey: string | null,
  refreshKey: number | undefined,
): ProjectGitState {
  const [, setVersion] = useState(0);
  const currentKey = useRef(projectKey);
  currentKey.current = projectKey;

  useEffect(() => {
    const listener = () => setVersion((v) => v + 1);
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }, []);

  const load = useCallback(() => {
    const key = currentKey.current;
    if (key !== null) void loadGitActivity(key);
  }, []);

  useEffect(() => {
    load();
  }, [load, projectKey, refreshKey]);

  useEffect(() => {
    window.addEventListener("focus", load);
    return () => window.removeEventListener("focus", load);
  }, [load]);

  if (projectKey === null) return IDLE;
  return lastKnown.get(projectKey) ?? { status: "loading" };
}
