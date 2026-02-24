import { useEffect, useState } from "react";
import Skills from "./Skills";
import Projects from "./Projects";
import McpServers from "./McpServers";
import Templates from "./Templates";
import Agents from "./Agents";
import { Code, Server, ChevronDown, Settings, FolderOpen, LayoutTemplate, Bot } from "lucide-react";
import "./App.css";

function App() {
  const [activeTab, setActiveTab] = useState(() => {
    const saved = localStorage.getItem("nexus.activeTab");
    // Reset to projects if saved tab was removed (activity)
    if (saved === "activity") return "projects";
    return saved || "projects";
  });

  useEffect(() => {
    localStorage.setItem("nexus.activeTab", activeTab);
  }, [activeTab]);

  const NavItem = ({ id, icon: Icon, label, count }: any) => {
    const isActive = activeTab === id;
    return (
      <button
        onClick={() => setActiveTab(id)}
        className={`w-full flex items-center gap-2.5 px-3 py-1.5 rounded-md text-[13px] font-medium transition-colors ${
          isActive 
            ? "bg-[#2D2E36] text-[#E0E1E6]" 
            : "text-[#8A8C93] hover:bg-[#2D2E36] hover:text-[#E0E1E6]"
        }`}
      >
        <Icon size={14} className={isActive ? "text-[#E0E1E6]" : "text-[#8A8C93]"} />
        <span className="flex-1 text-left">{label}</span>
        {count && (
          <span className="text-[11px] bg-[#2D2E36] text-[#8A8C93] px-1.5 rounded-sm">
            {count}
          </span>
        )}
      </button>
    );
  };

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-[#222327] text-[#e0e1e6] selection:bg-[#5E6AD2]/30">
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
            <span>Nexus</span>
          </div>
        </div>

        {/* Navigation */}
        <nav className="flex-1 overflow-y-auto py-3 px-3 custom-scrollbar">
          <div className="mb-6">
            <div className="px-3 pb-1.5 text-[11px] font-semibold text-[#8A8C93] tracking-wider flex items-center justify-between group cursor-pointer hover:text-[#E0E1E6]">
              <span>Workspace</span>
              <ChevronDown size={12} className="opacity-0 group-hover:opacity-100 transition-opacity" />
            </div>
            <ul className="space-y-0.5">
              <NavItem id="projects" icon={FolderOpen} label="Projects" />
            </ul>
          </div>

          <div className="mb-6">
            <div className="px-3 pb-1.5 text-[11px] font-semibold text-[#8A8C93] tracking-wider flex items-center justify-between group cursor-pointer hover:text-[#E0E1E6]">
              <span>Configuration</span>
              <ChevronDown size={12} className="opacity-0 group-hover:opacity-100 transition-opacity" />
            </div>
            <ul className="space-y-0.5">
              <NavItem id="agents" icon={Bot} label="Agents" />
              <NavItem id="skills" icon={Code} label="Skills & Prompts" />
              <NavItem id="templates" icon={LayoutTemplate} label="Templates" />
              <NavItem id="mcp" icon={Server} label="MCP Servers" />
            </ul>
          </div>

          <div className="mb-6">
            <div className="px-3 pb-1.5 text-[11px] font-semibold text-[#8A8C93] tracking-wider flex items-center justify-between group cursor-pointer hover:text-[#E0E1E6]">
              <span>Settings</span>
              <ChevronDown size={12} className="opacity-0 group-hover:opacity-100 transition-opacity" />
            </div>
            <ul className="space-y-0.5">
              <NavItem id="settings" icon={Settings} label="Preferences" />
            </ul>
          </div>
        </nav>
      </aside>

      {/* Main Content */}
      <main className="flex-1 flex flex-col min-w-0">
        {/* Top Header */}
        <header className="h-11 border-b border-[#33353A] flex items-center px-4 bg-[#222327]">
          <span className="text-[13px] text-[#E0E1E6] capitalize">{activeTab.replace('-', ' ')}</span>
        </header>

        {/* Content Area */}
        <div className="flex-1 overflow-hidden flex flex-col">
          {activeTab === "projects" && (
            <div className="flex-1 h-full">
              <Projects />
            </div>
          )}
          {activeTab === "agents" && (
            <div className="flex-1 h-full">
              <Agents />
            </div>
          )}
          {activeTab === "skills" && (
            <div className="flex-1 h-full">
              <Skills />
            </div>
          )}
          {activeTab === "templates" && (
            <div className="flex-1 h-full">
              <Templates />
            </div>
          )}
          {activeTab === "mcp" && (
            <div className="flex-1 h-full">
              <McpServers />
            </div>
          )}
          {activeTab === "settings" && (
             <div className="flex-1 p-8 bg-[#222327]">
                <h3 className="text-sm font-medium mb-4">Preferences</h3>
                <div className="text-[13px] text-[#8A8C93]">Application settings will go here.</div>
             </div>
          )}
        </div>
      </main>
    </div>
  );
}

export default App;
