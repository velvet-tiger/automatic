import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Bot, FolderOpen, AlertCircle, ArrowRight, CheckCircle2, XCircle, Settings2 } from "lucide-react";
import { ICONS } from "../../lib/icons";
import { AgentIcon } from "../../components/AgentIcon";
import type { AgentCapabilities, AgentOptions } from "../../components/AgentSelector";

interface AgentProject {
  name: string;
  directory: string;
}

/** Describes a single toggleable default option for a particular agent. */
interface AgentOptionDef {
  /** Key in AgentOptions */
  key: keyof AgentOptions;
  label: string;
  description: string;
  /** Hard-coded default value when not set in settings */
  hardDefault: boolean;
}

/**
 * Static catalogue of configurable default options per agent id.
 * Keyed by agent id.  Empty list = no configurable options.
 */
const AGENT_OPTION_DEFS: Record<string, AgentOptionDef[]> = {
  claude: [
    {
      key: "claude_rules_in_dot_claude",
      label: "Store rules in .claude/rules/",
      description:
        "Write each rule as an individual Markdown file under .claude/rules/ " +
        "instead of injecting them inline into CLAUDE.md. " +
        "Claude Code loads these files automatically every session.",
      hardDefault: true,
    },
  ],
  cursor: [
    {
      key: "cursor_rules_in_dot_cursor",
      label: "Store rules in .cursor/rules/",
      description:
        "Write each rule as an individual .mdc file under .cursor/rules/ " +
        "instead of injecting them inline into AGENTS.md. " +
        "This is Cursor's native project-rule format.",
      hardDefault: false,
    },
  ],
};

interface AgentWithProjects {
  id: string;
  label: string;
  description: string;
  project_file: string;
  capabilities: AgentCapabilities;
  mcp_note: string | null;
  projects: AgentProject[];
}

