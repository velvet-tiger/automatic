import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Plus,
  X,
  Check,
  Code,
  Server,
  Trash2,
  Bot,
  LayoutTemplate,
  Copy,
  ChevronDown,
  FileText,
  ChevronRight,
  Search,
} from "lucide-react";

interface TemplateProjectFile {
  filename: string;
  content: string;
}

interface ProjectTemplate {
  name: string;
  description: string;
  skills: string[];
  mcp_servers: string[];
  providers: string[];
  agents: string[];
  project_files: TemplateProjectFile[];
}

interface AgentInfo {
  id: string;
  label: string;
  description: string;
}

interface Project {
  name: string;
  description: string;
  directory: string;
  skills: string[];
  local_skills: string[];
  mcp_servers: string[];
  providers: string[];
  agents: string[];
}

const SIDEBAR_MIN = 180;
const SIDEBAR_MAX = 420;
const SIDEBAR_DEFAULT = 220;

function emptyTemplate(name: string): ProjectTemplate {
  return { name, description: "", skills: [], mcp_servers: [], providers: [], agents: [], project_files: [] };
}

// Derive a colour for the sidebar icon box based on what's in the template
function templateAccent(t: ProjectTemplate): { bg: string; icon: string } {
  if (t.skills.length >= t.mcp_servers.length && t.skills.length > 0)
    return { bg: "bg-[#4ADE80]/15", icon: "text-[#4ADE80]" };
  if (t.mcp_servers.length > 0)
    return { bg: "bg-[#F59E0B]/15", icon: "text-[#F59E0B]" };
  return { bg: "bg-[#5E6AD2]/15", icon: "text-[#5E6AD2]" };
}

// Known project file names and which agents use them
const KNOWN_PROJECT_FILES = [
  { filename: "CLAUDE.md", label: "CLAUDE.md", hint: "Claude Code" },
  { filename: "AGENTS.md", label: "AGENTS.md", hint: "Claude Code / Codex" },
  { filename: ".cursorrules", label: ".cursorrules", hint: "Cursor" },
  { filename: ".windsurfrules", label: ".windsurfrules", hint: "Windsurf" },
  { filename: ".github/copilot-instructions.md", label: "copilot-instructions.md", hint: "GitHub Copilot" },
];

