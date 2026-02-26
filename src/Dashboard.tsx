import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { 
  FolderOpen, 
  Code, 
  Server, 
  Bot, 
  RefreshCw,
  AlertCircle,
  ArrowRight,
  CheckCircle2,
  Layers,
  Sparkles,
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

interface DriftReport {
  drifted: boolean;
  agents: {
    agent_id: string;
    agent_label: string;
    files: { path: string; reason: string }[];
  }[];
}

interface DashboardProps {
  onNavigate: (tab: string) => void;
}

export default function Dashboard({ onNavigate }: DashboardProps) {
  const [projects, setProjects] = useState<Project[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  // Map of project name → drift report (undefined = not yet checked, null = not applicable)
  const [driftMap, setDriftMap] = useState<Record<string, DriftReport | null>>({});

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
      
      const loaded = projectDetails.filter(Boolean) as Project[];
      setProjects(loaded);
      setError(null);

      // Check drift for projects that have a directory and at least one agent configured
      const driftResults: Record<string, DriftReport | null> = {};
      await Promise.all(
        loaded.map(async (p) => {
          if (!p.directory || p.agents.length === 0) {
            driftResults[p.name] = null; // not applicable
            return;
          }
          try {
            const raw: string = await invoke("check_project_drift", { name: p.name });
            driftResults[p.name] = JSON.parse(raw) as DriftReport;
          } catch {
            driftResults[p.name] = null;
          }
        })
      );
      setDriftMap(driftResults);
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

  const driftedCount = Object.values(driftMap).filter((r) => r?.drifted).length;

  if (loading) {
    return (
      <div className="flex-1 flex items-center justify-center h-full bg-[#222327]">
        <div className="flex items-center gap-2 text-[#C8CAD0]">
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
          <h1 className="text-2xl font-semibold text-[#F8F8FA] mb-2">Welcome to Automatic</h1>
          <p className="text-[#C8CAD0] text-sm">Manage your AI agent configurations and projects</p>
        </div>

        {error && (
          <div className="bg-red-500/10 text-red-400 p-4 rounded-md border border-red-500/20 flex items-start gap-3">
            <AlertCircle size={16} className="mt-0.5 shrink-0" />
            <div className="text-sm">{error}</div>
          </div>
        )}

        {/* Drift alert banner — shown when at least one project has drifted */}
        {driftedCount > 0 && (
          <div className="bg-[#F59E0B]/10 border border-[#F59E0B]/30 rounded-lg px-5 py-3 flex items-center justify-between">
            <div className="flex items-center gap-2.5 text-[#F59E0B]">
              <AlertCircle size={15} className="shrink-0" />
              <span className="text-[13px] font-medium">
                {driftedCount === 1
                  ? "1 project has drifted — agent config files are out of sync."
                  : `${driftedCount} projects have drifted — agent config files are out of sync.`}
              </span>
            </div>
            <button
              onClick={() => onNavigate("projects")}
              className="text-[12px] font-medium text-[#F59E0B] hover:text-[#FBB60D] underline decoration-[#F59E0B]/40 hover:decoration-[#FBB60D] transition-colors shrink-0 ml-4"
            >
              Review in Projects
            </button>
          </div>
        )}

        {/* Getting Started — shown only when there are no projects */}
        {projects.length === 0 && (
          <div>
            <div className="flex items-center gap-2.5 mb-4">
              <Sparkles size={16} className="text-[#5E6AD2]" />
              <h2 className="text-lg font-medium text-[#F8F8FA]">Getting started</h2>
            </div>
            <div className="grid grid-cols-2 gap-4">
              {/* Step 1 */}
              <button
                onClick={() => onNavigate("projects")}
                className="bg-[#1A1A1E] border border-[#33353A] rounded-lg p-5 text-left hover:border-[#3B82F6]/50 transition-all group flex flex-col gap-3"
              >
                <div className="flex items-center gap-3">
                  <div className="w-7 h-7 rounded-full bg-[#3B82F6]/10 border border-[#3B82F6]/20 flex items-center justify-center shrink-0">
                    <span className="text-[11px] font-semibold text-[#3B82F6]">1</span>
                  </div>
                  <h3 className="font-medium text-[#F8F8FA] group-hover:text-[#3B82F6] transition-colors">Create a project</h3>
                </div>
                <p className="text-[13px] text-[#C8CAD0] leading-relaxed pl-10">
                  Link a directory to an agent configuration so your tools are always in sync.
                </p>
                <div className="flex items-center gap-1 text-[12px] font-medium text-[#3B82F6] pl-10 group-hover:gap-2 transition-all">
                  Go to Projects <ArrowRight size={13} />
                </div>
              </button>

              {/* Step 2 */}
              <button
                onClick={() => onNavigate("project-templates")}
                className="bg-[#1A1A1E] border border-[#33353A] rounded-lg p-5 text-left hover:border-[#818CF8]/50 transition-all group flex flex-col gap-3"
              >
                <div className="flex items-center gap-3">
                  <div className="w-7 h-7 rounded-full bg-[#818CF8]/10 border border-[#818CF8]/20 flex items-center justify-center shrink-0">
                    <span className="text-[11px] font-semibold text-[#818CF8]">2</span>
                  </div>
                  <h3 className="font-medium text-[#F8F8FA] group-hover:text-[#818CF8] transition-colors">Browse project templates</h3>
                </div>
                <p className="text-[13px] text-[#C8CAD0] leading-relaxed pl-10">
                  Start from a pre-built project template to hit the ground running with a proven setup.
                </p>
                <div className="flex items-center gap-1 text-[12px] font-medium text-[#818CF8] pl-10 group-hover:gap-2 transition-all">
                  Browse Templates <ArrowRight size={13} />
                </div>
              </button>

              {/* Step 3 */}
              <button
                onClick={() => onNavigate("skill-store")}
                className="bg-[#1A1A1E] border border-[#33353A] rounded-lg p-5 text-left hover:border-[#4ADE80]/50 transition-all group flex flex-col gap-3"
              >
                <div className="flex items-center gap-3">
                  <div className="w-7 h-7 rounded-full bg-[#4ADE80]/10 border border-[#4ADE80]/20 flex items-center justify-center shrink-0">
                    <span className="text-[11px] font-semibold text-[#4ADE80]">3</span>
                  </div>
                  <h3 className="font-medium text-[#F8F8FA] group-hover:text-[#4ADE80] transition-colors">Install skills</h3>
                </div>
                <p className="text-[13px] text-[#C8CAD0] leading-relaxed pl-10">
                  Browse the community skill store and load specialised capabilities into your agents.
                </p>
                <div className="flex items-center gap-1 text-[12px] font-medium text-[#4ADE80] pl-10 group-hover:gap-2 transition-all">
                  Browse Skills <ArrowRight size={13} />
                </div>
              </button>

              {/* Step 4 */}
              <button
                onClick={() => onNavigate("mcp-marketplace")}
                className="bg-[#1A1A1E] border border-[#33353A] rounded-lg p-5 text-left hover:border-[#F59E0B]/50 transition-all group flex flex-col gap-3"
              >
                <div className="flex items-center gap-3">
                  <div className="w-7 h-7 rounded-full bg-[#F59E0B]/10 border border-[#F59E0B]/20 flex items-center justify-center shrink-0">
                    <span className="text-[11px] font-semibold text-[#F59E0B]">4</span>
                  </div>
                  <h3 className="font-medium text-[#F8F8FA] group-hover:text-[#F59E0B] transition-colors">Connect MCP servers</h3>
                </div>
                <p className="text-[13px] text-[#C8CAD0] leading-relaxed pl-10">
                  Extend your agents with powerful integrations from the MCP server marketplace.
                </p>
                <div className="flex items-center gap-1 text-[12px] font-medium text-[#F59E0B] pl-10 group-hover:gap-2 transition-all">
                  Browse Servers <ArrowRight size={13} />
                </div>
              </button>
            </div>
          </div>
        )}

        {/* Recent Projects — shown only when projects exist */}
        {projects.length > 0 && (
          <div>
            <div className="flex items-center justify-between mb-4">
              <h2 className="text-lg font-medium text-[#F8F8FA]">Recent Projects</h2>
              <button 
                onClick={() => onNavigate("projects")}
                className="text-sm text-[#3B82F6] hover:text-[#60A5FA] flex items-center gap-1 transition-colors"
              >
                View all <ArrowRight size={14} />
              </button>
            </div>
            <div className="grid grid-cols-2 gap-4">
              {projects.sort((a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime()).slice(0, 6).map(project => {
                const drift = driftMap[project.name];
                const isDrifted = drift?.drifted === true;
                const isInSync = drift !== undefined && drift !== null && !drift.drifted;
                const isConfigured = !!project.directory && project.agents.length > 0;

                return (
                  <div 
                    key={project.name}
                    onClick={() => handleProjectClick(project.name)}
                    className={`bg-[#1A1A1E] border rounded-lg p-5 cursor-pointer transition-all group ${
                      isDrifted
                        ? "border-[#F59E0B]/40 hover:border-[#F59E0B]/70"
                        : "border-[#33353A] hover:border-[#3B82F6]/50"
                    }`}
                  >
                    <div className="flex items-start justify-between mb-3">
                      <div className="flex items-center gap-2">
                        <FolderOpen size={16} className={isDrifted ? "text-[#F59E0B]" : "text-[#3B82F6]"} />
                        <h3 className={`font-medium text-[#F8F8FA] transition-colors ${
                          isDrifted ? "group-hover:text-[#F59E0B]" : "group-hover:text-[#3B82F6]"
                        }`}>{project.name}</h3>
                      </div>
                      {isConfigured && (
                        isDrifted ? (
                          <span className="flex items-center gap-1 text-[10px] bg-[#F59E0B]/10 text-[#F59E0B] px-1.5 py-0.5 rounded uppercase tracking-wider font-semibold">
                            <AlertCircle size={9} />
                            Drifted
                          </span>
                        ) : isInSync ? (
                          <span className="flex items-center gap-1 text-[10px] bg-[#4ADE80]/10 text-[#4ADE80] px-1.5 py-0.5 rounded uppercase tracking-wider font-semibold">
                            <CheckCircle2 size={9} />
                            In Sync
                          </span>
                        ) : (
                          <span className="text-[10px] bg-[#4ADE80]/10 text-[#4ADE80] px-1.5 py-0.5 rounded uppercase tracking-wider font-semibold">Configured</span>
                        )
                      )}
                    </div>
                    
                    {project.description && (
                      <p className="text-sm text-[#C8CAD0] mb-4 line-clamp-2">{project.description}</p>
                    )}

                    {/* Drift detail — which agents are affected */}
                    {isDrifted && drift && (
                      <div className="mb-3 text-[11px] text-[#F59E0B]/70 space-y-0.5">
                        {drift.agents.map((a) => (
                          <div key={a.agent_id}>
                            <span className="font-medium text-[#F59E0B]/90">{a.agent_label}:</span>{" "}
                            {a.files.map((f) => f.reason).join(", ")}
                          </div>
                        ))}
                      </div>
                    )}
                    
                    <div className="flex items-center gap-4 mt-auto pt-2 border-t border-[#33353A]/50">
                      <div className="flex items-center gap-1.5 text-xs text-[#C8CAD0]">
                        <Bot size={12} />
                        <span>{project.agents.length}</span>
                      </div>
                      <div className="flex items-center gap-1.5 text-xs text-[#C8CAD0]">
                        <Code size={12} />
                        <span>{project.skills.length + project.local_skills.length}</span>
                      </div>
                      <div className="flex items-center gap-1.5 text-xs text-[#C8CAD0]">
                        <Server size={12} />
                        <span>{project.mcp_servers.length}</span>
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {/* Marketplace Section */}
        <div>
          <h2 className="text-lg font-medium text-[#F8F8FA] mb-4">Discover & Extend</h2>
          <div className="grid grid-cols-3 gap-4">
            {/* Skills Marketplace */}
            <button
              onClick={() => onNavigate("skill-store")}
              className="bg-[#1A1A1E] border border-[#33353A] rounded-lg p-6 text-left hover:border-[#4ADE80]/50 hover:bg-[#222327] transition-all group flex flex-col justify-between"
            >
              <div>
                <div className="flex items-center gap-3 mb-4">
                  <div className="p-2.5 bg-[#4ADE80]/10 rounded-lg group-hover:bg-[#4ADE80]/20 transition-colors">
                    <Code size={20} className="text-[#4ADE80]" />
                  </div>
                  <h3 className="font-semibold text-[#F8F8FA]">Skills Marketplace</h3>
                </div>
                <p className="text-sm text-[#C8CAD0]">Discover and install pre-built skills, prompts, and workflows from the community.</p>
              </div>
              <div className="flex items-center gap-1.5 text-[12px] font-medium text-[#4ADE80] group-hover:gap-2 transition-all mt-4">
                Browse Skills <ArrowRight size={14} />
              </div>
            </button>

            {/* Templates Marketplace */}
            <button
              onClick={() => onNavigate("template-marketplace")}
              className="bg-[#1A1A1E] border border-[#33353A] rounded-lg p-6 text-left hover:border-[#818CF8]/50 hover:bg-[#222327] transition-all group flex flex-col justify-between"
            >
              <div>
                <div className="flex items-center gap-3 mb-4">
                  <div className="p-2.5 bg-[#818CF8]/10 rounded-lg group-hover:bg-[#818CF8]/20 transition-colors">
                    <Layers size={20} className="text-[#818CF8]" />
                  </div>
                  <h3 className="font-semibold text-[#F8F8FA]">Templates Marketplace</h3>
                </div>
                <p className="text-sm text-[#C8CAD0]">Explore project templates and file scaffolds to jumpstart your development.</p>
              </div>
              <div className="flex items-center gap-1.5 text-[12px] font-medium text-[#818CF8] group-hover:gap-2 transition-all mt-4">
                Browse Templates <ArrowRight size={14} />
              </div>
            </button>

            {/* MCP Servers Marketplace */}
            <button
              onClick={() => onNavigate("mcp-marketplace")}
              className="bg-[#1A1A1E] border border-[#33353A] rounded-lg p-6 text-left hover:border-[#F59E0B]/50 hover:bg-[#222327] transition-all group flex flex-col justify-between"
            >
              <div>
                <div className="flex items-center gap-3 mb-4">
                  <div className="p-2.5 bg-[#F59E0B]/10 rounded-lg group-hover:bg-[#F59E0B]/20 transition-colors">
                    <Server size={20} className="text-[#F59E0B]" />
                  </div>
                  <h3 className="font-semibold text-[#F8F8FA]">MCP Servers Marketplace</h3>
                </div>
                <p className="text-sm text-[#C8CAD0]">Connect AI-powered integrations and extend your agents with MCP servers.</p>
              </div>
              <div className="flex items-center gap-1.5 text-[12px] font-medium text-[#F59E0B] group-hover:gap-2 transition-all mt-4">
                Browse Servers <ArrowRight size={14} />
              </div>
            </button>
          </div>
        </div>
       </div>
     </div>
   );
}
