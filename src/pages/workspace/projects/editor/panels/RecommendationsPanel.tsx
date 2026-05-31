// Extracted verbatim from ProjectEditor.tsx (Phase 2E — behavior-preserving).

import { invoke } from "@tauri-apps/api/core";
import { AlertCircle, ArrowRight, Code, RefreshCw, Search, Server, Sparkles, X } from "lucide-react";
import { relativeTime } from "../../helpers";
import type { Project, ProjectRecommendation } from "../../types";
import { SkillAddButton } from "../SkillAddButton";
import { McpAddButton } from "../McpAddButton";

type ProjectTabId =
  | "summary" | "agents" | "commands" | "hooks" | "custom_agents" | "skills"
  | "mcp_servers" | "groups" | "project_file" | "rules" | "context"
  | "docs_files" | "docs_links" | "docs_notes" | "memory" | "activity"
  | "recommendations" | "tools" | "settings";

interface RecommendationsPanelProps {
  project: Project;
  normalRecs: ProjectRecommendation[];
  aiSkillsRollupCount: number;
  aiMcpRollupCount: number;
  recsDisplayCount: number;
  aiRecsLoading: boolean;
  aiRecsLastRunAt: string | null;
  handleUpdateAiRecommendations: () => void | Promise<void>;
  handleDismissRecommendation: (id: number) => void | Promise<void>;
  removeRecommendation: (id: number) => void;
  addItem: (field: "skills" | "mcp_servers" | "providers" | "agents", value: string) => Promise<boolean>;
  selectTab: (tab: ProjectTabId) => void;
  onNavigateToSkillStore?: (skillId: string) => void;
  onNavigateToSkillStoreWithResult?: (result: { id: string; name: string; source: string; installs: number }) => void;
  onNavigateToDiscoverMcp?: (slug: string) => void;
}

