import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Bot, FolderOpen, AlertCircle, ArrowRight } from "lucide-react";
import { ICONS } from "./icons";

interface AgentProject {
  name: string;
  directory: string;
}

interface AgentWithProjects {
  id: string;
  label: string;
  description: string;
  project_file: string;
  mcp_note: string | null;
  projects: AgentProject[];
}

interface AgentsProps {
  onNavigateToProject?: (projectName: string) => void;
}

export default function Agents({ onNavigateToProject }: AgentsProps = {}) {
  const LAST_AGENT_KEY = "nexus.agents.selected";
  const [agents, setAgents] = useState<AgentWithProjects[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(() => {
    return localStorage.getItem(LAST_AGENT_KEY);
  });
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    loadAgents();
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
      setAgents(parsed);
      setError(null);
    } catch (err: any) {
      setError(`Failed to load agents: ${err}`);
    }
  };

  const selected = agents.find((a) => a.id === selectedId) || null;

  const selectAgent = (id: string) => {
    setSelectedId(id);
    localStorage.setItem(LAST_AGENT_KEY, id);
  };

  return (
    <div className="flex h-full w-full bg-[#222327]">
      {/* Left sidebar - agent list */}
      <div className="w-64 flex-shrink-0 flex flex-col border-r border-[#33353A] bg-[#1A1A1E]/50">
        <div className="h-11 px-4 border-b border-[#33353A] flex items-center bg-[#222327]/30">
          <span className="text-[11px] font-semibold text-[#C8CAD0] tracking-wider uppercase">
            Agents
          </span>
        </div>

        <div className="flex-1 overflow-y-auto py-2 custom-scrollbar">
          {agents.length === 0 ? (
            <div className="px-4 py-3 text-[13px] text-[#C8CAD0] text-center">
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
                          ? "bg-[#2D2E36] text-[#F8F8FA]"
                          : "text-[#C8CAD0] hover:bg-[#2D2E36]/60 hover:text-[#F8F8FA]"
                      }`}
                    >
                      <div className={ICONS.agent.iconBox}>
                        <Bot size={15} className={ICONS.agent.iconColor} />
                      </div>
                      <div className="flex-1 min-w-0">
                        <div className={`text-[13px] font-medium truncate ${isActive ? "text-[#F8F8FA]" : "text-[#E8E9ED]"}`}>
                          {agent.label}
                        </div>
                        {agent.projects.length > 0 && (
                          <div className="text-[11px] text-[#C8CAD0] mt-0.5">
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
      <div className="flex-1 flex flex-col min-w-0 bg-[#222327]">
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
            <div className="h-11 px-6 border-b border-[#33353A] flex items-center gap-3">
              <Bot size={14} className={ICONS.agent.iconColor} />
              <h3 className="text-[14px] font-medium text-[#F8F8FA]">
                {selected.label}
              </h3>
            </div>

            {/* Body */}
            <div className="flex-1 overflow-y-auto p-6 custom-scrollbar">
              <div className="max-w-2xl space-y-8">
                {/* Agent Info */}
                <section>
                  <label className="block text-[11px] font-semibold text-[#C8CAD0] tracking-wider uppercase mb-3">
                    Details
                  </label>
                  <div className="space-y-3">
                    <div className="flex items-start gap-3 px-3 py-2.5 bg-[#1A1A1E] rounded-md border border-[#33353A]">
                      <span className="text-[11px] text-[#C8CAD0] w-24 flex-shrink-0 pt-0.5">Config File</span>
                      <span className="text-[13px] text-[#F8F8FA] font-mono">{selected.description}</span>
                    </div>
                    <div className="flex items-start gap-3 px-3 py-2.5 bg-[#1A1A1E] rounded-md border border-[#33353A]">
                      <span className="text-[11px] text-[#C8CAD0] w-24 flex-shrink-0 pt-0.5">Project Instructions</span>
                      <span className="text-[13px] text-[#F8F8FA] font-mono">{selected.project_file}</span>
                    </div>
                  </div>
                </section>

                {/* Limitations */}
                {selected.mcp_note && (
                  <section>
                    <label className="block text-[11px] font-semibold text-[#C8CAD0] tracking-wider uppercase mb-3">
                      Limitations
                    </label>
                    <div className="flex items-start gap-3 px-3 py-3 bg-[#1A1A1E] rounded-md border border-[#44474F]">
                      <AlertCircle size={14} className="text-[#C8CAD0] flex-shrink-0 mt-0.5" />
                      <div>
                        <p className="text-[13px] font-medium text-[#F8F8FA] mb-1">MCP configuration</p>
                        <p className="text-[12px] text-[#C8CAD0] leading-relaxed">{selected.mcp_note}</p>
                      </div>
                    </div>
                  </section>
                )}

                {/* Projects */}
                <section>
                  <label className="text-[11px] font-semibold text-[#C8CAD0] tracking-wider uppercase flex items-center gap-1.5 mb-3">
                    <FolderOpen size={12} className={ICONS.project.iconColor} /> Projects Using This Agent
                  </label>
                  {selected.projects.length === 0 ? (
                    <p className="text-[13px] text-[#C8CAD0]/60 italic">
                      No projects are using {selected.label} yet. Add it to a project in the Projects tab.
                    </p>
                  ) : (
                    <ul className="space-y-2">
                      {selected.projects.map((p) => (
                        <li key={p.name}>
                          <button
                            onClick={() => onNavigateToProject?.(p.name)}
                            className={`w-full flex items-center gap-3 px-3 py-3 bg-[#1A1A1E] rounded-lg border border-[#33353A] text-left transition-colors ${onNavigateToProject ? "hover:bg-[#2D2E36] hover:border-[#5E6AD2]/40 group cursor-pointer" : "cursor-default"}`}
                          >
                            <div className={ICONS.project.iconBox}>
                              <FolderOpen size={15} className={ICONS.project.iconColor} />
                            </div>
                            <div className="flex-1 min-w-0">
                              <div className="text-[13px] font-medium text-[#F8F8FA]">{p.name}</div>
                              {p.directory && (
                                <div className="text-[11px] text-[#C8CAD0] font-mono truncate mt-0.5">
                                  {p.directory}
                                </div>
                              )}
                            </div>
                            {onNavigateToProject && (
                              <ArrowRight size={13} className="text-[#C8CAD0] opacity-0 group-hover:opacity-100 flex-shrink-0 transition-opacity" />
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
            <div className="w-16 h-16 mx-auto mb-6 rounded-2xl bg-[#5E6AD2]/10 border border-[#5E6AD2]/20 flex items-center justify-center">
              <Bot size={24} className={ICONS.agent.iconColor} strokeWidth={1.5} />
            </div>
            <h2 className="text-lg font-medium text-[#F8F8FA] mb-2">
              Agents
            </h2>
            <p className="text-[14px] text-[#C8CAD0] mb-4 leading-relaxed max-w-sm">
              Agents are the coding tools that Automatic syncs configurations to.
              Select one from the sidebar to see which projects use it.
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
