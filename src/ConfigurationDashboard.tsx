import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Bot,
  Code,
  Server,
  ScrollText,
  LayoutTemplate,
  Layers,
  ArrowRight,
  RefreshCw,
  AlertCircle,
  Plus,
  Puzzle,
  Store,
} from "lucide-react";

interface ConfigSection {
  id: string;
  label: string;
  description: string;
  helpText: string;
  icon: React.ElementType;
  iconColorClass: string;
  iconBgClass: string;
  borderHoverClass: string;
  count: number | null;
  countLabel: string;
  primaryAction: {
    label: string;
    tab: string;
  };
  secondaryActions: {
    label: string;
    tab: string;
    icon: React.ElementType;
  }[];
}

interface ConfigurationDashboardProps {
  onNavigate: (tab: string) => void;
}

export default function ConfigurationDashboard({ onNavigate }: ConfigurationDashboardProps) {
  const [counts, setCounts] = useState<Record<string, number>>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    loadCounts();
  }, []);

  const loadCounts = async () => {
    setLoading(true);
    try {
      const [agentsJson, skills, mcpServers, rules, templates, projectTemplates] = await Promise.all([
        invoke<string>("list_agents_with_projects").catch(() => "[]"),
        invoke<unknown[]>("get_skills").catch(() => [] as unknown[]),
        invoke<string[]>("list_mcp_server_configs").catch(() => [] as string[]),
        invoke<unknown[]>("get_rules").catch(() => [] as unknown[]),
        invoke<string[]>("get_templates").catch(() => [] as string[]),
        invoke<string[]>("get_project_templates").catch(() => [] as string[]),
      ]);

      let agentCount = 0;
      try {
        const parsed = JSON.parse(agentsJson) as unknown[];
        agentCount = parsed.length;
      } catch {
        agentCount = 0;
      }

      setCounts({
        agents: agentCount,
        skills: skills.length,
        mcp: mcpServers.length,
        rules: rules.length,
        templates: templates.length,
        projectTemplates: projectTemplates.length,
      });
      setError(null);
    } catch (err: any) {
      setError(`Failed to load configuration data: ${err}`);
    } finally {
      setLoading(false);
    }
  };

  const SECTIONS: ConfigSection[] = [
    {
      id: "agents",
      label: "Agents",
      description: "Connected AI agents and their project associations.",
      helpText:
        "Agents are AI tools like Claude Code, Cursor, or custom bots linked to your projects. Automatic syncs configuration to each agent's expected directory.",
      icon: Bot,
      iconColorClass: "text-icon-agent",
      iconBgClass: "bg-icon-agent/10 group-hover:bg-icon-agent/20",
      borderHoverClass: "hover:border-icon-agent/50",
      count: counts.agents ?? null,
      countLabel: "configured",
      primaryAction: { label: "View Agents", tab: "agents" },
      secondaryActions: [],
    },
    {
      id: "skills",
      label: "Skills",
      description: "Reusable instructions and capabilities for your agents.",
      helpText:
        "Skills are markdown documents containing instructions, workflows, or domain knowledge loaded into agent context. Install community skills or write your own.",
      icon: Code,
      iconColorClass: "text-icon-skill",
      iconBgClass: "bg-icon-skill/10 group-hover:bg-icon-skill/20",
      borderHoverClass: "hover:border-icon-skill/50",
      count: counts.skills ?? null,
      countLabel: "installed",
      primaryAction: { label: "Manage Skills", tab: "skills" },
      secondaryActions: [
        { label: "Browse Skill Store", tab: "skill-store", icon: Store },
      ],
    },
    {
      id: "mcp",
      label: "MCP Servers",
      description: "Model Context Protocol server connections for your agents.",
      helpText:
        "MCP servers extend agent capabilities with tools like GitHub, databases, web search, and more. Automatic manages and syncs server configs across projects.",
      icon: Server,
      iconColorClass: "text-icon-mcp",
      iconBgClass: "bg-icon-mcp/10 group-hover:bg-icon-mcp/20",
      borderHoverClass: "hover:border-icon-mcp/50",
      count: counts.mcp ?? null,
      countLabel: "connected",
      primaryAction: { label: "Manage MCP Servers", tab: "mcp" },
      secondaryActions: [
        { label: "Browse MCP Marketplace", tab: "mcp-marketplace", icon: Store },
      ],
    },
    {
      id: "rules",
      label: "Rules",
      description: "Persistent instructions injected into every agent session.",
      helpText:
        "Rules are always-on instructions automatically prepended to agent context. Use them for team conventions, style guides, or project-wide constraints.",
      icon: ScrollText,
      iconColorClass: "text-icon-rule",
      iconBgClass: "bg-icon-rule/10 group-hover:bg-icon-rule/20",
      borderHoverClass: "hover:border-icon-rule/50",
      count: counts.rules ?? null,
      countLabel: "active",
      primaryAction: { label: "Manage Rules", tab: "rules" },
      secondaryActions: [],
    },
    {
      id: "templates",
      label: "Instructions",
      description: "Reusable instruction templates for agent prompts and tasks.",
      helpText:
        "Instruction templates are reusable text blocks you can reference in agent sessions. Great for recurring workflows, review checklists, or structured prompts.",
      icon: LayoutTemplate,
      iconColorClass: "text-icon-file-template",
      iconBgClass: "bg-icon-file-template/10 group-hover:bg-icon-file-template/20",
      borderHoverClass: "hover:border-icon-file-template/50",
      count: counts.templates ?? null,
      countLabel: "saved",
      primaryAction: { label: "Manage Instructions", tab: "templates" },
      secondaryActions: [],
    },
    {
      id: "project-templates",
      label: "Project Templates",
      description: "Scaffold new projects from pre-built configuration bundles.",
      helpText:
        "Project templates bundle skills, MCP servers, rules, and instructions into a reusable starting point. Create new projects in seconds with the right tools pre-configured.",
      icon: Layers,
      iconColorClass: "text-icon-file-template",
      iconBgClass: "bg-icon-file-template/10 group-hover:bg-icon-file-template/20",
      borderHoverClass: "hover:border-icon-file-template/50",
      count: counts.projectTemplates ?? null,
      countLabel: "templates",
      primaryAction: { label: "Manage Templates", tab: "project-templates" },
      secondaryActions: [
        { label: "Browse Marketplace", tab: "template-marketplace", icon: Store },
      ],
    },
  ];

  if (loading) {
    return (
      <div className="flex-1 flex items-center justify-center h-full bg-bg-base">
        <div className="flex items-center gap-2 text-text-muted">
          <RefreshCw size={16} className="animate-spin" />
          <span>Loading configuration...</span>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 h-full overflow-y-auto p-8 custom-scrollbar bg-bg-base">
      <div className="max-w-5xl mx-auto space-y-8">

        {error && (
          <div className="bg-red-500/10 text-red-400 p-4 rounded-md border border-red-500/20 flex items-start gap-3">
            <AlertCircle size={16} className="mt-0.5 shrink-0" />
            <div className="text-sm">{error}</div>
          </div>
        )}

        {/* Header */}
        <div>
          <h1 className="text-2xl font-semibold text-text-base mb-2">Configuration</h1>
          <p className="text-text-muted text-sm">
            Your local set of capabilities — skills, instructions, rules, MCP servers, and templates that power your AI agents.
          </p>
        </div>

        {/* Quick-add strip */}
        <div className="flex flex-wrap gap-2">
          {[
            { label: "New Skill", tab: "skills", icon: Code },
            { label: "New Rule", tab: "rules", icon: ScrollText },
            { label: "New Instruction", tab: "templates", icon: LayoutTemplate },
            { label: "Add MCP Server", tab: "mcp", icon: Server },
            { label: "New Project Template", tab: "project-templates", icon: Layers },
          ].map(({ label, tab }) => (
            <button
              key={tab}
              onClick={() => onNavigate(tab)}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[12px] font-medium bg-bg-input border border-border-strong/40 text-text-muted hover:text-text-base hover:border-border-strong transition-colors"
            >
              <Plus size={12} />
              {label}
            </button>
          ))}
        </div>

        {/* Section cards */}
        <div className="grid grid-cols-2 gap-4">
          {SECTIONS.map((section) => {
            const Icon = section.icon;
            return (
              <div
                key={section.id}
                className={`group bg-bg-input border border-border-strong/40 rounded-xl p-5 flex flex-col gap-4 transition-all ${section.borderHoverClass}`}
              >
                {/* Header row */}
                <div className="flex items-start justify-between gap-3">
                  <div className="flex items-center gap-3 min-w-0">
                    <div className={`p-2.5 rounded-lg transition-colors ${section.iconBgClass}`}>
                      <Icon size={18} className={section.iconColorClass} />
                    </div>
                    <div className="min-w-0">
                      <div className="text-[14px] font-semibold text-text-base leading-snug">
                        {section.label}
                      </div>
                      {section.count !== null && (
                        <div className="text-[11px] text-text-muted mt-0.5">
                          <span className={`font-semibold ${section.iconColorClass}`}>{section.count}</span>
                          {" "}{section.countLabel}
                        </div>
                      )}
                    </div>
                  </div>

                  <button
                    onClick={() => onNavigate(section.primaryAction.tab)}
                    className="flex items-center gap-1 text-[12px] text-text-muted hover:text-text-base transition-colors shrink-0"
                  >
                    {section.primaryAction.label}
                    <ArrowRight size={12} />
                  </button>
                </div>

                {/* Help text */}
                <p className="text-[12px] text-text-muted leading-relaxed">{section.helpText}</p>

                {/* Actions */}
                <div className="flex flex-wrap gap-2 border-t border-border-strong/30 pt-3">
                  <button
                    onClick={() => onNavigate(section.primaryAction.tab)}
                    className={`flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[12px] font-medium bg-surface border border-border-strong/60 hover:border-border-strong hover:bg-surface-hover transition-colors ${section.iconColorClass}`}
                  >
                    <Icon size={12} />
                    {section.primaryAction.label}
                  </button>
                  {section.secondaryActions.map((action) => {
                    const ActionIcon = action.icon;
                    return (
                      <button
                        key={action.tab}
                        onClick={() => onNavigate(action.tab)}
                        className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[12px] font-medium bg-surface border border-border-strong/60 text-text-muted hover:text-text-base hover:border-border-strong hover:bg-surface-hover transition-colors"
                      >
                        <ActionIcon size={12} />
                        {action.label}
                      </button>
                    );
                  })}
                </div>
              </div>
            );
          })}
        </div>

        {/* Marketplace callout */}
        <div className="bg-bg-input border border-border-strong/40 rounded-xl p-6">
          <div className="flex items-start gap-4">
            <div className="p-3 bg-brand/10 rounded-lg shrink-0">
              <Puzzle size={20} className="text-brand" />
            </div>
            <div className="flex-1 min-w-0">
              <h3 className="text-[14px] font-semibold text-text-base mb-1">Extend your configuration</h3>
              <p className="text-[13px] text-text-muted leading-relaxed mb-4">
                The Automatic marketplace offers community-built skills, project templates, and MCP server configs you can install with one click.
              </p>
              <div className="flex flex-wrap gap-2">
                {[
                  { label: "Browse Skills", tab: "skill-store", icon: Code },
                  { label: "Browse Templates", tab: "template-marketplace", icon: Layers },
                  { label: "Browse MCP Servers", tab: "mcp-marketplace", icon: Server },
                  { label: "Collections", tab: "collection-marketplace", icon: Store },
                ].map(({ label, tab, icon: Icon }) => (
                  <button
                    key={tab}
                    onClick={() => onNavigate(tab)}
                    className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[12px] font-medium bg-brand/10 border border-brand/20 text-brand hover:bg-brand/20 hover:border-brand/40 transition-colors"
                  >
                    <Icon size={12} />
                    {label}
                  </button>
                ))}
              </div>
            </div>
          </div>
        </div>

      </div>
    </div>
  );
}
