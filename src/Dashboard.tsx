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
  Star,
} from "lucide-react";
import { AuthorPanel, type AuthorDescriptor } from "./AuthorPanel";

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

interface FeaturedItem {
  type: "skill" | "template" | "mcp";
  title: string;
  description: string;
  navigateTo: string;
  badge: string;
  _author?: AuthorDescriptor;
}

const FEATURED_ITEMS: FeaturedItem[] = [
  {
    type: "skill",
    title: "Web Interface Guidelines",
    description: "Audits files for compliance with Vercel's web interface guidelines, fetching the latest rules from the source.",
    navigateTo: "skill-store",
    badge: "Skill",
    _author: { type: "provider", name: "Vercel Labs" },
  },
  {
    type: "template",
    title: "Next.js SaaS Starter",
    description: "Full-stack SaaS boilerplate with App Router, Tailwind, Prisma, and NextAuth. Ready for rapid product development.",
    navigateTo: "template-marketplace",
    badge: "Template",
    _author: { type: "provider", name: "Automatic", url: "https://automatic.sh" },
  },
  {
    type: "mcp",
    title: "GitHub",
    description: "Manage repos, issues, PRs, and workflows through natural language via the official GitHub MCP server.",
    navigateTo: "mcp-marketplace",
    badge: "MCP Server",
    _author: { type: "provider", name: "GitHub", url: "https://github.com" },
  },
];

const FEATURED_COLORS: Record<FeaturedItem["type"], { icon: string; border: string; badge: string; badgeBg: string }> = {
  skill:    { icon: "text-icon-skill",          border: "border-icon-skill/30 hover:border-icon-skill/60",          badge: "text-icon-skill",          badgeBg: "bg-icon-skill/10" },
  template: { icon: "text-icon-file-template",  border: "border-icon-file-template/30 hover:border-icon-file-template/60",  badge: "text-icon-file-template",  badgeBg: "bg-icon-file-template/10" },
  mcp:      { icon: "text-icon-mcp",            border: "border-icon-mcp/30 hover:border-icon-mcp/60",            badge: "text-icon-mcp",            badgeBg: "bg-icon-mcp/10" },
};



interface DashboardProps {
  onNavigate: (tab: string) => void;
}

