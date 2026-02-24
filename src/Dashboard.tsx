import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { 
  FolderOpen, 
  Code, 
  Server, 
  Bot, 
  RefreshCw,
  AlertCircle,
  ArrowRight
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
}

interface DashboardProps {
  onNavigate: (tab: string) => void;
}

export default function Dashboard({ onNavigate }: DashboardProps) {
  const [projects, setProjects] = useState<Project[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    loadData();
  }, []);

  const loadData = async () => {
    setLoading(true);
    try {
      // Get project names
      const names: string[] = await invoke("get_projects");
      
      // Load details for each project
      const projectDetails = await Promise.all(
        names.map(async (name) => {
          try {
            const raw: string = await invoke("read_project", { name });
            return JSON.parse(raw) as Project;
          } catch (e) {
            console.error(`Failed to load project ${name}:`, e);
            return null;
          }
        })
      );
      
      setProjects(projectDetails.filter(Boolean) as Project[]);
      setError(null);
    } catch (err: any) {
      setError(`Failed to load dashboard data: ${err}`);
    } finally {
      setLoading(false);
    }
  };

  const handleProjectClick = (name: string) => {
    localStorage.setItem("nexus.projects.selected", name);
    onNavigate("projects");
  };

  if (loading) {
    return (
      <div className="flex-1 flex items-center justify-center h-full bg-[#222327]">
        <div className="flex items-center gap-2 text-[#8A8C93]">
          <RefreshCw size={16} className="animate-spin" />
          <span>Loading dashboard...</span>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto p-8 custom-scrollbar bg-[#222327]">
      <div className="max-w-5xl mx-auto space-y-8">
        
        {/* Header */}
        <div>
          <h1 className="text-2xl font-semibold text-[#E0E1E6] mb-2">Welcome to Automatic</h1>
          <p className="text-[#8A8C93] text-sm">Manage your AI agent configurations and projects</p>
        </div>

        {error && (
          <div className="bg-red-500/10 text-red-400 p-4 rounded-md border border-red-500/20 flex items-start gap-3">
            <AlertCircle size={16} className="mt-0.5 shrink-0" />
            <div className="text-sm">{error}</div>
          </div>
        )}

        {/* Quick Stats */}
        <div className="grid grid-cols-4 gap-4">
          <div className="bg-[#1A1A1E] border border-[#33353A] rounded-lg p-5 flex flex-col items-center justify-center text-center cursor-pointer hover:border-[#44474F] transition-colors" onClick={() => onNavigate("projects")}>
            <FolderOpen size={24} className="text-[#3B82F6] mb-2" />
            <div className="text-2xl font-semibold text-[#E0E1E6] mb-1">{projects.length}</div>
            <div className="text-xs text-[#8A8C93] uppercase tracking-wider font-medium">Projects</div>
          </div>
          <div className="bg-[#1A1A1E] border border-[#33353A] rounded-lg p-5 flex flex-col items-center justify-center text-center cursor-pointer hover:border-[#44474F] transition-colors" onClick={() => onNavigate("agents")}>
            <Bot size={24} className="text-[#818CF8] mb-2" />
            <div className="text-2xl font-semibold text-[#E0E1E6] mb-1">
               {new Set(projects.flatMap(p => p.agents)).size}
            </div>
            <div className="text-xs text-[#8A8C93] uppercase tracking-wider font-medium">Active Agents</div>
          </div>
          <div className="bg-[#1A1A1E] border border-[#33353A] rounded-lg p-5 flex flex-col items-center justify-center text-center cursor-pointer hover:border-[#44474F] transition-colors" onClick={() => onNavigate("skills")}>
            <Code size={24} className="text-[#4ADE80] mb-2" />
            <div className="text-2xl font-semibold text-[#E0E1E6] mb-1">
               {new Set(projects.flatMap(p => p.skills)).size}
            </div>
            <div className="text-xs text-[#8A8C93] uppercase tracking-wider font-medium">Skills In Use</div>
          </div>
          <div className="bg-[#1A1A1E] border border-[#33353A] rounded-lg p-5 flex flex-col items-center justify-center text-center cursor-pointer hover:border-[#44474F] transition-colors" onClick={() => onNavigate("mcp")}>
            <Server size={24} className="text-[#F59E0B] mb-2" />
            <div className="text-2xl font-semibold text-[#E0E1E6] mb-1">
               {new Set(projects.flatMap(p => p.mcp_servers)).size}
            </div>
            <div className="text-xs text-[#8A8C93] uppercase tracking-wider font-medium">MCP Servers</div>
          </div>
        </div>

        {/* Projects List */}
        <div>
          <div className="flex items-center justify-between mb-4">
            <h2 className="text-lg font-medium text-[#E0E1E6]">Recent Projects</h2>
            <button 
              onClick={() => onNavigate("projects")}
              className="text-sm text-[#3B82F6] hover:text-[#60A5FA] flex items-center gap-1 transition-colors"
            >
              View all <ArrowRight size={14} />
            </button>
          </div>

          {projects.length === 0 ? (
            <div className="bg-[#1A1A1E] border border-[#33353A] rounded-lg p-8 text-center">
              <FolderOpen size={32} className="text-[#8A8C93] mx-auto mb-3 opacity-50" />
              <h3 className="text-[#E0E1E6] font-medium mb-1">No projects yet</h3>
              <p className="text-[#8A8C93] text-sm mb-4">Create your first project to start managing agent configurations.</p>
              <button 
                onClick={() => onNavigate("projects")}
                className="px-4 py-2 bg-[#3B82F6] hover:bg-[#60A5FA] text-white rounded text-sm font-medium transition-colors"
              >
                Go to Projects
              </button>
            </div>
          ) : (
            <div className="grid grid-cols-2 gap-4">
              {projects.sort((a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime()).slice(0, 6).map(project => (
                <div 
                  key={project.name}
                  onClick={() => handleProjectClick(project.name)}
                  className="bg-[#1A1A1E] border border-[#33353A] hover:border-[#3B82F6]/50 rounded-lg p-5 cursor-pointer transition-all group"
                >
                  <div className="flex items-start justify-between mb-3">
                    <div className="flex items-center gap-2">
                      <FolderOpen size={16} className="text-[#3B82F6]" />
                      <h3 className="font-medium text-[#E0E1E6] group-hover:text-[#3B82F6] transition-colors">{project.name}</h3>
                    </div>
                    {project.directory && project.agents.length > 0 && (
                      <span className="text-[10px] bg-[#4ADE80]/10 text-[#4ADE80] px-1.5 py-0.5 rounded uppercase tracking-wider font-semibold">Configured</span>
                    )}
                  </div>
                  
                  {project.description && (
                    <p className="text-sm text-[#8A8C93] mb-4 line-clamp-2">{project.description}</p>
                  )}
                  
                  <div className="flex items-center gap-4 mt-auto pt-2 border-t border-[#33353A]/50">
                    <div className="flex items-center gap-1.5 text-xs text-[#8A8C93]">
                      <Bot size={12} />
                      <span>{project.agents.length}</span>
                    </div>
                    <div className="flex items-center gap-1.5 text-xs text-[#8A8C93]">
                      <Code size={12} />
                      <span>{project.skills.length + project.local_skills.length}</span>
                    </div>
                    <div className="flex items-center gap-1.5 text-xs text-[#8A8C93]">
                      <Server size={12} />
                      <span>{project.mcp_servers.length}</span>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
