import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Star,
  ExternalLink,
  ArrowLeft,
  ArrowRight,
  Globe,
  Github,
  Puzzle,
  Download,
  Clock,
  Loader2,
} from "lucide-react";
import { SkillAvatar } from "../../components/SkillAvatar";
import { usePlugin } from "../../plugins/usePlugin";
import { openExternalUrl } from "../../lib/externalLinks";

// ── Types ──────────────────────────────────────────────────────────────────

interface FeaturedAuthor {
  name: string;
  url: string;
}

interface MarketplaceTarget {
  tab: string;
  id: string;
}

interface AppTarget {
  tab: string;
  label: string;
}

/** Plugin-specific navigation: goes to Library > Tools if enabled, Settings > Plugins if not. */
interface PluginTarget {
  plugin_id: string;
}

interface FeaturedCreator {
  name: string;
  bio: string | null;
  picture: string | null;
  url: string | null;
}

interface FeaturedItem {
  id: string;
  name: string;
  type: string;
  description: string;
  about: string;
  icon: string | null;
  author: FeaturedAuthor;
  creator: FeaturedCreator | null;
  links: {
    website: string | null;
    github: string | null;
  };
  marketplace_target: MarketplaceTarget | null;
  app_target: AppTarget | null;
  plugin_target: PluginTarget | null;
  external_url: string | null;
  tags: string[];
  integration_highlights?: string[];
  usage_steps?: string[];
  placeholder?: boolean;
}

// ── Helpers ────────────────────────────────────────────────────────────────

const BRANDFETCH_CLIENT_ID = import.meta.env.VITE_BRANDFETCH_CLIENT_ID as string | undefined;

function brandfetchUrl(domain: string, px: number): string {
  const s = Math.min(px * 2, 64);
  return `https://cdn.brandfetch.io/${encodeURIComponent(domain)}/w/${s}/h/${s}/theme/dark/fallback/lettermark/type/icon?c=${BRANDFETCH_CLIENT_ID ?? ""}`;
}

const TYPE_LABELS: Record<string, string> = {
  skill: "Skill",
  collection: "Collection",
  template: "Template",
  "mcp-server": "MCP Server",
  command: "Command",
  instruction: "Instruction",
  rule: "Rule",
  "sub-agent": "Sub-Agent",
  hook: "Hook",
  plugin: "Plugin",
  external: "External",
};

function typeLabel(type: string): string {
  return TYPE_LABELS[type] ?? type.charAt(0).toUpperCase() + type.slice(1);
}

// ── Icon component ─────────────────────────────────────────────────────────

function FeaturedIcon({ item, size }: { item: FeaturedItem; size: number }) {
  const [imgError, setImgError] = useState(false);

  // icon is either a direct URL (https://...) or a Brandfetch domain (e.g. "example.com")
  const isUrl = item.icon?.startsWith("http");
  const src = item.icon
    ? (isUrl ? item.icon : (BRANDFETCH_CLIENT_ID ? brandfetchUrl(item.icon, size) : null))
    : null;

  if (src && !imgError) {
    return (
      <img
        src={src}
        alt={item.name}
        width={size}
        height={size}
        onError={() => setImgError(true)}
        className="flex-shrink-0 rounded-full object-cover"
      />
    );
  }

  return <SkillAvatar name={item.name} kind="bundled" size={size} />;
}

// ── Plugin-aware navigation button ─────────────────────────────────────────

function PluginButton({
  pluginId,
  onNavigateToTab,
  onNavigateToSettings,
}: {
  pluginId: string;
  onNavigateToTab: (tab: string) => void;
  onNavigateToSettings?: (page: string) => void;
}) {
  const isEnabled = usePlugin(pluginId);

  if (isEnabled) {
    return (
      <button
        onClick={() => onNavigateToTab("tools")}
        className="flex items-center gap-1.5 px-4 py-2 rounded-lg text-[13px] font-medium bg-bg-sidebar border border-border-strong/40 text-text-base hover:bg-surface-hover transition-colors"
      >
        <Puzzle size={14} />
        View Plugin
      </button>
    );
  }

  return (
    <button
      onClick={() => onNavigateToSettings?.("plugins")}
      className="flex items-center gap-1.5 px-4 py-2 rounded-lg text-[13px] font-medium bg-brand hover:bg-brand-hover text-white transition-colors"
    >
      <Download size={14} />
      Enable Plugin
    </button>
  );
}

// ── Simple markdown link renderer ──────────────────────────────────────────

