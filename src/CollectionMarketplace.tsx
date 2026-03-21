import { useState, useEffect, useCallback, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AuthorSection, type AuthorDescriptor } from "./AuthorPanel";
import { handleExternalLinkClick } from "./lib/externalLinks";
import { SkillAvatar } from "./SkillAvatar";
import {
  Search,
  ArrowLeft,
  ArrowRight,
  Download,
  CheckCircle2,
  Loader2,
  Code,
  Server,
  LayoutTemplate,
  Github,
  Package,
  Layers,
  X,
} from "lucide-react";
// ── Types ──────────────────────────────────────────────────────────────────

interface CollectionSkill {
  name: string;
  display_name: string;
  description: string;
  source: string;
  id: string;
  kind: "bundled" | "github";
}

interface CollectionMcpServer {
  name: string;
  display_name: string;
  description: string;
  config: Record<string, unknown>;
}

interface CollectionTemplate {
  name: string;
  display_name: string;
  description: string;
}

interface Collection {
  id: string;
  name: string;
  slug: string;
  description: string;
  author: AuthorDescriptor;
  icon: string | null;
  tags: string[];
  skills: CollectionSkill[];
  mcp_servers: CollectionMcpServer[];
  templates: CollectionTemplate[];
}

// collections is loaded asynchronously in the component via invoke("search_collections")

// ── Accent theme for Collections ──────────────────────────────────────────

const ACCENT_BG = "bg-icon-file-template/10";
const ACCENT_BORDER = "border-icon-file-template/20";
const ACCENT_TEXT = "text-icon-file-template";

// ── Skill source registry type ────────────────────────────────────────────

interface SkillSource {
  source: string;
  id: string;
}

// ── Collection icon ───────────────────────────────────────────────────────

const BRANDFETCH_CLIENT_ID = import.meta.env.VITE_BRANDFETCH_CLIENT_ID as string | undefined;

function brandfetchUrl(domain: string, px: number): string {
  const s = Math.min(px * 2, 64);
  return `https://cdn.brandfetch.io/${encodeURIComponent(domain)}/w/${s}/h/${s}/theme/dark/fallback/lettermark/type/icon?c=${BRANDFETCH_CLIENT_ID ?? ""}`;
}

function CollectionIcon({
  collection,
  size,
}: {
  collection: Collection;
  size: number;
}) {
  const [imgError, setImgError] = useState(false);

  // Bundled/provider collections have no meaningful remote logo — use SkillAvatar
  // which renders a letter fallback with the correct skill icon styling.
  if (collection.author.type === "provider") {
    return (
      <SkillAvatar name={collection.name} kind="bundled" size={size} />
    );
  }

  if (collection.icon && BRANDFETCH_CLIENT_ID && !imgError) {
    return (
      <img
        src={brandfetchUrl(collection.icon, size)}
        alt={collection.name}
        width={size}
        height={size}
        onError={() => setImgError(true)}
        className="flex-shrink-0 rounded-md object-contain"
        style={{ width: size, height: size }}
        draggable={false}
      />
    );
  }

  // Fallback: first letter on accent background
  const letter = collection.name.charAt(0).toUpperCase();
  const fontSize = Math.round(size * 0.42);
  return (
    <div
      className={`flex-shrink-0 rounded-md ${ACCENT_BG} border ${ACCENT_BORDER} flex items-center justify-center font-semibold ${ACCENT_TEXT}`}
      style={{ width: size, height: size, fontSize }}
      aria-hidden="true"
    >
      {letter}
    </div>
  );
}

// ── Skill row ─────────────────────────────────────────────────────────────

