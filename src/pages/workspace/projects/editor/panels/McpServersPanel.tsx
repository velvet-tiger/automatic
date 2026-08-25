// Extracted verbatim from ProjectEditor.tsx (Phase 2E — behavior-preserving).

import { invoke } from "@tauri-apps/api/core";
import { AlertCircle, RefreshCw, Search, Sparkles, X } from "lucide-react";
import { McpSelector } from "../../../../../components/McpSelector";
import { McpAddButton } from "../McpAddButton";
import type { AgentInfo, Project, ProjectRecommendation } from "../../types";

interface McpServersPanelProps {
  project: Project;
  availableAgents: AgentInfo[];
  availableMcpServers: string[];
  addItem: (field: "skills" | "mcp_servers" | "providers" | "agents", value: string) => Promise<boolean>;
  removeItem: (field: "skills" | "mcp_servers" | "providers" | "agents", index: number) => void;
  isMcpServerEnabled: (server: string) => boolean;
  toggleMcpServerEnabled: (server: string, enabled: boolean) => void | Promise<void>;
  aiMcpSuggestions: ProjectRecommendation[];
  aiMcpLoading: boolean;
  handleSuggestMcpServers: () => void | Promise<void>;
  removeRecommendation: (id: number) => void;
  onNavigateToMcpServer?: (serverName: string) => void;
  onNavigateToDiscoverMcp?: (slug: string) => void;
}

export function McpServersPanel({
  project,
  availableAgents,
  availableMcpServers,
  addItem,
  removeItem,
  isMcpServerEnabled,
  toggleMcpServerEnabled,
  aiMcpSuggestions,
  aiMcpLoading,
  handleSuggestMcpServers,
  removeRecommendation,
  onNavigateToMcpServer,
  onNavigateToDiscoverMcp,
}: McpServersPanelProps) {
  // "No MCP" means Automatic cannot write the agent's MCP config
  // (capabilities.mcp_servers is false) — not merely that the agent carries an
  // informational mcp_note (Z Code and OpenCode have notes but are writable).
  const noMcpAgents = availableAgents.filter(
    (a) =>
      project.agents.includes(a.id) &&
      (a.capabilities ? !a.capabilities.mcp_servers : Boolean(a.mcp_note))
  );
  const allNoMcp = noMcpAgents.length > 0 && noMcpAgents.length === project.agents.length;
  const someNoMcp = noMcpAgents.length > 0 && !allNoMcp;

  return (
    <section>
      {/* All agents require manual MCP setup */}
      {allNoMcp && (
        <div className="mb-4 flex items-start gap-3 px-4 py-3 bg-bg-input border border-border-strong rounded-lg">
          <AlertCircle size={15} className="text-text-muted flex-shrink-0 mt-0.5" />
          <div>
            <p className="text-[13px] font-medium text-text-base mb-0.5">MCP not configurable via Automatic</p>
            {noMcpAgents.map((a) => (
              <p key={a.id} className="text-[12px] text-text-muted leading-relaxed">{a.mcp_note}</p>
            ))}
          </div>
        </div>
      )}

      {/* Some agents require manual MCP setup */}
      {someNoMcp && (
        <div className="mb-4 flex items-start gap-3 px-4 py-3 bg-warning/8 border border-warning/30 rounded-lg">
          <AlertCircle size={15} className="text-warning flex-shrink-0 mt-0.5" />
          <div>
            <p className="text-[13px] font-medium text-warning mb-0.5">Some agents require manual MCP setup</p>
            {noMcpAgents.map((a) => (
              <p key={a.id} className="text-[12px] text-warning/80 leading-relaxed">
                <span className="font-medium">{a.label}:</span> {a.mcp_note}
              </p>
            ))}
          </div>
        </div>
      )}

      <McpSelector
        servers={project.mcp_servers}
        availableServers={availableMcpServers}
        onAdd={(s) => addItem("mcp_servers", s)}
        onRemove={(i) => removeItem("mcp_servers", i)}
        isServerEnabled={isMcpServerEnabled}
        onToggleEnabled={toggleMcpServerEnabled}
        showRemoveButtonAlways
        disableAdd={allNoMcp}
        emptyMessage={allNoMcp ? "Add other agent tools to enable MCP server syncing." : "No MCP servers attached."}
        onNavigateToMcpServer={onNavigateToMcpServer}
      />

      {/* ── AI MCP suggestions ─────────────────────────────── */}
      {!allNoMcp && (
        <div className="mt-4 space-y-2">
          <div className="flex items-center gap-2">
            <Sparkles size={12} className="text-text-muted" />
            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">AI Suggestions</span>
            {aiMcpSuggestions.length > 0 && !aiMcpLoading && (
              <span className="text-[10px] font-semibold px-1.5 py-0.5 rounded-full bg-brand/10 text-brand border border-brand/20 leading-none">
                {aiMcpSuggestions.length}
              </span>
            )}
            <div className="flex-1" />
            <button
              onClick={handleSuggestMcpServers}
              disabled={aiMcpLoading}
              className="flex items-center gap-1.5 px-2.5 py-1.5 text-[11px] font-medium text-text-muted hover:text-text-base border border-border-strong/50 rounded-md disabled:opacity-40 transition-colors"
              title="Ask AI to suggest MCP servers based on this project's configuration"
            >
              <Sparkles size={11} className={aiMcpLoading ? "animate-pulse" : ""} />
              {aiMcpLoading ? "Analysing…" : "Suggest MCP servers"}
            </button>
          </div>

          {aiMcpLoading && (
            <div className="bg-bg-input border border-border-strong/40 rounded-lg px-4 py-4 flex items-center gap-3">
              <RefreshCw size={13} className="text-brand animate-spin flex-shrink-0" />
              <p className="text-[12px] text-text-muted">Searching the MCP server catalogue…</p>
            </div>
          )}

          {!aiMcpLoading && aiMcpSuggestions.length === 0 && (
            <p className="text-[12px] text-text-muted">
              Click "Suggest MCP servers" to get AI-powered recommendations based on this project.
            </p>
          )}

          {!aiMcpLoading && aiMcpSuggestions.length > 0 && (
            <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
              {aiMcpSuggestions.map((rec) => (
                <div key={rec.id} className="flex items-start gap-3 px-4 py-3 group hover:bg-surface-hover transition-colors">
                  <Sparkles size={13} className="flex-shrink-0 mt-0.5 text-brand" />
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 mb-0.5">
                      <span className="text-[13px] font-semibold text-text-base font-mono">{rec.title}</span>
                      {rec.priority === "high" && (
                        <span className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-warning/15 text-warning border border-warning/20 leading-none">High</span>
                      )}
                    </div>
                    <p className="text-[12px] text-text-muted leading-relaxed">{rec.body}</p>
                    <div className="flex items-center gap-2 mt-2">
                      {onNavigateToDiscoverMcp && (
                        <button
                          onClick={() => onNavigateToDiscoverMcp(rec.title)}
                          className="text-[11px] font-medium text-text-muted hover:text-text-base border border-border-strong/50 rounded px-2 py-1 transition-colors flex items-center gap-1"
                        >
                          <Search size={10} /> View
                        </button>
                      )}
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
                    </div>
                  </div>
                  <button
                    onClick={async () => {
                      await invoke("dismiss_recommendation", { id: rec.id });
                      removeRecommendation(rec.id);
                    }}
                    className="flex-shrink-0 p-1 text-text-muted hover:text-text-base transition-colors opacity-0 group-hover:opacity-100"
                    title="Dismiss"
                  >
                    <X size={12} />
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </section>
  );
}
