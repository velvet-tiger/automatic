import { useEffect, useState } from "react";
import { SignIn, SignedIn, SignedOut, UserButton } from "@clerk/clerk-react";
import { ProfileProvider } from "./ProfileContext";
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
import { Code, Server, ChevronDown, FolderOpen, LayoutTemplate, Bot, Layers, Store, Settings as SettingsIcon, ScrollText } from "lucide-react";
import "./App.css";

function App() {
  const [activeTab, setActiveTab] = useState(() => {
    const saved = localStorage.getItem("nexus.activeTab");
    // Reset to projects if saved tab was removed (activity)
    if (saved === "activity") return "dashboard";
    return saved || "dashboard";
  });
  const [pendingProject, setPendingProject] = useState<string | null>(null);
  const [skillStoreResetKey, setSkillStoreResetKey] = useState(0);
  const [templateMarketplaceResetKey, setTemplateMarketplaceResetKey] = useState(0);
  const [mcpMarketplaceResetKey, setMcpMarketplaceResetKey] = useState(0);

  useEffect(() => {
    localStorage.setItem("nexus.activeTab", activeTab);
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
    <div className="relative flex h-screen w-screen overflow-hidden bg-[#222327] text-[#fafafa] selection:bg-[#5E6AD2]/30">
      {/* Sign-in overlay when not authenticated */}
      <SignedOut>
        <div className="absolute inset-0 z-50 flex items-center justify-center bg-[#222327]/80 backdrop-blur-sm">
          <SignIn
            routing="hash"
            appearance={{
              elements: {
                rootBox: "mx-auto",
                card: "bg-[#1A1A1E] border border-[#33353A] shadow-2xl",
              },
            }}
          />
        </div>
      </SignedOut>

      {/* Sidebar */}
      <aside className="w-[180px] flex-shrink-0 bg-[#1A1A1E] border-r border-[#33353A] flex flex-col">
        {/* Workspace Header */}
        <div className="h-12 px-4 flex items-center border-b border-[#33353A]/50">
          <div className="flex items-center gap-2 text-sm font-semibold">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" className="text-white">
              <circle cx="12" cy="12" r="2" fill="currentColor"/>
              <circle cx="12" cy="4" r="2" fill="currentColor"/>
              <circle cx="19" cy="8" r="2" fill="currentColor"/>
              <circle cx="19" cy="16" r="2" fill="currentColor"/>
              <circle cx="12" cy="20" r="2" fill="currentColor"/>
              <circle cx="5" cy="16" r="2" fill="currentColor"/>
              <circle cx="5" cy="8" r="2" fill="currentColor"/>
              <line x1="12" y1="10" x2="12" y2="6" stroke="currentColor" strokeWidth="2"/>
              <line x1="13.5" y1="13" x2="17.5" y2="15" stroke="currentColor" strokeWidth="2"/>
              <line x1="13.5" y1="11" x2="17.5" y2="9" stroke="currentColor" strokeWidth="2"/>
              <line x1="12" y1="14" x2="12" y2="18" stroke="currentColor" strokeWidth="2"/>
              <line x1="10.5" y1="13" x2="6.5" y2="15" stroke="currentColor" strokeWidth="2"/>
              <line x1="10.5" y1="11" x2="6.5" y2="9" stroke="currentColor" strokeWidth="2"/>
            </svg>
            <span>Automatic</span>
          </div>
        </div>

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
              <NavItem id="skills" icon={Code} label="Skills" />
              <NavItem id="project-templates" icon={Layers} label="Proj Templates" />
              <NavItem id="templates" icon={LayoutTemplate} label="File Templates" />
              <NavItem id="rules" icon={ScrollText} label="Rules" />
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

        {/* User account at bottom of sidebar */}
        <SignedIn>
          <div className="border-t border-[#33353A] px-3 py-3 flex items-center gap-2.5">
            <UserButton
              appearance={{
                elements: {
                  avatarBox: "w-7 h-7",
                  userButtonPopoverCard: "bg-[#1A1A1E] border border-[#33353A]",
                  userButtonBox: "flex-row-reverse",
                  userButtonOuterIdentifier: "text-left",
                },
              }}
              showName
            />
          </div>
        </SignedIn>
      </aside>

      {/* Main Content */}
      <main className="flex-1 flex flex-col min-w-0">
        {/* Top Header */}
        <header className="h-11 border-b border-[#33353A] flex items-center justify-between px-4 bg-[#222327]">
          <span className="text-[13px] text-[#F8F8FA] capitalize">{activeTab.replace(/-/g, ' ')}</span>
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
        </header>

        {/* Content Area */}
        <div className="flex-1 overflow-hidden flex flex-col">
          {activeTab === "dashboard" && (
            <div className="flex-1 h-full">
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
