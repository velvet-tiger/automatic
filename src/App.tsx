import { useState } from "react";
import Providers from "./Providers";
import Skills from "./Skills";
import Projects from "./Projects";
import { Bot, Key, Code, Server, Search, Edit, ChevronDown, PlayCircle, Settings, Command, Activity, FolderOpen } from "lucide-react";
import "./App.css";

function App() {
  const [activeTab, setActiveTab] = useState("agents");

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
      <aside className="w-[240px] flex-shrink-0 bg-[#1A1A1E] border-r border-[#33353A] flex flex-col">
        {/* Workspace Header */}
        <div className="h-12 px-4 flex items-center justify-between border-b border-[#33353A]/50">
          <div className="flex items-center gap-2 text-sm font-semibold hover:bg-[#2D2E36] px-2 py-1 -ml-2 rounded-md cursor-pointer transition-colors">
            <div className="w-5 h-5 bg-gradient-to-br from-[#5E6AD2] to-[#8C98F2] rounded flex items-center justify-center text-white text-xs">
              A
            </div>
            <span>Nexus</span>
            <ChevronDown size={14} className="text-[#8A8C93]" />
          </div>
          <div className="flex gap-1 text-[#8A8C93]">
            <button className="p-1 hover:bg-[#2D2E36] rounded-md transition-colors"><Search size={14} /></button>
            <button className="p-1 hover:bg-[#2D2E36] rounded-md transition-colors"><Edit size={14} /></button>
          </div>
        </div>

        {/* Navigation */}
        <nav className="flex-1 overflow-y-auto py-3 px-3 custom-scrollbar">
          <div className="mb-6">
            <div className="px-3 pb-1.5 text-[11px] font-semibold text-[#8A8C93] tracking-wider flex items-center justify-between group cursor-pointer hover:text-[#E0E1E6]">
              <span>Orchestration</span>
              <ChevronDown size={12} className="opacity-0 group-hover:opacity-100 transition-opacity" />
            </div>
            <ul className="space-y-0.5">
              <NavItem id="agents" icon={Bot} label="Connected Agents" count="0" />
              <NavItem id="activity" icon={Activity} label="Activity Log" />
            </ul>
          </div>

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
              <NavItem id="providers" icon={Key} label="LLM Providers" />
              <NavItem id="skills" icon={Code} label="Skills & Prompts" />
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
        <header className="h-11 border-b border-[#33353A] flex items-center justify-between px-4 bg-[#222327]">
          <div className="flex items-center gap-2 text-[13px] text-[#8A8C93]">
            <Command size={14} />
            <span>Nexus</span>
            <span className="text-[#33353A]">›</span>
            <span className="text-[#E0E1E6] capitalize">{activeTab.replace('-', ' ')}</span>
          </div>
          <div className="flex items-center gap-3 text-[#8A8C93] text-[13px]">
            <button className="hover:text-[#E0E1E6] transition-colors flex items-center gap-1">
              <Settings size={14} /> View Options
            </button>
          </div>
        </header>

        {/* Content Area */}
        <div className="flex-1 overflow-hidden flex flex-col">
          {activeTab === "agents" && (
            <div className="flex-1 flex items-center justify-center p-8 text-center bg-[#222327]">
              <div className="max-w-md w-full">
                <div className="w-16 h-16 mx-auto mb-6 rounded-full border border-dashed border-[#44474F] flex items-center justify-center text-[#8A8C93]">
                  <PlayCircle size={24} strokeWidth={1.5} />
                </div>
                <h2 className="text-lg font-medium text-[#E0E1E6] mb-2">No Connected Agents</h2>
                <p className="text-[14px] text-[#8A8C93] mb-8 leading-relaxed">
                  External agents (Claude Code, OpenCode, custom scripts) connect to Nexus 
                  to pull credentials, skills, and MCP configs. Connected agents will appear here.
                </p>
                <div className="flex items-center justify-center gap-3">
                  <button className="px-4 py-2 bg-[#2D2E36] hover:bg-[#33353A] text-[#E0E1E6] text-[13px] font-medium rounded border border-[#3A3B42] transition-colors">
                    Documentation
                  </button>
                </div>
              </div>
            </div>
          )}
          {activeTab === "activity" && (
             <div className="flex-1 p-8 bg-[#222327]">
                <h3 className="text-sm font-medium mb-4">Activity Log</h3>
                <div className="text-[13px] text-[#8A8C93]">No recent activity.</div>
             </div>
          )}
          {activeTab === "projects" && (
            <div className="flex-1 h-full">
              <Projects />
            </div>
          )}
          {activeTab === "providers" && (
            <div className="flex-1 overflow-auto bg-[#222327] p-8">
              <div className="max-w-2xl mx-auto">
                <Providers />
              </div>
            </div>
          )}
          {activeTab === "skills" && (
            <div className="flex-1 h-full">
              <Skills />
            </div>
          )}
          {activeTab === "mcp" && (
            <div className="flex-1 flex items-center justify-center p-8 text-center bg-[#222327]">
               <div className="max-w-md w-full">
                 <div className="w-16 h-16 mx-auto mb-6 rounded-full border border-dashed border-[#44474F] flex items-center justify-center text-[#8A8C93]">
                   <Server size={24} strokeWidth={1.5} />
                 </div>
                 <h2 className="text-lg font-medium text-[#E0E1E6] mb-2">MCP Servers Configuration</h2>
                 <p className="text-[14px] text-[#8A8C93] mb-8 leading-relaxed">
                   Connect local Model Context Protocol servers to give your agents access to your filesystem, 
                   databases, and internal developer tools.
                 </p>
                 <div className="flex items-center justify-center gap-3">
                   <button className="px-4 py-2 bg-[#5E6AD2] hover:bg-[#6B78E3] text-white text-[13px] font-medium rounded shadow-sm transition-colors">
                     Add MCP Server
                   </button>
                 </div>
               </div>
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
