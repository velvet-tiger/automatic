import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Bot, FolderOpen, AlertCircle, ArrowRight, CheckCircle2, XCircle, Settings2 } from "lucide-react";
import { ICONS } from "./icons";
import { AgentIcon } from "./AgentIcon";
import type { AgentCapabilities, AgentOptions } from "./AgentSelector";

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
  supported: boolean;
}

function CapabilityRow({ label, description, supported }: CapabilityRowProps) {
  return (
    <div className="flex items-center gap-3 px-3 py-2.5 bg-bg-input rounded-md border border-border-strong/40">
      {supported ? (
        <CheckCircle2 size={14} className="text-green-500 flex-shrink-0" />
      ) : (
        <XCircle size={14} className="text-text-muted flex-shrink-0" />
      )}
      <div className="flex-1 min-w-0">
        <span className="text-[13px] text-text-base">{label}</span>
        <span className="text-[11px] text-text-muted ml-2">{description}</span>
      </div>
    </div>
  );
}

interface AgentsProps {
  onNavigateToProject?: (projectName: string) => void;
}

export default function Agents({ onNavigateToProject }: AgentsProps = {}) {
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
  const [error, setError] = useState<string | null>(null);
  /** Default agent options loaded from settings — keyed by agent id */
  const [defaultOptions, setDefaultOptions] = useState<Record<string, AgentOptions>>({});

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
            Agents
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

            {/* Body */}
            <div className="flex-1 overflow-y-auto p-6 custom-scrollbar">
              <div className="max-w-2xl space-y-8">
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
                      supported={selected.capabilities.mcp_servers}
                    />
                  </div>
                  {selected.mcp_note && (
                    <div className="flex items-start gap-3 px-3 py-3 bg-bg-input rounded-md border border-border-strong mt-3">
                      <AlertCircle size={14} className="text-text-muted flex-shrink-0 mt-0.5" />
                      <p className="text-[12px] text-text-muted leading-relaxed">{selected.mcp_note}</p>
                    </div>
                  )}
                </section>

                {/* Default options — settable here, applied to new projects */}
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
                       override them individually in its Agents tab.
                     </p>
                   </section>
                 )}

                {/* Projects */}
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
              Agents
            </h2>
            <p className="text-[14px] text-text-muted mb-4 leading-relaxed max-w-sm">
              Agents are the coding tools that Automatic syncs configurations to.
              Select one from the sidebar to see which projects use it.
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
