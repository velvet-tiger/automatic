// Extracted verbatim from ProjectEditor.tsx (Phase 2E — behavior-preserving).

import { MemoryBrowser } from "../../../../../components/MemoryBrowser";
import { ClaudeMemoryPanel } from "../../../../../components/ClaudeMemoryPanel";
import type { Project } from "../../types";

interface MemoryPanelProps {
  /** The project's local_key. */
  projectKey: string;
  project: Project | null;
  memories: Record<string, { value: string; timestamp: string; source: string | null }>;
  loadingMemories: boolean;
  reloadMemories: (projectKey: string) => Promise<void>;
  onError: (msg: string) => void;
}

export function MemoryPanel({ projectKey, project, memories, loadingMemories, reloadMemories, onError }: MemoryPanelProps) {
  return (
    <>
      <MemoryBrowser
        projectKey={projectKey}
        memories={memories}
        loading={loadingMemories}
        onRefresh={() => reloadMemories(projectKey)}
        onError={onError}
      />
      {project?.directory && project.agents.includes("claude") && (
        <ClaudeMemoryPanel
          projectKey={projectKey}
          projectDirectory={project.directory}
          onPromoted={() => reloadMemories(projectKey)}
        />
      )}
    </>
  );
}