function SkillRow({
  skill,
  installed,
  importing,
  onImport,
}: {
  skill: CollectionSkill;
  installed: boolean;
  importing: boolean;
  onImport: () => void;
}) {
  return (
    <div className="flex items-center gap-3 px-3 py-2.5 rounded-lg bg-bg-input border border-border-strong/40">
      <SkillAvatar name={skill.display_name} source={skill.source} kind={skill.kind} size={32} />
      <div className="flex-1 min-w-0">
        <div className="text-[13px] font-medium text-text-base leading-snug">
          {skill.display_name}
        </div>
        <p className="text-[11px] text-text-muted leading-relaxed line-clamp-2 mt-0.5">
          {skill.description}
        </p>
      </div>
      <div className="flex-shrink-0">
        {installed ? (
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-icon-skill/10 border border-icon-skill/20 text-[10px] font-medium text-icon-skill">
            <CheckCircle2 size={10} />
            Installed
          </span>
        ) : (
          <button
            onClick={onImport}
            disabled={importing}
            className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-md text-[11px] font-medium transition-colors bg-brand hover:bg-brand-hover text-white disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {importing ? (
              <Loader2 size={10} className="animate-spin" />
            ) : (
              <Download size={10} />
            )}
            Import
          </button>
        )}
      </div>
    </div>
  );
}

// ── Detail view ───────────────────────────────────────────────────────────