export default function ProjectTemplates() {
  const [templates, setTemplates] = useState<string[]>([]);
  // Map of template name → loaded data (for sidebar summaries)
  const [templateData, setTemplateData] = useState<Record<string, ProjectTemplate>>({});

  const [selectedName, setSelectedName] = useState<string | null>(null);
  const [template, setTemplate] = useState<ProjectTemplate | null>(null);
  const [dirty, setDirty] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameName, setRenameName] = useState("");
  const [saveStatus, setSaveStatus] = useState<string | null>(null);

  // Available items to pick from
  const [availableAgents, setAvailableAgents] = useState<AgentInfo[]>([]);
  const [availableSkills, setAvailableSkills] = useState<string[]>([]);
  const [availableMcpServers, setAvailableMcpServers] = useState<string[]>([]);
  const [availableFileTemplates, setAvailableFileTemplates] = useState<string[]>([]);

  // All projects (for "Applied to" + "Apply to project")
  const [allProjects, setAllProjects] = useState<Project[]>([]);
  const [showApplyPicker, setShowApplyPicker] = useState(false);
  const [applyStatus, setApplyStatus] = useState<string | null>(null);

  // Inline-add state
  const [addingSkill, setAddingSkill] = useState(false);
  const [skillSearch, setSkillSearch] = useState("");
  const [addingMcp, setAddingMcp] = useState(false);
  const [addingAgent, setAddingAgent] = useState(false);
  const [addingFile, setAddingFile] = useState(false);
  const [customFilename, setCustomFilename] = useState("");

  // Expanded project file (for inline content editing)
  const [expandedFile, setExpandedFile] = useState<string | null>(null);

  // Inline delete confirmation — holds the name awaiting confirmation
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);

  // Sidebar resize
  const [sidebarWidth, setSidebarWidth] = useState(SIDEBAR_DEFAULT);
  const isSidebarDragging = useRef(false);

  const onSidebarMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    isSidebarDragging.current = true;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
  }, []);

  useEffect(() => {
    const onMouseMove = (e: MouseEvent) => {
      if (!isSidebarDragging.current) return;
      const newWidth = Math.min(SIDEBAR_MAX, Math.max(SIDEBAR_MIN, e.clientX - 180));
      setSidebarWidth(newWidth);
    };
    const onMouseUp = () => {
      if (isSidebarDragging.current) {
        isSidebarDragging.current = false;
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
      }
    };
    window.addEventListener("mousemove", onMouseMove);
    window.addEventListener("mouseup", onMouseUp);
    return () => {
      window.removeEventListener("mousemove", onMouseMove);
      window.removeEventListener("mouseup", onMouseUp);
    };
  }, []);

  useEffect(() => {
    loadTemplates();
    loadAvailableAgents();
    loadAvailableSkills();
    loadAvailableMcpServers();
    loadAvailableFileTemplates();
    loadAllProjects();
  }, []);

  useEffect(() => {
    if (templates.length === 0) return;
    const preferred = selectedName && templates.includes(selectedName)
      ? selectedName
      : templates[0];
    if (preferred && (!template || template.name !== preferred) && !isCreating) {
      selectTemplate(preferred);
    }
  }, [templates]);

  const loadTemplates = async () => {
    try {
      const names: string[] = await invoke("get_project_templates");
      setTemplates(names);
      setError(null);
      // Load all template data for sidebar summaries
      const entries = await Promise.all(
        names.map(async (name) => {
          try {
            const raw: string = await invoke("read_project_template", { name });
            return [name, JSON.parse(raw) as ProjectTemplate] as const;
          } catch {
            return [name, emptyTemplate(name)] as const;
          }
        })
      );
      setTemplateData(Object.fromEntries(entries));
    } catch (err: any) {
      setError(`Failed to load project templates: ${err}`);
    }
  };

  const loadAvailableAgents = async () => {
    try {
      const result: AgentInfo[] = await invoke("list_agents");
      setAvailableAgents(result);
    } catch { /* ignore */ }
  };

  const loadAvailableSkills = async () => {
    try {
      const result: { name: string }[] = await invoke("get_skills");
      setAvailableSkills(result.map((e) => e.name).sort());
    } catch { /* ignore */ }
  };

  const loadAvailableMcpServers = async () => {
    try {
      const result: string[] = await invoke("list_mcp_server_configs");
      setAvailableMcpServers(result.sort());
    } catch { /* ignore */ }
  };

  const loadAvailableFileTemplates = async () => {
    try {
      const result: string[] = await invoke("get_templates");
      setAvailableFileTemplates(result.sort());
    } catch { /* ignore */ }
  };

  const loadAllProjects = async () => {
    try {
      const names: string[] = await invoke("get_projects");
      const loaded = await Promise.all(
        names.map(async (name) => {
          try {
            const raw: string = await invoke("read_project", { name });
            return JSON.parse(raw) as Project;
          } catch {
            return null;
          }
        })
      );
      setAllProjects(loaded.filter(Boolean) as Project[]);
    } catch { /* ignore */ }
  };

  const selectTemplate = async (name: string) => {
    try {
      const raw: string = await invoke("read_project_template", { name });
      const parsed: ProjectTemplate = JSON.parse(raw);
      setSelectedName(name);
      setTemplate({
        name: parsed.name || name,
        description: parsed.description || "",
        skills: parsed.skills || [],
        mcp_servers: parsed.mcp_servers || [],
        providers: parsed.providers || [],
        agents: parsed.agents || [],
        project_files: parsed.project_files || [],
      });
      setDirty(false);
      setIsCreating(false);
      setError(null);
      setShowApplyPicker(false);
      setApplyStatus(null);
      setConfirmDelete(null);
      setExpandedFile(null);
    } catch (err: any) {
      setError(`Failed to read project template: ${err}`);
    }
  };

  const updateField = <K extends keyof ProjectTemplate>(key: K, value: ProjectTemplate[K]) => {
    if (!template) return;
    setTemplate({ ...template, [key]: value });
    setDirty(true);
  };

  const handleSave = async () => {
    if (!template) return;
    const name = isCreating ? newName.trim() : selectedName;
    if (!name) return;
    try {
      setSaveStatus("saving");
      const toSave: ProjectTemplate = { ...template, name };
      await invoke("save_project_template", { name, data: JSON.stringify(toSave, null, 2) });
      setSelectedName(name);
      if (isCreating) {
        setIsCreating(false);
        await loadTemplates();
      } else {
        // Refresh sidebar data
        setTemplateData((prev) => ({ ...prev, [name]: toSave }));
      }
      setDirty(false);
      setError(null);
      setSaveStatus("Saved");
      setTimeout(() => setSaveStatus(null), 3000);
    } catch (err: any) {
      setSaveStatus(null);
      setError(`Failed to save project template: ${err}`);
    }
  };

  const handleDelete = async (name: string, e?: React.MouseEvent) => {
    if (e) e.stopPropagation();
    // First click arms the confirmation; second click executes
    if (confirmDelete !== name) {
      setConfirmDelete(name);
      return;
    }
    setConfirmDelete(null);
    try {
      await invoke("delete_project_template", { name });
      if (selectedName === name) {
        setSelectedName(null);
        setTemplate(null);
        setDirty(false);
      }
      await loadTemplates();
      setError(null);
    } catch (err: any) {
      setError(`Failed to delete project template: ${err}`);
    }
  };

  const handleDuplicate = async () => {
    if (!template || !selectedName) return;
    let base = `${selectedName} Copy`;
    let candidate = base;
    let i = 2;
    while (templates.includes(candidate)) candidate = `${base} ${i++}`;
    try {
      const copy: ProjectTemplate = { ...template, name: candidate };
      await invoke("save_project_template", { name: candidate, data: JSON.stringify(copy, null, 2) });
      await loadTemplates();
      await selectTemplate(candidate);
      setError(null);
    } catch (err: any) {
      setError(`Failed to duplicate template: ${err}`);
    }
  };

  // Apply template to a project (merge, non-destructive)
  const applyToProject = async (projectName: string) => {
    if (!template) return;
    const proj = allProjects.find((p) => p.name === projectName);
    if (!proj) return;
    try {
      // Merge config
      const updated: Project = {
        ...proj,
        description: proj.description || template.description,
        agents: [...new Set([...proj.agents, ...template.agents])],
        skills: [...new Set([...proj.skills, ...template.skills])],
        mcp_servers: [...new Set([...proj.mcp_servers, ...template.mcp_servers])],
        providers: [...new Set([...proj.providers, ...template.providers])],
      };
      await invoke("save_project", { name: projectName, data: JSON.stringify(updated, null, 2) });

      // Write project files to the project's directory (non-destructive: only if file doesn't exist)
      for (const pf of template.project_files) {
        if (pf.filename && pf.content) {
          try {
            await invoke("save_project_file", {
              name: projectName,
              filename: pf.filename,
              content: pf.content,
            });
          } catch { /* skip files that can't be written (e.g. no directory set) */ }
        }
      }

      await loadAllProjects();
      setShowApplyPicker(false);
      setApplyStatus(`Applied to "${projectName}"`);
      setTimeout(() => setApplyStatus(null), 3000);
    } catch (err: any) {
      setError(`Failed to apply template: ${err}`);
    }
  };

  const startCreate = () => {
    setSelectedName(null);
    setTemplate(emptyTemplate(""));
    setDirty(true);
    setIsCreating(true);
    setNewName("");
  };

  const startRename = () => {
    if (!selectedName || isCreating) return;
    setRenameName(selectedName);
    setIsRenaming(true);
  };

  const handleRename = async () => {
    const trimmed = renameName.trim();
    if (!selectedName || !trimmed || trimmed === selectedName) {
      setIsRenaming(false);
      return;
    }
    try {
      await invoke("rename_project_template", { oldName: selectedName, newName: trimmed });
      setSelectedName(trimmed);
      setIsRenaming(false);
      setError(null);
      await loadTemplates();
      await selectTemplate(trimmed);
    } catch (err: any) {
      setError(`Failed to rename project template: ${err}`);
      setIsRenaming(false);
    }
  };

  type ListField = "skills" | "mcp_servers" | "providers" | "agents";

  const addItem = (key: ListField, item: string) => {
    if (!template || !item.trim()) return;
    if (template[key].includes(item.trim())) return;
    updateField(key, [...template[key], item.trim()]);
  };

  const removeItem = (key: ListField, idx: number) => {
    if (!template) return;
    updateField(key, template[key].filter((_, i) => i !== idx));
  };

  // Project file helpers
  const addProjectFile = (filename: string, content: string = "") => {
    if (!template || !filename.trim()) return;
    if (template.project_files.some((f) => f.filename === filename.trim())) return;
    const updated = [...template.project_files, { filename: filename.trim(), content }];
    updateField("project_files", updated);
    setExpandedFile(filename.trim());
  };

  const removeProjectFile = (idx: number) => {
    if (!template) return;
    const removed = template.project_files[idx];
    if (expandedFile === removed.filename) setExpandedFile(null);
    updateField("project_files", template.project_files.filter((_, i) => i !== idx));
  };

  const updateProjectFileContent = (filename: string, content: string) => {
    if (!template) return;
    const updated = template.project_files.map((f) =>
      f.filename === filename ? { ...f, content } : f
    );
    updateField("project_files", updated);
  };

  // Add project file from a file template
  const addFromFileTemplate = async (templateName: string, filename: string) => {
    try {
      const content: string = await invoke("read_template", { name: templateName });
      addProjectFile(filename, content);
    } catch {
      addProjectFile(filename, "");
    }
    setAddingFile(false);
    setCustomFilename("");
  };

  // Projects that have had this template applied (superset match)
  const appliedProjects = template
    ? allProjects.filter((p) => {
        const hasAllAgents = template.agents.every((a) => p.agents.includes(a));
        const hasAllSkills = template.skills.every((s) => p.skills.includes(s));
        const hasAllMcp = template.mcp_servers.every((m) => p.mcp_servers.includes(m));
        const hasContent =
          template.agents.length > 0 || template.skills.length > 0 || template.mcp_servers.length > 0;
        return hasContent && hasAllAgents && hasAllSkills && hasAllMcp;
      })
    : [];

  // Projects not yet fully matching (candidates for apply)
  const unappliedProjects = allProjects.filter(
    (p) => !appliedProjects.some((ap) => ap.name === p.name)
  );

  // ── Render ──────────────────────────────────────────────────────────────────

  return (
    <div className="flex h-full w-full bg-[#222327]">
      {/* Left sidebar */}
      <div
        className="flex-shrink-0 flex flex-col border-r border-[#33353A] bg-[#1A1A1E]/50 relative"
        style={{ width: sidebarWidth }}
      >
        <div className="h-11 px-4 border-b border-[#33353A] flex justify-between items-center">
          <span className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase">
            Templates
          </span>
          <button
            onClick={startCreate}
            className="text-[#8A8C93] hover:text-[#E0E1E6] transition-colors p-1 hover:bg-[#2D2E36] rounded"
            title="Create New Template"
          >
            <Plus size={14} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto py-2 custom-scrollbar">
          {templates.length === 0 && !isCreating ? (
            <div className="px-4 py-8 text-center">
              <div className="w-10 h-10 mx-auto mb-3 rounded-full border border-dashed border-[#44474F] flex items-center justify-center">
                <LayoutTemplate size={16} className="text-[#8A8C93]" strokeWidth={1.5} />
              </div>
              <p className="text-[12px] text-[#8A8C93]">No templates yet.</p>
              <button
                onClick={startCreate}
                className="mt-3 text-[12px] text-[#5E6AD2] hover:text-[#6B78E3] transition-colors"
              >
                Create one
              </button>
            </div>
          ) : (
            <ul className="space-y-1 px-2">
              {isCreating && (
                <li className="flex items-center gap-3 px-3 py-2.5 rounded-lg bg-[#2D2E36]">
                  <div className="w-9 h-9 rounded-lg bg-[#5E6AD2]/15 flex items-center justify-center flex-shrink-0">
                    <LayoutTemplate size={16} className="text-[#5E6AD2]" />
                  </div>
                  <span className="text-[13px] text-[#E0E1E6] italic">New Template...</span>
                </li>
              )}
              {templates.map((name) => {
                const td = templateData[name];
                const isActive = selectedName === name && !isCreating;
                const accent = td ? templateAccent(td) : { bg: "bg-[#5E6AD2]/15", icon: "text-[#5E6AD2]" };
                const skillCount = td?.skills.length ?? 0;
                const mcpCount = td?.mcp_servers.length ?? 0;
                const fileCount = td?.project_files?.length ?? 0;
                const parts: string[] = [];
                if (skillCount > 0) parts.push(`${skillCount} skill${skillCount !== 1 ? "s" : ""}`);
                if (mcpCount > 0) parts.push(`${mcpCount} server${mcpCount !== 1 ? "s" : ""}`);
                if (fileCount > 0) parts.push(`${fileCount} file${fileCount !== 1 ? "s" : ""}`);

                return (
                  <li key={name} className="group relative">
                    <button
                      onClick={() => { if (!isCreating) selectTemplate(name); }}
                      className={`w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-left transition-colors ${
                        isActive
                          ? "bg-[#2D2E36] text-[#E0E1E6]"
                          : "text-[#8A8C93] hover:bg-[#2D2E36]/60 hover:text-[#E0E1E6]"
                      }`}
                    >
                      <div className={`w-9 h-9 rounded-lg ${accent.bg} flex items-center justify-center flex-shrink-0`}>
                        <LayoutTemplate size={16} className={accent.icon} />
                      </div>
                      <div className="flex-1 min-w-0">
                        <div className={`text-[13px] font-medium truncate ${isActive ? "text-[#E0E1E6]" : "text-[#C8CAD0]"}`}>
                          {name}
                        </div>
                        {parts.length > 0 && (
                          <div className="text-[11px] text-[#8A8C93] mt-0.5">
                            {parts.join(" · ")}
                          </div>
                        )}
                      </div>
                    </button>
                    {confirmDelete === name ? (
                      <div className="absolute right-2 top-1/2 -translate-y-1/2 flex items-center gap-1">
                        <button
                          onClick={(e) => handleDelete(name, e)}
                          className="px-1.5 py-0.5 text-[11px] font-medium text-[#FF6B6B] hover:bg-[#FF6B6B]/15 rounded transition-colors"
                        >
                          Delete
                        </button>
                        <button
                          onClick={(e) => { e.stopPropagation(); setConfirmDelete(null); }}
                          className="p-0.5 text-[#8A8C93] hover:text-[#E0E1E6] hover:bg-[#33353A] rounded transition-colors"
                        >
                          <X size={11} />
                        </button>
                      </div>
                    ) : (
                      <button
                        onClick={(e) => handleDelete(name, e)}
                        className="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-[#8A8C93] hover:text-[#FF6B6B] opacity-0 group-hover:opacity-100 hover:bg-[#33353A] rounded transition-all"
                        title="Delete template"
                      >
                        <X size={12} />
                      </button>
                    )}
                  </li>
                );
              })}
            </ul>
          )}
        </div>

        {/* Resize handle */}
        <div
          className="absolute top-0 right-0 w-1 h-full cursor-col-resize hover:bg-[#5E6AD2]/40 active:bg-[#5E6AD2]/60 transition-colors z-10"
          onMouseDown={onSidebarMouseDown}
        />
      </div>

      {/* Right panel */}
      <div className="flex-1 flex flex-col min-w-0 bg-[#222327]">
        {error && (
          <div className="bg-red-500/10 text-red-400 p-3 text-[13px] border-b border-red-500/20 flex items-center justify-between">
            {error}
            <button onClick={() => setError(null)}><X size={14} /></button>
          </div>
        )}

        {template ? (
          <div className="flex-1 flex flex-col h-full">
            {/* Header */}
            <div className="h-11 px-6 border-b border-[#33353A] flex justify-between items-center">
              <div className="flex items-center gap-3">
                {isCreating ? (
                  <input
                    type="text"
                    placeholder="template-name"
                    value={newName}
                    onChange={(e) => setNewName(e.target.value)}
                    autoFocus
                    className="bg-transparent border-none outline-none text-[14px] font-medium text-[#E0E1E6] placeholder-[#8A8C93]/50 w-64"
                  />
                ) : isRenaming ? (
                  <input
                    type="text"
                    value={renameName}
                    onChange={(e) => setRenameName(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") handleRename();
                      if (e.key === "Escape") setIsRenaming(false);
                    }}
                    onBlur={handleRename}
                    autoFocus
                    className="bg-transparent border-none outline-none text-[14px] font-medium text-[#E0E1E6] placeholder-[#8A8C93]/50 w-64"
                  />
                ) : (
                  <h3
                    className="text-[14px] font-medium text-[#E0E1E6] cursor-text"
                    onDoubleClick={startRename}
                    title="Double-click to rename"
                  >
                    {selectedName}
                  </h3>
                )}
              </div>

              <div className="flex items-center gap-2">
                {saveStatus && (
                  <span className={`text-[12px] ${saveStatus === "saving" ? "text-[#8A8C93]" : "text-[#4ADE80]"}`}>
                    {saveStatus === "saving" ? "Saving..." : saveStatus}
                  </span>
                )}
                {applyStatus && (
                  <span className="text-[12px] text-[#4ADE80]">{applyStatus}</span>
                )}
                {!isCreating && selectedName && (
                  <>
                    <button
                      onClick={handleDuplicate}
                      className="flex items-center gap-1.5 px-3 py-1.5 bg-[#2D2E36] hover:bg-[#33353A] text-[#8A8C93] hover:text-[#E0E1E6] rounded text-[12px] font-medium border border-[#3A3B42] transition-colors"
                    >
                      <Copy size={12} /> Duplicate
                    </button>
                    {/* Apply to project */}
                    <div className="relative">
                      <button
                        onClick={() => setShowApplyPicker(!showApplyPicker)}
                        className="flex items-center gap-1.5 px-3 py-1.5 bg-[#5E6AD2] hover:bg-[#6B78E3] text-white rounded text-[12px] font-medium transition-colors shadow-sm"
                      >
                        Apply to project...
                        <ChevronDown size={12} className={`transition-transform ${showApplyPicker ? "rotate-180" : ""}`} />
                      </button>
                      {showApplyPicker && (
                        <div className="absolute right-0 top-full mt-1 w-56 bg-[#1A1A1E] border border-[#33353A] rounded-lg shadow-xl z-20 overflow-hidden">
                          {allProjects.length === 0 ? (
                            <div className="px-3 py-4 text-[12px] text-[#8A8C93] text-center">
                              No projects yet
                            </div>
                          ) : (
                            <>
                              {unappliedProjects.length > 0 && (
                                <div>
                                  <div className="px-3 pt-2.5 pb-1 text-[10px] font-semibold text-[#8A8C93] uppercase tracking-wider">
                                    Apply to
                                  </div>
                                  {unappliedProjects.map((p) => (
                                    <button
                                      key={p.name}
                                      onClick={() => applyToProject(p.name)}
                                      className="w-full text-left px-3 py-2 text-[13px] text-[#E0E1E6] hover:bg-[#2D2E36] transition-colors"
                                    >
                                      {p.name}
                                    </button>
                                  ))}
                                </div>
                              )}
                              {appliedProjects.length > 0 && (
                                <div className={unappliedProjects.length > 0 ? "border-t border-[#33353A]" : ""}>
                                  <div className="px-3 pt-2.5 pb-1 text-[10px] font-semibold text-[#8A8C93] uppercase tracking-wider">
                                    Already applied
                                  </div>
                                  {appliedProjects.map((p) => (
                                    <button
                                      key={p.name}
                                      onClick={() => applyToProject(p.name)}
                                      className="w-full text-left px-3 py-2 text-[13px] text-[#8A8C93] hover:bg-[#2D2E36] hover:text-[#E0E1E6] transition-colors flex items-center justify-between"
                                    >
                                      <span>{p.name}</span>
                                      <Check size={11} className="text-[#4ADE80]" />
                                    </button>
                                  ))}
                                </div>
                              )}
                            </>
                          )}
                          <div className="border-t border-[#33353A]">
                            <button
                              onClick={() => setShowApplyPicker(false)}
                              className="w-full px-3 py-2 text-[12px] text-[#8A8C93] hover:text-[#E0E1E6] text-left transition-colors"
                            >
                              Cancel
                            </button>
                          </div>
                        </div>
                      )}
                    </div>
                  </>
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
            <div className="flex-1 overflow-y-auto p-6 custom-scrollbar" onClick={() => { setShowApplyPicker(false); setConfirmDelete(null); }}>
              <div className="max-w-2xl space-y-8">

                {/* Description */}
                {(template.description || isCreating || dirty) && (
                  <div>
                    <label className="block text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase mb-2">
                      Description
                    </label>
                    <textarea
                      value={template.description}
                      onChange={(e) => updateField("description", e.target.value)}
                      placeholder="What is this template for? What kind of projects should use it?"
                      rows={2}
                      className="w-full bg-[#1A1A1E] border border-[#33353A] hover:border-[#44474F] focus:border-[#5E6AD2] rounded-md px-3 py-2 text-[13px] text-[#E0E1E6] placeholder-[#8A8C93]/40 outline-none resize-none transition-colors"
                    />
                  </div>
                )}

                {/* Agents */}
                <div>
                  <div className="flex items-center justify-between mb-3">
                    <div className="flex items-center gap-2">
                      <Bot size={13} className="text-[#5E6AD2]" />
                      <span className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase">
                        Agent Tools
                      </span>
                    </div>
                    <button
                      onClick={(e) => { e.stopPropagation(); setAddingAgent(true); }}
                      className="text-[11px] text-[#5E6AD2] hover:text-[#6B78E3] flex items-center gap-1 transition-colors"
                    >
                      <Plus size={12} /> Add
                    </button>
                  </div>

                  {template.agents.length === 0 && !addingAgent && (
                    <p className="text-[12px] text-[#8A8C93]/50 italic pl-1">No agents configured.</p>
                  )}

                  <div className="space-y-2">
                    {template.agents.map((id, idx) => {
                      const info = availableAgents.find((a) => a.id === id);
                      return (
                        <div
                          key={id}
                          className="flex items-center gap-3 px-3 py-3 bg-[#1A1A1E] border border-[#33353A] rounded-lg group"
                        >
                          <div className="w-8 h-8 rounded-md bg-[#5E6AD2]/15 flex items-center justify-center flex-shrink-0">
                            <Bot size={15} className="text-[#5E6AD2]" />
                          </div>
                          <div className="flex-1 min-w-0">
                            <div className="text-[13px] font-medium text-[#E0E1E6]">{info?.label ?? id}</div>
                            {info?.description && (
                              <div className="text-[11px] text-[#8A8C93] mt-0.5">{info.description}</div>
                            )}
                          </div>
                          <button
                            onClick={() => removeItem("agents", idx)}
                            className="text-[#8A8C93] hover:text-[#FF6B6B] opacity-0 group-hover:opacity-100 transition-all p-1 hover:bg-[#33353A] rounded"
                          >
                            <X size={12} />
                          </button>
                        </div>
                      );
                    })}
                  </div>

                  {addingAgent && (
                    <div className="mt-2 p-2 bg-[#1A1A1E] border border-[#33353A] rounded-lg">
                      <p className="text-[11px] text-[#8A8C93] mb-2 px-1">Select agents to add:</p>
                      <div className="space-y-0.5">
                        {availableAgents
                          .filter((a) => !template.agents.includes(a.id))
                          .map((a) => (
                            <button
                              key={a.id}
                              onClick={() => { addItem("agents", a.id); setAddingAgent(false); }}
                              className="w-full flex items-center gap-2.5 px-2 py-1.5 hover:bg-[#2D2E36] rounded-md text-left transition-colors"
                            >
                              <Bot size={13} className="text-[#5E6AD2] shrink-0" />
                              <span className="text-[13px] text-[#E0E1E6] font-medium">{a.label}</span>
                              <span className="text-[11px] text-[#8A8C93]">{a.description}</span>
                            </button>
                          ))}
                        {availableAgents.filter((a) => !template.agents.includes(a.id)).length === 0 && (
                          <p className="text-[12px] text-[#8A8C93] italic px-2 py-1">All agents already added.</p>
                        )}
                      </div>
                      <button onClick={() => setAddingAgent(false)} className="mt-1.5 px-2 text-[11px] text-[#8A8C93] hover:text-[#E0E1E6] transition-colors">
                        Cancel
                      </button>
                    </div>
                  )}
                </div>

                {/* Skills */}
                <div>
                  <div className="flex items-center justify-between mb-3">
                    <div className="flex items-center gap-2">
                      <Code size={13} className="text-[#4ADE80]" />
                      <span className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase">
                        Skills
                      </span>
                    </div>
                    {availableSkills.length > 0 && (
                      <button
                        onClick={(e) => { e.stopPropagation(); setAddingSkill(true); }}
                        className="text-[11px] text-[#5E6AD2] hover:text-[#6B78E3] flex items-center gap-1 transition-colors"
                      >
                        <Plus size={12} /> Add
                      </button>
                    )}
                  </div>

                  {template.skills.length === 0 && !addingSkill && (
                    <p className="text-[12px] text-[#8A8C93]/50 italic pl-1">No skills configured.</p>
                  )}

                  <div className="space-y-2">
                    {template.skills.map((skill, idx) => (
                      <div
                        key={skill}
                        className="flex items-center gap-3 px-3 py-3 bg-[#1A1A1E] border border-[#33353A] rounded-lg group"
                      >
                        <div className="w-8 h-8 rounded-md bg-[#4ADE80]/12 flex items-center justify-center flex-shrink-0">
                          <Code size={15} className="text-[#4ADE80]" />
                        </div>
                        <div className="flex-1 min-w-0">
                          <div className="text-[13px] font-medium text-[#E0E1E6]">{skill}</div>
                        </div>
                        <button
                          onClick={() => removeItem("skills", idx)}
                          className="text-[#8A8C93] hover:text-[#FF6B6B] opacity-0 group-hover:opacity-100 transition-all p-1 hover:bg-[#33353A] rounded"
                        >
                          <X size={12} />
                        </button>
                      </div>
                    ))}
                  </div>

                  {addingSkill && (() => {
                    const unaddedSkills = availableSkills.filter((s) => !template.skills.includes(s));
                    const filteredSkills = skillSearch.trim()
                      ? unaddedSkills.filter((s) => s.toLowerCase().includes(skillSearch.toLowerCase()))
                      : unaddedSkills;
                    return (
                      <div className="mt-2 bg-[#1A1A1E] border border-[#33353A] rounded-lg overflow-hidden">
                        {/* Search input */}
                        <div className="flex items-center gap-2 px-3 py-2 border-b border-[#33353A]">
                          <Search size={12} className="text-[#8A8C93] shrink-0" />
                          <input
                            type="text"
                            value={skillSearch}
                            onChange={(e) => setSkillSearch(e.target.value)}
                            onKeyDown={(e) => {
                              if (e.key === "Escape") { setAddingSkill(false); setSkillSearch(""); }
                              if (e.key === "Enter" && filteredSkills.length === 1) {
                                addItem("skills", filteredSkills[0]!);
                                setAddingSkill(false);
                                setSkillSearch("");
                              }
                            }}
                            placeholder="Search skills..."
                            autoFocus
                            className="flex-1 bg-transparent outline-none text-[13px] text-[#E0E1E6] placeholder-[#8A8C93]/50"
                          />
                          {skillSearch && (
                            <button
                              onClick={() => setSkillSearch("")}
                              className="text-[#8A8C93] hover:text-[#E0E1E6] transition-colors"
                            >
                              <X size={11} />
                            </button>
                          )}
                        </div>
                        {/* Results list */}
                        <div className="max-h-48 overflow-y-auto custom-scrollbar py-1">
                          {filteredSkills.length > 0 ? (
                            filteredSkills.map((s) => (
                              <button
                                key={s}
                                onClick={() => { addItem("skills", s); setAddingSkill(false); setSkillSearch(""); }}
                                className="w-full flex items-center gap-2.5 px-3 py-2 hover:bg-[#2D2E36] text-left transition-colors"
                              >
                                <div className="w-5 h-5 rounded bg-[#4ADE80]/10 flex items-center justify-center flex-shrink-0">
                                  <Code size={11} className="text-[#4ADE80]" />
                                </div>
                                <span className="text-[13px] text-[#E0E1E6]">{s}</span>
                              </button>
                            ))
                          ) : (
                            <p className="text-[12px] text-[#8A8C93] italic px-3 py-3">
                              {unaddedSkills.length === 0 ? "All skills already added." : "No skills match."}
                            </p>
                          )}
                        </div>
                        {/* Footer */}
                        <div className="border-t border-[#33353A] px-3 py-2 flex items-center justify-between">
                          <span className="text-[11px] text-[#8A8C93]">
                            {filteredSkills.length} of {unaddedSkills.length} skill{unaddedSkills.length !== 1 ? "s" : ""}
                          </span>
                          <button
                            onClick={() => { setAddingSkill(false); setSkillSearch(""); }}
                            className="text-[11px] text-[#8A8C93] hover:text-[#E0E1E6] transition-colors"
                          >
                            Cancel
                          </button>
                        </div>
                      </div>
                    );
                  })()}
                </div>

                {/* MCP Servers */}
                <div>
                  <div className="flex items-center justify-between mb-3">
                    <div className="flex items-center gap-2">
                      <Server size={13} className="text-[#F59E0B]" />
                      <span className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase">
                        MCP Servers
                      </span>
                    </div>
                    {availableMcpServers.length > 0 && (
                      <button
                        onClick={(e) => { e.stopPropagation(); setAddingMcp(true); }}
                        className="text-[11px] text-[#5E6AD2] hover:text-[#6B78E3] flex items-center gap-1 transition-colors"
                      >
                        <Plus size={12} /> Add
                      </button>
                    )}
                  </div>

                  {template.mcp_servers.length === 0 && !addingMcp && (
                    <p className="text-[12px] text-[#8A8C93]/50 italic pl-1">No MCP servers configured.</p>
                  )}

                  <div className="space-y-2">
                    {template.mcp_servers.map((srv, idx) => (
                      <div
                        key={srv}
                        className="flex items-center gap-3 px-3 py-3 bg-[#1A1A1E] border border-[#33353A] rounded-lg group"
                      >
                        <div className="w-8 h-8 rounded-md bg-[#F59E0B]/12 flex items-center justify-center flex-shrink-0">
                          <Server size={15} className="text-[#F59E0B]" />
                        </div>
                        <div className="flex-1 min-w-0">
                          <div className="text-[13px] font-medium text-[#E0E1E6]">{srv}</div>
                        </div>
                        <button
                          onClick={() => removeItem("mcp_servers", idx)}
                          className="text-[#8A8C93] hover:text-[#FF6B6B] opacity-0 group-hover:opacity-100 transition-all p-1 hover:bg-[#33353A] rounded"
                        >
                          <X size={12} />
                        </button>
                      </div>
                    ))}
                  </div>

                  {addingMcp && (
                    <div className="mt-2 p-2 bg-[#1A1A1E] border border-[#33353A] rounded-lg max-h-48 overflow-y-auto custom-scrollbar">
                      <p className="text-[11px] text-[#8A8C93] mb-2 px-1">Select an MCP server:</p>
                      <div className="space-y-0.5">
                        {availableMcpServers
                          .filter((s) => !template.mcp_servers.includes(s))
                          .map((s) => (
                            <button
                              key={s}
                              onClick={() => { addItem("mcp_servers", s); setAddingMcp(false); }}
                              className="w-full flex items-center gap-2.5 px-2 py-1.5 hover:bg-[#2D2E36] rounded-md text-left transition-colors"
                            >
                              <Server size={12} className="text-[#F59E0B] shrink-0" />
                              <span className="text-[13px] text-[#E0E1E6]">{s}</span>
                            </button>
                          ))}
                        {availableMcpServers.filter((s) => !template.mcp_servers.includes(s)).length === 0 && (
                          <p className="text-[12px] text-[#8A8C93] italic px-2 py-1">All MCP servers already added.</p>
                        )}
                      </div>
                      <button onClick={() => setAddingMcp(false)} className="mt-1.5 px-2 text-[11px] text-[#8A8C93] hover:text-[#E0E1E6] transition-colors">
                        Cancel
                      </button>
                    </div>
                  )}
                </div>

                {/* Project Files */}
                <div>
                  <div className="flex items-center justify-between mb-3">
                    <div className="flex items-center gap-2">
                      <FileText size={13} className="text-[#5E6AD2]" />
                      <span className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase">
                        Project Files
                      </span>
                    </div>
                    <button
                      onClick={(e) => { e.stopPropagation(); setAddingFile(true); setCustomFilename(""); }}
                      className="text-[11px] text-[#5E6AD2] hover:text-[#6B78E3] flex items-center gap-1 transition-colors"
                    >
                      <Plus size={12} /> Add
                    </button>
                  </div>

                  {template.project_files.length === 0 && !addingFile && (
                    <p className="text-[12px] text-[#8A8C93]/50 italic pl-1">No project files configured.</p>
                  )}

                  <div className="space-y-2">
                    {template.project_files.map((pf, idx) => {
                      const isExpanded = expandedFile === pf.filename;
                      return (
                        <div
                          key={pf.filename}
                          className="bg-[#1A1A1E] border border-[#33353A] rounded-lg overflow-hidden group"
                        >
                          {/* File row */}
                          <div className="flex items-center gap-3 px-3 py-3">
                            <div className="w-8 h-8 rounded-md bg-[#5E6AD2]/15 flex items-center justify-center flex-shrink-0">
                              <FileText size={15} className="text-[#5E6AD2]" />
                            </div>
                            <button
                              className="flex-1 min-w-0 text-left flex items-center gap-1.5"
                              onClick={() => setExpandedFile(isExpanded ? null : pf.filename)}
                            >
                              <span className="text-[13px] font-medium text-[#E0E1E6] font-mono">{pf.filename}</span>
                              {pf.content && (
                                <span className="text-[11px] text-[#8A8C93]">
                                  · {pf.content.split("\n").length} line{pf.content.split("\n").length !== 1 ? "s" : ""}
                                </span>
                              )}
                              <ChevronRight
                                size={12}
                                className={`text-[#8A8C93] ml-auto transition-transform ${isExpanded ? "rotate-90" : ""}`}
                              />
                            </button>
                            <button
                              onClick={() => removeProjectFile(idx)}
                              className="text-[#8A8C93] hover:text-[#FF6B6B] opacity-0 group-hover:opacity-100 transition-all p-1 hover:bg-[#33353A] rounded flex-shrink-0"
                            >
                              <X size={12} />
                            </button>
                          </div>

                          {/* Expanded content editor */}
                          {isExpanded && (
                            <div className="border-t border-[#33353A] px-3 pb-3 pt-2">
                              <textarea
                                value={pf.content}
                                onChange={(e) => updateProjectFileContent(pf.filename, e.target.value)}
                                placeholder={`Content for ${pf.filename}...`}
                                rows={10}
                                className="w-full bg-[#111114] border border-[#33353A] hover:border-[#44474F] focus:border-[#5E6AD2] rounded-md px-3 py-2 text-[12px] text-[#E0E1E6] placeholder-[#8A8C93]/40 outline-none resize-y transition-colors font-mono leading-relaxed"
                                onClick={(e) => e.stopPropagation()}
                              />
                            </div>
                          )}
                        </div>
                      );
                    })}
                  </div>

                  {/* Add file picker */}
                  {addingFile && (
                    <div className="mt-2 p-3 bg-[#1A1A1E] border border-[#33353A] rounded-lg">
                      <p className="text-[11px] text-[#8A8C93] mb-3 px-1">Choose a file to add:</p>

                      {/* Known project files */}
                      {KNOWN_PROJECT_FILES.filter(
                        (kf) => !template.project_files.some((f) => f.filename === kf.filename)
                      ).length > 0 && (
                        <div className="space-y-0.5 mb-3">
                          {KNOWN_PROJECT_FILES
                            .filter((kf) => !template.project_files.some((f) => f.filename === kf.filename))
                            .map((kf) => (
                              <button
                                key={kf.filename}
                                onClick={() => { addProjectFile(kf.filename, ""); setAddingFile(false); }}
                                className="w-full flex items-center gap-2.5 px-2 py-1.5 hover:bg-[#2D2E36] rounded-md text-left transition-colors"
                              >
                                <FileText size={12} className="text-[#5E6AD2] shrink-0" />
                                <span className="text-[13px] text-[#E0E1E6] font-mono font-medium">{kf.label}</span>
                                <span className="text-[11px] text-[#8A8C93]">{kf.hint}</span>
                              </button>
                            ))}
                        </div>
                      )}

                      {/* From file templates */}
                      {availableFileTemplates.length > 0 && (
                        <>
                          <p className="text-[10px] font-semibold text-[#8A8C93] uppercase tracking-wider px-1 mb-1.5">
                            From file template
                          </p>
                          <div className="space-y-0.5 mb-3 max-h-32 overflow-y-auto custom-scrollbar">
                            {availableFileTemplates.map((t) => (
                              <button
                                key={t}
                                onClick={() => addFromFileTemplate(t, "CLAUDE.md")}
                                className="w-full flex items-center gap-2.5 px-2 py-1.5 hover:bg-[#2D2E36] rounded-md text-left transition-colors"
                              >
                                <LayoutTemplate size={12} className="text-[#8B5CF6] shrink-0" />
                                <span className="text-[13px] text-[#E0E1E6]">{t}</span>
                                <span className="text-[11px] text-[#8A8C93] ml-auto">→ CLAUDE.md</span>
                              </button>
                            ))}
                          </div>
                        </>
                      )}

                      {/* Custom filename */}
                      <div className="flex items-center gap-2 pt-2 border-t border-[#33353A]">
                        <input
                          type="text"
                          value={customFilename}
                          onChange={(e) => setCustomFilename(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === "Enter" && customFilename.trim()) {
                              addProjectFile(customFilename.trim(), "");
                              setAddingFile(false);
                              setCustomFilename("");
                            }
                            if (e.key === "Escape") { setAddingFile(false); setCustomFilename(""); }
                          }}
                          placeholder="Custom filename, e.g. README.md"
                          className="flex-1 bg-[#111114] border border-[#33353A] focus:border-[#5E6AD2] rounded px-2.5 py-1.5 text-[12px] text-[#E0E1E6] placeholder-[#8A8C93]/40 outline-none font-mono"
                          autoFocus
                        />
                        <button
                          onClick={() => {
                            if (customFilename.trim()) {
                              addProjectFile(customFilename.trim(), "");
                              setCustomFilename("");
                            }
                            setAddingFile(false);
                          }}
                          disabled={!customFilename.trim()}
                          className="px-2.5 py-1.5 text-[12px] bg-[#5E6AD2] hover:bg-[#6B78E3] text-white rounded disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
                        >
                          Add
                        </button>
                        <button
                          onClick={() => { setAddingFile(false); setCustomFilename(""); }}
                          className="px-2 py-1.5 text-[12px] text-[#8A8C93] hover:text-[#E0E1E6] transition-colors"
                        >
                          Cancel
                        </button>
                      </div>
                    </div>
                  )}
                </div>

                {/* Applied to */}
                {!isCreating && appliedProjects.length > 0 && (
                  <div>
                    <div className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase mb-3">
                      Applied to
                    </div>
                    <div className="flex flex-wrap gap-2">
                      {appliedProjects.map((p) => (
                        <span
                          key={p.name}
                          className="px-2.5 py-1 bg-[#2D2E36] border border-[#3A3B42] rounded-md text-[12px] text-[#C8CAD0] font-medium"
                        >
                          {p.name}
                        </span>
                      ))}
                    </div>
                  </div>
                )}

                {/* Delete */}
                {!isCreating && selectedName && !dirty && (
                  <div className="pt-2 border-t border-[#33353A] flex items-center justify-between">
                    <p className="text-[11px] text-[#8A8C93]/60">
                      Deleting this template will not affect projects that used it.
                    </p>
                    {confirmDelete === selectedName ? (
                      <div className="flex items-center gap-2">
                        <span className="text-[11px] text-[#8A8C93]">Are you sure?</span>
                        <button
                          onClick={() => handleDelete(selectedName)}
                          className="px-2.5 py-1 text-[12px] font-medium text-white bg-[#FF6B6B] hover:bg-[#ff5252] rounded transition-colors"
                        >
                          Delete
                        </button>
                        <button
                          onClick={() => setConfirmDelete(null)}
                          className="px-2.5 py-1 text-[12px] text-[#8A8C93] hover:text-[#E0E1E6] bg-[#2D2E36] hover:bg-[#33353A] rounded transition-colors"
                        >
                          Cancel
                        </button>
                      </div>
                    ) : (
                      <button
                        onClick={() => handleDelete(selectedName)}
                        className="flex items-center gap-1.5 px-3 py-1.5 text-[#8A8C93] hover:text-[#FF6B6B] text-[12px] transition-colors"
                      >
                        <Trash2 size={12} /> Delete
                      </button>
                    )}
                  </div>
                )}
              </div>
            </div>
          </div>
        ) : (
          /* Empty state */
          <div className="flex-1 flex flex-col items-center justify-center p-8 text-center">
            <div className="w-14 h-14 mx-auto mb-4 rounded-full border border-dashed border-[#44474F] flex items-center justify-center text-[#8A8C93]">
              <LayoutTemplate size={24} strokeWidth={1.5} />
            </div>
            <h2 className="text-[16px] font-semibold text-[#E0E1E6] mb-2">No template selected</h2>
            <p className="text-[13px] text-[#8A8C93] max-w-sm leading-relaxed mb-6">
              Project Templates capture agents, skills, MCP servers, and project files
              that can be applied to new or existing projects.
            </p>
            <button
              onClick={startCreate}
              className="flex items-center gap-2 px-4 py-2 bg-[#5E6AD2] hover:bg-[#6B78E3] text-white text-[13px] font-medium rounded shadow-sm transition-colors"
            >
              <Plus size={14} /> New Template
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
