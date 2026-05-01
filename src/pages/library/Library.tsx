import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  ArrowRight,
  Bot,
  ClipboardList,
  Code,
  LayoutTemplate,
  Library as LibraryIcon,
  MessagesSquare,
  ScrollText,
  Server,
  Terminal,
  Wrench,
} from "lucide-react";

interface LibraryProps {
  onNavigate: (tab: string) => void;
}

interface AssetCard {
  /** Internal tab id this card navigates to. */
  tab: string;
  title: string;
  description: string;
  icon: React.ElementType;
  /**
   * Static Tailwind class strings. Must be written out in full so Tailwind's
   * JIT compiler can statically extract them — `bg-${token}` would not work.
   */
  classes: {
    cardHover: string;
    iconBg: string;
    iconBgHover: string;
    iconBorder: string;
    iconText: string;
    countText: string;
    ctaText: string;
  };
  /** Bucket key in `counts` for displaying the catalogue size badge. */
  countKey: keyof Counts;
  /** Suffix shown after the numeric count, e.g. "skills installed". */
  countLabel: string;
  ctaLabel: string;
}

type Counts = {
  projectTemplates: number | null;
  templates: number | null;
  rules: number | null;
  userAgents: number | null;
  commands: number | null;
  skills: number | null;
  mcp: number | null;
  providers: number | null;
  tools: number | null;
};

// Reusable Tailwind class bundles per icon token. Defined at module scope so
// every card looks consistent and Tailwind's JIT only needs to scan once.
const STYLE_FILE_TEMPLATE = {
  cardHover: "hover:border-icon-file-template/50",
  iconBg: "bg-icon-file-template/10",
  iconBgHover: "group-hover:bg-icon-file-template/20",
  iconBorder: "border-icon-file-template/20",
  iconText: "text-icon-file-template",
  countText: "text-icon-file-template",
  ctaText: "text-icon-file-template",
};
const STYLE_RULE = {
  cardHover: "hover:border-icon-rule/50",
  iconBg: "bg-icon-rule/10",
  iconBgHover: "group-hover:bg-icon-rule/20",
  iconBorder: "border-icon-rule/20",
  iconText: "text-icon-rule",
  countText: "text-icon-rule",
  ctaText: "text-icon-rule",
};
const STYLE_AGENT = {
  cardHover: "hover:border-icon-agent/50",
  iconBg: "bg-icon-agent/10",
  iconBgHover: "group-hover:bg-icon-agent/20",
  iconBorder: "border-icon-agent/20",
  iconText: "text-icon-agent",
  countText: "text-icon-agent",
  ctaText: "text-icon-agent",
};
const STYLE_SKILL = {
  cardHover: "hover:border-icon-skill/50",
  iconBg: "bg-icon-skill/10",
  iconBgHover: "group-hover:bg-icon-skill/20",
  iconBorder: "border-icon-skill/20",
  iconText: "text-icon-skill",
  countText: "text-icon-skill",
  ctaText: "text-icon-skill",
};
const STYLE_MCP = {
  cardHover: "hover:border-icon-mcp/50",
  iconBg: "bg-icon-mcp/10",
  iconBgHover: "group-hover:bg-icon-mcp/20",
  iconBorder: "border-icon-mcp/20",
  iconText: "text-icon-mcp",
  countText: "text-icon-mcp",
  ctaText: "text-icon-mcp",
};

