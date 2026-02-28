import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ProfileProvider } from "./ProfileContext";
import { useCurrentUser } from "./ProfileContext";
import { initAnalytics, setAnalyticsEnabled, trackNavigation } from "./analytics";
import Dashboard from "./Dashboard";
import Skills from "./Skills";
import SkillStore from "./SkillStore";
import Projects from "./Projects";
import ProjectTemplates from "./ProjectTemplates";
import McpServers from "./McpServers";
import Templates from "./Templates";
import Rules from "./Rules";
import Agents from "./Agents";
import Settings from "./Settings";
import TemplateMarketplace from "./TemplateMarketplace";
import McpMarketplace from "./McpMarketplace";
import TechMeshBackground from "./TechMeshBackground";
import FirstRunWizard from "./FirstRunWizard";
import { Code, Server, ChevronDown, FolderOpen, LayoutTemplate, Bot, Layers, Store, Settings as SettingsIcon, ScrollText, Sparkles } from "lucide-react";
import graphLogo from "../logos/graph_5.svg";
import "./App.css";

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
  const [activeTab, setActiveTab] = useState(() => {
    // Migrate legacy "nexus." localStorage keys to "automatic." prefix
    const legacy = localStorage.getItem("nexus.activeTab");
    if (legacy) {
      localStorage.setItem("automatic.activeTab", legacy);
      localStorage.removeItem("nexus.activeTab");
    }
    const saved = localStorage.getItem("automatic.activeTab") || legacy;
    // Reset to projects if saved tab was removed (activity)
    if (saved === "activity") return "dashboard";
    return saved || "dashboard";
  });
  const [pendingProject, setPendingProject] = useState<string | null>(null);
  const [skillStoreResetKey, setSkillStoreResetKey] = useState(0);
  const [templateMarketplaceResetKey, setTemplateMarketplaceResetKey] = useState(0);
  const [mcpMarketplaceResetKey, setMcpMarketplaceResetKey] = useState(0);

  // ── First-run wizard ────────────────────────────────────────────────────
  // null = still loading, true = must show, false = already completed
  const [showWizard, setShowWizard] = useState<boolean | null>(null);

  useEffect(() => {
    async function checkWizard() {
      try {
        const settings: any = await invoke("read_settings");
        setShowWizard(!(settings?.wizard_completed ?? false));
      } catch {
        // If we can't read settings, show the wizard to be safe.
        setShowWizard(true);
      }
    }
    checkWizard();
  }, []);

  const handleWizardComplete = (answers: { analyticsEnabled: boolean }) => {
    // Apply analytics preference immediately so the runtime flag is in sync.
    setAnalyticsEnabled(answers.analyticsEnabled);
    setShowWizard(false);
  };

  useEffect(() => {
    localStorage.setItem("automatic.activeTab", activeTab);
  }, [activeTab]);

  const navigateToProject = (projectName: string) => {
    setPendingProject(projectName);
    setActiveTab("projects");
  };

  const MARKETPLACE_TABS: Record<string, () => void> = {
    "skill-store": () => setSkillStoreResetKey((k) => k + 1),
    "template-marketplace": () => setTemplateMarketplaceResetKey((k) => k + 1),
    "mcp-marketplace": () => setMcpMarketplaceResetKey((k) => k + 1),
  };

  const handleTabClick = (id: string) => {
    if (activeTab === id && MARKETPLACE_TABS[id]) {
      MARKETPLACE_TABS[id]!();
    }
    setActiveTab(id);
    trackNavigation(id);
  };

  const NavItem = ({ id, icon: Icon, label, count }: any) => {
    const isActive = activeTab === id;
    return (
      <button
        onClick={() => handleTabClick(id)}
        className={`w-full flex items-center gap-2.5 px-3 py-1.5 rounded-md text-[13px] font-medium transition-colors ${
          isActive 
            ? "bg-[#2D2E36] text-[#F8F8FA]" 
            : "text-[#C8CAD0] hover:bg-[#2D2E36] hover:text-[#F8F8FA]"
        }`}
      >
        <Icon size={14} className={isActive ? "text-[#F8F8FA]" : "text-[#C8CAD0]"} />
        <span className="flex-1 text-left">{label}</span>
        {count && (
          <span className="text-[11px] bg-[#2D2E36] text-[#C8CAD0] px-1.5 rounded-sm">
            {count}
          </span>
        )}
      </button>
    );
  };

  return (
    <ProfileProvider>
    <AnalyticsBootstrap />
    {/* First-run wizard — rendered as a full-screen overlay; main UI is
        mounted but hidden so that tabs retain their state after completion. */}
    {showWizard === true && (
      <FirstRunWizard onComplete={handleWizardComplete} />
    )}
    <div
      className="relative flex h-screen w-screen overflow-hidden bg-[#222327] text-[#fafafa] selection:bg-[#5E6AD2]/30"
      aria-hidden={showWizard === true}
    >
      {/* Sidebar */}
      <aside className="w-[180px] flex-shrink-0 bg-[#1A1A1E] border-r border-[#33353A] flex flex-col">
        {/* Workspace Header — drag region; left padding clears macOS traffic lights */}
        <div
          data-tauri-drag-region
          className="h-11 border-b border-[#33353A]/50 select-none"
        />

        {/* Navigation */}
        <nav className="flex-1 overflow-y-auto py-3 px-3 custom-scrollbar">
          <div className="mb-6">
            <div className="px-3 pb-1.5 text-[11px] font-semibold text-[#C8CAD0] tracking-wider flex items-center justify-between group cursor-pointer hover:text-[#F8F8FA]">
              <span>Workspace</span>
              <ChevronDown size={12} className="opacity-0 group-hover:opacity-100 transition-opacity" />
            </div>
            <ul className="space-y-0.5">
              <NavItem id="dashboard" icon={LayoutTemplate} label="Dashboard" />
              <NavItem id="projects" icon={FolderOpen} label="Projects" />
            </ul>
          </div>

          <div className="mb-6">
            <div className="px-3 pb-1.5 text-[11px] font-semibold text-[#C8CAD0] tracking-wider flex items-center justify-between group cursor-pointer hover:text-[#F8F8FA]">
              <span>Configuration</span>
              <ChevronDown size={12} className="opacity-0 group-hover:opacity-100 transition-opacity" />
            </div>
            <ul className="space-y-0.5">
              <NavItem id="agents" icon={Bot} label="Agents" />
              <NavItem id="project-templates" icon={Layers} label="Proj Templates" />
              <NavItem id="templates" icon={LayoutTemplate} label="File Templates" />
              <NavItem id="rules" icon={ScrollText} label="Rules" />
              <NavItem id="skills" icon={Code} label="Skills" />
              <NavItem id="mcp" icon={Server} label="MCP Servers" />
              <NavItem id="settings" icon={SettingsIcon} label="Settings" />
            </ul>
          </div>

          <div className="mb-6">
            <div className="px-3 pb-1.5 text-[11px] font-semibold text-[#C8CAD0] tracking-wider flex items-center justify-between group cursor-pointer hover:text-[#F8F8FA]">
              <span>Marketplace</span>
              <ChevronDown size={12} className="opacity-0 group-hover:opacity-100 transition-opacity" />
            </div>
            <ul className="space-y-0.5">
              <NavItem id="template-marketplace" icon={Layers} label="Templates" />
              <NavItem id="skill-store" icon={Store} label="Skills.sh" />
              <NavItem id="mcp-marketplace" icon={Server} label="MCP Servers" />
            </ul>
          </div>

        </nav>

        {/* Sidebar footer — setup wizard */}
        <div className="px-3 pt-3 pb-1">
          <button
            onClick={() => setShowWizard(true)}
            className="w-full flex items-center gap-2.5 px-3 py-1.5 rounded-md text-[13px] font-medium text-[#C8CAD0] hover:bg-[#2D2E36] hover:text-[#F8F8FA] transition-colors"
          >
            <Sparkles size={14} className="text-[#C8CAD0]" />
            <span className="flex-1 text-left">Setup wizard</span>
          </button>
        </div>
        {/* Sidebar footer — branding */}
        <div className="px-3 py-3 border-t border-[#33353A]/60">
          <div className="flex items-center gap-2 px-3 py-1.5 text-[14px] font-semibold text-white">
            <img src={graphLogo} width="16" height="16" alt="Automatic" />
            <span>Automatic</span>
          </div>
        </div>
      </aside>

      {/* Main Content */}
      <main className="flex-1 flex flex-col min-w-0">
        {/* Top Header — drag region, title centered, actions right */}
        <header
          data-tauri-drag-region
          className="h-11 border-b border-[#33353A] flex items-center bg-[#222327] select-none relative"
        >
          {/* Center: page title */}
          <span
            data-tauri-drag-region
            className="absolute inset-0 flex items-center justify-center text-[13px] font-medium text-[#C8CAD0] pointer-events-none capitalize"
          >
            {activeTab.replace(/-/g, ' ')}
          </span>

          {/* Right: contextual actions */}
          <div className="ml-auto pr-4 flex items-center gap-2 relative z-10">
            {activeTab === "skills" && (
              <button
                onClick={() => setActiveTab("skill-store")}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[12px] font-medium bg-[#5E6AD2] hover:bg-[#6B78E3] text-white shadow-sm transition-colors"
              >
                <Store size={13} />
                Skill Store
              </button>
            )}
            {activeTab === "project-templates" && (
              <button
                onClick={() => setActiveTab("template-marketplace")}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[12px] font-medium bg-[#5E6AD2] hover:bg-[#6B78E3] text-white shadow-sm transition-colors"
              >
                <Store size={13} />
                Template Marketplace
              </button>
            )}
            {activeTab === "mcp" && (
              <button
                onClick={() => setActiveTab("mcp-marketplace")}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[12px] font-medium bg-[#5E6AD2] hover:bg-[#6B78E3] text-white shadow-sm transition-colors"
              >
                <Store size={13} />
                MCP Marketplace
              </button>
            )}
          </div>
        </header>

        {/* Content Area */}
        <div className="flex-1 overflow-hidden flex flex-col">
          {activeTab === "dashboard" && (
            <div className="relative flex-1 h-full">
              <TechMeshBackground />
              <Dashboard onNavigate={setActiveTab} />
            </div>
          )}
          {activeTab === "projects" && (
            <div className="flex-1 h-full">
              <Projects initialProject={pendingProject} onInitialProjectConsumed={() => setPendingProject(null)} />
            </div>
          )}
          {activeTab === "project-templates" && (
            <div className="flex-1 h-full">
              <ProjectTemplates />
            </div>
          )}
          {activeTab === "agents" && (
            <div className="flex-1 h-full">
              <Agents onNavigateToProject={navigateToProject} />
            </div>
          )}
          {activeTab === "skills" && (
            <div className="flex-1 h-full">
              <Skills />
            </div>
          )}
          {activeTab === "skill-store" && (
            <div className="flex-1 h-full">
              <SkillStore resetKey={skillStoreResetKey} />
            </div>
          )}
          {activeTab === "template-marketplace" && (
            <div className="flex-1 h-full">
              <TemplateMarketplace resetKey={templateMarketplaceResetKey} />
            </div>
          )}
          {activeTab === "mcp-marketplace" && (
            <div className="flex-1 h-full">
              <McpMarketplace resetKey={mcpMarketplaceResetKey} />
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
          {activeTab === "mcp" && (
            <div className="flex-1 h-full">
              <McpServers />
            </div>
          )}
          {activeTab === "settings" && (
            <div className="flex-1 h-full">
              <Settings />
            </div>
          )}
        </div>
      </main>
    </div>
    </ProfileProvider>
  );
}

export default App;
