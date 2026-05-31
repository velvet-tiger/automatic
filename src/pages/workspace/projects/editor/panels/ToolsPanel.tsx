// Extracted verbatim from ProjectEditor.tsx (Phase 2E — behavior-preserving).

import { ProjectToolsTab, ProjectToolDetailPanel } from "../tools/ProjectToolsTab";
import type { Project, ProjectToolEntry } from "../../types";

interface ToolsPanelProps {
  project: Project;
  toolTab: string | null;
  toolEntries: ProjectToolEntry[];
  toolEntriesLoading: boolean;
  loadToolEntries: () => void | Promise<void>;
  setProject: (next: Project) => void;
  setDirty: (v: boolean) => void;
  saveProjectSnapshot: (snapshot: Project) => Promise<boolean>;
}

export function ToolsPanel({
  project,
  toolTab,
  toolEntries,
  toolEntriesLoading,
  loadToolEntries,
  setProject,
  setDirty,
  saveProjectSnapshot,
}: ToolsPanelProps) {
  if (toolTab === null) {
    return (
      <ProjectToolsTab
        projectDir={project.directory}
        projectTools={project.tools ?? []}
        entries={toolEntries}
        loading={toolEntriesLoading}
        onReload={loadToolEntries}
        onToolsChange={(tools) => {
          const updated = { ...project, tools, updated_at: new Date().toISOString() };
          setProject(updated);
          setDirty(false);
          saveProjectSnapshot(updated);
        }}
      />
    );
  }
  const entry = toolEntries.find((e) => e.name === toolTab);
  if (!entry) return <p className="text-[12px] text-text-muted">Tool not found.</p>;
  return (
    <ProjectToolDetailPanel
      entry={entry}
      projectDir={project.directory}
      active={(project.tools ?? []).includes(entry.name)}
      onAdd={() => {
        const tools = [...new Set([...(project.tools ?? []), entry.name])];
        const updated = { ...project, tools, updated_at: new Date().toISOString() };
        setProject(updated);
        setDirty(false);
        saveProjectSnapshot(updated);
      }}
      onRemove={() => {
        const tools = (project.tools ?? []).filter((t) => t !== entry.name);
        const updated = { ...project, tools, updated_at: new Date().toISOString() };
        setProject(updated);
        setDirty(false);
        saveProjectSnapshot(updated);
      }}
    />
  );
}
