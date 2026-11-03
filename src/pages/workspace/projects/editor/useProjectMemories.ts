import { useCallback, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { MemoryRecord } from "../../../../components/MemoryBrowser";

export interface ProjectMemories {
  /**
   * Automatic's memories for the selected project, or null while none are
   * loaded for it (a load is pending, failed, or belongs to another project).
   */
  selectedMemories: MemoryRecord | null;
  loadingMemories: boolean;
  /** True when the selected project's latest load failed. */
  memoriesLoadFailed: boolean;
  /** Load the memories for `projectKey`. Errors go to `onError`. */
  loadMemories: (projectKey: string) => Promise<void>;
}

/**
 * Loads a project's Automatic memories for the project editor.
 *
 * Loads can overlap when the user switches projects, and can finish in any
 * order. Only the latest request may change state, including the loading
 * flag, so a slow load for an earlier project never replaces or blanks the
 * current one. A load for a project that is no longer selected is ignored,
 * because a slow project switch can reach it after the next project's load.
 * Loaded entries also carry their project, so they are shown
 * only while that project is selected.
 */
export function useProjectMemories(
  selectedKey: string | null,
  onError: (msg: string) => void,
): ProjectMemories {
  const [loaded, setLoaded] = useState<{ projectKey: string; entries: MemoryRecord } | null>(null);
  const [loadingMemories, setLoadingMemories] = useState(false);
  const [failedKey, setFailedKey] = useState<string | null>(null);
  const latestRequest = useRef(0);
  const selectedRef = useRef(selectedKey);
  selectedRef.current = selectedKey;

  const loadMemories = useCallback(
    async (projectKey: string) => {
      if (projectKey !== selectedRef.current) return;
      const request = ++latestRequest.current;
      const isLatest = () => request === latestRequest.current;
      setLoadingMemories(true);
      try {
        const data: MemoryRecord = await invoke("get_project_memories", { project: projectKey });
        if (isLatest()) {
          setLoaded({ projectKey, entries: data });
          setFailedKey(null);
        }
      } catch (err: unknown) {
        if (!isLatest()) return;
        // A failed load drops what was loaded for this project, so no panel
        // shows entries the store may no longer hold. The error is shown
        // because a stale list makes saves look like they did nothing.
        setLoaded((prev) => (prev?.projectKey === projectKey ? null : prev));
        setFailedKey(projectKey);
        const msg = err instanceof Error ? err.message : String(err);
        console.error("Failed to load memories:", err);
        onError(`Failed to load memories: ${msg}`);
      } finally {
        if (isLatest()) setLoadingMemories(false);
      }
    },
    [onError],
  );

  const selectedMemories =
    loaded !== null && loaded.projectKey === selectedKey ? loaded.entries : null;

  const memoriesLoadFailed = selectedKey !== null && failedKey === selectedKey;

  return { selectedMemories, loadingMemories, memoriesLoadFailed, loadMemories };
}
