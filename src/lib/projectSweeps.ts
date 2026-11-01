/**
 * Progress events from the background queue that re-syncs projects after a
 * library change (`src-tauri/src/commands/project_sweeps.rs`).
 */
import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";

export const PROJECT_SWEEP_EVENT = "project-sweep-progress";

export interface ProjectSweepFailure {
  project: string;
  error: string;
}

export interface ProjectSweepEvent {
  job_id: number;
  /** Finishes the phrase "Updating projects for ...", e.g. "rule automatic-code". */
  label: string;
  done: number;
  total: number;
  failed: ProjectSweepFailure[];
  finished: boolean;
  duration_ms: number;
}

function projectCount(count: number): string {
  return `${count} project${count === 1 ? "" : "s"}`;
}

/** The Task Log line for a job at this point in its run. */
export function projectSweepMessage(event: ProjectSweepEvent): string {
  const { label, done, total, failed, finished, duration_ms } = event;
  if (!finished) {
    return `Updating projects for ${label}: ${done} of ${total}`;
  }
  if (failed.length > 0) {
    const reasons = failed.map((f) => `${f.project}: ${f.error}`).join("; ");
    return `Could not update ${projectCount(failed.length)} for ${label}. ${reasons}`;
  }
  return `Updated ${projectCount(total)} for ${label} in ${(duration_ms / 1000).toFixed(1)}s`;
}

/** Call `onFinished` each time a background project update ends. */
export function useProjectSweepFinished(onFinished: () => void): void {
  const latest = useRef(onFinished);
  latest.current = onFinished;

  useEffect(() => {
    const unlisten = listen<ProjectSweepEvent>(PROJECT_SWEEP_EVENT, (event) => {
      if (event.payload.finished) latest.current();
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);
}
