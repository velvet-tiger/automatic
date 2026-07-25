import { useState, useEffect, useCallback, useRef } from "react";
import { ICONS } from "../../lib/icons";
import { useRecentlyAdded } from "../../lib/useRecentlyAdded";
import { RecentlyAddedSectionLabel, RecentlyAddedDivider } from "../../components/RecentlyAddedMarker";
import { AuthorSection, type AuthorDescriptor } from "../../components/AuthorPanel";
import { SkillSelector } from "../../components/SkillSelector";
import { AgentSelector, AgentInfo } from "../../components/AgentSelector";
import { McpSelector } from "../../components/McpSelector";
import { invoke } from "@tauri-apps/api/core";
import {
  Plus,
  X,
  Check,
  Trash2,
  LayoutTemplate,
  Copy,
  ScrollText,
  Edit2,
  Files,
  FolderPlus,
  Bot,
  Search,
  Terminal,
  Folder,
  Webhook,
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
  /** Workspace sub-agent IDs to include when this template is applied to a project. */
  user_agents: string[];
  /** Workspace command names to include when this template is applied to a project. */
  user_commands: string[];
  /** Hook machine names to include when this template is applied to a project. */
  hooks: string[];
  project_files: TemplateProjectFile[];
  /** Single unified project instruction content (written to CLAUDE.md / AGENTS.md etc.) */
  unified_instruction?: string;
  /** Rule IDs attached to the unified instruction */
  unified_rules?: string[];
  /** Author/provider metadata — present on Discover-imported templates. */
  _author?: AuthorDescriptor;
}


interface UserCommandEntry {
  id: string;
  description: string;
}

interface Project {
  name: string;
  description: string;
  directory: string;
  skills: string[];
  mcp_servers: string[];
  providers: string[];
  agents: string[];
  user_agents?: string[];
  user_commands?: string[];
  hooks?: string[];
}

interface HookEntry {
  id: string;
  name: string;
  agent: string;
  event: string;
  plugin_id?: string | null;
}

const SIDEBAR_MIN = 180;
const SIDEBAR_MAX = 420;
const SIDEBAR_DEFAULT = 220;

