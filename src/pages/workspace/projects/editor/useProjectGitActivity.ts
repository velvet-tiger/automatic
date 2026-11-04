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

function isGitActivity(value: unknown): value is GitActivity {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as Partial<GitActivity>;
  return Array.isArray(candidate.commits)
    && typeof candidate.repo === "object" && candidate.repo !== null
    && typeof candidate.truncated === "boolean"
    && typeof candidate.history_days === "number";
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
 * Loads again when the project or its folder changes, and when the window
 * regains focus, so the uncommitted count follows work done in another app.
 * Only the latest request may change state, because a slow repository can
 * answer after the user has moved to another project. A refresh keeps the
 * current data on screen until its reply arrives.
 *
 * `project` is null while a new project is being created and has no saved
 * record for the backend to read. The backend reads the saved folder, so a
 * folder change shows once saved: `refreshKey` changes when the project's
 * newest activity row does, which a save writes.
 */
export function useProjectGitActivity(
  project: string | null,
  directory: string,
  refreshKey: number | undefined,
): ProjectGitState {
  const [state, setState] = useState<{ key: string; value: ProjectGitState } | null>(null);
  const latestRequest = useRef(0);
  const key = project === null || directory.trim() === "" ? null : `${project}\u0000${directory}`;

  const load = useCallback(async () => {
    if (project === null || key === null) return;
    const request = ++latestRequest.current;
    try {
      const reply: unknown = await invoke("get_project_git_activity", { project });
      if (request !== latestRequest.current) return;
      setState({ key, value: gitStateFromReply(reply) });
    } catch (err: unknown) {
      if (request !== latestRequest.current) return;
      console.error("Failed to load git activity:", err);
      setState({ key, value: { status: "failed", error: checkErrorText(err) } });
    }
  }, [project, key]);

  useEffect(() => {
    void load();
    const onFocus = () => void load();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [load, refreshKey]);

  if (key === null) return IDLE;
  if (state === null || state.key !== key) return { status: "loading" };
  return state.value;
}
