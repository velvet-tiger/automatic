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
  Download,
  Cloud,
  Monitor,
  Zap,
  Copy,
  Compass,
} from "lucide-react";
import { SkillAvatar } from "./SkillAvatar";

const BRANDFETCH_CLIENT_ID = import.meta.env.VITE_BRANDFETCH_CLIENT_ID as string | undefined;

function brandfetchUrl(domain: string, px: number): string {
  const s = Math.min(px * 2, 64);
  return `https://cdn.brandfetch.io/${encodeURIComponent(domain)}/w/${s}/h/${s}/theme/dark/fallback/lettermark/type/icon?c=${BRANDFETCH_CLIENT_ID ?? ""}`;
}

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

type PillIcon = "download" | "cloud" | "monitor" | "code" | "server";

interface FeaturedItem {
  type: "skill" | "template" | "mcp";
  title: string;
  description: string;
  provider: string;
  /** Unique identifier passed to the marketplace to pre-select this item:
   *  skill → skill store ID (e.g. "owner/repo/name")
   *  template → template name slug (e.g. "nextjs-saas-starter")
   *  mcp → server slug (e.g. "github") */
  itemId: string;
  /** For skills: "owner/repo" source slug used by SkillAvatar */
  source?: string;
  /** For templates/MCP: brand domain for Brandfetch icon, e.g. "nextjs.org" */
  icon?: string;
  /** Classification badge label (MCP only) */
  classification?: string;
  /** Metadata pills to show below description */
  pills?: { label: string; icon: PillIcon }[];
}

const FEATURED_ITEMS: FeaturedItem[] = [
  {
    type: "skill",
    title: "Web Interface Guidelines",
    description: "Audits files for compliance with Vercel's web interface guidelines, fetching the latest rules from the source.",
    provider: "vercel-labs/agent-skills",
    itemId: "vercel-labs/agent-skills/web-design-guidelines",
    source: "vercel-labs/agent-skills",
    pills: [{ label: "112k installs", icon: "download" }],
  },
  {
    type: "template",
    title: "Next.js SaaS Starter",
    description: "A full-stack SaaS boilerplate with Next.js App Router, Tailwind CSS, Prisma ORM, and NextAuth.js. Includes authentication flows, dashboard layout, billing hooks, and a component library ready for rapid product development.",
    provider: "Automatic",
    itemId: "nextjs-saas-starter",
    icon: "nextjs.org",
    pills: [{ label: "2 skills", icon: "code" }],
  },
  {
    type: "mcp",
    title: "GitHub",
    description: "Connect AI assistants to GitHub — manage repos, issues, PRs, and workflows through natural language.",
    provider: "GitHub",
    itemId: "github",
    icon: "github.com",
    classification: "official",
    pills: [{ label: "Remote", icon: "cloud" }, { label: "Local", icon: "monitor" }],
  },
];

// ── Featured card icon ────────────────────────────────────────────────────────

function FeaturedIcon({ item, size }: { item: FeaturedItem; size: number }) {
  const [imgError, setImgError] = useState(false);

  if (item.type === "skill") {
    return (
      <SkillAvatar
        name={item.title}
        source={item.source}
        size={size}
      />
    );
  }

  if (item.icon && BRANDFETCH_CLIENT_ID && !imgError) {
    return (
      <img
        src={brandfetchUrl(item.icon, size)}
        alt={item.title}
        width={size}
        height={size}
        onError={() => setImgError(true)}
        className="flex-shrink-0 rounded-md object-contain"
        style={{ width: size, height: size }}
        draggable={false}
      />
    );
  }

  // Letter fallback
  const letter = item.title.charAt(0).toUpperCase();
  const fontSize = Math.round(size * 0.45);
  const bg = item.type === "mcp" ? "bg-icon-mcp/10" : "bg-brand/10";
  const color = item.type === "mcp" ? "text-icon-mcp" : "text-brand";
  return (
    <div
      className={`flex-shrink-0 rounded-md flex items-center justify-center font-semibold ${bg} ${color}`}
      style={{ width: size, height: size, fontSize }}
      aria-hidden="true"
    >
      {letter}
    </div>
  );
}

