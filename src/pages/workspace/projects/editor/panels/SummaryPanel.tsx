// Extracted verbatim from ProjectEditor.tsx (Phase 2E — behavior-preserving).

import type { MouseEventHandler } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ArrowRight, Bot, Code, ExternalLink, Lightbulb, Package, ScrollText, Server, Terminal, Webhook } from "lucide-react";
import { SummaryMetricCard, SummarySidebarSection } from "../SummaryMetricCard";
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
  const instructionStatus = !project.directory || project.agents.length === 0
    ? "Add a directory and at least one agent to generate instruction files."
    : hasInstructionFiles
      ? `${projectFiles.filter((file) => file.exists).length} instruction file${projectFiles.filter((file) => file.exists).length === 1 ? "" : "s"} available.`
      : "No instruction files found for this project yet.";
  const recentDocsLinks = linkDocEntries.slice(0, 5);

  return (
    <div className="space-y-6">
      <div className="grid gap-4 xl:grid-cols-5 md:grid-cols-2">
        <SummaryMetricCard
          icon={<Code size={13} className="text-icon-skill" />}
          label="Skills"
          count={totalSkills}
          accentClass="bg-icon-skill/10"
          onView={() => selectTab("skills")}
        />
        <SummaryMetricCard
          icon={<Server size={13} className="text-icon-mcp" />}
          label="MCP Servers"
          count={project.mcp_servers.length}
          accentClass="bg-icon-mcp/10"
          onView={() => selectTab("mcp_servers")}
        />
        <SummaryMetricCard
          icon={<ScrollText size={13} className="text-icon-rule" />}
          label="Rules"
          count={totalRules}
          accentClass="bg-icon-rule/10"
          onView={() => selectTab("rules")}
        />
        <SummaryMetricCard
          icon={<Bot size={13} className="text-brand" />}
          label="Sub-agents"
          count={totalSubAgents}
          accentClass="bg-brand/10"
          onView={() => selectTab("custom_agents")}
        />
        <SummaryMetricCard
          icon={<Terminal size={13} className="text-icon-command" />}
          label="Commands"
          count={totalCommands}
          accentClass="bg-icon-command/10"
          onView={() => selectTab("commands")}
        />
        <SummaryMetricCard
          icon={<Webhook size={13} className="text-icon-skill" />}
          label="Hooks"
          count={totalHooks}
          accentClass="bg-icon-skill/10"
          onView={() => selectTab("hooks")}
        />
      </div>

      <div className="grid grid-cols-[minmax(0,1fr)_280px] gap-6 max-xl:grid-cols-1">

        {/* ── Column 1: Activity, recommendations, setup ──── */}
        <div className="space-y-6 min-w-0">

          {/* Recommendations banner */}
          {recsDisplayCount > 0 && (
            <div className="flex items-center gap-3 px-4 py-3 rounded-lg bg-warning/5 border border-warning/25">
              <Lightbulb size={14} className="text-warning shrink-0" />
              <p className="flex-1 text-[12px] text-text-muted leading-snug">
                <span className="font-semibold text-text-base">
                  {recsDisplayCount === 1 ? "1 recommendation" : `${recsDisplayCount} recommendations`}
                </span>
                {" "}available for this project.
              </p>
              <button
                onClick={() => selectTab("recommendations")}
                className="shrink-0 flex items-center gap-1 text-[12px] font-medium text-warning hover:text-warning-hover transition-colors"
              >
                Review <ArrowRight size={11} />
              </button>
            </div>
          )}

          {/* Getting Started callout (incomplete setup) */}
          {!isCreating && (!project.directory || project.agents.length === 0) && (
            <section className="bg-gradient-to-br from-brand/10 to-brand/5 border border-brand/20 rounded-lg p-5">
              <div className="flex items-start gap-3">
                <div className="p-2 bg-brand/20 rounded-lg flex-shrink-0">
                  <Package size={18} className="text-brand" />
                </div>
                <div>
                  <h3 className="text-[13px] font-semibold text-text-base mb-2">Complete Setup</h3>
                  <p className="text-[12px] text-text-muted mb-3 leading-relaxed">To start using this project, complete these steps:</p>
                  <ol className="space-y-2 text-[12px] text-text-base">
                    {!project.directory && (
                      <li className="flex items-start gap-2">
                        <div className="w-5 h-5 rounded-full border border-brand flex items-center justify-center flex-shrink-0 mt-0.5">
                          <span className="text-[10px] text-brand">1</span>
                        </div>
                        <div>
                          <button
                            onClick={async () => {
                              let selected: string | null = null;
                              try {
                                selected = await invoke<string | null>("open_directory_dialog");
                              } catch (err) {
                                console.error("open_directory_dialog failed:", err);
                              }
                              if (selected) updateField("directory", selected);
                            }}
                            className="text-brand hover:text-brand-hover transition-colors font-medium"
                          >
                            Set project directory
                          </button>
                          <div className="text-[11px] text-text-muted mt-0.5">Click the path below the project name, or click here</div>
                        </div>
                      </li>
                    )}
                    {project.agents.length === 0 && (
                      <li className="flex items-start gap-2">
                        <div className="w-5 h-5 rounded-full border border-brand flex items-center justify-center flex-shrink-0 mt-0.5">
                          <span className="text-[10px] text-brand">{!project.directory ? "2" : "1"}</span>
                        </div>
                        <div>
                          <button onClick={() => selectTab("agents")} className="text-brand hover:text-brand-hover transition-colors font-medium">Add agent tools</button>
                          <div className="text-[11px] text-text-muted mt-0.5">Select which agents will use this project</div>
                        </div>
                      </li>
                    )}
                    <li className="flex items-start gap-2">
                      <div className="w-5 h-5 rounded-full border border-text-muted/50 flex items-center justify-center flex-shrink-0 mt-0.5">
                        <span className="text-[10px] text-text-muted">•</span>
                      </div>
                      <div>
                         <button onClick={() => selectTab("skills")} className="text-text-base hover:text-brand transition-colors">Add skills (optional)</button>
                        <div className="text-[11px] text-text-muted mt-0.5">Give agents specialized capabilities</div>
                      </div>
                    </li>
                  </ol>
                </div>
              </div>
            </section>
          )}

          {/* Activity */}
          <ActivityFeed entries={activityEntries} loading={loadingActivity} />
        </div>

        {/* ── Column 2: project sidebar ───────────────────── */}
        <div className="space-y-4">
          <SummarySidebarSection title="Instructions">
            <div className="flex items-start justify-between gap-3">
              <p className="text-[12px] leading-relaxed text-text-muted">{instructionStatus}</p>
              <span className={`shrink-0 rounded-full px-2 py-0.5 text-[10px] font-semibold ${hasInstructionFiles ? "bg-success/10 text-success border border-success/20" : "bg-warning/10 text-warning border border-warning/20"}`}>
                {hasInstructionFiles ? "Set" : "Missing"}
              </span>
            </div>
            <button
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
              <div className="space-y-2">
                {recentDocsLinks.map(([key, entry]) => (
                  <button
                    key={key}
                    onClick={handleExternalLinkClick(entry.path)}
                    className="flex w-full items-start gap-2 rounded-md px-2 py-1.5 text-left transition-colors hover:bg-bg-sidebar"
                  >
                    <ExternalLink size={11} className="mt-0.5 shrink-0 text-text-muted" />
                    <span className="min-w-0 text-[12px] text-text-base truncate">{entry.summary || key}</span>
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
                <div className="flex items-end gap-2">
                  <span className="text-[24px] font-semibold leading-none tabular-nums text-text-base">{memoryCount}</span>
                  <span className="pb-0.5 text-[12px] text-text-muted">{memoryCount === 1 ? "memory" : "memories"}</span>
                </div>
                <p className="text-[12px] text-text-muted">
                  {memoryCount === 0 ? "No stored memories for this project yet." : "Stored memories are available for connected agents."}
                </p>
                <button
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
