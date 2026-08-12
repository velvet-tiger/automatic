import { ArrowRight, FlaskConical, Hash, Lightbulb, ServerCog, Sparkles, Wrench } from "lucide-react";
import { flag } from "../../lib/flags";
import { usePlugin } from "../../plugins/usePlugin";

interface ToolsHomeProps {
  onNavigate: (tab: string) => void;
}

interface ToolCard {
  tab: string;
  title: string;
  description: string;
  icon: React.ElementType;
  classes: {
    cardHover: string;
    iconBg: string;
    iconBgHover: string;
    iconBorder: string;
    iconText: string;
    ctaText: string;
  };
}

const BASE_CARDS: ToolCard[] = [
  {
    tab: "library-generator",
    title: "Library Generator",
    description:
      "Generate a new skill, command, rule, or sub-agent from a short description, then review and save.",
    icon: Sparkles,
    classes: {
      cardHover: "hover:border-brand/50",
      iconBg: "bg-brand/10",
      iconBgHover: "group-hover:bg-brand/20",
      iconBorder: "border-brand/20",
      iconText: "text-brand",
      ctaText: "text-brand",
    },
  },
  {
    tab: "token-estimator",
    title: "Token Estimator",
    description:
      "Estimate token counts and per-call costs for files across Anthropic, OpenAI, and Google models.",
    icon: Hash,
    classes: {
      cardHover: "hover:border-icon-skill/50",
      iconBg: "bg-icon-skill/10",
      iconBgHover: "group-hover:bg-icon-skill/20",
      iconBorder: "border-icon-skill/20",
      iconText: "text-icon-skill",
      ctaText: "text-icon-skill",
    },
  },
  {
    tab: "recommendations",
    title: "Insights",
    description:
      "Review tailored recommendations for skills, MCP servers, templates, and collections based on your projects.",
    icon: Lightbulb,
    classes: {
      cardHover: "hover:border-icon-rule/50",
      iconBg: "bg-icon-rule/10",
      iconBgHover: "group-hover:bg-icon-rule/20",
      iconBorder: "border-icon-rule/20",
      iconText: "text-icon-rule",
      ctaText: "text-icon-rule",
    },
  },
];

const DEV_SERVERS_CARD: ToolCard = {
  tab: "dev-servers",
  title: "Servers",
  description:
    "Start, stop, and monitor npm, pnpm, and yarn dev servers across all your projects.",
  icon: ServerCog,
  classes: {
    cardHover: "hover:border-icon-mcp/50",
    iconBg: "bg-icon-mcp/10",
    iconBgHover: "group-hover:bg-icon-mcp/20",
    iconBorder: "border-icon-mcp/20",
    iconText: "text-icon-mcp",
    ctaText: "text-icon-mcp",
  },
};

const PLAYGROUND_CARD: ToolCard = {
  tab: "ai-playground",
  title: "AI Playground",
  description:
    "Chat directly with Anthropic, OpenAI, or Google models without leaving Automatic.",
  icon: FlaskConical,
  classes: {
    cardHover: "hover:border-icon-agent/50",
    iconBg: "bg-icon-agent/10",
    iconBgHover: "group-hover:bg-icon-agent/20",
    iconBorder: "border-icon-agent/20",
    iconText: "text-icon-agent",
    ctaText: "text-icon-agent",
  },
};

export default function ToolsHome({ onNavigate }: ToolsHomeProps) {
  const devServersEnabled = usePlugin("dev-servers");

  const cards: ToolCard[] = [
    ...BASE_CARDS,
    ...(devServersEnabled ? [DEV_SERVERS_CARD] : []),
    ...(flag("ai_playground") ? [PLAYGROUND_CARD] : []),
  ];

  return (
    <div className="flex-1 h-full overflow-y-auto p-8 custom-scrollbar bg-bg-base">
      <div className="max-w-5xl mx-auto space-y-8">
        {/* Header */}
        <div className="flex items-start gap-4">
          <div className="p-3 rounded-xl bg-brand/10 border border-brand/20 shrink-0">
            <Wrench size={20} className="text-brand" />
          </div>
          <div>
            <h1 className="text-2xl font-semibold text-text-base mb-2">Tools</h1>
            <p className="text-text-muted text-[13px] leading-relaxed max-w-2xl">
              Utilities that run inside Automatic — quick estimators, model chats, and
              other helpers that don't belong to a single project.
            </p>
          </div>
        </div>

        {/* Tool cards */}
        <div className="grid grid-cols-2 gap-4">
          {cards.map((card) => {
            const Icon = card.icon;
            const c = card.classes;
            return (
              <button
                key={card.tab}
                onClick={() => onNavigate(card.tab)}
                className={`bg-bg-input border border-border-strong/40 rounded-xl p-6 text-left ${c.cardHover} hover:bg-surface-hover transition-all group flex flex-col`}
              >
                <div className="flex items-start gap-3 mb-3">
                  <div
                    className={`p-2.5 ${c.iconBg} rounded-lg border ${c.iconBorder} flex-shrink-0 ${c.iconBgHover} transition-colors`}
                  >
                    <Icon size={20} className={c.iconText} />
                  </div>
                  <div className="flex-1 min-w-0">
                    <h3 className="text-[15px] font-semibold text-text-base leading-snug pt-0.5">
                      {card.title}
                    </h3>
                  </div>
                </div>
                <p className="text-[12px] text-text-muted leading-relaxed flex-1 mb-4">
                  {card.description}
                </p>
                <div
                  className={`w-full flex items-center justify-between gap-2 px-3 py-2 rounded-lg text-[12px] font-medium transition-all group/btn bg-bg-sidebar border border-transparent hover:border-border-strong/60 hover:bg-surface-hover ${c.ctaText}`}
                >
                  <span className="text-text-base">Open</span>
                  <ArrowRight
                    size={11}
                    className={`flex-shrink-0 opacity-40 group-hover/btn:opacity-100 group-hover/btn:translate-x-0.5 transition-all ${c.ctaText}`}
                  />
                </div>
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}