// ── Featured card ─────────────────────────────────────────────────────────────

const PILL_ICONS: Record<PillIcon, React.ReactNode> = {
  download: <Download size={10} />,
  cloud:    <Cloud size={10} />,
  monitor:  <Monitor size={10} />,
  code:     <Code size={10} />,
  server:   <Server size={10} />,
};

const CLASSIFICATION_COLORS: Record<string, string> = {
  official:  "bg-brand/15 text-brand border-brand/20",
  reference: "bg-accent/15 text-accent border-accent/20",
  community: "bg-accent-hover/15 text-accent-hover border-accent-hover/20",
};

function FeaturedCard({
  item,
  onNavigateToSkillStore,
  onNavigateToMcpMarketplace,
  onNavigateToTemplateMarketplace,
}: {
  item: FeaturedItem;
  onNavigateToSkillStore?: (id: string) => void;
  onNavigateToMcpMarketplace?: (slug: string) => void;
  onNavigateToTemplateMarketplace?: (name: string) => void;
}) {
  const handleClick = () => {
    if (item.type === "skill") onNavigateToSkillStore?.(item.itemId);
    else if (item.type === "mcp") onNavigateToMcpMarketplace?.(item.itemId);
    else if (item.type === "template") onNavigateToTemplateMarketplace?.(item.itemId);
  };

  return (
    <button
      onClick={handleClick}
      className="group w-full h-full text-left p-5 rounded-xl bg-bg-input border border-border-strong/40 hover:border-border-strong hover:bg-surface-hover transition-all flex flex-col"
    >
      {/* Header */}
      <div className="flex items-start justify-between gap-3 mb-4 min-h-[52px]">
        <div className="flex items-center gap-2.5 min-w-0">
          <FeaturedIcon item={item} size={36} />
          <div className="min-w-0">
            <div className="text-[14px] font-semibold text-text-base leading-snug truncate">
              {item.title}
            </div>
            {item.classification ? (
              <span
                className={`inline-flex items-center px-2 py-0.5 rounded text-[10px] font-medium border ${CLASSIFICATION_COLORS[item.classification] ?? "bg-surface text-text-muted border-border-strong/40"}`}
              >
                {item.classification}
              </span>
            ) : (
              <span className="text-[10px] font-medium text-text-muted truncate block">
                {item.provider}
              </span>
            )}
          </div>
        </div>
        <ArrowRight size={13} className="text-surface group-hover:text-text-muted transition-colors flex-shrink-0 mt-0.5" />
      </div>

      {/* Description — flex-1 pushes pills+footer to bottom */}
      <div className="flex-1 min-h-0">
        <p className="text-[12px] text-text-muted leading-relaxed line-clamp-3">
          {item.description}
        </p>
      </div>

      {/* Pills */}
      {item.pills && item.pills.length > 0 && (
        <div className="flex flex-wrap gap-1.5 mt-3">
          {item.pills.map((pill) => (
            <span
              key={pill.label}
              className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-bg-sidebar border border-border-strong/40 text-[10px] text-text-muted"
            >
              {PILL_ICONS[pill.icon]}
              {pill.label}
            </span>
          ))}
        </div>
      )}

      {/* Footer — type badge + provider */}
      <div className="flex items-center justify-between gap-2 mt-2.5 pt-2.5 border-t border-border-strong/40">
        <TypeBadge type={item.type} />
        <span className="text-[10px] text-text-muted truncate">{item.provider}</span>
      </div>
    </button>
  );
}