function SubAgentSelector({
  agentIds,
  available,
  onAdd,
  onRemove,
}: {
  agentIds: string[];
  available: { id: string; name: string }[];
  onAdd: (id: string) => void;
  onRemove: (idx: number) => void;
}) {
  const [adding, setAdding] = useState(false);
  const [search, setSearch] = useState("");

  const unadded = available.filter(a => !agentIds.includes(a.id));
  const filtered = search.trim()
    ? unadded.filter(a =>
        a.name.toLowerCase().includes(search.toLowerCase()) ||
        a.id.toLowerCase().includes(search.toLowerCase())
      )
    : unadded;

  function handleAdd(id: string) {
    onAdd(id);
    setAdding(false);
    setSearch("");
  }

  function handleCancel() {
    setAdding(false);
    setSearch("");
  }

  return (
    <div>
      {/* Header */}
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-2">
          <Bot size={13} className="text-icon-agent" />
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
            Sub-Agents
          </span>
        </div>
        <button
          onClick={(e) => { e.stopPropagation(); setAdding(true); }}
          className="text-[11px] text-brand hover:text-text-base flex items-center gap-1 px-2 py-1 rounded border border-brand/50 hover:border-brand hover:bg-brand/15 transition-all"
        >
          <Plus size={11} /> Add
        </button>
      </div>

      {/* Empty state */}
      {agentIds.length === 0 && !adding && (
        <p className="text-[12px] text-text-muted italic pl-1">No sub-agents configured.</p>
      )}

      {/* Current list */}
      <div className="space-y-2">
        {agentIds.map((id, idx) => {
          const agent = available.find(a => a.id === id);
          return (
            <div key={id} className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden">
              <div className="flex items-center gap-3 px-3 py-3 group">
                <Bot size={20} className="text-icon-agent flex-shrink-0" />
                <div className="flex-1 min-w-0">
                  <div className="text-[13px] font-medium text-text-base">{agent?.name ?? id}</div>
                </div>
                <button
                  onClick={(e) => { e.stopPropagation(); onRemove(idx); }}
                  className="text-text-muted hover:text-danger opacity-0 group-hover:opacity-100 transition-all p-1 hover:bg-surface rounded flex-shrink-0"
                >
                  <Trash2 size={12} />
                </button>
              </div>
            </div>
          );
        })}
      </div>

      {/* Searchable add dropdown */}
      {adding && (
        <div className="mt-2 bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden">
          <div className="flex items-center gap-2 px-3 py-2 border-b border-border-strong/40">
            <Search size={12} className="text-text-muted shrink-0" />
            <input
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") handleCancel();
                if (e.key === "Enter" && filtered.length === 1) handleAdd(filtered[0]!.id);
              }}
              placeholder="Search sub-agents..."
              autoFocus
              className="flex-1 bg-transparent outline-none text-[13px] text-text-base placeholder-text-muted/50"
            />
            {search && (
              <button onClick={() => setSearch("")} className="text-text-muted hover:text-text-base transition-colors">
                <X size={11} />
              </button>
            )}
          </div>
          <div className="max-h-48 overflow-y-auto custom-scrollbar py-1">
            {filtered.length > 0 ? (
              filtered.map((a) => (
                <button
                  key={a.id}
                  onClick={() => handleAdd(a.id)}
                  className="w-full flex items-center gap-2.5 px-3 py-2 hover:bg-bg-sidebar text-left transition-colors"
                >
                  <Bot size={14} className="text-icon-agent flex-shrink-0" />
                  <span className="text-[13px] text-text-base font-medium">{a.name}</span>
                </button>
              ))
            ) : (
              <p className="text-[12px] text-text-muted italic px-3 py-3">
                {unadded.length === 0 ? "All sub-agents already added." : "No sub-agents match."}
              </p>
            )}
          </div>
          <div className="border-t border-border-strong/40 px-3 py-2 flex items-center justify-between">
            <span className="text-[11px] text-text-muted">
              {filtered.length} of {unadded.length} sub-agent{unadded.length !== 1 ? "s" : ""}
            </span>
            <button onClick={handleCancel} className="text-[11px] text-text-muted hover:text-text-base transition-colors">
              Cancel
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

function CommandSelector({
  commandIds,
  available,
  onAdd,
  onRemove,
}: {
  commandIds: string[];
  available: UserCommandEntry[];
  onAdd: (id: string) => void;
  onRemove: (idx: number) => void;
}) {
  const [adding, setAdding] = useState(false);
  const [search, setSearch] = useState("");

  const unadded = available.filter(a => !commandIds.includes(a.id));
  const filtered = search.trim()
    ? unadded.filter(a =>
        a.id.toLowerCase().includes(search.toLowerCase()) ||
        a.description.toLowerCase().includes(search.toLowerCase())
      )
    : unadded;

  function handleAdd(id: string) {
    onAdd(id);
    setAdding(false);
    setSearch("");
  }

  function handleCancel() {
    setAdding(false);
    setSearch("");
  }

  return (
    <div>
      {/* Header */}
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-2">
          <Terminal size={13} className="text-icon-agent" />
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
            Commands
          </span>
        </div>
        <button
          onClick={(e) => { e.stopPropagation(); setAdding(true); }}
          className="text-[11px] text-brand hover:text-text-base flex items-center gap-1 px-2 py-1 rounded border border-brand/50 hover:border-brand hover:bg-brand/15 transition-all"
        >
          <Plus size={11} /> Add
        </button>
      </div>

      {/* Empty state */}
      {commandIds.length === 0 && !adding && (
        <p className="text-[12px] text-text-muted italic pl-1">No commands configured.</p>
      )}

      {/* Current list */}
      <div className="space-y-2">
        {commandIds.map((id, idx) => {
          const cmd = available.find(a => a.id === id);
          return (
            <div key={id} className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden">
              <div className="flex items-center gap-3 px-3 py-3 group">
                <Terminal size={20} className="text-icon-agent flex-shrink-0" />
                <div className="flex-1 min-w-0">
                  <div className="text-[13px] font-medium text-text-base">/{id}</div>
                  {cmd?.description && (
                    <div className="text-[11px] text-text-muted truncate">{cmd.description}</div>
                  )}
                </div>
                <button
                  onClick={(e) => { e.stopPropagation(); onRemove(idx); }}
                  className="text-text-muted hover:text-danger opacity-0 group-hover:opacity-100 transition-all p-1 hover:bg-surface rounded flex-shrink-0"
                >
                  <Trash2 size={12} />
                </button>
              </div>
            </div>
          );
        })}
      </div>

      {/* Searchable add dropdown */}
      {adding && (
        <div className="mt-2 bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden">
          <div className="flex items-center gap-2 px-3 py-2 border-b border-border-strong/40">
            <Search size={12} className="text-text-muted shrink-0" />
            <input
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") handleCancel();
                if (e.key === "Enter" && filtered.length === 1) handleAdd(filtered[0]!.id);
              }}
              placeholder="Search commands..."
              autoFocus
              className="flex-1 bg-transparent outline-none text-[13px] text-text-base placeholder-text-muted/50"
            />
            {search && (
              <button onClick={() => setSearch("")} className="text-text-muted hover:text-text-base transition-colors">
                <X size={11} />
              </button>
            )}
          </div>
          <div className="max-h-48 overflow-y-auto custom-scrollbar py-1">
            {filtered.length > 0 ? (
              filtered.map((a) => (
                <button
                  key={a.id}
                  onClick={() => handleAdd(a.id)}
                  className="w-full flex items-center gap-2.5 px-3 py-2 hover:bg-bg-sidebar text-left transition-colors"
                >
                  <Terminal size={14} className="text-icon-agent flex-shrink-0" />
                  <div className="min-w-0">
                    <span className="text-[13px] text-text-base font-medium">/{a.id}</span>
                    {a.description && (
                      <span className="text-[11px] text-text-muted ml-2">{a.description}</span>
                    )}
                  </div>
                </button>
              ))
            ) : (
              <p className="text-[12px] text-text-muted italic px-3 py-3">
                {unadded.length === 0 ? "All commands already added." : "No commands match."}
              </p>
            )}
          </div>
          <div className="border-t border-border-strong/40 px-3 py-2 flex items-center justify-between">
            <span className="text-[11px] text-text-muted">
              {filtered.length} of {unadded.length} command{unadded.length !== 1 ? "s" : ""}
            </span>
            <button onClick={handleCancel} className="text-[11px] text-text-muted hover:text-text-base transition-colors">
              Cancel
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

function HookSelector({
  hookIds,
  available,
  onAdd,
  onRemove,
}: {
  hookIds: string[];
  available: HookEntry[];
  onAdd: (id: string) => void;
  onRemove: (idx: number) => void;
}) {
  const [adding, setAdding] = useState(false);
  const [search, setSearch] = useState("");

  const unadded = available.filter((a) => !hookIds.includes(a.id));
  const filtered = search.trim()
    ? unadded.filter(
        (a) =>
          a.id.toLowerCase().includes(search.toLowerCase()) ||
          a.name.toLowerCase().includes(search.toLowerCase()) ||
          a.event.toLowerCase().includes(search.toLowerCase()) ||
          a.agent.toLowerCase().includes(search.toLowerCase()),
      )
    : unadded;

  function handleAdd(id: string) {
    onAdd(id);
    setAdding(false);
    setSearch("");
  }

  function handleCancel() {
    setAdding(false);
    setSearch("");
  }

  return (
    <div>
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-2">
          <Webhook size={13} className="text-icon-skill" />
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
            Hooks
          </span>
        </div>
        <button
          onClick={(e) => { e.stopPropagation(); setAdding(true); }}
          className="text-[11px] text-brand hover:text-text-base flex items-center gap-1 px-2 py-1 rounded border border-brand/50 hover:border-brand hover:bg-brand/15 transition-all"
        >
          <Plus size={11} /> Add
        </button>
      </div>

      {hookIds.length === 0 && !adding && (
        <p className="text-[12px] text-text-muted italic pl-1">No hooks configured.</p>
      )}

      <div className="space-y-2">
        {hookIds.map((id, idx) => {
          const hook = available.find((a) => a.id === id);
          return (
            <div key={id} className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden">
              <div className="flex items-center gap-3 px-3 py-3 group">
                <Webhook size={20} className="text-icon-skill flex-shrink-0" />
                <div className="flex-1 min-w-0">
                  <div className="text-[13px] font-medium text-text-base truncate">
                    {hook?.name ?? id}
                  </div>
                  <div className="text-[11px] text-text-muted truncate">
                    {hook ? `${hook.agent} · ${hook.event}` : "Hook missing from library"}
                  </div>
                </div>
                <button
                  onClick={(e) => { e.stopPropagation(); onRemove(idx); }}
                  className="text-text-muted hover:text-danger opacity-0 group-hover:opacity-100 transition-all p-1 hover:bg-surface rounded flex-shrink-0"
                >
                  <Trash2 size={12} />
                </button>
              </div>
            </div>
          );
        })}
      </div>

      {adding && (
        <div className="mt-2 bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden">
          <div className="flex items-center gap-2 px-3 py-2 border-b border-border-strong/40">
            <Search size={12} className="text-text-muted shrink-0" />
            <input
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") handleCancel();
                if (e.key === "Enter" && filtered.length === 1) handleAdd(filtered[0]!.id);
              }}
              placeholder="Search hooks..."
              autoFocus
              className="flex-1 bg-transparent outline-none text-[13px] text-text-base placeholder-text-muted/50"
            />
            {search && (
              <button onClick={() => setSearch("")} className="text-text-muted hover:text-text-base transition-colors">
                <X size={11} />
              </button>
            )}
          </div>
          <div className="max-h-48 overflow-y-auto custom-scrollbar py-1">
            {filtered.length > 0 ? (
              filtered.map((a) => (
                <button
                  key={a.id}
                  onClick={() => handleAdd(a.id)}
                  className="w-full flex items-center gap-2.5 px-3 py-2 hover:bg-bg-sidebar text-left transition-colors"
                >
                  <Webhook size={14} className="text-icon-skill flex-shrink-0" />
                  <div className="min-w-0 flex-1">
                    <div className="text-[13px] text-text-base font-medium truncate">{a.name}</div>
                    <div className="text-[11px] text-text-muted truncate">
                      {a.agent} · {a.event}
                    </div>
                  </div>
                </button>
              ))
            ) : (
              <p className="text-[12px] text-text-muted italic px-3 py-3">
                {unadded.length === 0 ? "All hooks already added." : "No hooks match."}
              </p>
            )}
          </div>
          <div className="border-t border-border-strong/40 px-3 py-2 flex items-center justify-between">
            <span className="text-[11px] text-text-muted">
              {filtered.length} of {unadded.length} hook{unadded.length !== 1 ? "s" : ""}
            </span>
            <button onClick={handleCancel} className="text-[11px] text-text-muted hover:text-text-base transition-colors">
              Cancel
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

function emptyTemplate(name: string): ProjectTemplate {
  return { name, description: "", skills: [], mcp_servers: [], providers: [], agents: [], user_agents: [], user_commands: [], hooks: [], project_files: [], unified_instruction: "", unified_rules: ["automatic-service"] };
}

// Derive a colour for the sidebar icon box based on what's in the template
function templateAccent(t: ProjectTemplate): { bg: string; icon: string } {
  if (t.skills.length >= t.mcp_servers.length && t.skills.length > 0)
    return { bg: "bg-icon-skill/15", icon: "text-icon-skill" };
  if (t.mcp_servers.length > 0)
    return { bg: "bg-icon-mcp/15", icon: "text-icon-mcp" };
  return { bg: "bg-icon-agent/15", icon: "text-icon-agent" };
}


export default function Templates({
  initialTemplate,
  onCreateProjectFromTemplate,
  onNavigateToProject,
}: {
  initialTemplate?: string | null;
  onCreateProjectFromTemplate?: (templateName: string) => void;
  onNavigateToProject?: (projectName: string) => void;
}) {
  const [templates, setTemplates] = useState<string[]>([]);
  const [recentRefresh, setRecentRefresh] = useState(0);
  const recentIds = useRecentlyAdded("project_templates", recentRefresh);
  // Map of template name → loaded data (for sidebar summaries)
  const [templateData, setTemplateData] = useState<Record<string, ProjectTemplate>>({});

  const [selectedName, setSelectedName] = useState<string | null>(initialTemplate ?? null);
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
  const [availableUserAgents, setAvailableUserAgents] = useState<{ id: string; name: string }[]>([]);
  const [availableUserCommands, setAvailableUserCommands] = useState<UserCommandEntry[]>([]);
  const [availableHooks, setAvailableHooks] = useState<HookEntry[]>([]);
  const [availableSkills, setAvailableSkills] = useState<string[]>([]);
  const [availableMcpServers, setAvailableMcpServers] = useState<string[]>([]);
  const [availableFileTemplates, setAvailableFileTemplates] = useState<string[]>([]);
  const [availableRules, setAvailableRules] = useState<{ id: string; name: string }[]>([]);

  // Unified instruction editing state
  const [unifiedEditing, setUnifiedEditing] = useState(false);
  const [showUnifiedTemplatePicker, setShowUnifiedTemplatePicker] = useState(false);

  // All projects (for "Applied to" + "Apply to project")
  const [allProjects, setAllProjects] = useState<Project[]>([]);
  const [showApplyPicker, setShowApplyPicker] = useState(false);
  const [applyStatus, setApplyStatus] = useState<string | null>(null);
  const [applyTargetProject, setApplyTargetProject] = useState<string | null>(null);

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
    loadAvailableUserAgents();
    loadAvailableUserCommands();
    loadAvailableHooks();
    loadAvailableSkills();
    loadAvailableMcpServers();
    loadAvailableFileTemplates();
    loadAvailableRules();
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
      const names: string[] = await invoke("get_templates");
      names.sort((a, b) => a.localeCompare(b));
      setTemplates(names);
      setError(null);
      // Load all template data for sidebar summaries
      const entries = await Promise.all(
        names.map(async (name) => {
          try {
            const raw: string = await invoke("read_template", { name });
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
      result.sort((a, b) => a.label.localeCompare(b.label));
      setAvailableAgents(result);
    } catch { /* ignore */ }
  };

  const loadAvailableUserAgents = async () => {
    try {
      const result: { id: string; name: string }[] = await invoke("get_subagents");
      result.sort((a, b) => a.name.localeCompare(b.name));
      setAvailableUserAgents(result);
    } catch { /* ignore */ }
  };

  const loadAvailableUserCommands = async () => {
    try {
      const result: UserCommandEntry[] = await invoke("get_user_commands");
      result.sort((a, b) => a.id.localeCompare(b.id));
      setAvailableUserCommands(result);
    } catch { /* ignore */ }
  };

  const loadAvailableHooks = async () => {
    try {
      const result: HookEntry[] = await invoke("get_hooks");
      result.sort((a, b) => a.id.localeCompare(b.id));
      setAvailableHooks(result);
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
      const result: string[] = await invoke("get_instructions");
      setAvailableFileTemplates(result.sort());
    } catch { /* ignore */ }
  };

  const loadAvailableRules = async () => {
    try {
      const result: { id: string; name: string }[] = await invoke("get_rules");
      setAvailableRules(result.sort((a, b) => a.name.localeCompare(b.name)));
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
      const raw: string = await invoke("read_template", { name });
      const parsed: ProjectTemplate = JSON.parse(raw);
      setSelectedName(name);
      setTemplate({
        name: parsed.name || name,
        description: parsed.description || "",
        skills: parsed.skills || [],
        mcp_servers: parsed.mcp_servers || [],
        providers: parsed.providers || [],
        agents: parsed.agents || [],
        user_agents: parsed.user_agents || [],
        user_commands: parsed.user_commands || [],
        hooks: parsed.hooks || [],
        project_files: parsed.project_files || [],
        unified_instruction: parsed.unified_instruction || "",
        unified_rules: parsed.unified_rules === undefined ? ["automatic-service"] : parsed.unified_rules,
        _author: parsed._author,
      });
      setUnifiedEditing(false);
      setDirty(false);
      setIsCreating(false);
      setError(null);
      setShowApplyPicker(false);
      setApplyStatus(null);
      setConfirmDelete(null);
    } catch (err: any) {
      setError(`Failed to read project template: ${err}`);
    }
  };

  const updateField = <K extends keyof ProjectTemplate>(key: K, value: ProjectTemplate[K]) => {
    if (!template) return;
    // Editing the template transfers authorship to the local user — clear any imported author.
    setTemplate({ ...template, [key]: value, _author: undefined });
    setDirty(true);
  };

  const handleSave = async () => {
    if (!template) return;
    const name = isCreating ? newName.trim() : selectedName;
    if (!name) return;
    try {
      setSaveStatus("saving");
      const toSave: ProjectTemplate = { ...template, name };
      await invoke("save_template", { name, data: JSON.stringify(toSave, null, 2) });
      setSelectedName(name);
      if (isCreating) {
        setIsCreating(false);
        await loadTemplates();
        setRecentRefresh(prev => prev + 1);
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
      await invoke("delete_template", { name });
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
      await invoke("save_template", { name: candidate, data: JSON.stringify(copy, null, 2) });
      await loadTemplates();
      await selectTemplate(candidate);
      setError(null);
    } catch (err: any) {
      setError(`Failed to duplicate template: ${err}`);
    }
  };

  // Apply template to a project (merge, non-destructive).
  // `apply_templates_to_project` owns all merging, deduplication, rule
  // attachment, project-file writing and syncing, so this shares one
  // implementation with the project editor's Apply Template action. It reads the
  // template from disk, so unsaved edits are not applied.
  const applyToProject = async (projectName: string) => {
    if (!template || !selectedName) return;
    try {
      const raw: string = await invoke("apply_templates_to_project", {
        projectName,
        templateNames: [selectedName],
      });
      const result: { pending_unified: { content: string; rules: string[] }[] } = JSON.parse(raw);

      // The unified instruction is the one asset the backend hands back instead
      // of writing, so the caller decides where it lands. Rules-only templates
      // yield no content, which keeps existing per-agent instruction files from
      // being overwritten with nothing.
      const mergedContent = result.pending_unified
        .map((e) => e.content)
        .filter((c) => c.trim())
        .join("\n\n---\n\n");
      let instructionError: string | null = null;
      if (mergedContent) {
        try {
          await invoke("save_project_file", {
            name: projectName,
            filename: "_unified",
            content: mergedContent,
          });
        } catch (err: unknown) {
          // Typically no directory or no agents configured yet. Everything else
          // about the apply already succeeded, so report it without discarding.
          instructionError = String(err);
        }
      }

      await loadAllProjects();
      setShowApplyPicker(false);
      setError(
        instructionError
          ? `Applied to "${projectName}", but its instruction could not be written: ${instructionError}`
          : null
      );
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
    setUnifiedEditing(false);
    setShowUnifiedTemplatePicker(false);
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
      await invoke("rename_template", { oldName: selectedName, newName: trimmed });
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

  type ListField = "skills" | "mcp_servers" | "providers" | "agents" | "user_agents" | "user_commands" | "hooks";

  const addItem = (key: ListField, item: string) => {
    if (!template || !item.trim()) return;
    if (template[key].includes(item.trim())) return;
    updateField(key, [...template[key], item.trim()]);
  };

  const removeItem = (key: ListField, idx: number) => {
    if (!template) return;
    updateField(key, template[key].filter((_, i) => i !== idx));
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

  // ── Render ──────────────────────────────────────────────────────────────────

  return (
    <div className="flex h-full w-full bg-bg-base">
      {/* Left sidebar */}
      <div
        className="flex-shrink-0 flex flex-col border-r border-border-strong/40 bg-bg-input/50 relative"
        style={{ width: sidebarWidth }}
      >
        <div className="h-11 px-4 border-b border-border-strong/40 flex justify-between items-center">
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
            Templates
          </span>
          <button
            onClick={startCreate}
            className="text-text-muted hover:text-text-base transition-colors p-1 hover:bg-bg-sidebar rounded"
            title="Create New Template"
          >
            <Plus size={14} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto py-2 custom-scrollbar">
          {templates.length === 0 && !isCreating ? (
            <div className="px-4 py-8 text-center">
              <div className="w-10 h-10 mx-auto mb-3 rounded-full border border-dashed border-border-strong flex items-center justify-center">
                <LayoutTemplate size={16} className="text-text-muted" strokeWidth={1.5} />
              </div>
              <p className="text-[12px] text-text-muted">No templates yet.</p>
              <button
                onClick={startCreate}
                className="mt-3 text-[12px] text-brand hover:text-brand-hover transition-colors"
              >
                Create one
              </button>
            </div>
          ) : (
            <ul className="space-y-1 px-2">
              {isCreating && (
                <li className="flex items-center gap-3 px-3 py-2.5 rounded-lg bg-bg-sidebar">
                  <div className="w-9 h-9 rounded-lg bg-brand/15 flex items-center justify-center flex-shrink-0">
                    <LayoutTemplate size={16} className="text-brand" />
                  </div>
                  <span className="text-[13px] text-text-base italic">New Template...</span>
                </li>
              )}
              {(() => {
                const recentTemplates = templates.filter(n => recentIds.has(n));
                const otherTemplates = templates.filter(n => !recentIds.has(n));
                const renderTemplate = (name: string) => {
                  const td = templateData[name];
                  const isActive = selectedName === name && !isCreating;
                  const accent = td ? templateAccent(td) : { bg: "bg-brand/15", icon: "text-brand" };
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
                            ? "bg-bg-sidebar text-text-base"
                            : "text-text-muted hover:bg-bg-sidebar/60 hover:text-text-base"
                        }`}
                      >
                        <div className={`w-9 h-9 rounded-lg ${accent.bg} flex items-center justify-center flex-shrink-0`}>
                          <LayoutTemplate size={16} className={accent.icon} />
                        </div>
                        <div className="flex-1 min-w-0">
                          <div className={`text-[13px] font-medium truncate ${isActive ? "text-text-base" : "text-text-base"}`}>
                            {name}
                          </div>
                          {parts.length > 0 && (
                            <div className="text-[11px] text-text-muted mt-0.5">
                              {parts.join(" · ")}
                            </div>
                          )}
                        </div>
                      </button>
                      {confirmDelete === name ? (
                        <div className="absolute right-2 top-1/2 -translate-y-1/2 flex items-center gap-1">
                          <button
                            onClick={(e) => handleDelete(name, e)}
                            className="px-1.5 py-0.5 text-[11px] font-medium text-danger hover:bg-danger/15 rounded transition-colors"
                          >
                            Delete
                          </button>
                          <button
                            onClick={(e) => { e.stopPropagation(); setConfirmDelete(null); }}
                            className="p-0.5 text-text-muted hover:text-text-base hover:bg-surface rounded transition-colors"
                          >
                            <X size={11} />
                          </button>
                        </div>
                      ) : (
                        <button
                          onClick={(e) => handleDelete(name, e)}
                          className="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-text-muted hover:text-danger opacity-0 group-hover:opacity-100 hover:bg-surface rounded transition-all"
                          title="Delete template"
                        >
                          <X size={12} />
                        </button>
                      )}
                    </li>
                  );
                };
                return (
                  <>
                    {recentTemplates.length > 0 && <RecentlyAddedSectionLabel />}
                    {recentTemplates.map(renderTemplate)}
                    {recentTemplates.length > 0 && otherTemplates.length > 0 && <RecentlyAddedDivider />}
                    {otherTemplates.map(renderTemplate)}
                  </>
                );
              })()}
            </ul>
          )}
        </div>

        {/* Resize handle */}
        <div
          className="absolute top-0 right-0 w-1 h-full cursor-col-resize hover:bg-brand/40 active:bg-brand/60 transition-colors z-10"
          onMouseDown={onSidebarMouseDown}
        />
      </div>

      {/* Right panel */}
      <div className="flex-1 flex flex-col min-w-0 bg-bg-base">
        {error && (
          <div className="bg-red-500/10 text-red-400 p-3 text-[13px] border-b border-red-500/20 flex items-center justify-between">
            {error}
            <button onClick={() => setError(null)}><X size={14} /></button>
          </div>
        )}

        {template ? (
          <div className="flex-1 flex flex-col h-full">
            {/* Header */}
            <div className="h-11 px-6 border-b border-border-strong/40 flex justify-between items-center">
              <div className="flex items-center gap-3">
                {isCreating ? (
                  <input
                    type="text"
                    placeholder="template-name"
                    value={newName}
                    onChange={(e) => setNewName(e.target.value)}
                    autoFocus
                    className="bg-transparent border-none outline-none text-[14px] font-medium text-text-base placeholder-text-muted/50 w-64"
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
                    className="bg-transparent border-none outline-none text-[14px] font-medium text-text-base placeholder-text-muted/50 w-64"
                  />
                ) : (
                  <h3
                    className="text-[14px] font-medium text-text-base cursor-text"
                    onDoubleClick={startRename}
                    title="Double-click to rename"
                  >
                    {selectedName}
                  </h3>
                )}
              </div>

              <div className="flex items-center gap-2">
                {saveStatus && (
                  <span className={`text-[12px] ${saveStatus === "saving" ? "text-text-muted" : "text-icon-skill"}`}>
                    {saveStatus === "saving" ? "Saving..." : saveStatus}
                  </span>
                )}
                {applyStatus && (
                  <span className="text-[12px] text-icon-skill">{applyStatus}</span>
                )}
                {!isCreating && selectedName && (
                  <>
                    <button
                      onClick={handleDuplicate}
                      className="flex h-[26px] items-center gap-1.5 px-2.5 bg-bg-input hover:bg-surface-hover text-text-base rounded text-[11px] font-medium border border-border-strong transition-colors shadow-sm"
                    >
                      <Copy size={12} /> Duplicate
                    </button>
                    {/* New project from template */}
                    {onCreateProjectFromTemplate && (
                      <button
                        onClick={() => onCreateProjectFromTemplate(selectedName)}
                        className="flex h-[26px] items-center gap-1.5 px-2.5 bg-bg-input hover:bg-surface-hover text-text-base rounded text-[11px] font-medium border border-border-strong transition-colors shadow-sm"
                        title="Create a new project using this template"
                      >
                        <FolderPlus size={12} /> New project...
                      </button>
                    )}
                    {/* Apply to project */}
                    <button
                      onClick={() => {
                        setApplyTargetProject(null);
                        setShowApplyPicker(true);
                      }}
                      className="flex h-[26px] items-center gap-1.5 px-2.5 bg-brand hover:bg-brand-hover text-white rounded text-[11px] font-medium transition-colors shadow-sm"
                    >
                      Apply to project...
                    </button>
                  </>
                )}
                {dirty && (
                    <button
                      onClick={handleSave}
                      disabled={isCreating && !newName.trim()}
                      className="flex h-[26px] items-center gap-1.5 px-2.5 bg-brand hover:bg-brand-hover text-white rounded text-[11px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-sm border border-transparent"
                    >
                    <Check size={12} /> Save
                  </button>
                )}
              </div>
            </div>

            {/* Body */}
            <div className="flex-1 overflow-y-auto p-6 custom-scrollbar" onClick={() => { setConfirmDelete(null); }}>
              <div className="max-w-2xl space-y-8">

                {/* Author */}
                <section className="pb-2 border-b border-border-strong/40">
                  <AuthorSection descriptor={template._author ?? { type: "local" }} />
                </section>

                {/* Description */}
                {(template.description || isCreating || dirty) && (
                  <div>
                    <label className="block text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-2">
                      Description
                    </label>
                    <textarea
                      value={template.description}
                      onChange={(e) => updateField("description", e.target.value)}
                      placeholder="What is this template for? What kind of projects should use it?"
                      rows={2}
                      className="w-full bg-bg-input border border-border-strong/40 hover:border-border-strong focus:border-brand rounded-md px-3 py-2 text-[13px] text-text-base placeholder-text-muted/40 outline-none resize-none transition-colors"
                    />
                  </div>
                )}

                {/* Agents */}
                <AgentSelector
                  agentIds={template.agents}
                  availableAgents={availableAgents}
                  onAdd={(id) => addItem("agents", id)}
                  onRemove={(idx) => removeItem("agents", idx)}
                />

                {/* Sub-Agents */}
                <SubAgentSelector
                  agentIds={template.user_agents}
                  available={availableUserAgents}
                  onAdd={(id) => addItem("user_agents", id)}
                  onRemove={(idx) => removeItem("user_agents", idx)}
                />

                {/* Commands */}
                <CommandSelector
                  commandIds={template.user_commands}
                  available={availableUserCommands}
                  onAdd={(id) => addItem("user_commands", id)}
                  onRemove={(idx) => removeItem("user_commands", idx)}
                />

                {/* Hooks */}
                <HookSelector
                  hookIds={template.hooks}
                  available={availableHooks}
                  onAdd={(id) => addItem("hooks", id)}
                  onRemove={(idx) => removeItem("hooks", idx)}
                />

                {/* Unified Project Instruction */}
                <div>
                  <div className="flex items-center justify-between mb-3">
                    <div className="flex items-center gap-2">
                      <Files size={13} className="text-brand" />
                      <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
                        Unified Project Instruction
                      </span>
                      {template.unified_instruction && template.unified_instruction.trim() && (
                        <span className="text-[10px] text-brand bg-brand/10 px-1.5 py-0.5 rounded">Active</span>
                      )}
                    </div>
                    <div className="flex items-center gap-1.5">
                      {availableFileTemplates.length > 0 && (
                        <button
                          onClick={(e) => { e.stopPropagation(); setShowUnifiedTemplatePicker(!showUnifiedTemplatePicker); }}
                          className="text-[11px] text-text-muted hover:text-text-base flex items-center gap-1 transition-colors px-1.5 py-0.5 hover:bg-bg-sidebar rounded"
                          title="Load from file template"
                        >
                          <LayoutTemplate size={11} /> Template
                        </button>
                      )}
                      {!unifiedEditing ? (
                        <button
                          onClick={() => setUnifiedEditing(true)}
                          className="text-[11px] text-brand hover:text-brand-hover flex items-center gap-1 transition-colors"
                        >
                          <Edit2 size={11} /> Edit
                        </button>
                      ) : (
                        <button
                          onClick={() => setUnifiedEditing(false)}
                          className="text-[11px] text-text-muted hover:text-text-base transition-colors"
                        >
                          Done
                        </button>
                      )}
                    </div>
                  </div>

                  {/* Template picker dropdown */}
                  {showUnifiedTemplatePicker && (
                    <div className="mb-2 p-2 bg-bg-input border border-border-strong/40 rounded-lg" onClick={(e) => e.stopPropagation()}>
                      <p className="text-[10px] text-text-muted mb-1.5 px-1">Load from file template:</p>
                      <div className="space-y-0.5 max-h-32 overflow-y-auto custom-scrollbar">
                        {availableFileTemplates.map((ft) => (
                          <button
                            key={ft}
                            onClick={async () => {
                              try {
                                const content: string = await invoke("read_instruction", { name: ft });
                                updateField("unified_instruction", content);
                                setUnifiedEditing(true);
                              } catch { /* ignore */ }
                              setShowUnifiedTemplatePicker(false);
                            }}
                            className="w-full flex items-center gap-2 px-2 py-1.5 hover:bg-bg-sidebar rounded text-left transition-colors"
                          >
                            <LayoutTemplate size={11} className="text-accent shrink-0" />
                            <span className="text-[12px] text-text-base">{ft}</span>
                          </button>
                        ))}
                      </div>
                      <button
                        onClick={() => setShowUnifiedTemplatePicker(false)}
                        className="mt-1.5 w-full text-[11px] text-text-muted hover:text-text-base transition-colors text-left px-1"
                      >
                        Cancel
                      </button>
                    </div>
                  )}

                  {unifiedEditing ? (
                    <textarea
                      value={template.unified_instruction || ""}
                      onChange={(e) => updateField("unified_instruction", e.target.value)}
                      placeholder="Write project instructions here. This becomes the single unified instruction file (CLAUDE.md / AGENTS.md etc.) when applied to a project."
                      rows={10}
                      className="w-full bg-bg-input-dark border border-border-strong/40 hover:border-border-strong focus:border-brand rounded-md px-3 py-2 text-[12px] text-text-base placeholder-text-muted/40 outline-none resize-y transition-colors font-mono leading-relaxed"
                      onClick={(e) => e.stopPropagation()}
                    />
                  ) : (
                    <div
                      className="min-h-[48px] bg-bg-input border border-border-strong/40 rounded-md px-3 py-2 cursor-pointer hover:border-border-strong transition-colors"
                      onClick={() => setUnifiedEditing(true)}
                    >
                      {template.unified_instruction && template.unified_instruction.trim() ? (
                        <pre className="text-[12px] text-text-base font-mono whitespace-pre-wrap line-clamp-4 leading-relaxed">
                          {template.unified_instruction}
                        </pre>
                      ) : (
                        <span className="text-[12px] text-text-muted italic">
                          No unified instruction yet. Click Edit to write one or load from a template.
                        </span>
                      )}
                    </div>
                  )}

                  {/* Rules selection */}
                  <div className="mt-3 pt-3 border-t border-border-strong/40">
                    <div className="flex items-center gap-2 mb-2">
                      <ScrollText size={12} className="text-accent-hover" />
                      <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Rules</span>
                      {(template.unified_rules || []).length > 0 && (
                        <span className="text-[10px] text-accent-hover bg-accent-hover/10 px-1.5 py-0.5 rounded">
                          {(template.unified_rules || []).length}
                        </span>
                      )}
                    </div>
                    {availableRules.length > 0 ? (
                      <div className="flex flex-wrap gap-1.5">
                        {availableRules.map((rule) => {
                          const isSelected = (template.unified_rules || []).includes(rule.id);
                          return (
                            <button
                              key={rule.id}
                              onClick={() => {
                                const current = template.unified_rules || [];
                                const updated = isSelected
                                  ? current.filter((r) => r !== rule.id)
                                  : [...current, rule.id];
                                updateField("unified_rules", updated);
                              }}
                              className={`px-2.5 py-1 text-[12px] rounded border transition-colors flex items-center gap-1.5 ${
                                isSelected
                                ? "bg-bg-sidebar border-brand/40 font-medium text-text-base"
                                : "bg-bg-sidebar border-border-strong/40 text-text-muted hover:text-text-base hover:border-border-strong"
                              }`}
                            >
                              <ScrollText size={10} className={isSelected ? "text-brand" : ""} />
                              {rule.name}
                              {isSelected && <Check size={10} className="text-brand" />}
                            </button>
                          );
                        })}
                      </div>
                    ) : (
                      <p className="text-[11px] text-text-muted italic">
                        No rules created yet. Create rules in the Rules section to attach them here.
                      </p>
                    )}
                  </div>
                </div>

                {/* Skills */}
                <SkillSelector
                  skills={template.skills}
                  availableSkills={availableSkills}
                  onAdd={(s) => addItem("skills", s)}
                  onRemove={(idx) => removeItem("skills", idx)}
                  showRemoveButtonAlways
                />

                {/* MCP Servers */}
                <McpSelector
                  servers={template.mcp_servers}
                  availableServers={availableMcpServers}
                  onAdd={(s) => addItem("mcp_servers", s)}
                  onRemove={(idx) => removeItem("mcp_servers", idx)}
                  showRemoveButtonAlways
                />

                {/* Applied to */}
                {!isCreating && appliedProjects.length > 0 && (
                  <div>
                    <div className="text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-3">
                      Applied to
                    </div>
                    <div className="flex flex-wrap gap-2">
                      {appliedProjects.map((p) => (
                        <button
                          key={p.name}
                          onClick={() => onNavigateToProject?.(p.name)}
                          className="px-2.5 py-1 bg-bg-sidebar border border-border-strong/40 rounded-md text-[12px] text-text-base font-medium hover:border-border-strong hover:text-text-base hover:bg-bg-hover transition-colors cursor-pointer"
                        >
                          {p.name}
                        </button>
                      ))}
                    </div>
                  </div>
                )}

                {/* Delete */}
                {!isCreating && selectedName && !dirty && (
                  <div className="pt-2 border-t border-border-strong/40 flex items-center justify-between">
                    <p className="text-[11px] text-text-muted">
                      Deleting this template will not affect projects that used it.
                    </p>
                    {confirmDelete === selectedName ? (
                      <div className="flex items-center gap-2">
                        <span className="text-[11px] text-text-muted">Are you sure?</span>
                        <button
                          onClick={() => handleDelete(selectedName)}
                          className="px-2.5 py-1 text-[12px] font-medium text-white bg-danger hover:bg-danger-hover rounded transition-colors"
                        >
                          Delete
                        </button>
                        <button
                          onClick={() => setConfirmDelete(null)}
                          className="px-2.5 py-1 text-[12px] text-text-muted hover:text-text-base bg-bg-sidebar hover:bg-surface rounded transition-colors"
                        >
                          Cancel
                        </button>
                      </div>
                    ) : (
                      <button
                        onClick={() => handleDelete(selectedName)}
                        className="flex items-center gap-1.5 px-3 py-1.5 text-text-muted hover:text-danger text-[12px] transition-colors"
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
            <div className="w-14 h-14 mx-auto mb-4 rounded-2xl bg-icon-agent/15 border border-icon-agent/20 flex items-center justify-center">
              <LayoutTemplate size={24} strokeWidth={1.5} className={ICONS.projectTemplate.iconColor} />
            </div>
            <h2 className="text-[16px] font-semibold text-text-base mb-2">No template selected</h2>
            <p className="text-[13px] text-text-muted max-w-sm leading-relaxed mb-6">
              Project Templates capture agents, skills, MCP servers, and project files
              that can be applied to new or existing projects.
            </p>
            <button
              onClick={startCreate}
              className="flex items-center gap-2 px-4 py-2 bg-brand hover:bg-brand-hover text-white text-[13px] font-medium rounded shadow-sm transition-colors"
            >
              <Plus size={14} /> New Template
            </button>
          </div>
        )}
      </div>

      {/* Apply-to-project modal */}
      {showApplyPicker && template && (
        <ApplyToProjectModal
          projects={[...allProjects].sort((a, b) => a.name.localeCompare(b.name))}
          appliedProjectNames={appliedProjects.map((p) => p.name)}
          selected={applyTargetProject}
          onSelect={setApplyTargetProject}
          onCancel={() => {
            setShowApplyPicker(false);
            setApplyTargetProject(null);
          }}
          onConfirm={() => {
            if (!applyTargetProject) return;
            const target = applyTargetProject;
            setApplyTargetProject(null);
            applyToProject(target);
          }}
        />
      )}
    </div>
  );
}

function ApplyToProjectModal({
  projects,
  appliedProjectNames,
  selected,
  onSelect,
  onCancel,
  onConfirm,
}: {
  projects: Project[];
  appliedProjectNames: string[];
  selected: string | null;
  onSelect: (name: string) => void;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const [filter, setFilter] = useState("");
  const trimmed = filter.trim().toLowerCase();
  const visible = trimmed
    ? projects.filter((p) => p.name.toLowerCase().includes(trimmed))
    : projects;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/50" onClick={onCancel} />
      <div className="relative bg-bg-input border border-border-strong rounded-xl shadow-2xl w-full max-w-md mx-4 flex flex-col max-h-[80vh]">
        <div className="flex items-center justify-between px-5 py-4 border-b border-border-strong/40 flex-shrink-0">
          <h2 className="text-[15px] font-semibold text-text-base">Apply Template to Project</h2>
          <button
            onClick={onCancel}
            className="p-1 text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
          >
            <X size={16} />
          </button>
        </div>

        <div className="px-5 pt-3 pb-2 flex-shrink-0">
          <p className="text-[12px] text-text-muted leading-relaxed mb-3">
            Select a project to apply this template to. Resources will only be added — existing
            project configuration will not be overwritten or removed.
          </p>
          {projects.length > 0 && (
            <div className="flex items-center gap-2 px-3 py-2 bg-bg-base border border-border-strong/40 rounded-md">
              <Search size={12} className="text-text-muted shrink-0" />
              <input
                type="text"
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
                placeholder="Filter projects..."
                autoFocus
                className="flex-1 bg-transparent outline-none text-[13px] text-text-base placeholder-text-muted/50"
              />
              {filter && (
                <button
                  onClick={() => setFilter("")}
                  className="text-text-muted hover:text-text-base transition-colors"
                >
                  <X size={11} />
                </button>
              )}
            </div>
          )}
        </div>

        <div className="flex-1 overflow-y-auto custom-scrollbar px-3 pb-3 min-h-0">
          {projects.length === 0 ? (
            <div className="px-3 py-8 text-[12px] text-text-muted text-center">
              No projects yet. Create a project first.
            </div>
          ) : visible.length === 0 ? (
            <div className="px-3 py-8 text-[12px] text-text-muted text-center">
              No projects match.
            </div>
          ) : (
            <ul className="space-y-1">
              {visible.map((p) => {
                const isSelected = selected === p.name;
                const alreadyApplied = appliedProjectNames.includes(p.name);
                return (
                  <li key={p.name}>
                    <button
                      onClick={() => onSelect(p.name)}
                      onDoubleClick={() => { onSelect(p.name); onConfirm(); }}
                      className={`w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-left transition-colors ${
                        isSelected
                          ? "bg-brand/15 border border-brand/40"
                          : "border border-transparent hover:bg-bg-sidebar"
                      }`}
                    >
                      <Folder size={14} className={isSelected ? "text-brand flex-shrink-0" : "text-text-muted flex-shrink-0"} />
                      <div className="flex-1 min-w-0">
                        <div className="text-[13px] font-medium text-text-base truncate">{p.name}</div>
                        {p.directory && (
                          <div className="text-[11px] text-text-muted truncate">{p.directory}</div>
                        )}
                      </div>
                      {alreadyApplied && (
                        <span className="flex items-center gap-1 text-[10px] text-icon-skill flex-shrink-0">
                          <Check size={11} /> Applied
                        </span>
                      )}
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>

        <div className="flex items-center justify-end gap-2 px-5 py-3 border-t border-border-strong/40 flex-shrink-0">
          <button
            onClick={onCancel}
            className="flex h-[28px] items-center px-3 text-[12px] text-text-muted hover:text-text-base bg-bg-sidebar hover:bg-surface rounded transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={onConfirm}
            disabled={!selected}
            className="flex h-[28px] items-center gap-1.5 px-3 bg-brand hover:bg-brand-hover text-white rounded text-[12px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-sm"
          >
            <Check size={12} /> Apply
          </button>
        </div>
      </div>
    </div>
  );
}
