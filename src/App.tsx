import { useEffect, useRef, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { applyTheme, Theme, THEMES } from "./lib/theme";
import { ProfileProvider } from "./contexts/ProfileContext";
import { useCurrentUser } from "./contexts/ProfileContext";
import { initAnalytics, setAnalyticsEnabled, trackNavigation } from "./lib/analytics";
import GettingStarted from "./pages/GettingStarted";

import Skills from "./pages/workspace/Skills";
import SkillStore from "./pages/discover/SkillStore";
import Projects from "./pages/workspace/Projects";
import ProjectGroups from "./pages/workspace/ProjectGroups";
import ProjectTemplates from "./pages/workspace/ProjectTemplates";
import McpServers from "./pages/workspace/McpServers";
import Templates from "./pages/workspace/Templates";
import Rules from "./pages/workspace/Rules";
import UserAgents from "./pages/workspace/UserAgents";
import Commands from "./pages/workspace/Commands";
import Agents from "./pages/workspace/Agents";
import Tools from "./pages/workspace/Tools";
import Recommendations from "./pages/Recommendations";
import Settings from "./pages/Settings";
import Library from "./pages/library/Library";
import Discover from "./pages/discover/Discover";
import DiscoverTemplates from "./pages/discover/DiscoverTemplates";
import DiscoverMcp from "./pages/discover/DiscoverMcp";
import DiscoverCollections from "./pages/discover/DiscoverCollections";
import FirstRunWizard from "./pages/FirstRunWizard";
import { TaskLogProvider, useTaskLog } from "./contexts/TaskLogContext";
import TaskLog from "./components/TaskLog";
import { UpdateProvider } from "./contexts/UpdateContext";
import UpdateToast from "./components/UpdateToast";
import RemoteInstallDialog from "./components/RemoteInstallDialog";
import WorkspaceSidebar from "./components/WorkspaceSidebar";
import Featured from "./pages/community/Featured";
import { ClipboardList, Code, Server, ChevronDown, LayoutTemplate, Bot, Layers, Library as LibraryIcon, Store, Settings as SettingsIcon, ScrollText, Sparkles, PackageOpen, Puzzle, Lightbulb, List, Wrench, MessagesSquare, Terminal, PanelLeft, Star, RefreshCw } from "lucide-react";
import { flag } from "./lib/flags";
import CloudSync from "./pages/CloudSync";
import graphLogo from "../logos/graph_5.svg";
import "./App.css";

// ── Section / Tab mapping ────────────────────────────────────────────────────

type Section = "start" | "workspace" | "library" | "discover" | "community";

const SECTION_TABS: Record<Section, string[]> = {
  start: ["getting-started", "recommendations"],
  workspace: ["projects", "project-groups"],
  library: ["library-home", "project-templates", "templates", "rules", "user-agents", "commands", "skills", "mcp", "agents", "tools"],
  discover: ["discover-home", "discover-collections", "discover-templates", "skill-store", "discover-mcp"],
  community: ["community-featured"],
};

const DEFAULT_TAB: Record<Section, string> = {
  start: "getting-started",
  workspace: "projects",
  library: "library-home",
  discover: "discover-home",
  community: "community-featured",
};

const SECTION_LABELS: Record<Section, string> = {
  start: "Start",
  workspace: "Workspace",
  library: "Library",
  discover: "Discover",
  community: "Community",
};

function sectionForTab(tabId: string): Section {
  for (const [section, tabs] of Object.entries(SECTION_TABS)) {
    if (tabs.includes(tabId)) return section as Section;
  }
  return "start";
}

// ── Small helper components ──────────────────────────────────────────────────

/**
 * Small icon button that toggles the Task Log panel open/closed.
 * Must be rendered inside TaskLogProvider.
 */
function TaskLogToggleButton() {
  const { isVisible, show, dismiss } = useTaskLog();

  return (
    <button
      onClick={isVisible ? dismiss : show}
      className="flex items-center justify-center w-[26px] h-[26px] rounded-md text-text-muted hover:bg-bg-sidebar hover:text-text-base transition-colors"
      aria-label={isVisible ? "Close task log" : "Open task log"}
      title={isVisible ? "Close task log" : "Open task log"}
    >
      <List size={14} />
    </button>
  );
}

/**
 * Bootstraps Amplitude analytics once the user profile and settings are loaded.
 * Rendered inside ProfileProvider so it can access useCurrentUser().
 */
function AnalyticsBootstrap() {
  const { userId, isLoaded } = useCurrentUser();
  const initialised = useRef(false);

  useEffect(() => {
    if (!isLoaded || initialised.current) return;
    initialised.current = true;

    async function boot() {
      try {
        const settings: any = await invoke("read_settings");
        const enabled: boolean = settings?.analytics_enabled ?? true;
        await initAnalytics(userId ?? "anonymous", enabled);
      } catch (e) {
        console.error("[analytics] Failed to read settings for analytics init:", e);
        await initAnalytics(userId ?? "anonymous", false);
      }
    }

    boot();
  }, [isLoaded, userId]);

  return null;
}

function App() {
  // ── Active tab + section state ───────────────────────────────────────────
  const [activeTab, setActiveTab] = useState(() => {
    // Migrate legacy "nexus." localStorage keys to "automatic." prefix
    const legacy = localStorage.getItem("nexus.activeTab");
    if (legacy) {
      localStorage.setItem("automatic.activeTab", legacy);
      localStorage.removeItem("nexus.activeTab");
    }
    const saved = localStorage.getItem("automatic.activeTab") || legacy;
    // Migrate from removed tabs
    if (saved === "activity" || saved === "configuration" || saved === "dashboard") return "getting-started";
    if (saved === "support") return "settings";
    // Migrate utilities into settings
    if (saved === "token-estimator" || saved === "ai-playground") return "settings";
    return saved || "getting-started";
  });

  const [activeSection, setActiveSection] = useState<Section>(() => {
    if (activeTab === "settings") {
      // When settings is active, restore last real section for the sidebar
      const rawSaved = localStorage.getItem("automatic.activeSection");
      // Migrate legacy "marketplace" section id (renamed to "discover").
      const saved = (rawSaved === "marketplace" ? "discover" : rawSaved) as Section | null;
      return saved && SECTION_TABS[saved] ? saved : "workspace";
    }
    return sectionForTab(activeTab);
  });

  // ── Per-section sidebar collapsed state ────────────────────────────────────
  // Sections default to expanded, except community which defaults to collapsed.
  const SIDEBAR_COLLAPSED_DEFAULTS: Record<Section, boolean> = {
    start: false,
    workspace: false,
    library: false,
    discover: false,
    community: true,
  };

  const [sidebarBySection, setSidebarBySection] = useState<Record<Section, boolean>>(() => {
    try {
      const saved = localStorage.getItem("automatic.sidebarBySection");
      if (saved) {
        const parsed = JSON.parse(saved) as Record<string, boolean>;
        // Merge with defaults so new sections get their default value
        return { ...SIDEBAR_COLLAPSED_DEFAULTS, ...parsed } as Record<Section, boolean>;
      }
    } catch { /* ignore parse errors */ }
    // Migrate legacy single-value setting
    const legacy = localStorage.getItem("automatic.sidebarCollapsed");
    if (legacy !== null) {
      const val = legacy === "true";
      return { start: val, workspace: val, library: val, discover: val, community: true };
    }
    return { ...SIDEBAR_COLLAPSED_DEFAULTS };
  });

  const sidebarCollapsed = sidebarBySection[activeSection] ?? false;

  const setSidebarCollapsed = useCallback((valueOrFn: boolean | ((prev: boolean) => boolean)) => {
    setSidebarBySection((prev) => {
      const current = prev[activeSection] ?? false;
      const next = typeof valueOrFn === "function" ? valueOrFn(current) : valueOrFn;
      return { ...prev, [activeSection]: next };
    });
  }, [activeSection]);

  useEffect(() => {
    localStorage.setItem("automatic.sidebarBySection", JSON.stringify(sidebarBySection));
  }, [sidebarBySection]);

  // ── Group filter for Projects page ────────────────────────────────────────
  const [activeGroupFilter, setActiveGroupFilter] = useState<string | null>(null);

  // ── Pending navigation state ─────────────────────────────────────────────
  const [pendingProject, setPendingProject] = useState<string | null>(null);
  const [pendingProjectTab, setPendingProjectTab] = useState<string | null>(null);
  const [pendingTemplate, setPendingTemplate] = useState<string | null>(null);
  const [pendingSkill, setPendingSkill] = useState<string | null>(null);
  const [pendingCreateWithTemplate, setPendingCreateWithTemplate] = useState<string | null>(null);
  const [projectsResetKey, setProjectsResetKey] = useState(0);
  const [skillStoreResetKey, setSkillStoreResetKey] = useState(0);
  const [discoverTemplatesResetKey, setDiscoverTemplatesResetKey] = useState(0);
  const [discoverMcpResetKey, setDiscoverMcpResetKey] = useState(0);
  const [discoverCollectionsResetKey, setDiscoverCollectionsResetKey] = useState(0);
  const [pendingSkillStoreId, setPendingSkillStoreId] = useState<string | null>(null);
  const [pendingSkillStoreQuery, setPendingSkillStoreQuery] = useState<string | null>(null);
  const [pendingSkillStoreResult, setPendingSkillStoreResult] = useState<{ id: string; name: string; source: string; installs: number } | null>(null);
  const [pendingMcpSlug, setPendingMcpSlug] = useState<string | null>(null);
  const [pendingMcpQuery, setPendingMcpQuery] = useState<string | null>(null);
  const [pendingCollectionQuery, setPendingCollectionQuery] = useState<string | null>(null);
  const [pendingDiscoverTemplate, setPendingDiscoverTemplate] = useState<string | null>(null);
  const [pendingMcpServer, setPendingMcpServer] = useState<string | null>(null);
  const [pendingGroup, setPendingGroup] = useState<string | null>(null);
  const [pendingCommand, setPendingCommand] = useState<string | null>(null);
  const [pendingSettingsPage, setPendingSettingsPage] = useState<string | null>(null);

  // ── Deep-link install dialog ────────────────────────────────────────────
  const [deepLinkInstall, setDeepLinkInstall] = useState<{
    repo: string;
    git_ref: string | null;
    directory: string | null;
  } | null>(null);

  useEffect(() => {
    const unlisten = listen<{ repo: string; git_ref: string | null; directory: string | null }>(
      "deep-link://install",
      (event) => {
        setDeepLinkInstall(event.payload);
      }
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // ── App version ──────────────────────────────────────────────────────────
  const [appVersion, setAppVersion] = useState<string>("");
  useEffect(() => { getVersion().then(setAppVersion).catch(() => {}); }, []);

  // ── Theme Init ───────────────────────────────────────────────────────────
  const [activeTheme, setActiveTheme] = useState<Theme>(
    () => (localStorage.getItem("automatic.theme") as Theme | null) ?? "system"
  );
  useEffect(() => {
    applyTheme(activeTheme);
  }, []);

  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const handleChange = () => {
      if ((localStorage.getItem("automatic.theme") ?? "system") === "system") {
        applyTheme("system");
      }
    };
    mq.addEventListener("change", handleChange);
    return () => mq.removeEventListener("change", handleChange);
  }, []);

  // ── First-run wizard ─────────────────────────────────────────────────────
  const [showWizard, setShowWizard] = useState<boolean | null>(null);
  const [wizardIsReopen, setWizardIsReopen] = useState(false);

  useEffect(() => {
    async function checkWizard() {
      try {
        const settings: any = await invoke("read_settings");
        setShowWizard(!(settings?.wizard_completed ?? false));
      } catch {
        setShowWizard(true);
      }
    }
    checkWizard();
  }, []);

  const handleWizardComplete = (answers: { analyticsEnabled: boolean; createdProjectName?: string }) => {
    setAnalyticsEnabled(answers.analyticsEnabled);
    setShowWizard(false);
    if (answers.createdProjectName) {
      navigateToProject(answers.createdProjectName);
    } else {
      setActiveTab("getting-started");
    }
  };

  // ── Persist active tab + section ─────────────────────────────────────────
  useEffect(() => {
    localStorage.setItem("automatic.activeTab", activeTab);
  }, [activeTab]);

  useEffect(() => {
    localStorage.setItem("automatic.activeSection", activeSection);
    // Save per-section last tab
    if (activeTab !== "settings") {
      localStorage.setItem(`automatic.lastTab.${activeSection}`, activeTab);
    }
  }, [activeSection, activeTab]);

  // ── Section-aware tab setter ─────────────────────────────────────────────
  const setActiveTabWithSection = useCallback((tabId: string) => {
    const section = sectionForTab(tabId);
    setActiveSection(section);
    setActiveTab(tabId);
  }, []);

  // ── Navigation helpers ───────────────────────────────────────────────────
  const navigateToProject = (projectName: string, tab?: string) => {
    setPendingProject(projectName);
    setPendingProjectTab(tab ?? null);
    setActiveTabWithSection("projects");
  };

  const navigateToTemplate = (templateName: string) => {
    setPendingTemplate(templateName);
    setActiveTabWithSection("project-templates");
  };

  const navigateToSkill = (skillName: string) => {
    setPendingSkill(skillName);
    setActiveTabWithSection("skills");
  };

  const navigateToCreateWithTemplate = (templateName: string) => {
    setPendingCreateWithTemplate(templateName);
    setActiveTabWithSection("projects");
  };

  const navigateToMcpServer = (serverName: string) => {
    setPendingMcpServer(serverName);
    setActiveTabWithSection("mcp");
  };

  const navigateToGroup = (groupName: string) => {
    setPendingGroup(groupName);
    setActiveTabWithSection("project-groups");
  };

  const navigateToCommand = (commandId: string) => {
    setPendingCommand(commandId);
    setActiveTabWithSection("commands");
  };

  const navigateToSettings = (page: string) => {
    setPendingSettingsPage(page);
    setActiveTab("settings");
  };

  const navigateToSkillStore = (skillId: string) => {
    setPendingSkillStoreId(skillId);
    const bareName = skillId.includes("/") ? skillId.split("/").pop()! : skillId;
    setPendingSkillStoreQuery(bareName);
    setPendingSkillStoreResult(null);
    setActiveTabWithSection("skill-store");
  };

  const navigateToSkillStoreWithResult = (result: { id: string; name: string; source: string; installs: number }) => {
    setPendingSkillStoreResult(result);
    setPendingSkillStoreId(null);
    setPendingSkillStoreQuery(null);
    setActiveTabWithSection("skill-store");
  };

  const navigateToDiscoverMcp = (slug: string) => {
    setPendingMcpSlug(slug);
    setPendingMcpQuery(slug);
    setActiveTabWithSection("discover-mcp");
  };

  const navigateToDiscoverTemplates = (templateName: string) => {
    setPendingDiscoverTemplate(templateName);
    setActiveTabWithSection("discover-templates");
  };

  const navigateToDiscoverCollections = (query: string) => {
    setPendingCollectionQuery(query);
    setActiveTabWithSection("discover-collections");
  };

  // ── Tab click + double-click refresh ─────────────────────────────────────
  const REFRESHABLE_TABS: Record<string, () => void> = {
    "projects": () => setProjectsResetKey((k) => k + 1),
    "skill-store": () => setSkillStoreResetKey((k) => k + 1),
    "discover-templates": () => setDiscoverTemplatesResetKey((k) => k + 1),
    "discover-mcp": () => setDiscoverMcpResetKey((k) => k + 1),
    "discover-collections": () => setDiscoverCollectionsResetKey((k) => k + 1),
  };

  const handleTabClick = (id: string) => {
    if (activeTab === id && REFRESHABLE_TABS[id]) {
      REFRESHABLE_TABS[id]!();
    }
    setActiveTab(id);
    trackNavigation(id);
  };

  const handleSectionClick = (section: Section) => {
    if (section === activeSection && activeTab !== "settings") return;
    setActiveSection(section);
    const lastTab = localStorage.getItem(`automatic.lastTab.${section}`);
    const tab = lastTab && SECTION_TABS[section].includes(lastTab) ? lastTab : DEFAULT_TAB[section];
    setActiveTab(tab);
    trackNavigation(tab);
  };

  // ── Sidebar NavItem ──────────────────────────────────────────────────────
  const NavItem = ({ id, icon: Icon, label, count }: { id: string; icon: React.ComponentType<{ size?: number; className?: string }>; label: string; count?: number }) => {
    const isActive = activeTab === id;
    return (
      <button
        onClick={() => handleTabClick(id)}
        className={`w-full flex items-center gap-2.5 px-3 py-1.5 rounded-md text-[13px] font-medium transition-colors ${
          isActive
            ? "bg-bg-sidebar text-text-base"
            : "text-text-muted hover:bg-bg-sidebar hover:text-text-base"
        }`}
      >
        <Icon size={14} className={`shrink-0 ${isActive ? "text-text-base" : "text-text-muted"}`} />
        <span className="flex-1 text-left">{label}</span>
        {count != null && (
          <span className="text-[11px] bg-bg-sidebar text-text-muted px-1.5 rounded-sm">
            {count}
          </span>
        )}
      </button>
    );
  };

  // ── Compute grouped projects for workspace sidebar ───────────────────────
  // ── Render ───────────────────────────────────────────────────────────────
  return (
    <UpdateProvider>
    <TaskLogProvider>
    <ProfileProvider>
    <AnalyticsBootstrap />
    {showWizard === true && (
      <FirstRunWizard
        onComplete={handleWizardComplete}
        onCancel={wizardIsReopen ? () => { setShowWizard(false); setWizardIsReopen(false); } : undefined}
      />
    )}
    <RemoteInstallDialog
      isOpen={deepLinkInstall !== null}
      repo={deepLinkInstall?.repo ?? ""}
      gitRef={deepLinkInstall?.git_ref}
      directory={deepLinkInstall?.directory}
      onClose={() => setDeepLinkInstall(null)}
      onInstalled={() => setDeepLinkInstall(null)}
    />
    <div
      className="relative flex flex-col h-screen w-screen overflow-hidden bg-bg-base text-[#fafafa] selection:bg-brand/30"
      aria-hidden={showWizard === true}
    >
      {/* ── Top bar — drag region with section toggle and actions ───── */}
      <div
        data-tauri-drag-region
        className="h-11 flex-shrink-0 flex items-center border-b border-border-strong/40 bg-bg-base select-none relative"
      >
        {/* Left: sidebar toggle (after traffic-light clearance) */}
        <div className="pl-20 relative z-10">
          {activeTab !== "settings" && activeTab !== "sync" && (
            <button
              onClick={() => setSidebarCollapsed((c) => !c)}
              className="flex items-center justify-center w-[26px] h-[26px] rounded-md transition-colors text-text-muted hover:bg-bg-sidebar hover:text-text-base"
              aria-label={sidebarCollapsed ? "Show sidebar" : "Hide sidebar"}
              title={sidebarCollapsed ? "Show sidebar" : "Hide sidebar"}
            >
              <PanelLeft size={14} />
            </button>
          )}
        </div>

        {/* Center: section toggle pill */}
        <div className="absolute left-0 right-0 flex items-center justify-center pointer-events-none z-0">
          <div className="flex items-center bg-bg-input border border-border-strong/40 rounded-lg p-0.5 pointer-events-auto">
            {(["start", "workspace", "library", "discover", "community"] as const).map((section) => {
              const isActive = activeSection === section && activeTab !== "settings" && activeTab !== "sync";
              return (
                <button
                  key={section}
                  onClick={() => handleSectionClick(section)}
                  className={`px-3 py-1 rounded-md text-[12px] font-medium transition-all ${
                    isActive
                      ? "text-text-base bg-bg-sidebar shadow-sm"
                      : "text-text-muted hover:text-text-base"
                  }`}
                >
                  {SECTION_LABELS[section]}
                </button>
              );
            })}
          </div>
        </div>

        {/* Right: contextual actions + task log toggle + settings cog */}
        <div className="ml-auto pr-4 flex items-center gap-2 relative z-10">
          {activeTab === "skills" && (
            <button
              onClick={() => setActiveTabWithSection("skill-store")}
              className="flex h-[26px] items-center gap-1.5 px-2.5 rounded-md text-[11px] font-medium bg-brand hover:bg-brand-hover text-white shadow-sm transition-colors border border-transparent"
            >
              <Store size={13} />
              Skill Store
            </button>
          )}
          {activeTab === "project-templates" && (
            <button
              onClick={() => setActiveTabWithSection("discover-templates")}
              className="flex h-[26px] items-center gap-1.5 px-2.5 rounded-md text-[11px] font-medium bg-brand hover:bg-brand-hover text-white shadow-sm transition-colors border border-transparent"
            >
              <Store size={13} />
              Discover Templates
            </button>
          )}
          {activeTab === "mcp" && (
            <button
              onClick={() => setActiveTabWithSection("discover-mcp")}
              className="flex h-[26px] items-center gap-1.5 px-2.5 rounded-md text-[11px] font-medium bg-brand hover:bg-brand-hover text-white shadow-sm transition-colors border border-transparent"
            >
              <Store size={13} />
              Discover MCP Servers
            </button>
          )}
          <TaskLogToggleButton />
          {flag("cloud_sync") && (
            <button
              onClick={() => setActiveTab("sync")}
              className={`flex items-center justify-center w-[26px] h-[26px] rounded-md transition-colors ${
                activeTab === "sync"
                  ? "bg-bg-sidebar text-text-base"
                  : "text-text-muted hover:bg-bg-sidebar hover:text-text-base"
              }`}
              aria-label="Cloud Sync"
              title="Cloud Sync"
            >
              <RefreshCw size={14} />
            </button>
          )}
          <button
            onClick={() => setActiveTab("settings")}
            className={`flex items-center justify-center w-[26px] h-[26px] rounded-md transition-colors ${
              activeTab === "settings"
                ? "bg-bg-sidebar text-text-base"
                : "text-text-muted hover:bg-bg-sidebar hover:text-text-base"
            }`}
            aria-label="Settings"
            title="Settings"
          >
            <SettingsIcon size={14} />
          </button>
        </div>
      </div>

      {/* ── Sidebar + Main content ────────────────────────────────────── */}
      <div className="flex flex-1 min-h-0 overflow-hidden">
      {/* ── Sidebar ─────────────────────────────────────────────────────── */}
      <aside className={`flex-shrink-0 bg-bg-input border-r border-border-strong/40 flex flex-col transition-all duration-200 overflow-hidden ${sidebarCollapsed || activeTab === "settings" || activeTab === "sync" ? "w-0 border-r-0" : "w-[270px]"}`}>

        {/* Section-specific navigation */}
        <nav className="flex-1 overflow-y-auto py-3 px-3 custom-scrollbar">

          {/* ── Start sidebar ─────────────────────────────────────────── */}
          {activeSection === "start" && (
            <ul className="space-y-0.5">
              <NavItem id="getting-started" icon={Sparkles} label="Getting Started" />
              <NavItem id="recommendations" icon={Lightbulb} label="Insights" />
            </ul>
          )}

          {/* ── Workspace sidebar ──────────────────────────────────────── */}
          {activeSection === "workspace" && (
            <WorkspaceSidebar
              activeTab={activeTab}
              onTabClick={handleTabClick}
              onNavigateToProject={navigateToProject}
              activeGroupFilter={activeGroupFilter}
              onFilterByGroup={setActiveGroupFilter}
              onCreateProject={() => {
                setActiveGroupFilter(null);
                setActiveTabWithSection("projects");
                window.dispatchEvent(new CustomEvent("create-project"));
              }}
            />
          )}

          {/* ── Library sidebar ─────────────────────────────────────────── */}
          {activeSection === "library" && (
            <div className="space-y-3">
              <div>
                <ul className="space-y-0.5 mb-2">
                  <NavItem id="library-home" icon={LibraryIcon} label="Overview" />
                </ul>
                <p className="px-3 mb-1 text-[10px] font-semibold uppercase tracking-wider text-text-muted">My Library</p>
                <ul className="space-y-0.5">
                  <NavItem id="project-templates" icon={LayoutTemplate} label="Templates" />
                  <NavItem id="templates" icon={ClipboardList} label="Instructions" />
                  <NavItem id="rules" icon={ScrollText} label="Rules" />
                  <NavItem id="user-agents" icon={MessagesSquare} label="Sub-Agents" />
                  <NavItem id="commands" icon={Terminal} label="Commands" />
                  <NavItem id="skills" icon={Code} label="Skills" />
                  <NavItem id="mcp" icon={Server} label="MCP Servers" />
                  <NavItem id="agents" icon={Bot} label="Providers" />
                  <NavItem id="tools" icon={Wrench} label="Tools" />
                </ul>
              </div>
            </div>
          )}

          {/* ── Discover sidebar ────────────────────────────────────────── */}
          {activeSection === "discover" && (
            <ul className="space-y-0.5">
              <NavItem id="discover-home" icon={Sparkles} label="Overview" />
              <NavItem id="discover-collections" icon={PackageOpen} label="Collections" />
              <NavItem id="discover-templates" icon={Layers} label="Templates" />
              <NavItem id="skill-store" icon={Puzzle} label="Skills" />
              <NavItem id="discover-mcp" icon={Server} label="MCP Servers" />
            </ul>
          )}

          {/* ── Community sidebar ──────────────────────────────────────── */}
          {activeSection === "community" && (
            <ul className="space-y-0.5">
              <NavItem id="community-featured" icon={Star} label="Featured" />
            </ul>
          )}

        </nav>

        {/* Sidebar footer — dev theme switcher (dev builds only) */}
        {import.meta.env.DEV && (() => {
          const [open, setOpen] = useState(false);
          const current = THEMES.find((t) => t.id === activeTheme) ?? THEMES[0]!;
          return (
            <div className="px-3 pt-3 pb-1 relative">
              <button
                onClick={() => setOpen((o) => !o)}
                className="w-full flex items-center gap-2.5 px-3 py-1.5 rounded-md text-[13px] font-medium text-text-muted hover:bg-bg-sidebar hover:text-text-base transition-colors"
              >
                <span
                  className="w-2.5 h-2.5 rounded-full shrink-0"
                  style={{ backgroundColor: current.colors.primary }}
                />
                <span className="flex-1 text-left">{current.name}</span>
                <ChevronDown size={12} className={`transition-transform ${open ? "rotate-180" : ""}`} />
              </button>
              {open && (
                <div className="absolute bottom-full left-3 right-3 mb-1 rounded-md overflow-hidden bg-bg-input border border-border-strong/40">
                  {THEMES.map((t) => (
                    <button
                      key={t.id}
                      onClick={() => {
                        setActiveTheme(t.id);
                        applyTheme(t.id);
                        localStorage.setItem("automatic.theme", t.id);
                        setOpen(false);
                      }}
                      className={`w-full flex items-center gap-2.5 px-3 py-1.5 text-[13px] transition-colors ${
                        t.id === activeTheme
                          ? "text-text-base bg-bg-sidebar"
                          : "text-text-muted hover:bg-bg-sidebar hover:text-text-base"
                      }`}
                    >
                      <span
                        className="w-2.5 h-2.5 rounded-full shrink-0"
                        style={{ backgroundColor: t.colors.primary }}
                      />
                      {t.name}
                    </button>
                  ))}
                </div>
              )}
            </div>
          );
        })()}
        {/* Sidebar footer — branding */}
        <div className="px-3 py-3 border-t border-border-strong/60">
          <div className="flex items-center gap-2 px-3 py-1.5 text-[14px] font-semibold text-text-base">
            <img src={graphLogo} width="16" height="16" alt="Automatic" />
            <span>Automatic</span>
            {appVersion && <span className="ml-auto text-[11px] font-normal text-text-muted">v{appVersion}</span>}
          </div>
        </div>
      </aside>

      {/* ── Main Content ────────────────────────────────────────────────── */}
      <main className="flex-1 flex flex-col min-w-0">

        {/* Update toast */}
        <UpdateToast />

        {/* Content Area */}
        <div className="flex-1 overflow-hidden flex flex-col">
          {activeTab === "getting-started" && (
            <div className="flex-1 h-full">
              <GettingStarted onNavigate={setActiveTabWithSection} />
            </div>
          )}
          {activeTab === "projects" && (
            <div className="flex-1 h-full">
              <Projects
                resetKey={projectsResetKey}
                initialProject={pendingProject}
                onInitialProjectConsumed={() => setPendingProject(null)}
                initialProjectTab={pendingProjectTab}
                onInitialProjectTabConsumed={() => setPendingProjectTab(null)}
                onNavigateToSkill={navigateToSkill}
                onNavigateToMcpServer={navigateToMcpServer}
                onNavigateToSkillStore={navigateToSkillStore}
                onNavigateToSkillStoreWithResult={navigateToSkillStoreWithResult}
                onNavigateToDiscoverMcp={navigateToDiscoverMcp}
                onNavigateToGroup={navigateToGroup}
                onNavigateToCommand={navigateToCommand}
                initialCreateWithTemplate={pendingCreateWithTemplate}
                onInitialCreateWithTemplateConsumed={() => setPendingCreateWithTemplate(null)}
                filterGroup={activeGroupFilter}
              />
            </div>
          )}
          {activeTab === "project-groups" && (
            <div className="flex-1 h-full">
              <ProjectGroups
                onNavigateToProject={navigateToProject}
                initialGroup={pendingGroup}
                onInitialGroupConsumed={() => setPendingGroup(null)}
              />
            </div>
          )}
          {activeTab === "library-home" && (
            <div className="flex-1 h-full">
              <Library onNavigate={setActiveTabWithSection} />
            </div>
          )}
          {activeTab === "project-templates" && (
            <div className="flex-1 h-full">
              <ProjectTemplates
                initialTemplate={pendingTemplate}
                onCreateProjectFromTemplate={navigateToCreateWithTemplate}
                onNavigateToProject={navigateToProject}
              />
            </div>
          )}
          {activeTab === "recommendations" && (
            <div className="flex-1 h-full">
              <Recommendations
                onNavigateToProject={navigateToProject}
                onNavigateToSkillStoreWithResult={navigateToSkillStoreWithResult}
                onNavigateToDiscoverMcp={navigateToDiscoverMcp}
                onNavigateToDiscoverTemplates={navigateToDiscoverTemplates}
                onNavigateToDiscoverCollections={navigateToDiscoverCollections}
              />
            </div>
          )}
          {activeTab === "agents" && (
            <div className="flex-1 h-full">
              <Agents onNavigateToProject={navigateToProject} />
            </div>
          )}
          {activeTab === "tools" && (
            <div className="flex-1 h-full">
              <Tools />
            </div>
          )}
          {activeTab === "skills" && (
            <div className="flex-1 h-full">
              <Skills
                initialSkill={pendingSkill}
                onInitialSkillConsumed={() => setPendingSkill(null)}
                onNavigateToProject={navigateToProject}
                onNavigateToTemplate={navigateToTemplate}
              />
            </div>
          )}
          {activeTab === "skill-store" && (
            <div className="flex-1 h-full">
              <SkillStore
                resetKey={skillStoreResetKey}
                initialSkillId={pendingSkillStoreId}
                onInitialSkillIdConsumed={() => setPendingSkillStoreId(null)}
                initialQuery={pendingSkillStoreQuery}
                onInitialQueryConsumed={() => setPendingSkillStoreQuery(null)}
                initialSkillResult={pendingSkillStoreResult}
                onInitialSkillResultConsumed={() => setPendingSkillStoreResult(null)}
              />
            </div>
          )}
          {activeTab === "discover-home" && (
            <div className="flex-1 h-full">
              <Discover onNavigate={setActiveTabWithSection} />
            </div>
          )}
          {activeTab === "discover-templates" && (
            <div className="flex-1 h-full">
              <DiscoverTemplates
                resetKey={discoverTemplatesResetKey}
                onNavigateToTemplate={navigateToTemplate}
                initialTemplateName={pendingDiscoverTemplate}
                onInitialTemplateConsumed={() => setPendingDiscoverTemplate(null)}
              />
            </div>
          )}
          {activeTab === "discover-mcp" && (
            <div className="flex-1 h-full">
              <DiscoverMcp
                resetKey={discoverMcpResetKey}
                initialSlug={pendingMcpSlug}
                onInitialSlugConsumed={() => setPendingMcpSlug(null)}
                initialQuery={pendingMcpQuery}
                onInitialQueryConsumed={() => setPendingMcpQuery(null)}
                onNavigateToMcpServer={navigateToMcpServer}
              />
            </div>
          )}
          {activeTab === "discover-collections" && (
            <div className="flex-1 h-full">
              <DiscoverCollections
                resetKey={discoverCollectionsResetKey}
                initialQuery={pendingCollectionQuery}
                onInitialQueryConsumed={() => setPendingCollectionQuery(null)}
              />
            </div>
          )}
          {activeTab === "community-featured" && (
            <div className="flex-1 h-full">
              <Featured onNavigateToTab={setActiveTabWithSection} onNavigateToSettings={navigateToSettings} />
            </div>
          )}
          {activeTab === "templates" && (
            <div className="flex-1 h-full">
              <Templates />
            </div>
          )}
          {activeTab === "rules" && (
            <div className="flex-1 h-full">
              <Rules />
            </div>
          )}
          {activeTab === "user-agents" && (
            <div className="flex-1 h-full">
              <UserAgents />
            </div>
          )}
          {activeTab === "commands" && (
            <div className="flex-1 h-full">
              <Commands
                initialCommand={pendingCommand}
                onInitialCommandConsumed={() => setPendingCommand(null)}
              />
            </div>
          )}
          {activeTab === "mcp" && (
            <div className="flex-1 h-full">
              <McpServers
                initialServer={pendingMcpServer}
                onInitialServerConsumed={() => setPendingMcpServer(null)}
              />
            </div>
          )}
          {activeTab === "settings" && (
            <div className="flex-1 h-full">
              <Settings
                onOpenWizard={() => { setWizardIsReopen(true); setShowWizard(true); }}
                initialPage={pendingSettingsPage}
                onInitialPageConsumed={() => setPendingSettingsPage(null)}
              />
            </div>
          )}
          {activeTab === "sync" && (
            <div className="flex-1 h-full">
              <CloudSync />
            </div>
          )}
        </div>
      </main>
      <TaskLog />
    </div>{/* end sidebar+main flex wrapper */}
    </div>
    </ProfileProvider>
    </TaskLogProvider>
    </UpdateProvider>
  );
}

export default App;
