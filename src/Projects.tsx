import { useState, useEffect, useRef, useCallback } from "react";
import { SkillSelector } from "./SkillSelector";
import { AgentSelector } from "./AgentSelector";
import { McpSelector } from "./McpSelector";
import { MarkdownPreview } from "./MarkdownPreview";
import { useCurrentUser } from "./ProfileContext";
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
  FileText,
  LayoutTemplate,
  Edit2,
  Upload,
  ArrowRightLeft,
  GripVertical,
  Package,
  CheckCircle2,
  AlertCircle,
  ArrowRight,
  ScrollText,
  Files,
  SplitSquareHorizontal,
  Brain,
} from "lucide-react";

interface Project {
  name: string;
  description: string;
  directory: string;
  skills: string[];
  local_skills: string[];
  mcp_servers: string[];
  providers: string[];
  agents: string[];
  created_at: string;
  updated_at: string;
  created_by?: string;
  file_rules?: Record<string, string[]>;
  instruction_mode?: string;
}

interface AgentInfo {
  id: string;
  label: string;
  description: string;
  /** Non-null when this agent cannot have MCP config written by Automatic. */
  mcp_note: string | null;
}

interface DriftedFile {
  path: string;
  reason: "missing" | "modified" | "stale" | "unreadable";
}

interface AgentDrift {
  agent_id: string;
  agent_label: string;
  files: DriftedFile[];
}

interface DriftReport {
  drifted: boolean;
  agents: AgentDrift[];
}

interface ProjectFileInfo {
  filename: string;
  agents: string[];
  exists: boolean;
  target_files?: string[];
}

interface ProjectTemplate {
  name: string;
  description: string;
  skills: string[];
  mcp_servers: string[];
  providers: string[];
  agents: string[];
  unified_instruction?: string;
  unified_rules?: string[];
}

const SIDEBAR_MIN = 160;
const SIDEBAR_MAX = 400;
const SIDEBAR_DEFAULT = 192; // w-48 equivalent

function emptyProject(name: string): Project {
  return {
    name,
    description: "",
    directory: "",
    skills: [],
    local_skills: [],
    mcp_servers: [],
    providers: [],
    agents: [],
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    file_rules: {},
    instruction_mode: "per-agent",
  };
}

interface ProjectsProps {
  initialProject?: string | null;
  onInitialProjectConsumed?: () => void;
}

