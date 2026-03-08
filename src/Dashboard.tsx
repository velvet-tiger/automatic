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
  History,
  Check,
  Lightbulb,
} from "lucide-react";
import { SkillAvatar } from "./SkillAvatar";

const BRANDFETCH_CLIENT_ID = import.meta.env.VITE_BRANDFETCH_CLIENT_ID as string | undefined;

// ── Activity feed ─────────────────────────────────────────────────────────────

interface ActivityEntry {
  id: number;
  project: string;
  event: string;
  label: string;
  detail: string;
  timestamp: string;
}

function relativeTime(iso: string): string {
  const now = Date.now();
  const then = new Date(iso).getTime();
  const diffMs = now - then;
  if (diffMs < 0) return "just now";
  const diffSec = Math.floor(diffMs / 1000);
  if (diffSec < 60) return "just now";
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return `${diffMin} min ago`;
  const diffHr = Math.floor(diffMin / 60);
  if (diffHr < 24) return `${diffHr}h ago`;
  const diffDay = Math.floor(diffHr / 24);
  if (diffDay === 1) return "Yesterday";
  if (diffDay < 7) return `${diffDay} days ago`;
  const diffWk = Math.floor(diffDay / 7);
  if (diffWk === 1) return "1 week ago";
  if (diffWk < 5) return `${diffWk} weeks ago`;
  return new Date(iso).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

function activityEntryMeta(event: string): { icon: React.ReactNode; dot: string } {
  switch (event) {
    case "sync":
      return { icon: <Check size={12} className="text-success" />, dot: "bg-success" };
    case "skill_added":
      return { icon: <Code size={12} className="text-icon-skill" />, dot: "bg-icon-skill" };
    case "skill_removed":
      return { icon: <Code size={12} className="text-text-muted" />, dot: "bg-text-muted" };
    case "mcp_server_added":
      return { icon: <Server size={12} className="text-icon-mcp" />, dot: "bg-icon-mcp" };
    case "mcp_server_removed":
      return { icon: <Server size={12} className="text-text-muted" />, dot: "bg-text-muted" };
    case "agent_added":
      return { icon: <Bot size={12} className="text-brand" />, dot: "bg-brand" };
    case "agent_removed":
      return { icon: <Bot size={12} className="text-text-muted" />, dot: "bg-text-muted" };
    case "project_created":
      return { icon: <FolderOpen size={12} className="text-brand" />, dot: "bg-brand" };
    case "project_updated":
      return { icon: <RefreshCw size={12} className="text-text-muted" />, dot: "bg-text-muted" };
    default:
      return { icon: <History size={12} className="text-text-muted" />, dot: "bg-text-muted" };
  }
}

function GlobalActivityFeed({
  entries,
  loading,
}: {
  entries: ActivityEntry[];
  loading: boolean;
}) {
  return (
    <div>
      <div className="flex items-center gap-2 mb-3">
        <History size={13} className="text-text-muted" />
        <h2 className="text-[13px] font-semibold text-text-muted tracking-wide uppercase">Recent Activity</h2>
      </div>
      <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden">
        {loading ? (
          <div className="px-4 py-6 text-center text-[12px] text-text-muted">Loading activity…</div>
        ) : entries.length === 0 ? (
          <div className="px-4 py-6 text-center text-[12px] text-text-muted italic">
            No activity yet. Save or sync a project to start recording events.
          </div>
        ) : (
          entries.map((item, i) => {
            const { icon } = activityEntryMeta(item.event);
            return (
              <div
                key={item.id}
                className={`flex items-center gap-3 px-4 h-[42px] ${i < entries.length - 1 ? "border-b border-border-strong/20" : ""}`}
              >
                <div className="flex-shrink-0 leading-[0]">{icon}</div>
                <div className="flex-1 min-w-0 leading-none flex items-center gap-0 truncate">
                  <span className="text-[11px] text-text-muted font-medium mr-1.5 shrink-0">[{item.project}]</span>
                  <span className="text-[12px] text-text-base truncate">{item.label}</span>
                  {item.detail && (
                    <span className="text-[12px] text-text-muted ml-1.5 shrink-0">{item.detail}</span>
                  )}
                </div>
                <span className="text-[11px] text-text-muted flex-shrink-0 leading-none ml-3">{relativeTime(item.timestamp)}</span>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}

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
  /** Instruction files modified outside Automatic (optional, used by Projects view). */
  instruction_conflicts?: {
    filename: string;
    agent_labels: string[];
    disk_content: string;
    automatic_content: string;
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

// ── Projects Health Bar ───────────────────────────────────────────────────────

interface ProjectsHealthBarProps {
  projects: string[];
  projectDetails: Map<string, Project>;
  driftByProject: Record<string, DriftReport | null>;
}

function ProjectsHealthBar({ projects, projectDetails, driftByProject }: ProjectsHealthBarProps) {
  if (projects.length === 0) return null;

  const total = projects.length;
  const synced = projects.filter((n) => driftByProject[n]?.drifted === false).length;
  const drifted = projects.filter((n) => driftByProject[n]?.drifted === true).length;
  const checking = projects.filter((n) => driftByProject[n] === undefined).length;

  // Unique agent ids across all projects
  const agentSet = new Set<string>();
  let totalSkills = 0;
  let totalMcp = 0;
  let fullyConfigured = 0;

  for (const name of projects) {
    const p = projectDetails.get(name);
    if (!p) continue;
    (p.agents ?? []).forEach((a) => agentSet.add(a));
    totalSkills += (p.skills?.length ?? 0) + (p.local_skills?.length ?? 0);
    totalMcp += p.mcp_servers?.length ?? 0;
    if ((p.agents?.length ?? 0) > 0 && !!p.directory) fullyConfigured++;
  }

  // Show a compact progress-like bar for synced/drifted/checking ratio
  const syncedPct = total > 0 ? Math.round((synced / total) * 100) : 0;
  const driftedPct = total > 0 ? Math.round((drifted / total) * 100) : 0;
  const checkingPct = total > 0 ? Math.max(0, 100 - syncedPct - driftedPct) : 0;

  return (
    <div className="rounded-xl border border-border-strong/40 bg-bg-input overflow-hidden">
      {/* Stat strip */}
      <div className="flex items-stretch divide-x divide-border-strong/30">
        {/* Projects */}
        <div className="flex-1 flex flex-col items-center justify-center gap-0.5 px-3 py-3 min-w-0">
          <div className="flex items-center gap-1 text-text-base">
            <FolderOpen size={13} />
            <span className="text-[15px] font-semibold tabular-nums leading-none">{total}</span>
          </div>
          <span className="text-[10px] text-text-muted tracking-wide uppercase mt-0.5">Projects</span>
        </div>

        {/* Synced — uses health token so corporate themes get luminance-stepped grey */}
        <div className="flex-1 flex flex-col items-center justify-center gap-0.5 px-3 py-3 min-w-0">
          <div
            className="flex items-center gap-1"
            style={{ color: synced > 0 ? "var(--health-synced)" : undefined }}
          >
            <Check size={13} className={synced === 0 ? "text-text-muted" : ""} />
            <span className={`text-[15px] font-semibold tabular-nums leading-none ${synced === 0 ? "text-text-muted" : ""}`}>{synced}</span>
          </div>
          <span className="text-[10px] text-text-muted tracking-wide uppercase mt-0.5">Synced</span>
        </div>

        {/* Drifted — uses health token */}
        <div className="flex-1 flex flex-col items-center justify-center gap-0.5 px-3 py-3 min-w-0">
          <div
            className="flex items-center gap-1"
            style={{ color: drifted > 0 ? "var(--health-drifted)" : undefined }}
          >
            <AlertCircle size={13} className={drifted === 0 ? "text-text-muted" : ""} />
            <span className={`text-[15px] font-semibold tabular-nums leading-none ${drifted === 0 ? "text-text-muted" : ""}`}>{drifted}</span>
          </div>
          <span className="text-[10px] text-text-muted tracking-wide uppercase mt-0.5">Drifted</span>
        </div>

        {/* Agents */}
        <div className="flex-1 flex flex-col items-center justify-center gap-0.5 px-3 py-3 min-w-0">
          <div className={`flex items-center gap-1 ${agentSet.size > 0 ? "text-brand" : "text-text-muted"}`}>
            <Bot size={13} />
            <span className="text-[15px] font-semibold tabular-nums leading-none">{agentSet.size}</span>
          </div>
          <span className="text-[10px] text-text-muted tracking-wide uppercase mt-0.5">Agents</span>
        </div>

        {/* Skills */}
        <div className="flex-1 flex flex-col items-center justify-center gap-0.5 px-3 py-3 min-w-0">
          <div className={`flex items-center gap-1 ${totalSkills > 0 ? "text-icon-skill" : "text-text-muted"}`}>
            <Code size={13} />
            <span className="text-[15px] font-semibold tabular-nums leading-none">{totalSkills}</span>
          </div>
          <span className="text-[10px] text-text-muted tracking-wide uppercase mt-0.5">Skills</span>
        </div>

        {/* MCP Servers */}
        <div className="flex-1 flex flex-col items-center justify-center gap-0.5 px-3 py-3 min-w-0">
          <div className={`flex items-center gap-1 ${totalMcp > 0 ? "text-icon-mcp" : "text-text-muted"}`}>
            <Server size={13} />
            <span className="text-[15px] font-semibold tabular-nums leading-none">{totalMcp}</span>
          </div>
          <span className="text-[10px] text-text-muted tracking-wide uppercase mt-0.5">MCP Servers</span>
        </div>
      </div>

      {/* Sync health bar — only shown when we have drift data for at least one project */}
      {checking < total && (
        <div className="border-t border-border-strong/30 px-4 py-2 flex items-center gap-3">
          <span className="text-[10px] text-text-muted uppercase tracking-wider flex-shrink-0">Sync health</span>
          <div className="flex-1 h-1.5 rounded-full overflow-hidden flex" style={{ background: "var(--health-checking)" }}>
            {syncedPct > 0 && (
              <div
                className="h-full transition-all"
                style={{ width: `${syncedPct}%`, background: "var(--health-synced)" }}
                title={`${synced} synced`}
              />
            )}
            {driftedPct > 0 && (
              <div
                className="h-full transition-all"
                style={{ width: `${driftedPct}%`, background: "var(--health-drifted)" }}
                title={`${drifted} drifted`}
              />
            )}
            {checkingPct > 0 && (
              <div
                className="h-full transition-all"
                style={{ width: `${checkingPct}%`, background: "var(--health-checking)" }}
                title={`${checking} checking`}
              />
            )}
          </div>
          <div className="flex items-center gap-2.5 flex-shrink-0 text-[10px]">
            {synced > 0 && (
              <span style={{ color: "var(--health-synced)" }}>{syncedPct}% synced</span>
            )}
            {drifted > 0 && (
              <span style={{ color: "var(--health-drifted)" }}>{drifted} drifted</span>
            )}
            {checking > 0 && <span className="text-text-muted">{checking} checking…</span>}
            {fullyConfigured < total && (
              <span className="text-text-muted">{total - fullyConfigured} unconfigured</span>
            )}
          </div>
        </div>
      )}
    </div>
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

interface DashboardRecommendation {
  id: number;
  project: string;
  kind: string;
  title: string;
  body: string;
  priority: "low" | "normal" | "high";
  status: "pending" | "dismissed" | "actioned";
  source: string;
  created_at: string;
  updated_at: string;
}

export default function Dashboard({ onNavigate, onNavigateToSkillStore, onNavigateToMcpMarketplace, onNavigateToTemplateMarketplace }: DashboardProps) {
  const [projects, setProjects] = useState<Project[]>([]);
  const [mcpServerCount, setMcpServerCount] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  // Map of project name → drift report (undefined = not yet checked, null = not applicable)
  const [driftMap, setDriftMap] = useState<Record<string, DriftReport | null>>({});
  const [activityEntries, setActivityEntries] = useState<ActivityEntry[]>([]);
  const [loadingActivity, setLoadingActivity] = useState(true);
  const [recommendations, setRecommendations] = useState<DashboardRecommendation[]>([]);

  // Getting-started flags — persisted in settings.json via the backend.
  const [skillInstalled, setSkillInstalled] = useState(false);
  const [templateImported, setTemplateImported] = useState(false);
  const [welcomeDismissed, setWelcomeDismissed] = useState(false);

  useEffect(() => {
    loadData();
  }, []);

  const loadActivity = async () => {
    setLoadingActivity(true);
    try {
      const raw: string = await invoke("get_all_activity", { limit: 100 });
      setActivityEntries(JSON.parse(raw) as ActivityEntry[]);
    } catch (e) {
      console.error("Failed to load global activity:", e);
      setActivityEntries([]);
    } finally {
      setLoadingActivity(false);
    }
  };

  const loadData = async () => {
    setLoading(true);
    // Load activity independently so it doesn't block the main data load
    loadActivity();
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
      setWelcomeDismissed(!!settings?.welcome_dismissed);
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

      // Check drift for projects that have a directory and at least one agent configured,
      // and load all pending recommendations in parallel.
      const driftResults: Record<string, DriftReport | null> = {};
      const [, recs] = await Promise.all([
        Promise.all(
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
        ),
        invoke<DashboardRecommendation[]>("list_all_pending_recommendations").catch(() => [] as DashboardRecommendation[]),
      ]);
      setDriftMap(driftResults);
      setRecommendations(recs);
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

  const handleDismissWelcome = async () => {
    setWelcomeDismissed(true);
    try {
      await invoke("dismiss_welcome");
    } catch (e) {
      console.error("Failed to persist welcome dismissal:", e);
    }
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

        {/* Projects Health Bar */}
        {projects.length > 0 && (
          <ProjectsHealthBar
            projects={projects.map(p => p.name)}
            projectDetails={new Map(projects.map(p => [p.name, p]))}
            driftByProject={driftMap}
          />
        )}

        {/* Two-column grid: Left = Welcome + Activity | Right = Projects + Onboarding */}
        <div className="grid grid-cols-[1fr_320px] gap-6 items-start">

          {/* Left column: Welcome (dismissable) → Activity (hidden when empty) */}
          <div className="flex flex-col gap-4">

            {!welcomeDismissed && (
              <div className="bg-bg-input border border-border-strong/40 rounded-lg p-5 text-[13px] text-text-base leading-relaxed">
                <div className="flex items-start justify-between gap-4">
                  <div className="space-y-3 flex-1">
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
                    <p>We've got exciting plans for Automatic, and we can't wait to show you what we're working on.</p>
                    <p className="text-text-base font-medium">— Chris</p>
                  </div>
                  <button
                    onClick={handleDismissWelcome}
                    className="text-[12px] text-text-muted hover:text-text-base transition-colors shrink-0 underline decoration-text-muted/40 hover:decoration-text-base/60 mt-0.5"
                  >
                    Dismiss
                  </button>
                </div>
              </div>
            )}

            {/* Recommendations banner — compact callout when there are pending items */}
            {recommendations.length > 0 && (
              <div className="flex items-center gap-3 px-4 py-3 rounded-lg bg-warning/5 border border-warning/25">
                <Lightbulb size={14} className="text-warning shrink-0" />
                <p className="flex-1 text-[12px] text-text-muted leading-snug">
                  <span className="font-semibold text-text-base">
                    {recommendations.length === 1 ? "1 recommendation" : `${recommendations.length} recommendations`}
                  </span>
                  {" "}available across your projects.
                </p>
                <button
                  onClick={() => onNavigate("recommendations")}
                  className="shrink-0 flex items-center gap-1 text-[12px] font-medium text-warning hover:text-warning-hover transition-colors"
                >
                  Review <ArrowRight size={11} />
                </button>
              </div>
            )}

            {activityEntries.length > 0 && (
              <GlobalActivityFeed
                entries={activityEntries.slice(0, 6)}
                loading={loadingActivity}
              />
            )}

          </div>

          {/* Right column: Recent Projects (hidden when empty) → Onboarding checklist */}
          <div className="flex flex-col gap-4">

            {projects.length > 0 && (
              <div>
                <div className="flex items-center justify-between mb-3">
                  <div className="flex items-center gap-2">
                    <FolderOpen size={13} className="text-text-muted" />
                    <h2 className="text-[13px] font-semibold text-text-muted tracking-wide uppercase">Recent Projects</h2>
                  </div>
                  <button
                    onClick={() => onNavigate("projects")}
                    className="text-xs text-brand hover:text-brand-hover flex items-center gap-1 transition-colors"
                  >
                    View all <ArrowRight size={12} />
                  </button>
                </div>
                <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden">
                <div className="divide-y divide-border-strong/30">
                  {projects
                    .slice()
                    .sort((a, b) => {
                      const latestFor = (p: typeof a) => {
                        const entry = activityEntries.find(e => e.project === p.name);
                        return entry ? new Date(entry.timestamp).getTime() : new Date(p.updated_at).getTime();
                      };
                      return latestFor(b) - latestFor(a);
                    })
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
                          className={`flex items-center gap-3 px-4 h-[42px] cursor-pointer transition-colors group ${
                            isDrifted ? "hover:bg-warning/5" : "hover:bg-surface-hover"
                          }`}
                        >
                          <FolderOpen size={14} className={`shrink-0 ${isDrifted ? "text-warning" : "text-icon-agent"}`} />
                          <span className={`text-[12px] font-medium text-text-base truncate min-w-0 transition-colors ${
                            isDrifted ? "group-hover:text-warning" : "group-hover:text-icon-agent"
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
              </div>
            )}

            {/* Onboarding checklist — only shown while any item is incomplete */}
            {(() => {
              const items = [
                {
                  show: projects.length === 0,
                  label: "Create a project",
                  description: "Link a directory to an agent configuration.",
                  action: () => onNavigate("projects"),
                  color: "text-brand",
                },
                {
                  show: !templateImported,
                  label: "Import a template",
                  description: "Start from a pre-built project setup.",
                  action: () => onNavigate("template-marketplace"),
                  color: "text-icon-file-template",
                },
                {
                  show: !skillInstalled,
                  label: "Install a skill",
                  description: "Load specialised capabilities into your agents.",
                  action: () => onNavigate("skill-store"),
                  color: "text-icon-skill",
                },
                {
                  show: mcpServerCount === 0,
                  label: "Connect MCP servers",
                  description: "Extend your agents with powerful integrations.",
                  action: () => onNavigate("mcp-marketplace"),
                  color: "text-icon-mcp",
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
                        className="w-full flex items-start gap-3 px-4 py-2.5 text-left transition-all group hover:bg-surface-hover"
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
          <h2 className="text-[13px] font-semibold text-text-muted tracking-wide uppercase mb-4">Discover &amp; Extend</h2>
          <div className="grid grid-cols-3 gap-4">
            {/* Skills Marketplace */}
            <button
              onClick={() => onNavigate("skill-store")}
              className="bg-bg-input border border-border-strong/40 rounded-xl p-5 text-left hover:border-icon-skill/50 hover:bg-surface-hover transition-all group flex flex-col"
            >
              <div className="flex items-start gap-3 mb-3">
                <div className="p-2 bg-icon-skill/10 rounded-lg border border-icon-skill/20 flex-shrink-0 group-hover:bg-icon-skill/20 transition-colors">
                  <Code size={18} className="text-icon-skill" />
                </div>
                <h3 className="text-[14px] font-semibold text-text-base leading-snug pt-1">Skills Marketplace</h3>
              </div>
              <p className="text-[12px] text-text-muted leading-relaxed flex-1 mb-4">Discover and install pre-built skills, prompts, and workflows from the community.</p>
              <button
                onClick={() => onNavigate("skill-store")}
                className="w-full flex items-center justify-between gap-2 px-3 py-2 rounded-lg text-left text-[12px] font-medium transition-all group/btn bg-bg-sidebar border border-transparent hover:border-border-strong/60 hover:bg-surface-hover text-icon-skill"
              >
                <span className="text-text-base group-hover/btn:text-text-base transition-colors">Browse Skills</span>
                <ArrowRight size={11} className="flex-shrink-0 opacity-40 group-hover/btn:opacity-100 group-hover/btn:translate-x-0.5 transition-all text-icon-skill" />
              </button>
            </button>

            {/* Templates Marketplace */}
            <button
              onClick={() => onNavigate("template-marketplace")}
              className="bg-bg-input border border-border-strong/40 rounded-xl p-5 text-left hover:border-icon-file-template/50 hover:bg-surface-hover transition-all group flex flex-col"
            >
              <div className="flex items-start gap-3 mb-3">
                <div className="p-2 bg-icon-file-template/10 rounded-lg border border-icon-file-template/20 flex-shrink-0 group-hover:bg-icon-file-template/20 transition-colors">
                  <Layers size={18} className="text-icon-file-template" />
                </div>
                <h3 className="text-[14px] font-semibold text-text-base leading-snug pt-1">Templates Marketplace</h3>
              </div>
              <p className="text-[12px] text-text-muted leading-relaxed flex-1 mb-4">Explore project templates and file scaffolds to jumpstart your development.</p>
              <button
                onClick={() => onNavigate("template-marketplace")}
                className="w-full flex items-center justify-between gap-2 px-3 py-2 rounded-lg text-left text-[12px] font-medium transition-all group/btn bg-bg-sidebar border border-transparent hover:border-border-strong/60 hover:bg-surface-hover text-icon-file-template"
              >
                <span className="text-text-base group-hover/btn:text-text-base transition-colors">Browse Templates</span>
                <ArrowRight size={11} className="flex-shrink-0 opacity-40 group-hover/btn:opacity-100 group-hover/btn:translate-x-0.5 transition-all text-icon-file-template" />
              </button>
            </button>

            {/* MCP Servers Marketplace */}
            <button
              onClick={() => onNavigate("mcp-marketplace")}
              className="bg-bg-input border border-border-strong/40 rounded-xl p-5 text-left hover:border-icon-mcp/50 hover:bg-surface-hover transition-all group flex flex-col"
            >
              <div className="flex items-start gap-3 mb-3">
                <div className="p-2 bg-icon-mcp/10 rounded-lg border border-icon-mcp/20 flex-shrink-0 group-hover:bg-icon-mcp/20 transition-colors">
                  <Server size={18} className="text-icon-mcp" />
                </div>
                <h3 className="text-[14px] font-semibold text-text-base leading-snug pt-1">MCP Servers Marketplace</h3>
              </div>
              <p className="text-[12px] text-text-muted leading-relaxed flex-1 mb-4">Connect AI-powered integrations and extend your agents with MCP servers.</p>
              <button
                onClick={() => onNavigate("mcp-marketplace")}
                className="w-full flex items-center justify-between gap-2 px-3 py-2 rounded-lg text-left text-[12px] font-medium transition-all group/btn bg-bg-sidebar border border-transparent hover:border-border-strong/60 hover:bg-surface-hover text-icon-mcp"
              >
                <span className="text-text-base group-hover/btn:text-text-base transition-colors">Browse Servers</span>
                <ArrowRight size={11} className="flex-shrink-0 opacity-40 group-hover/btn:opacity-100 group-hover/btn:translate-x-0.5 transition-all text-icon-mcp" />
              </button>
            </button>
          </div>
        </div>

      </div>
    </div>
  );
}