export default function Dashboard({ onNavigate }: DashboardProps) {
  const [projects, setProjects] = useState<Project[]>([]);
  const [mcpServerCount, setMcpServerCount] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  // Map of project name → drift report (undefined = not yet checked, null = not applicable)
  const [driftMap, setDriftMap] = useState<Record<string, DriftReport | null>>({});

  // Visited flags — read once on mount; updated reactively when the user navigates
  const [visitedTemplates, setVisitedTemplates] = useState(
    () => !!localStorage.getItem("automatic.visited.template-marketplace")
  );
  const [visitedSkills, setVisitedSkills] = useState(
    () => !!localStorage.getItem("automatic.visited.skill-store")
  );

  useEffect(() => {
    loadData();
  }, []);

  // Re-read visited flags whenever the window regains focus (user may have
  // navigated away and come back to the dashboard tab)
  useEffect(() => {
    const sync = () => {
      setVisitedTemplates(!!localStorage.getItem("automatic.visited.template-marketplace"));
      setVisitedSkills(!!localStorage.getItem("automatic.visited.skill-store"));
    };
    window.addEventListener("focus", sync);
    return () => window.removeEventListener("focus", sync);
  }, []);

  const loadData = async () => {
    setLoading(true);
    try {
      // Get project names + MCP server configs in parallel
      const [names, mcpNames] = await Promise.all([
        invoke<string[]>("get_projects"),
        invoke<string[]>("list_mcp_server_configs").catch(() => [] as string[]),
      ]);
      setMcpServerCount(mcpNames.length);

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
    localStorage.setItem("automatic.projects.selected", name);
    onNavigate("projects");
  };

  const driftedCount = Object.values(driftMap).filter((r) => r?.drifted).length;

  if (loading) {
    return (
      <div className="flex-1 flex items-center justify-center h-full bg-bg-base">
        <div className="flex items-center gap-2 text-text-muted">
          <RefreshCw size={16} className="animate-spin" />
          <span>Loading dashboard...</span>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 h-full overflow-y-auto p-8 custom-scrollbar bg-transparent relative z-10">
      <div className="max-w-5xl mx-auto space-y-8">

        {error && (
          <div className="bg-red-500/10 text-red-400 p-4 rounded-md border border-red-500/20 flex items-start gap-3">
            <AlertCircle size={16} className="mt-0.5 shrink-0" />
            <div className="text-sm">{error}</div>
          </div>
        )}

        {/* Drift alert banner — shown when at least one project has drifted */}
        {driftedCount > 0 && (
          <div className="bg-warning/10 border border-warning/30 rounded-lg px-5 py-3 flex items-center justify-between">
            <div className="flex items-center gap-2.5 text-warning">
              <AlertCircle size={15} className="shrink-0" />
              <span className="text-[13px] font-medium">
                {driftedCount === 1
                  ? "1 project has drifted — agent config files are out of sync."
                  : `${driftedCount} projects have drifted — agent config files are out of sync.`}
              </span>
            </div>
            <button
              onClick={() => onNavigate("projects")}
              className="text-[12px] font-medium text-warning hover:text-warning-hover underline decoration-warning/40 hover:decoration-warning-hover transition-colors shrink-0 ml-4"
            >
              Review in Projects
            </button>
          </div>
        )}

        {/* Top row: Header + Chris note (left) | Recent Projects + Getting Started (right) */}
        <div className="grid grid-cols-[1fr_320px] gap-6 items-start">

          {/* Left column: title + Chris note */}
          <div>
            <h1 className="text-2xl font-semibold text-text-base mb-2">Welcome to Automatic</h1>
            <p className="text-text-muted text-sm">Manage your AI agent configurations and projects</p>

            <div className="mt-6 bg-bg-input border border-border-strong/40 rounded-lg p-5 text-[13px] text-text-base leading-relaxed space-y-3">
              <p>Hi,</p>
              <p>
                I'm Chris, the developer of Automatic. Automatic was built to solve a problem I experienced working with AI tools — it keeps your shared project instructions, skills, MCP servers and other AI config in sync and up to date.
              </p>
              <p>
                If you find it useful, please{" "}
                <a
                  href="https://github.com/velvet-tiger/automatic"
                  target="_blank"
                  rel="noreferrer"
                  className="text-text-base hover:text-text-base underline decoration-text-muted/40 hover:decoration-text-base/60 font-medium transition-colors"
                >
                  give us a star on GitHub
                </a>{" "}
                and tell your friends about us.
              </p>
              <p>
                We've got heaps of plans for Automatic, and we can't wait to show you what we're working on.
              </p>
              <p className="text-text-base font-medium">— Chris</p>
            </div>
          </div>

          {/* Right column: Recent Projects + conditional Getting Started checklist */}
          <div className="flex flex-col gap-4">

            {/* Recent Projects — hidden when empty */}
            {projects.length > 0 && (
            <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden">
              <div className="flex items-center justify-between px-4 pt-4 pb-3">
                <h2 className="text-sm font-semibold text-text-base">Recent Projects</h2>
                <button
                  onClick={() => onNavigate("projects")}
                  className="text-xs text-brand hover:text-brand-hover flex items-center gap-1 transition-colors"
                >
                  View all <ArrowRight size={12} />
                </button>
              </div>
              <div className="divide-y divide-border-strong/30 border-t border-border-strong/30">
                {projects
                  .sort((a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime())
                  .slice(0, 6)
                  .map(project => {
                      const drift = driftMap[project.name];
                      const isDrifted = drift?.drifted === true;
                      const isInSync = drift !== undefined && drift !== null && !drift.drifted;
                      const isConfigured = !!project.directory && project.agents.length > 0;

                      return (
                        <div
                          key={project.name}
                          onClick={() => handleProjectClick(project.name)}
                          className={`flex items-center gap-3 px-4 py-2.5 cursor-pointer transition-colors group ${
                            isDrifted ? "hover:bg-warning/5" : "hover:bg-surface-hover"
                          }`}
                        >
                          <FolderOpen size={14} className={`shrink-0 ${isDrifted ? "text-warning" : "text-brand"}`} />

                          <span className={`text-[12px] font-medium text-text-base truncate min-w-0 transition-colors ${
                            isDrifted ? "group-hover:text-warning" : "group-hover:text-brand"
                          }`}>{project.name}</span>

                          <div className="ml-auto flex items-center gap-2 shrink-0">
                            <div className="flex items-center gap-2 text-[10px] text-text-muted">
                              <span className="flex items-center gap-0.5"><Bot size={10} />{project.agents.length}</span>
                              <span className="flex items-center gap-0.5"><Code size={10} />{project.skills.length + project.local_skills.length}</span>
                              <span className="flex items-center gap-0.5"><Server size={10} />{project.mcp_servers.length}</span>
                            </div>

                            {isConfigured && (
                              isDrifted ? (
                                <span className="flex items-center gap-0.5 text-[10px] bg-warning/10 text-warning px-1.5 py-0.5 rounded font-semibold">
                                  <AlertCircle size={8} />
                                  Drifted
                                </span>
                              ) : isInSync ? (
                                <span className="flex items-center gap-0.5 text-[10px] bg-success/10 text-success px-1.5 py-0.5 rounded font-semibold">
                                  <CheckCircle2 size={8} />
                                  Synced
                                </span>
                              ) : null
                            )}
                          </div>
                        </div>
                      );
                    })}
              </div>
            </div>
            )}

            {/* Getting Started checklist — only shown while any item is incomplete */}
            {(() => {
              const items = [
                {
                  show: projects.length === 0,
                  label: "Create a project",
                  description: "Link a directory to an agent configuration.",
                  action: () => onNavigate("projects"),
                  actionLabel: "Go to Projects",
                  color: "text-brand",
                  hoverBorder: "hover:border-brand/50",
                },
                {
                  show: !visitedTemplates,
                  label: "Browse project templates",
                  description: "Start from a pre-built setup.",
                  action: () => onNavigate("template-marketplace"),
                  actionLabel: "Browse Templates",
                  color: "text-icon-file-template",
                  hoverBorder: "hover:border-icon-file-template/50",
                },
                {
                  show: !visitedSkills,
                  label: "Install a skill",
                  description: "Load specialised capabilities into your agents.",
                  action: () => onNavigate("skill-store"),
                  actionLabel: "Browse Skills",
                  color: "text-icon-skill",
                  hoverBorder: "hover:border-icon-skill/50",
                },
                {
                  show: mcpServerCount === 0,
                  label: "Connect MCP servers",
                  description: "Extend your agents with powerful integrations.",
                  action: () => onNavigate("mcp-marketplace"),
                  actionLabel: "Browse Servers",
                  color: "text-icon-mcp",
                  hoverBorder: "hover:border-icon-mcp/50",
                },
              ].filter(i => i.show);

              if (items.length === 0) return null;

              return (
                <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden">
                  <div className="flex items-center gap-2 px-4 pt-4 pb-3">
                    <Sparkles size={12} className="text-brand" />
                    <h2 className="text-sm font-semibold text-text-base">Getting started</h2>
                  </div>
                  <div className="divide-y divide-border-strong/30 border-t border-border-strong/30">
                    {items.map((item) => (
                      <button
                        key={item.label}
                        onClick={item.action}
                        className={`w-full flex items-start gap-3 px-4 py-3 text-left border border-transparent transition-all group ${item.hoverBorder} hover:bg-surface-hover`}
                      >
                        <div className="flex-1 min-w-0">
                          <p className={`text-[12px] font-medium text-text-base group-hover:${item.color} transition-colors`}>{item.label}</p>
                          <p className="text-[11px] text-text-muted mt-0.5 leading-relaxed">{item.description}</p>
                        </div>
                        <ArrowRight size={12} className={`mt-0.5 shrink-0 ${item.color} opacity-60 group-hover:opacity-100 group-hover:translate-x-0.5 transition-all`} />
                      </button>
                    ))}
                  </div>
                </div>
              );
            })()}

          </div>
        </div>

        {/* Featured Section */}
        <div>
          <div className="flex items-center gap-2 mb-3">
            <Star size={13} className="text-brand" />
            <h2 className="text-[13px] font-semibold text-text-muted tracking-wide uppercase">Featured</h2>
          </div>
          <div className="grid grid-cols-3 gap-3">
            {FEATURED_ITEMS.map((item) => {
              const colors = FEATURED_COLORS[item.type];
              return (
                <button
                  key={item.title}
                  onClick={() => onNavigate(item.navigateTo)}
                  className={`relative bg-bg-input border rounded-lg p-5 text-left transition-all group flex flex-col gap-3 ${colors.border}`}
                >
                  <span className={`absolute top-3 right-3 text-[10px] font-semibold uppercase tracking-wider px-1.5 py-0.5 rounded ${colors.badge} ${colors.badgeBg}`}>{item.badge}</span>
                  <h4 className="text-sm font-semibold text-text-base">{item.title}</h4>
                  <AuthorPanel descriptor={item._author} />
                  <p className="text-[13px] text-text-muted leading-relaxed line-clamp-3">{item.description}</p>
                </button>
              );
            })}
          </div>
        </div>

        {/* Discover & Extend — full-width across the bottom */}
        <div>
          <h2 className="text-lg font-medium text-text-base mb-4">Discover &amp; Extend</h2>
          <div className="grid grid-cols-3 gap-4">
            {/* Skills Marketplace */}
            <button
              onClick={() => onNavigate("skill-store")}
              className="bg-bg-input border border-border-strong/40 rounded-lg p-6 text-left hover:border-icon-skill/50 hover:bg-bg-input transition-all group flex flex-col justify-between"
            >
              <div>
                <div className="flex items-center gap-3 mb-4">
                  <div className="p-2.5 bg-icon-skill/10 rounded-lg group-hover:bg-icon-skill/20 transition-colors">
                    <Code size={20} className="text-icon-skill" />
                  </div>
                  <h3 className="font-semibold text-text-base">Skills Marketplace</h3>
                </div>
                <p className="text-sm text-text-muted">Discover and install pre-built skills, prompts, and workflows from the community.</p>
              </div>
              <div className="flex items-center gap-1.5 text-[12px] font-medium text-icon-skill group-hover:gap-2 transition-all mt-4">
                Browse Skills <ArrowRight size={14} />
              </div>
            </button>

            {/* Templates Marketplace */}
            <button
              onClick={() => onNavigate("template-marketplace")}
              className="bg-bg-input border border-border-strong/40 rounded-lg p-6 text-left hover:border-icon-file-template/50 hover:bg-bg-input transition-all group flex flex-col justify-between"
            >
              <div>
                <div className="flex items-center gap-3 mb-4">
                  <div className="p-2.5 bg-icon-file-template/10 rounded-lg group-hover:bg-icon-file-template/20 transition-colors">
                    <Layers size={20} className="text-icon-file-template" />
                  </div>
                  <h3 className="font-semibold text-text-base">Templates Marketplace</h3>
                </div>
                <p className="text-sm text-text-muted">Explore project templates and file scaffolds to jumpstart your development.</p>
              </div>
              <div className="flex items-center gap-1.5 text-[12px] font-medium text-icon-file-template group-hover:gap-2 transition-all mt-4">
                Browse Templates <ArrowRight size={14} />
              </div>
            </button>

            {/* MCP Servers Marketplace */}
            <button
              onClick={() => onNavigate("mcp-marketplace")}
              className="bg-bg-input border border-border-strong/40 rounded-lg p-6 text-left hover:border-icon-mcp/50 hover:bg-bg-input transition-all group flex flex-col justify-between"
            >
              <div>
                <div className="flex items-center gap-3 mb-4">
                  <div className="p-2.5 bg-icon-mcp/10 rounded-lg group-hover:bg-icon-mcp/20 transition-colors">
                    <Server size={20} className="text-icon-mcp" />
                  </div>
                  <h3 className="font-semibold text-text-base">MCP Servers Marketplace</h3>
                </div>
                <p className="text-sm text-text-muted">Connect AI-powered integrations and extend your agents with MCP servers.</p>
              </div>
              <div className="flex items-center gap-1.5 text-[12px] font-medium text-icon-mcp group-hover:gap-2 transition-all mt-4">
                Browse Servers <ArrowRight size={14} />
              </div>
            </button>
          </div>
        </div>

      </div>
    </div>
  );
}