export function RecommendationsPanel({
  project,
  normalRecs,
  aiSkillsRollupCount,
  aiMcpRollupCount,
  recsDisplayCount,
  aiRecsLoading,
  aiRecsLastRunAt,
  handleUpdateAiRecommendations,
  handleDismissRecommendation,
  removeRecommendation,
  addItem,
  selectTab,
  onNavigateToSkillStore,
  onNavigateToSkillStoreWithResult,
  onNavigateToDiscoverMcp,
}: RecommendationsPanelProps) {
  return (
    <section className="space-y-3">
      {/* Header row */}
      <div className="flex items-center gap-2">
        <Sparkles size={13} className="text-text-muted" />
        <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Recommendations</span>
        {recsDisplayCount > 0 && (
          <span className="text-[10px] font-semibold px-1.5 py-0.5 rounded-full bg-warning/15 text-warning border border-warning/20 leading-none">
            {recsDisplayCount}
          </span>
        )}
        <div className="flex-1" />
        {/* Last-run metadata */}
        {aiRecsLastRunAt && !aiRecsLoading && (
          <span className="text-[11px] text-text-muted">
            Updated {relativeTime(aiRecsLastRunAt)}
          </span>
        )}
        {/* Manual trigger button */}
        <button
          onClick={handleUpdateAiRecommendations}
          disabled={aiRecsLoading}
          className="flex items-center gap-1.5 px-2.5 py-1.5 text-[11px] font-medium text-text-muted hover:text-text-base border border-border-strong/50 rounded-md disabled:opacity-40 transition-colors"
          title="Re-run AI analysis to refresh recommendations"
        >
          <RefreshCw size={11} className={aiRecsLoading ? "animate-spin" : ""} />
          {aiRecsLoading ? "Analysing…" : "Update recommendations"}
        </button>
      </div>

      {/* AI loading state */}
      {aiRecsLoading && (
        <div className="bg-bg-input border border-border-strong/40 rounded-lg px-4 py-6 flex items-center gap-3">
          <RefreshCw size={14} className="text-brand animate-spin flex-shrink-0" />
          <div>
            <p className="text-[13px] font-medium text-text-base">Analysing project…</p>
            <p className="text-[12px] text-text-muted">The AI is reviewing your configuration and searching for relevant skills and MCP servers.</p>
          </div>
        </div>
      )}

      {!aiRecsLoading && recsDisplayCount === 0 ? (
        <div className="bg-bg-input border border-border-strong/40 rounded-lg px-4 py-10 text-center">
          <Sparkles size={18} className="text-text-muted mx-auto mb-2" />
          <p className="text-[13px] font-medium text-text-base mb-1">No recommendations at this time</p>
          <p className="text-[12px] text-text-muted">Click "Update recommendations" to run an AI analysis of your project configuration.</p>
        </div>
      ) : !aiRecsLoading ? (
        <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
          {/* AI skill rollup card */}
          {aiSkillsRollupCount > 0 && (
            <div className="flex items-start gap-3 px-4 py-4 hover:bg-surface-hover transition-colors">
              <Sparkles size={14} className="flex-shrink-0 mt-0.5 text-brand" />
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2 mb-1">
                  <span className="text-[13px] font-semibold text-text-base">
                    {aiSkillsRollupCount} skill{aiSkillsRollupCount !== 1 ? "s" : ""} recommended
                  </span>
                  <span className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-brand/10 text-brand border border-brand/20 leading-none flex items-center gap-1">
                    <Sparkles size={8} /> AI
                  </span>
                </div>
                <p className="text-[12px] text-text-muted leading-relaxed">
                  The AI has identified {aiSkillsRollupCount} skill{aiSkillsRollupCount !== 1 ? "s" : ""} that may benefit this project. Review and add them from the Skills tab.
                </p>
                <button
                  onClick={() => selectTab("skills")}
                  className="mt-2 text-[11px] text-brand hover:text-brand-hover transition-colors font-medium flex items-center gap-1"
                >
                  <Code size={10} /> Go to Skills tab <ArrowRight size={10} />
                </button>
              </div>
            </div>
          )}

          {/* AI MCP rollup card */}
          {aiMcpRollupCount > 0 && (
            <div className="flex items-start gap-3 px-4 py-4 hover:bg-surface-hover transition-colors">
              <Sparkles size={14} className="flex-shrink-0 mt-0.5 text-brand" />
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2 mb-1">
                  <span className="text-[13px] font-semibold text-text-base">
                    {aiMcpRollupCount} MCP server{aiMcpRollupCount !== 1 ? "s" : ""} recommended
                  </span>
                  <span className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-brand/10 text-brand border border-brand/20 leading-none flex items-center gap-1">
                    <Sparkles size={8} /> AI
                  </span>
                </div>
                <p className="text-[12px] text-text-muted leading-relaxed">
                  The AI has identified {aiMcpRollupCount} MCP server{aiMcpRollupCount !== 1 ? "s" : ""} that may benefit this project. Review and add them from the MCP Servers tab.
                </p>
                <button
                  onClick={() => selectTab("mcp_servers")}
                  className="mt-2 text-[11px] text-brand hover:text-brand-hover transition-colors font-medium flex items-center gap-1"
                >
                  <Server size={10} /> Go to MCP Servers tab <ArrowRight size={10} />
                </button>
              </div>
            </div>
          )}

          {/* Normal (non-AI-suggestion) recommendation cards */}
          {normalRecs.map((rec) => (
            <div key={rec.id} className="flex items-start gap-3 px-4 py-4 group hover:bg-surface-hover transition-colors">
              <AlertCircle
                size={14}
                className={`flex-shrink-0 mt-0.5 ${rec.priority === "high" ? "text-warning" : "text-text-muted"}`}
              />
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2 mb-1">
                  <span className="text-[13px] font-semibold text-text-base">{rec.title}</span>
                  {rec.priority === "high" && (
                    <span className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-warning/15 text-warning border border-warning/20 leading-none">
                      Important
                    </span>
                  )}
                  {rec.source === "automatic-ai" && (
                    <span className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-brand/10 text-brand border border-brand/20 leading-none flex items-center gap-1">
                      <Sparkles size={8} />
                      AI
                    </span>
                  )}
                </div>
                <p className="text-[12px] text-text-muted leading-relaxed">{rec.body}</p>
                {(rec.kind === "skill" || rec.kind === "mcp_server") && (
                  <div className="mt-2 flex items-center gap-2">
                    {rec.kind === "skill" && (onNavigateToSkillStoreWithResult || onNavigateToSkillStore) && (
                      <button
                        onClick={() => {
                          if (rec.metadata && onNavigateToSkillStoreWithResult) {
                            try {
                              const meta = JSON.parse(rec.metadata) as { id: string; name: string; source: string; installs: number };
                              if (meta.id && meta.name && meta.source) {
                                onNavigateToSkillStoreWithResult(meta);
                                return;
                              }
                            } catch {
                              // fall through to plain query
                            }
                          }
                          onNavigateToSkillStore?.(rec.title);
                        }}
                        className="text-[11px] font-medium text-text-muted hover:text-text-base border border-border-strong/50 rounded px-2 py-1 transition-colors flex items-center gap-1"
                      >
                        <Search size={10} /> View
                      </button>
                    )}
                    {rec.kind === "skill" && (
                      <SkillAddButton
                        rec={rec}
                        alreadyAdded={project.skills.includes(rec.title)}
                        onAdd={async (skillName) => {
                          const added = await addItem("skills", skillName);
                          if (!added) return false;
                          try {
                            await invoke("action_recommendation", { id: rec.id });
                            removeRecommendation(rec.id);
                          } catch (err) {
                            console.error("Failed to mark recommendation as actioned:", err);
                          }
                          return true;
                        }}
                      />
                    )}

                    {rec.kind === "mcp_server" && onNavigateToDiscoverMcp && (
                      <button
                        onClick={() => {
                          if (rec.metadata) {
                            try {
                              const meta = JSON.parse(rec.metadata) as { slug?: string };
                              if (meta.slug) {
                                onNavigateToDiscoverMcp(meta.slug);
                                return;
                              }
                            } catch {
                              // fall through to title
                            }
                          }
                          onNavigateToDiscoverMcp(rec.title);
                        }}
                        className="text-[11px] font-medium text-text-muted hover:text-text-base border border-border-strong/50 rounded px-2 py-1 transition-colors flex items-center gap-1"
                      >
                        <Search size={10} /> View
                      </button>
                    )}
                    {rec.kind === "mcp_server" && (
                      <McpAddButton
                        rec={rec}
                        alreadyAdded={project.mcp_servers.includes(rec.title)}
                        onAdd={async (serverName) => {
                          const added = await addItem("mcp_servers", serverName);
                          if (!added) return false;
                          try {
                            await invoke("action_recommendation", { id: rec.id });
                            removeRecommendation(rec.id);
                          } catch (err) {
                            console.error("Failed to mark recommendation as actioned:", err);
                          }
                          return true;
                        }}
                      />
                    )}
                  </div>
                )}
                {rec.kind === "rule" && (
                  <button
                    onClick={() => selectTab("project_file")}
                    className="mt-2 text-[11px] text-brand hover:text-brand-hover transition-colors font-medium flex items-center gap-1"
                  >
                    Open Project File <ArrowRight size={10} />
                  </button>
                )}
                {rec.kind === "project_file" && (
                  <button
                    onClick={() => selectTab("project_file")}
                    className="mt-2 text-[11px] text-brand hover:text-brand-hover transition-colors font-medium flex items-center gap-1"
                  >
                    Create Instructions File <ArrowRight size={10} />
                  </button>
                )}
              </div>
              <button
                onClick={() => handleDismissRecommendation(rec.id)}
                className="flex-shrink-0 p-1 text-text-muted hover:text-text-base transition-colors opacity-0 group-hover:opacity-100"
                title="Dismiss"
              >
                <X size={13} />
              </button>
            </div>
          ))}
        </div>
      ) : null}
    </section>
  );
}
