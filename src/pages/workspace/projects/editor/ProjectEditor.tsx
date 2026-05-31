import { useState, useEffect, useRef, useMemo } from "react";
import { SkillSelector } from "../../../../components/SkillSelector";
import { AgentSelector } from "../../../../components/AgentSelector";
import type { AgentOptions } from "../../../../components/AgentSelector";
import { AgentIcon } from "../../../../components/AgentIcon";
import { McpSelector } from "../../../../components/McpSelector";
import { MarkdownPreview } from "../../../../components/MarkdownPreview";
import { LineNumberedTextarea } from "../../../../components/LineNumberedTextarea";
import { TokenPill } from "../../../../components/TokenPill";
import { useCurrentUser } from "../../../../contexts/ProfileContext";
import { useTaskLog } from "../../../../contexts/TaskLogContext";
import Features from "../../../../plugins/build/Features";
import { SpecKittyPanel } from "../../../../plugins/spec-kitty/SpecKittyPanel";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { handleExternalLinkClick } from "../../../../lib/externalLinks";
import {
  trackProjectCreated,
  trackProjectUpdated,
  trackProjectDeleted,
  trackProjectSynced,
  trackProjectAgentAdded,
  trackProjectAgentRemoved,
  trackProjectSkillAdded,
  trackProjectSkillRemoved,
  trackProjectMcpServerAdded,
  trackProjectMcpServerRemoved,
} from "../../../../lib/analytics";
import type {
  CustomRule,
  CustomAgent,
  CustomCommand,
  CustomSkill,
  SubagentEntry,
  UserCommandEntry,
  Project,
  AgentInfo,
  HookEntry,
  DriftedFile,
  InstructionFileConflict,
  UnifiedCandidate,
  UnifiedInspection,
  RebuildPreview,
  DriftReport,
  ProjectProblemsReport,
  ProjectFileInfo,
  ProjectTemplate,
  ActivityEntry,
  ProjectRecommendation,
  ProjectToolEntry,
} from "../types";
import {
  parseInvokeResult,
  agentIdToLabel,
  emptyProject,
  isHttpDocPath,
  isManagedDocNotePath,
} from "../helpers";
import { EditorIcon } from "../EditorIcon";
import { DriftDiffModal } from "../modals/DriftDiffModal";
import { InstructionConflictModal } from "../modals/InstructionConflictModal";
import { SwitchToUnifiedModal } from "./SwitchToUnifiedModal";
import { RebuildConfirmationModal } from "./RebuildConfirmationModal";
import { ApplyProjectTemplateModal } from "./ApplyProjectTemplateModal";
import { SkillAddButton } from "./SkillAddButton";
import { McpAddButton } from "./McpAddButton";
import { ProjectToolsTab, ToolInfoSidebar, ProjectToolDetailPanel } from "./tools/ProjectToolsTab";
import { SettingsPanel } from "./panels/SettingsPanel";
import { MemoryPanel } from "./panels/MemoryPanel";
import { GroupsPanel } from "./panels/GroupsPanel";
import { ActivityPanel } from "./panels/ActivityPanel";
import { RecommendationsPanel } from "./panels/RecommendationsPanel";
import { DocsFilesPanel } from "./panels/DocsFilesPanel";
import { DocsLinksPanel } from "./panels/DocsLinksPanel";
import { DocsNotesPanel } from "./panels/DocsNotesPanel";
import { SummaryPanel } from "./panels/SummaryPanel";

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
  AlertCircle,
  ArrowRight,
  ScrollText,
  Files,
  SplitSquareHorizontal,
  Brain,
  RotateCcw,
  ChevronLeft,
  ChevronRight,
  Copy,
  Search,
  Sparkles,
  Link as LinkIcon,
  ExternalLink,
  Terminal,
  Globe,
  MessagesSquare,
  EyeOff,
  Webhook,
} from "lucide-react";

interface ProjectEditorProps {
  selectedName: string | null;
  setSelectedName: (name: string | null) => void;
  isCreating: boolean;
  setIsCreating: (v: boolean) => void;
  reloadProjects: () => Promise<void>;
  setProjectDetailsMap: React.Dispatch<React.SetStateAction<Map<string, Project>>>;
  setDriftByProject: React.Dispatch<React.SetStateAction<Record<string, boolean>>>;
  onBack: () => void;
  /**
   * Cached project from the router's overview map, used as the initial value
   * so the editor chrome (title, tabs) renders immediately while SEAM 1's
   * `selectProject` fetches fresh data. Avoids a flash of empty content
   * between overview and editor.
   */
  initialProject?: Project | null;
  initialProjectTab?: string | null;
  onInitialProjectTabConsumed?: () => void;
  createFromTemplates?: ProjectTemplate[] | null;
  onCreateFromTemplatesConsumed?: () => void;
  onNavigateToSkill?: (skillName: string) => void;
  onNavigateToMcpServer?: (serverName: string) => void;
  onNavigateToSkillStore?: (skillId: string) => void;
  onNavigateToSkillStoreWithResult?: (result: { id: string; name: string; source: string; installs: number }) => void;
  onNavigateToDiscoverMcp?: (slug: string) => void;
  onNavigateToGroup?: (groupName: string) => void;
  onNavigateToCommand?: (commandId: string) => void;
}

