// Extracted verbatim from ProjectEditor.tsx (Phase 2E — behavior-preserving).

import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertCircle, Plus, RefreshCw, Search, Sparkles, X } from "lucide-react";
import { McpSelector } from "../../../../../components/McpSelector";
import { McpAddButton } from "../McpAddButton";
import type { AgentInfo, ProfileLockMap, Project, ProjectRecommendation } from "../../types";

interface McpServersPanelProps {
  project: Project;
  availableAgents: AgentInfo[];
  availableMcpServers: string[];
  /** Server → providing profile, for servers an attached profile provides. */
  profileLocks: ProfileLockMap;
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
  /** Server name → id of the project it is limited to, for project servers. */
  mcpServerScopes: Record<string, string>;
  /** local_key of the open project, or null while it is being created. */
  projectKey: string | null;
  /** Reload the server list after this panel created a project server. */
  onMcpServersChanged: () => void | Promise<void>;
}

/**
 * The config for a new project server, from what the user typed in one box.
 * A web address makes an HTTP server. Anything else is a command line,
 * split on spaces into the command and its arguments. Quoted arguments are
 * not supported here; the full editor handles them.
 */
export function projectServerConfig(commandOrUrl: string): Record<string, unknown> | null {
  const trimmed = commandOrUrl.trim();
  if (!trimmed) return null;
  if (/^https?:\/\//i.test(trimmed)) {
    return { type: "http", url: trimmed };
  }
  const [command, ...args] = trimmed.split(/\s+/);
  return args.length > 0 ? { type: "stdio", command, args } : { type: "stdio", command };
}

function NewProjectServerForm({
  projectKey,
  onCreate,
  onCancel,
}: {
  projectKey: string;
  onCreate: (name: string) => Promise<void>;
  onCancel: () => void;
}) {
  const [name, setName] = useState("");
  const [commandOrUrl, setCommandOrUrl] = useState("");
  const [saving, setSaving] = useState(false);
  // Set once the server exists, so a failed attach cannot create it twice.
  const [created, setCreated] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const config = projectServerConfig(commandOrUrl);
  const canCreate = name.trim() !== "" && config !== null && !saving && !created;

  const handleCreate = async () => {
    if (!canCreate) return;
    setSaving(true);
    setError(null);
    try {
      await invoke("create_project_mcp_server", {
        projectName: projectKey,
        name: name.trim(),
        data: JSON.stringify(config),
      });
    } catch (err: unknown) {
      setError(String(err));
      setSaving(false);
      return;
    }
    setCreated(true);
    try {
      await onCreate(name.trim());
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="mb-3 bg-bg-input border border-brand/30 rounded-lg px-4 py-3 space-y-2.5">
      <p className="text-[12px] text-text-muted leading-relaxed">
        A project server works like any library server, but only this project can see it.
        Add environment variables and other settings in the full editor afterwards.
      </p>
      <div className="flex gap-2">
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Name, for example local-db"
          autoFocus
          className="w-48 bg-bg-sidebar border border-border-strong/40 focus:border-brand rounded px-2 py-1.5 text-[12px] text-text-base placeholder-text-muted/50 outline-none"
        />
        <input
          type="text"
          value={commandOrUrl}
          onChange={(e) => setCommandOrUrl(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void handleCreate();
            if (e.key === "Escape") onCancel();
          }}
          placeholder="Command, for example npx -y my-server, or a URL"
          className="flex-1 min-w-0 bg-bg-sidebar border border-border-strong/40 focus:border-brand rounded px-2 py-1.5 text-[12px] font-mono text-text-base placeholder-text-muted/50 outline-none"
        />
      </div>
      {error && (
        <p className="text-[12px] text-danger flex items-center gap-1">
          <AlertCircle size={11} /> {error}
        </p>
      )}
      <div className="flex justify-end gap-2">
        <button
          onClick={onCancel}
          className="px-2.5 py-1 text-[12px] text-text-muted hover:text-text-base transition-colors"
        >
          {created ? "Close" : "Cancel"}
        </button>
        <button
          onClick={() => void handleCreate()}
          disabled={!canCreate}
          className="px-2.5 py-1 text-[12px] font-medium text-white bg-brand hover:bg-brand-hover rounded disabled:opacity-40 transition-colors"
        >
          {saving ? "Creating…" : "Create and add"}
        </button>
      </div>
    </div>
  );
}

export function McpServersPanel({
  project,
  availableAgents,
  availableMcpServers,
  profileLocks,
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
  mcpServerScopes,
  projectKey,
  onMcpServersChanged,
}: McpServersPanelProps) {
  const [creatingProjectServer, setCreatingProjectServer] = useState(false);
  const projectId = project.id ?? "";
  // Servers this checkout has not accepted are not this project's here.
  const pending = project.pending_project_mcp_servers ?? [];
  const projectServers = Object.entries(mcpServerScopes)
    .filter(([name, owner]) => projectId !== "" && owner === projectId && !pending.includes(name))
    .map(([name]) => name);

  const handleProjectServerCreated = async (name: string) => {
    await onMcpServersChanged();
    const added = await addItem("mcp_servers", name);
    if (!added) {
      throw new Error(`The server "${name}" was created, but it could not be added to this project. Add it from the library.`);
    }
    setCreatingProjectServer(false);
  };

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
        lockedLabel={(s) => profileLocks.mcp_servers[s]}
        showRemoveButtonAlways
        disableAdd={allNoMcp}
        emptyMessage={allNoMcp ? "Add other agent tools to enable MCP server syncing." : "No MCP servers attached."}
        onNavigateToMcpServer={onNavigateToMcpServer}
        projectServers={projectServers}
        headerActions={
          projectId !== "" && projectKey && !creatingProjectServer ? (
            <button
              onClick={() => setCreatingProjectServer(true)}
              className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium"
              title="Create a server that only this project can see"
            >
              <Plus size={12} /> New project server
            </button>
          ) : null
        }
        headerPanel={
          creatingProjectServer && projectKey ? (
            <NewProjectServerForm
              projectKey={projectKey}
              onCreate={handleProjectServerCreated}
              onCancel={() => setCreatingProjectServer(false)}
            />
          ) : null
        }
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
