import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Bot, FolderOpen } from "lucide-react";

interface AgentProject {
  name: string;
  directory: string;
}

interface AgentWithProjects {
  id: string;
  label: string;
  description: string;
  project_file: string;
  projects: AgentProject[];
}

export default function Agents() {
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
          <span className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase">
            Agents
          </span>
        </div>

        <div className="flex-1 overflow-y-auto py-2 custom-scrollbar">
          {agents.length === 0 ? (
            <div className="px-4 py-3 text-[13px] text-[#8A8C93] text-center">
              No agents registered.
            </div>
          ) : (
            <ul className="space-y-0.5 px-2">
              {agents.map((agent) => (
                <li key={agent.id}>
                  <button
                    onClick={() => selectAgent(agent.id)}
                    className={`w-full flex items-center gap-2.5 px-2 py-1.5 rounded-md text-[13px] font-medium transition-colors ${
                      selectedId === agent.id
                        ? "bg-[#2D2E36] text-[#E0E1E6]"
                        : "text-[#8A8C93] hover:bg-[#2D2E36]/50 hover:text-[#E0E1E6]"
                    }`}
                  >
                    <Bot
                      size={14}
                      className={
                        selectedId === agent.id
                          ? "text-[#E0E1E6]"
                          : "text-[#8A8C93]"
                      }
                    />
                    <span className="flex-1 text-left truncate">{agent.label}</span>
                    {agent.projects.length > 0 && (
                      <span className="text-[11px] bg-[#2D2E36] text-[#8A8C93] px-1.5 rounded-sm">
                        {agent.projects.length}
                      </span>
                    )}
                  </button>
                </li>
              ))}
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
              <Bot size={14} className="text-[#8A8C93]" />
              <h3 className="text-[14px] font-medium text-[#E0E1E6]">
                {selected.label}
              </h3>
            </div>

            {/* Body */}
            <div className="flex-1 overflow-y-auto p-6 custom-scrollbar">
              <div className="max-w-2xl space-y-8">
                {/* Agent Info */}
                <section>
                  <label className="block text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase mb-3">
                    Details
                  </label>
                  <div className="space-y-3">
                    <div className="flex items-start gap-3 px-3 py-2.5 bg-[#1A1A1E] rounded-md border border-[#33353A]">
                      <span className="text-[11px] text-[#8A8C93] w-24 flex-shrink-0 pt-0.5">Config File</span>
                      <span className="text-[13px] text-[#E0E1E6] font-mono">{selected.description}</span>
                    </div>
                    <div className="flex items-start gap-3 px-3 py-2.5 bg-[#1A1A1E] rounded-md border border-[#33353A]">
                      <span className="text-[11px] text-[#8A8C93] w-24 flex-shrink-0 pt-0.5">Project File</span>
                      <span className="text-[13px] text-[#E0E1E6] font-mono">{selected.project_file}</span>
                    </div>
                  </div>
                </section>

                {/* Projects */}
                <section>
                  <label className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase flex items-center gap-1.5 mb-3">
                    <FolderOpen size={12} /> Projects Using This Agent
                  </label>
                  {selected.projects.length === 0 ? (
                    <p className="text-[13px] text-[#8A8C93]/60 italic">
                      No projects are using {selected.label} yet. Add it to a project in the Projects tab.
                    </p>
                  ) : (
                    <ul className="space-y-1">
                      {selected.projects.map((p) => (
                        <li
                          key={p.name}
                          className="flex items-center gap-3 px-3 py-2.5 bg-[#1A1A1E] rounded-md border border-[#33353A] text-[13px] text-[#E0E1E6]"
                        >
                          <FolderOpen size={12} className="text-[#8A8C93] flex-shrink-0" />
                          <div className="flex-1 min-w-0">
                            <span className="font-medium">{p.name}</span>
                            {p.directory && (
                              <span className="ml-2 text-[11px] text-[#8A8C93] font-mono truncate">
                                {p.directory}
                              </span>
                            )}
                          </div>
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
            <div className="w-16 h-16 mx-auto mb-6 rounded-full border border-dashed border-[#44474F] flex items-center justify-center text-[#8A8C93]">
              <Bot size={24} strokeWidth={1.5} />
            </div>
            <h2 className="text-lg font-medium text-[#E0E1E6] mb-2">
              Agents
            </h2>
            <p className="text-[14px] text-[#8A8C93] mb-4 leading-relaxed max-w-sm">
              Agents are the coding tools that Nexus syncs configurations to.
              Select one from the sidebar to see which projects use it.
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