export default function Projects({ initialProject = null, onInitialProjectConsumed }: ProjectsProps = {}) {
  const { userId } = useCurrentUser();
  const LAST_PROJECT_KEY = "nexus.projects.selected";
  const PROJECT_ORDER_KEY = "nexus.projects.order";
  const [projects, setProjects] = useState<string[]>([]);
  const [selectedName, setSelectedName] = useState<string | null>(() => {
    return localStorage.getItem(LAST_PROJECT_KEY);
  });

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
      // 180px is the global sidebar width in App.tsx
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

  // Drag-and-drop reorder state (pointer-events based)
  const [dragIdx, setDragIdx] = useState<number | null>(null);
  const [dropIdx, setDropIdx] = useState<number | null>(null);
  const dragIdxRef = useRef<number | null>(null);
  const dropIdxRef = useRef<number | null>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const [project, setProject] = useState<Project | null>(null);
  const [dirty, setDirty] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameName, setRenameName] = useState("");

  // Available items to pick from
  const [availableAgents, setAvailableAgents] = useState<AgentInfo[]>([]);
  const [availableSkills, setAvailableSkills] = useState<string[]>([]);
  const [availableMcpServers, setAvailableMcpServers] = useState<string[]>([]);

  // Inline add state
  const [syncStatus, setSyncStatus] = useState<string | null>(null);

  // Drift detection state
  // null = unknown/not yet checked, DriftReport = result of last check
  const [driftReport, setDriftReport] = useState<DriftReport | null>(null);
  const driftCheckInFlight = useRef(false);

  // Project template state
  const [availableProjectTemplates, setAvailableProjectTemplates] = useState<ProjectTemplate[]>([]);
  const [showProjectTemplatePicker, setShowProjectTemplatePicker] = useState(false);
  const [selectedProjectTemplate, setSelectedProjectTemplate] = useState<string | null>(null);
  // Pending unified instruction content + rules to write after next save (from template apply)
  const pendingUnifiedInstruction = useRef<{ content: string; rules: string[] } | null>(null);

  // Project file state
  const [projectFiles, setProjectFiles] = useState<ProjectFileInfo[]>([]);
  const [activeProjectFile, setActiveProjectFile] = useState<string | null>(null);
  const [projectFileContent, setProjectFileContent] = useState("");
  const [projectFileEditing, setProjectFileEditing] = useState(false);
  const [projectFileDirty, setProjectFileDirty] = useState(false);
  const [projectFileSaving, setProjectFileSaving] = useState(false);
  const [availableTemplates, setAvailableTemplates] = useState<string[]>([]);
  const [showTemplatePicker, setShowTemplatePicker] = useState(false);
  const [availableRules, setAvailableRules] = useState<{ id: string; name: string }[]>([]);

  // Tab navigation within a project
  type ProjectTab = "summary" | "details" | "agents" | "skills" | "mcp_servers" | "project_file" | "memory";
  const [projectTab, setProjectTab] = useState<ProjectTab>("summary");

  // Memory state
  const [memories, setMemories] = useState<Record<string, { value: string; timestamp: string; source: string | null }>>({});
  const [loadingMemories, setLoadingMemories] = useState(false);

  useEffect(() => {
    loadProjects();
    loadAvailableAgents();
    loadAvailableSkills();
    loadAvailableMcpServers();
    loadAvailableTemplates();
    loadAvailableRules();
    loadAvailableProjectTemplates();
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

  // Navigate to a specific project when directed from another view (e.g. Agents)
  useEffect(() => {
    if (initialProject && projects.includes(initialProject)) {
      selectProject(initialProject);
      onInitialProjectConsumed?.();
    }
  }, [initialProject, projects]);

  // Reset drift state whenever the active project changes
  useEffect(() => {
    setDriftReport(null);
  }, [selectedName]);

  // Periodically check for configuration drift while a project tab is active
  useEffect(() => {
    const name = selectedName;
    if (!name || !project || !project.directory || project.agents.length === 0 || dirty || isCreating) {
      return;
    }

    const runCheck = async () => {
      if (driftCheckInFlight.current) return;
      driftCheckInFlight.current = true;
      try {
        const raw: string = await invoke("check_project_drift", { name });
        setDriftReport(JSON.parse(raw) as DriftReport);
      } catch {
        // Silently ignore drift-check errors (e.g. directory gone)
      } finally {
        driftCheckInFlight.current = false;
      }
    };

    // Run immediately on mount / project change, then every 15 seconds
    runCheck();
    const interval = setInterval(runCheck, 15_000);
    return () => clearInterval(interval);
  }, [selectedName, project?.directory, project?.agents.length, dirty, isCreating]);

  const applyStoredOrder = (names: string[]): string[] => {
    try {
      const stored = localStorage.getItem(PROJECT_ORDER_KEY);
      if (!stored) return names.sort();
      const order: string[] = JSON.parse(stored);
      // Projects in stored order first, then any new ones alphabetically at the end
      const ordered: string[] = [];
      for (const n of order) {
        if (names.includes(n)) ordered.push(n);
      }
      const remaining = names.filter((n) => !ordered.includes(n)).sort();
      return [...ordered, ...remaining];
    } catch {
      return names.sort();
    }
  };

  const saveProjectOrder = (ordered: string[]) => {
    localStorage.setItem(PROJECT_ORDER_KEY, JSON.stringify(ordered));
  };

  // Compute which list index the pointer is over, skipping the "New Project" item
  const getDropIndex = (clientY: number): number | null => {
    if (!listRef.current) return null;
    const children = Array.from(listRef.current.children) as HTMLElement[];
    // If creating, the first child is the "New Project" placeholder — skip it
    const offset = isCreating ? 1 : 0;
    const items = children.slice(offset);
    for (let i = 0; i < items.length; i++) {
      const rect = items[i].getBoundingClientRect();
      const midY = rect.top + rect.height / 2;
      if (clientY < midY) return i;
    }
    return items.length;
  };

  const handleGripDown = (idx: number, e: React.PointerEvent) => {
    e.preventDefault();
    e.stopPropagation();
    dragIdxRef.current = idx;
    dropIdxRef.current = idx;
    setDragIdx(idx);
    setDropIdx(idx);

    const onMove = (ev: PointerEvent) => {
      const target = getDropIndex(ev.clientY);
      dropIdxRef.current = target;
      setDropIdx(target);
    };

    const onUp = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);

      const fromIdx = dragIdxRef.current;
      const toIdx = dropIdxRef.current;
      dragIdxRef.current = null;
      dropIdxRef.current = null;
      setDragIdx(null);
      setDropIdx(null);

      if (fromIdx === null || toIdx === null || fromIdx === toIdx) return;

      setProjects((prev) => {
        const reordered = [...prev];
        const [removed] = reordered.splice(fromIdx, 1);
        const insertAt = toIdx > fromIdx ? toIdx - 1 : toIdx;
        reordered.splice(insertAt, 0, removed);
        saveProjectOrder(reordered);
        return reordered;
      });
    };

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };

  const loadProjects = async () => {
    try {
      const result: string[] = await invoke("get_projects");
      const ordered = applyStoredOrder(result);
      setProjects(ordered);
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
      const result: { name: string; in_agents: boolean; in_claude: boolean }[] = await invoke("get_skills");
      setAvailableSkills(result.map((e) => e.name).sort());
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

  const loadAvailableTemplates = async () => {
    try {
      const result: string[] = await invoke("get_templates");
      setAvailableTemplates(result.sort());
    } catch {
      // Templates may not exist yet
    }
  };

  const loadAvailableRules = async () => {
    try {
      const result: { id: string; name: string }[] = await invoke("get_rules");
      setAvailableRules(result.sort((a, b) => a.name.localeCompare(b.name)));
    } catch {
      // Rules may not exist yet
    }
  };

  const loadAvailableProjectTemplates = async () => {
    try {
      const names: string[] = await invoke("get_project_templates");
      const loaded: ProjectTemplate[] = await Promise.all(
        names.map(async (name) => {
          const raw: string = await invoke("read_project_template", { name });
          return JSON.parse(raw) as ProjectTemplate;
        })
      );
      setAvailableProjectTemplates(loaded);
    } catch {
      // Project templates may not exist yet
    }
  };

  const loadMemories = async (projectName: string) => {
    try {
      setLoadingMemories(true);
      const data: Record<string, { value: string; timestamp: string; source: string | null }> = await invoke("get_project_memories", { project: projectName });
      setMemories(data);
    } catch (err: any) {
      console.error("Failed to load memories:", err);
    } finally {
      setLoadingMemories(false);
    }
  };

  const applyProjectTemplate = (tmpl: ProjectTemplate) => {
    if (!project) return;
    // Merge: add template values, preserving anything already on the project
    const mergedAgents = [...new Set([...project.agents, ...tmpl.agents])];
    const mergedSkills = [...new Set([...project.skills, ...tmpl.skills])];
    const mergedMcpServers = [...new Set([...project.mcp_servers, ...tmpl.mcp_servers])];
    const mergedProviders = [...new Set([...project.providers, ...tmpl.providers])];
    const hasUnifiedContent = !!(tmpl.unified_instruction && tmpl.unified_instruction.trim());
    const hasUnifiedRules = (tmpl.unified_rules || []).length > 0;
    const hasUnified = hasUnifiedContent || hasUnifiedRules;
    setProject({
      ...project,
      description: project.description || tmpl.description,
      agents: mergedAgents,
      skills: mergedSkills,
      mcp_servers: mergedMcpServers,
      providers: mergedProviders,
      ...(hasUnified ? { instruction_mode: "unified" } : {}),
    });
    // Stash the unified instruction content + rules so handleSave can write them after the project is saved
    if (hasUnified) {
      pendingUnifiedInstruction.current = {
        content: tmpl.unified_instruction || "",
        rules: tmpl.unified_rules || [],
      };
    }
    setDirty(true);
    setSelectedProjectTemplate(tmpl.name);
    setShowProjectTemplatePicker(false);
  };

  const loadProjectFiles = async (name: string) => {
    try {
      const raw: string = await invoke("get_project_file_info", { name });
      const files: ProjectFileInfo[] = JSON.parse(raw);
      setProjectFiles(files);
      // Auto-select first file if none selected or previous one isn't available
      if (files.length > 0) {
        const currentValid = activeProjectFile && files.some(f => f.filename === activeProjectFile);
        const filename = currentValid ? activeProjectFile! : files[0].filename;
        setActiveProjectFile(filename);
        await loadProjectFileContent(name, filename);
      } else {
        setActiveProjectFile(null);
        setProjectFileContent("");
        setProjectFileEditing(false);
        setProjectFileDirty(false);
      }
    } catch {
      setProjectFiles([]);
      setActiveProjectFile(null);
      setProjectFileContent("");
    }
  };

  const loadProjectFileContent = async (projectName: string, filename: string) => {
    try {
      const content: string = await invoke("read_project_file", { name: projectName, filename });
      setProjectFileContent(content);
      setProjectFileEditing(false);
      setProjectFileDirty(false);
    } catch {
      setProjectFileContent("");
      setProjectFileEditing(false);
      setProjectFileDirty(false);
    }
  };

  const handleSaveProjectFile = async () => {
    if (!selectedName || !activeProjectFile) return;
    setProjectFileSaving(true);
    try {
      await invoke("save_project_file", {
        name: selectedName,
        filename: activeProjectFile,
        content: projectFileContent,
      });
      setProjectFileDirty(false);
    } catch (err: any) {
      setError(`Failed to save project file: ${err}`);
    } finally {
      setProjectFileSaving(false);
    }
  };

  const handleApplyTemplate = async (templateName: string) => {
    try {
      const content: string = await invoke("read_template", { name: templateName });
      setProjectFileContent(content);
      setProjectFileDirty(true);
      setProjectFileEditing(true);
      setShowTemplatePicker(false);
    } catch (err: any) {
      setError(`Failed to load template: ${err}`);
    }
  };

  const selectProject = async (name: string) => {
    try {
      // Fetch both the stored state and the autodetected state in parallel so
      // we can tell whether detection found anything new that hasn't been saved.
      const [rawDetected, rawStored] = await Promise.all([
        invoke<string>("autodetect_project_dependencies", { name }),
        invoke<string>("read_project", { name }),
      ]);
      const parsed = JSON.parse(rawDetected);
      const stored = JSON.parse(rawStored);

      // Use stored config as the source of truth so that intentional user
      // removals (e.g. de-selecting an agent) are preserved. Autodetected
      // items are only merged in when they are genuinely new — i.e. present
      // in the detected result but absent from the stored config — never
      // added back once the user has removed them.
      const storedAgents: string[] = stored.agents || [];
      const storedSkills: string[] = stored.skills || [];
      const storedLocalSkills: string[] = stored.local_skills || [];
      const storedMcp: string[] = stored.mcp_servers || [];

      const detectedAgents: string[] = parsed.agents || [];
      const detectedSkills: string[] = parsed.skills || [];
      const detectedLocalSkills: string[] = parsed.local_skills || [];
      const detectedMcp: string[] = parsed.mcp_servers || [];

      // New items found by autodetect that aren't yet in the stored config.
      const newAgents = detectedAgents.filter((a) => !storedAgents.includes(a));
      const newSkills = detectedSkills.filter((s) => !storedSkills.includes(s));
      const newLocalSkills = detectedLocalSkills.filter((s) => !storedLocalSkills.includes(s));
      const newMcp = detectedMcp.filter((m) => !storedMcp.includes(m));

      const detectedDiffers =
        newAgents.length > 0 ||
        newSkills.length > 0 ||
        newLocalSkills.length > 0 ||
        newMcp.length > 0;

      // Normalize: ensure all fields exist with defaults for older projects.
      // Start from stored data and append any newly-detected items.
      const data: Project = {
        name: stored.name || name,
        description: stored.description || "",
        directory: stored.directory || "",
        skills: [...storedSkills, ...newSkills],
        local_skills: [...storedLocalSkills, ...newLocalSkills],
        mcp_servers: [...storedMcp, ...newMcp],
        providers: stored.providers || [],
        agents: [...storedAgents, ...newAgents],
        created_at: stored.created_at || new Date().toISOString(),
        updated_at: stored.updated_at || new Date().toISOString(),
        file_rules: stored.file_rules || {},
        instruction_mode: stored.instruction_mode || "per-agent",
      };

      setSelectedName(name);
      localStorage.setItem(LAST_PROJECT_KEY, name);
      setProject(data);
      setDirty(detectedDiffers);
      setIsCreating(false);
      setError(null);
      // Load project files for this project
      if (data.directory && data.agents.length > 0) {
        await loadProjectFiles(name);
      } else {
        setProjectFiles([]);
        setActiveProjectFile(null);
        setProjectFileContent("");
        setProjectFileEditing(false);
        setProjectFileDirty(false);
      }
      await loadMemories(name);
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

  // Reload project state from disk and refresh all dependent UI
   const reloadProject = async (name: string) => {
    try {
      const raw: string = await invoke("read_project", { name });
      const parsed = JSON.parse(raw);
      const data: Project = {
        name: parsed.name || name,
        description: parsed.description || "",
        directory: parsed.directory || "",
        skills: parsed.skills || [],
        local_skills: parsed.local_skills || [],
        mcp_servers: parsed.mcp_servers || [],
        providers: parsed.providers || [],
        agents: parsed.agents || [],
        created_at: parsed.created_at || new Date().toISOString(),
        updated_at: parsed.updated_at || new Date().toISOString(),
        file_rules: parsed.file_rules || {},
        instruction_mode: parsed.instruction_mode || "per-agent",
      };
      setProject(data);
      setDirty(false);

      await loadAvailableSkills();
      await loadAvailableMcpServers();
      await loadMemories(name);

      if (data.directory && data.agents.length > 0) {
        await loadProjectFiles(name);
      } else {
        setProjectFiles([]);
        setActiveProjectFile(null);
        setProjectFileContent("");
        setProjectFileEditing(false);
        setProjectFileDirty(false);
      }
    } catch (err: any) {
      setError(`Failed to reload project: ${err}`);
    }
  };

  const handleSave = async () => {
    if (!project) return;
    const folderName = project.directory
      ? project.directory.split("/").filter(Boolean).pop() ?? ""
      : "";
    const name = isCreating
      ? (newName.trim() || folderName)
      : selectedName;
    if (!name) return;
    try {
      setSyncStatus("syncing");
      const toSave = { ...project, name, updated_at: new Date().toISOString() };
      // Tag new projects with the current user for future team/cloud sync
      if (isCreating && userId && !toSave.created_by) {
        toSave.created_by = userId;
      }
      // save_project writes the project config AND syncs all agent configs
      // (skills, MCP servers) in one atomic backend call.
      await invoke("save_project", {
        name,
        data: JSON.stringify(toSave, null, 2),
      });
      setSelectedName(name);
      localStorage.setItem(LAST_PROJECT_KEY, name);
      if (isCreating) {
        setIsCreating(false);
        await loadProjects();
      }
      setError(null);

      setSyncStatus(toSave.directory && toSave.agents.length > 0
        ? "Saved & synced"
        : "Saved");
      if (toSave.directory && toSave.agents.length > 0) {
        setDriftReport({ drifted: false, agents: [] });
      }

      // Write any pending unified instruction content from a template apply
      const pending = pendingUnifiedInstruction.current;
      if (pending !== null && toSave.directory && toSave.agents.length > 0) {
        pendingUnifiedInstruction.current = null;
        // If the template had rules, persist them into file_rules before writing
        if (pending.rules.length > 0) {
          const latestRaw: string = await invoke("read_project", { name });
          const latestProj = JSON.parse(latestRaw);
          const withRules = {
            ...latestProj,
            file_rules: { ...(latestProj.file_rules || {}), _unified: pending.rules },
          };
          await invoke("save_project", { name, data: JSON.stringify(withRules, null, 2) });
        }
        await invoke("save_project_file", {
          name,
          filename: "_unified",
          content: pending.content,
        });
      }

      // Reload UI state from disk (picks up autodetected changes)
      await reloadProject(name);

      setTimeout(() => setSyncStatus(null), 4000);
    } catch (err: any) {
      setSyncStatus(null);
      setError(`Failed to save project: ${err}`);
    }
  };

  const handleRemove = async (name: string, e?: React.MouseEvent) => {
    if (e) e.stopPropagation();
    if (!confirm(`Remove project "${name}" from Automatic?\n\n(This only removes the project from this app. Your actual project files will NOT be deleted.)`)) return;
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
    setSelectedProjectTemplate(null);
    setShowProjectTemplatePicker(false);
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
      await invoke("rename_project", { oldName: selectedName, newName: trimmed });
      // Update localStorage order
      const stored = localStorage.getItem(PROJECT_ORDER_KEY);
      if (stored) {
        try {
          const order: string[] = JSON.parse(stored);
          const idx = order.indexOf(selectedName);
          if (idx !== -1) {
            order[idx] = trimmed;
            localStorage.setItem(PROJECT_ORDER_KEY, JSON.stringify(order));
          }
        } catch { /* ignore */ }
      }
      setSelectedName(trimmed);
      localStorage.setItem(LAST_PROJECT_KEY, trimmed);
      setIsRenaming(false);
      setError(null);
      await loadProjects();
      await selectProject(trimmed);
    } catch (err: any) {
      setError(`Failed to rename project: ${err}`);
      setIsRenaming(false);
    }
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

    // Save first if dirty — handleSave already includes sync
    if (dirty) {
      await handleSave();
      return;
    }

    // Clean state: just re-sync from what's on disk
    try {
      setSyncStatus("syncing");
      const result: string = await invoke("sync_project", { name });
      const files: string[] = JSON.parse(result);
      setSyncStatus(`Synced ${files.length} config${files.length !== 1 ? "s" : ""}`);
      setDriftReport({ drifted: false, agents: [] });
    } catch (err: any) {
      setSyncStatus(`Sync failed: ${err}`);
    }

    await reloadProject(name);
    setTimeout(() => setSyncStatus(null), 4000);
  };

  return (
    <div className="flex h-full w-full bg-[#222327]">
      {/* Left sidebar - project list */}
      <div
        className="flex-shrink-0 flex flex-col border-r border-[#33353A] bg-[#1A1A1E]/50 relative"
        style={{ width: sidebarWidth }}
      >
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
            <ul className="space-y-0.5 px-2" ref={listRef}>
              {isCreating && (
                <li className="flex items-center gap-2.5 px-2 py-1.5 rounded-md text-[13px] bg-[#2D2E36] text-[#E0E1E6]">
                  <FolderOpen size={14} className="text-[#8A8C93]" />
                  <span className="italic">New Project...</span>
                </li>
              )}
              {projects.map((name, idx) => (
                <li
                  key={name}
                  className="relative"
                >
                  {/* Drop indicator line — above this item */}
                  {dragIdx !== null && dropIdx === idx && dropIdx !== dragIdx && dropIdx !== dragIdx + 1 && (
                    <div className="absolute -top-[1px] left-2 right-2 h-[2px] bg-[#5E6AD2] rounded-full z-10" />
                  )}
                  <div className={`group flex items-center relative ${dragIdx === idx ? "opacity-30" : ""}`}>
                    <div
                      className="absolute left-0 top-0 bottom-0 flex items-center pl-0.5 opacity-0 group-hover:opacity-100 cursor-grab active:cursor-grabbing transition-opacity touch-none select-none z-10"
                      onPointerDown={(e) => handleGripDown(idx, e)}
                    >
                      <GripVertical size={10} className="text-[#8A8C93]/60" />
                    </div>
                    <button
                      onClick={() => { if (dragIdx === null) selectProject(name); }}
                      className={`w-full flex items-center gap-2.5 pl-4 pr-2 py-1.5 rounded-md text-[13px] font-medium transition-colors ${
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
                      title="Remove Project from Automatic"
                    >
                      <X size={12} />
                    </button>
                  </div>
                  {/* Drop indicator line — after last item */}
                  {dragIdx !== null && dropIdx === projects.length && idx === projects.length - 1 && dropIdx !== dragIdx && (
                    <div className="absolute -bottom-[1px] left-2 right-2 h-[2px] bg-[#5E6AD2] rounded-full z-10" />
                  )}
                </li>
              ))}
            </ul>
          )}
        </div>
        {/* Resize handle */}
        <div
          className="absolute top-0 right-0 w-1 h-full cursor-col-resize hover:bg-[#5E6AD2]/40 active:bg-[#5E6AD2]/60 transition-colors z-10"
          onMouseDown={onSidebarMouseDown}
        />
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
                {syncStatus && (
                  <span className={`text-[12px] ${syncStatus.startsWith("Sync failed") ? "text-[#FF6B6B]" : syncStatus === "syncing" ? "text-[#8A8C93]" : "text-[#4ADE80]"}`}>
                    {syncStatus === "syncing" ? "Syncing..." : syncStatus}
                  </span>
                )}
                {!isCreating && selectedName && (
                  <button
                    onClick={() => handleRemove(selectedName)}
                    className="flex items-center gap-1.5 px-3 py-1.5 bg-[#2D2E36] hover:bg-[#FF6B6B]/10 text-[#8A8C93] hover:text-[#FF6B6B] rounded text-[12px] font-medium border border-[#3A3B42] hover:border-[#FF6B6B]/30 transition-colors mr-1"
                    title="Remove project from Automatic"
                  >
                    <Trash2 size={12} /> Remove
                  </button>
                )}
                {/* Sync / in-sync indicator — shown when project has directory + agents configured */}
                {!dirty && project.directory && project.agents.length > 0 && (
                  driftReport?.drifted ? (
                    <button
                      onClick={handleSync}
                      className="flex items-center gap-1.5 px-3 py-1.5 bg-[#F59E0B]/10 hover:bg-[#F59E0B]/20 text-[#F59E0B] rounded text-[12px] font-medium border border-[#F59E0B]/40 hover:border-[#F59E0B]/60 transition-colors"
                      title="Configuration has drifted — click to sync"
                    >
                      <RefreshCw size={12} /> Sync Configs
                    </button>
                  ) : driftReport && !driftReport.drifted ? (
                    <button
                      onClick={handleSync}
                      className="flex items-center gap-1.5 px-3 py-1.5 bg-[#2D2E36] hover:bg-[#33353A] text-[#4ADE80] rounded text-[12px] font-medium border border-[#4ADE80]/20 hover:border-[#4ADE80]/40 transition-colors"
                      title="Configuration is up to date — click to force sync"
                    >
                      <Check size={12} /> In Sync
                    </button>
                  ) : (
                    /* driftReport === null: not yet checked */
                    <button
                      onClick={handleSync}
                      className="flex items-center gap-1.5 px-3 py-1.5 bg-[#2D2E36] hover:bg-[#33353A] text-[#8A8C93] rounded text-[12px] font-medium border border-[#3A3B42] transition-colors"
                      title="Sync agent configurations"
                    >
                      <RefreshCw size={12} /> Sync Configs
                    </button>
                  )
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

            {/* ── Drift warning banner ─────────────────────────────── */}
            {driftReport?.drifted && !dirty && !isCreating && project.directory && project.agents.length > 0 && (
              <div className="border-b border-[#F59E0B]/25 bg-[#F59E0B]/10">
                <div className="flex items-center justify-between px-6 py-2 text-[#F59E0B]">
                  <div className="flex items-center gap-2 text-[12px]">
                    <AlertCircle size={13} />
                    <span>Configuration has drifted — agent config files no longer match Automatic settings.</span>
                  </div>
                  <button
                    onClick={handleSync}
                    className="text-[12px] font-medium text-[#F59E0B] hover:text-[#FBB60D] underline decoration-[#F59E0B]/40 hover:decoration-[#FBB60D] transition-colors ml-4 flex-shrink-0"
                  >
                    Sync now
                  </button>
                </div>
                {/* Detail: which agents/files have drifted */}
                <div className="px-6 pb-3 space-y-1.5">
                  {driftReport.agents.map((agentDrift) => (
                    <div key={agentDrift.agent_id}>
                      <div className="text-[11px] font-semibold text-[#F59E0B]/80 mb-0.5">{agentDrift.agent_label}</div>
                      <div className="flex flex-wrap gap-x-3 gap-y-0.5">
                        {agentDrift.files.map((f, i) => (
                          <span key={i} className="text-[11px] font-mono text-[#F59E0B]/60">
                            {f.path}
                            <span className="text-[#F59E0B]/40 ml-1 font-sans">({f.reason})</span>
                          </span>
                        ))}
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {/* ── New project setup screen ─────────────────────────── */}
            {isCreating && (
              <div className="flex-1 flex flex-col items-center justify-center p-8">
                <div className="w-full max-w-md">
                  <div className="mb-8 text-center">
                    <div className="w-14 h-14 mx-auto mb-4 rounded-full bg-[#5E6AD2]/10 border border-[#5E6AD2]/30 flex items-center justify-center">
                      <FolderOpen size={24} className="text-[#5E6AD2]" strokeWidth={1.5} />
                    </div>
                    <h2 className="text-[16px] font-semibold text-[#E0E1E6] mb-1">Where is this project?</h2>
                    <p className="text-[13px] text-[#8A8C93] leading-relaxed">
                      Choose the project directory so Automatic can detect what's already configured and sync agent files.
                    </p>
                  </div>

                  {/* Project Template picker */}
                  {availableProjectTemplates.length > 0 && (
                    <div className="mb-5">
                      <div className="flex items-center gap-1">
                        <button
                          onClick={() => setShowProjectTemplatePicker(!showProjectTemplatePicker)}
                          className={`flex-1 flex items-center justify-between px-3 py-2 bg-[#1A1A1E] border rounded-md text-[13px] transition-colors ${
                            selectedProjectTemplate
                              ? "border-[#5E6AD2]/60 text-[#5E6AD2]"
                              : "border-[#33353A] hover:border-[#44474F] text-[#8A8C93] hover:text-[#E0E1E6]"
                          }`}
                        >
                          <span className="flex items-center gap-2">
                            {selectedProjectTemplate ? (
                              <Check size={13} />
                            ) : (
                              <LayoutTemplate size={13} />
                            )}
                            {selectedProjectTemplate
                              ? `Template: ${selectedProjectTemplate}`
                              : "Start from a project template (optional)"}
                          </span>
                          <span className="text-[11px]">{showProjectTemplatePicker ? "▲" : "▼"}</span>
                        </button>
                        {selectedProjectTemplate && (
                          <button
                            onClick={() => {
                              setProject({ ...emptyProject(""), directory: project.directory });
                              setSelectedProjectTemplate(null);
                              setShowProjectTemplatePicker(false);
                            }}
                            title="Clear template"
                            className="p-2 text-[#8A8C93] hover:text-[#E0E1E6] hover:bg-[#2D2E36] rounded-md transition-colors flex-shrink-0"
                          >
                            <X size={13} />
                          </button>
                        )}
                      </div>
                      {showProjectTemplatePicker && (
                        <div className="mt-1.5 p-2 bg-[#1A1A1E] border border-[#33353A] rounded-md space-y-1">
                          {availableProjectTemplates.map((tmpl) => {
                            const isSelected = selectedProjectTemplate === tmpl.name;
                            return (
                              <button
                                key={tmpl.name}
                                onClick={() => applyProjectTemplate(tmpl)}
                                className={`w-full text-left px-3 py-2 rounded-md transition-colors flex items-start gap-2 ${
                                  isSelected
                                    ? "bg-[#5E6AD2]/15 border border-[#5E6AD2]/40"
                                    : "hover:bg-[#2D2E36] border border-transparent"
                                }`}
                              >
                                <div className="flex-1 min-w-0">
                                  <div className="text-[13px] font-medium text-[#E0E1E6]">{tmpl.name}</div>
                                  {tmpl.description && (
                                    <div className="text-[11px] text-[#8A8C93] mt-0.5 truncate">{tmpl.description}</div>
                                  )}
                                  <div className="flex items-center gap-3 mt-1">
                                    {tmpl.agents.length > 0 && (
                                      <span className="text-[10px] text-[#8A8C93] flex items-center gap-1">
                                        <Bot size={10} /> {tmpl.agents.length} agent{tmpl.agents.length !== 1 ? "s" : ""}
                                      </span>
                                    )}
                                    {tmpl.skills.length > 0 && (
                                      <span className="text-[10px] text-[#8A8C93] flex items-center gap-1">
                                        <Code size={10} /> {tmpl.skills.length} skill{tmpl.skills.length !== 1 ? "s" : ""}
                                      </span>
                                    )}
                                    {tmpl.mcp_servers.length > 0 && (
                                      <span className="text-[10px] text-[#8A8C93] flex items-center gap-1">
                                        <Server size={10} /> {tmpl.mcp_servers.length} MCP server{tmpl.mcp_servers.length !== 1 ? "s" : ""}
                                      </span>
                                    )}
                                  </div>
                                </div>
                                {isSelected && (
                                  <Check size={13} className="text-[#5E6AD2] flex-shrink-0 mt-0.5" />
                                )}
                              </button>
                            );
                          })}
                        </div>
                      )}
                    </div>
                  )}

                  <div className="space-y-3">
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
                          if (!selected) return;
                          const dir = selected as string;
                          // Derive name from folder if not already entered
                          const folderName = dir.split("/").filter(Boolean).pop() ?? "";
                          const name = newName.trim() || folderName;
                          setNewName(name);
                          updateField("directory", dir);
                          // Auto-save then autodetect
                          try {
                            setSyncStatus("syncing");
                            const toSave = { ...project, directory: dir, name, updated_at: new Date().toISOString() };
                            if (userId && !toSave.created_by) {
                              toSave.created_by = userId;
                            }
                            await invoke("save_project", { name, data: JSON.stringify(toSave, null, 2) });
                            setSelectedName(name);
                            localStorage.setItem(LAST_PROJECT_KEY, name);
                            setIsCreating(false);
                            await loadProjects();
                            await reloadProject(name);
                            setSyncStatus("Saved");
                            setTimeout(() => setSyncStatus(null), 3000);
                          } catch (err: any) {
                            setSyncStatus(null);
                            setError(`Failed to save project: ${err}`);
                          }
                        }}
                        className="px-4 py-2 bg-[#5E6AD2] hover:bg-[#6B78E3] text-white text-[13px] font-medium rounded shadow-sm transition-colors whitespace-nowrap"
                      >
                        Browse
                      </button>
                    </div>

                    {project.directory && (
                      <button
                        onClick={handleSave}
                        className="w-full flex items-center justify-center gap-2 px-4 py-2.5 bg-[#5E6AD2] hover:bg-[#6B78E3] text-white text-[13px] font-medium rounded shadow-sm transition-colors"
                      >
                        <Check size={14} /> Create Project
                      </button>
                    )}
                  </div>
                </div>
              </div>
            )}

            {/* Tab bar + content (hidden while in new-project setup) */}
            {!isCreating && <>
            <div className="flex items-center gap-0 px-6 border-b border-[#33353A] bg-[#222327]">
              {([
                { id: "summary" as ProjectTab, label: "Summary" },
                { id: "details" as ProjectTab, label: "Details" },
                { id: "agents" as ProjectTab, label: "Agents" },
                { id: "skills" as ProjectTab, label: "Skills" },
                { id: "mcp_servers" as ProjectTab, label: "MCP Servers" },
                { id: "project_file" as ProjectTab, label: "Project Instructions" },
                { id: "memory" as ProjectTab, label: "Memory" },
              ]).map((tab) => (
                <button
                  key={tab.id}
                  onClick={() => setProjectTab(tab.id)}
                  className={`px-3 py-2 text-[13px] font-medium transition-colors relative ${
                    projectTab === tab.id
                      ? "text-[#E0E1E6]"
                      : "text-[#8A8C93] hover:text-[#E0E1E6]"
                  }`}
                >
                  {tab.label}
                  {projectTab === tab.id && (
                    <span className="absolute bottom-0 left-0 right-0 h-[2px] bg-[#5E6AD2] rounded-t" />
                  )}
                </button>
              ))}
            </div>

            {/* Tab content */}

            {/* ── Project File tab (full-bleed layout) ──────────── */}
            {projectTab === "project_file" && (
              <>
                {project.directory && project.agents.length > 0 ? (
                  <div className="flex-1 flex flex-col min-h-0">
                    {/* Mode toggle bar */}
                    <div className="flex items-center gap-3 px-4 py-2.5 border-b border-[#33353A] bg-[#1A1A1E]/30 flex-shrink-0">
                      <span className="text-[11px] text-[#8A8C93]">Mode:</span>
                      <div className="flex rounded overflow-hidden border border-[#33353A]">
                        <button
                          onClick={async () => {
                            if (project.instruction_mode !== "unified" && selectedName) {
                              const updated = { ...project, instruction_mode: "unified", updated_at: new Date().toISOString() };
                              setProject(updated);
                              setDirty(false);
                              await invoke("save_project", { name: selectedName, data: JSON.stringify(updated, null, 2) });
                              await loadProjectFiles(selectedName);
                            }
                          }}
                          className={`flex items-center gap-1.5 px-2.5 py-1 text-[11px] font-medium transition-colors ${
                            (project.instruction_mode || "per-agent") === "unified"
                              ? "bg-[#5E6AD2] text-white"
                              : "bg-[#2D2E36] text-[#8A8C93] hover:text-[#E0E1E6]"
                          }`}
                        >
                          <Files size={11} />
                          Unified
                        </button>
                        <button
                          onClick={async () => {
                            if (project.instruction_mode !== "per-agent" && selectedName) {
                              const updated = { ...project, instruction_mode: "per-agent", updated_at: new Date().toISOString() };
                              setProject(updated);
                              setDirty(false);
                              await invoke("save_project", { name: selectedName, data: JSON.stringify(updated, null, 2) });
                              await loadProjectFiles(selectedName);
                            }
                          }}
                          className={`flex items-center gap-1.5 px-2.5 py-1 text-[11px] font-medium transition-colors ${
                            (project.instruction_mode || "per-agent") === "per-agent"
                              ? "bg-[#5E6AD2] text-white"
                              : "bg-[#2D2E36] text-[#8A8C93] hover:text-[#E0E1E6]"
                          }`}
                        >
                          <SplitSquareHorizontal size={11} />
                          Per Agent
                        </button>
                      </div>
                      {(project.instruction_mode || "per-agent") === "unified" && projectFiles.length > 0 && projectFiles[0].target_files && (
                        <span className="text-[10px] text-[#8A8C93]">
                          Writes to: {projectFiles[0].target_files.join(", ")}
                        </span>
                      )}
                    </div>

                    <div className="flex-1 flex min-h-0">
                    {/* File sidebar — hidden in unified mode */}
                    {(project.instruction_mode || "per-agent") === "per-agent" && projectFiles.length > 0 && (
                      <div className="w-52 flex-shrink-0 border-r border-[#33353A] bg-[#1A1A1E]/50 flex flex-col">
                        <div className="h-9 px-3 border-b border-[#33353A] flex items-center justify-between">
                          <span className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase">Files</span>
                          <button
                            onClick={() => setShowTemplatePicker(!showTemplatePicker)}
                            className="text-[#8A8C93] hover:text-[#E0E1E6] p-0.5 hover:bg-[#2D2E36] rounded transition-colors"
                            title="Start from template"
                          >
                            <LayoutTemplate size={12} />
                          </button>
                        </div>
                        <div className="flex-1 overflow-y-auto py-1.5 custom-scrollbar">
                          <ul className="space-y-0.5 px-1.5">
                            {projectFiles.map((f) => (
                              <li key={f.filename}>
                                <button
                                  onClick={async () => {
                                    if (projectFileDirty && !confirm("Discard unsaved changes?")) return;
                                    setActiveProjectFile(f.filename);
                                    if (selectedName) await loadProjectFileContent(selectedName, f.filename);
                                  }}
                                  className={`w-full text-left px-2.5 py-1.5 rounded-md text-[13px] font-medium transition-colors flex items-center gap-2 ${
                                    activeProjectFile === f.filename
                                      ? "bg-[#2D2E36] text-[#E0E1E6]"
                                      : "text-[#8A8C93] hover:bg-[#2D2E36]/50 hover:text-[#E0E1E6]"
                                  }`}
                                >
                                  <FileText size={13} className={activeProjectFile === f.filename ? "text-[#E0E1E6]" : f.exists ? "text-[#8A8C93]" : "text-[#8A8C93]/40"} />
                                  <div className="min-w-0">
                                    <div className={`truncate ${!f.exists ? "opacity-50" : ""}`}>{f.filename}</div>
                                    <div className="text-[10px] text-[#8A8C93] truncate">{f.agents.join(", ")}</div>
                                  </div>
                                </button>
                              </li>
                            ))}
                          </ul>
                        </div>
                        {/* Template picker (dropdown in sidebar) */}
                        {showTemplatePicker && availableTemplates.length > 0 && (
                          <div className="border-t border-[#33353A] p-2">
                            <p className="text-[10px] text-[#8A8C93] mb-1.5">Apply template:</p>
                            <div className="space-y-0.5">
                              {availableTemplates.map((t) => (
                                <button
                                  key={t}
                                  onClick={() => handleApplyTemplate(t)}
                                  className="w-full text-left px-2 py-1 text-[12px] bg-[#2D2E36] hover:bg-[#5E6AD2] text-[#E0E1E6] rounded transition-colors flex items-center gap-1.5"
                                >
                                  <LayoutTemplate size={10} />
                                  {t}
                                </button>
                              ))}
                            </div>
                          </div>
                        )}
                      </div>
                    )}

                    {/* Editor area (fills remaining space) */}
                    {projectFiles.length > 0 && activeProjectFile ? (() => {
                      const activeFile = projectFiles.find(f => f.filename === activeProjectFile);
                      const fileExists = activeFile?.exists ?? false;

                      if (!fileExists && !projectFileEditing) {
                        // File doesn't exist yet — show create prompt
                        return (
                          <div className="flex-1 flex flex-col items-center justify-center text-center p-8">
                            <div className="w-12 h-12 mx-auto mb-4 rounded-full border border-dashed border-[#44474F] flex items-center justify-center text-[#8A8C93]">
                              <FileText size={20} strokeWidth={1.5} />
                            </div>
                            <h3 className="text-[14px] font-medium text-[#E0E1E6] mb-1">
                              {activeProjectFile}
                            </h3>
                            <p className="text-[13px] text-[#8A8C93] mb-5 max-w-xs">
                              This file doesn't exist yet. Create it to provide project instructions for {activeFile?.agents.join(" & ")}.
                            </p>
                            <div className="flex items-center gap-2">
                              <button
                                onClick={() => {
                                  setProjectFileContent("");
                                  setProjectFileEditing(true);
                                  setProjectFileDirty(true);
                                }}
                                className="px-3 py-1.5 bg-[#5E6AD2] hover:bg-[#6B78E3] text-white text-[12px] font-medium rounded shadow-sm transition-colors flex items-center gap-1.5"
                              >
                                <Plus size={12} /> Create File
                              </button>
                              {availableTemplates.length > 0 && (
                                <button
                                  onClick={() => setShowTemplatePicker(!showTemplatePicker)}
                                  className="px-3 py-1.5 bg-[#2D2E36] hover:bg-[#33353A] text-[#E0E1E6] text-[12px] font-medium rounded border border-[#3A3B42] transition-colors flex items-center gap-1.5"
                                >
                                  <LayoutTemplate size={12} /> From Template
                                </button>
                              )}
                            </div>
                            {showTemplatePicker && availableTemplates.length > 0 && (
                              <div className="mt-3 p-2 bg-[#1A1A1E] rounded-md border border-[#33353A]">
                                <div className="flex flex-wrap gap-1.5">
                                  {availableTemplates.map((t) => (
                                    <button
                                      key={t}
                                      onClick={() => handleApplyTemplate(t)}
                                      className="px-2 py-1 text-[12px] bg-[#2D2E36] hover:bg-[#5E6AD2] text-[#E0E1E6] rounded transition-colors flex items-center gap-1.5"
                                    >
                                      <LayoutTemplate size={10} />
                                      {t}
                                    </button>
                                  ))}
                                </div>
                              </div>
                            )}
                          </div>
                        );
                      }

                      // File exists or we're editing a new file
                      const currentFileRules = (project.file_rules || {})[activeProjectFile] || [];

                      const handleToggleRule = (ruleName: string) => {
                        if (!project || !activeProjectFile) return;
                        const existing = (project.file_rules || {})[activeProjectFile] || [];
                        const updated = existing.includes(ruleName)
                          ? existing.filter(r => r !== ruleName)
                          : [...existing, ruleName];
                        const newFileRules = { ...(project.file_rules || {}), [activeProjectFile]: updated };
                        if (updated.length === 0) delete newFileRules[activeProjectFile];
                        setProject({ ...project, file_rules: newFileRules });
                        setDirty(true);
                      };

                      return (
                        <div className="flex-1 flex flex-col min-w-0">
                          {/* Editor toolbar */}
                          <div className="flex items-center justify-between px-4 h-9 bg-[#1A1A1E] border-b border-[#33353A] flex-shrink-0">
                            <span className="text-[11px] text-[#8A8C93]">
                              {activeProjectFile === "_unified"
                                ? <>{projectFileEditing ? "Editing" : ""}{projectFileDirty ? " (unsaved)" : ""}</>
                                : <>{activeProjectFile}{!fileExists ? " (new)" : ""}{projectFileEditing ? " — Editing" : ""}{projectFileDirty ? " (unsaved)" : ""}</>
                              }
                            </span>
                            <div className="flex items-center gap-1.5">
                              {!projectFileEditing ? (
                                <button
                                  onClick={() => setProjectFileEditing(true)}
                                  className="flex items-center gap-1 px-2 py-0.5 text-[11px] text-[#8A8C93] hover:text-[#E0E1E6] hover:bg-[#2D2E36] rounded transition-colors"
                                >
                                  <Edit2 size={10} /> Edit
                                </button>
                              ) : (
                                <>
                                  <button
                                    onClick={() => {
                                      setProjectFileEditing(false);
                                      if (projectFileDirty && selectedName && activeProjectFile) {
                                        if (fileExists) {
                                          loadProjectFileContent(selectedName, activeProjectFile);
                                        } else {
                                          setProjectFileContent("");
                                          setProjectFileDirty(false);
                                        }
                                      }
                                    }}
                                    className="px-2 py-0.5 text-[11px] text-[#8A8C93] hover:text-[#E0E1E6] hover:bg-[#2D2E36] rounded transition-colors"
                                  >
                                    Cancel
                                  </button>
                                  <button
                                    onClick={handleSaveProjectFile}
                                    disabled={!projectFileDirty || projectFileSaving}
                                    className="flex items-center gap-1 px-2 py-0.5 text-[11px] bg-[#5E6AD2] hover:bg-[#6B78E3] text-white rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                                  >
                                    <Check size={10} /> {projectFileSaving ? "Saving..." : "Save"}
                                  </button>
                                </>
                              )}
                            </div>
                          </div>

                          {/* Content area — fills remaining height above rules panel */}
                          {projectFileEditing ? (
                            <textarea
                              value={projectFileContent}
                              onChange={(e) => {
                                setProjectFileContent(e.target.value);
                                setProjectFileDirty(true);
                              }}
                              className="flex-1 w-full p-4 resize-none outline-none font-mono text-[12px] bg-[#222327] text-[#E0E1E6] leading-relaxed custom-scrollbar placeholder-[#8A8C93]/30 min-h-0"
                              placeholder="Write your project instructions here..."
                              spellCheck={false}
                            />
                          ) : (
                            <div className="flex-1 overflow-y-auto custom-scrollbar bg-[#222327] min-h-0">
                              {projectFileContent
                                ? <MarkdownPreview content={projectFileContent} />
                                : <span className="block p-4 text-[13px] text-[#8A8C93] italic">Empty file.</span>
                              }
                            </div>
                          )}

                          {/* Rules panel */}
                          <div className="border-t border-[#33353A] bg-[#1A1A1E] flex-shrink-0">
                            <div className="px-4 py-2 flex items-center gap-2">
                              <ScrollText size={12} className="text-[#22D3EE]" />
                              <span className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase">Rules</span>
                              {currentFileRules.length > 0 && (
                                <span className="text-[10px] text-[#22D3EE] bg-[#22D3EE]/10 px-1.5 py-0.5 rounded">{currentFileRules.length}</span>
                              )}
                            </div>
                            {availableRules.length > 0 ? (
                              <div className="px-4 pb-3 flex flex-wrap gap-1.5">
                                {availableRules.map(rule => {
                                  const isSelected = currentFileRules.includes(rule.id);
                                  return (
                                    <button
                                      key={rule.id}
                                      onClick={() => handleToggleRule(rule.id)}
                                      className={`px-2.5 py-1 text-[12px] rounded border transition-colors flex items-center gap-1.5 ${
                                        isSelected
                                          ? "bg-[#22D3EE]/15 border-[#22D3EE]/40 text-[#22D3EE]"
                                          : "bg-[#2D2E36] border-[#33353A] text-[#8A8C93] hover:text-[#E0E1E6] hover:border-[#44474F]"
                                      }`}
                                    >
                                      <ScrollText size={10} />
                                      {rule.name}
                                      {isSelected && <Check size={10} />}
                                    </button>
                                  );
                                })}
                              </div>
                            ) : (
                              <div className="px-4 pb-3">
                                <span className="text-[11px] text-[#8A8C93]/60 italic">No rules created yet. Create rules in the Rules section to attach them here.</span>
                              </div>
                            )}
                          </div>
                        </div>
                      );
                    })() : (
                      <div className="flex-1 flex items-center justify-center">
                        <p className="text-[13px] text-[#8A8C93]/60 italic">No project files configured. Add agent tools on the Agents tab first.</p>
                      </div>
                    )}
                  </div>
                  </div>
                ) : (
                  <div className="flex-1 flex items-center justify-center">
                    <p className="text-[13px] text-[#8A8C93]/60 italic">
                      Set a project directory and add agent tools on the Details and Agents tabs to manage project files.
                    </p>
                  </div>
                )}
              </>
            )}

            {/* Other tabs (padded container) */}
            {projectTab !== "project_file" && (
            <div className="flex-1 overflow-y-auto p-6 custom-scrollbar">
              <div className="space-y-8">

                {/* ── Summary tab ──────────────────────────────────────── */}
                {projectTab === "summary" && (
                  <>
                    {/* Quick Stats */}
                    <div className="grid grid-cols-3 gap-4">
                      {/* Agents Card */}
                      <button
                        onClick={() => setProjectTab("agents")}
                        className="group bg-[#1A1A1E] border border-[#33353A] hover:border-[#5E6AD2]/50 rounded-lg p-4 text-left transition-all hover:shadow-lg hover:shadow-[#5E6AD2]/10"
                      >
                        <div className="flex items-start gap-3">
                          <div className="flex-1 min-w-0">
                            <div className="flex items-start justify-between">
                              <div className="text-3xl font-semibold text-[#E0E1E6] leading-none mb-1">
                                {project.agents.length}
                              </div>
                              <ArrowRight size={14} className="text-[#8A8C93] opacity-0 group-hover:opacity-100 transition-opacity mt-0.5" />
                            </div>
                            <div className="text-[13px] text-[#8A8C93] mb-1">Agent Tools</div>
                            <div className="text-[11px] text-[#8A8C93]/60 truncate">
                              {project.agents.length === 0
                                ? "No agents configured"
                                : project.agents.slice(0, 2).map(a => availableAgents.find(ag => ag.id === a)?.label || a).join(", ") + (project.agents.length > 2 ? ` +${project.agents.length - 2}` : "")}
                            </div>
                          </div>
                          <div className="p-2 bg-[#5E6AD2]/10 rounded-lg group-hover:bg-[#5E6AD2]/20 transition-colors shrink-0">
                            <Bot size={18} className="text-[#5E6AD2]" />
                          </div>
                        </div>
                      </button>

                      {/* Skills Card */}
                      <button
                        onClick={() => setProjectTab("skills")}
                        className="group bg-[#1A1A1E] border border-[#33353A] hover:border-[#4ADE80]/50 rounded-lg p-4 text-left transition-all hover:shadow-lg hover:shadow-[#4ADE80]/10"
                      >
                        <div className="flex items-start gap-3">
                          <div className="flex-1 min-w-0">
                            <div className="flex items-start justify-between">
                              <div className="text-3xl font-semibold text-[#E0E1E6] leading-none mb-1">
                                {project.skills.length + project.local_skills.length}
                              </div>
                              <ArrowRight size={14} className="text-[#8A8C93] opacity-0 group-hover:opacity-100 transition-opacity mt-0.5" />
                            </div>
                            <div className="text-[13px] text-[#8A8C93] mb-1">Skills</div>
                            <div className="text-[11px] text-[#8A8C93]/60 truncate">
                              {project.skills.length === 0 && project.local_skills.length === 0
                                ? "No skills attached"
                                : `${project.skills.length} global, ${project.local_skills.length} local`}
                            </div>
                          </div>
                          <div className="p-2 bg-[#4ADE80]/10 rounded-lg group-hover:bg-[#4ADE80]/20 transition-colors shrink-0">
                            <Code size={18} className="text-[#4ADE80]" />
                          </div>
                        </div>
                      </button>

                      {/* MCP Servers Card */}
                      <button
                        onClick={() => setProjectTab("mcp_servers")}
                        className="group bg-[#1A1A1E] border border-[#33353A] hover:border-[#F59E0B]/50 rounded-lg p-4 text-left transition-all hover:shadow-lg hover:shadow-[#F59E0B]/10"
                      >
                        <div className="flex items-start gap-3">
                          <div className="flex-1 min-w-0">
                            <div className="flex items-start justify-between">
                              <div className="text-3xl font-semibold text-[#E0E1E6] leading-none mb-1">
                                {project.mcp_servers.length}
                              </div>
                              <ArrowRight size={14} className="text-[#8A8C93] opacity-0 group-hover:opacity-100 transition-opacity mt-0.5" />
                            </div>
                            <div className="text-[13px] text-[#8A8C93] mb-1">MCP Servers</div>
                            <div className="text-[11px] text-[#8A8C93]/60 truncate">
                              {project.mcp_servers.length === 0
                                ? "No servers configured"
                                : project.mcp_servers.slice(0, 2).join(", ") + (project.mcp_servers.length > 2 ? ` +${project.mcp_servers.length - 2}` : "")}
                            </div>
                          </div>
                          <div className="p-2 bg-[#F59E0B]/10 rounded-lg group-hover:bg-[#F59E0B]/20 transition-colors shrink-0">
                            <Server size={18} className="text-[#F59E0B]" />
                          </div>
                        </div>
                      </button>
                    </div>

                    {/* Project Status */}
                    <section className="bg-[#1A1A1E] border border-[#33353A] rounded-lg p-5">
                      <div className="flex items-start justify-between mb-4">
                        <div>
                          <h3 className="text-[13px] font-semibold text-[#E0E1E6] mb-1">Project Status</h3>
                          <p className="text-[11px] text-[#8A8C93]">Configuration and sync status</p>
                        </div>
                        {project.directory && project.agents.length > 0 ? (
                          <CheckCircle2 size={18} className="text-[#4ADE80]" />
                        ) : (
                          <AlertCircle size={18} className="text-[#F59E0B]" />
                        )}
                      </div>

                      <div className="space-y-3">
                        {/* Directory Status */}
                        <div className="flex items-center justify-between">
                          <div className="flex items-center gap-2">
                            <FolderOpen size={13} className="text-[#8A8C93]" />
                            <span className="text-[12px] text-[#E0E1E6]">Project Directory</span>
                          </div>
                          {project.directory ? (
                            <div className="flex items-center gap-1.5">
                              <CheckCircle2 size={12} className="text-[#4ADE80]" />
                              <span className="text-[11px] text-[#4ADE80]">Configured</span>
                            </div>
                          ) : (
                            <div className="flex items-center gap-1.5">
                              <AlertCircle size={12} className="text-[#F59E0B]" />
                              <button
                                onClick={() => setProjectTab("details")}
                                className="text-[11px] text-[#5E6AD2] hover:text-[#6B78E3] transition-colors"
                              >
                                Set directory
                              </button>
                            </div>
                          )}
                        </div>

                        {/* Agents Status */}
                        <div className="flex items-center justify-between">
                          <div className="flex items-center gap-2">
                            <Bot size={13} className="text-[#8A8C93]" />
                            <span className="text-[12px] text-[#E0E1E6]">Agent Tools</span>
                          </div>
                          {project.agents.length > 0 ? (
                            <div className="flex items-center gap-1.5">
                              <CheckCircle2 size={12} className="text-[#4ADE80]" />
                              <span className="text-[11px] text-[#4ADE80]">{project.agents.length} configured</span>
                            </div>
                          ) : (
                            <div className="flex items-center gap-1.5">
                              <AlertCircle size={12} className="text-[#F59E0B]" />
                              <button
                                onClick={() => setProjectTab("agents")}
                                className="text-[11px] text-[#5E6AD2] hover:text-[#6B78E3] transition-colors"
                              >
                                Add agents
                              </button>
                            </div>
                          )}
                        </div>

                        {/* Sync Status */}
                        <div className="flex items-center justify-between">
                          <div className="flex items-center gap-2">
                            <RefreshCw size={13} className="text-[#8A8C93]" />
                            <span className="text-[12px] text-[#E0E1E6]">Configuration Sync</span>
                          </div>
                          {project.directory && project.agents.length > 0 ? (
                            <div className="flex items-center gap-1.5">
                              <CheckCircle2 size={12} className="text-[#4ADE80]" />
                              <span className="text-[11px] text-[#4ADE80]">Ready</span>
                            </div>
                          ) : (
                            <div className="flex items-center gap-1.5">
                              <AlertCircle size={12} className="text-[#8A8C93]" />
                              <span className="text-[11px] text-[#8A8C93]">Not available</span>
                            </div>
                          )}
                        </div>
                      </div>

                      {/* Directory Path */}
                      {project.directory && (
                        <div className="mt-4 pt-4 border-t border-[#33353A]">
                          <div className="text-[10px] text-[#8A8C93] mb-1">Location</div>
                          <div className="text-[11px] font-mono text-[#E0E1E6] break-all bg-[#222327] px-2 py-1.5 rounded border border-[#33353A]">
                            {project.directory}
                          </div>
                        </div>
                      )}
                    </section>

                    {/* Quick Actions */}
                     <section>
                       <h3 className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase mb-3">Quick Actions</h3>

                       {/* Apply Project Template */}
                       {availableProjectTemplates.length > 0 && (
                         <div className="mb-3">
                           <button
                             onClick={() => setShowProjectTemplatePicker(!showProjectTemplatePicker)}
                             className="w-full group flex items-center gap-3 bg-[#1A1A1E] border border-[#33353A] hover:border-[#5E6AD2]/50 rounded-lg p-4 transition-all hover:shadow-lg hover:shadow-[#5E6AD2]/10 text-left"
                           >
                             <div className="p-2 bg-[#5E6AD2]/10 rounded-lg group-hover:bg-[#5E6AD2]/20 transition-colors">
                               <LayoutTemplate size={16} className="text-[#5E6AD2]" />
                             </div>
                             <div className="flex-1 min-w-0">
                               <div className="text-[13px] font-medium text-[#E0E1E6] mb-0.5">Apply Project Template</div>
                               <div className="text-[11px] text-[#8A8C93]">Merge agents, skills & servers from a template</div>
                             </div>
                             <ArrowRight size={14} className="text-[#8A8C93] opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0" />
                           </button>
                           {showProjectTemplatePicker && (
                              <div className="mt-1.5 p-2 bg-[#1A1A1E] border border-[#33353A] rounded-lg space-y-1">
                                {availableProjectTemplates.map((tmpl) => {
                                  const isSelected = selectedProjectTemplate === tmpl.name;
                                  return (
                                    <button
                                      key={tmpl.name}
                                      onClick={() => applyProjectTemplate(tmpl)}
                                      className={`w-full text-left px-3 py-2 rounded-md transition-colors flex items-start gap-2 ${
                                        isSelected
                                          ? "bg-[#5E6AD2]/15 border border-[#5E6AD2]/40"
                                          : "hover:bg-[#2D2E36] border border-transparent"
                                      }`}
                                    >
                                      <div className="flex-1 min-w-0">
                                        <div className="text-[13px] font-medium text-[#E0E1E6]">{tmpl.name}</div>
                                        {tmpl.description && (
                                          <div className="text-[11px] text-[#8A8C93] mt-0.5 truncate">{tmpl.description}</div>
                                        )}
                                        <div className="flex items-center gap-3 mt-1">
                                          {tmpl.agents.length > 0 && (
                                            <span className="text-[10px] text-[#8A8C93] flex items-center gap-1">
                                              <Bot size={10} /> {tmpl.agents.length} agent{tmpl.agents.length !== 1 ? "s" : ""}
                                            </span>
                                          )}
                                          {tmpl.skills.length > 0 && (
                                            <span className="text-[10px] text-[#8A8C93] flex items-center gap-1">
                                              <Code size={10} /> {tmpl.skills.length} skill{tmpl.skills.length !== 1 ? "s" : ""}
                                            </span>
                                          )}
                                          {tmpl.mcp_servers.length > 0 && (
                                            <span className="text-[10px] text-[#8A8C93] flex items-center gap-1">
                                              <Server size={10} /> {tmpl.mcp_servers.length} MCP server{tmpl.mcp_servers.length !== 1 ? "s" : ""}
                                            </span>
                                          )}
                                        </div>
                                      </div>
                                      {isSelected && (
                                        <Check size={13} className="text-[#5E6AD2] flex-shrink-0 mt-0.5" />
                                      )}
                                    </button>
                                  );
                                })}
                                 <div className="mt-1 flex items-center gap-2 px-3 py-1">
                                   <button
                                     onClick={() => setShowProjectTemplatePicker(false)}
                                     className="text-[11px] text-[#8A8C93] hover:text-[#E0E1E6] transition-colors"
                                   >
                                     Cancel
                                   </button>
                                   {selectedProjectTemplate && (
                                     <>
                                       <span className="text-[#33353A]">·</span>
                                       <button
                                         onClick={() => {
                                           setSelectedProjectTemplate(null);
                                           setShowProjectTemplatePicker(false);
                                         }}
                                         className="text-[11px] text-[#8A8C93] hover:text-[#E0E1E6] transition-colors"
                                       >
                                         Clear selection
                                       </button>
                                     </>
                                   )}
                                 </div>
                              </div>
                            )}
                         </div>
                       )}

                       <div className="grid grid-cols-2 gap-3">
                         <button
                           onClick={() => setProjectTab("project_file")}
                           className="group flex items-center gap-3 bg-[#1A1A1E] border border-[#33353A] hover:border-[#5E6AD2]/50 rounded-lg p-4 transition-all hover:shadow-lg hover:shadow-[#5E6AD2]/10 text-left"
                         >
                          <div className="p-2 bg-[#5E6AD2]/10 rounded-lg group-hover:bg-[#5E6AD2]/20 transition-colors">
                            <FileText size={16} className="text-[#5E6AD2]" />
                          </div>
                          <div className="flex-1 min-w-0">
                            <div className="text-[13px] font-medium text-[#E0E1E6] mb-0.5">Project Files</div>
                            <div className="text-[11px] text-[#8A8C93]">Manage agent instructions</div>
                          </div>
                          <ArrowRight size={14} className="text-[#8A8C93] opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0" />
                        </button>

                        <button
                           onClick={() => setProjectTab("memory")}
                           className="group flex items-center gap-3 bg-[#1A1A1E] border border-[#33353A] hover:border-[#5E6AD2]/50 rounded-lg p-4 transition-all hover:shadow-lg hover:shadow-[#5E6AD2]/10 text-left"
                         >
                           <div className="p-2 bg-[#5E6AD2]/10 rounded-lg group-hover:bg-[#5E6AD2]/20 transition-colors">
                             <Brain size={16} className="text-[#5E6AD2]" />
                           </div>
                           <div className="flex-1 min-w-0">
                             <div className="text-[13px] font-medium text-[#E0E1E6] mb-0.5">Memory</div>
                             <div className="text-[11px] text-[#8A8C93]">View agent memory</div>
                           </div>
                           <ArrowRight size={14} className="text-[#8A8C93] opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0" />
                         </button>

                        <button
                          onClick={() => setProjectTab("skills")}
                          className="group flex items-center gap-3 bg-[#1A1A1E] border border-[#33353A] hover:border-[#4ADE80]/50 rounded-lg p-4 transition-all hover:shadow-lg hover:shadow-[#4ADE80]/10 text-left"
                        >
                          <div className="p-2 bg-[#4ADE80]/10 rounded-lg group-hover:bg-[#4ADE80]/20 transition-colors">
                            <Package size={16} className="text-[#4ADE80]" />
                          </div>
                          <div className="flex-1 min-w-0">
                            <div className="text-[13px] font-medium text-[#E0E1E6] mb-0.5">Manage Skills</div>
                            <div className="text-[11px] text-[#8A8C93]">Add or remove capabilities</div>
                          </div>
                          <ArrowRight size={14} className="text-[#8A8C93] opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0" />
                        </button>

                        <button
                          onClick={() => setProjectTab("mcp_servers")}
                          className="group flex items-center gap-3 bg-[#1A1A1E] border border-[#33353A] hover:border-[#F59E0B]/50 rounded-lg p-4 transition-all hover:shadow-lg hover:shadow-[#F59E0B]/10 text-left"
                        >
                          <div className="p-2 bg-[#F59E0B]/10 rounded-lg group-hover:bg-[#F59E0B]/20 transition-colors">
                            <Server size={16} className="text-[#F59E0B]" />
                          </div>
                          <div className="flex-1 min-w-0">
                            <div className="text-[13px] font-medium text-[#E0E1E6] mb-0.5">MCP Servers</div>
                            <div className="text-[11px] text-[#8A8C93]">Configure integrations</div>
                          </div>
                          <ArrowRight size={14} className="text-[#8A8C93] opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0" />
                        </button>
                      </div>
                    </section>

                    {/* Description */}
                    {project.description && (
                      <section className="bg-[#1A1A1E] border border-[#33353A] rounded-lg p-5">
                        <h3 className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase mb-2">Description</h3>
                        <p className="text-[13px] text-[#E0E1E6] leading-relaxed whitespace-pre-wrap">
                          {project.description}
                        </p>
                      </section>
                    )}

                    {/* Getting Started (existing saved project that is still incomplete) */}
                    {!isCreating && (!project.directory || project.agents.length === 0) && (
                      <section className="bg-gradient-to-br from-[#5E6AD2]/10 to-[#5E6AD2]/5 border border-[#5E6AD2]/20 rounded-lg p-5">
                        <div className="flex items-start gap-3">
                          <div className="p-2 bg-[#5E6AD2]/20 rounded-lg flex-shrink-0">
                            <Package size={18} className="text-[#5E6AD2]" />
                          </div>
                          <div>
                            <h3 className="text-[13px] font-semibold text-[#E0E1E6] mb-2">Complete Setup</h3>
                            <p className="text-[12px] text-[#8A8C93] mb-3 leading-relaxed">
                              To start using this project, complete these steps:
                            </p>
                            <ol className="space-y-2 text-[12px] text-[#E0E1E6]">
                              {!project.directory && (
                                <li className="flex items-start gap-2">
                                  <div className="w-5 h-5 rounded-full border border-[#5E6AD2] flex items-center justify-center flex-shrink-0 mt-0.5">
                                    <span className="text-[10px] text-[#5E6AD2]">1</span>
                                  </div>
                                  <div>
                                    <button
                                      onClick={() => setProjectTab("details")}
                                      className="text-[#5E6AD2] hover:text-[#6B78E3] transition-colors font-medium"
                                    >
                                      Set project directory
                                    </button>
                                    <div className="text-[11px] text-[#8A8C93] mt-0.5">
                                      Choose where agent configs will be synced
                                    </div>
                                  </div>
                                </li>
                              )}
                              {project.agents.length === 0 && (
                                <li className="flex items-start gap-2">
                                  <div className="w-5 h-5 rounded-full border border-[#5E6AD2] flex items-center justify-center flex-shrink-0 mt-0.5">
                                    <span className="text-[10px] text-[#5E6AD2]">{!project.directory ? "2" : "1"}</span>
                                  </div>
                                  <div>
                                    <button
                                      onClick={() => setProjectTab("agents")}
                                      className="text-[#5E6AD2] hover:text-[#6B78E3] transition-colors font-medium"
                                    >
                                      Add agent tools
                                    </button>
                                    <div className="text-[11px] text-[#8A8C93] mt-0.5">
                                      Select which agents will use this project
                                    </div>
                                  </div>
                                </li>
                              )}
                              <li className="flex items-start gap-2">
                                <div className="w-5 h-5 rounded-full border border-[#8A8C93]/50 flex items-center justify-center flex-shrink-0 mt-0.5">
                                  <span className="text-[10px] text-[#8A8C93]">•</span>
                                </div>
                                <div>
                                  <button
                                    onClick={() => setProjectTab("skills")}
                                    className="text-[#E0E1E6] hover:text-[#5E6AD2] transition-colors"
                                  >
                                    Add skills (optional)
                                  </button>
                                  <div className="text-[11px] text-[#8A8C93] mt-0.5">
                                    Give agents specialized capabilities
                                  </div>
                                </div>
                              </li>
                            </ol>
                          </div>
                        </div>
                      </section>
                    )}
                  </>
                )}

                {/* ── Details tab ──────────────────────────────────────── */}
                {projectTab === "details" && (
                  <>
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
                  </>
                )}

                {/* ── Agents tab ───────────────────────────────────────── */}
                {projectTab === "agents" && (
                  <section>
                    <AgentSelector
                      agentIds={project.agents}
                      availableAgents={availableAgents}
                      onAdd={(id) => addItem("agents", id)}
                      onRemove={(i) => removeItem("agents", i)}
                      emptyMessage="No agent tools selected. Add tools to enable config sync."
                    />
                  </section>
                )}

                {/* ── Skills tab ───────────────────────────────────────── */}
                {projectTab === "skills" && (
                  <>
                    {/* Global Skills */}
                     <section>
                       <SkillSelector
                         skills={project.skills}
                         availableSkills={availableSkills}
                         onAdd={(s) => addItem("skills", s)}
                         onRemove={(i) => removeItem("skills", i)}
                         emptyMessage="No skills attached."
                       />
                     </section>

                    {/* Local Skills */}
                    {project.local_skills.length > 0 && (
                      <section>
                        <div className="flex items-center justify-between mb-2">
                          <label className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase flex items-center gap-1.5">
                            <Code size={12} /> Local Skills
                          </label>
                          {project.local_skills.length > 1 && selectedName && (
                            <button
                              onClick={async () => {
                                try {
                                  setSyncStatus("syncing");
                                  await invoke("sync_local_skills", { name: selectedName });
                                  setSyncStatus("Local skills synced across agents");
                                  setTimeout(() => setSyncStatus(null), 4000);
                                } catch (err: any) {
                                  setSyncStatus(`Sync failed: ${err}`);
                                  setTimeout(() => setSyncStatus(null), 4000);
                                }
                              }}
                              className="flex items-center gap-1 text-[11px] text-[#8A8C93] hover:text-[#E0E1E6] px-1.5 py-0.5 hover:bg-[#2D2E36] rounded transition-colors"
                              title="Copy all local skills to every agent's skill directory"
                            >
                              <ArrowRightLeft size={11} /> Sync Across Agents
                            </button>
                          )}
                        </div>
                        <p className="text-[11px] text-[#8A8C93] mb-2">
                          These skills exist only in this project directory, not in the global registry.
                        </p>
                        <ul className="space-y-1">
                          {project.local_skills.map((s) => (
                            <li
                              key={s}
                              className="group flex items-center justify-between px-3 py-1.5 bg-[#1A1A1E] rounded-md border border-[#33353A] text-[13px] text-[#E0E1E6]"
                            >
                              <span className="flex items-center gap-2">
                                <Code size={12} className="text-[#8A8C93]" />
                                {s}
                                <span className="text-[10px] px-1.5 py-0.5 rounded bg-[#2D2E36] text-[#8A8C93] border border-[#3A3B42]">
                                  local
                                </span>
                              </span>
                              <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-all">
                                <button
                                  onClick={async () => {
                                    if (!selectedName) return;
                                    try {
                                      setSyncStatus("syncing");
                                      await invoke("sync_local_skills", { name: selectedName });
                                      setSyncStatus(`Synced "${s}" across agents`);
                                      setTimeout(() => setSyncStatus(null), 4000);
                                    } catch (err: any) {
                                      setSyncStatus(`Sync failed: ${err}`);
                                      setTimeout(() => setSyncStatus(null), 4000);
                                    }
                                  }}
                                  className="text-[#8A8C93] hover:text-[#E0E1E6] p-1 hover:bg-[#2D2E36] rounded transition-colors"
                                  title="Sync to all agents in this project"
                                >
                                  <ArrowRightLeft size={12} />
                                </button>
                                <button
                                  onClick={async () => {
                                    if (!selectedName) return;
                                    try {
                                      setSyncStatus("syncing");
                                      const result: string = await invoke("import_local_skill", { name: selectedName, skillName: s });
                                      const updated = JSON.parse(result);
                                      setProject({
                                        ...project,
                                        skills: updated.skills || project.skills,
                                        local_skills: updated.local_skills || [],
                                      });
                                      await loadAvailableSkills();
                                      setSyncStatus(`Imported "${s}" to global registry`);
                                      setTimeout(() => setSyncStatus(null), 4000);
                                    } catch (err: any) {
                                      setSyncStatus(`Import failed: ${err}`);
                                      setTimeout(() => setSyncStatus(null), 4000);
                                    }
                                  }}
                                  className="text-[#8A8C93] hover:text-[#4ADE80] p-1 hover:bg-[#2D2E36] rounded transition-colors"
                                  title="Import to global skill registry"
                                >
                                  <Upload size={12} />
                                </button>
                              </div>
                            </li>
                          ))}
                        </ul>
                      </section>
                    )}
                  </>
                )}

                {/* ── MCP Servers tab ──────────────────────────────────── */}
                {projectTab === "mcp_servers" && (() => {
                  const hasWarp = project.agents.includes("warp");
                  const warpOnly = hasWarp && project.agents.length === 1;
                  const warpNote = availableAgents.find((a) => a.id === "warp")?.mcp_note ?? null;
                  return (
                  <section>
                    {/* Warp-only: MCP config not available */}
                    {warpOnly && warpNote && (
                      <div className="mb-4 flex items-start gap-3 px-4 py-3 bg-[#1A1A1E] border border-[#44474F] rounded-lg">
                        <AlertCircle size={15} className="text-[#8A8C93] flex-shrink-0 mt-0.5" />
                        <div>
                          <p className="text-[13px] font-medium text-[#E0E1E6] mb-0.5">MCP not configurable via Automatic</p>
                          <p className="text-[12px] text-[#8A8C93] leading-relaxed">{warpNote}</p>
                        </div>
                      </div>
                    )}

                    {/* Warp + other agents: partial warning */}
                    {hasWarp && !warpOnly && warpNote && (
                      <div className="mb-4 flex items-start gap-3 px-4 py-3 bg-[#F59E0B]/8 border border-[#F59E0B]/30 rounded-lg">
                        <AlertCircle size={15} className="text-[#F59E0B] flex-shrink-0 mt-0.5" />
                        <div>
                          <p className="text-[13px] font-medium text-[#F59E0B] mb-0.5">Warp requires manual MCP setup</p>
                          <p className="text-[12px] text-[#F59E0B]/80 leading-relaxed">{warpNote}</p>
                        </div>
                      </div>
                    )}

                    <McpSelector
                      servers={project.mcp_servers}
                      availableServers={availableMcpServers}
                      onAdd={(s) => addItem("mcp_servers", s)}
                      onRemove={(i) => removeItem("mcp_servers", i)}
                      disableAdd={warpOnly}
                      emptyMessage={warpOnly ? "Add other agent tools to enable MCP server syncing." : "No MCP servers attached."}
                    />
                  </section>
                  );
                })()}

                {/* ── Memory tab ──────────────────────────────────── */}
                {projectTab === "memory" && (
                  <section>
                    <div className="flex items-center justify-between mb-4">
                      <div>
                        <h3 className="text-[14px] font-medium text-[#E0E1E6]">Agent Memory</h3>
                        <p className="text-[12px] text-[#8A8C93] mt-1">
                          Persistent context and learnings stored by agents working on this project.
                        </p>
                      </div>
                      {Object.keys(memories).length > 0 && (
                        <button
                          onClick={async () => {
                            if (!selectedName || !confirm("Are you sure you want to clear all memory for this project? This cannot be undone.")) return;
                            try {
                              await invoke("clear_memories", { project: selectedName, confirm: true, pattern: null });
                              await loadMemories(selectedName);
                            } catch (err: any) {
                              setError(`Failed to clear memories: ${err}`);
                            }
                          }}
                          className="flex items-center gap-1.5 px-3 py-1.5 bg-[#FF6B6B]/10 hover:bg-[#FF6B6B]/20 text-[#FF6B6B] rounded text-[12px] font-medium border border-[#FF6B6B]/20 transition-colors"
                        >
                          <Trash2 size={12} /> Clear All
                        </button>
                      )}
                    </div>

                    {loadingMemories ? (
                      <div className="text-[13px] text-[#8A8C93] text-center py-8">Loading memories...</div>
                    ) : Object.keys(memories).length === 0 ? (
                      <div className="text-center py-12 bg-[#1A1A1E] rounded-lg border border-[#33353A] border-dashed">
                        <div className="w-12 h-12 mx-auto mb-3 rounded-full bg-[#2D2E36] flex items-center justify-center">
                          <Bot size={20} className="text-[#8A8C93]" />
                        </div>
                        <h4 className="text-[13px] font-medium text-[#E0E1E6] mb-1">No memories yet</h4>
                        <p className="text-[12px] text-[#8A8C93] max-w-sm mx-auto">
                          Agents haven't stored any learnings or context for this project yet.
                        </p>
                      </div>
                    ) : (
                      <div className="space-y-3">
                        {Object.entries(memories).map(([key, memory]) => (
                          <div key={key} className="bg-[#1A1A1E] border border-[#33353A] rounded-lg p-4 group">
                            <div className="flex items-start justify-between gap-4 mb-2">
                              <div className="min-w-0 flex-1">
                                <div className="flex items-center gap-2 mb-1">
                                  <h4 className="text-[13px] font-semibold text-[#E0E1E6] font-mono truncate">{key}</h4>
                                  {memory.source && (
                                    <span className="text-[10px] px-1.5 py-0.5 rounded bg-[#2D2E36] text-[#8A8C93] border border-[#3A3B42]">
                                      {memory.source}
                                    </span>
                                  )}
                                </div>
                                <p className="text-[11px] text-[#8A8C93]">
                                  {new Date(memory.timestamp).toLocaleString()}
                                </p>
                              </div>
                              <button
                                onClick={async () => {
                                  if (!selectedName) return;
                                  try {
                                    await invoke("delete_memory", { project: selectedName, key });
                                    await loadMemories(selectedName);
                                  } catch (err: any) {
                                    setError(`Failed to delete memory: ${err}`);
                                  }
                                }}
                                className="text-[#8A8C93] hover:text-[#FF6B6B] p-1.5 hover:bg-[#33353A] rounded transition-colors opacity-0 group-hover:opacity-100"
                                title="Delete memory"
                              >
                                <Trash2 size={14} />
                              </button>
                            </div>
                            <div className="text-[13px] text-[#E0E1E6] whitespace-pre-wrap font-mono bg-[#222327] p-3 rounded border border-[#33353A] max-h-60 overflow-y-auto custom-scrollbar">
                              {memory.value}
                            </div>
                          </div>
                        ))}
                      </div>
                    )}
                  </section>
                )}

              </div>
            </div>
            )}
            </>}
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
