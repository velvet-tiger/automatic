import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  ArrowRight,
  Layers,
  PackageOpen,
  Puzzle,
  Server,
  Sparkles,
} from "lucide-react";

interface DiscoverProps {
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
  /**
   * Suffix shown after the numeric count, e.g. "skills available" or
   * "featured on skills.sh".
   */
  countLabel: string;
  ctaLabel: string;
}

type Counts = {
  collections: number | null;
  templates: number | null;
  skills: number | null;
  mcp: number | null;
};

const ASSET_CARDS: AssetCard[] = [
  {
    tab: "discover-collections",
    title: "Collections",
    description:
      "Curated bundles of skills, MCP servers, and templates grouped around a workflow or stack. Install everything in one click.",
    icon: PackageOpen,
    classes: {
      cardHover: "hover:border-icon-agent/50",
      iconBg: "bg-icon-agent/10",
      iconBgHover: "group-hover:bg-icon-agent/20",
      iconBorder: "border-icon-agent/20",
      iconText: "text-icon-agent",
      countText: "text-icon-agent",
      ctaText: "text-icon-agent",
    },
    countKey: "collections",
    countLabel: "collections available",
    ctaLabel: "Browse collections",
  },
  {
    tab: "discover-templates",
    title: "Project Templates",
    description:
      "Pre-built project configurations for common stacks. Import a template to scaffold a new project with the right tools already wired up.",
    icon: Layers,
    classes: {
      cardHover: "hover:border-icon-file-template/50",
      iconBg: "bg-icon-file-template/10",
      iconBgHover: "group-hover:bg-icon-file-template/20",
      iconBorder: "border-icon-file-template/20",
      iconText: "text-icon-file-template",
      countText: "text-icon-file-template",
      ctaText: "text-icon-file-template",
    },
    countKey: "templates",
    countLabel: "templates available",
    ctaLabel: "Browse templates",
  },
  {
    tab: "skill-store",
    title: "Skills",
    description:
      "Reusable instructions, prompts, and workflows that load directly into agent context. Searched live from the skills.sh community registry.",
    icon: Puzzle,
    classes: {
      cardHover: "hover:border-icon-skill/50",
      iconBg: "bg-icon-skill/10",
      iconBgHover: "group-hover:bg-icon-skill/20",
      iconBorder: "border-icon-skill/20",
      iconText: "text-icon-skill",
      countText: "text-icon-skill",
      ctaText: "text-icon-skill",
    },
    countKey: "skills",
    countLabel: "featured on skills.sh",
    ctaLabel: "Browse skills",
  },
  {
    tab: "discover-mcp",
    title: "MCP Servers",
    description:
      "Model Context Protocol servers that give your agents new capabilities — databases, APIs, search, and more. Add to a project in one click.",
    icon: Server,
    classes: {
      cardHover: "hover:border-icon-mcp/50",
      iconBg: "bg-icon-mcp/10",
      iconBgHover: "group-hover:bg-icon-mcp/20",
      iconBorder: "border-icon-mcp/20",
      iconText: "text-icon-mcp",
      countText: "text-icon-mcp",
      ctaText: "text-icon-mcp",
    },
    countKey: "mcp",
    countLabel: "servers available",
    ctaLabel: "Browse servers",
  },
];

export default function Discover({ onNavigate }: DiscoverProps) {
  const [counts, setCounts] = useState<Counts>({
    collections: null,
    templates: null,
    skills: null,
    mcp: null,
  });

  useEffect(() => {
    async function loadCounts() {
      const collections = await safeCount(() =>
        invoke<string>("search_collections", { query: "" })
      );
      const templates = await safeCount(() =>
        invoke<string>("list_bundled_templates")
      );
      const mcp = await safeCount(() =>
        invoke<string>("search_discover_mcp", { query: "" })
      );
      // Skills come from a static asset bundle imported at runtime.
      let skills: number | null = null;
      try {
        const mod = await import(
          "../../../src-tauri/assets/discover/featured-skills.json"
        );
        const data = (mod.default ?? mod) as unknown[];
        skills = Array.isArray(data) ? data.length : null;
      } catch {
        skills = null;
      }
      setCounts({ collections, templates, skills, mcp });
    }
    loadCounts();
  }, []);

  return (
    <div className="flex-1 h-full overflow-y-auto p-8 custom-scrollbar bg-bg-base">
      <div className="max-w-5xl mx-auto space-y-8">
        {/* Header */}
        <div className="flex items-start gap-4">
          <div className="p-3 rounded-xl bg-brand/10 border border-brand/20 shrink-0">
            <Sparkles size={20} className="text-brand" />
          </div>
          <div>
            <h1 className="text-2xl font-semibold text-text-base mb-2">Discover</h1>
            <p className="text-text-muted text-[13px] leading-relaxed max-w-2xl">
              Browse community-curated catalogues of skills, project templates, MCP servers,
              and ready-made collections. Anything you install from Discover lands in your
              local Library, where you can edit, sync to projects, or remove it at any time.
            </p>
          </div>
        </div>

        {/* Asset cards */}
        <div className="grid grid-cols-2 gap-4">
          {ASSET_CARDS.map((card) => {
            const Icon = card.icon;
            const count = counts[card.countKey];
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
            The Collections, Templates, and MCP Servers catalogues ship with each
            Automatic release and refresh on upgrade. Skills are searched live against
            the <span className="text-text-base">skills.sh</span> community registry.
            Anything you install lands in your local Library, where you can edit it,
            sync it to projects, or remove it at any time.
          </p>
        </div>
      </div>
    </div>
  );
}

async function safeCount(fetch: () => Promise<string>): Promise<number | null> {
  try {
    const json = await fetch();
    const data = JSON.parse(json) as unknown[];
    return Array.isArray(data) ? data.length : null;
  } catch {
    return null;
  }
}