/** Parse inline markdown links within a line, returning React nodes. */
function renderInlineLinks(line: string, keyPrefix: string): React.ReactNode[] {
  const parts: React.ReactNode[] = [];
  const linkRegex = /\[([^\]]+)\]\(([^)]+)\)/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = linkRegex.exec(line)) !== null) {
    if (match.index > lastIndex) {
      parts.push(line.slice(lastIndex, match.index));
    }
    const linkUrl = match[2]!;
    const linkText = match[1]!;
    parts.push(
      <a
        key={`${keyPrefix}-${match.index}`}
        href={linkUrl}
        onClick={(e) => {
          e.preventDefault();
          e.stopPropagation();
          void openExternalUrl(linkUrl);
        }}
        className="text-brand hover:text-brand-hover underline underline-offset-2"
      >
        {linkText}
      </a>
    );
    lastIndex = match.index + match[0].length;
  }

  if (lastIndex < line.length) {
    parts.push(line.slice(lastIndex));
  }

  return parts.length > 0 ? parts : [line];
}

/** Render about text as paragraphs, bullet lists, and inline links. */
function renderAbout(text: string): React.ReactNode[] {
  // Split into paragraphs on double newlines
  const blocks = text.split(/\n\n+/);
  const elements: React.ReactNode[] = [];

  for (let b = 0; b < blocks.length; b++) {
    const block = blocks[b]!.trim();
    if (!block) continue;

    const lines = block.split("\n");

    // Check if this block is a bullet list (all lines start with "- ")
    const isList = lines.every((l) => l.trimStart().startsWith("- "));

    if (isList) {
      elements.push(
        <ul key={`block-${b}`} className="list-disc list-inside space-y-1 pl-1">
          {lines.map((line, li) => {
            const content = line.trimStart().slice(2); // strip "- "
            return (
              <li key={li}>
                {renderInlineLinks(content, `b${b}-l${li}`)}
              </li>
            );
          })}
        </ul>
      );
    } else {
      // Regular paragraph — join lines with spaces
      const joined = lines.join(" ");
      elements.push(
        <p key={`block-${b}`}>
          {renderInlineLinks(joined, `b${b}`)}
        </p>
      );
    }
  }

  return elements;
}

// ── Props ──────────────────────────────────────────────────────────────────

interface FeaturedProps {
  onNavigateToTab?: (tab: string) => void;
  onNavigateToSettings?: (page: string) => void;
}

// ── Component ──────────────────────────────────────────────────────────────

