// Extracted verbatim from ProjectEditor.tsx (Phase 2E — behavior-preserving).

import { MemoryBrowser, type MemoryRecord } from "../../../../../components/MemoryBrowser";
import { ClaudeMemoryPanel } from "../../../../../components/ClaudeMemoryPanel";
import type { Project } from "../../types";

interface MemoryPanelProps {
  /** The project's local_key. */
  projectKey: string;
  project: Project | null;
  /**
   * Automatic's memories for this project, or null while none have been
   * loaded for it (a load is pending, failed, or belongs to another project).
   */
  memories: MemoryRecord | null;
  loadingMemories: boolean;
  reloadMemories: (projectKey: string) => Promise<void>;
  onError: (msg: string) => void;
}

const NO_MEMORIES: MemoryRecord = {};

export function MemoryPanel({ projectKey, project, memories, loadingMemories, reloadMemories, onError }: MemoryPanelProps) {
  return (
    <>
      <MemoryBrowser
        projectKey={projectKey}
        memories={memories ?? NO_MEMORIES}
        loading={loadingMemories}
        onRefresh={() => reloadMemories(projectKey)}
        onError={onError}
      />
      {project?.directory && project.agents.includes("claude") && (
        <ClaudeMemoryPanel
          // A new project gets a fresh panel, so no confirmation or open
          // dialog from the previous project carries over.
          key={projectKey}
          projectKey={projectKey}
          projectDirectory={project.directory}
          storedMemories={memories ?? undefined}
          onPromoted={() => reloadMemories(projectKey)}
        />
      )}
    </>
  );
}
