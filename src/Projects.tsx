import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Plus,
  X,
  FolderOpen,
  Check,
  Code,
  Server,
  Trash2,
  Bot,
  RefreshCw,
} from "lucide-react";

interface Project {
  name: string;
  description: string;
  directory: string;
  skills: string[];
  mcp_servers: string[];
  providers: string[];
  agents: string[];
  created_at: string;
  updated_at: string;
}

interface AgentInfo {
  id: string;
  label: string;
  description: string;
}

function emptyProject(name: string): Project {
  return {
    name,
    description: "",
    directory: "",
    skills: [],
    mcp_servers: [],
    providers: [],
    agents: [],
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  };
}

export default function Projects() {
  const LAST_PROJECT_KEY = "nexus.projects.selected";
  const [projects, setProjects] = useState<string[]>([]);
  const [selectedName, setSelectedName] = useState<string | null>(() => {
    return localStorage.getItem(LAST_PROJECT_KEY);
  });
  const [project, setProject] = useState<Project | null>(null);
  const [dirty, setDirty] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [error, setError] = useState<string | null>(null);

  // Available items to pick from
  const [availableAgents, setAvailableAgents] = useState<AgentInfo[]>([]);
  const [availableSkills, setAvailableSkills] = useState<string[]>([]);
  const [availableMcpServers, setAvailableMcpServers] = useState<string[]>([]);

  // Inline add state
  const [addingSkill, setAddingSkill] = useState(false);
  const [addingMcp, setAddingMcp] = useState(false);
  const [addingAgent, setAddingAgent] = useState(false);
  const [syncStatus, setSyncStatus] = useState<string | null>(null);

  useEffect(() => {
    loadProjects();
    loadAvailableAgents();
    loadAvailableSkills();
    loadAvailableMcpServers();
  }, []);

  useEffect(() => {
    if (projects.length === 0) {
      return;
    }

    const preferred = selectedName && projects.includes(selectedName)
      ? selectedName
      : projects[0];

    if (preferred && (!project || project.name !== preferred) && !isCreating) {
      selectProject(preferred);
    }
  }, [projects]);

  const loadProjects = async () => {
    try {
      const result: string[] = await invoke("get_projects");
      setProjects(result.sort());
      setError(null);
    } catch (err: any) {
      setError(`Failed to load projects: ${err}`);
    }
  };

  const loadAvailableAgents = async () => {
    try {
      const result: AgentInfo[] = await invoke("list_agents");
      setAvailableAgents(result);
    } catch {
      // Agents list may not be available yet
    }
  };

  const loadAvailableSkills = async () => {
    try {
      const result: string[] = await invoke("get_skills");
      setAvailableSkills(result.sort());
    } catch {
      // Skills may not exist yet
    }
  };

  const loadAvailableMcpServers = async () => {
    try {
      const result: string[] = await invoke("list_mcp_server_configs");
      setAvailableMcpServers(result.sort());
    } catch {
      // MCP servers may not exist yet
    }
  };

  const selectProject = async (name: string) => {
    try {
      const raw: string = await invoke("autodetect_project_dependencies", { name });
      const parsed = JSON.parse(raw);
      // Normalize: ensure all fields exist with defaults for older projects
      const data: Project = {
        name: parsed.name || name,
        description: parsed.description || "",
        directory: parsed.directory || "",
        skills: parsed.skills || [],
        mcp_servers: parsed.mcp_servers || [],
        providers: parsed.providers || [],
        agents: parsed.agents || [],
        created_at: parsed.created_at || new Date().toISOString(),
        updated_at: parsed.updated_at || new Date().toISOString(),
      };
      setSelectedName(name);
      localStorage.setItem(LAST_PROJECT_KEY, name);
      setProject(data);
      setDirty(false);
      setIsCreating(false);
      setError(null);
    } catch (err: any) {
      setError(`Failed to read project: ${err}`);
    }
  };

  const updateField = <K extends keyof Project>(
    key: K,
    value: Project[K]
  ) => {
    if (!project) return;
    setProject({ ...project, [key]: value });
    setDirty(true);
  };

  const handleSave = async () => {
    if (!project) return;
    const name = isCreating ? newName.trim() : selectedName;
    if (!name) return;
    try {
      const toSave = { ...project, name, updated_at: new Date().toISOString() };
      await invoke("save_project", {
        name,
        data: JSON.stringify(toSave),
      });
      setDirty(false);
      setSelectedName(name);
      if (isCreating) {
        setIsCreating(false);
        await loadProjects();
      }
      setError(null);
    } catch (err: any) {
      setError(`Failed to save project: ${err}`);
    }
  };

  const handleRemove = async (name: string, e?: React.MouseEvent) => {
    if (e) e.stopPropagation();
    if (!confirm(`Remove project "${name}" from Nexus?\n\n(This only removes the project from this app. Your actual project files will NOT be deleted.)`)) return;
    try {
      await invoke("delete_project", { name });
      if (selectedName === name) {
        setSelectedName(null);
        localStorage.removeItem(LAST_PROJECT_KEY);
        setProject(null);
        setDirty(false);
      }
      await loadProjects();
      setError(null);
    } catch (err: any) {
      setError(`Failed to remove project: ${err}`);
    }
  };

  const startCreate = () => {
    setSelectedName(null);
    localStorage.removeItem(LAST_PROJECT_KEY);
    setProject(emptyProject(""));
    setDirty(true);
    setIsCreating(true);
    setNewName("");
  };

  // ── List helpers ─────────────────────────────────────────────────────────

  type ListField = "skills" | "mcp_servers" | "providers" | "agents";

  const addItem = (key: ListField, item: string) => {
    if (!project || !item.trim()) return;
    if (project[key].includes(item.trim())) return;
    updateField(key, [...project[key], item.trim()]);
  };

  const removeItem = (key: ListField, idx: number) => {
    if (!project) return;
    updateField(key, project[key].filter((_, i) => i !== idx));
  };

  const handleSync = async () => {
    const name = isCreating ? newName.trim() : selectedName;
    if (!name || !project) return;

    // Save first if dirty
    if (dirty) {
      await handleSave();
    }

    try {
      setSyncStatus("syncing");
      const result: string = await invoke("sync_project", { name });
      const files: string[] = JSON.parse(result);
      setSyncStatus(`Synced ${files.length} config${files.length !== 1 ? "s" : ""}`);
      
      // Reload the project in case sync discovered new skills/servers
      const raw: string = await invoke("read_project", { name });
      const parsed = JSON.parse(raw);
      setProject({
        name: parsed.name || name,
        description: parsed.description || "",
        directory: parsed.directory || "",
        skills: parsed.skills || [],
        mcp_servers: parsed.mcp_servers || [],
        providers: parsed.providers || [],
        agents: parsed.agents || [],
        created_at: parsed.created_at || new Date().toISOString(),
        updated_at: parsed.updated_at || new Date().toISOString(),
      });
      await loadAvailableSkills();
      await loadAvailableMcpServers();
      
      setTimeout(() => setSyncStatus(null), 4000);
    } catch (err: any) {
      setSyncStatus(`Sync failed: ${err}`);
      setTimeout(() => setSyncStatus(null), 5000);
    }
  };

  return (
    <div className="flex h-full w-full bg-[#222327]">
      {/* Left sidebar - project list */}
      <div className="w-64 flex-shrink-0 flex flex-col border-r border-[#33353A] bg-[#1A1A1E]/50">
        <div className="h-11 px-4 border-b border-[#33353A] flex justify-between items-center bg-[#222327]/30">
          <span className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase">
            Projects
          </span>
          <button
            onClick={startCreate}
            className="text-[#8A8C93] hover:text-[#E0E1E6] transition-colors p-1 hover:bg-[#2D2E36] rounded"
            title="Create New Project"
          >
            <Plus size={14} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto py-2 custom-scrollbar">
          {projects.length === 0 && !isCreating ? (
            <div className="px-4 py-3 text-[13px] text-[#8A8C93] text-center">
              No projects yet.
            </div>
          ) : (
            <ul className="space-y-0.5 px-2">
              {isCreating && (
                <li className="flex items-center gap-2.5 px-2 py-1.5 rounded-md text-[13px] bg-[#2D2E36] text-[#E0E1E6]">
                  <FolderOpen size={14} className="text-[#8A8C93]" />
                  <span className="italic">New Project...</span>
                </li>
              )}
              {projects.map((name) => (
                <li key={name} className="group flex items-center relative">
                  <button
                    onClick={() => selectProject(name)}
                    className={`w-full flex items-center gap-2.5 px-2 py-1.5 rounded-md text-[13px] font-medium transition-colors ${
                      selectedName === name && !isCreating
                        ? "bg-[#2D2E36] text-[#E0E1E6]"
                        : "text-[#8A8C93] hover:bg-[#2D2E36]/50 hover:text-[#E0E1E6]"
                    }`}
                  >
                    <FolderOpen
                      size={14}
                      className={
                        selectedName === name && !isCreating
                          ? "text-[#E0E1E6]"
                          : "text-[#8A8C93]"
                      }
                    />
                    <span className="flex-1 text-left truncate">{name}</span>
                  </button>
                  <button
                    onClick={(e) => handleRemove(name, e)}
                    className="absolute right-2 p-1 text-[#8A8C93] hover:text-[#FF6B6B] opacity-0 group-hover:opacity-100 hover:bg-[#33353A] rounded transition-all"
                    title="Remove Project from Nexus"
                  >
                    <X size={12} />
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>

      {/* Right area - project detail */}
      <div className="flex-1 flex flex-col min-w-0 bg-[#222327]">
        {error && (
          <div className="bg-red-500/10 text-red-400 p-3 text-[13px] border-b border-red-500/20 flex items-center justify-between">
            {error}
            <button onClick={() => setError(null)}>
              <X size={14} />
            </button>
          </div>
        )}

        {project ? (
          <div className="flex-1 flex flex-col h-full">
            {/* Header */}
            <div className="h-11 px-6 border-b border-[#33353A] flex justify-between items-center">
              <div className="flex items-center gap-3">
                <FolderOpen size={14} className="text-[#8A8C93]" />
                {isCreating ? (
                  <input
                    type="text"
                    placeholder="project-name (no spaces/slashes)"
                    value={newName}
                    onChange={(e) => setNewName(e.target.value)}
                    autoFocus
                    className="bg-transparent border-none outline-none text-[14px] font-medium text-[#E0E1E6] placeholder-[#8A8C93]/50 w-64"
                  />
                ) : (
                  <h3 className="text-[14px] font-medium text-[#E0E1E6]">
                    {selectedName}
                  </h3>
                )}
              </div>

              <div className="flex items-center gap-2">
                {syncStatus && (
                  <span className={`text-[12px] ${syncStatus.startsWith("Sync failed") ? "text-[#FF6B6B]" : syncStatus === "syncing" ? "text-[#8A8C93]" : "text-[#4ADE80]"}`}>
                    {syncStatus === "syncing" ? "Syncing..." : syncStatus}
                  </span>
                )}
                {!isCreating && selectedName && (
                  <button
                    onClick={() => handleRemove(selectedName)}
                    className="flex items-center gap-1.5 px-3 py-1.5 bg-[#2D2E36] hover:bg-[#FF6B6B]/10 text-[#8A8C93] hover:text-[#FF6B6B] rounded text-[12px] font-medium border border-[#3A3B42] hover:border-[#FF6B6B]/30 transition-colors mr-1"
                    title="Remove project from Nexus"
                  >
                    <Trash2 size={12} /> Remove
                  </button>
                )}
                {!dirty && project.directory && project.agents.length > 0 && (
                  <button
                    onClick={handleSync}
                    className="flex items-center gap-1.5 px-3 py-1.5 bg-[#2D2E36] hover:bg-[#33353A] text-[#E0E1E6] rounded text-[12px] font-medium border border-[#3A3B42] transition-colors"
                  >
                    <RefreshCw size={12} /> Sync Configs
                  </button>
                )}
                {dirty && (
                  <button
                    onClick={handleSave}
                    disabled={isCreating && !newName.trim()}
                    className="flex items-center gap-1.5 px-3 py-1.5 bg-[#5E6AD2] hover:bg-[#6B78E3] text-white rounded text-[12px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-sm"
                  >
                    <Check size={12} /> Save
                  </button>
                )}
              </div>
            </div>

            {/* Body */}
            <div className="flex-1 overflow-y-auto p-6 custom-scrollbar">
              <div className="max-w-2xl space-y-8">
                {/* Description */}
                <section>
                  <label className="block text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase mb-2">
                    Description
                  </label>
                  <textarea
                    value={project.description}
                    onChange={(e) => updateField("description", e.target.value)}
                    placeholder="What is this project for?"
                    rows={3}
                    className="w-full bg-[#1A1A1E] border border-[#33353A] hover:border-[#44474F] focus:border-[#5E6AD2] rounded-md px-3 py-2 text-[13px] text-[#E0E1E6] placeholder-[#8A8C93]/40 outline-none resize-none transition-colors"
                  />
                </section>

                {/* Directory */}
                <section>
                  <label className="block text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase mb-2">
                    <span className="flex items-center gap-1.5">
                      <FolderOpen size={12} /> Project Directory
                    </span>
                  </label>
                  <div className="flex gap-2">
                    <input
                      type="text"
                      value={project.directory}
                      onChange={(e) => updateField("directory", e.target.value)}
                      placeholder="/path/to/your/project"
                      className="flex-1 bg-[#1A1A1E] border border-[#33353A] hover:border-[#44474F] focus:border-[#5E6AD2] rounded-md px-3 py-2 text-[13px] text-[#E0E1E6] placeholder-[#8A8C93]/40 outline-none font-mono transition-colors"
                    />
                    <button
                      onClick={async () => {
                        const selected = await open({
                          directory: true,
                          multiple: false,
                          title: "Select project directory",
                        });
                        if (selected) {
                          updateField("directory", selected as string);
                        }
                      }}
                      className="px-3 py-2 bg-[#2D2E36] hover:bg-[#33353A] text-[#E0E1E6] text-[12px] font-medium rounded border border-[#3A3B42] transition-colors whitespace-nowrap"
                    >
                      Browse
                    </button>
                  </div>
                  <p className="mt-1.5 text-[11px] text-[#8A8C93]">
                    Agent configs will be written to this directory when you sync.
                  </p>
                </section>

                {/* Agent Tools */}
                <section>
                  <div className="flex items-center justify-between mb-2">
                    <label className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase flex items-center gap-1.5">
                      <Bot size={12} /> Agent Tools
                    </label>
                    <button
                      onClick={() => setAddingAgent(!addingAgent)}
                      className="text-[#8A8C93] hover:text-[#E0E1E6] p-0.5 hover:bg-[#2D2E36] rounded transition-colors"
                    >
                      <Plus size={13} />
                    </button>
                  </div>
                  {addingAgent && (
                    <div className="mb-2">
                      {availableAgents.filter((a) => !project.agents.includes(a.id)).length > 0 ? (
                        <div className="flex flex-wrap gap-1.5 p-2 bg-[#1A1A1E] rounded-md border border-[#33353A]">
                          {availableAgents
                            .filter((a) => !project.agents.includes(a.id))
                            .map((a) => (
                              <button
                                key={a.id}
                                onClick={() => { addItem("agents", a.id); setAddingAgent(false); }}
                                className="px-2 py-1 text-[12px] bg-[#2D2E36] hover:bg-[#5E6AD2] text-[#E0E1E6] rounded transition-colors flex items-center gap-1.5"
                              >
                                <span>{a.label}</span>
                                <span className="text-[10px] text-[#8A8C93]">{a.description}</span>
                              </button>
                            ))}
                        </div>
                      ) : (
                        <p className="text-[12px] text-[#8A8C93] italic">All agents added.</p>
                      )}
                    </div>
                  )}
                  {project.agents.length === 0 ? (
                    <p className="text-[13px] text-[#8A8C93]/60 italic">No agent tools selected. Add tools to enable config sync.</p>
                  ) : (
                    <ul className="space-y-1">
                      {project.agents.map((agentId, i) => {
                        const agent = availableAgents.find((a) => a.id === agentId);
                        return (
                          <li
                            key={agentId}
                            className="group flex items-center justify-between px-3 py-1.5 bg-[#1A1A1E] rounded-md border border-[#33353A] text-[13px] text-[#E0E1E6]"
                          >
                            <span className="flex items-center gap-2">
                              <Bot size={12} className="text-[#8A8C93]" />
                              {agent?.label || agentId}
                              <span className="text-[11px] text-[#8A8C93]">{agent?.description}</span>
                            </span>
                            <button
                              onClick={() => removeItem("agents", i)}
                              className="text-[#8A8C93] hover:text-[#FF6B6B] opacity-0 group-hover:opacity-100 transition-all"
                            >
                              <Trash2 size={12} />
                            </button>
                          </li>
                        );
                      })}
                    </ul>
                  )}
                </section>

                {/* Skills */}
                <section>
                  <div className="flex items-center justify-between mb-2">
                    <label className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase flex items-center gap-1.5">
                      <Code size={12} /> Skills
                    </label>
                    <button
                      onClick={() => setAddingSkill(!addingSkill)}
                      className="text-[#8A8C93] hover:text-[#E0E1E6] p-0.5 hover:bg-[#2D2E36] rounded transition-colors"
                    >
                      <Plus size={13} />
                    </button>
                  </div>
                  {addingSkill && (
                    <div className="mb-2">
                      {availableSkills.filter((s) => !project.skills.includes(s)).length > 0 ? (
                        <div className="flex flex-wrap gap-1.5 p-2 bg-[#1A1A1E] rounded-md border border-[#33353A]">
                          {availableSkills
                            .filter((s) => !project.skills.includes(s))
                            .map((s) => (
                              <button
                                key={s}
                                onClick={() => { addItem("skills", s); setAddingSkill(false); }}
                                className="px-2 py-1 text-[12px] bg-[#2D2E36] hover:bg-[#5E6AD2] text-[#E0E1E6] rounded transition-colors"
                              >
                                {s}
                              </button>
                            ))}
                        </div>
                      ) : (
                        <p className="text-[12px] text-[#8A8C93] italic">No more skills available.</p>
                      )}
                    </div>
                  )}
                  {project.skills.length === 0 ? (
                    <p className="text-[13px] text-[#8A8C93]/60 italic">No skills attached.</p>
                  ) : (
                    <ul className="space-y-1">
                      {project.skills.map((s, i) => (
                        <li
                          key={s}
                          className="group flex items-center justify-between px-3 py-1.5 bg-[#1A1A1E] rounded-md border border-[#33353A] text-[13px] text-[#E0E1E6]"
                        >
                          <span className="flex items-center gap-2">
                            <Code size={12} className="text-[#8A8C93]" />
                            {s}
                          </span>
                          <button
                            onClick={() => removeItem("skills", i)}
                            className="text-[#8A8C93] hover:text-[#FF6B6B] opacity-0 group-hover:opacity-100 transition-all"
                          >
                            <Trash2 size={12} />
                          </button>
                        </li>
                      ))}
                    </ul>
                  )}
                </section>

                {/* MCP Servers */}
                <section>
                  <div className="flex items-center justify-between mb-2">
                    <label className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase flex items-center gap-1.5">
                      <Server size={12} /> MCP Servers
                    </label>
                    <button
                      onClick={() => setAddingMcp(!addingMcp)}
                      className="text-[#8A8C93] hover:text-[#E0E1E6] p-0.5 hover:bg-[#2D2E36] rounded transition-colors"
                    >
                      <Plus size={13} />
                    </button>
                  </div>
                  {addingMcp && (
                    <div className="mb-2">
                      {availableMcpServers.filter((s) => !project.mcp_servers.includes(s)).length > 0 ? (
                        <div className="flex flex-wrap gap-1.5 p-2 bg-[#1A1A1E] rounded-md border border-[#33353A]">
                          {availableMcpServers
                            .filter((s) => !project.mcp_servers.includes(s))
                            .map((s) => (
                              <button
                                key={s}
                                onClick={() => { addItem("mcp_servers", s); setAddingMcp(false); }}
                                className="px-2 py-1 text-[12px] bg-[#2D2E36] hover:bg-[#5E6AD2] text-[#E0E1E6] rounded transition-colors"
                              >
                                {s}
                              </button>
                            ))}
                        </div>
                      ) : (
                        <p className="text-[12px] text-[#8A8C93] italic">No MCP servers found in config.</p>
                      )}
                    </div>
                  )}
                  {project.mcp_servers.length === 0 ? (
                    <p className="text-[13px] text-[#8A8C93]/60 italic">No MCP servers attached.</p>
                  ) : (
                    <ul className="space-y-1">
                      {project.mcp_servers.map((s, i) => (
                        <li
                          key={s}
                          className="group flex items-center justify-between px-3 py-1.5 bg-[#1A1A1E] rounded-md border border-[#33353A] text-[13px] text-[#E0E1E6]"
                        >
                          <span className="flex items-center gap-2">
                            <Server size={12} className="text-[#8A8C93]" />
                            {s}
                          </span>
                          <button
                            onClick={() => removeItem("mcp_servers", i)}
                            className="text-[#8A8C93] hover:text-[#FF6B6B] opacity-0 group-hover:opacity-100 transition-all"
                          >
                            <Trash2 size={12} />
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
            <div className="w-16 h-16 mx-auto mb-6 rounded-full border border-dashed border-[#44474F] flex items-center justify-center text-[#8A8C93]">
              <FolderOpen size={24} strokeWidth={1.5} />
            </div>
            <h2 className="text-lg font-medium text-[#E0E1E6] mb-2">
              No Project Selected
            </h2>
            <p className="text-[14px] text-[#8A8C93] mb-8 leading-relaxed max-w-sm">
              Projects group skills and MCP servers into reusable
              configurations. Select one from the sidebar or create a new
              project.
            </p>
            <button
              onClick={startCreate}
              className="px-4 py-2 bg-[#5E6AD2] hover:bg-[#6B78E3] text-white text-[13px] font-medium rounded shadow-sm transition-colors"
            >
              Create Project
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