export function ProjectEditor({
  selectedName,
  setSelectedName,
  isCreating,
  setIsCreating,
  reloadProjects,
  setProjectDetailsMap,
  setDriftByProject,
  onBack,
  initialProject = null,
  initialProjectTab = null,
  onInitialProjectTabConsumed,
  createFromTemplates = null,
  onCreateFromTemplatesConsumed,
  onNavigateToSkill,
  onNavigateToMcpServer,
  onNavigateToSkillStore,
  onNavigateToSkillStoreWithResult,
  onNavigateToDiscoverMcp,
  onNavigateToGroup,
  onNavigateToCommand,
}: ProjectEditorProps) {
  const { userId } = useCurrentUser();
  const { log, update } = useTaskLog();
  const LAST_PROJECT_KEY = "automatic.projects.selected";
  const PROJECT_ORDER_KEY = "automatic.projects.order";

  const [project, setProject] = useState<Project | null>(initialProject);
  const [dirty, setDirty] = useState(false);
  const [newName, setNewName] = useState("");
  // Wizard state (used while isCreating === true)
  const [wizardStep, setWizardStep] = useState<1 | 2 | 3>(1);
  const [wizardDiscovering, setWizardDiscovering] = useState(false);
  const [wizardDiscoveredAgents, setWizardDiscoveredAgents] = useState<string[]>([]);
  const [wizardDefaultAgents, setWizardDefaultAgents] = useState<string[]>([]);
  /** Non-empty when the wizard was launched from a "New project from template" action. */
  const [wizardSourceTemplates, setWizardSourceTemplates] = useState<string[]>([]);
  /** Tracks the name of the stub project saved during step 1 so it can be deleted on cancel. */
  const wizardStubName = useRef<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameName, setRenameName] = useState("");

  // Available items to pick from
  const [availableAgents, setAvailableAgents] = useState<AgentInfo[]>([]);
  const [availableSkills, setAvailableSkills] = useState<string[]>([]);
  const [availableMcpServers, setAvailableMcpServers] = useState<string[]>([]);

  // Plugin-locked resources — skills/rules that cannot be removed because
  // they are provided by a plugin whose tool is active on this project.
  const [pluginLockedSkills, setPluginLockedSkills] = useState<string[]>([]);
  const [pluginLockedRules, setPluginLockedRules] = useState<string[]>([]);

  // Inline add state
  const [syncStatus, setSyncStatus] = useState<string | null>(null);

  // Drift detection state
  // null = unknown/not yet checked, DriftReport = result of last check
  const [driftReport, setDriftReport] = useState<DriftReport | null>(null);
  const driftCheckInFlight = useRef(false);

  // Project problems state (e.g. MCP user-scope conflicts)
  // null = unknown/not yet checked, ProjectProblemsReport = result of last check
  const [problemsReport, setProblemsReport] = useState<ProjectProblemsReport | null>(null);

  // Drift diff modal state — null when closed
  const [driftDiffFile, setDriftDiffFile] = useState<{ file: DriftedFile; agentLabel: string } | null>(null);

  // Instruction file conflict modal state — null when closed, conflict when open
  const [instructionConflict, setInstructionConflict] = useState<InstructionFileConflict | null>(null);
  const [rebuildPreview, setRebuildPreview] = useState<RebuildPreview | null>(null);
  const [rebuildBusy, setRebuildBusy] = useState(false);

  // Unified-mode source picker state — populated when the user toggles to
  // unified mode and the per-agent files have divergent user content.
  const [unifiedSourcePicker, setUnifiedSourcePicker] = useState<UnifiedCandidate[] | null>(null);
  const [unifiedSourcePickerBusy, setUnifiedSourcePickerBusy] = useState(false);

  // Project template state
  const [availableProjectTemplates, setAvailableProjectTemplates] = useState<ProjectTemplate[]>([]);
  const [showProjectTemplatePicker, setShowProjectTemplatePicker] = useState(false);
  /** The template name currently highlighted in the apply modal (single-select). */
  const [templateApplySelection, setTemplateApplySelection] = useState<string | null>(null);
  /** Result of the most recent successful apply, shown in the modal's confirmation view. */
  const [templateApplyResult, setTemplateApplyResult] = useState<{
    templateName: string;
    added: {
      agents: string[];
      skills: string[];
      mcp_servers: string[];
      user_agents: string[];
      user_commands: string[];
      hooks: string[];
      rules: string[];
    };
    hasUnifiedContent: boolean;
    saveRequired: boolean;
  } | null>(null);
  /** Names of templates seeded into the new-project wizard so its panels can show provenance. */
  const [selectedProjectTemplates, setSelectedProjectTemplates] = useState<string[]>([]);
  // Pending unified instruction content + rules to write after next save (from template applies).
  // Each entry corresponds to one applied template; contents are concatenated on flush.
  const pendingUnifiedInstruction = useRef<{ content: string; rules: string[] }[] | null>(null);

  // Project file state
  const [projectFiles, setProjectFiles] = useState<ProjectFileInfo[]>([]);
  const [activeProjectFile, setActiveProjectFile] = useState<string | null>(null);
  const [projectFileContent, setProjectFileContent] = useState("");
  const [projectFileEditing, setProjectFileEditing] = useState(false);
  const [projectFileDirty, setProjectFileDirty] = useState(false);
  const [projectFileSaving, setProjectFileSaving] = useState(false);
  const [projectFileGenerating, setProjectFileGenerating] = useState(false);
  const [projectFileUpdating, setProjectFileUpdating] = useState(false);
  // Whether the master "Agent features" toggle is effectively on.
  // Controls whether AI Generate buttons are enabled.
  const [agentFeaturesEnabled, setAgentFeaturesEnabled] = useState(false);
  // Display label of the currently active Automatic agent (e.g. "Claude", "OpenAI").
  const [activeAgentLabel, setActiveAgentLabel] = useState<string>("Claude");
  // Incremented whenever any project configuration is mutated (saved, synced,
  // instruction files written, etc.).  A useEffect watches this counter and
  // re-evaluates recommendations after every change.
  const [projectVersion, setProjectVersion] = useState(0);
  const notifyProjectUpdated = () => setProjectVersion((v) => v + 1);
  const [availableTemplates, setAvailableTemplates] = useState<string[]>([]);
  const [showTemplatePicker, setShowTemplatePicker] = useState(false);
  const [availableRules, setAvailableRules] = useState<{ id: string; name: string }[]>([]);

  // Tab navigation within a project
  type ProjectTab = "summary" | "agents" | "commands" | "hooks" | "custom_agents" | "skills" | "mcp_servers" | "groups" | "project_file" | "rules" | "context" | "docs_files" | "docs_links" | "docs_notes" | "memory" | "activity" | "recommendations" | "tools" | "settings";
  type ProjectGroup = "summary" | "project_file" | "rules" | "skills" | "mcp_servers" | "custom_agents" | "commands" | "hooks" | "configuration" | "instructions" | "documentation" | "memory" | "activity" | "insights";

  const PROJECT_GROUPS: {
    id: ProjectGroup;
    label: string;
    tabs: { id: ProjectTab; label: string }[];
  }[] = [
    { id: "summary", label: "Summary", tabs: [] },
    { id: "project_file", label: "Instructions", tabs: [{ id: "project_file", label: "Instructions" }] },
    { id: "rules", label: "Rules", tabs: [{ id: "rules", label: "Rules" }] },
    { id: "skills", label: "Skills", tabs: [{ id: "skills", label: "Skills" }] },
    { id: "mcp_servers", label: "MCP", tabs: [{ id: "mcp_servers", label: "MCP" }] },
    { id: "custom_agents", label: "Agents", tabs: [{ id: "custom_agents", label: "Agents" }] },
    { id: "commands", label: "Commands", tabs: [{ id: "commands", label: "Commands" }] },
    { id: "hooks", label: "Hooks", tabs: [{ id: "hooks", label: "Hooks" }] },
    {
      id: "instructions",
      label: "Context",
      tabs: [
        { id: "context", label: "Context" },
      ],
    },
    {
      id: "documentation",
      label: "Documentation",
      tabs: [
        { id: "docs_files", label: "Files & Dirs" },
        { id: "docs_links", label: "Links" },
        { id: "docs_notes", label: "Notes" },
      ],
    },
  ];

  // Project controls — rendered in a secondary bar above the title area, right-aligned.
  const PROJECT_CONTROLS: {
    id: ProjectGroup;
    label: string;
    tabs: { id: ProjectTab; label: string }[];
  }[] = [
    {
      id: "configuration",
      label: "Configuration",
      tabs: [
        { id: "agents", label: "Providers" },
        { id: "tools", label: "Tools" },
        { id: "groups", label: "Groups" },
        { id: "settings", label: "Settings" },
      ],
    },
    { id: "insights", label: "Insights", tabs: [{ id: "recommendations", label: "Recommendations" }] },
    { id: "activity", label: "Activity", tabs: [{ id: "activity", label: "Activity" }] },
    { id: "memory", label: "Memory", tabs: [{ id: "memory", label: "Memory" }] },
  ];

  /** Derive the group for a given tab id */
  function groupForTab(tab: ProjectTab): ProjectGroup {
    for (const g of PROJECT_GROUPS) {
      if (g.id === "summary" && tab === "summary") return "summary";
      if (g.tabs.some((t) => t.id === tab)) return g.id;
    }
    // Also check the project controls bar (rendered separately above the title).
    for (const g of PROJECT_CONTROLS) {
      if (g.tabs.some((t) => t.id === tab)) return g.id;
    }
    return "summary";
  }

  /** True when the given group belongs to the secondary (controls) bar. */
  function isSecondaryGroup(group: ProjectGroup): boolean {
    return PROJECT_CONTROLS.some((c) => c.id === group);
  }

  const [projectTab, setProjectTab] = useState<ProjectTab>("summary");
  const [projectGroup, setProjectGroup] = useState<ProjectGroup>("summary");

  // Tool sub-tab state: null = show the overview; string = tool name of the selected tool detail tab.
  const [toolTab, setToolTab] = useState<string | null>(null);
  // Tool entries loaded — shared by the Configuration → Tools sub-tab and top-level tool tabs.
  const [toolEntries, setToolEntries] = useState<ProjectToolEntry[]>([]);
  const [toolEntriesLoading, setToolEntriesLoading] = useState(false);

  // When non-null, a top-level tool tab is active (overrides projectGroup/projectTab indicators).
  const [activeToolName, setActiveToolName] = useState<string | null>(null);

  // The view to restore when the user closes a secondary (controls bar) view via the X button.
  // Captured on entry into a secondary group; not updated while navigating between secondary items.
  const [returnView, setReturnView] = useState<{ group: ProjectGroup; tool: string | null }>(
    { group: "summary", tool: null },
  );

  function loadToolEntries() {
    setToolEntriesLoading(true);
    invoke<ProjectToolEntry[]>("list_tools_with_detection")
      .then((data) => { setToolEntries(data); setToolEntriesLoading(false); })
      .catch((err) => { console.error("Failed to load tools:", err); setToolEntriesLoading(false); });
  }

  /** Switch to a group; auto-select first sub-tab (or "summary") */
  function selectGroup(group: ProjectGroup) {
    // Capture the view to return to when entering a secondary group from a non-secondary one.
    // Navigating between secondary items does not overwrite this.
    if (isSecondaryGroup(group) && !isSecondaryGroup(projectGroup)) {
      setReturnView({ group: projectGroup, tool: activeToolName });
    }
    setActiveToolName(null);
    setProjectGroup(group);
    if (group === "summary") {
      setProjectTab("summary");
    } else {
      // Check both PROJECT_GROUPS and PROJECT_CONTROLS for the group definition.
      const g =
        PROJECT_GROUPS.find((g) => g.id === group) ??
        PROJECT_CONTROLS.find((g) => g.id === group);
      if (g && g.tabs.length > 0) {
        const tab = g.tabs[0]!.id;
        setProjectTab(tab);
        // Trigger data loading for tabs that need it.
        if (tab === "activity" && selectedName) {
          loadActivityPage(selectedName, 0);
        }
        if (tab === "tools") {
          setToolTab(null);
          loadToolEntries();
        }
      }
    }
  }

  /** Switch to a specific tab and update the group accordingly */
  function selectTab(tab: ProjectTab) {
    setActiveToolName(null);
    setProjectTab(tab);
    setProjectGroup(groupForTab(tab));
    if (tab !== "rules") setCustomRuleEditingIdx(null);
    if (tab !== "commands") setCustomCommandEditingIdx(null);
    if (tab !== "skills") setCustomSkillEditingIdx(null);
    if (tab === "activity" && selectedName) {
      loadActivityPage(selectedName, 0);
    }
    if (tab === "tools") {
      // Reset tool detail view and load available tool entries.
      setToolTab(null);
      loadToolEntries();
    }
  }

  /** Activate a top-level enabled-tool tab directly from the primary nav bar */
  function selectTopLevelTool(name: string) {
    setActiveToolName(name);
    // Deactivate static group/tab highlights.
    setProjectGroup("summary");
    setProjectTab("summary");
  }

  /** Close the active secondary view and restore the view that was active before it was opened. */
  function closeSecondaryView() {
    if (returnView.tool !== null) {
      selectTopLevelTool(returnView.tool);
    } else {
      selectGroup(returnView.group);
    }
  }

  // Memory state
  const [memories, setMemories] = useState<Record<string, { value: string; timestamp: string; source: string | null }>>({});
  const [loadingMemories, setLoadingMemories] = useState(false);


  // Groups state — names of all groups this project belongs to, and the full
  // list of all available groups (for the "add to group" picker).
  const [projectGroupMemberships, setProjectGroupMemberships] = useState<string[]>([]);
  const [allGroups, setAllGroups] = useState<string[]>([]);
  const [loadingGroups, setLoadingGroups] = useState(false);

  // Recommendations state
  const [recommendations, setRecommendations] = useState<ProjectRecommendation[]>([]);
  const [aiRecsLoading, setAiRecsLoading] = useState(false);
  const [aiRecsLastRunAt, setAiRecsLastRunAt] = useState<string | null>(null);

  // Derived recommendation display values.
  // AI-skill/MCP individual records are collapsed into single rollup cards so the
  // list stays concise — the full suggestions live on the Skills / MCP Servers tabs.
  const normalRecs = recommendations.filter(
    (r) => r.source !== "automatic-ai-skills" && r.source !== "automatic-ai-mcp",
  );
  const aiSkillsRollupCount = recommendations.filter((r) => r.source === "automatic-ai-skills").length;
  const aiMcpRollupCount    = recommendations.filter((r) => r.source === "automatic-ai-mcp").length;
  const recsDisplayCount =
    normalRecs.length +
    (aiSkillsRollupCount > 0 ? 1 : 0) +
    (aiMcpRollupCount > 0 ? 1 : 0);

  // Skills tab AI suggestion state
  const [aiSkillsLoading, setAiSkillsLoading] = useState(false);
  const [aiSkillsSuggestions, setAiSkillsSuggestions] = useState<ProjectRecommendation[]>([]);

  // MCP Servers tab AI suggestion state
  const [aiMcpLoading, setAiMcpLoading] = useState(false);
  const [aiMcpSuggestions, setAiMcpSuggestions] = useState<ProjectRecommendation[]>([]);

  // Activity state
  const [activityEntries, setActivityEntries] = useState<ActivityEntry[]>([]);
  const [loadingActivity, setLoadingActivity] = useState(false);
  // Activity tab pagination (50 per page, 0-based page index)
  const [activityPage, setActivityPage] = useState(0);
  const [activityTotalCount, setActivityTotalCount] = useState(0);
  const [activityPageEntries, setActivityPageEntries] = useState<ActivityEntry[]>([]);
  const [loadingActivityPage, setLoadingActivityPage] = useState(false);
  const ACTIVITY_PAGE_SIZE = 50;

  // Context state
  interface ProjectContextData {
    commands: Record<string, string>;
    entry_points: Record<string, string>;
    concepts: Record<string, { files: string[]; summary: string }>;
    conventions: Record<string, string>;
    gotchas: Record<string, string>;
    docs: Record<string, { path: string; summary: string }>;
  }
  type ProjectDocsData = Record<string, { path: string; summary: string }>;
  const [projectContext, setProjectContext] = useState<ProjectContextData | null>(null);
  const [projectDocs, setProjectDocs] = useState<ProjectDocsData>({});
  const [loadingContext, setLoadingContext] = useState(false);
  // Raw text editor state for context.json
  const [contextRaw, setContextRaw] = useState("");
  const [contextEditing, setContextEditing] = useState(false);
  const [contextDirty, setContextDirty] = useState(false);
  const [contextSaving, setContextSaving] = useState(false);
  const [contextGenerating, setContextGenerating] = useState(false);
  const [contextJsonError, setContextJsonError] = useState<string | null>(null);
  const [contextFileExists, setContextFileExists] = useState(false);

  // Documentation tab state
  // Inline form state for adding a new file/dir path entry
  const [docNewPath, setDocNewPath] = useState("");
  const [docNewPathSummary, setDocNewPathSummary] = useState("");
  // Inline form state for adding a new link entry
  const [docNewLinkUrl, setDocNewLinkUrl] = useState("");
  const [docNewLinkLabel, setDocNewLinkLabel] = useState("");
  // Note editor state
  const [docNoteSelected, setDocNoteSelected] = useState<string | null>(null);
  const [docNoteContent, setDocNoteContent] = useState("");
  const [docNoteDirty, setDocNoteDirty] = useState(false);
  const [docNoteSaving, setDocNoteSaving] = useState(false);
  const [docNoteLoading, setDocNoteLoading] = useState(false);
  const [docNewNoteName, setDocNewNoteName] = useState("");
  const [docNewNoteCreating, setDocNewNoteCreating] = useState(false);
  // Controls whether the inline add-form is visible for each doc sub-tab
  const [showDocPathForm, setShowDocPathForm] = useState(false);
  const [showDocLinkForm, setShowDocLinkForm] = useState(false);

  const fileDocEntries = useMemo(
    () => Object.entries(projectDocs).filter(([, entry]) => !isHttpDocPath(entry.path) && !isManagedDocNotePath(entry.path)),
    [projectDocs],
  );
  const linkDocEntries = useMemo(
    () => Object.entries(projectDocs).filter(([, entry]) => isHttpDocPath(entry.path)),
    [projectDocs],
  );
  // Custom rule editing state (for inline project rules in the Rules tab)
  const [customRuleEditingIdx, setCustomRuleEditingIdx] = useState<number | null>(null);
  const [customRuleEditName, setCustomRuleEditName] = useState("");
  const [customRuleEditContent, setCustomRuleEditContent] = useState("");
  const [globalRuleContentCache, setGlobalRuleContentCache] = useState<Record<string, string>>({});

  // Custom agent editing state (for project-local agents in the Agents tab)
  const [customAgentEditingIdx, setCustomAgentEditingIdx] = useState<number | null>(null);
  const [customAgentEditName, setCustomAgentEditName] = useState("");
  const [customAgentEditContent, setCustomAgentEditContent] = useState("");

  // Workspace agents state (user_agents from global registry)
  const [availableUserAgents, setAvailableUserAgents] = useState<SubagentEntry[]>([]);
  const [userAgentAdding, setUserAgentAdding] = useState(false);
  const [userAgentSearch, setUserAgentSearch] = useState("");

  // Workspace commands state (user_commands from global registry)
  const [availableUserCommands, setAvailableUserCommands] = useState<UserCommandEntry[]>([]);
  const [userCommandAdding, setUserCommandAdding] = useState(false);
  const [userCommandSearch, setUserCommandSearch] = useState("");

  // Hooks state (lifecycle hooks from the global library, keyed by hook id)
  const [availableHooks, setAvailableHooks] = useState<HookEntry[]>([]);
  const [hookAdding, setHookAdding] = useState(false);
  const [hookSearch, setHookSearch] = useState("");

  // Custom command editing state (for project-local commands)
  const [customCommandEditingIdx, setCustomCommandEditingIdx] = useState<number | null>(null);
  const [customCommandEditName, setCustomCommandEditName] = useState("");
  const [customCommandEditContent, setCustomCommandEditContent] = useState("");

  // Custom skill editing state (for project-local skills)
  const [customSkillEditingIdx, setCustomSkillEditingIdx] = useState<number | null>(null);
  const [customSkillEditName, setCustomSkillEditName] = useState("");
  const [customSkillEditContent, setCustomSkillEditContent] = useState("");

  // Expanded workspace command preview state (mirrors SkillSelector pattern)
  const [expandedCommandId, setExpandedCommandId] = useState<string | null>(null);
  const [expandedCommandContent, setExpandedCommandContent] = useState<string>("");
  const [expandedCommandLoading, setExpandedCommandLoading] = useState(false);
  const [expandedCommandError, setExpandedCommandError] = useState<string | null>(null);

  // Global rule picker state (dropdown add, mirrors SkillSelector pattern)
  const [globalRuleAdding, setGlobalRuleAdding] = useState(false);
  const [globalRuleSearch, setGlobalRuleSearch] = useState("");

  // Editor detection state
  interface EditorInfo { id: string; label: string; installed: boolean; }
  const [installedEditors, setInstalledEditors] = useState<EditorInfo[]>([]);
  const [editorIconPaths, setEditorIconPaths] = useState<Record<string, string>>({});
  const [openInDropdownOpen, setOpenInDropdownOpen] = useState(false);
  const openInDropdownRef = useRef<HTMLDivElement>(null);
  const userAgentDropdownRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    loadAvailableAgents();
    loadAvailableSkills();
    loadAvailableMcpServers();
    loadAvailableTemplates();
    loadAvailableRules();
    loadAvailableProjectTemplates();
    loadAvailableUserAgents();
    loadAvailableUserCommands();
    loadAvailableHooks();
    // Effective state of the master Settings > Agents toggle.
    invoke<boolean>("agent_features_enabled").then(setAgentFeaturesEnabled).catch(() => setAgentFeaturesEnabled(false));
    // Active agent display label for the task log.
    invoke<{ active_agent?: string }>("read_settings")
      .then((s) => setActiveAgentLabel(agentIdToLabel(s.active_agent ?? "anthropic")))
      .catch(() => {});
    // Detect which editors are installed on this machine, then fetch real icons
    invoke<EditorInfo[]>("check_installed_editors").then((editors) => {
      setInstalledEditors(editors);
      // Request icon PNG paths for all known editors (not just installed ones —
      // we may want fallback icons for installed editors whose .icns is present
      // regardless of whether the CLI was found).
      const iconIds = editors.map((e) => e.id);
      Promise.all(
        iconIds.map((id) =>
          invoke<string>("get_editor_icon", { editorId: id })
            .then((path) => ({ id, path }))
            .catch(() => null)
        )
      ).then((results) => {
        const paths: Record<string, string> = {};
        for (const r of results) {
          if (r) paths[r.id] = r.path;
        }
        setEditorIconPaths(paths);
      });
    }).catch(() => {});
  }, []);

  useEffect(() => {
    const ruleIds = ((project?.file_rules || {})["_project"] || []) as string[];
    if (ruleIds.length === 0) return;

    let cancelled = false;

    async function warmGlobalRuleContent(): Promise<void> {
      for (const ruleId of ruleIds) {
        if (globalRuleContentCache[ruleId] !== undefined) continue;
        try {
          const content: string = await invoke("read_rule", { machineName: ruleId });
          if (!cancelled) {
            setGlobalRuleContentCache((prev) => (prev[ruleId] !== undefined ? prev : { ...prev, [ruleId]: content }));
          }
        } catch {
          if (!cancelled) {
            setGlobalRuleContentCache((prev) => (prev[ruleId] !== undefined ? prev : { ...prev, [ruleId]: "" }));
          }
        }
      }
    }

    void warmGlobalRuleContent();

    return () => {
      cancelled = true;
    };
  }, [globalRuleContentCache, project?.file_rules]);

  // Close "Open in" dropdown when clicking outside
  useEffect(() => {
    if (!openInDropdownOpen) return;
    const handler = (e: MouseEvent) => {
      if (openInDropdownRef.current && !openInDropdownRef.current.contains(e.target as Node)) {
        setOpenInDropdownOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [openInDropdownOpen]);

  // SEAM 1 — load the active project whenever `selectedName` changes (not in create flow).
  useEffect(() => {
    if (selectedName && !isCreating) void selectProject(selectedName);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedName]);

  // After a project is selected via the router, switch to the requested tab.
  useEffect(() => {
    if (!initialProjectTab) return;
    const validTabs = ["summary", "agents", "skills", "mcp_servers", "commands", "hooks", "custom_agents", "groups", "project_file", "rules", "context", "memory", "activity", "recommendations", "settings"] as const;
    type ProjectTab = typeof validTabs[number];
    if (validTabs.includes(initialProjectTab as ProjectTab)) {
      selectTab(initialProjectTab as ProjectTab);
    }
    onInitialProjectTabConsumed?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initialProjectTab]);

  // SEAM 2 — when mounted with isCreating=true, run startCreate exactly once.
  // The router flips isCreating; the editor seeds the wizard.
  useEffect(() => {
    if (isCreating) {
      void startCreate(createFromTemplates && createFromTemplates.length > 0 ? { fromTemplates: createFromTemplates } : undefined);
      onCreateFromTemplatesConsumed?.();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Reset drift + problems + recommendations state whenever the active project changes
  useEffect(() => {
    setDriftReport(null);
    setProblemsReport(null);
    setCustomRuleEditingIdx(null);
    setGlobalRuleAdding(false);
    setGlobalRuleSearch("");
    setRecommendations([]);
    setAiRecsLastRunAt(null);
    setAiSkillsSuggestions([]);
    setAiMcpSuggestions([]);
    setActiveToolName(null);
    // Load tool entries eagerly so enabled tools can appear in the top-level nav.
    if (selectedName) loadToolEntries();
  }, [selectedName]);

  // Fetch plugin-locked skills/rules whenever the project's tools change.
  useEffect(() => {
    const tools = project?.tools ?? [];
    if (tools.length === 0) {
      setPluginLockedSkills([]);
      setPluginLockedRules([]);
      return;
    }
    invoke<{ skills: string[]; rules: string[] }>("get_plugin_locked_resources", { tools })
      .then(({ skills, rules }) => {
        setPluginLockedSkills(skills);
        setPluginLockedRules(rules);
      })
      .catch(() => {
        setPluginLockedSkills([]);
        setPluginLockedRules([]);
      });
  }, [project?.tools?.join(",")]);

  // Periodically check for configuration drift while a project tab is active.
  //
  // Note: this check runs even while `dirty` is true so that the per-project
  // drift indicator (`driftByProject[name]`) stays in agreement with the
  // background sweep that powers the projects-list badge.  The in-page drift
  // banner and header indicator remain gated on `!dirty` separately, so the
  // "save first" UX is unchanged — but the list badge no longer disagrees
  // with the project page when an in-memory edit (e.g. an autodetect merge
  // performed by `selectProject`) makes the project dirty on entry.
  useEffect(() => {
    const name = selectedName;
    if (!name || !project || !project.directory || project.agents.length === 0 || isCreating) {
      return;
    }

    const runCheck = async () => {
      if (driftCheckInFlight.current) return;
      driftCheckInFlight.current = true;
      try {
        const [rawDrift, rawProblems] = await Promise.all([
          invoke<string>("check_project_drift", { name }),
          invoke<string>("check_project_problems", { name }),
        ]);
        const report = JSON.parse(rawDrift) as DriftReport;
        setDriftReport(report);
        setDriftByProject((prev) => ({ ...prev, [name]: report.drifted }));

        // If there are instruction file conflicts, surface the first one so the
        // user can resolve it.  Only show one at a time to avoid overwhelming the UI.
        const conflicts = report.instruction_conflicts ?? [];
        if (conflicts.length > 0) {
          setInstructionConflict((prev) => {
            // Don't replace an already-open conflict dialog.
            if (prev !== null) return prev;
            return conflicts[0]!;
          });
        }

        setProblemsReport(JSON.parse(rawProblems) as ProjectProblemsReport);
      } catch {
        // Silently ignore drift/problems check errors (e.g. directory gone)
      } finally {
        driftCheckInFlight.current = false;
      }
    };

    // Run immediately on mount / project change, then every 15 seconds
    runCheck();
    const interval = setInterval(runCheck, 15_000);
    return () => clearInterval(interval);
  }, [selectedName, project?.directory, project?.agents.length, isCreating]);

  // Clean up any in-progress wizard stub when the component unmounts (e.g. user
  // navigates to a different top-level section via the sidebar).
  useEffect(() => {
    return () => {
      const stub = wizardStubName.current;
      if (stub) {
        // Fire-and-forget: best-effort deletion on unmount. We cannot await here
        // since React cleanup functions must be synchronous.
        invoke("delete_project", { name: stub }).catch(() => {});
        wizardStubName.current = null;
      }
    };
  }, []);

  const loadAvailableAgents = async () => {
    try {
      const result: AgentInfo[] = await invoke("list_agents");
      result.sort((a, b) => a.label.localeCompare(b.label));
      setAvailableAgents(result);
    } catch {
      // Agents list may not be available yet
    }
  };

  const loadAvailableSkills = async () => {
    try {
      const result: { name: string; sources: string[]
 }[] = await invoke("get_skills");
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

  const loadAvailableUserAgents = async () => {
    try {
      const result: SubagentEntry[] = await invoke("get_subagents");
      setAvailableUserAgents(result.sort((a, b) => a.name.localeCompare(b.name)));
    } catch {
      // User agents may not exist yet
    }
  };

  const loadAvailableUserCommands = async () => {
    try {
      const result: UserCommandEntry[] = await invoke("get_user_commands");
      setAvailableUserCommands(result.sort((a, b) => a.id.localeCompare(b.id)));
    } catch {
      // Commands may not exist yet
    }
  };

  const loadAvailableHooks = async () => {
    try {
      const result: HookEntry[] = await invoke("get_hooks");
      setAvailableHooks(result.sort((a, b) => a.id.localeCompare(b.id)));
    } catch {
      // Hooks may not exist yet
    }
  };

  const loadAvailableTemplates = async () => {
    try {
      const result: string[] = await invoke("get_instructions");
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
      const names: string[] = await invoke("get_templates");
      const loaded: ProjectTemplate[] = await Promise.all(
        names.map(async (name) => {
          const raw: string = await invoke("read_template", { name });
          return JSON.parse(raw) as ProjectTemplate;
        })
      );
      setAvailableProjectTemplates(loaded);
    } catch {
      // Project templates may not exist yet
    }
  };

  /** Toggle a template's selection in the wizard's multi-select picker. */
  const toggleProjectTemplateSelection = (tmplName: string) => {
    setSelectedProjectTemplates((prev) =>
      prev.includes(tmplName) ? prev.filter((n) => n !== tmplName) : [...prev, tmplName]
    );
  };

  /**
   * Merge a single template into the open project and switch the modal to a
   * confirmation view summarising what was added. The backend command
   * `apply_templates_to_project` performs all asset merging, deduplication,
   * and project file writing.
   */
  const applyProjectTemplate = async (templateName: string) => {
    if (!project || !templateName) return;

    const before = project;
    const tmpl = availableProjectTemplates.find((t) => t.name === templateName);

    try {
      const raw: string = await invoke("apply_templates_to_project", {
        projectName: project.name,
        templateNames: [templateName],
      });
      const result: { project: Project; pending_unified: { content: string; rules: string[] }[] } = JSON.parse(raw);

      setProject(result.project);

      const hasUnifiedContent = !!(tmpl?.unified_instruction && tmpl.unified_instruction.trim());
      if (result.pending_unified.length > 0) {
        pendingUnifiedInstruction.current = result.pending_unified;
        setDirty(true);
      }

      const existingRules = (before.file_rules ?? {})["_project"] ?? [];
      const added = {
        agents: (tmpl?.agents ?? []).filter((a) => !before.agents.includes(a)),
        skills: (tmpl?.skills ?? []).filter((s) => !before.skills.includes(s)),
        mcp_servers: (tmpl?.mcp_servers ?? []).filter((m) => !before.mcp_servers.includes(m)),
        user_agents: (tmpl?.user_agents ?? []).filter((a) => !(before.user_agents ?? []).includes(a)),
        user_commands: (tmpl?.user_commands ?? []).filter((c) => !(before.user_commands ?? []).includes(c)),
        hooks: (tmpl?.hooks ?? []).filter((h) => !(before.hooks ?? []).includes(h)),
        rules: (tmpl?.unified_rules ?? []).filter((r) => !existingRules.includes(r)),
      };

      setTemplateApplyResult({
        templateName,
        added,
        hasUnifiedContent,
        saveRequired: result.pending_unified.length > 0,
      });
      setError(null);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(`Failed to apply template "${templateName}": ${msg}`);
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

  /** Load which groups this project belongs to, and all available groups. */
  const loadGroups = async (projectName: string) => {
    try {
      setLoadingGroups(true);
      // Clear stale data immediately so UI doesn't show previous project's groups.
      setProjectGroupMemberships([]);
      const [memberships, available] = await Promise.all([
        invoke<string[]>("groups_for_project", { projectName }),
        invoke<string[]>("list_groups"),
      ]);
      setProjectGroupMemberships(memberships);
      setAllGroups(available.sort((a, b) => a.localeCompare(b)));
    } catch (err: any) {
      console.error("Failed to load groups:", err);
      setError(`Failed to load groups: ${err}`);
    } finally {
      setLoadingGroups(false);
    }
  };

  /** Add this project to a group, save the group, then re-sync ALL projects
   *  in the group so every project's peer list is updated. */
  const handleAddToGroup = async (groupName: string, projectName: string) => {
    try {
      const raw: string = await invoke("read_group", { name: groupName });
      const g = JSON.parse(raw);
      if (!g.projects.includes(projectName)) {
        g.projects.push(projectName);
        g.updated_at = new Date().toISOString();
        await invoke("save_group", { name: groupName, data: JSON.stringify(g) });
        setProjectGroupMemberships((prev) => [...prev, groupName].sort((a, b) => a.localeCompare(b)));
        // Sync ALL projects in the group - each one's peer list changes.
        for (const name of g.projects) {
          invoke("sync_project", { name }).catch((e: unknown) => {
            console.warn(`Group sync: could not sync project '${name}':`, e);
          });
        }
        window.dispatchEvent(new CustomEvent("groups-updated"));
      }
    } catch (err: any) {
      setError(`Failed to add to group: ${err}`);
    }
  };

  /** Remove this project from a group, save, then re-sync ALL remaining projects
   *  (peer lists change) and the removed project (to strip its group block). */
  const handleRemoveFromGroup = async (groupName: string, projectName: string) => {
    try {
      const raw: string = await invoke("read_group", { name: groupName });
      const g = JSON.parse(raw);
      g.projects = g.projects.filter((p: string) => p !== projectName);
      g.updated_at = new Date().toISOString();
      await invoke("save_group", { name: groupName, data: JSON.stringify(g) });
      setProjectGroupMemberships((prev) => prev.filter((n) => n !== groupName));
      // Sync remaining projects (peer lists change) and the removed project.
      const toSync = [...g.projects, projectName];
      for (const name of toSync) {
        invoke("sync_project", { name }).catch((e: unknown) => {
          console.warn(`Group sync: could not sync project '${name}':`, e);
        });
      }
      window.dispatchEvent(new CustomEvent("groups-updated"));
    } catch (err: any) {
      setError(`Failed to remove from group: ${err}`);
    }
  };

  const handleRemoveFromAllGroups = async (projectName: string) => {
    const confirmed = await ask(
      `Remove "${projectName}" from all ${projectGroupMemberships.length} group${projectGroupMemberships.length === 1 ? "" : "s"}?`,
      { title: "Remove from All Groups", kind: "warning" }
    );
    if (!confirmed) return;

    // Collect all projects that need to be synced (peers in each group + the removed project)
    const toSync = new Set<string>([projectName]);
    
    for (const groupName of projectGroupMemberships) {
      try {
        const raw: string = await invoke("read_group", { name: groupName });
        const g = JSON.parse(raw);
        g.projects = g.projects.filter((p: string) => p !== projectName);
        g.updated_at = new Date().toISOString();
        await invoke("save_group", { name: groupName, data: JSON.stringify(g) });
        // Add remaining projects in this group - their peer lists need updating
        for (const peer of g.projects) {
          toSync.add(peer);
        }
      } catch (err: any) {
        console.error(`Failed to remove from group ${groupName}:`, err);
      }
    }
    setProjectGroupMemberships([]);
    // Sync all affected projects
    for (const name of toSync) {
      invoke("sync_project", { name }).catch((e: unknown) => {
        console.warn(`Group sync: could not sync project '${name}':`, e);
      });
    }
    window.dispatchEvent(new CustomEvent("groups-updated"));
  };

  const loadContext = async (projectName: string) => {
    try {
      setLoadingContext(true);
      const [parsedRaw, rawText, docsRaw] = await Promise.all([
        invoke<string>("get_project_context", { name: projectName }),
        invoke<string>("read_project_context_raw", { name: projectName }),
        invoke<string>("get_project_docs", { name: projectName }),
      ]);
      setProjectContext(JSON.parse(parsedRaw));
      setProjectDocs(JSON.parse(docsRaw));
      setContextRaw(rawText);
      setContextFileExists(rawText.length > 0);
      setContextEditing(false);
      setContextDirty(false);
      setContextJsonError(null);
    } catch (err: any) {
      console.error("Failed to load project context:", err);
      setProjectContext(null);
      setProjectDocs({});
      setContextRaw("");
      setContextFileExists(false);
    } finally {
      setLoadingContext(false);
    }
  };

  const handleSaveContext = async () => {
    if (!selectedName) return;
    try {
      JSON.parse(contextRaw);
    } catch (e: any) {
      setContextJsonError(`Invalid JSON: ${e.message}`);
      return;
    }
    setContextSaving(true);
    setContextJsonError(null);
    try {
      await invoke("save_project_context_raw", { name: selectedName, content: contextRaw });
      setContextDirty(false);
      setContextEditing(false);
      setContextFileExists(true);
      const [parsed, docsRaw]: [string, string] = await Promise.all([
        invoke<string>("get_project_context", { name: selectedName }),
        invoke<string>("get_project_docs", { name: selectedName }),
      ]);
      setProjectContext(JSON.parse(parsed));
      setProjectDocs(JSON.parse(docsRaw));
      notifyProjectUpdated();
    } catch (err: any) {
      setContextJsonError(`${err}`);
    } finally {
      setContextSaving(false);
    }
  };

  const handleGenerateContext = async () => {
    if (!selectedName) return;
    setContextGenerating(true);
    setContextJsonError(null);
    const entryId = log(`Analysing project "${selectedName}"…`, "running", activeAgentLabel);
    try {
      const generated: string = await invoke("ai_generate_context", { name: selectedName });
      // Pretty-print the returned JSON before putting it in the editor.
      const pretty = JSON.stringify(JSON.parse(generated), null, 2);
      setContextRaw(pretty);
      setContextEditing(true);
      setContextDirty(true);
      update(entryId, `Context generated for "${selectedName}" — review and save`, "success");
    } catch (err: any) {
      setContextJsonError(`Generation failed: ${err}`);
      update(entryId, `Context generation failed: ${err}`, "error");
    } finally {
      setContextGenerating(false);
    }
  };

  // ── Documentation tab helpers ─────────────────────────────────────────────

  /**
   * Return the docs index loaded from `.automatic/docs.json`.
   */
  const parsedDocs = (): ProjectDocsData => projectDocs;

  /**
   * Persist an updated docs index to `.automatic/docs.json`.
   */
  const saveDocsToContext = async (
    newDocs: ProjectDocsData
  ): Promise<void> => {
    if (!selectedName) return;
    const updated = JSON.stringify(newDocs, null, 2);
    await invoke("save_project_docs_raw", { name: selectedName, content: updated });
    setProjectDocs(newDocs);
    setProjectContext((prev) => (prev ? { ...prev, docs: newDocs } : prev));
  };

  /** Add or update a file/dir entry in the docs map. */
  const addDocPath = async (path: string, summary: string): Promise<void> => {
    if (!path.trim()) return;
    const docs = parsedDocs();
    // Use the basename as the key (de-duplicating with a suffix if needed)
    const base = path.split("/").pop() ?? path;
    const key = docs[base] ? `${base}_${Date.now()}` : base;
    await saveDocsToContext({ ...docs, [key]: { path: path.trim(), summary: summary.trim() } });
  };

  /** Add or update a link entry in the docs map. */
  const addDocLink = async (url: string, label: string): Promise<void> => {
    if (!url.trim()) return;
    const docs = parsedDocs();
    const key = (label.trim() || url.trim().replace(/https?:\/\//, "").split("/")[0]) ?? "link";
    const safeKey = docs[key] ? `${key}_${Date.now()}` : key;
    await saveDocsToContext({ ...docs, [safeKey]: { path: url.trim(), summary: label.trim() } });
  };

  const handleBrowseDocPath = async (): Promise<void> => {
    let picked: string | null = null;
    try {
      picked = await invoke<string | null>("open_directory_dialog");
    } catch (err) {
      console.error("open_directory_dialog failed:", err);
    }
    if (picked) setDocNewPath(picked);
  };

  const handleBrowseDocFile = async (): Promise<void> => {
    let picked: string | null = null;
    try {
      picked = await invoke<string | null>("open_file_dialog");
    } catch (err) {
      console.error("open_file_dialog failed:", err);
    }
    if (picked) setDocNewPath(picked);
  };

  const handleAddDocPath = async (): Promise<void> => {
    if (!docNewPath.trim()) return;
    await addDocPath(docNewPath, docNewPathSummary);
    setDocNewPath("");
    setDocNewPathSummary("");
    setShowDocPathForm(false);
  };

  const handleAddDocLink = async (): Promise<void> => {
    if (!docNewLinkUrl.trim()) return;
    await addDocLink(docNewLinkUrl, docNewLinkLabel);
    setDocNewLinkUrl("");
    setDocNewLinkLabel("");
    setShowDocLinkForm(false);
  };

  /** Remove a doc entry by key. Also deletes the note file if it's a note entry. */
  const removeDocEntry = async (key: string, isNote: boolean): Promise<void> => {
    if (!selectedName) return;
    const docs = parsedDocs();
    const { [key]: _removed, ...rest } = docs;
    await saveDocsToContext(rest);
    if (isNote) {
      try {
        await invoke("delete_doc_note", { name: selectedName, noteName: key + ".md" });
      } catch {
        // best-effort — file may not exist yet
      }
      if (docNoteSelected === key) {
        setDocNoteSelected(null);
        setDocNoteContent("");
        setDocNoteDirty(false);
      }
    }
  };

  /** Load the content of a note file into the editor. */
  const loadDocNote = async (key: string): Promise<void> => {
    if (!selectedName) return;
    setDocNoteLoading(true);
    setDocNoteSelected(key);
    setDocNoteDirty(false);
    try {
      const content: string = await invoke("read_doc_note", {
        name: selectedName,
        noteName: key + ".md",
      });
      setDocNoteContent(content);
    } catch (err) {
      console.error("Failed to load doc note:", err);
      setDocNoteContent("");
    } finally {
      setDocNoteLoading(false);
    }
  };

  /** Save the current note editor content to disk. */
  const saveDocNote = async (): Promise<void> => {
    if (!selectedName || !docNoteSelected) return;
    setDocNoteSaving(true);
    try {
      await invoke("save_doc_note", {
        name: selectedName,
        noteName: docNoteSelected + ".md",
        content: docNoteContent,
      });
      setDocNoteDirty(false);
    } catch (err) {
      console.error("Failed to save doc note:", err);
    } finally {
      setDocNoteSaving(false);
    }
  };

  /** Create a new note: adds an index entry to docs.json, then opens the editor. */
  const createDocNote = async (noteName: string): Promise<void> => {
    if (!noteName.trim() || !selectedName) return;
    // Sanitise: lowercase, spaces → hyphens, strip non-alphanumeric except hyphens
    const slug = noteName
      .trim()
      .toLowerCase()
      .replace(/\s+/g, "-")
      .replace(/[^a-z0-9-]/g, "");
    if (!slug) return;
    const docs = parsedDocs();
    if (docs[slug]) {
      // Already exists — just select it
      await loadDocNote(slug);
      return;
    }
    await saveDocsToContext({
      ...docs,
      [slug]: { path: `.automatic/docs/${slug}.md`, summary: noteName.trim() },
    });
    setDocNoteContent("");
    setDocNoteDirty(false);
    setDocNoteSelected(slug);
    setDocNewNoteName("");
    setDocNewNoteCreating(false);
  };

  // ── End documentation tab helpers ────────────────────────────────────────

  // Remove a recommendation from all local state arrays and notify the global
  // Recommendations view to re-fetch. Call this after any dismiss or action so
  // the rollup counts (badge, Recommendations tab) stay accurate.
  const removeRecommendation = (id: number) => {
    setRecommendations((prev) => prev.filter((r) => r.id !== id));
    setAiSkillsSuggestions((prev) => prev.filter((r) => r.id !== id));
    setAiMcpSuggestions((prev) => prev.filter((r) => r.id !== id));
    window.dispatchEvent(new CustomEvent("recommendations-updated"));
  };

  const loadRecommendations = async (projectName: string) => {
    try {
      const [recs, skillRecs, mcpRecs] = await Promise.all([
        invoke<ProjectRecommendation[]>("evaluate_project_recommendations", { project: projectName }),
        invoke<ProjectRecommendation[]>("list_recommendations_by_source", { project: projectName, source: "automatic-ai-skills" }),
        invoke<ProjectRecommendation[]>("list_recommendations_by_source", { project: projectName, source: "automatic-ai-mcp" }),
      ]);
      setRecommendations(recs);
      setAiSkillsSuggestions(skillRecs);
      setAiMcpSuggestions(mcpRecs);
      // Fetch the last AI run timestamp (non-blocking, best-effort).
      invoke<string | null>("get_ai_recommendations_timestamp", { project: projectName })
        .then((ts) => setAiRecsLastRunAt(ts ?? null))
        .catch(() => {});
      // Notify the global Recommendations view so it re-fetches from the DB.
      window.dispatchEvent(new CustomEvent("recommendations-updated"));
    } catch (err: any) {
      console.error("Failed to evaluate recommendations:", err);
      // Non-fatal — clear so stale data isn't shown
      setRecommendations([]);
      setAiSkillsSuggestions([]);
      setAiMcpSuggestions([]);
    }
  };

  const handleUpdateAiRecommendations = async () => {
    if (!selectedName || aiRecsLoading) return;
    setAiRecsLoading(true);
    const entryId = log(`Analysing recommendations for "${selectedName}"…`, "running", activeAgentLabel);
    try {
      const result = await invoke<{ recommendations: ProjectRecommendation[]; last_run_at: string }>(
        "ai_generate_project_recommendations",
        { project: selectedName, force: true },
      );
      setRecommendations(result.recommendations);
      setAiRecsLastRunAt(result.last_run_at);
      window.dispatchEvent(new CustomEvent("recommendations-updated"));
      update(entryId, `Recommendations updated for "${selectedName}"`, "success");
    } catch (err: any) {
      console.error("Failed to generate AI recommendations:", err);
      update(entryId, `Recommendation analysis failed: ${err}`, "error");
    } finally {
      setAiRecsLoading(false);
    }
  };

  const handleSuggestSkills = async () => {
    if (!selectedName || aiSkillsLoading) return;
    setAiSkillsLoading(true);
    const entryId = log(`Suggesting skills for "${selectedName}"…`, "running", activeAgentLabel);
    try {
      const recs = await invoke<ProjectRecommendation[]>("ai_suggest_skills", { project: selectedName });
      const skillRecs = recs.filter((r) => r.source === "automatic-ai-skills" && r.status === "pending");
      setAiSkillsSuggestions(skillRecs);
      window.dispatchEvent(new CustomEvent("recommendations-updated"));
      update(entryId, `Skills suggestions ready for "${selectedName}"`, "success");
    } catch (err: any) {
      console.error("Failed to suggest skills:", err);
      update(entryId, `Skills suggestion failed: ${err}`, "error");
    } finally {
      setAiSkillsLoading(false);
    }
  };

  const handleSuggestMcpServers = async () => {
    if (!selectedName || aiMcpLoading) return;
    setAiMcpLoading(true);
    const entryId = log(`Suggesting MCP servers for "${selectedName}"…`, "running", activeAgentLabel);
    try {
      const recs = await invoke<ProjectRecommendation[]>("ai_suggest_mcp_servers", { project: selectedName });
      const mcpRecs = recs.filter((r) => r.source === "automatic-ai-mcp" && r.status === "pending");
      setAiMcpSuggestions(mcpRecs);
      window.dispatchEvent(new CustomEvent("recommendations-updated"));
      update(entryId, `MCP server suggestions ready for "${selectedName}"`, "success");
    } catch (err: any) {
      console.error("Failed to suggest MCP servers:", err);
      update(entryId, `MCP server suggestion failed: ${err}`, "error");
    } finally {
      setAiMcpLoading(false);
    }
  };

  // Re-evaluate recommendations whenever any project mutation occurs.
  // Callers signal a change by calling notifyProjectUpdated() — no need to
  // wire loadRecommendations into every individual save handler.
  useEffect(() => {
    if (projectVersion === 0 || !selectedName) return;
    loadRecommendations(selectedName);
  }, [projectVersion]); // eslint-disable-line react-hooks/exhaustive-deps

  const loadActivity = async (projectName: string) => {
    try {
      setLoadingActivity(true);
      const raw: string = await invoke("get_project_activity", { project: projectName, limit: 5 });
      setActivityEntries(JSON.parse(raw) as ActivityEntry[]);
    } catch (err: any) {
      console.error("Failed to load activity:", err);
    } finally {
      setLoadingActivity(false);
    }
  };

  const loadActivityPage = async (projectName: string, page: number) => {
    try {
      setLoadingActivityPage(true);
      const offset = page * ACTIVITY_PAGE_SIZE;
      const [raw, count] = await Promise.all([
        invoke<string>("get_project_activity_paged", {
          project: projectName,
          limit: ACTIVITY_PAGE_SIZE,
          offset,
        }),
        invoke<number>("get_project_activity_count", { project: projectName }),
      ]);
      setActivityPageEntries(JSON.parse(raw) as ActivityEntry[]);
      setActivityTotalCount(count);
      setActivityPage(page);
    } catch (err: any) {
      console.error("Failed to load activity page:", err);
    } finally {
      setLoadingActivityPage(false);
    }
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
    if (!selectedName || !activeProjectFile || !project) return;
    setProjectFileSaving(true);
    try {
      // Flush the in-memory project config (including file_rules) to disk first,
      // so save_project_file on the backend reads up-to-date rule assignments.
      // This also handles the case where rules were toggled on a not-yet-existing file.
      const toSave = { ...project, name: selectedName, updated_at: new Date().toISOString() };
      await invoke("save_project", { name: selectedName, data: JSON.stringify(toSave, null, 2) });
      setDirty(false);

      await invoke("save_project_file", {
        name: selectedName,
        filename: activeProjectFile,
        content: projectFileContent,
      });
      setProjectFileDirty(false);

      // Reload file list so the "exists" flag updates for newly created files
      await loadProjectFiles(selectedName);
      notifyProjectUpdated();
    } catch (err: any) {
      setError(`Failed to save project file: ${err}`);
    } finally {
      setProjectFileSaving(false);
    }
  };

  const handleApplyTemplate = async (templateName: string) => {
    try {
      const content: string = await invoke("read_instruction", { name: templateName });
      setProjectFileContent(content);
      setProjectFileDirty(true);
      setProjectFileEditing(true);
      setShowTemplatePicker(false);
    } catch (err: any) {
      setError(`Failed to load template: ${err}`);
    }
  };

  const handleGenerateInstruction = async () => {
    if (!selectedName || !activeProjectFile) return;
    setProjectFileGenerating(true);
    // Resolve a human-readable label: use the agent name(s) rather than the
    // internal "_unified" virtual filename.
    const fileInfo = projectFiles.find((f) => f.filename === activeProjectFile);
    const displayLabel =
      activeProjectFile === "_unified"
        ? (fileInfo?.agents?.join(" & ") ?? "shared instruction file")
        : activeProjectFile;
    const entryId = log(`Generating instruction file for ${displayLabel}…`, "running", activeAgentLabel);
    try {
      const generated: string = await invoke("ai_generate_instruction", {
        name: selectedName,
        filename: activeProjectFile,
      });
      setProjectFileContent(generated);
      setProjectFileEditing(true);
      setProjectFileDirty(true);
      update(entryId, `Instruction file for ${displayLabel} generated — review and save`, "success");
    } catch (err: any) {
      update(entryId, `Instruction generation failed: ${err}`, "error");
    } finally {
      setProjectFileGenerating(false);
    }
  };

  const handleUpdateInstruction = async () => {
    if (!selectedName || !activeProjectFile) return;
    if (!projectFileContent.trim()) return;
    setProjectFileUpdating(true);
    const fileInfo = projectFiles.find((f) => f.filename === activeProjectFile);
    const displayLabel =
      activeProjectFile === "_unified"
        ? (fileInfo?.agents?.join(" & ") ?? "shared instruction file")
        : activeProjectFile;
    const entryId = log(`Updating instruction file for ${displayLabel}…`, "running", activeAgentLabel);
    try {
      const updated: string = await invoke("ai_update_instruction", {
        name: selectedName,
        filename: activeProjectFile,
        currentContent: projectFileContent,
      });
      setProjectFileContent(updated);
      setProjectFileEditing(true);
      setProjectFileDirty(true);
      update(entryId, `Instruction file for ${displayLabel} updated — review and save`, "success");
    } catch (err: any) {
      update(entryId, `Instruction update failed: ${err}`, "error");
    } finally {
      setProjectFileUpdating(false);
    }
  };

  const selectProject = async (name: string) => {
    // If the wizard is open, cancel it (cleans up any saved stub) before loading the selected project.
    if (isCreating) {
      await cancelCreate();
    }
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
      const storedCustomSkills: CustomSkill[] = stored.custom_skills || [];
      const storedMcp: string[] = stored.mcp_servers || [];

      const detectedAgents: string[] = parsed.agents || [];
      const detectedSkills: string[] = parsed.skills || [];
      const detectedCustomSkills: CustomSkill[] = parsed.custom_skills || [];
      const detectedMcp: string[] = parsed.mcp_servers || [];

      // New items found by autodetect that aren't yet in the stored config.
      const newAgents = detectedAgents.filter((a) => !storedAgents.includes(a));
      const newSkills = detectedSkills.filter((s) => !storedSkills.includes(s));
      const storedCustomSkillNames = new Set(storedCustomSkills.map((s) => s.name));
      const newCustomSkills = detectedCustomSkills.filter((s) => !storedCustomSkillNames.has(s.name));
      const newMcp = detectedMcp.filter((m) => !storedMcp.includes(m));

      const detectedDiffers =
        newAgents.length > 0 ||
        newSkills.length > 0 ||
        newCustomSkills.length > 0 ||
        newMcp.length > 0;

      // Normalize: ensure all fields exist with defaults for older projects.
      // Start from stored data and append any newly-detected items.
      const data: Project = {
        name: stored.name || name,
        description: stored.description || "",
        directory: stored.directory || "",
        skills: [...storedSkills, ...newSkills],
        mcp_servers: [...storedMcp, ...newMcp],
        disabled_mcp_servers: stored.disabled_mcp_servers || [],
        providers: stored.providers || [],
        agents: [...storedAgents, ...newAgents],
        created_at: stored.created_at || new Date().toISOString(),
        updated_at: stored.updated_at || new Date().toISOString(),
        file_rules: stored.file_rules || {},
        instruction_mode: stored.instruction_mode || "per-agent",
        agent_options: stored.agent_options,
        custom_rules: stored.custom_rules || [],
        tools: stored.tools || [],
        custom_agents: stored.custom_agents || [],
        user_agents: stored.user_agents || [],
        custom_commands: stored.custom_commands || [],
        user_commands: stored.user_commands || [],
        hooks: stored.hooks || [],
        custom_skills: [...storedCustomSkills, ...newCustomSkills],
        mode: stored.mode === 'silent' ? 'silent' : 'normal',
        directory_missing: stored.directory_missing === true,
      };

      setSelectedName(name);
      localStorage.setItem(LAST_PROJECT_KEY, name);
      setProject(data);
      setProjectDetailsMap((prev) => new Map(prev).set(name, data));
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
      await loadGroups(name);
      await loadActivity(name);
      await loadRecommendations(name);
      await loadContext(name);
      // Reset activity tab pagination for the newly selected project
      setActivityPage(0);
      setActivityPageEntries([]);
      setActivityTotalCount(0);
      // Reset tools group state so a stale sub-tab from a previous project isn't shown
      setToolTab(null);
      setToolEntries([]);
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

  // Reload project state from disk and refresh all dependent UI.
  // Always re-affirms selectedName so that any async state race between
  // isCreating=false and the reload completing cannot drop back to the overview.
  const reloadProject = async (name: string) => {
    try {
      const raw: string = await invoke("read_project", { name });
      const parsed = JSON.parse(raw);
      const data: Project = {
        name: parsed.name || name,
        description: parsed.description || "",
        directory: parsed.directory || "",
        skills: parsed.skills || [],
        mcp_servers: parsed.mcp_servers || [],
        disabled_mcp_servers: parsed.disabled_mcp_servers || [],
        providers: parsed.providers || [],
        agents: parsed.agents || [],
        created_at: parsed.created_at || new Date().toISOString(),
        updated_at: parsed.updated_at || new Date().toISOString(),
        file_rules: parsed.file_rules || {},
        instruction_mode: parsed.instruction_mode || "per-agent",
        agent_options: parsed.agent_options,
        custom_rules: parsed.custom_rules || [],
        custom_agents: parsed.custom_agents || [],
        user_agents: parsed.user_agents || [],
        custom_commands: parsed.custom_commands || [],
        user_commands: parsed.user_commands || [],
        hooks: parsed.hooks || [],
        custom_skills: parsed.custom_skills || [],
        tools: parsed.tools || [],
        instructions_index_mode: parsed.instructions_index_mode || false,
        mode: parsed.mode === 'silent' ? 'silent' : 'normal',
        directory_missing: parsed.directory_missing === true,
      };
      setSelectedName(name);
      setIsCreating(false);
      setProject(data);
      // Keep the overview card in sync whenever a project is reloaded from disk.
      setProjectDetailsMap((prev) => new Map(prev).set(name, data));
      setDirty(false);

      await loadAvailableSkills();
      await loadAvailableMcpServers();
      await loadMemories(name);
      await loadGroups(name);
      await loadActivity(name);
      await loadContext(name);
      notifyProjectUpdated();
      // Reset activity tab pagination on project reload
      setActivityPage(0);
      setActivityPageEntries([]);
      setActivityTotalCount(0);

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

      // In the wizard (isCreating), selectedProjectTemplates represents the final
      // template choices from step 3. Merge them into the project snapshot here so
      // the save is atomic — no React state-update timing issues.
      let effectiveProject = project;
      if (isCreating && selectedProjectTemplates.length > 0) {
        const wizardTemplates = availableProjectTemplates.filter((t) =>
          selectedProjectTemplates.includes(t.name)
        );
        let mergedAgents = [...project.agents];
        let mergedSkills = [...project.skills];
        let mergedMcpServers = [...project.mcp_servers];
        let mergedProviders = [...project.providers];
        let anyUnified = false;
        const wizardPending: { content: string; rules: string[] }[] = [];

        for (const tmpl of wizardTemplates) {
          mergedAgents = [...new Set([...mergedAgents, ...tmpl.agents])];
          mergedSkills = [...new Set([...mergedSkills, ...tmpl.skills])];
          mergedMcpServers = [...new Set([...mergedMcpServers, ...tmpl.mcp_servers])];
          mergedProviders = [...new Set([...mergedProviders, ...tmpl.providers])];
          const hasContent = !!(tmpl.unified_instruction && tmpl.unified_instruction.trim());
          const hasRules = (tmpl.unified_rules || []).length > 0;
          // Collect pending entry for rules and/or content, but only switch to
          // unified mode when there is actual instruction content — rules alone
          // can be applied in per-agent mode without overwriting existing files.
          if (hasContent || hasRules) {
            wizardPending.push({ content: tmpl.unified_instruction || "", rules: tmpl.unified_rules || [] });
          }
          if (hasContent) {
            anyUnified = true;
          }
        }
        effectiveProject = {
          ...project,
          agents: mergedAgents,
          skills: mergedSkills,
          mcp_servers: mergedMcpServers,
          providers: mergedProviders,
          ...(anyUnified ? { instruction_mode: "unified" } : {}),
        };
        if (wizardPending.length > 0) {
          // Merge with any previously stashed pending entries (e.g. from startCreate)
          pendingUnifiedInstruction.current = [
            ...(pendingUnifiedInstruction.current ?? []),
            ...wizardPending,
          ];
        }
      }

      const toSave = { ...effectiveProject, name, updated_at: new Date().toISOString() };
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
        trackProjectCreated(name);
        // Clear the stub reference so the unmount cleanup does not delete the
        // project we just successfully saved.
        wizardStubName.current = null;
        setIsCreating(false);
        setProjectTab("summary");
        setProjectGroup("summary");
        await reloadProjects();
      } else {
        trackProjectUpdated(name, {
          agent_count: toSave.agents.length,
          skill_count: toSave.skills.length,
          mcp_count: (toSave.mcp_servers ?? []).length,
        });
        // Keep the overview card in sync with what was just persisted.
        setProjectDetailsMap((prev) => new Map(prev).set(name, toSave));
      }
      setError(null);

      setSyncStatus(toSave.directory && toSave.agents.length > 0
        ? "Saved & synced"
        : "Saved");
      if (toSave.directory && toSave.agents.length > 0) {
        setDriftReport({ drifted: false, agents: [] });
        setDriftByProject((prev) => ({ ...prev, [name]: false }));
      }

      // Write any pending unified instruction content from one or more template applies.
      // Multiple entries are concatenated with a separator; rules are unioned across all.
      const pending = pendingUnifiedInstruction.current;
      if (pending !== null && pending.length > 0 && toSave.directory && toSave.agents.length > 0) {
        pendingUnifiedInstruction.current = null;
        const mergedRules = [...new Set(pending.flatMap((e) => e.rules))];
        const mergedContent = pending
          .map((e) => e.content)
          .filter(Boolean)
          .join("\n\n---\n\n");
        // If any template had rules, persist them into file_rules._project before writing.
        // Using _project (not _unified) ensures template rules are visible in the Rules tab
        // and are not silently dropped when the user later toggles rules from the Rules UI
        // (which only reads/writes _project).
        if (mergedRules.length > 0) {
          const latestRaw: string = await invoke("read_project", { name });
          const latestProj = JSON.parse(latestRaw);
          const existingProjectRules: string[] = (latestProj.file_rules || {})["_project"] || [];
          const combinedRules = [...new Set([...existingProjectRules, ...mergedRules])];
          const withRules = {
            ...latestProj,
            file_rules: { ...(latestProj.file_rules || {}), _project: combinedRules },
          };
          await invoke("save_project", { name, data: JSON.stringify(withRules, null, 2) });
        }
        if (mergedContent.trim()) {
          await invoke("save_project_file", {
            name,
            filename: "_unified",
            content: mergedContent,
          });
        }
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
    const confirmed = await ask(`Remove project "${name}" from Automatic?\n\n(This only removes the project from this app. Your actual project files will NOT be deleted.)`, { title: "Remove Project", kind: "warning" });
    if (!confirmed) return;
    try {
      await invoke("delete_project", { name });
      trackProjectDeleted(name);
      if (selectedName === name) {
        setSelectedName(null);
        localStorage.removeItem(LAST_PROJECT_KEY);
        setProject(null);
        setDirty(false);
      }
      await reloadProjects();
      window.dispatchEvent(new CustomEvent("project-removed", { detail: { name } }));
      setError(null);
    } catch (err: any) {
      setError(`Failed to remove project: ${err}`);
    }
  };

  const startCreate = async (opts?: { fromTemplates?: ProjectTemplate[] }) => {
    setSelectedName(null);
    localStorage.removeItem(LAST_PROJECT_KEY);
    if (!opts?.fromTemplates?.length) setWizardSourceTemplates([]);
    // Pre-populate agents and agent options from settings defaults
    let defaultAgents: string[] = [];
    let defaultAgentOptions: Record<string, AgentOptions> = {};
    try {
      const raw: any = await invoke("read_settings");
      defaultAgents = raw.default_agents ?? [];
      defaultAgentOptions = raw.default_agent_options ?? {};
    } catch {
      // Non-fatal — proceed with empty agents if settings can't be read
    }

    // If launched from one or more templates, merge all their values into the initial project state.
    setWizardDefaultAgents(defaultAgents);
    const templates = opts?.fromTemplates ?? [];
    const baseProject = {
      ...emptyProject(""),
      agents: defaultAgents,
      ...(Object.keys(defaultAgentOptions).length > 0
        ? { agent_options: defaultAgentOptions }
        : {}),
    };

    let mergedAgents = [...defaultAgents];
    let mergedSkills: string[] = [];
    let mergedMcpServers: string[] = [];
    let mergedProviders: string[] = [];
    let mergedDescription = "";
    let anyUnified = false;
    const pendingEntries: { content: string; rules: string[] }[] = [];

    for (const tmpl of templates) {
      mergedAgents = [...new Set([...mergedAgents, ...tmpl.agents])];
      mergedSkills = [...new Set([...mergedSkills, ...tmpl.skills])];
      mergedMcpServers = [...new Set([...mergedMcpServers, ...tmpl.mcp_servers])];
      mergedProviders = [...new Set([...mergedProviders, ...tmpl.providers])];
      if (!mergedDescription) mergedDescription = tmpl.description || "";
      const hasContent = !!(tmpl.unified_instruction && tmpl.unified_instruction.trim());
      const hasRules = (tmpl.unified_rules || []).length > 0;
      // Collect pending entry for rules and/or content, but only switch to
      // unified mode when there is actual instruction content — rules alone
      // can be applied in per-agent mode without overwriting existing files.
      if (hasContent || hasRules) {
        pendingEntries.push({ content: tmpl.unified_instruction || "", rules: tmpl.unified_rules || [] });
      }
      if (hasContent) {
        anyUnified = true;
      }
    }

    const initialProject = templates.length > 0
      ? {
          ...baseProject,
          description: mergedDescription,
          agents: mergedAgents,
          skills: mergedSkills,
          mcp_servers: mergedMcpServers,
          providers: mergedProviders,
          ...(anyUnified ? { instruction_mode: "unified" as const } : {}),
        }
      : baseProject;

    if (pendingEntries.length > 0) {
      pendingUnifiedInstruction.current = pendingEntries;
    }

    setProject(initialProject);
    setDirty(true);
    setIsCreating(true);
    setNewName("");
    setSelectedProjectTemplates(templates.map((t) => t.name));
    setShowProjectTemplatePicker(false);
    setWizardStep(1);
    setWizardDiscoveredAgents([]);
    setWizardDiscovering(false);
    wizardStubName.current = null;
  };

  /**
   * Cancel an in-progress project creation wizard.
   * If a stub was already saved to disk (after step 1 "Continue"), delete it so
   * it does not appear as a broken project in the project list.
   */
  const cancelCreate = async () => {
    const stub = wizardStubName.current;
    wizardStubName.current = null;
    setIsCreating(false);
    setProject(null);
    setDirty(false);
    setError(null);
    pendingUnifiedInstruction.current = null;
    if (stub) {
      try {
        await invoke("delete_project", { name: stub });
      } catch {
        // Non-fatal — stub cleanup is best-effort
      }
      await reloadProjects();
    }
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
      await reloadProjects();
      await selectProject(trimmed);
    } catch (err: any) {
      setError(`Failed to rename project: ${err}`);
      setIsRenaming(false);
    }
  };

  // ── List helpers ─────────────────────────────────────────────────────────

  type ListField = "skills" | "mcp_servers" | "providers" | "agents";

  // Persist a project snapshot directly — used by addItem/removeItem so they
  // can pass the already-computed new value without waiting for a React state flush.
  const saveProjectSnapshot = async (snapshot: Project): Promise<boolean> => {
    const folderFallback = snapshot.directory?.split("/").filter(Boolean).pop() ?? "";
    const name = isCreating ? (newName.trim() || folderFallback) : selectedName;
    if (!name) return false;
    try {
      const toSave = { ...snapshot, name, updated_at: new Date().toISOString() };
      await invoke("save_project", { name, data: JSON.stringify(toSave, null, 2) });
      // Re-read the project — the backend may have enriched it (e.g. plugin
      // skills/rules added when a plugin tool is toggled on).
      let saved = toSave;
      try {
        const raw: string = await invoke("read_project", { name });
        saved = JSON.parse(raw);
        setProject(saved);
      } catch { /* fall back to pre-save snapshot */ }
      setSyncStatus(saved.directory && saved.agents.length > 0 ? "Saved & synced" : "Saved");
      setProjectDetailsMap((prev) => new Map(prev).set(name, saved));
      setDirty(false);
      return true;
    } catch (err: any) {
      console.error("Autosave failed:", err);
      setSyncStatus(`Save failed: ${err}`);
      return false;
    }
  };

  const addItem = async (key: ListField, item: string): Promise<boolean> => {
    if (!project || !item.trim()) return false;
    if (project[key].includes(item.trim())) return true;
    const newList = [...project[key], item.trim()];
    updateField(key, newList);
    const pName = isCreating ? newName.trim() : (selectedName ?? "");
    if (key === "agents") trackProjectAgentAdded(pName, item.trim());
    else if (key === "skills") {
      trackProjectSkillAdded(pName, item.trim());
      return await saveProjectSnapshot({ ...project, skills: newList as string[] });
    } else if (key === "mcp_servers") {
      trackProjectMcpServerAdded(pName, item.trim());
      const nextProject = {
        ...project,
        mcp_servers: newList as string[],
        disabled_mcp_servers: (project.disabled_mcp_servers || []).filter((name) => name !== item.trim()),
      };
      setProject(nextProject);
      return await saveProjectSnapshot(nextProject);
    }
    return true;
  };

  const removeItem = (key: ListField, idx: number) => {
    if (!project) return;
    const removed = project[key][idx];
    const newList = project[key].filter((_, i) => i !== idx);
    updateField(key, newList);
    const pName = isCreating ? newName.trim() : (selectedName ?? "");
    if (removed) {
      if (key === "agents") trackProjectAgentRemoved(pName, removed);
      else if (key === "skills") {
        trackProjectSkillRemoved(pName, removed);
        saveProjectSnapshot({ ...project, skills: newList as string[] });
      } else if (key === "mcp_servers") {
        trackProjectMcpServerRemoved(pName, removed);
        const nextProject = {
          ...project,
          mcp_servers: newList as string[],
          disabled_mcp_servers: (project.disabled_mcp_servers || []).filter((name) => name !== removed),
        };
        setProject(nextProject);
        saveProjectSnapshot(nextProject);
      }
    }
  };

  const isMcpServerEnabled = (server: string): boolean => {
    if (!project || server === "automatic") return true;
    return !(project.disabled_mcp_servers || []).includes(server);
  };

  const toggleMcpServerEnabled = async (server: string, enabled: boolean) => {
    if (!project || server === "automatic") return;
    const disabledServers = project.disabled_mcp_servers || [];
    const nextDisabledServers = enabled
      ? disabledServers.filter((name) => name !== server)
      : [...new Set([...disabledServers, server])];

    const nextProject = { ...project, disabled_mcp_servers: nextDisabledServers };
    setProject(nextProject);
    setDirty(true);
    await saveProjectSnapshot(nextProject);
  };

  const handleDismissRecommendation = async (id: number) => {
    try {
      await invoke("dismiss_recommendation", { id });
      setRecommendations((prev) => prev.filter((r) => r.id !== id));
    } catch (err: any) {
      console.error("Failed to dismiss recommendation:", err);
    }
  };

  /**
   * Remove an agent from the project, prompting for confirmation and cleaning
   * up the agent's config files and skill directories from the project directory.
   *
   * If the project has no directory (not yet synced), falls back to an in-memory
   * removal so the user can save later.
   */
  const handleRemoveAgent = async (idx: number) => {
    if (!project) return;
    const agentId = project.agents[idx];
    if (!agentId) return;

    const agentInfo = availableAgents.find((a) => a.id === agentId);
    const agentLabel = agentInfo?.label ?? agentId;

    // If no directory or project not yet persisted → in-memory removal only
    if (!project.directory || !selectedName || isCreating) {
      const message = `Remove ${agentLabel} from this project?\n\nNo config files will be deleted since no project directory is configured.`;
      const confirmed = await ask(message, { title: "Remove Agent", kind: "warning" });
      if (!confirmed) return;
      removeItem("agents", idx);
      return;
    }

    const name = selectedName; // narrowed: guaranteed non-null from here on

    // Fetch the list of files that would be cleaned up (read-only preview)
    let preview: string[] = [];
    try {
      const raw: string = await invoke("get_agent_cleanup_preview", { name, agentId });
      preview = JSON.parse(raw);
    } catch {
      // Non-fatal — proceed with a generic message if the preview fails
    }

    const fileList =
      preview.length > 0
        ? `\n\nThe following files and directories will be deleted:\n${preview.map((p) => `  • ${p}`).join("\n")}`
        : "\n\nNo config files were found on disk for this agent.";

    const confirmed = await ask(
      `Remove ${agentLabel} from this project?${fileList}`,
      { title: "Remove Agent", kind: "warning" }
    );
    if (!confirmed) return;

    try {
      await invoke("remove_agent_from_project", { name, agentId });
      trackProjectAgentRemoved(name, agentId);
      await reloadProject(name);
      setDirty(false);
    } catch (err: any) {
      setError(`Failed to remove agent: ${err}`);
    }
  };

  // ── Instruction file conflict resolution ──────────────────────────────────

  /** User chose "Use existing file" — adopt the on-disk content into the editor. */
  const handleAdoptInstructionFile = async (filename: string, adoptedContent: string) => {
    const name = selectedName;
    if (!name) return;
    try {
      await invoke("adopt_instruction_file", { name, filename });
      // Update the editor state so it reflects the adopted content.
      if (activeProjectFile === filename || activeProjectFile === "_unified") {
        setProjectFileContent(adoptedContent);
        setProjectFileDirty(false);
      }
      // Re-run drift check: conflict should now be gone.
      const raw: string = await invoke("check_project_drift", { name });
      const report = JSON.parse(raw) as DriftReport;
      setDriftReport(report);
      setDriftByProject((prev) => ({ ...prev, [name]: report.drifted }));
      notifyProjectUpdated();
    } catch (err: any) {
      setError(`Failed to adopt instruction file: ${err}`);
    } finally {
      setInstructionConflict(null);
    }
  };

  /** User chose "Overwrite with Automatic content" — wipe the externally-added content. */
  const handleOverwriteInstructionFile = async (filename: string) => {
    const name = selectedName;
    if (!name) return;
    try {
      await invoke("overwrite_instruction_file", { name, filename });
      // Clear the editor content to reflect the overwrite.
      if (activeProjectFile === filename || activeProjectFile === "_unified") {
        setProjectFileContent("");
        setProjectFileDirty(false);
      }
      // Re-run drift check.
      const raw: string = await invoke("check_project_drift", { name });
      const report = JSON.parse(raw) as DriftReport;
      setDriftReport(report);
      setDriftByProject((prev) => ({ ...prev, [name]: report.drifted }));
      notifyProjectUpdated();
    } catch (err: any) {
      setError(`Failed to overwrite instruction file: ${err}`);
    } finally {
      setInstructionConflict(null);
    }
  };

  /** Re-check drift after a stale skill was adopted, removed, or overwritten. */
  const handleDriftResolved = async () => {
    const name = selectedName;
    if (!name) return;
    try {
      const raw: string = await invoke("check_project_drift", { name });
      const report = JSON.parse(raw) as DriftReport;
      setDriftReport(report);
      setDriftByProject((prev) => ({ ...prev, [name]: report.drifted }));
      notifyProjectUpdated();
      // Re-read the project to pick up config changes (e.g. adopted skill).
      const projRaw: string = await invoke("read_project", { name });
      setProject(JSON.parse(projRaw));
    } catch {
      // Silently ignore — next periodic check will catch up.
    }
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
      trackProjectSynced(name);
      setSyncStatus(`Synced ${files.length} config${files.length !== 1 ? "s" : ""}`);
      setDriftReport({ drifted: false, agents: [] });
      setDriftByProject((prev) => ({ ...prev, [name]: false }));
      notifyProjectUpdated();
    } catch (err: any) {
      setSyncStatus(`Sync failed: ${err}`);
    }

    setTimeout(() => setSyncStatus(null), 4000);
  };

  const handleRebuild = async () => {
    const name = isCreating ? newName.trim() : selectedName;
    if (!name) return;

    try {
      setSyncStatus("Preparing rebuild...");
      const rawPreview = await invoke<RebuildPreview | string>("preview_rebuild_project", { name });
      const preview = parseInvokeResult<RebuildPreview>(rawPreview);
      setRebuildPreview(preview);
      setSyncStatus(null);
    } catch (err: any) {
      setSyncStatus(`Rebuild failed: ${err}`);
      setTimeout(() => setSyncStatus(null), 4000);
    }
  };

  const confirmRebuild = async () => {
    const name = isCreating ? newName.trim() : selectedName;
    if (!name) return;

    try {
      setRebuildBusy(true);
      setSyncStatus("Rebuilding...");
      await invoke("rebuild_project", { name });
      await reloadProject(name);
      setDirty(false);
      setDriftReport({ drifted: false, agents: [] });
      setDriftByProject((prev) => ({ ...prev, [name]: false }));
      notifyProjectUpdated();
      setRebuildPreview(null);
      setSyncStatus("Rebuilt project state");
    } catch (err: any) {
      setSyncStatus(`Rebuild failed: ${err}`);
    } finally {
      setRebuildBusy(false);
    }

    setTimeout(() => setSyncStatus(null), 4000);
  };

  const handleOpenInEditor = async (editorId: string) => {
    if (!project?.directory) return;
    setOpenInDropdownOpen(false);
    try {
      if (editorId === "copy_path") {
        await navigator.clipboard.writeText(project.directory);
      } else {
        await invoke("open_in_editor", { editorId, path: project.directory });
      }
    } catch (err: any) {
      setError(`Failed to open in editor: ${err}`);
    }
  };

  // Deselect: ask the router to go back to the overview.
  const handleBackToOverview = () => {
    onBack();
  };

  return (
    <>
    <div className="h-full w-full bg-bg-base overflow-hidden">
      {/* Project detail */}
      <div className="flex flex-col h-full bg-bg-base">
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
            {/* ── Top action bar: back + buttons ─────────────────── */}
            <div className="h-11 px-4 border-b border-border-strong/40 flex justify-between items-center flex-shrink-0">
              {/* Back to overview */}
              <button
                onClick={handleBackToOverview}
                className="flex items-center gap-1 text-text-muted hover:text-text-base transition-colors px-2 py-1 rounded hover:bg-bg-sidebar"
                title="Back to all projects"
              >
                <ChevronLeft size={14} />
                <span className="text-[12px]">Projects</span>
              </button>

              <div className="flex items-center gap-2">
                {syncStatus && syncStatus !== "syncing" && (
                  <span className={`text-[12px] ${syncStatus.startsWith("Sync failed") ? "text-danger" : "text-success"}`}>
                    {syncStatus}
                  </span>
                )}
                {/* Rebuild button */}
                {!isCreating && selectedName && (
                  <span className="relative group/keytip">
                    <button
                      onClick={handleRebuild}
                      aria-label="Rebuild"
                      className="flex items-center justify-center h-7 w-7 bg-bg-input hover:bg-surface-hover text-text-muted hover:text-text-base rounded transition-colors"
                    >
                      <RotateCcw size={12} />
                    </button>
                    <span className="pointer-events-none absolute top-full left-1/2 -translate-x-1/2 mt-1.5 whitespace-nowrap rounded bg-bg-input-dark border border-border-strong/40 px-2 py-1 text-[11px] text-text-base shadow-md opacity-0 group-hover/keytip:opacity-100 transition-opacity z-10">
                      Rebuild
                    </span>
                  </span>
                )}
                {/* Apply Template button */}
                {!isCreating && selectedName && (
                  <span className="relative group/keytip">
                    <button
                      onClick={() => {
                        setTemplateApplySelection(null);
                        setShowProjectTemplatePicker(true);
                      }}
                      aria-label="Apply Template"
                      className="flex items-center justify-center h-7 w-7 bg-bg-input hover:bg-brand/10 text-text-muted hover:text-brand rounded transition-colors"
                    >
                      <LayoutTemplate size={12} />
                    </button>
                    <span className="pointer-events-none absolute top-full left-1/2 -translate-x-1/2 mt-1.5 whitespace-nowrap rounded bg-bg-input-dark border border-border-strong/40 px-2 py-1 text-[11px] text-text-base shadow-md opacity-0 group-hover/keytip:opacity-100 transition-opacity z-10">
                      Apply Template
                    </span>
                  </span>
                )}
                {/* Open in editor dropdown — only shown when a directory is set */}
                {!isCreating && project.directory && (
                  <div className="relative group/keytip" ref={openInDropdownRef}>
                    <button
                      onClick={() => setOpenInDropdownOpen((v) => !v)}
                      className="flex items-center justify-center h-7 w-7 bg-bg-input hover:bg-surface-hover text-text-base rounded transition-colors"
                      aria-label="Open in editor"
                    >
                      <FolderOpen size={12} />
                    </button>
                    {!openInDropdownOpen && (
                      <span className="pointer-events-none absolute top-full left-1/2 -translate-x-1/2 mt-1.5 whitespace-nowrap rounded bg-bg-input-dark border border-border-strong/40 px-2 py-1 text-[11px] text-text-base shadow-md opacity-0 group-hover/keytip:opacity-100 transition-opacity z-10">
                        Open in
                      </span>
                    )}
                    {openInDropdownOpen && (
                      <div className="absolute right-0 top-full mt-1 w-44 bg-bg-input border border-border-strong/40 rounded-lg shadow-xl z-50 py-1 overflow-hidden">
                        {installedEditors.filter((e) => e.installed).map((editor) => (
                          <button
                            key={editor.id}
                            onClick={() => handleOpenInEditor(editor.id)}
                            className="w-full flex items-center gap-2.5 px-3 py-2 text-[13px] text-text-base hover:bg-bg-sidebar transition-colors text-left"
                          >
                            <EditorIcon id={editor.id} iconPath={editorIconPaths[editor.id]} />
                            {editor.label}
                          </button>
                        ))}
                        <div className="border-t border-border-strong/40 my-1" />
                        <button
                          onClick={() => handleOpenInEditor("copy_path")}
                          className="w-full flex items-center gap-2.5 px-3 py-2 text-[13px] text-text-muted hover:bg-bg-sidebar hover:text-text-base transition-colors text-left"
                        >
                          <Copy size={13} />
                          Copy path
                        </button>
                      </div>
                    )}
                  </div>
                )}
                {!isCreating && selectedName && (
                  <span className="relative group/keytip">
                    <button
                      onClick={() => handleRemove(selectedName)}
                      className="flex items-center justify-center h-7 w-7 bg-bg-input hover:bg-danger/10 text-text-base hover:text-danger rounded transition-colors"
                      aria-label="Remove project"
                    >
                      <Trash2 size={12} />
                    </button>
                    <span className="pointer-events-none absolute top-full right-0 mt-1.5 whitespace-nowrap rounded bg-bg-input-dark border border-border-strong/40 px-2 py-1 text-[11px] text-text-base shadow-md opacity-0 group-hover/keytip:opacity-100 transition-opacity z-10">
                      Remove
                    </span>
                  </span>
                )}
                {/* Sync / in-sync indicator — shown when project has directory + agents configured */}
                {!dirty && project.directory && project.agents.length > 0 && (
                  syncStatus === "syncing" ? (
                    <button
                      disabled
                      className="flex items-center gap-1.5 px-3 py-1 bg-bg-input text-brand rounded text-[12px] font-medium transition-colors opacity-80 cursor-not-allowed"
                      title="Synchronising…"
                    >
                      <RefreshCw size={12} className="animate-spin" /> Syncing…
                    </button>
                  ) : driftReport?.drifted ? (
                    <button
                      onClick={handleSync}
                      className="flex items-center gap-1.5 px-3 py-1 bg-bg-input hover:bg-warning/10 text-warning rounded text-[12px] font-medium transition-colors"
                      title="Configuration has drifted — click to sync"
                    >
                      <RefreshCw size={12} /> Sync Configs
                    </button>
                  ) : driftReport && !driftReport.drifted ? (
                    <button
                      onClick={handleSync}
                      className="flex items-center gap-1.5 px-3 py-1 bg-bg-input hover:bg-success/10 text-success rounded text-[12px] font-medium transition-colors"
                      title="Configuration is up to date — click to force sync"
                    >
                      <Check size={12} /> In Sync
                    </button>
                  ) : (
                    /* driftReport === null: not yet checked */
                    <button
                      onClick={handleSync}
                      className="flex items-center gap-1.5 px-3 py-1 bg-bg-input hover:bg-surface-hover text-text-muted hover:text-text-base rounded text-[12px] font-medium transition-colors"
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
                    className="flex items-center gap-1.5 px-3 py-1.5 bg-brand hover:bg-brand-hover text-white rounded text-[12px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-sm"
                  >
                    <Check size={12} /> Save
                  </button>
                )}
              </div>
            </div>

            {/* ── Sync progress bar ──────────────────────────────── */}
            {syncStatus === "syncing" && (
              <div className="h-1 w-full bg-brand/10 overflow-hidden flex-shrink-0">
                <div
                  className="h-full w-1/3 bg-brand rounded-full"
                  style={{
                    animation: "sync-progress 1.2s ease-in-out infinite",
                  }}
                />
              </div>
            )}

            {/* ── Project title ───────────────────────────────────── */}
            {!isCreating && (
              <div className="px-6 pt-5 pb-4 border-b border-border-strong/40 flex-shrink-0 flex items-start justify-between gap-4">
                {/* Left: name + directory */}
                <div className="min-w-0 flex-1">
                  {isRenaming ? (
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
                      className="bg-transparent border-none outline-none text-[22px] font-semibold text-text-base placeholder-text-muted/50 w-full"
                    />
                  ) : (
                    <h1
                      className="text-[22px] font-semibold text-text-base cursor-text leading-tight"
                      onDoubleClick={startRename}
                      title="Double-click to rename"
                    >
                      {selectedName}
                    </h1>
                  )}
                  {/* Directory path — click to change */}
                  <button
                    onClick={async () => {
                      let selected: string | null = null;
                      try {
                        selected = await invoke<string | null>("open_directory_dialog");
                      } catch (err) {
                        console.error("open_directory_dialog failed:", err);
                      }
                      if (selected) updateField("directory", selected);
                    }}
                    className="mt-1 flex items-center gap-1.5 text-[11px] text-text-muted hover:text-text-base font-mono transition-colors group"
                    title="Click to change directory"
                  >
                    <FolderOpen size={11} className="flex-shrink-0 text-text-muted/60 group-hover:text-text-muted transition-colors" />
                    {project.directory
                      ? <span className="truncate max-w-[480px]">{project.directory.replace(/^\/Users\/[^/]+/, "~")}</span>
                      : <span className="italic text-text-muted/50">No directory set — click to choose</span>
                    }
                  </button>
                </div>

                {/* Right: agent icons + group pills */}
                <div className="flex flex-col items-end flex-shrink-0">
                  {project.agents.length > 0 && (
                    <button
                      onClick={() => selectTab("agents")}
                      className="flex items-center gap-1.5 mt-1 group"
                      title="Agents — click to manage"
                    >
                      {project.agents.map((agentId) => (
                        <span
                          key={agentId}
                          className="opacity-70 group-hover:opacity-100 transition-opacity"
                          title={availableAgents.find(a => a.id === agentId)?.label ?? agentId}
                        >
                          <AgentIcon agentId={agentId} size={20} />
                        </span>
                      ))}
                    </button>
                  )}
                  {projectGroupMemberships.length > 0 && (
                    <div className="flex flex-wrap justify-end gap-1.5 mt-1.5">
                      {projectGroupMemberships.map((groupName) => (
                        <button
                          key={groupName}
                          onClick={() => selectTab("groups")}
                          className="rounded-full border border-border-strong/40 bg-bg-sidebar px-2 py-0.5 text-[11px] text-text-muted transition-colors hover:text-text-base hover:border-border-strong"
                        >
                          {groupName}
                        </button>
                      ))}
                    </div>
                  )}
                </div>
              </div>
            )}

            {/* ── Missing directory banner ─────────────────────────── */}
            {!isCreating && project.directory_missing && (
              <div className="border-b border-danger/30 bg-danger/10 px-6 py-4 flex items-start gap-3 flex-shrink-0">
                <AlertCircle size={16} className="text-danger shrink-0 mt-0.5" />
                <div className="flex-1 min-w-0">
                  <div className="text-[13px] font-semibold text-danger mb-0.5">Project folder not found</div>
                  <div className="text-[11px] font-mono text-danger/70 truncate">{project.directory}</div>
                  <div className="text-[11px] text-danger/60 mt-1">This folder has been moved or deleted. Relink it to continue syncing.</div>
                </div>
                <div className="flex items-center gap-2 flex-shrink-0 mt-0.5">
                  <button
                    onClick={async () => {
                      let selected: string | null = null;
                      try {
                        selected = await invoke<string | null>("open_directory_dialog");
                      } catch (err) {
                        console.error("open_directory_dialog failed:", err);
                      }
                      if (!selected || !project || !selectedName) return;
                      const updatedProject = { ...project, directory: selected, directory_missing: false };
                      setProject(updatedProject);
                      setDirty(false);
                      setSyncStatus("syncing");
                      try {
                        await invoke("save_project", {
                          name: selectedName,
                          data: JSON.stringify(updatedProject),
                        });
                        await reloadProject(selectedName);
                        setSyncStatus("saved");
                        setTimeout(() => setSyncStatus(null), 4000);
                      } catch (err: any) {
                        setSyncStatus(null);
                        setError(`Failed to relink project: ${err}`);
                      }
                    }}
                    className="flex items-center gap-1.5 px-3 py-1.5 text-[12px] font-medium rounded-md border border-brand/40 bg-brand/10 text-brand hover:bg-brand/20 transition-colors"
                  >
                    <LinkIcon size={12} />
                    Relink folder
                  </button>
                  <button
                    onClick={() => handleRemove(selectedName!)}
                    className="flex items-center gap-1.5 px-3 py-1.5 text-[12px] font-medium rounded-md border border-danger/40 bg-danger/5 text-danger hover:bg-danger/15 transition-colors"
                  >
                    <Trash2 size={12} />
                    Delete project
                  </button>
                </div>
              </div>
            )}

            {/* ── Silent mode banner ───────────────────────────────── */}
            {!isCreating && project.mode === 'silent' && project.directory && (
              <div className="border-b border-border-strong/30 bg-bg-input px-6 py-3 flex items-start gap-3 flex-shrink-0">
                <EyeOff size={14} className="text-text-muted shrink-0 mt-0.5" />
                <div className="flex-1 min-w-0">
                  <div className="text-[12px] font-semibold text-text-base mb-0.5">Silent mode enabled</div>
                  <div className="text-[11px] text-text-muted leading-relaxed">
                    Automatic writes all files to{" "}
                    <code className="bg-bg-sidebar px-1 rounded text-[10px]">.automatic/silent/</code>{" "}
                    instead of the project root, leaving existing files untouched.
                  </div>
                </div>
                <button
                  onClick={() => {
                    const silentDir = `${project.directory}/.automatic/silent`;
                    const prompt = [
                      `This project uses Automatic in Silent mode.`,
                      `All Automatic-generated config is stored under \`.automatic/silent/\` rather than the project root.`,
                      ``,
                      `When looking for agent configuration, check:`,
                      `- Instruction files (CLAUDE.md, AGENTS.md, etc.): \`.automatic/silent/\``,
                      `- Skills: \`.automatic/silent/.agents/skills/\``,
                      `- Sub-agents: \`.automatic/silent/.claude/agents/\` (or equivalent for your agent)`,
                      `- MCP config: \`.automatic/silent/.claude/\` (or equivalent)`,
                      `- Commands: \`.automatic/silent/.agents/commands/\``,
                      ``,
                      `Full path: \`${silentDir}\``,
                    ].join('\n');
                    navigator.clipboard.writeText(prompt).catch(() => {});
                  }}
                  className="flex-shrink-0 flex items-center gap-1.5 px-2.5 py-1.5 text-[11px] font-medium rounded-md border border-border-strong/40 bg-bg-sidebar text-text-muted hover:text-text-base hover:border-border-strong/60 transition-colors"
                >
                  <Copy size={11} />
                  Copy prompt
                </button>
              </div>
            )}

            {/* ── Problems banner ──────────────────────────────────── */}
            {problemsReport?.has_problems && !dirty && !isCreating && project.directory && project.agents.length > 0 && (
              <div className="border-b border-danger/25 bg-danger/10">
                <div className="flex items-center gap-2 px-6 py-2 text-danger text-[12px]">
                  <AlertCircle size={13} className="shrink-0" />
                  <span className="font-medium">
                    {problemsReport.problems.length === 1 ? "1 configuration problem detected" : `${problemsReport.problems.length} configuration problems detected`}
                  </span>
                </div>
                <div className="px-6 pb-3 space-y-2">
                  {problemsReport.problems.map((problem, i) => (
                    <div key={i} className="rounded-md border border-danger/20 bg-danger/5 px-3 py-2">
                      <div className="flex items-start justify-between gap-2">
                        <div className="flex-1 min-w-0">
                          <div className="text-[12px] font-semibold text-danger mb-0.5">{problem.title}</div>
                          <div className="text-[11px] text-danger/80 leading-snug">{problem.description}</div>
                          {problem.resources.length > 0 && (
                            <div className="flex flex-wrap gap-1 mt-1.5">
                              {problem.resources.map((r) => (
                                <span key={r} className="font-mono text-[10px] bg-danger/10 border border-danger/20 rounded px-1.5 py-0.5 text-danger/70">
                                  {r}
                                </span>
                              ))}
                            </div>
                          )}
                        </div>
                        {problem.reference_url && (
                          <a
                            href={problem.reference_url}
                            onClick={handleExternalLinkClick(problem.reference_url)}
                            className="shrink-0 flex items-center gap-1 text-[11px] text-danger/70 hover:text-danger underline decoration-danger/30 transition-colors mt-0.5"
                          >
                            <ExternalLink size={10} />
                            Docs
                          </a>
                        )}
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {/* ── Drift warning banner ─────────────────────────────── */}
            {/* Drift describes real on-disk state independent of any in-memory edits,
                so the banner stays visible even when the project is `dirty`. */}
            {driftReport?.drifted && !isCreating && project.directory && project.agents.length > 0 && (
              <div className="border-b border-warning/25 bg-warning/10">
                <div className="flex items-center justify-between px-6 py-2 text-warning">
                  <div className="flex items-center gap-2 text-[12px]">
                    <AlertCircle size={13} />
                    <span>Configuration has drifted — agent config files no longer match Automatic settings.</span>
                  </div>
                  <button
                    onClick={handleSync}
                    disabled={syncStatus === "syncing"}
                    className="text-[12px] font-medium text-warning hover:text-warning-hover underline decoration-warning/40 hover:decoration-warning-hover transition-colors ml-4 flex-shrink-0 disabled:opacity-50 disabled:cursor-not-allowed"
                  >
                    {syncStatus === "syncing" ? "Syncing…" : "Sync now"}
                  </button>
                </div>
                {/* Detail: which agents/files have drifted — click any file to view the diff */}
                <div className="px-6 pb-3 space-y-1.5">
                  {driftReport.agents.map((agentDrift) => (
                    <div key={agentDrift.agent_id}>
                      <div className="text-[11px] font-semibold text-warning/80 mb-0.5">{agentDrift.agent_label}</div>
                      <div className="flex flex-wrap gap-x-2 gap-y-1">
                        {agentDrift.files.map((f, i) => (
                          <button
                            key={i}
                            onClick={() => setDriftDiffFile({ file: f, agentLabel: agentDrift.agent_label })}
                            className="flex items-center gap-1 text-[11px] font-mono text-warning/70 hover:text-warning bg-warning/5 hover:bg-warning/15 border border-warning/20 hover:border-warning/40 rounded px-1.5 py-0.5 transition-colors"
                            title="View diff"
                          >
                            {f.path}
                            <span className="text-warning/50 font-sans ml-0.5">({f.reason})</span>
                          </button>
                        ))}
                      </div>
                    </div>
                  ))}
                  {/* Instruction file conflicts within the drift banner */}
                  {(driftReport.instruction_conflicts ?? []).length > 0 && (
                    <div>
                      <div className="text-[11px] font-semibold text-warning/80 mb-0.5">Instruction files</div>
                      <div className="flex flex-wrap gap-x-2 gap-y-1">
                        {(driftReport.instruction_conflicts ?? []).map((c) => (
                          <button
                            key={c.filename}
                            onClick={() => setInstructionConflict(c)}
                            className="flex items-center gap-1 text-[11px] font-mono text-warning/70 hover:text-warning bg-warning/5 hover:bg-warning/15 border border-warning/20 hover:border-warning/40 rounded px-1.5 py-0.5 transition-colors"
                            title="Resolve conflict"
                          >
                            {c.filename}
                            <span className="text-warning/50 font-sans ml-0.5">(conflict)</span>
                          </button>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              </div>
            )}

            {/* ── New project wizard (3 steps) ─────────────────────── */}
            {isCreating && (
              <div className="flex-1 flex flex-col items-center justify-center p-8">
                <div className="w-full max-w-md relative">

                  {/* Cancel wizard button */}
                  <button
                    onClick={cancelCreate}
                    className="absolute -top-2 right-0 flex items-center gap-1.5 text-[12px] text-text-muted hover:text-text-base transition-colors"
                    title="Cancel project creation"
                  >
                    <X size={13} /> Cancel
                  </button>

                  {/* Template source badge */}
                  {wizardSourceTemplates.length > 0 && (
                    <div className="flex items-center justify-center gap-2 mb-5">
                      <div className="flex items-center gap-2 px-3 py-1.5 bg-brand/10 border border-brand/30 rounded-full">
                        <LayoutTemplate size={12} className="text-brand" />
                        <span className="text-[12px] text-brand font-medium">From template{wizardSourceTemplates.length > 1 ? "s" : ""}: {wizardSourceTemplates.join(", ")}</span>
                      </div>
                    </div>
                  )}

                  {/* Step indicator */}
                  <div className="flex items-center justify-center gap-2 mb-8">
                    {([1, 2, 3] as const).map((s) => (
                      <div key={s} className="flex items-center gap-2">
                        <div className={`w-6 h-6 rounded-full flex items-center justify-center text-[11px] font-semibold transition-colors ${
                          wizardStep === s
                            ? "bg-brand text-white"
                            : wizardStep > s
                            ? "bg-brand/30 text-brand"
                            : "bg-bg-sidebar text-text-muted"
                        }`}>
                          {wizardStep > s ? <Check size={11} /> : s}
                        </div>
                        {s < 3 && (
                          <div className={`w-8 h-px ${wizardStep > s ? "bg-brand/50" : "bg-surface"}`} />
                        )}
                      </div>
                    ))}
                  </div>

                  {/* ── Step 1: Directory ──────────────────────────────── */}
                  {wizardStep === 1 && (
                    <>
                      <div className="mb-8 text-center">
                        <div className="w-14 h-14 mx-auto mb-4 rounded-full bg-brand/10 border border-brand/30 flex items-center justify-center">
                          <FolderOpen size={24} className="text-brand" strokeWidth={1.5} />
                        </div>
                        <h2 className="text-[16px] font-semibold text-text-base mb-1">Where is this project?</h2>
                        <p className="text-[13px] text-text-muted leading-relaxed">
                          Choose an existing project directory — Automatic will scan it and detect your agents automatically.
                        </p>
                      </div>

                      <div className="space-y-3">
                        <div className="flex gap-2">
                          <input
                            type="text"
                            value={project.directory}
                            onChange={(e) => updateField("directory", e.target.value)}
                            placeholder="/path/to/your/project"
                            className="flex-1 bg-bg-input border border-border-strong/40 hover:border-border-strong focus:border-brand rounded-md px-3 py-2 text-[13px] text-text-base placeholder-text-muted/40 outline-none font-mono transition-colors"
                          />
                          <button
                            onClick={async () => {
                              let selected: string | null = null;
                              try {
                                selected = await invoke<string | null>("open_directory_dialog");
                              } catch (err) {
                                console.error("open_directory_dialog failed:", err);
                              }
                              if (!selected) return;
                              const folderName = selected.split(/[\\/]/).filter(Boolean).pop() ?? "";
                              const name = newName.trim() || folderName;
                              setNewName(name);
                              updateField("directory", selected);
                            }}
                            className="px-4 py-2 bg-brand hover:bg-brand-hover text-white text-[13px] font-medium rounded shadow-sm transition-colors whitespace-nowrap"
                          >
                            Browse
                          </button>
                        </div>

                        {project.directory && (
                          <button
                            disabled={wizardDiscovering}
                            onClick={async () => {
                              const dir = project.directory.trim();
                              if (!dir) return;
                              const folderName = dir.split("/").filter(Boolean).pop() ?? "";
                              const name = newName.trim() || folderName;
                              setNewName(name);
                              setWizardDiscovering(true);
                              setError(null);
                              try {
                                // Save minimal stub so autodetect can read it back
                                const stub = { ...emptyProject(name), directory: dir, name };
                                if (userId && !stub.created_by) stub.created_by = userId;
                                await invoke("save_project", { name, data: JSON.stringify(stub, null, 2) });
                                // Track stub name so cancelCreate can clean it up if the user navigates away
                                wizardStubName.current = name;
                                // Run read-only autodetection
                                const raw: string = await invoke("autodetect_project_dependencies", { name });
                                const detected = JSON.parse(raw) as Project;
                                // Merge: start from current project state (which holds any
                                // template-applied skills/MCP/agents), then add autodetected
                                // items on top. Use emptyProject only for structural defaults.
                                const currentProject = project ?? emptyProject(name);
                                const mergedAgents = [
                                  ...new Set([...currentProject.agents, ...detected.agents]),
                                ];
                                const mergedSkills = [
                                  ...new Set([...currentProject.skills, ...detected.skills]),
                                ];
                                const mergedMcp = [
                                  ...new Set([...currentProject.mcp_servers, ...detected.mcp_servers]),
                                ];
                                const detectedCustomSkills: CustomSkill[] = detected.custom_skills ?? [];
                                const existingCustomNames = new Set(
                                  (currentProject.custom_skills ?? []).map((s) => s.name)
                                );
                                const mergedCustomSkills = [
                                  ...(currentProject.custom_skills ?? []),
                                  ...detectedCustomSkills.filter((s) => !existingCustomNames.has(s.name)),
                                ];
                                setProject({
                                  ...currentProject,
                                  name,
                                  directory: dir,
                                  agents: mergedAgents,
                                  skills: mergedSkills,
                                  custom_skills: mergedCustomSkills,
                                  mcp_servers: mergedMcp,
                                });
                                setWizardDiscoveredAgents(detected.agents);
                                setWizardStep(2);
                              } catch (err: any) {
                                setError(`Autodetect failed: ${err}`);
                              } finally {
                                setWizardDiscovering(false);
                              }
                            }}
                            className="w-full flex items-center justify-center gap-2 px-4 py-2.5 bg-brand hover:bg-brand-hover disabled:opacity-50 disabled:cursor-not-allowed text-white text-[13px] font-medium rounded shadow-sm transition-colors"
                          >
                            {wizardDiscovering ? (
                              <><RefreshCw size={13} className="animate-spin" /> Scanning…</>
                            ) : (
                              <><ArrowRight size={13} /> Continue</>
                            )}
                          </button>
                        )}
                      </div>
                    </>
                  )}

                  {/* ── Step 2: Agents ────────────────────────────────── */}
                  {wizardStep === 2 && (
                    <>
                      <div className="mb-6 text-center">
                        <div className="w-14 h-14 mx-auto mb-4 rounded-full bg-brand/10 border border-brand/30 flex items-center justify-center">
                          <Bot size={24} className="text-brand" strokeWidth={1.5} />
                        </div>
                        <h2 className="text-[16px] font-semibold text-text-base mb-1">Which agents are you using?</h2>
                        <p className="text-[13px] text-text-muted leading-relaxed">
                          {wizardDiscoveredAgents.length > 0
                            ? `We detected ${wizardDiscoveredAgents.length} agent${wizardDiscoveredAgents.length !== 1 ? "s" : ""} in this directory. Other agents below may also come from your defaults or selected templates.`
                            : "No agents were detected. Add the ones you use."}
                        </p>
                      </div>

                      {/* Agent toggle list */}
                      <div className="space-y-2 mb-4 max-h-56 overflow-y-auto custom-scrollbar">
                        {project.agents.map((id, idx) => {
                          const info = availableAgents.find((a) => a.id === id);
                          const isDiscovered = wizardDiscoveredAgents.includes(id);
                          const isDefault = wizardDefaultAgents.includes(id);
                          const templateNames = availableProjectTemplates
                            .filter((tmpl) => selectedProjectTemplates.includes(tmpl.name) && tmpl.agents.includes(id))
                            .map((tmpl) => tmpl.name);
                          return (
                            <div
                              key={id}
                              className="flex items-center gap-3 px-3 py-2.5 bg-bg-input border border-border-strong/40 rounded-lg"
                            >
                              <AgentIcon agentId={id} size={18} />
                              <div className="flex-1 min-w-0">
                                <div className="text-[13px] font-medium text-text-base">{info?.label ?? id}</div>
                                {(isDiscovered || isDefault || templateNames.length > 0) && (
                                  <div className="flex flex-wrap items-center gap-1.5 mt-1">
                                    {isDiscovered && (
                                      <span className="inline-flex items-center px-1.5 py-0.5 rounded-full bg-brand/10 border border-brand/20 text-[10px] font-medium text-brand">
                                        Detected
                                      </span>
                                    )}
                                    {isDefault && (
                                      <span className="inline-flex items-center px-1.5 py-0.5 rounded-full bg-bg-sidebar border border-border-strong/40 text-[10px] font-medium text-text-muted">
                                        Default
                                      </span>
                                    )}
                                    {templateNames.length > 0 && (
                                      <span className="inline-flex items-center px-1.5 py-0.5 rounded-full bg-bg-sidebar border border-border-strong/40 text-[10px] font-medium text-text-base">
                                        {templateNames.length === 1
                                          ? `Template: ${templateNames[0]}`
                                          : `Templates: ${templateNames.join(", ")}`}
                                      </span>
                                    )}
                                  </div>
                                )}
                              </div>
                              <button
                                onClick={() => removeItem("agents", idx)}
                                className="p-1 text-text-muted hover:text-danger hover:bg-surface rounded transition-colors"
                                title="Remove"
                              >
                                <X size={12} />
                              </button>
                            </div>
                          );
                        })}
                        {project.agents.length === 0 && (
                          <p className="text-[12px] text-text-muted italic px-1">No agents selected.</p>
                        )}
                      </div>

                      {/* Add more agents inline */}
                      {(() => {
                        const unaddedAgents = availableAgents.filter((a) => !project.agents.includes(a.id));
                        return unaddedAgents.length > 0 ? (
                          <div className="mt-1">
                            <div className="text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-2">Add agent</div>
                            <div className="space-y-1 max-h-36 overflow-y-auto custom-scrollbar">
                              {unaddedAgents.map((a) => (
                                <button
                                  key={a.id}
                                  onClick={() => addItem("agents", a.id)}
                                  className="w-full flex items-center gap-2.5 px-3 py-2 bg-bg-input hover:bg-bg-sidebar border border-border-strong/40 hover:border-border-strong rounded-md text-left transition-colors"
                                >
                                  <AgentIcon agentId={a.id} size={14} />
                                  <div className="flex-1 min-w-0">
                                    <span className="text-[13px] text-text-base font-medium">{a.label}</span>
                                    {a.description && (
                                      <span className="text-[11px] text-text-muted ml-2">{a.description}</span>
                                    )}
                                  </div>
                                  <Plus size={11} className="text-brand flex-shrink-0" />
                                </button>
                              ))}
                            </div>
                          </div>
                        ) : null;
                      })()}

                      <div className="flex gap-2 mt-6">
                        <button
                          onClick={() => setWizardStep(1)}
                          className="flex-1 px-4 py-2.5 bg-bg-sidebar hover:bg-surface text-text-muted hover:text-text-base text-[13px] font-medium rounded transition-colors"
                        >
                          Back
                        </button>
                        <button
                          onClick={() => setWizardStep(3)}
                          className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 bg-brand hover:bg-brand-hover text-white text-[13px] font-medium rounded shadow-sm transition-colors"
                        >
                          <ArrowRight size={13} /> Continue
                        </button>
                      </div>
                    </>
                  )}

                  {/* ── Step 3: Templates ─────────────────────────────── */}
                  {wizardStep === 3 && (
                    <>
                      <div className="mb-6 text-center">
                        <div className="w-14 h-14 mx-auto mb-4 rounded-full bg-brand/10 border border-brand/30 flex items-center justify-center">
                          <LayoutTemplate size={24} className="text-brand" strokeWidth={1.5} />
                        </div>
                        <h2 className="text-[16px] font-semibold text-text-base mb-1">Apply templates</h2>
                        <p className="text-[13px] text-text-muted leading-relaxed">
                          Optionally select one or more templates to pre-configure skills, MCP servers, and instructions.
                        </p>
                      </div>

                      {availableProjectTemplates.length > 0 ? (
                        <div className="space-y-1 max-h-56 overflow-y-auto custom-scrollbar mb-3">
                          {availableProjectTemplates.map((tmpl) => {
                            const isSelected = selectedProjectTemplates.includes(tmpl.name);
                            return (
                              <button
                                key={tmpl.name}
                                onClick={() => toggleProjectTemplateSelection(tmpl.name)}
                                className={`w-full text-left px-3 py-2.5 rounded-md transition-colors flex items-start gap-2 border ${
                                  isSelected
                                    ? "bg-brand/15 border-brand/40"
                                    : "bg-bg-input border-border-strong/40 hover:border-border-strong hover:bg-bg-sidebar"
                                }`}
                              >
                                <div className="flex-1 min-w-0">
                                  <div className="text-[13px] font-medium text-text-base">{tmpl.name}</div>
                                  {tmpl.description && (
                                    <div className="text-[11px] text-text-muted mt-0.5 truncate">{tmpl.description}</div>
                                  )}
                                  <div className="flex items-center gap-3 mt-1">
                                    {tmpl.agents.length > 0 && (
                                      <span className="text-[10px] text-text-muted flex items-center gap-1">
                                        <Bot size={10} /> {tmpl.agents.length}
                                      </span>
                                    )}
                                    {tmpl.skills.length > 0 && (
                                      <span className="text-[10px] text-text-muted flex items-center gap-1">
                                        <Code size={10} /> {tmpl.skills.length}
                                      </span>
                                    )}
                                    {tmpl.mcp_servers.length > 0 && (
                                      <span className="text-[10px] text-text-muted flex items-center gap-1">
                                        <Server size={10} /> {tmpl.mcp_servers.length}
                                      </span>
                                    )}
                                  </div>
                                </div>
                                {isSelected && (
                                  <Check size={13} className="text-brand flex-shrink-0 mt-0.5" />
                                )}
                              </button>
                            );
                          })}
                        </div>
                      ) : (
                        <div className="mb-5 px-3 py-4 bg-bg-input border border-border-strong/40 rounded-md text-center">
                          <p className="text-[12px] text-text-muted italic">No project templates configured.</p>
                        </div>
                      )}

                      {selectedProjectTemplates.length > 0 && (
                        <button
                          onClick={() => setSelectedProjectTemplates([])}
                          className="flex items-center gap-1.5 text-[12px] text-text-muted hover:text-text-base mb-3 transition-colors"
                        >
                          <X size={11} /> Clear selection ({selectedProjectTemplates.length})
                        </button>
                      )}

                      <div className="flex gap-2">
                        <button
                          onClick={() => setWizardStep(2)}
                          className="flex-1 px-4 py-2.5 bg-bg-sidebar hover:bg-surface text-text-muted hover:text-text-base text-[13px] font-medium rounded transition-colors"
                        >
                          Back
                        </button>
                        <button
                          onClick={handleSave}
                          className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 bg-brand hover:bg-brand-hover text-white text-[13px] font-medium rounded shadow-sm transition-colors"
                        >
                          <Check size={13} /> Create Project
                        </button>
                      </div>
                    </>
                  )}

                </div>
              </div>
            )}

            {/* Tab bar + content (hidden while in new-project setup) */}
            {!isCreating && <>
            {/* When a secondary (controls bar) view is active, replace the primary tab strip
                with a thin bar containing only the close button. */}
            {activeToolName === null && isSecondaryGroup(projectGroup) && (
              <div className="flex items-center justify-end px-6 py-1 border-b border-border-strong/40 bg-bg-input/20 flex-shrink-0">
                <button
                  type="button"
                  onClick={closeSecondaryView}
                  aria-label="Close and return to previous view"
                  title="Close"
                  className="p-1 text-text-muted hover:text-text-base transition-colors"
                >
                  <X size={14} />
                </button>
              </div>
            )}
            {/* Primary group tabs — hidden when a secondary (controls bar) view is active. */}
            {!(activeToolName === null && isSecondaryGroup(projectGroup)) && (
            <div className="flex items-center gap-0 px-6 border-b border-border-strong/40 flex-shrink-0">
              {PROJECT_GROUPS.map((group) => (
                <button
                  key={group.id}
                  onClick={() => selectGroup(group.id)}
                  className={`px-3 py-2.5 text-[13px] font-medium transition-colors relative flex items-center gap-1.5 ${
                    activeToolName === null && projectGroup === group.id
                      ? "text-text-base"
                      : "text-text-muted hover:text-text-base"
                  }`}
                >
                  {group.label}
                  {activeToolName === null && projectGroup === group.id && (
                    <span className="absolute bottom-0 left-0 right-0 h-[2px] bg-brand rounded-t" />
                  )}
                </button>
              ))}
              {/* Enabled tools that declare provides_tab get a top-level tab. */}
              {toolEntries.filter((e) =>
                e.provides_tab && (project?.tools ?? []).includes(e.name)
              ).map((entry) => (
                <button
                  key={entry.name}
                  onClick={() => selectTopLevelTool(entry.name)}
                  className={`px-3 py-2.5 text-[13px] font-medium transition-colors relative flex items-center gap-1.5 ${
                    activeToolName === entry.name
                      ? "text-text-base"
                      : "text-text-muted hover:text-text-base"
                  }`}
                >
                  {entry.display_name}
                  {activeToolName === entry.name && (
                    <span className="absolute bottom-0 left-0 right-0 h-[2px] bg-brand rounded-t" />
                  )}
                </button>
              ))}

            </div>
            )}
            {/* Secondary sub-tabs (only shown when a static group with sub-tabs is active) */}
            {activeToolName === null && projectGroup !== "summary" && (() => {
              const activeGroup =
                PROJECT_GROUPS.find((g) => g.id === projectGroup) ??
                PROJECT_CONTROLS.find((g) => g.id === projectGroup);
              if (!activeGroup || activeGroup.tabs.length <= 1) return null;
              return (
                <div className="flex items-center gap-0 px-6 border-b border-border-strong/20 bg-bg-input/30 flex-shrink-0">
                  {activeGroup.tabs.map((tab) => (
                    <button
                      key={tab.id}
                      onClick={() => selectTab(tab.id)}
                      className={`px-3 py-2 text-[12px] font-medium transition-colors relative flex items-center gap-1.5 ${
                        projectTab === tab.id
                          ? "text-text-base"
                          : "text-text-muted hover:text-text-base"
                      }`}
                    >
                      {tab.label}
                      {projectTab === tab.id && (
                        <span className="absolute bottom-0 left-0 right-0 h-[2px] bg-brand/60 rounded-t" />
                      )}
                    </button>
                  ))}
                </div>
              );
            })()}

            {/* Tab content */}

            {/* ── Project File tab (full-bleed layout) ──────────── */}
            {projectTab === "project_file" && (
              <>
                {project.directory && project.agents.length > 0 ? (
                  <div className="flex-1 flex flex-col min-h-0">
                    {/* Mode toggle bar */}
                    <div className="flex items-center gap-3 px-4 py-2.5 border-b border-border-strong/40 bg-bg-input/30 flex-shrink-0">
                      <span className="text-[11px] text-text-muted">Mode:</span>
                      <div className="flex rounded overflow-hidden border border-border-strong/40">
                        <button
                          onClick={async () => {
                            if (project.instruction_mode === "unified" || !selectedName) {
                              return;
                            }
                            let inspection: UnifiedInspection;
                            try {
                              const raw = await invoke<string>("inspect_unified_candidates", {
                                name: selectedName,
                              });
                              inspection = JSON.parse(raw) as UnifiedInspection;
                            } catch (e) {
                              console.error("inspect_unified_candidates failed", e);
                              return;
                            }
                            // No instruction-capable agents (or no candidate files at all):
                            // nothing to overwrite, switch silently.
                            if (inspection.candidates.length === 0) {
                              const updated = { ...project, instruction_mode: "unified", updated_at: new Date().toISOString() };
                              setProject(updated);
                              setDirty(false);
                              await invoke("save_project", { name: selectedName, data: JSON.stringify(updated, null, 2) });
                              await loadProjectFiles(selectedName);
                              notifyProjectUpdated();
                              return;
                            }
                            // Always show the picker + confirmation flow so the user
                            // sees what content becomes the source and which files
                            // are about to be overwritten.
                            setUnifiedSourcePicker(inspection.candidates);
                          }}
                          className={`flex items-center gap-1.5 px-2.5 py-1 text-[11px] font-medium transition-colors ${
                            (project.instruction_mode || "per-agent") === "unified"
                              ? "bg-brand text-white"
                              : "bg-bg-sidebar text-text-muted hover:text-text-base"
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
                              notifyProjectUpdated();
                            }
                          }}
                          className={`flex items-center gap-1.5 px-2.5 py-1 text-[11px] font-medium transition-colors ${
                            (project.instruction_mode || "per-agent") === "per-agent"
                              ? "bg-brand text-white"
                              : "bg-bg-sidebar text-text-muted hover:text-text-base"
                          }`}
                        >
                          <SplitSquareHorizontal size={11} />
                          Per Agent
                        </button>
                      </div>
                      {(project.instruction_mode || "per-agent") === "unified" && projectFiles.length > 0 && projectFiles[0].target_files && (
                        <span className="text-[10px] text-text-muted">
                          Writes to: {projectFiles[0].target_files.join(", ")}
                        </span>
                      )}
                    </div>

                    <div className="flex-1 flex min-h-0">
                    {/* File sidebar — hidden in unified mode */}
                    {(project.instruction_mode || "per-agent") === "per-agent" && projectFiles.length > 0 && (
                      <div className="w-52 flex-shrink-0 border-r border-border-strong/40 bg-bg-input/50 flex flex-col">
                        <div className="h-9 px-3 border-b border-border-strong/40 flex items-center justify-between">
                          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Files</span>
                          <button
                            onClick={() => setShowTemplatePicker(!showTemplatePicker)}
                            className="text-text-muted hover:text-text-base p-0.5 hover:bg-bg-sidebar rounded transition-colors"
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
                                    if (projectFileDirty && !(await ask("Discard unsaved changes?", { title: "Unsaved Changes", kind: "warning" }))) return;
                                    setActiveProjectFile(f.filename);
                                    if (selectedName) await loadProjectFileContent(selectedName, f.filename);
                                  }}
                                  className={`w-full text-left px-2.5 py-1.5 rounded-md text-[13px] font-medium transition-colors flex items-center gap-2 ${
                                    activeProjectFile === f.filename
                                      ? "bg-bg-sidebar text-text-base"
                                      : "text-text-muted hover:bg-bg-sidebar/50 hover:text-text-base"
                                  }`}
                                >
                                  <FileText size={13} className={activeProjectFile === f.filename ? "text-text-base" : f.exists ? "text-text-muted" : "text-text-muted"} />
                                  <div className="min-w-0">
                                    <div className={`truncate ${!f.exists ? "opacity-50" : ""}`}>{f.filename}</div>
                                    <div className="text-[10px] text-text-muted truncate">{f.agents.join(", ")}</div>
                                  </div>
                                </button>
                              </li>
                            ))}
                          </ul>
                        </div>
                        {/* Template picker (dropdown in sidebar) */}
                        {showTemplatePicker && availableTemplates.length > 0 && (
                          <div className="border-t border-border-strong/40 p-2">
                            <p className="text-[10px] text-text-muted mb-1.5">Apply template:</p>
                            <div className="space-y-0.5">
                              {availableTemplates.map((t) => (
                                <button
                                  key={t}
                                  onClick={() => handleApplyTemplate(t)}
                                  className="w-full text-left px-2 py-1 text-[12px] bg-bg-sidebar hover:bg-brand text-text-base hover:text-white rounded transition-colors flex items-center gap-1.5"
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
                            <div className="w-12 h-12 mx-auto mb-4 rounded-full border border-dashed border-border-strong flex items-center justify-center text-text-muted">
                              <FileText size={20} strokeWidth={1.5} />
                            </div>
                            <h3 className="text-[14px] font-medium text-text-base mb-1">
                              {activeProjectFile === "_unified" ? "Shared File" : activeProjectFile}
                            </h3>
                            <p className="text-[13px] text-text-muted mb-5 max-w-xs">
                              This file doesn't exist yet. Create it to provide project instructions for {activeFile?.agents.join(" & ")}.
                            </p>
                             <div className="flex items-center gap-2">
                               {/* Primary action: Generate with AI */}
                                 <span className="relative group/keytip">
                                   <button
                                     onClick={handleGenerateInstruction}
                                     disabled={projectFileGenerating || !agentFeaturesEnabled}
                                     className="px-3 py-1.5 bg-brand hover:bg-brand-hover text-white text-[12px] font-medium rounded shadow-sm transition-colors flex items-center gap-1.5 disabled:opacity-50 disabled:cursor-not-allowed"
                                   >
                                     <Sparkles size={12} className={projectFileGenerating ? "animate-pulse" : ""} />
                                     {projectFileGenerating ? "Generating…" : "Generate with AI"}
                                   </button>
                                   {!agentFeaturesEnabled && (
                                     <span className="pointer-events-none absolute bottom-full left-1/2 -translate-x-1/2 mb-1.5 whitespace-nowrap rounded bg-bg-input-dark border border-border-strong/40 px-2 py-1 text-[11px] text-text-base shadow-md opacity-0 group-hover/keytip:opacity-100 transition-opacity z-10">
                                       Enable Agent features to access
                                     </span>
                                   )}
                                 </span>
                               {/* Secondary: blank file */}
                               <button
                                  onClick={() => {
                                    setProjectFileContent("");
                                    setProjectFileEditing(true);
                                    setProjectFileDirty(true);
                                  }}
                                  className="px-3 py-1.5 bg-bg-sidebar hover:bg-surface text-text-base text-[12px] font-medium rounded border border-border-strong/40 transition-colors flex items-center gap-1.5"
                                >
                                  <Plus size={12} /> Create File
                                </button>
                               {/* Secondary: from template */}
                               {availableTemplates.length > 0 && (
                                 <button
                                   onClick={() => setShowTemplatePicker(!showTemplatePicker)}
                                   className="px-3 py-1.5 bg-bg-sidebar hover:bg-surface text-text-base text-[12px] font-medium rounded border border-border-strong/40 transition-colors flex items-center gap-1.5"
                                 >
                                   <LayoutTemplate size={12} /> From Template
                                 </button>
                               )}
                             </div>
                            {showTemplatePicker && availableTemplates.length > 0 && (
                              <div className="mt-3 p-2 bg-bg-input rounded-md border border-border-strong/40">
                                <div className="flex flex-wrap gap-1.5">
                                  {availableTemplates.map((t) => (
                                    <button
                                      key={t}
                                      onClick={() => handleApplyTemplate(t)}
                                      className="px-2 py-1 text-[12px] bg-bg-sidebar hover:bg-brand text-text-base hover:text-white rounded transition-colors flex items-center gap-1.5"
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

                      return (
                        <div className="flex-1 flex min-w-0 min-h-0">
                          {/* Editor column */}
                          <div className="flex-1 flex flex-col min-w-0">
                             {/* Editor toolbar */}
                             <div className="flex items-center justify-between px-4 h-9 bg-bg-input border-b border-border-strong/40 flex-shrink-0">
                               <div className="flex items-center gap-2 min-w-0">
                                 <span className="text-[11px] text-text-muted">
                                   {activeProjectFile === "_unified"
                                     ? <>{projectFileEditing ? "Editing" : ""}{projectFileDirty ? " (unsaved)" : ""}</>
                                     : <>{activeProjectFile}{!fileExists ? " (new)" : ""}{projectFileEditing ? " — Editing" : ""}{projectFileDirty ? " (unsaved)" : ""}</>
                                   }
                                 </span>
                                 <TokenPill text={projectFileContent} />
                               </div>
                                <div className="flex items-center gap-1.5">
                                   {/* Update with AI — only when content already exists */}
                                   {(fileExists || projectFileContent.trim().length > 0) && (
                                    <span className="relative group/keytip">
                                      <button
                                        onClick={handleUpdateInstruction}
                                        disabled={projectFileUpdating || projectFileGenerating || projectFileSaving || !agentFeaturesEnabled || !projectFileContent.trim()}
                                        className="flex items-center gap-1 px-2 py-0.5 text-[11px] text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                                      >
                                        <RefreshCw size={10} className={projectFileUpdating ? "animate-spin text-brand" : ""} />
                                        {projectFileUpdating ? "Updating…" : "Update"}
                                      </button>
                                      {!agentFeaturesEnabled && (
                                        <span className="pointer-events-none absolute bottom-full left-1/2 -translate-x-1/2 mb-1.5 whitespace-nowrap rounded bg-bg-input-dark border border-border-strong/40 px-2 py-1 text-[11px] text-text-base shadow-md opacity-0 group-hover/keytip:opacity-100 transition-opacity z-10">
                                          Enable Agent features to access
                                        </span>
                                      )}
                                    </span>
                                   )}
                                   {/* Generate with AI — always visible */}
                                    <span className="relative group/keytip">
                                      <button
                                        onClick={handleGenerateInstruction}
                                        disabled={projectFileGenerating || projectFileSaving || !agentFeaturesEnabled}
                                        className="flex items-center gap-1 px-2 py-0.5 text-[11px] text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                                      >
                                        <Sparkles size={10} className={projectFileGenerating ? "animate-pulse text-brand" : ""} />
                                        {projectFileGenerating ? "Generating…" : "Generate"}
                                      </button>
                                      {!agentFeaturesEnabled && (
                                        <span className="pointer-events-none absolute bottom-full left-1/2 -translate-x-1/2 mb-1.5 whitespace-nowrap rounded bg-bg-input-dark border border-border-strong/40 px-2 py-1 text-[11px] text-text-base shadow-md opacity-0 group-hover/keytip:opacity-100 transition-opacity z-10">
                                          Enable Agent features to access
                                        </span>
                                      )}
                                    </span>
                                  <span className="w-px h-3 bg-border-strong/40" />
                                 {!projectFileEditing ? (
                                    <button
                                      onClick={() => setProjectFileEditing(true)}
                                      className="flex items-center gap-1 px-2 py-0.5 text-[11px] text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
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
                                       className="px-2 py-0.5 text-[11px] text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
                                     >
                                       Cancel
                                     </button>
                                     <button
                                       onClick={handleSaveProjectFile}
                                       disabled={!projectFileDirty || projectFileSaving}
                                       className="flex items-center gap-1 px-2 py-0.5 text-[11px] bg-brand hover:bg-brand-hover text-white rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                                     >
                                       <Check size={10} /> {projectFileSaving ? "Saving..." : "Save"}
                                     </button>
                                   </>
                                 )}
                               </div>
                            </div>

                            {/* Content area */}
                            {projectFileEditing ? (
                              <LineNumberedTextarea
                                value={projectFileContent}
                                onChange={(v) => {
                                  setProjectFileContent(v);
                                  setProjectFileDirty(true);
                                }}
                                className="flex-1 min-h-0"
                                placeholder="Write your project instructions here..."
                              />
                            ) : (
                              <div className="flex-1 overflow-y-auto custom-scrollbar bg-bg-base min-h-0">
                                {projectFileContent
                                  ? <MarkdownPreview content={projectFileContent} />
                                  : <span className="block p-4 text-[13px] text-text-muted italic">Empty file.</span>
                                }
                              </div>
                            )}
                          </div>


                        </div>
                      );
                    })() : (
                      <div className="flex-1 flex items-center justify-center">
                        <p className="text-[13px] text-text-muted italic">No project files configured. Add agent tools on the Agents tab first.</p>
                      </div>
                    )}
                  </div>
                  </div>
                ) : (
                  <div className="flex-1 flex items-center justify-center">
                    <p className="text-[13px] text-text-muted italic">
                      Set a project directory and add agent tools on the Details and Agents tabs to manage project files.
                    </p>
                  </div>
                )}
              </>
            )}

            {/* ── Context tab (full-bleed, like project_file) ──────────── */}
            {projectTab === "context" && (
              <div className="flex-1 flex flex-col min-h-0">
                {!project?.directory ? (
                  <div className="flex-1 flex items-center justify-center">
                    <p className="text-[13px] text-text-muted italic">
                      Set a project directory to use context.
                    </p>
                  </div>
                ) : loadingContext ? (
                  <div className="flex-1 flex items-center justify-center text-text-muted">
                    <RefreshCw size={14} className="animate-spin mr-2" />
                    <span className="text-[13px]">Loading…</span>
                  </div>
                ) : !contextFileExists && !contextEditing ? (
                  /* ── Create prompt ── */
                  <div className="flex-1 flex flex-col items-center justify-center text-center p-8">
                    <div className="w-12 h-12 mx-auto mb-4 rounded-full border border-dashed border-border-strong flex items-center justify-center text-text-muted">
                      <Brain size={20} strokeWidth={1.5} />
                    </div>
                    <h3 className="text-[14px] font-medium text-text-base mb-1">No context file</h3>
                    <p className="text-[13px] text-text-muted mb-1 max-w-xs">
                      Create <code className="font-mono text-[12px]">.automatic/context.json</code> to give agents structured knowledge about this project.
                    </p>
                    <p className="text-[12px] text-text-muted mb-5 max-w-sm">
                      Define commands, entry points, architecture concepts, conventions, and gotchas.
                    </p>
                    <div className="flex items-center gap-2">
                      <span className="relative group/keytip">
                        <button
                          onClick={handleGenerateContext}
                          disabled={contextGenerating || !agentFeaturesEnabled}
                          className="px-3 py-1.5 bg-brand hover:bg-brand-hover text-white text-[12px] font-medium rounded shadow-sm transition-colors flex items-center gap-1.5 disabled:opacity-50 disabled:cursor-not-allowed"
                        >
                          <Sparkles size={12} className={contextGenerating ? "animate-pulse" : ""} />
                          {contextGenerating ? "Generating…" : "Generate with AI"}
                        </button>
                        {!agentFeaturesEnabled && (
                          <span className="pointer-events-none absolute bottom-full left-1/2 -translate-x-1/2 mb-1.5 whitespace-nowrap rounded bg-bg-input-dark border border-border-strong/40 px-2 py-1 text-[11px] text-text-base shadow-md opacity-0 group-hover/keytip:opacity-100 transition-opacity z-10">
                            Enable Agent features to access
                          </span>
                        )}
                      </span>
                      <button
                        onClick={() => {
                          const template = JSON.stringify({
                            commands: { build: "npm run build", test: "npm test" },
                            entry_points: { app: "src/main.ts" },
                            concepts: { example: { summary: "Describe a key concept here", files: [] } },
                            conventions: { naming: "Describe a naming convention" },
                            gotchas: {},
                          }, null, 2);
                          setContextRaw(template);
                          setContextEditing(true);
                          setContextDirty(true);
                          setContextJsonError(null);
                        }}
                        className="px-3 py-1.5 bg-bg-input hover:bg-surface-hover border border-border-strong/50 text-text-muted hover:text-text-base text-[12px] font-medium rounded shadow-sm transition-colors flex items-center gap-1.5"
                      >
                        <Plus size={12} /> Create manually
                      </button>
                    </div>
                    {contextJsonError && (
                      <div className="flex items-start gap-2 mt-4 px-4 py-2 bg-error/10 border border-error/30 rounded-lg max-w-sm">
                        <AlertCircle size={12} className="text-error mt-0.5 flex-shrink-0" />
                        <span className="text-[11px] text-error font-mono">{contextJsonError}</span>
                      </div>
                    )}
                  </div>
                ) : (
                  /* ── Editor area ── */
                  <div className="flex-1 flex flex-col min-h-0">
                    {/* Toolbar */}
                    <div className="flex items-center justify-between px-4 h-9 bg-bg-input border-b border-border-strong/40 flex-shrink-0">
                      <div className="flex items-center gap-2 min-w-0">
                        <span className="text-[11px] text-text-muted font-mono">
                          .automatic/context.json
                          {!contextFileExists ? " (new)" : ""}
                          {contextEditing ? " — Editing" : ""}
                          {contextDirty ? " (unsaved)" : ""}
                        </span>
                        <TokenPill text={contextRaw} />
                      </div>
                      <div className="flex items-center gap-1.5">
                        {/* Generate button — always visible in the toolbar */}
                        <span className="relative group/keytip">
                          <button
                            onClick={handleGenerateContext}
                            disabled={contextGenerating || contextSaving || !agentFeaturesEnabled}
                            className="flex items-center gap-1 px-2 py-0.5 text-[11px] text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                          >
                            <Sparkles size={10} className={contextGenerating ? "animate-pulse text-brand" : ""} />
                            {contextGenerating ? "Generating…" : "Generate"}
                          </button>
                          {!agentFeaturesEnabled && (
                            <span className="pointer-events-none absolute bottom-full left-1/2 -translate-x-1/2 mb-1.5 whitespace-nowrap rounded bg-bg-input-dark border border-border-strong/40 px-2 py-1 text-[11px] text-text-base shadow-md opacity-0 group-hover/keytip:opacity-100 transition-opacity z-10">
                              Enable Agent features to access
                            </span>
                          )}
                        </span>
                        <div className="w-px h-3 bg-border-strong/40" />
                        {!contextEditing ? (
                          <button
                            onClick={() => setContextEditing(true)}
                            className="flex items-center gap-1 px-2 py-0.5 text-[11px] text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
                          >
                            <Edit2 size={10} /> Edit
                          </button>
                        ) : (
                          <>
                            <button
                              onClick={() => {
                                setContextEditing(false);
                                setContextJsonError(null);
                                if (contextDirty && selectedName) {
                                  if (contextFileExists) {
                                    loadContext(selectedName);
                                  } else {
                                    setContextRaw("");
                                    setContextDirty(false);
                                  }
                                }
                              }}
                              className="px-2 py-0.5 text-[11px] text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
                            >
                              Cancel
                            </button>
                            <button
                              onClick={handleSaveContext}
                              disabled={!contextDirty || contextSaving}
                              className="flex items-center gap-1 px-2 py-0.5 text-[11px] bg-brand hover:bg-brand-hover text-white rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                            >
                              <Check size={10} /> {contextSaving ? "Saving…" : "Save"}
                            </button>
                          </>
                        )}
                      </div>
                    </div>

                    {/* JSON error banner */}
                    {contextJsonError && (
                      <div className="flex items-start gap-2 px-4 py-2 bg-error/10 border-b border-error/30 flex-shrink-0">
                        <AlertCircle size={12} className="text-error mt-0.5 flex-shrink-0" />
                        <span className="text-[11px] text-error font-mono">{contextJsonError}</span>
                      </div>
                    )}

                    {/* Content: raw JSON editor or structured read-only view */}
                    {contextEditing ? (
                      <LineNumberedTextarea
                        value={contextRaw}
                        onChange={(v) => {
                          setContextRaw(v);
                          setContextDirty(true);
                          setContextJsonError(null);
                        }}
                        className="flex-1 min-h-0"
                        placeholder={`{\n  "commands": {},\n  "concepts": {},\n  "conventions": {},\n  "gotchas": {}\n}`}
                      />
                    ) : (
                      <div className="flex-1 overflow-y-auto custom-scrollbar p-6 space-y-5">
                        {(() => {
                          const ctx = projectContext;
                          if (!ctx) return <span className="text-[13px] text-text-muted italic">Empty file.</span>;
                          // eslint-disable-next-line @typescript-eslint/no-explicit-any
                          const sections: any[] = [];

                          if (Object.keys(ctx.commands).length > 0)
                            sections.push(
                              <div key="commands">
                                <div className="flex items-center gap-2 mb-2">
                                  <Code size={12} className="text-text-muted" />
                                  <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Commands</span>
                                </div>
                                <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
                                  {Object.entries(ctx.commands).map(([name, cmd]) => (
                                    <div key={name} className="flex items-start gap-3 px-4 py-2.5">
                                      <span className="text-[12px] font-medium text-text-base w-32 flex-shrink-0 pt-px">{name}</span>
                                      <code className="text-[11px] font-mono text-text-muted break-all">{cmd}</code>
                                    </div>
                                  ))}
                                </div>
                              </div>
                            );

                          if (Object.keys(ctx.entry_points).length > 0)
                            sections.push(
                              <div key="entry_points">
                                <div className="flex items-center gap-2 mb-2">
                                  <ArrowRight size={12} className="text-text-muted" />
                                  <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Entry Points</span>
                                </div>
                                <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
                                  {Object.entries(ctx.entry_points).map(([name, path]) => (
                                    <div key={name} className="flex items-start gap-3 px-4 py-2.5">
                                      <span className="text-[12px] font-medium text-text-base w-32 flex-shrink-0 pt-px">{name}</span>
                                      <code className="text-[11px] font-mono text-text-muted break-all">{path}</code>
                                    </div>
                                  ))}
                                </div>
                              </div>
                            );

                          if (Object.keys(ctx.concepts).length > 0)
                            sections.push(
                              <div key="concepts">
                                <div className="flex items-center gap-2 mb-2">
                                  <Brain size={12} className="text-text-muted" />
                                  <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Architecture Concepts</span>
                                </div>
                                <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
                                  {Object.entries(ctx.concepts).map(([name, concept]) => (
                                    <div key={name} className="px-4 py-3 space-y-1.5">
                                      <span className="text-[12px] font-semibold text-text-base block">{name}</span>
                                      <p className="text-[12px] text-text-muted leading-relaxed">{concept.summary}</p>
                                      {concept.files.length > 0 && (
                                        <div className="flex flex-wrap gap-1.5">
                                          {concept.files.map((f) => (
                                            <code key={f} className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-bg-sidebar border border-border-strong/30 text-text-muted">{f}</code>
                                          ))}
                                        </div>
                                      )}
                                    </div>
                                  ))}
                                </div>
                              </div>
                            );

                          if (Object.keys(ctx.conventions).length > 0)
                            sections.push(
                              <div key="conventions">
                                <div className="flex items-center gap-2 mb-2">
                                  <ScrollText size={12} className="text-text-muted" />
                                  <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Conventions</span>
                                </div>
                                <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
                                  {Object.entries(ctx.conventions).map(([name, desc]) => (
                                    <div key={name} className="px-4 py-2.5 space-y-0.5">
                                      <span className="text-[12px] font-medium text-text-base block">{name}</span>
                                      <p className="text-[12px] text-text-muted leading-relaxed">{desc}</p>
                                    </div>
                                  ))}
                                </div>
                              </div>
                            );

                          if (Object.keys(ctx.gotchas).length > 0)
                            sections.push(
                              <div key="gotchas">
                                <div className="flex items-center gap-2 mb-2">
                                  <AlertCircle size={12} className="text-text-muted" />
                                  <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Gotchas</span>
                                </div>
                                <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
                                  {Object.entries(ctx.gotchas).map(([name, desc]) => (
                                    <div key={name} className="px-4 py-2.5 space-y-0.5">
                                      <span className="text-[12px] font-medium text-text-base block">{name}</span>
                                      <p className="text-[12px] text-text-muted leading-relaxed">{desc}</p>
                                    </div>
                                  ))}
                                </div>
                              </div>
                            );

                          return sections.length > 0
                            ? <>{sections}</>
                            : <span className="text-[13px] text-text-muted italic">Empty context file. Click Edit to add content.</span>;
                        })()}
                      </div>
                    )}
                  </div>
                )}
              </div>
            )}

            {/* ── Top-level tool tab panels ──────────────────────── */}
            {activeToolName === "build" && selectedName && (
              <div className="flex-1 overflow-hidden">
                <Features projectName={selectedName} />
              </div>
            )}
            {activeToolName === "spec-kitty" && project.directory && (
              <div className="flex-1 overflow-hidden">
                <SpecKittyPanel
                  projectDir={project.directory}
                  sidebar={(() => {
                    const entry = toolEntries.find((e) => e.name === "spec-kitty");
                    if (!entry) return null;
                    return (
                      <ToolInfoSidebar
                        entry={entry}
                        active
                        onAdd={() => {}}
                        onRemove={() => {
                          const tools = (project.tools ?? []).filter((t) => t !== "spec-kitty");
                          const updated = { ...project, tools, updated_at: new Date().toISOString() };
                          setProject(updated);
                          setDirty(false);
                          saveProjectSnapshot(updated);
                          setActiveToolName(null);
                        }}
                      />
                    );
                  })()}
                />
              </div>
            )}

            {/* ── Tools tab (under Configuration) ──────────────────── */}
            {activeToolName === null && projectTab === "tools" && (
              <div className="flex-1 overflow-y-auto p-6 custom-scrollbar">
                <div className="space-y-8">
                  {toolTab === null ? (
                    <ProjectToolsTab
                      projectDir={project.directory}
                      projectTools={project.tools ?? []}
                      entries={toolEntries}
                      loading={toolEntriesLoading}
                      onReload={loadToolEntries}
                      onToolsChange={(tools) => {
                        const updated = { ...project, tools, updated_at: new Date().toISOString() };
                        setProject(updated);
                        setDirty(false);
                        saveProjectSnapshot(updated);
                      }}
                    />
                  ) : (
                    (() => {
                      const entry = toolEntries.find((e) => e.name === toolTab);
                      if (!entry) return (
                        <p className="text-[12px] text-text-muted">Tool not found.</p>
                      );
                      return (
                        <ProjectToolDetailPanel
                          entry={entry}
                          projectDir={project.directory}
                          active={(project.tools ?? []).includes(entry.name)}
                          onAdd={() => {
                            const tools = [...new Set([...(project.tools ?? []), entry.name])];
                            const updated = { ...project, tools, updated_at: new Date().toISOString() };
                            setProject(updated);
                            setDirty(false);
                            saveProjectSnapshot(updated);
                          }}
                          onRemove={() => {
                            const tools = (project.tools ?? []).filter((t) => t !== entry.name);
                            const updated = { ...project, tools, updated_at: new Date().toISOString() };
                            setProject(updated);
                            setDirty(false);
                            saveProjectSnapshot(updated);
                          }}
                        />
                      );
                    })()
                  )}
                </div>
              </div>
            )}

            {/* Other tabs (padded container) */}
            {activeToolName === null && projectTab !== "tools" && projectTab !== "project_file" && projectTab !== "context" && (
            <div className="flex-1 overflow-y-auto p-6 custom-scrollbar">
              <div className="space-y-8">

                {/* ── Rules tab ─────────────────────────────────────────── */}
                {projectTab === "rules" && (() => {
                  // The automatic-service rule is mandatory on every project.
                  // It is always enforced by the backend and cannot be removed.
                  const MANDATORY_RULE = "automatic-service";
                  const isRuleLocked = (ruleId: string) =>
                    pluginLockedRules.includes(ruleId) || ruleId === MANDATORY_RULE;

                  const configuredRules = (project.file_rules || {})["_project"] || [];
                  // Ensure the mandatory rule always appears in the displayed list,
                  // even if the user hasn't explicitly added it to file_rules.
                  const projectRules = configuredRules.includes(MANDATORY_RULE)
                    ? configuredRules
                    : [MANDATORY_RULE, ...configuredRules];
                  const customRules: CustomRule[] = project.custom_rules || [];

                  const handleToggleProjectRule = (ruleId: string) => {
                    const existing = (project.file_rules || {})["_project"] || [];
                    // Prevent removal of mandatory and plugin-locked rules.
                    if (existing.includes(ruleId) && isRuleLocked(ruleId)) return;
                    const updated = existing.includes(ruleId)
                      ? existing.filter(r => r !== ruleId)
                      : [...existing, ruleId];
                    const newFileRules: Record<string, string[]> = { ...(project.file_rules || {}), _project: updated };
                    if (updated.length === 0) delete newFileRules["_project"];
                    setProject({ ...project, file_rules: newFileRules });
                    setDirty(true);
                  };

                  const handleAddCustomRule = () => {
                    const newRule: CustomRule = { name: "New Rule", content: "" };
                    setProject({ ...project, custom_rules: [...customRules, newRule] });
                    setCustomRuleEditingIdx(customRules.length);
                    setCustomRuleEditName("New Rule");
                    setCustomRuleEditContent("");
                    setDirty(true);
                  };

                  const handleDeleteCustomRule = (idx: number) => {
                    const updated = customRules.filter((_, i) => i !== idx);
                    setProject({ ...project, custom_rules: updated });
                    if (customRuleEditingIdx === idx) {
                      setCustomRuleEditingIdx(null);
                    } else if (customRuleEditingIdx !== null && customRuleEditingIdx > idx) {
                      setCustomRuleEditingIdx(customRuleEditingIdx - 1);
                    }
                    setDirty(true);
                  };

                  const handleStartEditCustomRule = (idx: number) => {
                    setCustomRuleEditingIdx(idx);
                    setCustomRuleEditName(customRules[idx]?.name ?? "");
                    setCustomRuleEditContent(customRules[idx]?.content ?? "");
                  };

                  const handleCommitCustomRule = () => {
                    if (customRuleEditingIdx === null) return;
                    const updated = customRules.map((r, i) =>
                      i === customRuleEditingIdx
                        ? { name: customRuleEditName.trim() || "Untitled Rule", content: customRuleEditContent }
                        : r
                    );
                    setProject({ ...project, custom_rules: updated });
                    setCustomRuleEditingIdx(null);
                    setDirty(true);
                  };

                  const totalActive = projectRules.length + customRules.filter(r => r.content.trim()).length;

                  return (
                    <div className="flex gap-6">
                    <div className="flex-1 min-w-0 space-y-8">

                      {/* ── Section header ── */}
                      <div className="flex items-center justify-between">
                        <div>
                          <h2 className="text-[15px] font-semibold text-text-base">Rules</h2>
                          <p className="text-[12px] text-text-muted mt-0.5">
                            Rules are injected into all agent instruction files when the project is synced.
                          </p>
                        </div>
                        {totalActive > 0 && (
                          <span className="text-[11px] text-brand bg-brand/10 px-2 py-0.5 rounded border border-brand/20">
                            {totalActive} active
                          </span>
                        )}
                      </div>

                      {/* ── Custom Rules ── */}
                      <section>
                        <div className="flex items-center justify-between mb-3">
                          <div className="flex items-center gap-2">
                            <Edit2 size={13} className="text-text-muted" />
                            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Project Rules</span>
                            {customRules.length > 0 && (
                              <span className="text-[10px] bg-bg-sidebar border border-border-strong/40 rounded-full px-1.5 py-0.5 text-text-muted leading-none">
                                {customRules.length}
                              </span>
                            )}
                          </div>
                          <button
                            onClick={handleAddCustomRule}
                            className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium"
                          >
                            <Plus size={12} /> Add Rule
                          </button>
                        </div>
                        <p className="text-[12px] text-text-muted mb-3">
                          Write rules directly in this project. They are injected alongside any global rules selected below.
                        </p>

                        {customRules.length === 0 ? (
                          <button
                            onClick={handleAddCustomRule}
                            className="w-full flex items-center justify-center gap-2 px-4 py-6 border border-dashed border-border-strong/60 hover:border-brand/40 rounded-lg text-text-muted hover:text-brand transition-colors text-[13px]"
                          >
                            <Plus size={14} /> Write your first project rule
                          </button>
                        ) : (
                          <div className="space-y-2">
                            {customRules.map((rule, idx) => {
                              const isEditing = customRuleEditingIdx === idx;
                              return (
                                <div
                                  key={idx}
                                  className={`rounded-lg border transition-colors ${
                                    isEditing
                                      ? "border-brand/40 bg-bg-input"
                                      : "border-border-strong/40 bg-bg-input hover:border-border-strong"
                                  }`}
                                >
                                  {isEditing ? (
                                    /* ── Edit mode ── */
                                    <div className="p-3 space-y-2">
                                      <input
                                        type="text"
                                        value={customRuleEditName}
                                        onChange={(e) => setCustomRuleEditName(e.target.value)}
                                        placeholder="Rule name"
                                        className="w-full bg-bg-sidebar border border-border-strong/40 focus:border-brand rounded-md px-3 py-1.5 text-[13px] text-text-base placeholder-text-muted/50 outline-none transition-colors font-medium"
                                      />
                                      <LineNumberedTextarea
                                        value={customRuleEditContent}
                                        onChange={setCustomRuleEditContent}
                                        placeholder="Write the rule content in Markdown…"
                                        variant="inline"
                                        rows={8}
                                        className="w-full"
                                      />
                                      <div className="flex items-center justify-end gap-2 pt-1">
                                        <button
                                          onClick={() => setCustomRuleEditingIdx(null)}
                                          className="px-3 py-1 text-[12px] text-text-muted hover:text-text-base transition-colors"
                                        >
                                          Cancel
                                        </button>
                                        <button
                                          onClick={handleCommitCustomRule}
                                          className="flex items-center gap-1 px-3 py-1 bg-brand hover:bg-brand-hover text-white text-[12px] font-medium rounded transition-colors"
                                        >
                                          <Check size={11} /> Save
                                        </button>
                                      </div>
                                    </div>
                                  ) : (
                                    /* ── View mode ── */
                                    <div className="flex items-center gap-3 px-3 py-2.5">
                                      <ScrollText size={14} className="flex-shrink-0 text-text-muted" />
                                      <div className="flex-1 min-w-0">
                                        <div className="text-[13px] font-medium text-text-base truncate">{rule.name || "Untitled Rule"}</div>
                                        {rule.content.trim() ? (
                                          <div className="text-[11px] text-text-muted truncate mt-0.5">
                                            {rule.content.trim().split("\n")[0]}
                                          </div>
                                        ) : (
                                          <div className="text-[11px] text-text-muted/60 italic mt-0.5">Empty — add content to activate</div>
                                        )}
                                      </div>
                                      <TokenPill text={rule.content} />
                                      <div className="flex items-center gap-1 flex-shrink-0">
                                        <button
                                          onClick={() => handleStartEditCustomRule(idx)}
                                          className="p-1.5 text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
                                          title="Edit"
                                        >
                                          <Edit2 size={12} />
                                        </button>
                                        <button
                                          onClick={() => handleDeleteCustomRule(idx)}
                                          className="p-1.5 text-text-muted hover:text-danger hover:bg-danger/10 rounded transition-colors"
                                          title="Delete"
                                        >
                                          <Trash2 size={12} />
                                        </button>
                                      </div>
                                    </div>
                                  )}
                                </div>
                              );
                            })}
                          </div>
                        )}
                      </section>

                      {/* ── Divider ── */}
                      <div className="border-t border-border-strong/30" />

                      {/* ── Global Rules ── */}
                      {(() => {
                        const unaddedRules = availableRules.filter(r => !projectRules.includes(r.id));
                        const filteredRules = globalRuleSearch.trim()
                          ? unaddedRules.filter(r =>
                              r.name.toLowerCase().includes(globalRuleSearch.toLowerCase()) ||
                              r.id.toLowerCase().includes(globalRuleSearch.toLowerCase())
                            )
                          : unaddedRules;
                        const emptyDropdownMessage = availableRules.length === 0
                          ? "No rules in the library yet."
                          : unaddedRules.length === 0
                            ? "All rules already added."
                            : "No rules match.";

                        return (
                          <section>
                            {/* Header */}
                            <div className="flex items-center justify-between mb-3">
                              <div className="flex items-center gap-2">
                                <ScrollText size={13} className="text-text-muted" />
                                <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Global Rules</span>
                                {projectRules.length > 0 && (
                                  <span className="text-[10px] bg-bg-sidebar border border-border-strong/40 rounded-full px-1.5 py-0.5 text-text-muted leading-none">
                                    {projectRules.length}
                                  </span>
                                )}
                              </div>
                              <div className="relative">
                                <button
                                  onClick={() => setGlobalRuleAdding(!globalRuleAdding)}
                                  className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium"
                                >
                                  <Plus size={12} /> Add from Library
                                </button>
                                {globalRuleAdding && (
                                  <div className="absolute right-0 top-full mt-1 w-72 bg-bg-sidebar border border-border-strong rounded-lg shadow-xl z-50 max-h-72 overflow-y-auto">
                                    <div className="p-2 border-b border-border-strong/40">
                                      <input
                                        type="text"
                                        value={globalRuleSearch}
                                        onChange={(e) => setGlobalRuleSearch(e.target.value)}
                                        onKeyDown={(e) => {
                                          if (e.key === "Escape") { setGlobalRuleAdding(false); setGlobalRuleSearch(""); }
                                          if (e.key === "Enter" && filteredRules.length === 1) {
                                            handleToggleProjectRule(filteredRules[0]!.id);
                                            setGlobalRuleAdding(false);
                                            setGlobalRuleSearch("");
                                          }
                                        }}
                                        placeholder="Search rules..."
                                        autoFocus
                                        className="w-full bg-bg-input border border-border-strong/40 focus:border-brand rounded px-2 py-1 text-[12px] text-text-base placeholder-text-muted/50 outline-none"
                                      />
                                    </div>
                                    <div className="py-1">
                                      {filteredRules.length === 0 ? (
                                        <div className="px-3 py-2 text-[12px] text-text-muted italic">
                                          {emptyDropdownMessage}
                                        </div>
                                      ) : (
                                        filteredRules.map((r) => (
                                          <button
                                            key={r.id}
                                            onClick={() => {
                                              handleToggleProjectRule(r.id);
                                              setGlobalRuleAdding(false);
                                              setGlobalRuleSearch("");
                                            }}
                                            className="w-full flex items-center gap-2 px-3 py-2 hover:bg-bg-input text-left transition-colors"
                                          >
                                            <ScrollText size={14} className="text-text-muted flex-shrink-0" />
                                            <div className="min-w-0">
                                              <div className="text-[12px] font-medium text-text-base truncate">{r.name}</div>
                                              <div className="text-[11px] text-text-muted truncate">{r.id}</div>
                                            </div>
                                          </button>
                                        ))
                                      )}
                                    </div>
                                  </div>
                                )}
                              </div>
                            </div>

                            {/* Selected rules list */}
                            {projectRules.length === 0 && !globalRuleAdding && (
                              <p className="text-[12px] text-text-muted italic pl-1">No global rules selected.</p>
                            )}
                            <div className="space-y-2">
                              {projectRules.map((ruleId) => {
                                const meta = availableRules.find(r => r.id === ruleId);
                                return (
                                  <div
                                    key={ruleId}
                                    className="bg-bg-input border border-border-strong/40 rounded-lg group flex items-center gap-3 px-3 py-2.5"
                                  >
                                    <ScrollText size={14} className="flex-shrink-0 text-text-muted" />
                                    <div className="flex-1 min-w-0">
                                      <div className="text-[13px] font-medium text-text-base truncate">
                                        {meta?.name ?? ruleId}
                                      </div>
                                      <div className="text-[11px] text-text-muted truncate">{ruleId}</div>
                                    </div>
                                    <TokenPill text={globalRuleContentCache[ruleId] ?? ""} />
                                    {!isRuleLocked(ruleId) && (
                                    <button
                                      onClick={() => handleToggleProjectRule(ruleId)}
                                      className="p-1.5 text-text-muted hover:text-danger hover:bg-danger/10 rounded transition-colors flex-shrink-0 opacity-0 group-hover:opacity-100"
                                      title="Remove"
                                    >
                                      <X size={12} />
                                    </button>
                                    )}
                                  </div>
                                );
                              })}
                            </div>

                            {availableRules.length === 0 && (
                              <div className="px-4 py-6 bg-bg-input border border-border-strong/40 rounded-lg text-center">
                                <ScrollText size={18} className="mx-auto mb-2 text-text-muted" strokeWidth={1.5} />
                                <p className="text-[13px] text-text-muted mb-1">No global rules yet.</p>
                                <p className="text-[12px] text-text-muted/70">Create reusable rules in the Rules section of the sidebar.</p>
                              </div>
                            )}
                          </section>
                        );
                      })()}

                      {dirty && (
                        <div className="flex justify-end">
                          <button
                            onClick={handleSave}
                            disabled={syncStatus === "syncing"}
                            className="flex items-center gap-1.5 px-4 py-2 bg-brand hover:bg-brand-hover text-white text-[13px] font-medium rounded shadow-sm transition-colors disabled:opacity-50"
                          >
                            <Check size={13} /> {syncStatus === "syncing" ? "Saving…" : "Save Changes"}
                          </button>
                        </div>
                      )}
                    </div>

                    {/* Help sidebar */}
                    <div className="w-52 flex-shrink-0">
                      <div className="rounded-md bg-bg-input border border-border-strong/30 px-3 py-2.5 text-[11px] text-text-muted space-y-2.5 sticky top-0">
                        <div>
                          <p className="font-medium text-text-base text-[12px]">Write rules to separate files</p>
                          <p className="leading-relaxed mt-1">
                            Instead of embedding rules inline, each rule is saved as its own file under{" "}
                            <code className="text-[10px] bg-bg-sidebar px-1 rounded">.automatic/instructions/</code>.
                            The instruction file becomes a short index that lists them.
                          </p>
                        </div>
                        <button
                          role="switch"
                          aria-checked={!!project.instructions_index_mode}
                          onClick={() => {
                            setProject({ ...project, instructions_index_mode: !project.instructions_index_mode });
                            setDirty(true);
                          }}
                          className={`w-full flex items-center justify-between gap-2 px-2 py-1.5 rounded transition-colors ${
                            project.instructions_index_mode ? "bg-brand/10 text-brand" : "bg-bg-sidebar text-text-muted"
                          }`}
                        >
                          <span className="text-[11px] font-medium">{project.instructions_index_mode ? "Enabled" : "Disabled"}</span>
                          <span
                            className={`relative inline-flex h-4 w-7 items-center rounded-full transition-colors flex-shrink-0 ${
                              project.instructions_index_mode ? "bg-brand" : "bg-border-strong/60"
                            }`}
                          >
                            <span
                              className={`inline-block h-3 w-3 transform rounded-full bg-white shadow transition-transform ${
                                project.instructions_index_mode ? "translate-x-3.5" : "translate-x-0.5"
                              }`}
                            />
                          </span>
                        </button>
                      </div>
                    </div>
                    </div>
                  );
                })()}

                {/* ── Commands tab ─────────────────────────────────────────── */}
                {projectTab === "commands" && (() => {
                  const customCommands: CustomCommand[] = project.custom_commands || [];

                  const handleAddCustomCommand = () => {
                    const newCommand: CustomCommand = {
                      name: "new-command",
                      content: "---\ndescription: Describe what this command does.\n---\n\nWrite the reusable prompt here.\n",
                    };
                    setProject({ ...project, custom_commands: [...customCommands, newCommand] });
                    setCustomCommandEditingIdx(customCommands.length);
                    setCustomCommandEditName(newCommand.name);
                    setCustomCommandEditContent(newCommand.content);
                    setDirty(true);
                  };

                  const handleDeleteCustomCommand = (idx: number) => {
                    const updated = customCommands.filter((_, i) => i !== idx);
                    setProject({ ...project, custom_commands: updated.length > 0 ? updated : undefined });
                    if (customCommandEditingIdx === idx) {
                      setCustomCommandEditingIdx(null);
                    } else if (customCommandEditingIdx !== null && customCommandEditingIdx > idx) {
                      setCustomCommandEditingIdx(customCommandEditingIdx - 1);
                    }
                    setDirty(true);
                  };

                  const handleStartEditCustomCommand = (idx: number) => {
                    setCustomCommandEditingIdx(idx);
                    setCustomCommandEditName(customCommands[idx]?.name ?? "");
                    setCustomCommandEditContent(customCommands[idx]?.content ?? "");
                  };

                  const handleCommitCustomCommand = () => {
                    if (customCommandEditingIdx === null) return;
                    const updated = customCommands.map((command, i) =>
                      i === customCommandEditingIdx
                        ? {
                            name: customCommandEditName.trim() || "untitled-command",
                            content: customCommandEditContent,
                          }
                        : command
                    );
                    setProject({ ...project, custom_commands: updated });
                    setCustomCommandEditingIdx(null);
                    setDirty(true);
                  };

                  return (
                    <div className="space-y-8">
                      <div className="flex items-center justify-between">
                        <div>
                          <h2 className="text-[15px] font-semibold text-text-base">Commands</h2>
                        </div>
                        {((project.user_commands?.length ?? 0) + customCommands.length) > 0 && (
                          <span className="text-[11px] text-brand bg-brand/10 px-2 py-0.5 rounded border border-brand/20">
                            {(project.user_commands?.length ?? 0) + customCommands.length} commands
                          </span>
                        )}
                      </div>

                      <section>
                        <div className="flex items-center justify-between mb-3">
                          <div className="flex items-center gap-2">
                            <Terminal size={13} className="text-text-muted" />
                            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Project Commands</span>
                            {customCommands.length > 0 && (
                              <span className="text-[10px] bg-bg-sidebar border border-border-strong/40 rounded-full px-1.5 py-0.5 text-text-muted leading-none">
                                {customCommands.length}
                              </span>
                            )}
                          </div>
                          <button
                            onClick={handleAddCustomCommand}
                            className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium"
                          >
                            <Plus size={12} /> Add Command
                          </button>
                        </div>

                        {customCommands.length === 0 ? (
                          <button
                            onClick={handleAddCustomCommand}
                            className="w-full flex items-center justify-center gap-2 px-4 py-6 border border-dashed border-border-strong/60 hover:border-brand/40 rounded-lg text-text-muted hover:text-brand transition-colors text-[13px]"
                          >
                            <Plus size={14} /> Create your first project command
                          </button>
                        ) : (
                          <div className="space-y-2">
                            {customCommands.map((command, idx) => {
                              const isEditing = customCommandEditingIdx === idx;
                              return (
                                <div
                                  key={`${command.name}-${idx}`}
                                  className={`rounded-lg border transition-colors ${
                                    isEditing
                                      ? "border-brand/40 bg-bg-input"
                                      : "border-border-strong/40 bg-bg-input hover:border-border-strong"
                                  }`}
                                >
                                  {isEditing ? (
                                    <div className="p-3 space-y-2">
                                      <input
                                        type="text"
                                        value={customCommandEditName}
                                        onChange={(e) => setCustomCommandEditName(e.target.value)}
                                        placeholder="command-name"
                                        className="w-full bg-bg-sidebar border border-border-strong/40 focus:border-brand rounded-md px-3 py-1.5 text-[13px] text-text-base placeholder-text-muted/50 outline-none transition-colors font-medium"
                                      />
                                      <LineNumberedTextarea
                                        value={customCommandEditContent}
                                        onChange={setCustomCommandEditContent}
                                        placeholder="Write the command as Markdown with optional YAML frontmatter..."
                                        variant="inline"
                                        rows={12}
                                        className="w-full"
                                      />
                                      <div className="flex items-center justify-end gap-2 pt-1">
                                        <button
                                          onClick={() => setCustomCommandEditingIdx(null)}
                                          className="px-3 py-1 text-[12px] text-text-muted hover:text-text-base transition-colors"
                                        >
                                          Cancel
                                        </button>
                                        <button
                                          onClick={handleCommitCustomCommand}
                                          className="flex items-center gap-1 px-3 py-1 bg-brand hover:bg-brand-hover text-white text-[12px] font-medium rounded transition-colors"
                                        >
                                          <Check size={11} /> Save
                                        </button>
                                      </div>
                                    </div>
                                  ) : (
                                    <div className="flex items-center gap-3 px-3 py-2.5">
                                      <Terminal size={14} className="flex-shrink-0 text-text-muted" />
                                      <div className="flex-1 min-w-0">
                                        <div className="text-[13px] font-medium text-text-base truncate">/{command.name || "untitled-command"}</div>
                                        <div className="text-[11px] text-text-muted truncate mt-0.5">
                                          {command.content.trim().split("\n").find((line) => line.trim() && !line.startsWith("---"))?.slice(0, 80) || "Custom command"}
                                        </div>
                                      </div>
                                      <TokenPill text={command.content} />
                                      <div className="flex items-center gap-1 flex-shrink-0">
                                        <button
                                          onClick={() => handleStartEditCustomCommand(idx)}
                                          className="p-1.5 text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
                                          title="Edit"
                                        >
                                          <Edit2 size={12} />
                                        </button>
                                        <button
                                          onClick={() => handleDeleteCustomCommand(idx)}
                                          className="p-1.5 text-text-muted hover:text-danger hover:bg-danger/10 rounded transition-colors"
                                          title="Delete"
                                        >
                                          <Trash2 size={12} />
                                        </button>
                                      </div>
                                    </div>
                                  )}
                                </div>
                              );
                            })}
                          </div>
                        )}
                      </section>

                      <section>
                        <div className="flex items-center justify-between mb-3">
                          <div className="flex items-center gap-2">
                            <div className="p-1 bg-icon-agent/10 rounded"><Globe size={12} className="text-icon-agent" /></div>
                            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Workspace Commands</span>
                            {(project.user_commands?.length ?? 0) > 0 && (
                              <span className="text-[10px] bg-bg-sidebar border border-border-strong/40 rounded-full px-1.5 py-0.5 text-text-muted leading-none">
                                {project.user_commands?.length ?? 0}
                              </span>
                            )}
                          </div>
                          <div className="relative">
                            <button
                              onClick={() => setUserCommandAdding(!userCommandAdding)}
                              className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium"
                            >
                              <Plus size={12} /> Add from Library
                            </button>
                            {userCommandAdding && (
                              <div className="absolute right-0 top-full mt-1 w-72 bg-bg-sidebar border border-border-strong rounded-lg shadow-xl z-50 max-h-72 overflow-y-auto">
                                <div className="p-2 border-b border-border-strong/40">
                                  <input
                                    type="text"
                                    value={userCommandSearch}
                                    onChange={(e) => setUserCommandSearch(e.target.value)}
                                    placeholder="Search commands..."
                                    className="w-full bg-bg-input border border-border-strong/40 focus:border-brand rounded px-2 py-1 text-[12px] text-text-base placeholder-text-muted/50 outline-none"
                                    autoFocus
                                  />
                                </div>
                                <div className="py-1">
                                  {availableUserCommands
                                    .filter((command) => {
                                      const search = userCommandSearch.toLowerCase();
                                      return (
                                        command.id.toLowerCase().includes(search) ||
                                        command.description.toLowerCase().includes(search)
                                      );
                                    })
                                    .filter((command) => !(project.user_commands ?? []).includes(command.id))
                                    .length === 0 ? (
                                    <div className="px-3 py-2 text-[12px] text-text-muted italic">
                                      {availableUserCommands.length === 0
                                        ? "No workspace commands available"
                                        : "All commands already added"}
                                    </div>
                                  ) : (
                                    availableUserCommands
                                      .filter((command) => {
                                        const search = userCommandSearch.toLowerCase();
                                        return (
                                          command.id.toLowerCase().includes(search) ||
                                          command.description.toLowerCase().includes(search)
                                        );
                                      })
                                      .filter((command) => !(project.user_commands ?? []).includes(command.id))
                                      .map((command) => (
                                        <button
                                          key={command.id}
                                          onClick={() => {
                                            const currentUserCommands = project.user_commands ?? [];
                                            setProject({
                                              ...project,
                                              user_commands: [...currentUserCommands, command.id],
                                            });
                                            setDirty(true);
                                            setUserCommandAdding(false);
                                            setUserCommandSearch("");
                                          }}
                                          className="w-full flex items-center gap-2 px-3 py-2 hover:bg-bg-input text-left transition-colors"
                                        >
                                          <Terminal size={14} className="text-text-muted flex-shrink-0" />
                                          <div className="min-w-0">
                                            <div className="text-[12px] font-medium text-text-base truncate">
                                              /{command.id}
                                            </div>
                                            <div className="text-[11px] text-text-muted truncate">
                                              {command.description || "No description"}
                                            </div>
                                          </div>
                                        </button>
                                      ))
                                  )}
                                </div>
                              </div>
                            )}
                          </div>
                        </div>

                        {(project.user_commands?.length ?? 0) === 0 ? (
                          <div className="text-[12px] text-text-muted/60 italic py-4 text-center">
                            No workspace commands selected. Add commands from your library to include them in this project.
                          </div>
                        ) : (
                          <div className="space-y-2">
                            {project.user_commands?.map((commandId) => {
                              const command = availableUserCommands.find((entry) => entry.id === commandId);
                              const isExpanded = expandedCommandId === commandId;

                              const handleToggleExpandCommand = async () => {
                                if (isExpanded) {
                                  setExpandedCommandId(null);
                                  setExpandedCommandContent("");
                                  setExpandedCommandError(null);
                                  return;
                                }
                                setExpandedCommandId(commandId);
                                setExpandedCommandContent("");
                                setExpandedCommandError(null);
                                setExpandedCommandLoading(true);
                                try {
                                  const raw: string = await invoke("read_user_command", { machineName: commandId });
                                  setExpandedCommandContent(raw);
                                } catch (err: unknown) {
                                  setExpandedCommandError(String(err));
                                } finally {
                                  setExpandedCommandLoading(false);
                                }
                              };

                              // Strip YAML frontmatter for the markdown preview body
                              const extractCommandBody = (raw: string): string => {
                                const match = raw.match(/^---\r?\n[\s\S]*?\r?\n---\r?\n?([\s\S]*)$/);
                                return match ? match[1]!.trimStart() : raw;
                              };

                              return (
                                <div
                                  key={commandId}
                                  className={`bg-bg-input border rounded-lg group transition-colors ${
                                    isExpanded ? "border-brand/40" : "border-border-strong/40"
                                  }`}
                                >
                                  {/* Row */}
                                  <div className="flex items-center gap-3 px-3 py-2.5">
                                    <Terminal size={14} className="flex-shrink-0 text-text-muted" />
                                    <button
                                      className="flex-1 flex items-center gap-2 text-left min-w-0"
                                      onClick={handleToggleExpandCommand}
                                    >
                                      <div className="flex-1 min-w-0">
                                        <div className="text-[13px] font-medium text-text-base truncate">
                                          /{command?.id ?? commandId}
                                        </div>
                                        <div className="text-[11px] text-text-muted truncate">
                                          {command?.description || commandId}
                                        </div>
                                      </div>
                                      <ChevronRight
                                        size={12}
                                        className={`text-text-muted flex-shrink-0 transition-transform ${isExpanded ? "rotate-90" : ""}`}
                                      />
                                    </button>
                                    <button
                                      onClick={() => {
                                        const updated = (project.user_commands ?? []).filter((id) => id !== commandId);
                                        setProject({ ...project, user_commands: updated.length > 0 ? updated : undefined });
                                        setDirty(true);
                                        if (isExpanded) {
                                          setExpandedCommandId(null);
                                          setExpandedCommandContent("");
                                        }
                                      }}
                                      className="p-1.5 text-text-muted hover:text-danger hover:bg-danger/10 rounded transition-colors flex-shrink-0 opacity-0 group-hover:opacity-100"
                                      title="Remove"
                                    >
                                      <X size={12} />
                                    </button>
                                  </div>

                                  {/* Expanded preview panel */}
                                  {isExpanded && (
                                    <div className="border-t border-border-strong/40">
                                      {/* Action bar */}
                                      {onNavigateToCommand && (
                                        <div className="flex items-center gap-3 px-3 py-2 border-b border-border-strong/30 bg-bg-sidebar/30">
                                          <button
                                            onClick={() => onNavigateToCommand(commandId)}
                                            className="flex items-center gap-1 text-[11px] text-text-muted hover:text-brand transition-colors"
                                            title="View this command in the Commands library"
                                          >
                                            <ExternalLink size={11} />
                                            View in library
                                          </button>
                                        </div>
                                      )}

                                      {/* Content */}
                                      <div className="px-4 py-3 max-h-80 overflow-y-auto custom-scrollbar">
                                        {expandedCommandLoading && (
                                          <p className="text-[12px] text-text-muted italic">Loading…</p>
                                        )}
                                        {expandedCommandError && (
                                          <p className="text-[12px] text-danger">{expandedCommandError}</p>
                                        )}
                                        {!expandedCommandLoading && !expandedCommandError && expandedCommandContent && (
                                          <MarkdownPreview content={extractCommandBody(expandedCommandContent)} />
                                        )}
                                      </div>
                                    </div>
                                  )}
                                </div>
                              );
                            })}
                          </div>
                        )}
                      </section>

                      {dirty && (
                        <div className="flex justify-end">
                          <button
                            onClick={handleSave}
                            disabled={syncStatus === "syncing"}
                            className="flex items-center gap-1.5 px-4 py-2 bg-brand hover:bg-brand-hover text-white text-[13px] font-medium rounded shadow-sm transition-colors disabled:opacity-50"
                          >
                            <Check size={13} /> {syncStatus === "syncing" ? "Saving..." : "Save Changes"}
                          </button>
                        </div>
                      )}
                    </div>
                  );
                })()}

                {/* ── Hooks tab ────────────────────────────────────────────── */}
                {projectTab === "hooks" && (() => {
                  const attachedHookIds = project.hooks ?? [];

                  // Set of agent ids on this project that actually support
                  // hooks. Hooks bound to an agent outside this set are still
                  // listed in the picker (with a warning chip) so the user can
                  // see why a hook would do nothing on this project, but they
                  // can also be attached — sync silently skips them.
                  const projectAgentIds = new Set(project.agents);
                  const hookCapableAgentIds = new Set(
                    availableAgents
                      .filter((a) => a.capabilities?.hooks)
                      .map((a) => a.id),
                  );
                  const projectHasHookCapableAgent = project.agents.some((id) =>
                    hookCapableAgentIds.has(id),
                  );

                  const matchesSearch = (entry: HookEntry) => {
                    const q = hookSearch.toLowerCase();
                    if (!q) return true;
                    return (
                      entry.id.toLowerCase().includes(q) ||
                      entry.name.toLowerCase().includes(q) ||
                      entry.event.toLowerCase().includes(q) ||
                      entry.agent.toLowerCase().includes(q)
                    );
                  };

                  const isCompatible = (entry: HookEntry) =>
                    projectAgentIds.has(entry.agent) &&
                    hookCapableAgentIds.has(entry.agent);

                  const pickerCandidates = availableHooks
                    .filter((h) => !attachedHookIds.includes(h.id))
                    .filter(matchesSearch);

                  const compatibleCandidates = pickerCandidates.filter(isCompatible);
                  const incompatibleCandidates = pickerCandidates.filter(
                    (h) => !isCompatible(h),
                  );

                  return (
                    <div className="space-y-8">
                      <div className="flex items-center justify-between">
                        <div>
                          <h2 className="text-[15px] font-semibold text-text-base">Hooks</h2>
                          <p className="text-[12px] text-text-muted mt-1">
                            Lifecycle hooks that run on agent events. Synced
                            per-agent into the project's settings on next sync.
                          </p>
                        </div>
                        {attachedHookIds.length > 0 && (
                          <span className="text-[11px] text-brand bg-brand/10 px-2 py-0.5 rounded border border-brand/20">
                            {attachedHookIds.length} hook{attachedHookIds.length === 1 ? "" : "s"}
                          </span>
                        )}
                      </div>

                      {!projectHasHookCapableAgent && (
                        <div className="rounded-lg border border-border-strong/40 bg-bg-input px-4 py-3 text-[12px] text-text-muted">
                          None of this project's agents support hooks today.
                          Add Claude Code or Codex CLI under{" "}
                          <button
                            onClick={() => selectTab("agents")}
                            className="text-brand hover:underline"
                          >
                            Configuration → Providers
                          </button>{" "}
                          to enable hook syncing.
                        </div>
                      )}

                      <section>
                        <div className="flex items-center justify-between mb-3">
                          <div className="flex items-center gap-2">
                            <div className="p-1 bg-icon-skill/10 rounded">
                              <Webhook size={12} className="text-icon-skill" />
                            </div>
                            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Attached Hooks</span>
                            {attachedHookIds.length > 0 && (
                              <span className="text-[10px] bg-bg-sidebar border border-border-strong/40 rounded-full px-1.5 py-0.5 text-text-muted leading-none">
                                {attachedHookIds.length}
                              </span>
                            )}
                          </div>
                          <div className="relative">
                            <button
                              onClick={() => setHookAdding(!hookAdding)}
                              className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium"
                            >
                              <Plus size={12} /> Add from Library
                            </button>
                            {hookAdding && (
                              <div className="absolute right-0 top-full mt-1 w-80 bg-bg-sidebar border border-border-strong rounded-lg shadow-xl z-50 max-h-80 overflow-y-auto">
                                <div className="p-2 border-b border-border-strong/40">
                                  <input
                                    type="text"
                                    value={hookSearch}
                                    onChange={(e) => setHookSearch(e.target.value)}
                                    placeholder="Search hooks..."
                                    className="w-full bg-bg-input border border-border-strong/40 focus:border-brand rounded px-2 py-1 text-[12px] text-text-base placeholder-text-muted/50 outline-none"
                                    autoFocus
                                  />
                                </div>
                                <div className="py-1">
                                  {availableHooks.length === 0 ? (
                                    <div className="px-3 py-2 text-[12px] text-text-muted italic">
                                      No hooks in the library yet.
                                    </div>
                                  ) : pickerCandidates.length === 0 ? (
                                    <div className="px-3 py-2 text-[12px] text-text-muted italic">
                                      All hooks already attached.
                                    </div>
                                  ) : (
                                    <>
                                      {compatibleCandidates.map((hook) => (
                                        <button
                                          key={hook.id}
                                          onClick={() => {
                                            setProject({
                                              ...project,
                                              hooks: [...attachedHookIds, hook.id],
                                            });
                                            setDirty(true);
                                            setHookAdding(false);
                                            setHookSearch("");
                                          }}
                                          className="w-full flex items-center gap-2 px-3 py-2 hover:bg-bg-input text-left transition-colors"
                                        >
                                          <Webhook size={14} className="text-text-muted flex-shrink-0" />
                                          <div className="min-w-0 flex-1">
                                            <div className="text-[12px] font-medium text-text-base truncate">
                                              {hook.name}
                                            </div>
                                            <div className="text-[11px] text-text-muted truncate">
                                              {hook.agent} · {hook.event}
                                            </div>
                                          </div>
                                        </button>
                                      ))}
                                      {incompatibleCandidates.length > 0 && (
                                        <>
                                          <div className="px-3 pt-2 pb-1 text-[10px] uppercase tracking-wider text-text-muted/70">
                                            Not compatible with this project
                                          </div>
                                          {incompatibleCandidates.map((hook) => {
                                            const reason = !projectAgentIds.has(hook.agent)
                                              ? `${hook.agent} not in this project`
                                              : "agent does not support hooks";
                                            return (
                                              <button
                                                key={hook.id}
                                                onClick={() => {
                                                  setProject({
                                                    ...project,
                                                    hooks: [...attachedHookIds, hook.id],
                                                  });
                                                  setDirty(true);
                                                  setHookAdding(false);
                                                  setHookSearch("");
                                                }}
                                                className="w-full flex items-center gap-2 px-3 py-2 hover:bg-bg-input text-left transition-colors opacity-60"
                                                title={reason}
                                              >
                                                <Webhook size={14} className="text-text-muted flex-shrink-0" />
                                                <div className="min-w-0 flex-1">
                                                  <div className="text-[12px] font-medium text-text-base truncate">
                                                    {hook.name}
                                                  </div>
                                                  <div className="text-[11px] text-text-muted truncate">
                                                    {hook.agent} · {hook.event} — {reason}
                                                  </div>
                                                </div>
                                              </button>
                                            );
                                          })}
                                        </>
                                      )}
                                    </>
                                  )}
                                </div>
                              </div>
                            )}
                          </div>
                        </div>

                        {attachedHookIds.length === 0 ? (
                          <div className="text-[12px] text-text-muted/60 italic py-4 text-center">
                            No hooks attached. Add hooks from your library to
                            run scripts on agent lifecycle events for this project.
                          </div>
                        ) : (
                          <div className="space-y-2">
                            {attachedHookIds.map((hookId) => {
                              const hook = availableHooks.find((h) => h.id === hookId);
                              const missing = !hook;
                              const incompatible =
                                !!hook && !isCompatible(hook);
                              return (
                                <div
                                  key={hookId}
                                  className="bg-bg-input border border-border-strong/40 rounded-lg group hover:border-border-strong transition-colors"
                                >
                                  <div className="flex items-center gap-3 px-3 py-2.5">
                                    <Webhook size={14} className="flex-shrink-0 text-text-muted" />
                                    <div className="flex-1 min-w-0">
                                      <div className="text-[13px] font-medium text-text-base truncate flex items-center gap-2">
                                        {hook?.name ?? hookId}
                                        {missing && (
                                          <span className="text-[10px] text-warning bg-warning/10 border border-warning/30 rounded px-1.5 py-0.5">
                                            Missing from library
                                          </span>
                                        )}
                                        {!missing && incompatible && (
                                          <span className="text-[10px] text-text-muted bg-bg-sidebar border border-border-strong/40 rounded px-1.5 py-0.5">
                                            Skipped on sync
                                          </span>
                                        )}
                                      </div>
                                      <div className="text-[11px] text-text-muted truncate">
                                        {hook
                                          ? `${hook.agent} · ${hook.event}`
                                          : "Hook was deleted or never existed — remove this entry."}
                                      </div>
                                    </div>
                                    <button
                                      onClick={() => {
                                        const updated = attachedHookIds.filter((id) => id !== hookId);
                                        setProject({
                                          ...project,
                                          hooks: updated.length > 0 ? updated : undefined,
                                        });
                                        setDirty(true);
                                      }}
                                      className="p-1.5 text-text-muted hover:text-danger hover:bg-danger/10 rounded transition-colors flex-shrink-0 opacity-0 group-hover:opacity-100"
                                      title="Remove"
                                    >
                                      <X size={12} />
                                    </button>
                                  </div>
                                </div>
                              );
                            })}
                          </div>
                        )}
                      </section>

                      {dirty && (
                        <div className="flex justify-end">
                          <button
                            onClick={handleSave}
                            disabled={syncStatus === "syncing"}
                            className="flex items-center gap-1.5 px-4 py-2 bg-brand hover:bg-brand-hover text-white text-[13px] font-medium rounded shadow-sm transition-colors disabled:opacity-50"
                          >
                            <Check size={13} /> {syncStatus === "syncing" ? "Saving..." : "Save Changes"}
                          </button>
                        </div>
                      )}
                    </div>
                  );
                })()}

                {/* ── Agents tab (custom_agents) ───────────────────────────── */}
                {projectTab === "custom_agents" && (() => {
                  const customAgents: CustomAgent[] = project.custom_agents || [];

                  const handleAddCustomAgent = () => {
                    const newAgent: CustomAgent = {
                      name: "New Agent",
                      content: "---\nname: new-agent\ndescription: A specialized AI assistant.\ntools: Read, Grep, Glob, Bash\nmodel: inherit\n---\n\nYou are a specialized AI assistant.\n"
                    };
                    setProject({ ...project, custom_agents: [...customAgents, newAgent] });
                    setCustomAgentEditingIdx(customAgents.length);
                    setCustomAgentEditName("New Agent");
                    setCustomAgentEditContent(newAgent.content);
                    setDirty(true);
                  };

                  const handleDeleteCustomAgent = (idx: number) => {
                    const updated = customAgents.filter((_, i) => i !== idx);
                    setProject({ ...project, custom_agents: updated.length > 0 ? updated : undefined });
                    if (customAgentEditingIdx === idx) {
                      setCustomAgentEditingIdx(null);
                    } else if (customAgentEditingIdx !== null && customAgentEditingIdx > idx) {
                      setCustomAgentEditingIdx(customAgentEditingIdx - 1);
                    }
                    setDirty(true);
                  };

                  const handleStartEditCustomAgent = (idx: number) => {
                    setCustomAgentEditingIdx(idx);
                    setCustomAgentEditName(customAgents[idx]?.name ?? "");
                    setCustomAgentEditContent(customAgents[idx]?.content ?? "");
                  };

                  const handleCommitCustomAgent = () => {
                    if (customAgentEditingIdx === null) return;
                    const updated = customAgents.map((a, i) =>
                      i === customAgentEditingIdx
                        ? { name: customAgentEditName.trim() || "Untitled Agent", content: customAgentEditContent }
                        : a
                    );
                    setProject({ ...project, custom_agents: updated });
                    setCustomAgentEditingIdx(null);
                    setDirty(true);
                  };

                  return (
                    <div className="space-y-8">
                      {/* ── Section header ── */}
                      <div className="flex items-center justify-between">
                        <div>
                          <h2 className="text-[15px] font-semibold text-text-base">Agents</h2>
                        </div>
                        {customAgents.length > 0 && (
                          <span className="text-[11px] text-brand bg-brand/10 px-2 py-0.5 rounded border border-brand/20">
                            {customAgents.length} {customAgents.length === 1 ? "agent" : "agents"}
                          </span>
                        )}
                      </div>

                      {/* ── Custom Agents ── */}
                      <section>
                        <div className="flex items-center justify-between mb-3">
                          <div className="flex items-center gap-2">
                            <MessagesSquare size={13} className="text-text-muted" />
                            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Project Agents</span>
                            {customAgents.length > 0 && (
                              <span className="text-[10px] bg-bg-sidebar border border-border-strong/40 rounded-full px-1.5 py-0.5 text-text-muted leading-none">
                                {customAgents.length}
                              </span>
                            )}
                          </div>
                          <button
                            onClick={handleAddCustomAgent}
                            className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium"
                          >
                            <Plus size={12} /> Add Agent
                          </button>
                        </div>

                        {customAgents.length === 0 ? (
                          <button
                            onClick={handleAddCustomAgent}
                            className="w-full flex items-center justify-center gap-2 px-4 py-6 border border-dashed border-border-strong/60 hover:border-brand/40 rounded-lg text-text-muted hover:text-brand transition-colors text-[13px]"
                          >
                            <Plus size={14} /> Create your first project agent
                          </button>
                        ) : (
                          <div className="space-y-2">
                            {customAgents.map((agent, idx) => {
                              const isEditing = customAgentEditingIdx === idx;
                              return (
                                <div
                                  key={idx}
                                  className={`rounded-lg border transition-colors ${
                                    isEditing
                                      ? "border-brand/40 bg-bg-input"
                                      : "border-border-strong/40 bg-bg-input hover:border-border-strong"
                                  }`}
                                >
                                  {isEditing ? (
                                    <div className="p-3 space-y-2">
                                      <input
                                        type="text"
                                        value={customAgentEditName}
                                        onChange={(e) => setCustomAgentEditName(e.target.value)}
                                        placeholder="Agent display name"
                                        className="w-full bg-bg-sidebar border border-border-strong/40 focus:border-brand rounded-md px-3 py-1.5 text-[13px] text-text-base placeholder-text-muted/50 outline-none transition-colors font-medium"
                                      />
                                      <LineNumberedTextarea
                                        value={customAgentEditContent}
                                        onChange={setCustomAgentEditContent}
                                        placeholder="Write the agent content as Markdown with YAML frontmatter..."
                                        variant="inline"
                                        rows={12}
                                        className="w-full"
                                      />
                                      <div className="flex items-center justify-end gap-2 pt-1">
                                        <button
                                          onClick={() => setCustomAgentEditingIdx(null)}
                                          className="px-3 py-1 text-[12px] text-text-muted hover:text-text-base transition-colors"
                                        >
                                          Cancel
                                        </button>
                                        <button
                                          onClick={handleCommitCustomAgent}
                                          className="flex items-center gap-1 px-3 py-1 bg-brand hover:bg-brand-hover text-white text-[12px] font-medium rounded transition-colors"
                                        >
                                          <Check size={11} /> Save
                                        </button>
                                      </div>
                                    </div>
                                  ) : (
                                    <div className="flex items-center gap-3 px-3 py-2.5">
                                      <MessagesSquare size={14} className="flex-shrink-0 text-text-muted" />
                                      <div className="flex-1 min-w-0">
                                        <div className="text-[13px] font-medium text-text-base truncate">{agent.name || "Untitled Agent"}</div>
                                        {agent.content.trim() ? (
                                          <div className="text-[11px] text-text-muted truncate mt-0.5">
                                            {agent.content.trim().split("\n").find(l => l.trim() && !l.startsWith("---"))?.slice(0, 60) || "Custom agent"}
                                          </div>
                                        ) : (
                                          <div className="text-[11px] text-text-muted/60 italic mt-0.5">Empty</div>
                                        )}
                                      </div>
                                      <TokenPill text={agent.content} />
                                      <div className="flex items-center gap-1 flex-shrink-0">
                                        <button
                                          onClick={() => handleStartEditCustomAgent(idx)}
                                          className="p-1.5 text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
                                          title="Edit"
                                        >
                                          <Edit2 size={12} />
                                        </button>
                                        <button
                                          onClick={() => handleDeleteCustomAgent(idx)}
                                          className="p-1.5 text-text-muted hover:text-danger hover:bg-danger/10 rounded transition-colors"
                                          title="Delete"
                                        >
                                          <Trash2 size={12} />
                                        </button>
                                      </div>
                                    </div>
                                  )}
                                </div>
                              );
                            })}
                          </div>
                        )}
                      </section>

                      {/* ── Workspace Agents (from ~/.automatic/agents/) ── */}
                      <section>
                        <div className="flex items-center justify-between mb-3">
                          <div className="flex items-center gap-2">
                            <div className="p-1 bg-icon-agent/10 rounded"><Globe size={12} className="text-icon-agent" /></div>
                            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Workspace Agents</span>
                            {(project.user_agents?.length ?? 0) > 0 && (
                              <span className="text-[10px] bg-bg-sidebar border border-border-strong/40 rounded-full px-1.5 py-0.5 text-text-muted leading-none">
                                {project.user_agents?.length ?? 0}
                              </span>
                            )}
                          </div>
                          <div className="relative" ref={userAgentDropdownRef}>
                            <button
                              onClick={() => setUserAgentAdding(!userAgentAdding)}
                              className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium"
                            >
                              <Plus size={12} /> Add from Library
                            </button>
                            {userAgentAdding && (
                              <div className="absolute right-0 top-full mt-1 w-64 bg-bg-sidebar border border-border-strong rounded-lg shadow-xl z-50 max-h-64 overflow-y-auto">
                                <div className="p-2 border-b border-border-strong/40">
                                  <input
                                    type="text"
                                    value={userAgentSearch}
                                    onChange={(e) => setUserAgentSearch(e.target.value)}
                                    placeholder="Search agents..."
                                    className="w-full bg-bg-input border border-border-strong/40 focus:border-brand rounded px-2 py-1 text-[12px] text-text-base placeholder-text-muted/50 outline-none"
                                    autoFocus
                                  />
                                </div>
                                <div className="py-1">
                                  {availableUserAgents
                                    .filter((a) => {
                                      const search = userAgentSearch.toLowerCase();
                                      return (
                                        a.name.toLowerCase().includes(search) ||
                                        a.id.toLowerCase().includes(search)
                                      );
                                    })
                                    .filter((a) => !(project.user_agents ?? []).includes(a.id))
                                    .length === 0 ? (
                                    <div className="px-3 py-2 text-[12px] text-text-muted italic">
                                      {availableUserAgents.length === 0
                                        ? "No workspace agents available"
                                        : "All agents already added"}
                                    </div>
                                  ) : (
                                    availableUserAgents
                                      .filter((a) => {
                                        const search = userAgentSearch.toLowerCase();
                                        return (
                                          a.name.toLowerCase().includes(search) ||
                                          a.id.toLowerCase().includes(search)
                                        );
                                      })
                                      .filter((a) => !(project.user_agents ?? []).includes(a.id))
                                      .map((agent) => (
                                        <button
                                          key={agent.id}
                                          onClick={() => {
                                            const currentUserAgents = project.user_agents ?? [];
                                            setProject({
                                              ...project,
                                              user_agents: [...currentUserAgents, agent.id],
                                            });
                                            setDirty(true);
                                            setUserAgentAdding(false);
                                            setUserAgentSearch("");
                                          }}
                                          className="w-full flex items-center gap-2 px-3 py-2 hover:bg-bg-input text-left transition-colors"
                                        >
                                          <MessagesSquare size={14} className="text-text-muted flex-shrink-0" />
                                          <div className="min-w-0">
                                            <div className="text-[12px] font-medium text-text-base truncate">
                                              {agent.name}
                                            </div>
                                            <div className="text-[11px] text-text-muted truncate">
                                              {agent.id}
                                            </div>
                                          </div>
                                        </button>
                                      ))
                                  )}
                                </div>
                              </div>
                            )}
                          </div>
                        </div>

                        {(project.user_agents?.length ?? 0) === 0 ? (
                          <div className="text-[12px] text-text-muted/60 italic py-4 text-center">
                            No workspace agents selected. Add agents from your library to include them in this project.
                          </div>
                        ) : (
                          <div className="space-y-1">
                            {project.user_agents?.map((agentId) => {
                              const agent = availableUserAgents.find((a) => a.id === agentId);
                              return (
                                <div
                                  key={agentId}
                                  className="flex items-center gap-3 px-3 py-2 bg-bg-input border border-border-strong/40 hover:border-border-strong rounded-lg transition-colors"
                                >
                                  <MessagesSquare size={14} className="flex-shrink-0 text-text-muted" />
                                  <div className="flex-1 min-w-0">
                                    <div className="text-[13px] font-medium text-text-base truncate">
                                      {agent?.name ?? agentId}
                                    </div>
                                    <div className="text-[11px] text-text-muted truncate">
                                      {agentId}
                                    </div>
                                  </div>
                                  <button
                                    onClick={() => {
                                      const updated = (project.user_agents ?? []).filter((id) => id !== agentId);
                                      setProject({ ...project, user_agents: updated.length > 0 ? updated : undefined });
                                      setDirty(true);
                                    }}
                                    className="p-1.5 text-text-muted hover:text-danger hover:bg-danger/10 rounded transition-colors flex-shrink-0"
                                    title="Remove"
                                  >
                                    <X size={12} />
                                  </button>
                                </div>
                              );
                            })}
                          </div>
                        )}
                      </section>

                      {dirty && (
                        <div className="flex justify-end">
                          <button
                            onClick={handleSave}
                            disabled={syncStatus === "syncing"}
                            className="flex items-center gap-1.5 px-4 py-2 bg-brand hover:bg-brand-hover text-white text-[13px] font-medium rounded shadow-sm transition-colors disabled:opacity-50"
                          >
                            <Check size={13} /> {syncStatus === "syncing" ? "Saving..." : "Save Changes"}
                          </button>
                        </div>
                      )}
                    </div>
                  );
                })()}

                {/* ── Summary tab ──────────────────────────────────────── */}
                {projectTab === "summary" && project && (
                  <SummaryPanel
                    project={project}
                    isCreating={isCreating}
                    memories={memories}
                    projectFiles={projectFiles}
                    linkDocEntries={linkDocEntries}
                    recsDisplayCount={recsDisplayCount}
                    activityEntries={activityEntries}
                    loadingActivity={loadingActivity}
                    loadingMemories={loadingMemories}
                    selectTab={selectTab}
                    updateField={updateField}
                    handleExternalLinkClick={handleExternalLinkClick}
                  />
                )}

                {/* ── Details tab ──────────────────────────────────────── */}
                 {/* ── Agents tab ───────────────────────────────────────── */}
                {projectTab === "agents" && (
                   <section>
                      <AgentSelector
                        agentIds={project.agents}
                        availableAgents={availableAgents}
                        onAdd={(id) => addItem("agents", id)}
                        onRemove={(i) => handleRemoveAgent(i)}
                        emptyMessage="No agent tools selected. Add tools to enable config sync."
                        agentOptions={project.agent_options}
                        onOptionChange={(agentId, patch) => {
                          const current = project.agent_options?.[agentId] ?? { claude_rules_in_dot_claude: true };
                          setProject({
                            ...project,
                            agent_options: {
                              ...(project.agent_options ?? {}),
                              [agentId]: { ...current, ...patch },
                            },
                            updated_at: new Date().toISOString(),
                          });
                          setDirty(true);
                        }}
                      />
                   </section>
                 )}

                {/* ── Skills tab ───────────────────────────────────────── */}
                {projectTab === "skills" && (
                  <>
                    {/* ── Project Skills (custom, inline) ──────────────── */}
                    {(() => {
                      const customSkills: CustomSkill[] = project.custom_skills || [];

                      // Helper: persist an updated project immediately (save + sync).
                      const saveProjectWithSkills = async (updatedProject: Project) => {
                        if (!selectedName) return;
                        const toSave = { ...updatedProject, name: selectedName, updated_at: new Date().toISOString() };
                        setProject(toSave);
                        try {
                          setSyncStatus("syncing");
                          await invoke("save_project", {
                            name: selectedName,
                            data: JSON.stringify(toSave, null, 2),
                          });
                          setProjectDetailsMap((prev) => new Map(prev).set(selectedName, toSave));
                          setDirty(false);
                          setSyncStatus(toSave.directory && toSave.agents.length > 0 ? "Saved & synced" : "Saved");
                          if (toSave.directory && toSave.agents.length > 0) {
                            setDriftReport({ drifted: false, agents: [] });
                            setDriftByProject((prev) => ({ ...prev, [selectedName]: false }));
                          }
                        } catch (err: unknown) {
                          setError(`Save failed: ${err}`);
                          setSyncStatus(null);
                        }
                      };

                      const handleAddCustomSkill = () => {
                        const newSkill: CustomSkill = {
                          name: "new-skill",
                          content: "---\nname: New Skill\ndescription: Describe what this skill does and when to use it.\n---\n\nWrite the skill instructions here.\n",
                        };
                        setProject({ ...project, custom_skills: [...customSkills, newSkill] });
                        setCustomSkillEditingIdx(customSkills.length);
                        setCustomSkillEditName(newSkill.name);
                        setCustomSkillEditContent(newSkill.content);
                        setDirty(true);
                      };

                      const handleDeleteCustomSkill = async (idx: number) => {
                        const updated = customSkills.filter((_, i) => i !== idx);
                        if (customSkillEditingIdx === idx) {
                          setCustomSkillEditingIdx(null);
                        } else if (customSkillEditingIdx !== null && customSkillEditingIdx > idx) {
                          setCustomSkillEditingIdx(customSkillEditingIdx - 1);
                        }
                        await saveProjectWithSkills({ ...project, custom_skills: updated.length > 0 ? updated : undefined });
                      };

                      const handleStartEditCustomSkill = (idx: number) => {
                        setCustomSkillEditingIdx(idx);
                        setCustomSkillEditName(customSkills[idx]?.name ?? "");
                        setCustomSkillEditContent(customSkills[idx]?.content ?? "");
                      };

                      const handleCommitCustomSkill = async () => {
                        if (customSkillEditingIdx === null) return;
                        const updated = customSkills.map((skill, i) =>
                          i === customSkillEditingIdx
                            ? {
                                name: customSkillEditName.trim().toLowerCase().replace(/\s+/g, "-") || "untitled-skill",
                                content: customSkillEditContent,
                              }
                            : skill
                        );
                        setCustomSkillEditingIdx(null);
                        await saveProjectWithSkills({ ...project, custom_skills: updated });
                      };

                      return (
                        <section className="mb-6">
                          <div className="flex items-center justify-between mb-3">
                            <div className="flex items-center gap-2">
                              <Code size={13} className="text-text-muted" />
                              <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Project Skills</span>
                              {customSkills.length > 0 && (
                                <span className="text-[10px] bg-bg-sidebar border border-border-strong/40 rounded-full px-1.5 py-0.5 text-text-muted leading-none">
                                  {customSkills.length}
                                </span>
                              )}
                            </div>
                            <button
                              onClick={handleAddCustomSkill}
                              className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium"
                            >
                              <Plus size={12} /> Add Skill
                            </button>
                          </div>

                          {customSkills.length === 0 ? (
                            <button
                              onClick={handleAddCustomSkill}
                              className="w-full flex items-center justify-center gap-2 px-4 py-6 border border-dashed border-border-strong/60 hover:border-brand/40 rounded-lg text-text-muted hover:text-brand transition-colors text-[13px]"
                            >
                              <Plus size={14} /> Create a project-scoped skill
                            </button>
                          ) : (
                            <div className="space-y-2">
                              {customSkills.map((skill, idx) => {
                                const isEditing = customSkillEditingIdx === idx;
                                return (
                                  <div
                                    key={`${skill.name}-${idx}`}
                                    className={`rounded-lg border transition-colors ${
                                      isEditing
                                        ? "border-brand/40 bg-bg-input"
                                        : "border-border-strong/40 bg-bg-input hover:border-border-strong"
                                    }`}
                                  >
                                    {isEditing ? (
                                      <div className="p-3 space-y-2">
                                        <input
                                          type="text"
                                          value={customSkillEditName}
                                          onChange={(e) => setCustomSkillEditName(e.target.value)}
                                          placeholder="skill-name (lowercase, hyphens)"
                                          className="w-full bg-bg-sidebar border border-border-strong/40 focus:border-brand rounded-md px-3 py-1.5 text-[13px] text-text-base placeholder-text-muted/50 outline-none transition-colors font-mono"
                                        />
                                        <LineNumberedTextarea
                                          value={customSkillEditContent}
                                          onChange={setCustomSkillEditContent}
                                          placeholder="Write the skill content as Markdown with YAML frontmatter..."
                                          variant="inline"
                                          rows={12}
                                          className="w-full"
                                        />
                                        <div className="flex items-center justify-end gap-2 pt-1">
                                          <button
                                            onClick={() => setCustomSkillEditingIdx(null)}
                                            className="px-3 py-1 text-[12px] text-text-muted hover:text-text-base transition-colors"
                                          >
                                            Cancel
                                          </button>
                                          <button
                                            onClick={handleCommitCustomSkill}
                                            className="flex items-center gap-1 px-3 py-1 bg-brand hover:bg-brand-hover text-white text-[12px] font-medium rounded transition-colors"
                                          >
                                            <Check size={11} /> Save
                                          </button>
                                        </div>
                                      </div>
                                    ) : (
                                      <div className="flex items-center gap-3 px-3 py-2.5">
                                        <Code size={14} className="flex-shrink-0 text-text-muted" />
                                        <div className="flex-1 min-w-0">
                                          <div className="text-[13px] font-medium text-text-base truncate font-mono">{skill.name || "untitled-skill"}</div>
                                          {skill.content.trim() ? (
                                            <div className="text-[11px] text-text-muted truncate mt-0.5">
                                              {skill.content.trim().split("\n").find(l => l.trim() && !l.startsWith("---"))?.slice(0, 60) || "Custom skill"}
                                            </div>
                                          ) : (
                                            <div className="text-[11px] text-text-muted/60 italic mt-0.5">Empty</div>
                                          )}
                                        </div>
                                        <div className="flex items-center gap-1 flex-shrink-0">
                                          <button
                                            onClick={() => handleStartEditCustomSkill(idx)}
                                            className="p-1.5 text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
                                            title="Edit"
                                          >
                                            <Edit2 size={12} />
                                          </button>
                                          <button
                                            onClick={async () => {
                                              if (!selectedName) return;
                                              try {
                                                setSyncStatus("syncing");
                                                // Save the skill content to the global registry
                                                await invoke("save_skill", { name: skill.name, content: skill.content });
                                                // Move from custom_skills to global skills
                                                const remainingCustom = customSkills.filter((_, i) => i !== idx);
                                                const updatedProject = {
                                                  ...project,
                                                  skills: [...project.skills, skill.name],
                                                  custom_skills: remainingCustom.length > 0 ? remainingCustom : undefined,
                                                };
                                                if (customSkillEditingIdx === idx) {
                                                  setCustomSkillEditingIdx(null);
                                                } else if (customSkillEditingIdx !== null && customSkillEditingIdx > idx) {
                                                  setCustomSkillEditingIdx(customSkillEditingIdx - 1);
                                                }
                                                await saveProjectWithSkills(updatedProject);
                                                await loadAvailableSkills();
                                                setSyncStatus(`Imported "${skill.name}" to global registry`);
                                                setTimeout(() => setSyncStatus(null), 4000);
                                              } catch (err: unknown) {
                                                setSyncStatus(`Import failed: ${err}`);
                                                setTimeout(() => setSyncStatus(null), 4000);
                                              }
                                            }}
                                            className="p-1.5 text-text-muted hover:text-success hover:bg-success/10 rounded transition-colors"
                                            title="Import to global skill registry"
                                          >
                                            <Upload size={12} />
                                          </button>
                                          <button
                                            onClick={() => handleDeleteCustomSkill(idx)}
                                            className="p-1.5 text-text-muted hover:text-danger hover:bg-danger/10 rounded transition-colors"
                                            title="Delete"
                                          >
                                            <Trash2 size={12} />
                                          </button>
                                        </div>
                                      </div>
                                    )}
                                  </div>
                                );
                              })}
                            </div>
                          )}
                        </section>
                      );
                    })()}

                    {/* Global Skills */}
                     <section>
                        <SkillSelector
                          skills={project.skills}
                          availableSkills={availableSkills}
                          onAdd={(s) => addItem("skills", s)}
                          onRemove={(i) => removeItem("skills", i)}
                          showRemoveButtonAlways
                          lockedSkills={pluginLockedSkills}
                          emptyMessage="No skills attached."
                          onReadSkill={async (skillName) => {
                            const content: string = await invoke("read_skill", { name: skillName });
                            return content;
                          }}
                          onNavigateToSkill={onNavigateToSkill}
                          onForkSkill={async (skillName, content) => {
                            if (!selectedName) return;
                            try {
                              // Derive a unique name: "<name>-copy", then
                              // "<name>-copy-2", "<name>-copy-3", …
                              const existingCustomNames = new Set((project.custom_skills ?? []).map(s => s.name));
                              const taken = new Set([...project.skills, ...existingCustomNames]);
                              let copyName = `${skillName}-copy`;
                              let n = 2;
                              while (taken.has(copyName)) {
                                copyName = `${skillName}-copy-${n}`;
                                n++;
                              }

                              // Add as a project-scoped custom skill and auto-save.
                              const newCustomSkill: CustomSkill = { name: copyName, content };
                              const forkedProject = {
                                ...project,
                                name: selectedName,
                                custom_skills: [...(project.custom_skills ?? []), newCustomSkill],
                                updated_at: new Date().toISOString(),
                              };
                              setProject(forkedProject);
                              await invoke("save_project", {
                                name: selectedName,
                                data: JSON.stringify(forkedProject, null, 2),
                              });
                              setProjectDetailsMap((prev) => new Map(prev).set(selectedName, forkedProject));
                              setDirty(false);
                              notifyProjectUpdated();
                              setSyncStatus(`Forked "${skillName}" → project skill "${copyName}"`);
                              setTimeout(() => setSyncStatus(null), 5000);
                            } catch (err: unknown) {
                              setError(`Fork failed: ${err}`);
                            }
                          }}
                         />
                      </section>

                     {/* ── AI skill suggestions ──────────────────────────── */}
                     <section>
                       <div className="flex items-center gap-2">
                         <Sparkles size={12} className="text-text-muted" />
                         <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">AI Suggestions</span>
                         {aiSkillsSuggestions.length > 0 && !aiSkillsLoading && (
                           <span className="text-[10px] font-semibold px-1.5 py-0.5 rounded-full bg-brand/10 text-brand border border-brand/20 leading-none">
                             {aiSkillsSuggestions.length}
                           </span>
                         )}
                         <div className="flex-1" />
                         <button
                           onClick={handleSuggestSkills}
                           disabled={aiSkillsLoading}
                           className="flex items-center gap-1.5 px-2.5 py-1.5 text-[11px] font-medium text-text-muted hover:text-text-base border border-border-strong/50 rounded-md disabled:opacity-40 transition-colors"
                           title="Ask AI to suggest skills based on this project's configuration"
                         >
                           <Sparkles size={11} className={aiSkillsLoading ? "animate-pulse" : ""} />
                           {aiSkillsLoading ? "Analysing…" : "Suggest skills"}
                         </button>
                       </div>

                       {aiSkillsLoading && (
                         <div className="mt-2 bg-bg-input border border-border-strong/40 rounded-lg px-4 py-4 flex items-center gap-3">
                           <RefreshCw size={13} className="text-brand animate-spin flex-shrink-0" />
                           <p className="text-[12px] text-text-muted">Searching the skill library and Discover…</p>
                         </div>
                       )}

                       {!aiSkillsLoading && aiSkillsSuggestions.length === 0 && (
                         <p className="mt-1.5 text-[12px] text-text-muted">
                           Click "Suggest skills" to get AI-powered recommendations based on this project.
                         </p>
                       )}

                       {!aiSkillsLoading && aiSkillsSuggestions.length > 0 && (
                         <div className="mt-2 bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
                           {aiSkillsSuggestions.map((rec) => (
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
                                    {(onNavigateToSkillStoreWithResult || onNavigateToSkillStore) && (
                                      <button
                                        onClick={() => {
                                          // If the recommendation has full metadata (id, name, source, installs)
                                          // from the AI search result, use it to deep-link directly to the skill.
                                          if (rec.metadata && onNavigateToSkillStoreWithResult) {
                                            try {
                                              const meta = JSON.parse(rec.metadata) as { id: string; name: string; source: string; installs: number };
                                              if (meta.id && meta.name && meta.source) {
                                                onNavigateToSkillStoreWithResult(meta);
                                                return;
                                              }
                                            } catch {
                                              // fall through to plain query
                                            }
                                          }
                                          onNavigateToSkillStore?.(rec.title);
                                        }}
                                        className="text-[11px] font-medium text-text-muted hover:text-text-base border border-border-strong/50 rounded px-2 py-1 transition-colors flex items-center gap-1"
                                      >
                                        <Search size={10} /> View
                                      </button>
                                    )}
                                     <SkillAddButton
                                      rec={rec}
                                      alreadyAdded={project.skills.includes(rec.title)}
                                      onAdd={async (skillName) => {
                                        const added = await addItem("skills", skillName);
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
                     </section>

                  </>
                )}

                {/* ── MCP Servers tab ──────────────────────────────────── */}
                {projectTab === "mcp_servers" && (() => {
                  // Agents that cannot have MCP config written by Automatic (e.g. Warp, Goose).
                  const noMcpAgents = availableAgents.filter(
                    (a) => project.agents.includes(a.id) && a.mcp_note
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
                })()}


                {/* ── Documentation: Files & Dirs tab ─────────────── */}
                {projectTab === "docs_files" && project && (
                  <DocsFilesPanel
                    project={project}
                    fileDocEntries={fileDocEntries}
                    showDocPathForm={showDocPathForm}
                    setShowDocPathForm={setShowDocPathForm}
                    docNewPath={docNewPath}
                    setDocNewPath={setDocNewPath}
                    docNewPathSummary={docNewPathSummary}
                    setDocNewPathSummary={setDocNewPathSummary}
                    handleAddDocPath={handleAddDocPath}
                    handleBrowseDocPath={handleBrowseDocPath}
                    handleBrowseDocFile={handleBrowseDocFile}
                    removeDocEntry={removeDocEntry}
                  />
                )}

                 {/* ── Documentation: Links tab ─────────────────────── */}
                 {projectTab === "docs_links" && project && (
                   <DocsLinksPanel
                     project={project}
                     linkDocEntries={linkDocEntries}
                     showDocLinkForm={showDocLinkForm}
                     setShowDocLinkForm={setShowDocLinkForm}
                     docNewLinkUrl={docNewLinkUrl}
                     setDocNewLinkUrl={setDocNewLinkUrl}
                     docNewLinkLabel={docNewLinkLabel}
                     setDocNewLinkLabel={setDocNewLinkLabel}
                     handleAddDocLink={handleAddDocLink}
                     removeDocEntry={removeDocEntry}
                     handleExternalLinkClick={handleExternalLinkClick}
                   />
                 )}

                {/* ── Documentation: Notes tab ─────────────────────── */}
                {projectTab === "docs_notes" && project && (
                  <DocsNotesPanel
                    project={project}
                    docs={parsedDocs()}
                    docNoteSelected={docNoteSelected}
                    docNoteContent={docNoteContent}
                    setDocNoteContent={setDocNoteContent}
                    docNoteDirty={docNoteDirty}
                    setDocNoteDirty={setDocNoteDirty}
                    docNoteSaving={docNoteSaving}
                    docNoteLoading={docNoteLoading}
                    docNewNoteCreating={docNewNoteCreating}
                    setDocNewNoteCreating={setDocNewNoteCreating}
                    docNewNoteName={docNewNoteName}
                    setDocNewNoteName={setDocNewNoteName}
                    createDocNote={createDocNote}
                    loadDocNote={loadDocNote}
                    saveDocNote={saveDocNote}
                    removeDocEntry={removeDocEntry}
                  />
                )}

                {/* ── Memory tab ──────────────────────────────────── */}
                {projectTab === "memory" && selectedName && (
                  <MemoryPanel
                    projectName={selectedName}
                    project={project}
                    memories={memories}
                    loadingMemories={loadingMemories}
                    reloadMemories={loadMemories}
                    onError={setError}
                  />
                )}

                {/* ── Activity tab ─────────────────────────────────── */}
                {projectTab === "activity" && selectedName && (
                  <ActivityPanel
                    projectName={selectedName}
                    activityPageEntries={activityPageEntries}
                    activityPage={activityPage}
                    activityTotalCount={activityTotalCount}
                    loadingActivityPage={loadingActivityPage}
                    reloadActivityPage={loadActivityPage}
                  />
                )}

                {/* ── Recommendations tab ──────────────────────────── */}
                {projectTab === "recommendations" && (
                  <RecommendationsPanel
                    project={project}
                    normalRecs={normalRecs}
                    aiSkillsRollupCount={aiSkillsRollupCount}
                    aiMcpRollupCount={aiMcpRollupCount}
                    recsDisplayCount={recsDisplayCount}
                    aiRecsLoading={aiRecsLoading}
                    aiRecsLastRunAt={aiRecsLastRunAt}
                    handleUpdateAiRecommendations={handleUpdateAiRecommendations}
                    handleDismissRecommendation={handleDismissRecommendation}
                    removeRecommendation={removeRecommendation}
                    addItem={addItem}
                    selectTab={selectTab}
                    onNavigateToSkillStore={onNavigateToSkillStore}
                    onNavigateToSkillStoreWithResult={onNavigateToSkillStoreWithResult}
                    onNavigateToDiscoverMcp={onNavigateToDiscoverMcp}
                  />
                )}

                {/* ── Groups tab ───────────────────────────────────── */}
                {projectTab === "groups" && selectedName && (
                  <GroupsPanel
                    projectName={selectedName}
                    projectGroupMemberships={projectGroupMemberships}
                    allGroups={allGroups}
                    loadingGroups={loadingGroups}
                    reloadGroups={loadGroups}
                    onAddToGroup={handleAddToGroup}
                    onRemoveFromGroup={handleRemoveFromGroup}
                    onRemoveFromAllGroups={handleRemoveFromAllGroups}
                    onNavigateToGroup={onNavigateToGroup}
                  />
                )}

                {/* ── Settings tab ─────────────────────────────────────── */}
                {projectTab === "settings" && (
                  <SettingsPanel project={project} setProject={setProject} setDirty={setDirty} />
                )}

              </div>
            </div>
            )}
            </>}

            {/* ── Project controls bar (Configuration, Insights, Activity, Memory) — pinned to bottom ── */}
            {!isCreating && (
              <div className="flex items-center justify-end gap-0 px-6 border-t border-border-strong/20 bg-bg-input/20 flex-shrink-0">
                {PROJECT_CONTROLS.map((ctrl) => (
                  <button
                    key={ctrl.id}
                    onClick={() => selectGroup(ctrl.id)}
                    className={`px-3 py-1.5 text-[12px] font-medium transition-colors relative flex items-center gap-1.5 ${
                      activeToolName === null && projectGroup === ctrl.id
                        ? "text-text-base"
                        : "text-text-muted hover:text-text-base"
                    }`}
                  >
                    {ctrl.label}
                    {ctrl.id === "insights" && recsDisplayCount > 0 && (
                      <span className="text-[10px] font-semibold px-1.5 py-0.5 rounded-full bg-warning/15 text-warning border border-warning/20 leading-none">
                        {recsDisplayCount}
                      </span>
                    )}
                    {activeToolName === null && projectGroup === ctrl.id && (
                      <span className="absolute top-0 left-0 right-0 h-[2px] bg-brand rounded-b" />
                    )}
                  </button>
                ))}
              </div>
            )}
          </div>
        ) : (
          // Loading: SEAM 1 (selectProject) or SEAM 2 (startCreate) will
          // populate `project` shortly. Render an empty pane to avoid flashing
          // the old "No Project Selected" empty state — the router now owns
          // the no-selection screen (ProjectsOverview).
          null
        )}
      </div>
    </div>

    {/* ── Drift diff modal ─────────────────────────────────────────────── */}
    {driftDiffFile && (
      <DriftDiffModal
        file={driftDiffFile.file}
        agentLabel={driftDiffFile.agentLabel}
        projectName={selectedName ?? undefined}
        onClose={() => setDriftDiffFile(null)}
        onResolved={handleDriftResolved}
      />
    )}

    {/* ── Instruction file conflict modal ──────────────────────────────── */}
    {instructionConflict && selectedName && (
      <InstructionConflictModal
        conflict={instructionConflict}
        projectName={selectedName}
        onAdopt={(adopted) => handleAdoptInstructionFile(instructionConflict.filename, adopted)}
        onOverwrite={() => handleOverwriteInstructionFile(instructionConflict.filename)}
        onClose={() => setInstructionConflict(null)}
      />
    )}

    {rebuildPreview && (
      <RebuildConfirmationModal
        preview={rebuildPreview}
        busy={rebuildBusy}
        onConfirm={confirmRebuild}
        onClose={() => {
          if (!rebuildBusy) {
            setRebuildPreview(null);
            setSyncStatus(null);
          }
        }}
      />
    )}

    {unifiedSourcePicker && selectedName && (
      <SwitchToUnifiedModal
        candidates={unifiedSourcePicker}
        busy={unifiedSourcePickerBusy}
        onConfirm={async (filename) => {
          if (!project) {
            return;
          }
          setUnifiedSourcePickerBusy(true);
          try {
            await invoke("switch_to_unified_mode", {
              name: selectedName,
              sourceFilename: filename,
            });
            setProject({
              ...project,
              instruction_mode: "unified",
              updated_at: new Date().toISOString(),
            });
            setDirty(false);
            await loadProjectFiles(selectedName);
            notifyProjectUpdated();
            setUnifiedSourcePicker(null);
          } catch (e) {
            console.error("switch_to_unified_mode failed", e);
          } finally {
            setUnifiedSourcePickerBusy(false);
          }
        }}
        onClose={() => { if (!unifiedSourcePickerBusy) setUnifiedSourcePicker(null); }}
      />
    )}

    {/* Apply-template modal */}
    {showProjectTemplatePicker && project && !isCreating && (
      <ApplyProjectTemplateModal
        templates={[...availableProjectTemplates].sort((a, b) => a.name.localeCompare(b.name))}
        selected={templateApplySelection}
        onSelect={setTemplateApplySelection}
        onCancel={() => {
          setShowProjectTemplatePicker(false);
          setTemplateApplySelection(null);
          setTemplateApplyResult(null);
        }}
        onConfirm={() => {
          if (!templateApplySelection) return;
          applyProjectTemplate(templateApplySelection);
        }}
        result={templateApplyResult}
        onAcknowledge={() => {
          setShowProjectTemplatePicker(false);
          setTemplateApplySelection(null);
          setTemplateApplyResult(null);
        }}
      />
    )}

    </>
  );
}

