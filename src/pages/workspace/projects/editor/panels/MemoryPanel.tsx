// Extracted verbatim from ProjectEditor.tsx (Phase 2E — behavior-preserving).

import { MemoryBrowser } from "../../../../../components/MemoryBrowser";
import { ClaudeMemoryPanel } from "../../../../../components/ClaudeMemoryPanel";
import type { Project } from "../../types";

interface MemoryPanelProps {
  projectName: string;
  project: Project | null;
  memories: Record<string, { value: string; timestamp: string; source: string | null }>;
  loadingMemories: boolean;
  reloadMemories: (projectName: string) => Promise<void>;
  onError: (msg: string) => void;
}

export function MemoryPanel({ projectName, project, memories, loadingMemories, reloadMemories, onError }: MemoryPanelProps) {
  return (
    <>
      <MemoryBrowser
        projectName={projectName}
        memories={memories}
        loading={loadingMemories}
        onRefresh={() => reloadMemories(projectName)}
        onError={onError}
      />
      {project?.directory && project.agents.includes("claude") && (
        <ClaudeMemoryPanel
          projectName={projectName}
          projectDirectory={project.directory}
          onPromoted={() => reloadMemories(projectName)}
        />
      )}
    </>
  );
}