const TYPE_META: Record<FeaturedItem["type"], { label: string; icon: React.ReactNode; bg: string; text: string }> = {
  skill:    { label: "Skill",    icon: <Code size={10} />,   bg: "bg-icon-skill/15",          text: "text-icon-skill" },
  template: { label: "Template", icon: <Layers size={10} />, bg: "bg-icon-file-template/15",  text: "text-icon-file-template" },
  mcp:      { label: "MCP",      icon: <Server size={10} />, bg: "bg-icon-mcp/15",            text: "text-icon-mcp" },
};

function TypeBadge({ type }: { type: FeaturedItem["type"] }) {
  const meta = TYPE_META[type];
  return (
    <span className={`inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium ${meta.bg} ${meta.text}`}>
      {meta.icon}
      {meta.label}
    </span>
  );
}

// ── Use Cases Section ─────────────────────────────────────────────────────────

interface UseCaseAction {
  label: string;
  tab: string;
}

interface UseCase {
  icon: React.ReactNode;
  accentBg: string;
  accentText: string;
  accentBorder: string;
  title: string;
  goal: string;
  actions: UseCaseAction[];
}

const USE_CASES: UseCase[] = [
  {
    icon: <Zap size={18} />,
    accentBg: "bg-icon-agent/10",
    accentText: "text-icon-agent",
    accentBorder: "border-icon-agent/20",
    title: "Ship higher-quality AI code",
    goal: "Your agent produces inconsistent results and doesn't follow your conventions. Give it the context it needs to work the way you do.",
    actions: [
      { label: "Create a project", tab: "projects" },
      { label: "Add skills for your stack", tab: "skills" },
      { label: "Write rules & instructions", tab: "rules" },
    ],
  },
  {
    icon: <Copy size={18} />,
    accentBg: "bg-icon-file-template/10",
    accentText: "text-icon-file-template",
    accentBorder: "border-icon-file-template/20",
    title: "Reuse your setup everywhere",
    goal: "You've found patterns that work, but rebuilding them for each project wastes time. Capture them once and share them across every agent and project.",
    actions: [
      { label: "Create reusable instructions", tab: "templates" },
      { label: "Define shared rules", tab: "rules" },
      { label: "Build project templates", tab: "project-templates" },
    ],
  },
  {
    icon: <Compass size={18} />,
    accentBg: "bg-icon-mcp/10",
    accentText: "text-icon-mcp",
    accentBorder: "border-icon-mcp/20",
    title: "Discover the best tools",
    goal: "The ecosystem moves fast. Find proven skills, pre-built project setups, and MCP servers that give your agents new capabilities without starting from scratch.",
    actions: [
      { label: "Browse the skill library", tab: "skill-store" },
      { label: "Explore project templates", tab: "template-marketplace" },
      { label: "Connect MCP servers", tab: "mcp-marketplace" },
    ],
  },
];