export default function Featured({ onNavigateToTab, onNavigateToSettings }: FeaturedProps) {
  const [items, setItems] = useState<FeaturedItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const selected = items.find((i) => i.id === selectedId) ?? null;

  useEffect(() => {
    let cancelled = false;
    async function load() {
      try {
        const json = await invoke<string>("get_featured_community");
        if (!cancelled) {
          setItems(JSON.parse(json) as FeaturedItem[]);
        }
      } catch (e) {
        console.error("[community] Failed to load featured items:", e);
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    load();
    return () => { cancelled = true; };
  }, []);

  // ── Loading state ──────────────────────────────────────────────────────
  if (loading) {
    return (
      <div className="h-full flex items-center justify-center">
        <Loader2 size={24} className="animate-spin text-text-muted" />
      </div>
    );
  }

  // ── Empty state ───────────────────────────────────────────────────────
  if (items.length === 0) {
    return (
      <div className="h-full flex flex-col items-center justify-center gap-3 text-center px-8">
        <Star size={32} className="text-text-muted/30" />
        <h2 className="text-[15px] font-semibold text-text-muted">Check back soon</h2>
        <p className="text-[13px] text-text-muted/60 max-w-sm">
          Featured community items will appear here once they're available. Make sure you're connected to the internet.
        </p>
      </div>
    );
  }

  // ── Detail view ────────────────────────────────────────────────────────
  if (selected) {
    return (
      <div className="h-full flex flex-col overflow-hidden">
        <div className="flex-1 overflow-y-auto custom-scrollbar">
          <div className="max-w-4xl mx-auto px-8 py-8">
            {/* Back button */}
            <button
              onClick={() => setSelectedId(null)}
              className="flex items-center gap-1.5 text-[12px] text-text-muted hover:text-text-base transition-colors mb-6"
            >
              <ArrowLeft size={14} />
              Back to Featured
            </button>

            {/* Header */}
            <div className="flex items-start gap-5 mb-6">
              <FeaturedIcon item={selected} size={56} />
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-3 mb-1">
                  <h1 className="text-[22px] font-bold text-text-base">{selected.name}</h1>
                  <span className="inline-flex items-center px-2.5 py-0.5 rounded-full bg-brand/10 border border-brand/20 text-[11px] font-medium text-brand">
                    {typeLabel(selected.type)}
                  </span>
                </div>
                <p className="text-[13px] text-text-muted">{selected.author.name}</p>
              </div>
              <div className="flex items-center gap-2 flex-shrink-0">
                {selected.app_target && onNavigateToTab && (
                  <button
                    onClick={() => onNavigateToTab(selected.app_target!.tab)}
                    className="flex items-center gap-1.5 px-4 py-2 rounded-lg text-[13px] font-medium bg-bg-sidebar border border-border-strong/40 text-text-base hover:bg-surface-hover transition-colors"
                  >
                    <Puzzle size={14} />
                    {selected.app_target.label}
                  </button>
                )}
                {selected.plugin_target && onNavigateToTab && (
                  <PluginButton
                    pluginId={selected.plugin_target.plugin_id}
                    onNavigateToTab={onNavigateToTab}
                    onNavigateToSettings={onNavigateToSettings}
                  />
                )}
                {selected.external_url && (
                  <button
                    onClick={() => void openExternalUrl(selected.external_url!)}
                    className="flex items-center gap-1.5 px-4 py-2 rounded-lg text-[13px] font-medium bg-bg-sidebar border border-border-strong/40 text-text-muted hover:text-text-base hover:bg-surface-hover transition-colors"
                  >
                    <ExternalLink size={14} />
                    Visit Website
                  </button>
                )}
              </div>
            </div>

            {/* Description */}
            <p className="text-[14px] text-text-base leading-relaxed mb-6">
              {selected.description}
            </p>

            {/* About */}
            <div className="mb-6">
              <h2 className="text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-3">
                About
              </h2>
              <div className="text-[13px] text-text-muted leading-relaxed space-y-3">
                {renderAbout(selected.about)}
              </div>
            </div>

            {/* Integration highlights */}
            {selected.integration_highlights && selected.integration_highlights.length > 0 && (
              <div className="mb-6">
                <h2 className="text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-3">
                  Highlights
                </h2>
                <ul className="list-disc list-inside space-y-1.5 pl-1">
                  {selected.integration_highlights.map((h, i) => (
                    <li key={i} className="text-[13px] text-text-muted leading-relaxed">
                      {h}
                    </li>
                  ))}
                </ul>
              </div>
            )}

            {/* Usage steps */}
            {selected.usage_steps && selected.usage_steps.length > 0 && (
              <div className="mb-6">
                <h2 className="text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-3">
                  Getting Started
                </h2>
                <ol className="space-y-2">
                  {selected.usage_steps.map((step, i) => (
                    <li key={i} className="flex items-start gap-3 text-[13px] text-text-muted leading-relaxed">
                      <span className="flex-shrink-0 w-5 h-5 rounded-full bg-brand/10 border border-brand/20 text-brand text-[11px] font-semibold flex items-center justify-center mt-[1px]">
                        {i + 1}
                      </span>
                      {step}
                    </li>
                  ))}
                </ol>
              </div>
            )}

            {/* Links */}
            {(selected.links.website || selected.links.github) && (
              <div className="mb-6">
                <h2 className="text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-3">
                  Links
                </h2>
                <div className="flex flex-wrap gap-2">
                  {selected.links.website && (
                    <a
                      href={selected.links.website}
                      onClick={(e) => {
                        e.preventDefault();
                        void openExternalUrl(selected.links.website!);
                      }}
                      className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-bg-sidebar border border-border-strong/40 text-[12px] text-text-muted hover:text-text-base hover:border-border-strong transition-colors"
                    >
                      <Globe size={12} />
                      Website
                    </a>
                  )}
                  {selected.links.github && (
                    <a
                      href={selected.links.github}
                      onClick={(e) => {
                        e.preventDefault();
                        void openExternalUrl(selected.links.github!);
                      }}
                      className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-bg-sidebar border border-border-strong/40 text-[12px] text-text-muted hover:text-text-base hover:border-border-strong transition-colors"
                    >
                      <Github size={12} />
                      GitHub
                    </a>
                  )}
                </div>
              </div>
            )}

            {/* Creator / Author */}
            {selected.creator && (
              <div className="mb-6">
                <h2 className="text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-3">
                  Author
                </h2>
                <div className="flex items-center gap-4 p-4 rounded-lg bg-bg-sidebar border border-border-strong/40">
                  {selected.creator.picture && (
                    <img
                      src={selected.creator.picture}
                      alt={selected.creator.name}
                      width={44}
                      height={44}
                      className="rounded-full flex-shrink-0 object-cover"
                    />
                  )}
                  <div className="flex-1 min-w-0">
                    {selected.creator.url ? (
                      <a
                        href={selected.creator.url}
                        onClick={(e) => {
                          e.preventDefault();
                          void openExternalUrl(selected.creator!.url!);
                        }}
                        className="text-[13px] font-semibold text-text-base hover:text-brand transition-colors"
                      >
                        {selected.creator.name}
                      </a>
                    ) : (
                      <span className="text-[13px] font-semibold text-text-base">
                        {selected.creator.name}
                      </span>
                    )}
                    {selected.creator.bio && (
                      <p className="text-[12px] text-text-muted leading-relaxed mt-0.5">
                        {selected.creator.bio}
                      </p>
                    )}
                  </div>
                  {selected.creator.url && (
                    <a
                      href={selected.creator.url}
                      onClick={(e) => {
                        e.preventDefault();
                        void openExternalUrl(selected.creator!.url!);
                      }}
                      className="inline-flex items-center gap-1.5 text-[11px] text-brand hover:text-brand-hover transition-colors flex-shrink-0"
                    >
                      <ExternalLink size={11} />
                      Profile
                    </a>
                  )}
                </div>
              </div>
            )}

            {/* Tags */}
            {selected.tags.length > 0 && (
              <div>
                <h2 className="text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-3">
                  Tags
                </h2>
                <div className="flex flex-wrap gap-1.5">
                  {selected.tags.map((tag) => (
                    <span
                      key={tag}
                      className="px-2.5 py-0.5 rounded-full bg-bg-sidebar border border-border-strong/40 text-[11px] text-text-muted"
                    >
                      {tag}
                    </span>
                  ))}
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    );
  }

  // ── Grid view ──────────────────────────────────────────────────────────
  return (
    <div className="h-full flex flex-col overflow-hidden">
      <div className="flex-1 overflow-y-auto custom-scrollbar">
        <div className="px-8 py-8">
          {/* Header */}
          <div className="mb-8">
            <div className="flex items-center gap-2.5 mb-2">
              <Star size={18} className="text-brand" />
              <h1 className="text-[18px] font-bold text-text-base">Featured</h1>
            </div>
            <p className="text-[13px] text-text-muted">
              Curated tools, skills, and resources from the AI coding community.
            </p>
          </div>

          {/* Grid */}
          <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-5">
            {items.map((item) =>
              item.placeholder ? (
                <div
                  key={item.id}
                  className="w-full text-left p-6 rounded-xl border border-dashed border-border-strong/30 flex flex-col items-center justify-center min-h-[220px]"
                >
                  <Clock size={24} className="text-text-muted/30 mb-3" />
                  <span className="text-[14px] font-medium text-text-muted/40">Coming Soon</span>
                  <span className="text-[11px] text-text-muted/25 mt-1">{typeLabel(item.type)}</span>
                </div>
              ) : (
                <button
                  key={item.id}
                  onClick={() => setSelectedId(item.id)}
                  className="group w-full text-left p-6 rounded-xl bg-bg-input border border-border-strong/40 hover:border-border-strong hover:bg-surface-hover transition-all flex flex-col"
                >
                  {/* Card header */}
                  <div className="flex items-start gap-3 mb-4">
                    <FeaturedIcon item={item} size={44} />
                    <div className="flex-1 min-w-0">
                      <div className="text-[15px] font-semibold text-text-base leading-snug truncate">
                        {item.name}
                      </div>
                      <div className="flex items-center gap-2 mt-0.5">
                        <span className="text-[11px] text-text-muted truncate">
                          {item.author.name}
                        </span>
                        <span className="inline-flex items-center px-1.5 py-0 rounded-full bg-brand/10 border border-brand/20 text-[10px] font-medium text-brand leading-relaxed">
                          {typeLabel(item.type)}
                        </span>
                      </div>
                    </div>
                  </div>

                  {/* Description */}
                  <p className="text-[13px] text-text-muted leading-relaxed mb-3">
                    {item.description}
                  </p>

                  {/* About preview */}
                  <p className="text-[12px] text-text-muted/70 leading-relaxed line-clamp-3 flex-1">
                    {item.about.replace(/\[([^\]]+)\]\([^)]+\)/g, "$1").replace(/^- /gm, "")}
                  </p>

                  {/* Tags */}
                  {item.tags.length > 0 && (
                    <div className="flex flex-wrap gap-1.5 mt-4">
                      {item.tags.slice(0, 3).map((tag) => (
                        <span
                          key={tag}
                          className="px-2 py-0.5 rounded-full bg-bg-sidebar border border-border-strong/40 text-[10px] text-text-muted"
                        >
                          {tag}
                        </span>
                      ))}
                    </div>
                  )}

                  {/* Footer */}
                  <div className="flex items-center justify-end mt-4 pt-3 border-t border-border-strong/40">
                    <span className="inline-flex items-center gap-1 text-[11px] text-brand group-hover:text-brand-hover transition-colors">
                      More
                      <ArrowRight size={12} />
                    </span>
                  </div>
                </button>
              )
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