interface CapabilityRowProps {
  label: string;
  description: string;
  unsupportedDescription?: string;
  supported: boolean;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function CapabilityRow({ label, description, unsupportedDescription, supported }: CapabilityRowProps) {
  const shownDescription = !supported && unsupportedDescription ? unsupportedDescription : description;
  return (
    <div className="flex items-center gap-3 px-3 py-2.5 bg-bg-input rounded-md border border-border-strong/40">
      {supported ? (
        <CheckCircle2 size={14} className="text-green-500 flex-shrink-0" />
      ) : (
        <XCircle size={14} className="text-text-muted flex-shrink-0" />
      )}
      <div className="flex-1 min-w-0">
        <span className="text-[13px] text-text-base">{label}</span>
        <span className="text-[11px] text-text-muted ml-2">{shownDescription}</span>
      </div>
    </div>
  );
}

interface ProvidersProps {
  onNavigateToProject?: (projectName: string) => void;
}

type DetailTab = "details" | "management" | "projects";

const DETAIL_TABS: { id: DetailTab; label: string }[] = [
  { id: "details", label: "Details" },
  { id: "management", label: "Management" },
  { id: "projects", label: "Projects" },
];

export default function Providers({ onNavigateToProject }: ProvidersProps = {}) {
  const LAST_AGENT_KEY = "automatic.agents.selected";
  const [agents, setAgents] = useState<AgentWithProjects[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(() => {
    // Migrate legacy "nexus." key
    const legacy = localStorage.getItem("nexus.agents.selected");
    if (legacy) {
      localStorage.setItem(LAST_AGENT_KEY, legacy);
      localStorage.removeItem("nexus.agents.selected");
      return legacy;
    }
    return localStorage.getItem(LAST_AGENT_KEY);
  });
  const [detailTab, setDetailTab] = useState<DetailTab>("details");
  const [error, setError] = useState<string | null>(null);
  /** Default agent options loaded from settings — keyed by agent id */
  const [defaultOptions, setDefaultOptions] = useState<Record<string, AgentOptions>>({});
  /** OpenCode: clear archived sessions */
  const [clearCacheStatus, setClearCacheStatus] = useState<"idle" | "running" | "done" | "error">("idle");
  const [clearCacheResult, setClearCacheResult] = useState<{ sessions_deleted: number; storage_entries_removed: number; bytes_reclaimed: number } | null>(null);
  const [clearCacheError, setClearCacheError] = useState("");
  /** OpenCode: clean snapshot storage */
  const [cleanSnapshotsStatus, setCleanSnapshotsStatus] = useState<"idle" | "running" | "done" | "error">("idle");
  const [cleanSnapshotsResult, setCleanSnapshotsResult] = useState<{ repos_gced: number; orphans_removed: number; tmp_pack_files_removed: number; bytes_freed: number } | null>(null);
  const [cleanSnapshotsError, setCleanSnapshotsError] = useState("");

  useEffect(() => {
    loadAgents();
    loadDefaults();
  }, []);

  useEffect(() => {
    // Auto-select first agent if none selected or selection is invalid
    if (agents.length > 0 && (!selectedId || !agents.find((a) => a.id === selectedId))) {
      const first = agents[0].id;
      setSelectedId(first);
      localStorage.setItem(LAST_AGENT_KEY, first);
    }
  }, [agents]);

  const loadAgents = async () => {
    try {
      const raw: string = await invoke("list_agents_with_projects");
      const parsed: AgentWithProjects[] = JSON.parse(raw);
      parsed.sort((a, b) => {
        const aCount = a.projects.length;
        const bCount = b.projects.length;
        if (aCount > 0 && bCount > 0) {
          if (bCount !== aCount) return bCount - aCount;
          return a.label.localeCompare(b.label);
        }
        if (aCount > 0) return -1;
        if (bCount > 0) return 1;
        return a.label.localeCompare(b.label);
      });
      setAgents(parsed);
      setError(null);
    } catch (err: any) {
      setError(`Failed to load agents: ${err}`);
    }
  };

  const loadDefaults = async () => {
    try {
      const raw: any = await invoke("read_settings");
      setDefaultOptions(raw.default_agent_options ?? {});
    } catch {
      // Non-fatal — fall back to hard defaults
    }
  };

  const setDefaultOption = async (agentId: string, key: keyof AgentOptions, value: boolean) => {
    try {
      const raw: any = await invoke("read_settings");
      const existing: AgentOptions = {
        claude_rules_in_dot_claude: true,
        cursor_rules_in_dot_cursor: false,
        ...(raw.default_agent_options?.[agentId] ?? {}),
      };
      const updated = {
        ...raw,
        default_agent_options: {
          ...(raw.default_agent_options ?? {}),
          [agentId]: { ...existing, [key]: value },
        },
      };
      await invoke("write_settings", { settings: updated });
      setDefaultOptions(updated.default_agent_options);
    } catch (err: any) {
      setError(`Failed to save default: ${err}`);
    }
  };

  const clearOpenCodeCache = async () => {
    setClearCacheStatus("running");
    setClearCacheResult(null);
    setClearCacheError("");
    try {
      const result = await invoke<{ sessions_deleted: number; storage_entries_removed: number; bytes_reclaimed: number }>("clear_opencode_cache");
      setClearCacheResult(result);
      setClearCacheStatus("done");
      setTimeout(() => setClearCacheStatus("idle"), 6000);
    } catch (e) {
      setClearCacheError(String(e));
      setClearCacheStatus("error");
    }
  };

  const cleanOpenCodeSnapshots = async () => {
    setCleanSnapshotsStatus("running");
    setCleanSnapshotsResult(null);
    setCleanSnapshotsError("");
    try {
      const result = await invoke<{ repos_gced: number; orphans_removed: number; tmp_pack_files_removed: number; bytes_freed: number }>("clean_opencode_snapshots");
      setCleanSnapshotsResult(result);
      setCleanSnapshotsStatus("done");
      setTimeout(() => setCleanSnapshotsStatus("idle"), 8000);
    } catch (e) {
      setCleanSnapshotsError(String(e));
      setCleanSnapshotsStatus("error");
    }
  };

  const selected = agents.find((a) => a.id === selectedId) || null;

  const selectAgent = (id: string) => {
    setSelectedId(id);
    localStorage.setItem(LAST_AGENT_KEY, id);
  };

  return (
    <div className="flex h-full w-full bg-bg-base">
      {/* Left sidebar - agent list */}
      <div className="w-64 flex-shrink-0 flex flex-col border-r border-border-strong/40 bg-bg-input/50">
        <div className="h-11 px-4 border-b border-border-strong/40 flex items-center bg-bg-base/30">
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
            Providers
          </span>
        </div>

        <div className="flex-1 overflow-y-auto py-2 custom-scrollbar">
          {agents.length === 0 ? (
            <div className="px-4 py-3 text-[13px] text-text-muted text-center">
              No agents registered.
            </div>
          ) : (
            <ul className="space-y-1 px-2">
              {agents.map((agent) => {
                const isActive = selectedId === agent.id;
                return (
                  <li key={agent.id}>
                    <button
                      onClick={() => selectAgent(agent.id)}
                      className={`w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-left transition-colors ${
                        isActive
                          ? "bg-bg-sidebar text-text-base"
                          : "text-text-muted hover:bg-bg-sidebar/60 hover:text-text-base"
                      }`}
                    >
                      <AgentIcon agentId={agent.id} size={20} />
                      <div className="flex-1 min-w-0">
                        <div className={`text-[13px] font-medium truncate ${isActive ? "text-text-base" : "text-text-base"}`}>
                          {agent.label}
                        </div>
                        {agent.projects.length > 0 && (
                          <div className="text-[11px] text-text-muted mt-0.5">
                            {agent.projects.length} project{agent.projects.length !== 1 ? "s" : ""}
                          </div>
                        )}
                      </div>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </div>

      {/* Right area - agent detail */}
      <div className="flex-1 flex flex-col min-w-0 bg-bg-base">
        {error && (
          <div className="bg-red-500/10 text-red-400 p-3 text-[13px] border-b border-red-500/20 flex items-center justify-between">
            {error}
            <button onClick={() => setError(null)} className="text-red-400 hover:text-red-300">
              &times;
            </button>
          </div>
        )}

{selected ? (
          <div className="flex-1 flex flex-col h-full">
            {/* Header */}
            <div className="h-11 px-6 border-b border-border-strong/40 flex items-center gap-3">
              <AgentIcon agentId={selected.id} size={16} />
              <h3 className="text-[14px] font-medium text-text-base">
                {selected.label}
              </h3>
            </div>

            {/* Tab Bar */}
            <div className="h-10 px-6 border-b border-border-strong/40 flex items-center gap-6">
              {DETAIL_TABS.map((tab) => {
                const isActive = detailTab === tab.id;
                return (
                  <button
                    key={tab.id}
                    onClick={() => setDetailTab(tab.id)}
                    className={`text-[12px] font-medium transition-colors pb-2 -mb-[10px] border-b-2 ${
                      isActive
                        ? "text-brand border-brand"
                        : "text-text-muted border-transparent hover:text-text-base"
                    }`}
                  >
                    {tab.label}
                  </button>
                );
              })}
            </div>

            {/* Body */}
            <div className="flex-1 overflow-y-auto p-6 custom-scrollbar">
              <div className="max-w-2xl space-y-8">
                {/* Details Tab */}
                {detailTab === "details" && (
                  <>
                    {/* Agent Info */}
                    <section>
                      <label className="block text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-3">
                        Details
                      </label>
                      <div className="space-y-3">
                        <div className="flex items-start gap-3 px-3 py-2.5 bg-bg-input rounded-md border border-border-strong/40">
                          <span className="text-[11px] text-text-muted w-24 flex-shrink-0 pt-0.5">Config File</span>
                          <span className="text-[13px] text-text-base font-mono">{selected.description}</span>
                        </div>
                        <div className="flex items-start gap-3 px-3 py-2.5 bg-bg-input rounded-md border border-border-strong/40">
                          <span className="text-[11px] text-text-muted w-24 flex-shrink-0 pt-0.5">Project Instructions</span>
                          <span className="text-[13px] text-text-base font-mono">{selected.project_file}</span>
                        </div>
                      </div>
                    </section>

                    {/* Capabilities */}
                    <section>
                      <label className="block text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-3">
                        Capabilities
                      </label>
                      <div className="space-y-2">
                        <CapabilityRow
                          label="Skills"
                          description="Automatic can sync skills to this agent"
                          supported={selected.capabilities.skills}
                        />
                        <CapabilityRow
                          label="Instructions"
                          description="Reads a project instructions file"
                          supported={selected.capabilities.instructions}
                        />
                        <CapabilityRow
                          label="MCP Servers"
                          description="Automatic can write MCP server configuration"
                          unsupportedDescription="Automatic cannot write MCP server configuration"
                          supported={selected.capabilities.mcp_servers}
                        />
                        <CapabilityRow
                          label="Sub-Agents"
                          description="Automatic can sync sub-agents to this agent"
                          unsupportedDescription="This agent does not support sub-agents"
                          supported={selected.capabilities.agents}
                        />
                        <CapabilityRow
                          label="Commands"
                          description="Automatic can sync project commands to this agent"
                          unsupportedDescription="This agent does not support project-local commands"
                          supported={selected.capabilities.commands}
                        />
                        <CapabilityRow
                          label="Hooks"
                          description="Automatic can sync lifecycle hooks to this agent"
                          unsupportedDescription="This agent does not support lifecycle hooks"
                          supported={selected.capabilities.hooks}
                        />
                      </div>
                      {selected.mcp_note && (
                        <div className="flex items-start gap-3 px-3 py-3 bg-bg-input rounded-md border border-border-strong mt-3">
                          <AlertCircle size={14} className="text-text-muted flex-shrink-0 mt-0.5" />
                          <p className="text-[12px] text-text-muted leading-relaxed">{selected.mcp_note}</p>
                        </div>
                      )}
                    </section>
                  </>
                )}

                {/* Management Tab */}
                {detailTab === "management" && (
                  <>
                    {/* Default options */}
                    {(AGENT_OPTION_DEFS[selected.id]?.length ?? 0) > 0 && (
                      <section>
                        <label className="block text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-3 flex items-center gap-1.5">
                          <Settings2 size={12} className="text-text-muted" /> Default Options
                        </label>
                        <div className="divide-y divide-border-strong/20 border border-border-strong/40 rounded-lg overflow-hidden">
                          {AGENT_OPTION_DEFS[selected.id]!.map((opt) => {
                            const agentDefaults = defaultOptions[selected.id];
                            const value: boolean = agentDefaults
                              ? (agentDefaults[opt.key] as boolean)
                              : opt.hardDefault;
                            return (
                              <label
                                key={opt.key}
                                className="flex items-start gap-3 px-3 py-3 bg-bg-input cursor-pointer hover:bg-bg-sidebar/40 transition-colors"
                              >
                                <div className="flex-1 min-w-0">
                                  <div className="text-[13px] text-text-base font-medium leading-snug">
                                    {opt.label}
                                  </div>
                                  <div className="text-[11px] text-text-muted mt-0.5 leading-relaxed">
                                    {opt.description}
                                  </div>
                                </div>
                                <div className="flex-shrink-0 pt-0.5">
                                  <input
                                    type="checkbox"
                                    checked={value}
                                    onChange={(e) =>
                                      setDefaultOption(selected.id, opt.key, e.target.checked)
                                    }
                                    className="w-4 h-4 accent-brand cursor-pointer"
                                  />
                                </div>
                              </label>
                            );
                          })}
                        </div>
                        <p className="text-[11px] text-text-muted mt-2 leading-relaxed">
                          These defaults apply when a new project is created. Each project can
                          override them individually in its Providers tab.
                        </p>
                      </section>
                    )}

                    {/* OpenCode maintenance */}
                    {selected.id === "opencode" && (
                      <section>
                        <label className="block text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-3">
                          Maintenance
                        </label>

                        <div className="mb-4">
                          <div className="text-[13px] font-medium text-text-base mb-1">Archived Sessions</div>
                          <p className="text-[12px] text-text-muted mb-3 leading-relaxed">
                            Delete archived sessions and their messages from the OpenCode database to reclaim disk space.
                          </p>
                          {clearCacheStatus === "done" && clearCacheResult && clearCacheResult.sessions_deleted > 0 && (
                            <div className="mb-2 p-2.5 rounded-lg border border-success bg-success/10 text-[12px] text-success">
                              Cleared {clearCacheResult.sessions_deleted} archived {clearCacheResult.sessions_deleted === 1 ? "session" : "sessions"}
                              {clearCacheResult.storage_entries_removed > 0 && `, ${clearCacheResult.storage_entries_removed} storage ${clearCacheResult.storage_entries_removed === 1 ? "entry" : "entries"} removed`}
                              {clearCacheResult.bytes_reclaimed > 0 && ` — ${formatBytes(clearCacheResult.bytes_reclaimed)} reclaimed`}.
                            </div>
                          )}
                          {clearCacheStatus === "done" && clearCacheResult?.sessions_deleted === 0 && (
                            <div className="mb-2 p-2.5 rounded-lg border border-border-strong/40 bg-bg-input-dark text-[12px] text-text-muted">
                              No archived sessions found.
                            </div>
                          )}
                          {clearCacheStatus === "error" && clearCacheError && (
                            <div className="mb-2 p-2.5 rounded-lg border border-danger bg-danger/10 text-[12px] text-danger">
                              {clearCacheError}
                            </div>
                          )}
                          <button
                            onClick={clearOpenCodeCache}
                            disabled={clearCacheStatus === "running"}
                            className="px-3 py-1.5 rounded-lg border border-border-strong/40 bg-bg-input-dark text-[12px] text-text-base hover:border-border-strong hover:bg-surface-hover transition-all disabled:opacity-50 disabled:cursor-not-allowed"
                          >
                            {clearCacheStatus === "running" ? "Clearing…" : "Clear Archived Sessions"}
                          </button>
                        </div>

                        <div>
                          <div className="text-[13px] font-medium text-text-base mb-1">Snapshot Storage</div>
                          <p className="text-[12px] text-text-muted mb-3 leading-relaxed">
                            Run <span className="font-mono">git gc</span> on each project snapshot repo, remove orphaned repos for deleted projects, and delete stale <span className="font-mono">tmp_pack_*</span> files.
                          </p>
                          {cleanSnapshotsStatus === "done" && cleanSnapshotsResult && (
                            <div className="mb-2 p-2.5 rounded-lg border border-success bg-success/10 text-[12px] text-success">
                              {cleanSnapshotsResult.repos_gced > 0
                                ? `Compacted ${cleanSnapshotsResult.repos_gced} snapshot ${cleanSnapshotsResult.repos_gced === 1 ? "repo" : "repos"}`
                                : "No snapshot repos to compact"}
                              {cleanSnapshotsResult.orphans_removed > 0 && `, removed ${cleanSnapshotsResult.orphans_removed} orphaned ${cleanSnapshotsResult.orphans_removed === 1 ? "repo" : "repos"}`}
                              {cleanSnapshotsResult.tmp_pack_files_removed > 0 && `, deleted ${cleanSnapshotsResult.tmp_pack_files_removed} tmp_pack ${cleanSnapshotsResult.tmp_pack_files_removed === 1 ? "file" : "files"}`}
                              {cleanSnapshotsResult.bytes_freed > 0 && ` — ${formatBytes(cleanSnapshotsResult.bytes_freed)} freed`}.
                            </div>
                          )}
                          {cleanSnapshotsStatus === "error" && cleanSnapshotsError && (
                            <div className="mb-2 p-2.5 rounded-lg border border-danger bg-danger/10 text-[12px] text-danger">
                              {cleanSnapshotsError}
                            </div>
                          )}
                          <button
                            onClick={cleanOpenCodeSnapshots}
                            disabled={cleanSnapshotsStatus === "running"}
                            className="px-3 py-1.5 rounded-lg border border-border-strong/40 bg-bg-input-dark text-[12px] text-text-base hover:border-border-strong hover:bg-surface-hover transition-all disabled:opacity-50 disabled:cursor-not-allowed"
                          >
                            {cleanSnapshotsStatus === "running" ? "Cleaning…" : "Clean Snapshot Storage"}
                          </button>
                        </div>
                      </section>
                    )}
                  </>
                )}

                {/* Projects Tab */}
                {detailTab === "projects" && (
                  <section>
                    <label className="text-[11px] font-semibold text-text-muted tracking-wider uppercase flex items-center gap-1.5 mb-3">
                      <FolderOpen size={12} className={ICONS.project.iconColor} /> Projects Using This Agent
                    </label>
                    {selected.projects.length === 0 ? (
                      <p className="text-[13px] text-text-muted italic">
                        No projects are using {selected.label} yet. Add it to a project in the Projects tab.
                      </p>
                    ) : (
                      <ul className="space-y-2">
                        {selected.projects.map((p) => (
                          <li key={p.name}>
                            <button
                              onClick={() => onNavigateToProject?.(p.name)}
                              className={`w-full flex items-center gap-3 px-3 py-3 bg-bg-input rounded-lg border border-border-strong/40 text-left transition-colors ${onNavigateToProject ? "hover:bg-bg-sidebar hover:border-brand/40 group cursor-pointer" : "cursor-default"}`}
                            >
                              <div className={ICONS.project.iconBox}>
                                <FolderOpen size={15} className={ICONS.project.iconColor} />
                              </div>
                              <div className="flex-1 min-w-0">
                                <div className="text-[13px] font-medium text-text-base">{p.name}</div>
                                {p.directory && (
                                  <div className="text-[11px] text-text-muted font-mono truncate mt-0.5">
                                    {p.directory}
                                  </div>
                                )}
                              </div>
                              {onNavigateToProject && (
                                <ArrowRight size={13} className="text-text-muted opacity-0 group-hover:opacity-100 flex-shrink-0 transition-opacity" />
                              )}
                            </button>
                          </li>
                        ))}
                      </ul>
                    )}
                  </section>
                )}
              </div>
            </div>
          </div>
        ) : (
          /* Empty state */
          <div className="flex-1 flex flex-col items-center justify-center text-center p-8">
            <div className="w-16 h-16 mx-auto mb-6 rounded-2xl bg-brand/10 border border-brand/20 flex items-center justify-center">
              <Bot size={24} className={ICONS.agent.iconColor} strokeWidth={1.5} />
            </div>
            <h2 className="text-lg font-medium text-text-base mb-2">
              Providers
            </h2>
            <p className="text-[14px] text-text-muted mb-4 leading-relaxed max-w-sm">
              Providers are the coding tools that Automatic syncs configurations to.
              Select one from the sidebar to see which projects use it.
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
