import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ArrowRight, Code, Compass, Copy, Layers, Server, Sparkles, Zap } from "lucide-react";

interface GettingStartedProps {
  onNavigate: (tab: string) => void;
}

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

interface OnboardingItem {
  label: string;
  description: string;
  action: () => void;
  color: string;
}

function getOnboardingItems(
  projectCount: number,
  templateImported: boolean,
  skillInstalled: boolean,
  mcpServerCount: number,
  onNavigate: (tab: string) => void
): OnboardingItem[] {
  return [
    {
      show: projectCount === 0,
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
  ].filter((i) => i.show) as OnboardingItem[];
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

function UseCasesSection({ onNavigate, onboardingItems }: GettingStartedProps & { onboardingItems: OnboardingItem[] }) {
  return (
    <div>
      <div className="flex items-center gap-2 mb-4">
        <Sparkles size={13} className="text-brand" />
        <h2 className="text-[13px] font-semibold text-text-muted tracking-wide uppercase">Getting Started</h2>
      </div>
      {onboardingItems.length > 0 && (
        <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden mb-6">
          <div className="flex items-center gap-2 px-4 pt-4 pb-3">
            <Sparkles size={12} className="text-brand" />
            <h2 className="text-sm font-semibold text-text-base">Setup checklist</h2>
          </div>
          <div className="divide-y divide-border-strong/30 border-t border-border-strong/30">
            {onboardingItems.map((item) => (
              <button
                key={item.label}
                onClick={item.action}
                className="w-full flex items-start gap-3 px-4 py-2.5 text-left transition-all group hover:bg-surface-hover"
              >
                <div className="flex-1 min-w-0">
                  <p className={`text-[12px] font-medium text-text-base`}>{item.label}</p>
                  <p className="text-[11px] text-text-muted mt-0.5 leading-relaxed">{item.description}</p>
                </div>
                <ArrowRight size={12} className={`mt-0.5 shrink-0 ${item.color} opacity-60 group-hover:opacity-100 group-hover:translate-x-0.5 transition-all`} />
              </button>
            ))}
          </div>
        </div>
      )}
      <div className="grid grid-cols-3 gap-4">
        {USE_CASES.map((uc) => (
          <div
            key={uc.title}
            className="bg-bg-input border border-border-strong/40 rounded-xl p-5 flex flex-col"
          >
            <div className="flex items-start gap-3 mb-3">
              <div className={`p-2 rounded-lg flex-shrink-0 ${uc.accentBg} border ${uc.accentBorder}`}>
                <span className={uc.accentText}>{uc.icon}</span>
              </div>
              <h3 className="text-[14px] font-semibold text-text-base leading-snug pt-1">{uc.title}</h3>
            </div>

            <p className="text-[12px] text-text-muted leading-relaxed flex-1 mb-4">{uc.goal}</p>

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

function DiscoverAndExtendSection({ onNavigate }: GettingStartedProps) {
  return (
    <div>
      <h2 className="text-[13px] font-semibold text-text-muted tracking-wide uppercase mb-4">Discover &amp; Extend</h2>
      <div className="grid grid-cols-3 gap-4">
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
          <div className="w-full flex items-center justify-between gap-2 px-3 py-2 rounded-lg text-left text-[12px] font-medium transition-all group/btn bg-bg-sidebar border border-transparent hover:border-border-strong/60 hover:bg-surface-hover text-icon-skill">
            <span className="text-text-base group-hover/btn:text-text-base transition-colors">Browse Skills</span>
            <ArrowRight size={11} className="flex-shrink-0 opacity-40 group-hover/btn:opacity-100 group-hover/btn:translate-x-0.5 transition-all text-icon-skill" />
          </div>
        </button>

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
          <div className="w-full flex items-center justify-between gap-2 px-3 py-2 rounded-lg text-left text-[12px] font-medium transition-all group/btn bg-bg-sidebar border border-transparent hover:border-border-strong/60 hover:bg-surface-hover text-icon-file-template">
            <span className="text-text-base group-hover/btn:text-text-base transition-colors">Browse Templates</span>
            <ArrowRight size={11} className="flex-shrink-0 opacity-40 group-hover/btn:opacity-100 group-hover/btn:translate-x-0.5 transition-all text-icon-file-template" />
          </div>
        </button>

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
          <div className="w-full flex items-center justify-between gap-2 px-3 py-2 rounded-lg text-left text-[12px] font-medium transition-all group/btn bg-bg-sidebar border border-transparent hover:border-border-strong/60 hover:bg-surface-hover text-icon-mcp">
            <span className="text-text-base group-hover/btn:text-text-base transition-colors">Browse Servers</span>
            <ArrowRight size={11} className="flex-shrink-0 opacity-40 group-hover/btn:opacity-100 group-hover/btn:translate-x-0.5 transition-all text-icon-mcp" />
          </div>
        </button>
      </div>
    </div>
  );
}

export default function GettingStarted({ onNavigate }: GettingStartedProps) {
  const [skillInstalled, setSkillInstalled] = useState(false);
  const [templateImported, setTemplateImported] = useState(false);
  const [projectCount, setProjectCount] = useState(0);
  const [mcpServerCount, setMcpServerCount] = useState(0);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    async function loadData() {
      try {
        const [projectNames, mcpNames, settings] = await Promise.all([
          invoke<string[]>("get_projects").catch(() => [] as string[]),
          invoke<string[]>("list_mcp_server_configs").catch(() => [] as string[]),
          invoke<any>("read_settings").catch(() => null),
        ]);

        setProjectCount(projectNames.length);
        setMcpServerCount(mcpNames.length);

        if (settings?.getting_started) {
          setSkillInstalled(!!settings.getting_started.skill_installed);
          setTemplateImported(!!settings.getting_started.template_imported);
        }
      } catch (e) {
        console.error("Failed to load getting started data:", e);
      } finally {
        setLoading(false);
      }
    }
    loadData();
  }, []);

  const onboardingItems = getOnboardingItems(projectCount, templateImported, skillInstalled, mcpServerCount, onNavigate);

  if (loading) {
    return (
      <div className="flex-1 h-full overflow-y-auto p-8 custom-scrollbar bg-transparent relative z-10">
        <div className="max-w-5xl mx-auto space-y-8">
          <div className="animate-pulse space-y-4">
            <div className="h-4 bg-bg-input rounded w-32" />
            <div className="grid grid-cols-3 gap-4">
              {[1, 2, 3].map((i) => (
                <div key={i} className="h-40 bg-bg-input rounded-xl" />
              ))}
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 h-full overflow-y-auto p-8 custom-scrollbar bg-transparent relative z-10">
      <div className="max-w-5xl mx-auto space-y-8">
        <UseCasesSection onNavigate={onNavigate} onboardingItems={onboardingItems} />
        <DiscoverAndExtendSection onNavigate={onNavigate} />
      </div>
    </div>
  );
}