const ASSET_CARDS: AssetCard[] = [
  {
    tab: "project-templates",
    title: "Project Templates",
    description:
      "Reusable project bundles — skills, MCP servers, rules, and instructions packaged together — that you can apply to new or existing projects.",
    icon: LayoutTemplate,
    classes: STYLE_FILE_TEMPLATE,
    countKey: "projectTemplates",
    countLabel: "templates saved",
    ctaLabel: "Manage templates",
  },
  {
    tab: "templates",
    title: "Instructions",
    description:
      "Reusable text blocks you can reference inside agent sessions for recurring prompts, checklists, and structured workflows.",
    icon: ClipboardList,
    classes: STYLE_FILE_TEMPLATE,
    countKey: "templates",
    countLabel: "instructions saved",
    ctaLabel: "Manage instructions",
  },
  {
    tab: "rules",
    title: "Rules",
    description:
      "Always-on instructions automatically prepended to every agent session. Use them for team conventions, style guides, and project constraints.",
    icon: ScrollText,
    classes: STYLE_RULE,
    countKey: "rules",
    countLabel: "rules active",
    ctaLabel: "Manage rules",
  },
  {
    tab: "user-agents",
    title: "Sub-Agents",
    description:
      "Specialised agent personas with their own context, tools, and instructions — invoked from primary agent sessions for focused tasks.",
    icon: MessagesSquare,
    classes: STYLE_AGENT,
    countKey: "userAgents",
    countLabel: "sub-agents defined",
    ctaLabel: "Manage sub-agents",
  },
  {
    tab: "commands",
    title: "Commands",
    description:
      "Custom slash commands that automate repeatable workflows. Define once, invoke from any agent that supports user commands.",
    icon: Terminal,
    classes: STYLE_SKILL,
    countKey: "commands",
    countLabel: "commands saved",
    ctaLabel: "Manage commands",
  },
  {
    tab: "skills",
    title: "Skills",
    description:
      "Markdown skill packs that load into agent context — instructions, prompts, and workflows the agent can pick up on demand.",
    icon: Code,
    classes: STYLE_SKILL,
    countKey: "skills",
    countLabel: "skills installed",
    ctaLabel: "Manage skills",
  },
  {
    tab: "mcp",
    title: "MCP Servers",
    description:
      "Model Context Protocol server connections that extend agent capabilities — databases, APIs, search, and more — synced to each project.",
    icon: Server,
    classes: STYLE_MCP,
    countKey: "mcp",
    countLabel: "servers connected",
    ctaLabel: "Manage MCP servers",
  },
  {
    tab: "agents",
    title: "Providers",
    description:
      "Connected AI providers — Claude Code, Cursor, Codex, and others — and the projects each one is wired up to.",
    icon: Bot,
    classes: STYLE_AGENT,
    countKey: "providers",
    countLabel: "providers configured",
    ctaLabel: "Manage providers",
  },
  {
    tab: "tools",
    title: "Tools",
    description:
      "Developer tools detected on this machine that agents can be connected to or invoked through — a registry of what is locally available.",
    icon: Wrench,
    classes: STYLE_MCP,
    countKey: "tools",
    countLabel: "tools detected",
    ctaLabel: "Manage tools",
  },
];

