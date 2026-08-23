// Extracted verbatim from ProjectEditor.tsx (Phase 2E — behavior-preserving).

import type { MouseEventHandler } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ExternalLink } from "lucide-react";
import { SummaryInventoryRow, SummarySidebarSection } from "../SummaryMetricCard";
import { ActivityFeed } from "../ActivityFeed";
import type { ActivityEntry, Project, ProjectFileInfo } from "../../types";

type ProjectTabId =
  | "summary" | "agents" | "commands" | "hooks" | "custom_agents" | "skills"
  | "mcp_servers" | "groups" | "project_file" | "rules" | "context"
  | "docs_files" | "docs_links" | "docs_notes" | "memory" | "activity"
  | "recommendations" | "tools" | "settings";

type DocEntry = { path: string; summary?: string };

interface SummaryPanelProps {
  project: Project;
  isCreating: boolean;
  memories: Record<string, unknown>;
  projectFiles: ProjectFileInfo[];
  linkDocEntries: [string, DocEntry][];
  recsDisplayCount: number;
  activityEntries: ActivityEntry[];
  loadingActivity: boolean;
  loadingMemories: boolean;
  selectTab: (tab: ProjectTabId) => void;
  updateField: <K extends keyof Project>(key: K, value: Project[K]) => void;
  handleExternalLinkClick: (url: string, isExternal?: boolean) => MouseEventHandler<HTMLElement>;
}