function CollectionDetail({
  collection,
  registry,
  installedMcpServers,
  onBack,
  onRegistryUpdate,
}: {
  collection: Collection;
  registry: Record<string, SkillSource>;
  installedMcpServers: Set<string>;
  onBack: () => void;
  onRegistryUpdate: (name: string, source: string, id: string) => void;
}) {
  // Per-skill import state
  const [importingSkills, setImportingSkills] = useState<Set<string>>(new Set());
  const [skillErrors, setSkillErrors] = useState<Record<string, string>>({});

  // Import-all state
  const [importingAll, setImportingAll] = useState(false);
  const [importAllError, setImportAllError] = useState<string | null>(null);
  const [importAllDone, setImportAllDone] = useState(false);

  const isSkillInstalled = useCallback(
    (skill: CollectionSkill) =>
      Object.values(registry).some((s) => s.id === skill.id),
    [registry]
  );

  const allSkillsInstalled = useMemo(
    () => collection.skills.every((s) => isSkillInstalled(s)),
    [collection.skills, isSkillInstalled]
  );

  const importSingleSkill = useCallback(
    async (skill: CollectionSkill) => {
      setImportingSkills((prev) => new Set([...prev, skill.name]));
      setSkillErrors((prev) => { const n = { ...prev }; delete n[skill.name]; return n; });

      try {
        if (skill.kind === "bundled") {
          // Bundled skills can be installed via install_skills_from_bundle
          // which is exposed as install_local_skill equivalent; we use
          // import_remote_skill with the bundled content if available,
          // otherwise call a general install.
          // The Tauri command `import_remote_skill` works for any source/id pair;
          // for bundled skills we fetch from the backend directly.
          const content: string = await invoke("fetch_remote_skill_content", {
            source: skill.source,
            name: skill.name,
          });
          await invoke("import_remote_skill", {
            name: skill.name,
            content,
            source: skill.source,
            id: skill.id,
          });
        } else {
          // GitHub-hosted skill
          const content: string = await invoke("fetch_remote_skill_content", {
            source: skill.source,
            name: skill.name,
          });
          await invoke("import_remote_skill", {
            name: skill.name,
            content,
            source: skill.source,
            id: skill.id,
          });
        }
        onRegistryUpdate(skill.name, skill.source, skill.id);
      } catch (err: any) {
        setSkillErrors((prev) => ({ ...prev, [skill.name]: `${err}` }));
      } finally {
        setImportingSkills((prev) => {
          const n = new Set(prev);
          n.delete(skill.name);
          return n;
        });
      }
    },
    [onRegistryUpdate]
  );

  const importAll = useCallback(async () => {
    setImportingAll(true);
    setImportAllError(null);
    setImportAllDone(false);

    const notInstalled = collection.skills.filter((s) => !isSkillInstalled(s));
    let failed = 0;

    for (const skill of notInstalled) {
      try {
        const content: string = await invoke("fetch_remote_skill_content", {
          source: skill.source,
          name: skill.name,
        });
        await invoke("import_remote_skill", {
          name: skill.name,
          content,
          source: skill.source,
          id: skill.id,
        });
        onRegistryUpdate(skill.name, skill.source, skill.id);
      } catch {
        failed++;
      }
    }

    setImportingAll(false);
    if (failed > 0) {
      setImportAllError(`${failed} skill${failed !== 1 ? "s" : ""} could not be imported.`);
    } else {
      setImportAllDone(true);
    }
  }, [collection.skills, isSkillInstalled, onRegistryUpdate]);

  const totalItems =
    collection.skills.length +
    collection.mcp_servers.length +
    collection.templates.length;

  return (
    <div className="flex h-full flex-col bg-bg-base">
      {/* Header bar */}
      <div className="h-12 px-6 border-b border-border-strong/40 flex items-center justify-between flex-shrink-0">
        <div className="flex items-center gap-3">
          <button
            onClick={onBack}
            className="flex items-center gap-1.5 text-[12px] text-text-muted hover:text-text-base transition-colors"
          >
            <ArrowLeft size={13} />
            Collections
          </button>
          <span className="text-surface">/</span>
          <div className="flex items-center gap-2.5">
            <CollectionIcon collection={collection} size={24} />
            <span className="text-[14px] font-semibold text-text-base">
              {collection.name}
            </span>
          </div>
        </div>

        {/* Import all button */}
        <div className="flex items-center gap-2">
          {importAllError && (
            <span className="text-[12px] text-red-400">{importAllError}</span>
          )}
          {allSkillsInstalled || importAllDone ? (
            <div className="flex h-[26px] items-center gap-1.5 px-2.5 rounded-md text-[11px] font-medium bg-bg-sidebar border border-border-strong/40 text-icon-skill">
              <CheckCircle2 size={12} />
              All installed
            </div>
          ) : (
            <button
              onClick={importAll}
              disabled={importingAll}
              className="flex h-[26px] items-center gap-1.5 px-2.5 rounded-md text-[11px] font-medium bg-brand hover:bg-brand-hover text-white transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-sm border border-transparent"
            >
              {importingAll ? (
                <><Loader2 size={12} className="animate-spin" /> Importing…</>
              ) : (
                <><Download size={12} /> Import All</>
              )}
            </button>
          )}
        </div>
      </div>

      {/* Body */}
      <div className="flex-1 overflow-y-auto custom-scrollbar">
        <div className="max-w-3xl mx-auto px-8 py-8">

          {/* Description */}
          <p className="text-[14px] text-text-base leading-relaxed pb-6 mb-6 border-b border-border-strong/40">
            {collection.description}
          </p>

          <div className="grid grid-cols-[1fr_240px] gap-8">

            {/* Left — item lists */}
            <div className="space-y-8 min-w-0">

              {/* Skills */}
              {collection.skills.length > 0 && (
                <div>
                  <div className="flex items-center gap-2 mb-3">
                    <Code size={13} className="text-icon-skill" />
                    <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
                      Skills
                    </span>
                    <span className="text-[10px] bg-icon-skill/15 text-icon-skill px-1.5 py-0.5 rounded">
                      {collection.skills.length}
                    </span>
                  </div>
                  <div className="space-y-2">
                    {collection.skills.map((skill) => (
                      <div key={skill.id}>
                        <SkillRow
                          skill={skill}
                          installed={isSkillInstalled(skill)}
                          importing={importingSkills.has(skill.name)}
                          onImport={() => importSingleSkill(skill)}
                        />
                        {skillErrors[skill.name] && (
                          <p className="mt-1 text-[11px] text-red-400 px-1">
                            {skillErrors[skill.name]}
                          </p>
                        )}
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {/* MCP Servers */}
              {collection.mcp_servers.length > 0 && (
                <div>
                  <div className="flex items-center gap-2 mb-3">
                    <Server size={13} className="text-icon-mcp" />
                    <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
                      MCP Servers
                    </span>
                    <span className="text-[10px] bg-icon-mcp/15 text-icon-mcp px-1.5 py-0.5 rounded">
                      {collection.mcp_servers.length}
                    </span>
                  </div>
                  <div className="space-y-2">
                    {collection.mcp_servers.map((server) => {
                      const isInstalled = installedMcpServers.has(server.name);
                      return (
                        <div
                          key={server.name}
                          className="flex items-center gap-3 px-3 py-2.5 rounded-lg bg-bg-input border border-border-strong/40"
                        >
                          <div className="w-8 h-8 rounded-md bg-icon-mcp/10 border border-icon-mcp/20 flex items-center justify-center flex-shrink-0">
                            <Server size={15} className="text-icon-mcp" />
                          </div>
                          <div className="flex-1 min-w-0">
                            <div className="text-[13px] font-medium text-text-base leading-snug">
                              {server.display_name}
                            </div>
                            <p className="text-[11px] text-text-muted leading-relaxed line-clamp-2 mt-0.5">
                              {server.description}
                            </p>
                          </div>
                          <div className="flex-shrink-0">
                            {isInstalled ? (
                              <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-success/10 border border-success/20 text-[10px] font-medium text-success">
                                <CheckCircle2 size={10} />
                                Added
                              </span>
                            ) : (
                              <span className="text-[10px] text-text-muted italic">
                                see MCP Marketplace
                              </span>
                            )}
                          </div>
                        </div>
                      );
                    })}
                  </div>
                </div>
              )}

              {/* Templates */}
              {collection.templates.length > 0 && (
                <div>
                  <div className="flex items-center gap-2 mb-3">
                    <LayoutTemplate size={13} className="text-brand" />
                    <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
                      Templates
                    </span>
                    <span className="text-[10px] bg-brand/15 text-brand px-1.5 py-0.5 rounded">
                      {collection.templates.length}
                    </span>
                  </div>
                  <div className="space-y-2">
                    {collection.templates.map((tpl) => (
                      <div
                        key={tpl.name}
                        className="flex items-center gap-3 px-3 py-2.5 rounded-lg bg-bg-input border border-border-strong/40"
                      >
                        <div className="w-8 h-8 rounded-md bg-brand/10 border border-brand/20 flex items-center justify-center flex-shrink-0">
                          <LayoutTemplate size={15} className="text-brand" />
                        </div>
                        <div className="flex-1 min-w-0">
                          <div className="text-[13px] font-medium text-text-base leading-snug">
                            {tpl.display_name}
                          </div>
                          <p className="text-[11px] text-text-muted leading-relaxed line-clamp-2 mt-0.5">
                            {tpl.description}
                          </p>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>

            {/* Right — metadata sidebar */}
            <div className="space-y-6">

              {/* Author */}
              <AuthorSection descriptor={collection.author} />

              {/* Stats */}
              <div>
                <div className="text-[10px] font-semibold text-text-muted uppercase tracking-wider mb-3">
                  Contents
                </div>
                <div className="space-y-2">
                  {collection.skills.length > 0 && (
                    <div className="flex items-center justify-between text-[12px]">
                      <span className="flex items-center gap-1.5 text-text-muted">
                        <Code size={11} className="text-icon-skill" />
                        Skills
                      </span>
                      <span className="font-medium text-text-base">{collection.skills.length}</span>
                    </div>
                  )}
                  {collection.mcp_servers.length > 0 && (
                    <div className="flex items-center justify-between text-[12px]">
                      <span className="flex items-center gap-1.5 text-text-muted">
                        <Server size={11} className="text-icon-mcp" />
                        MCP Servers
                      </span>
                      <span className="font-medium text-text-base">{collection.mcp_servers.length}</span>
                    </div>
                  )}
                  {collection.templates.length > 0 && (
                    <div className="flex items-center justify-between text-[12px]">
                      <span className="flex items-center gap-1.5 text-text-muted">
                        <LayoutTemplate size={11} className="text-brand" />
                        Templates
                      </span>
                      <span className="font-medium text-text-base">{collection.templates.length}</span>
                    </div>
                  )}
                  <div className="pt-1 border-t border-border-strong/40 flex items-center justify-between text-[12px]">
                    <span className="text-text-muted">Total</span>
                    <span className="font-semibold text-text-base">{totalItems}</span>
                  </div>
                </div>
              </div>

              {/* Tags */}
              {collection.tags.length > 0 && (
                <div>
                  <div className="text-[10px] font-semibold text-text-muted uppercase tracking-wider mb-2.5">
                    Tags
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    {collection.tags.map((tag) => (
                      <span
                        key={tag}
                        className="text-[11px] px-2 py-0.5 rounded-full bg-bg-sidebar border border-border-strong/40 text-text-muted"
                      >
                        {tag}
                      </span>
                    ))}
                  </div>
                </div>
              )}

              {/* Source link for GitHub collections */}
              {collection.author.type === "github" && "repo" in collection.author && (
                <div>
                  <div className="text-[10px] font-semibold text-text-muted uppercase tracking-wider mb-2">
                    Repository
                  </div>
                  <a
                    href={`https://github.com/${(collection.author as any).repo}`}
                    target="_blank"
                    rel="noopener noreferrer"
                    onClick={handleExternalLinkClick(`https://github.com/${(collection.author as any).repo}`)}
                    className="flex items-center gap-1.5 text-[12px] text-icon-skill hover:text-icon-skill-light transition-colors"
                  >
                    <Github size={12} className="flex-shrink-0" />
                    <span className="truncate">{(collection.author as any).repo}</span>
                  </a>
                </div>
              )}

              {/* What Import All does */}
              <div className="p-3.5 rounded-lg bg-bg-input border border-border-strong/40">
                <div className="text-[10px] font-semibold text-text-muted tracking-wider uppercase mb-2">
                  Import All installs
                </div>
                <ul className="space-y-1.5 text-[11px] text-text-muted">
                  {collection.skills.length > 0 && (
                    <li className="flex items-start gap-1.5">
                      <Code size={10} className={`${ACCENT_TEXT} flex-shrink-0 mt-0.5`} />
                      {collection.skills.length} skill{collection.skills.length !== 1 ? "s" : ""} to your Skills library
                    </li>
                  )}
                  {collection.mcp_servers.length > 0 && (
                    <li className="flex items-start gap-1.5">
                      <Server size={10} className={`${ACCENT_TEXT} flex-shrink-0 mt-0.5`} />
                      {collection.mcp_servers.length} MCP server config{collection.mcp_servers.length !== 1 ? "s" : ""}
                    </li>
                  )}
                  <li className="flex items-start gap-1.5">
                    <Package size={10} className={`${ACCENT_TEXT} flex-shrink-0 mt-0.5`} />
                    You can also import items one at a time
                  </li>
                </ul>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Collection card ───────────────────────────────────────────────────────

function CollectionCard({
  collection,
  installedCount,
  onClick,
}: {
  collection: Collection;
  installedCount: number;
  onClick: () => void;
}) {
  const total = collection.skills.length + collection.mcp_servers.length + collection.templates.length;
  const allInstalled = total > 0 && installedCount >= collection.skills.length;

  return (
    <button
      onClick={onClick}
      className="group w-full h-full text-left p-5 rounded-xl bg-bg-input border border-border-strong/40 hover:border-border-strong hover:bg-surface-hover transition-all flex flex-col"
    >
      {/* Header */}
      <div className="flex items-start justify-between gap-3 mb-3">
        <div className="flex items-center gap-2.5 min-w-0">
          <CollectionIcon collection={collection} size={36} />
          <div className="min-w-0">
            <div className="text-[14px] font-semibold text-text-base leading-snug truncate">
              {collection.name}
            </div>
            <span className={`text-[10px] font-medium ${ACCENT_TEXT}`}>
              {total} item{total !== 1 ? "s" : ""}
            </span>
          </div>
        </div>
        <div className="flex items-center gap-1.5 flex-shrink-0 mt-0.5">
          {allInstalled && (
            <span className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full ${ACCENT_BG} border ${ACCENT_BORDER} text-[10px] font-medium ${ACCENT_TEXT}`}>
              <CheckCircle2 size={10} />
              Installed
            </span>
          )}
          <ArrowRight size={13} className="text-surface group-hover:text-text-muted transition-colors" />
        </div>
      </div>

      {/* Description */}
      <div className="flex-1 min-h-0">
        <p className="text-[12px] text-text-muted leading-relaxed line-clamp-3">
          {collection.description}
        </p>
      </div>

      {/* Content pills */}
      <div className="flex flex-wrap gap-1.5 mt-3">
        {collection.skills.length > 0 && (
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-bg-sidebar border border-border-strong/40 text-[10px] text-text-muted">
            <Code size={10} />
            {collection.skills.length} skill{collection.skills.length !== 1 ? "s" : ""}
          </span>
        )}
        {collection.mcp_servers.length > 0 && (
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-bg-sidebar border border-border-strong/40 text-[10px] text-text-muted">
            <Server size={10} />
            {collection.mcp_servers.length} MCP
          </span>
        )}
        {collection.templates.length > 0 && (
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-bg-sidebar border border-border-strong/40 text-[10px] text-text-muted">
            <LayoutTemplate size={10} />
            {collection.templates.length} template{collection.templates.length !== 1 ? "s" : ""}
          </span>
        )}
      </div>

      {/* Tags */}
      <div className="flex flex-wrap gap-1 mt-2">
        {collection.tags.slice(0, 3).map((tag) => (
          <span key={tag} className="text-[10px] px-1.5 py-0.5 rounded bg-bg-sidebar text-text-muted">
            {tag}
          </span>
        ))}
        {collection.tags.length > 3 && (
          <span className="text-[10px] text-text-muted">+{collection.tags.length - 3}</span>
        )}
      </div>

      {/* Footer */}
      <div className="flex items-center justify-between gap-2 mt-2.5 pt-2.5 border-t border-border-strong/40">
        {installedCount > 0 && (
          <span className="text-[10px] text-icon-skill">
            {installedCount} of {collection.skills.length} skill{collection.skills.length !== 1 ? "s" : ""} installed
          </span>
        )}
      </div>
    </button>
  );
}

// ── Main component ────────────────────────────────────────────────────────

export default function CollectionMarketplace({
  resetKey,
  initialQuery,
  onInitialQueryConsumed,
}: {
  resetKey?: number;
  initialQuery?: string | null;
  onInitialQueryConsumed?: () => void;
}) {
  const [collections, setCollections] = useState<Collection[]>([]);
  const [collectionsLoading, setCollectionsLoading] = useState(true);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<Collection | null>(null);
  // registry: skill name → SkillSource, loaded from ~/.automatic/skills.json
  const [registry, setRegistry] = useState<Record<string, SkillSource>>({});
  const [installedMcpServers, setInstalledMcpServers] = useState<Set<string>>(new Set());

  // Reset when nav item re-clicked
  useEffect(() => {
    if (resetKey !== undefined) {
      setSelected(null);
      setQuery("");
    }
  }, [resetKey]);

  useEffect(() => {
    if (!initialQuery) return;
    setSelected(null);
    setQuery(initialQuery);
    onInitialQueryConsumed?.();
  }, [initialQuery, onInitialQueryConsumed]);

  // Load collections catalogue from ~/.automatic/marketplace/collections.json
  const loadCollections = useCallback(async () => {
    setCollectionsLoading(true);
    try {
      const json: string = await invoke("search_collections", { query: "" });
      setCollections(JSON.parse(json) as Collection[]);
    } catch {
      // non-fatal: leave collections empty
    } finally {
      setCollectionsLoading(false);
    }
  }, []);

  // Load skill registry
  const loadRegistry = useCallback(async () => {
    try {
      const raw: string = await invoke("get_skill_sources");
      setRegistry(JSON.parse(raw));
    } catch {
      // non-fatal
    }
  }, []);

  // Load installed MCP servers
  const loadInstalledMcp = useCallback(async () => {
    try {
      const result: string[] = await invoke("list_mcp_server_configs");
      setInstalledMcpServers(new Set(result));
    } catch {
      // non-fatal
    }
  }, []);

  useEffect(() => {
    loadCollections();
    loadRegistry();
    loadInstalledMcp();
  }, [loadCollections, loadRegistry, loadInstalledMcp]);

  // Update registry after a skill is installed
  const handleRegistryUpdate = useCallback(
    (name: string, source: string, id: string) => {
      setRegistry((prev) => ({ ...prev, [name]: { source, id } }));
    },
    []
  );

  // Count installed skills per collection
  const installedCountFor = useCallback(
    (collection: Collection) =>
      collection.skills.filter((s) =>
        Object.values(registry).some((r) => r.id === s.id)
      ).length,
    [registry]
  );

  // Filtered collections
  const filtered = useMemo(() => {
    if (!query.trim()) return collections;
    const q = query.toLowerCase();
    return collections.filter(
      (c) =>
        c.name.toLowerCase().includes(q) ||
        c.description.toLowerCase().includes(q) ||
        c.tags.some((t) => t.toLowerCase().includes(q)) ||
        c.skills.some(
          (s) =>
            s.display_name.toLowerCase().includes(q) ||
            s.description.toLowerCase().includes(q)
        )
    );
  }, [query, collections]);

  // ── Detail view ──────────────────────────────────────────────────────────

  if (selected) {
    return (
      <CollectionDetail
        collection={selected}
        registry={registry}
        installedMcpServers={installedMcpServers}
        onBack={() => setSelected(null)}
        onRegistryUpdate={handleRegistryUpdate}
      />
    );
  }

  // ── Landing / directory ──────────────────────────────────────────────────

  return (
    <div className="flex h-full flex-col overflow-y-auto custom-scrollbar bg-bg-base">
      <div className="flex flex-col px-6 pt-10 pb-10 w-full">
        <div className="w-full max-w-[1400px] mx-auto">

          {/* Heading */}
          <div className="text-center mb-6 max-w-lg mx-auto">
            <div
              className={`w-12 h-12 mx-auto mb-4 rounded-2xl ${ACCENT_BG} border ${ACCENT_BORDER} flex items-center justify-center`}
            >
              <Layers size={20} className={ACCENT_TEXT} strokeWidth={1.5} />
            </div>
            <h2 className="text-[18px] font-semibold text-text-base mb-1.5">
              Collections
            </h2>
            <p className="text-[13px] text-text-muted leading-relaxed">
              Curated bundles of skills, MCP servers, and templates grouped by theme.
              Import an entire collection at once, or pick individual items.
            </p>
          </div>

          {/* Search */}
          <div className="max-w-2xl mx-auto w-full mb-6">
            <div className="relative">
              <Search
                size={16}
                className="absolute left-4 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none"
              />
              <input
                type="text"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Search collections, skills, tags…"
                autoFocus
                className={`w-full bg-bg-input border border-border-strong/40 hover:border-border-strong focus:${ACCENT_BORDER.replace("border-", "border-")} rounded-xl pl-11 pr-10 py-3 text-[14px] text-text-base placeholder-text-muted/60 outline-none transition-colors shadow-sm`}
              />
              {query && (
                <button
                  onClick={() => setQuery("")}
                  className="absolute right-4 top-1/2 -translate-y-1/2 text-text-muted hover:text-text-base transition-colors"
                >
                  <X size={14} />
                </button>
              )}
            </div>
          </div>

          {/* Results header */}
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
              {query.trim()
                ? `${filtered.length} result${filtered.length !== 1 ? "s" : ""}`
                : `${collections.length} Collection${collections.length !== 1 ? "s" : ""}`}
            </h3>
          </div>

          {/* Grid */}
          {collectionsLoading ? (
            <div className="flex items-center justify-center py-16">
              <Loader2 size={24} className="animate-spin text-text-muted" />
            </div>
          ) : filtered.length === 0 ? (
            <div className="text-center py-16">
              <div
                className={`w-12 h-12 rounded-full border-2 border-dashed border-border-strong/40 flex items-center justify-center mx-auto mb-4`}
              >
                <Search size={18} className="text-text-muted" />
              </div>
              <h3 className="text-[14px] font-medium text-text-base mb-1">
                No collections found
              </h3>
              <p className="text-[13px] text-text-muted">Try a different search term</p>
              <button
                onClick={() => setQuery("")}
                className={`mt-4 text-[12px] ${ACCENT_TEXT} hover:opacity-80 transition-opacity`}
              >
                Clear search
              </button>
            </div>
          ) : (
            <div className="grid grid-cols-3 2xl:grid-cols-4 gap-4">
              {filtered.map((c) => (
                <CollectionCard
                  key={c.id}
                  collection={c}
                  installedCount={installedCountFor(c)}
                  onClick={() => setSelected(c)}
                />
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