function UseCasesSection({ onNavigate }: { onNavigate: (tab: string) => void }) {
  return (
    <div>
      <div className="flex items-center gap-2 mb-3">
        <Sparkles size={13} className="text-brand" />
        <h2 className="text-[13px] font-semibold text-text-muted tracking-wide uppercase">What's your goal?</h2>
      </div>
      <div className="grid grid-cols-3 gap-4">
        {USE_CASES.map((uc) => (
          <div
            key={uc.title}
            className="bg-bg-input border border-border-strong/40 rounded-xl p-5 flex flex-col"
          >
            {/* Icon + title */}
            <div className="flex items-start gap-3 mb-3">
              <div className={`p-2 rounded-lg flex-shrink-0 ${uc.accentBg} border ${uc.accentBorder}`}>
                <span className={uc.accentText}>{uc.icon}</span>
              </div>
              <h3 className="text-[14px] font-semibold text-text-base leading-snug pt-1">{uc.title}</h3>
            </div>

            {/* Problem framing */}
            <p className="text-[12px] text-text-muted leading-relaxed flex-1 mb-4">{uc.goal}</p>

            {/* Action steps */}
            <div className="space-y-1">
              {uc.actions.map((action) => (
                <button
                  key={action.label}
                  onClick={() => onNavigate(action.tab)}
                  className={`w-full flex items-center justify-between gap-2 px-3 py-2 rounded-lg text-left text-[12px] font-medium transition-all group bg-bg-sidebar border border-transparent hover:border-border-strong/60 hover:bg-surface-hover ${uc.accentText}`}
                >
                  <span className="text-text-base group-hover:text-text-base transition-colors">{action.label}</span>
                  <ArrowRight size={11} className={`flex-shrink-0 opacity-40 group-hover:opacity-100 group-hover:translate-x-0.5 transition-all ${uc.accentText}`} />
                </button>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

interface DashboardProps {
  onNavigate: (tab: string) => void;
  onNavigateToSkillStore?: (skillId: string) => void;
  onNavigateToMcpMarketplace?: (slug: string) => void;
  onNavigateToTemplateMarketplace?: (templateName: string) => void;
}

export default function Dashboard({ onNavigate, onNavigateToSkillStore, onNavigateToMcpMarketplace, onNavigateToTemplateMarketplace }: DashboardProps) {
  const [projects, setProjects] = useState<Project[]>([]);
  const [mcpServerCount, setMcpServerCount] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  // Map of project name → drift report (undefined = not yet checked, null = not applicable)
  const [driftMap, setDriftMap] = useState<Record<string, DriftReport | null>>({});

  // Getting-started flags — persisted in settings.json via the backend.
  const [skillInstalled, setSkillInstalled] = useState(false);
  const [templateImported, setTemplateImported] = useState(false);

  useEffect(() => {
    loadData();
  }, []);

  const loadData = async () => {
    setLoading(true);
    try {
      // Get project names + MCP server configs + settings in parallel
      const [names, mcpNames, settings] = await Promise.all([
        invoke<string[]>("get_projects"),
        invoke<string[]>("list_mcp_server_configs").catch(() => [] as string[]),
        invoke<any>("read_settings").catch(() => null),
      ]);

      // Seed getting-started flags from persisted settings
      if (settings?.getting_started) {
        setSkillInstalled(!!settings.getting_started.skill_installed);
        setTemplateImported(!!settings.getting_started.template_imported);
      }
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
                I'm Chris, the developer of Automatic. Automatic was built to solve a problem I experienced working with AI tools — it keeps your shared project instructions, skills, MCP servers and other AI config in sync and up to date across all your projects and agents.
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
                We've got exciting plans for Automatic, and we can't wait to show you what we're working on.
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
                  show: !templateImported,
                  label: "Import a template",
                  description: "Start from a pre-built project setup.",
                  action: () => onNavigate("template-marketplace"),
                  actionLabel: "Browse Templates",
                  color: "text-icon-file-template",
                  hoverBorder: "hover:border-icon-file-template/50",
                },
                {
                  show: !skillInstalled,
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
                        className={`w-full flex items-start gap-3 px-4 py-3 text-left transition-all group hover:bg-surface-hover`}
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

        {/* Use Cases Section */}
        <UseCasesSection onNavigate={onNavigate} />

        {/* Featured Section — hidden for now */}
        {false && (
        <div>
          <div className="flex items-center gap-2 mb-3">
            <Star size={13} className="text-brand" />
            <h2 className="text-[13px] font-semibold text-text-muted tracking-wide uppercase">Featured</h2>
          </div>
          <div className="grid grid-cols-3 gap-4">
            {FEATURED_ITEMS.map((item) => (
              <FeaturedCard
                key={item.title}
                item={item}
                onNavigateToSkillStore={onNavigateToSkillStore}
                onNavigateToMcpMarketplace={onNavigateToMcpMarketplace}
                onNavigateToTemplateMarketplace={onNavigateToTemplateMarketplace}
              />
            ))}
          </div>
        </div>
        )}

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