export default function Library({ onNavigate }: LibraryProps) {
  const [counts, setCounts] = useState<Counts>({
    projectTemplates: null,
    templates: null,
    rules: null,
    userAgents: null,
    commands: null,
    skills: null,
    mcp: null,
    providers: null,
    tools: null,
  });

  useEffect(() => {
    async function loadCounts() {
      const [
        projectTemplates,
        templates,
        rules,
        userAgents,
        commands,
        skills,
        mcp,
        providers,
        tools,
      ] = await Promise.all([
        safeArrayLength(() => invoke<string[]>("get_project_templates")),
        safeArrayLength(() => invoke<string[]>("get_templates")),
        safeArrayLength(() => invoke<unknown[]>("get_rules")),
        safeArrayLength(() => invoke<unknown[]>("get_user_agents")),
        safeArrayLength(() => invoke<unknown[]>("get_user_commands")),
        safeArrayLength(() => invoke<unknown[]>("get_skills")),
        safeArrayLength(() => invoke<string[]>("list_mcp_server_configs")),
        safeJsonArrayLength(() => invoke<string>("list_agents_with_projects")),
        safeArrayLength(() => invoke<string[]>("list_tools")),
      ]);
      setCounts({
        projectTemplates,
        templates,
        rules,
        userAgents,
        commands,
        skills,
        mcp,
        providers,
        tools,
      });
    }
    loadCounts();
  }, []);

  return (
    <div className="flex-1 h-full overflow-y-auto p-8 custom-scrollbar bg-bg-base">
      <div className="max-w-5xl mx-auto space-y-8">
        {/* Header */}
        <div className="flex items-start gap-4">
          <div className="p-3 rounded-xl bg-brand/10 border border-brand/20 shrink-0">
            <LibraryIcon size={20} className="text-brand" />
          </div>
          <div>
            <h1 className="text-2xl font-semibold text-text-base mb-2">Library</h1>
            <p className="text-text-muted text-[13px] leading-relaxed max-w-2xl">
              Your local set of capabilities — skills, rules, instructions, sub-agents,
              commands, MCP servers, project templates, providers, and tools. Items here
              are the source of truth for everything Automatic syncs into your projects.
            </p>
          </div>
        </div>

        {/* Asset cards */}
        <div className="grid grid-cols-3 gap-4">
          {ASSET_CARDS.map((card) => {
            const Icon = card.icon;
            const count = counts[card.countKey];
            const c = card.classes;
            return (
              <button
                key={card.tab}
                onClick={() => onNavigate(card.tab)}
                className={`bg-bg-input border border-border-strong/40 rounded-xl p-5 text-left ${c.cardHover} hover:bg-surface-hover transition-all group flex flex-col`}
              >
                <div className="flex items-start gap-3 mb-3">
                  <div
                    className={`p-2 ${c.iconBg} rounded-lg border ${c.iconBorder} flex-shrink-0 ${c.iconBgHover} transition-colors`}
                  >
                    <Icon size={16} className={c.iconText} />
                  </div>
                  <div className="flex-1 min-w-0">
                    <h3 className="text-[14px] font-semibold text-text-base leading-snug pt-0.5">
                      {card.title}
                    </h3>
                    {count !== null && (
                      <p className="text-[11px] text-text-muted mt-0.5">
                        <span className={`font-semibold ${c.countText}`}>{count}</span>{" "}
                        {card.countLabel}
                      </p>
                    )}
                  </div>
                </div>
                <p className="text-[12px] text-text-muted leading-relaxed flex-1 mb-4">
                  {card.description}
                </p>
                <div
                  className={`w-full flex items-center justify-between gap-2 px-3 py-2 rounded-lg text-[12px] font-medium transition-all group/btn bg-bg-sidebar border border-transparent hover:border-border-strong/60 hover:bg-surface-hover ${c.ctaText}`}
                >
                  <span className="text-text-base">{card.ctaLabel}</span>
                  <ArrowRight
                    size={11}
                    className={`flex-shrink-0 opacity-40 group-hover/btn:opacity-100 group-hover/btn:translate-x-0.5 transition-all ${c.ctaText}`}
                  />
                </div>
              </button>
            );
          })}
        </div>

        {/* Footer note */}
        <div className="bg-bg-input border border-border-strong/40 rounded-xl p-5">
          <p className="text-[12px] text-text-muted leading-relaxed">
            Library items live on disk under <span className="text-text-base">~/.automatic/</span>{" "}
            and are synced into each project's agent configuration directories. Need
            something you don't have yet? Pick it up from the{" "}
            <button
              onClick={() => onNavigate("discover-home")}
              className="text-brand hover:underline"
            >
              Discover
            </button>{" "}
            section.
          </p>
        </div>
      </div>
    </div>
  );
}

async function safeArrayLength<T>(fetch: () => Promise<T[]>): Promise<number | null> {
  try {
    const data = await fetch();
    return Array.isArray(data) ? data.length : null;
  } catch {
    return null;
  }
}

async function safeJsonArrayLength(
  fetch: () => Promise<string>
): Promise<number | null> {
  try {
    const json = await fetch();
    const data = JSON.parse(json) as unknown[];
    return Array.isArray(data) ? data.length : null;
  } catch {
    return null;
  }
}