export function SummaryPanel({
  project,
  isCreating,
  memories,
  projectFiles,
  linkDocEntries,
  recsDisplayCount,
  activityEntries,
  loadingActivity,
  loadingMemories,
  selectTab,
  updateField,
  handleExternalLinkClick,
}: SummaryPanelProps) {
  const totalSkills = project.skills.length + (project.custom_skills?.length ?? 0);
  const totalRules = ((project.file_rules || {})["_project"] || []).length + (project.custom_rules?.length ?? 0);
  const totalSubAgents = (project.user_agents?.length ?? 0) + (project.custom_agents?.length ?? 0);
  const totalCommands = (project.user_commands?.length ?? 0) + (project.custom_commands?.length ?? 0);
  const totalHooks = project.hooks?.length ?? 0;
  const memoryCount = Object.keys(memories).length;
  const hasInstructionFiles = projectFiles.some((file) => file.exists);
  const instructionFileCount = projectFiles.filter((file) => file.exists).length;
  const instructionStatus = !project.directory || project.agents.length === 0
    ? "Add a directory and at least one agent to generate instruction files."
    : hasInstructionFiles
      ? `${instructionFileCount} instruction file${instructionFileCount === 1 ? "" : "s"} available.`
      : "No instruction files found for this project yet.";
  const recentDocsLinks = linkDocEntries.slice(0, 5);
  const needsSetup = !isCreating && (!project.directory || project.agents.length === 0);

  const inventoryItems = [
    { label: "skills", count: totalSkills, onView: () => selectTab("skills") },
    { label: "mcp", count: project.mcp_servers.length, onView: () => selectTab("mcp_servers") },
    { label: "rules", count: totalRules, onView: () => selectTab("rules") },
    { label: "agents", count: totalSubAgents, onView: () => selectTab("custom_agents") },
    { label: "cmds", count: totalCommands, onView: () => selectTab("commands") },
    { label: "hooks", count: totalHooks, onView: () => selectTab("hooks") },
  ];

  return (
    <div className="space-y-5">
      {/* Attention: setup gaps + recommendations */}
      {(needsSetup || recsDisplayCount > 0) && (
        <div className="space-y-3">
          {needsSetup && (
            <section className="rounded-lg border border-border-strong/35 bg-bg-input px-4 py-3">
              <h3 className="mb-1 text-[13px] font-medium text-text-base">Complete Setup</h3>
              <p className="mb-3 text-[12px] leading-relaxed text-text-muted">
                To start using this project, complete these steps:
              </p>
              <ol className="space-y-2 text-[12px] text-text-base">
                {!project.directory && (
                  <li className="flex items-start gap-2">
                    <span className="mt-0.5 w-4 shrink-0 tabular-nums text-text-muted/50">1.</span>
                    <div>
                      <button
                        type="button"
                        onClick={async () => {
                          let selected: string | null = null;
                          try {
                            selected = await invoke<string | null>("open_directory_dialog");
                          } catch (err) {
                            console.error("open_directory_dialog failed:", err);
                          }
                          if (selected) updateField("directory", selected);
                        }}
                        className="font-medium text-text-base transition-colors hover:text-brand"
                      >
                        Set project directory
                      </button>
                      <div className="mt-0.5 text-[11px] text-text-muted">
                        Click the path below the project name, or click here
                      </div>
                    </div>
                  </li>
                )}
                {project.agents.length === 0 && (
                  <li className="flex items-start gap-2">
                    <span className="mt-0.5 w-4 shrink-0 tabular-nums text-text-muted/50">
                      {!project.directory ? "2." : "1."}
                    </span>
                    <div>
                      <button
                        type="button"
                        onClick={() => selectTab("agents")}
                        className="font-medium text-text-base transition-colors hover:text-brand"
                      >
                        Add agent tools
                      </button>
                      <div className="mt-0.5 text-[11px] text-text-muted">
                        Select which agents will use this project
                      </div>
                    </div>
                  </li>
                )}
                <li className="flex items-start gap-2">
                  <span className="mt-0.5 w-4 shrink-0 text-text-muted/40">·</span>
                  <div>
                    <button
                      type="button"
                      onClick={() => selectTab("skills")}
                      className="text-text-base transition-colors hover:text-brand"
                    >
                      Add skills (optional)
                    </button>
                    <div className="mt-0.5 text-[11px] text-text-muted">
                      Give agents specialized capabilities
                    </div>
                  </div>
                </li>
              </ol>
            </section>
          )}

          {recsDisplayCount > 0 && (
            <div className="flex items-center gap-2 text-[12px] text-text-muted/60">
              <span>
                {recsDisplayCount === 1
                  ? "1 recommendation"
                  : `${recsDisplayCount} recommendations`}
              </span>
              <button
                type="button"
                onClick={() => selectTab("recommendations")}
                className="font-medium text-text-muted transition-colors hover:text-text-base"
              >
                Review
              </button>
            </div>
          )}
        </div>
      )}

      {/* Demoted inventory — overview-style muted metrics */}
      <SummaryInventoryRow items={inventoryItems} />

      <div className="grid grid-cols-[minmax(0,1fr)_280px] gap-6 max-xl:grid-cols-1">
        <div className="min-w-0">
          <ActivityFeed entries={activityEntries} loading={loadingActivity} />
        </div>

        <div className="space-y-3">
          <SummarySidebarSection title="Instructions">
            <p className="text-[12px] leading-relaxed text-text-muted">{instructionStatus}</p>
            {!hasInstructionFiles && project.directory && project.agents.length > 0 && (
              <p className="text-[11px] text-text-muted/50">Missing instruction files</p>
            )}
            <button
              type="button"
              onClick={() => selectTab("project_file")}
              className="text-[11px] font-medium text-text-muted transition-colors hover:text-text-base"
            >
              View instructions
            </button>
          </SummarySidebarSection>

          <SummarySidebarSection title="Docs">
            {recentDocsLinks.length === 0 ? (
              <p className="text-[12px] text-text-muted">No docs links added yet.</p>
            ) : (
              <div className="space-y-1">
                {recentDocsLinks.map(([key, entry]) => (
                  <button
                    key={key}
                    type="button"
                    onClick={handleExternalLinkClick(entry.path)}
                    className="flex w-full items-start gap-2 rounded-md px-1.5 py-1 text-left transition-colors hover:bg-bg-sidebar"
                  >
                    <ExternalLink size={11} className="mt-0.5 shrink-0 text-text-muted" />
                    <span className="min-w-0 truncate text-[12px] text-text-base">
                      {entry.summary || key}
                    </span>
                  </button>
                ))}
              </div>
            )}
          </SummarySidebarSection>

          <SummarySidebarSection title="Memory">
            {loadingMemories ? (
              <p className="text-[12px] text-text-muted">Loading memory…</p>
            ) : (
              <>
                <p className="text-[12px] text-text-muted">
                  {memoryCount === 0
                    ? "No stored memories for this project yet."
                    : `${memoryCount} ${memoryCount === 1 ? "memory" : "memories"} available for connected agents.`}
                </p>
                <button
                  type="button"
                  onClick={() => selectTab("memory")}
                  className="text-[11px] font-medium text-text-muted transition-colors hover:text-text-base"
                >
                  View memory
                </button>
              </>
            )}
          </SummarySidebarSection>
        </div>
      </div>
    </div>
  );
}
