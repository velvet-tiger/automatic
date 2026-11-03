import { useCallback, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ActivityEntry } from "../types";
import { ACTIVITY_HISTORY_LIMIT } from "./summaryStats";

const NO_ENTRIES: ActivityEntry[] = [];

export interface ProjectActivity {
  /** Newest first. Empty unless the selected project's events are loaded. */
  activityEntries: ActivityEntry[];
  /** True until the selected project's first load finishes. */
  loadingActivity: boolean;
  activityLoadFailed: boolean;
  loadActivity: (projectKey: string) => Promise<void>;
}

/**
 * Loads a project's activity for the project editor.
 *
 * Loads can overlap and finish in any order, so only the latest request may
 * change state. A load for a project that is no longer selected is ignored,
 * because a slow project switch can reach it after the next project's load.
 * Loaded entries carry their project, so they show only while
 * that project is selected. A refresh keeps the current entries on screen
 * until its reply arrives, so saving does not blank the Summary charts.
 */
export function useProjectActivity(selectedKey: string | null): ProjectActivity {
  const [loaded, setLoaded] = useState<{ projectKey: string; entries: ActivityEntry[] } | null>(null);
  const [failedKey, setFailedKey] = useState<string | null>(null);
  const latestRequest = useRef(0);
  const selectedRef = useRef(selectedKey);
  selectedRef.current = selectedKey;

  const loadActivity = useCallback(async (projectKey: string) => {
    if (projectKey !== selectedRef.current) return;
    const request = ++latestRequest.current;
    try {
      const raw: string = await invoke("get_project_activity", {
        project: projectKey,
        limit: ACTIVITY_HISTORY_LIMIT,
      });
      if (request !== latestRequest.current) return;
      setLoaded({ projectKey, entries: JSON.parse(raw) as ActivityEntry[] });
      setFailedKey(null);
    } catch (err: unknown) {
      if (request !== latestRequest.current) return;
      console.error("Failed to load activity:", err);
      setLoaded(null);
      setFailedKey(projectKey);
    }
  }, []);

  const isLoaded = loaded !== null && loaded.projectKey === selectedKey;
  const failed = selectedKey !== null && failedKey === selectedKey;
  return {
    activityEntries: isLoaded ? loaded.entries : NO_ENTRIES,
    loadingActivity: selectedKey !== null && !isLoaded && !failed,
    activityLoadFailed: failed,
    loadActivity,
  };
}
