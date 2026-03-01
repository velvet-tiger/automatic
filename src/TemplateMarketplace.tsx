import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Search,
  Download,
  CheckCircle2,
  Loader2,
  Code,
  Server,
  Tag,
  FolderOpen,
  ArrowRight,
  ArrowLeft,
  X,
  FileText,
  AlertTriangle,
  PackagePlus,
} from "lucide-react";
import { AuthorSection, type AuthorDescriptor } from "./AuthorPanel";

// ── Types ────────────────────────────────────────────────────────────────────

interface BundledProjectTemplate {
  name: string;
  display_name: string;
  description: string;
  category: string;
  tags: string[];
  skills: string[];
  mcp_servers: string[];
  providers: string[];
  agents: string[];
  project_files: { filename: string; content: string }[];
  unified_instruction?: string;
  unified_rules?: string[];
  /** Optional icon filename (e.g. "nextjs.svg") served from /template-icons/ */
  icon?: string;
  /** Author/provider metadata — same shape as AuthorDescriptor. */
  _author?: AuthorDescriptor;
}

interface SkillDependencyStatus {
  name: string;
  installed: boolean;
  /** True when the skill is shipped with the app and will be installed automatically on import. */
  bundled: boolean;
}

interface TemplateDependencyReport {
  skills: SkillDependencyStatus[];
  missing_mcp_servers: string[];
}

// ── Helpers ──────────────────────────────────────────────────────────────────

const CATEGORY_COLOURS: Record<string, { bg: string; text: string; dot: string }> = {
  "Web Application":  { bg: "bg-brand/15", text: "text-brand", dot: "bg-brand" },
  "API / Backend":    { bg: "bg-brand/15", text: "text-brand", dot: "bg-brand" },
  "Data & Analytics": { bg: "bg-brand/15", text: "text-brand", dot: "bg-brand" },
  "Desktop App":      { bg: "bg-brand/15", text: "text-brand", dot: "bg-brand" },
  "Infrastructure":   { bg: "bg-brand/15", text: "text-brand", dot: "bg-brand" },
  "Frontend":         { bg: "bg-brand/15", text: "text-brand", dot: "bg-brand" },
};

function categoryStyle(cat: string) {
  return CATEGORY_COLOURS[cat] ?? { bg: "bg-brand/15", text: "text-brand", dot: "bg-brand" };
}

// ── Template Icon ─────────────────────────────────────────────────────────────

const BRANDFETCH_CLIENT_ID = import.meta.env.VITE_BRANDFETCH_CLIENT_ID as string | undefined;

/**
 * Build a Brandfetch CDN URL for a brand domain.
 * Uses type=icon (social/app icon), dark theme, lettermark fallback.
 * Size is capped at 64px — these are small UI icons.
 */
function brandfetchUrl(domain: string, px: number): string {
  const s = Math.min(px * 2, 64); // 2× for retina, max 64
  return `https://cdn.brandfetch.io/${encodeURIComponent(domain)}/w/${s}/h/${s}/theme/dark/fallback/lettermark/type/icon?c=${BRANDFETCH_CLIENT_ID ?? ""}`;
}

/**
 * Renders the template's brand icon via Brandfetch CDN if the template has an
 * `icon` domain set, with a letter-avatar fallback on load error or if absent.
 */
function TemplateIcon({
  template,
  size,
}: {
  template: BundledProjectTemplate;
  /** Square size in pixels */
  size: number;
}) {
  const [imgError, setImgError] = useState(false);
  const style = categoryStyle(template.category);

  if (template.icon && BRANDFETCH_CLIENT_ID && !imgError) {
    return (
      <img
        src={brandfetchUrl(template.icon, size)}
        alt={template.display_name}
        width={size}
        height={size}
        onError={() => setImgError(true)}
        className="flex-shrink-0 rounded-md object-contain"
        style={{ width: size, height: size }}
        draggable={false}
      />
    );
  }

  // Fallback: first letter on a tinted background
  const letter = template.display_name.charAt(0).toUpperCase();
  const fontSize = Math.round(size * 0.45);
  return (
    <div
      className={`flex-shrink-0 rounded-md flex items-center justify-center font-semibold ${style.bg} ${style.text}`}
      style={{ width: size, height: size, fontSize }}
      aria-hidden="true"
    >
      {letter}
    </div>
  );
}

// ── Dependency Panel ──────────────────────────────────────────────────────────

function DependencyPanel({ report }: { report: TemplateDependencyReport }) {
  const missingSkills = report.skills.filter((s) => !s.installed);
  const hasMissing = missingSkills.length > 0 || report.missing_mcp_servers.length > 0;

  if (!hasMissing) {
    return (
      <div className="flex items-center gap-2 px-3 py-2.5 rounded-lg bg-emerald-500/10 border border-emerald-500/20 text-[12px] text-emerald-400">
        <CheckCircle2 size={13} className="flex-shrink-0" />
        All dependencies installed
      </div>
    );
  }

  // All skills required by bundled templates are themselves bundled, so
  // missing ones will be installed automatically when the template is imported.
  const willAutoInstall = missingSkills.filter((s) => s.bundled);
  const needsManual = missingSkills.filter((s) => !s.bundled);

  return (
    <div className="rounded-lg border border-brand/25 bg-brand/5 overflow-hidden">
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-brand/20">
        <PackagePlus size={12} className="text-brand flex-shrink-0" />
        <span className="text-[11px] font-semibold text-brand tracking-wider uppercase">
          Skills not yet installed
        </span>
        <span className="ml-auto text-[10px] text-brand/60">
          {missingSkills.length} missing
        </span>
      </div>

      <div className="px-3 py-2.5 space-y-1.5">
        {missingSkills.map((s) => (
          <div key={s.name} className="flex items-center gap-2 text-[11px]">
            <Code size={10} className="text-brand/70 flex-shrink-0" />
            <span className="font-mono text-text-base">{s.name}</span>
            {s.bundled ? (
              <span className="text-[10px] text-brand/70 ml-auto flex-shrink-0 flex items-center gap-1">
                <PackagePlus size={9} />
                bundled
              </span>
            ) : (
              <span className="text-[10px] text-text-muted ml-auto flex-shrink-0 italic">
                install manually
              </span>
            )}
          </div>
        ))}

        {report.missing_mcp_servers.map((s) => (
          <div key={s} className="flex items-center gap-2 text-[11px]">
            <Server size={10} className="text-amber-400/70 flex-shrink-0" />
            <span className="font-mono text-text-base">{s}</span>
            <span className="text-[10px] text-text-muted ml-auto flex-shrink-0 italic">
              add manually
            </span>
          </div>
        ))}
      </div>

      {willAutoInstall.length > 0 && (
        <div className="px-3 pb-2.5 text-[10px] text-brand/70">
          {willAutoInstall.length} bundled skill{willAutoInstall.length !== 1 ? "s" : ""} will be installed automatically when you import this template.
          {needsManual.length > 0 && ` ${needsManual.length} must be installed manually.`}
        </div>
      )}
    </div>
  );
}

// ── Card ─────────────────────────────────────────────────────────────────────

function TemplateCard({
  template,
  imported,
  onClick,
}: {
  template: BundledProjectTemplate;
  imported: boolean;
  onClick: () => void;
}) {
  const style = categoryStyle(template.category);
  const hasInstruction = !!(template.unified_instruction?.trim());

  return (
    <button
      onClick={onClick}
      className="group text-left p-5 rounded-xl bg-bg-input border border-border-strong/40 hover:border-border-strong hover:bg-surface-hover transition-all flex flex-col"
    >
      {/* Header */}
      <div className="flex items-start justify-between gap-3 mb-3">
        <div className="flex items-center gap-2.5 min-w-0">
          <TemplateIcon template={template} size={36} />
          <div className="min-w-0">
            <div className="text-[14px] font-semibold text-text-base leading-snug truncate">
              {template.display_name}
            </div>
            <span className={`text-[10px] font-medium ${style.text}`}>
              {template.category}
            </span>
          </div>
        </div>
        <div className="flex items-center gap-1.5 flex-shrink-0 mt-0.5">
          {imported && <CheckCircle2 size={13} className="text-brand" />}
          <ArrowRight size={13} className="text-surface group-hover:text-text-muted transition-colors" />
        </div>
      </div>

      {/* Description */}
      <p className="text-[12px] text-text-muted leading-relaxed line-clamp-3 flex-1">
        {template.description}
      </p>

      {/* Content pills */}
      <div className="flex flex-wrap gap-1.5 mt-3">
        {template.skills.length > 0 && (
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-bg-sidebar border border-border-strong/40 text-[10px] text-text-muted">
            <Code size={10} />
            {template.skills.length} skill{template.skills.length !== 1 ? "s" : ""}
          </span>
        )}
        {template.mcp_servers.length > 0 && (
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-bg-sidebar border border-border-strong/40 text-[10px] text-text-muted">
            <Server size={10} />
            {template.mcp_servers.length} MCP
          </span>
        )}
        {hasInstruction && (
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-bg-sidebar border border-border-strong/40 text-[10px] text-text-muted">
            <FileText size={10} />
            Instructions
          </span>
        )}
      </div>

      {/* Tags */}
      <div className="flex flex-wrap gap-1 mt-2.5">
        {template.tags.slice(0, 4).map((tag) => (
          <span key={tag} className="text-[10px] px-1.5 py-0.5 rounded bg-bg-sidebar text-text-muted">
            {tag}
          </span>
        ))}
        {template.tags.length > 4 && (
          <span className="text-[10px] text-text-muted">+{template.tags.length - 4}</span>
        )}
      </div>
    </button>
  );
}

// ── Detail panel ─────────────────────────────────────────────────────────────

function TemplateDetail({
  template,
  imported,
  importing,
  error,
  onImport,
  onClose,
  onGoToTemplate,
}: {
  template: BundledProjectTemplate;
  imported: boolean;
  importing: boolean;
  error: string | null;
  onImport: () => void;
  onClose: () => void;
  onGoToTemplate?: (name: string) => void;
}) {
  const style = categoryStyle(template.category);
  const instruction = template.unified_instruction?.trim() ?? "";

  // Dependency check state
  const [depReport, setDepReport] = useState<TemplateDependencyReport | null>(null);
  const [depLoading, setDepLoading] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setDepReport(null);

    if (template.skills.length === 0 && template.mcp_servers.length === 0) {
      setDepReport({ skills: [], missing_mcp_servers: [] });
      return;
    }

    setDepLoading(true);
    (async () => {
      try {
        const raw: string = await invoke("check_template_dependencies", { name: template.name });
        if (!cancelled) setDepReport(JSON.parse(raw));
      } catch (err) {
        console.error("Dependency check failed:", err);
      } finally {
        if (!cancelled) setDepLoading(false);
      }
    })();

    return () => { cancelled = true; };
  }, [template.name]);

  return (
    <div className="flex h-full flex-col bg-bg-base">
      {/* Header */}
      <div className="h-12 px-6 border-b border-border-strong/40 flex items-center justify-between flex-shrink-0">
        <div className="flex items-center gap-3">
          <button
            onClick={onClose}
            className="flex items-center gap-1.5 text-[12px] text-text-muted hover:text-text-base transition-colors"
          >
            <ArrowLeft size={13} />
            Back
          </button>
          <span className="text-surface">/</span>
          <div className="flex items-center gap-2.5">
            <TemplateIcon template={template} size={28} />
            <span className="text-[14px] font-semibold text-text-base">{template.display_name}</span>
            <span className={`text-[10px] font-medium px-2 py-0.5 rounded-full ${style.bg} ${style.text}`}>
              {template.category}
            </span>
          </div>
        </div>
        <div className="flex items-center gap-2">
          {error && <span className="text-[12px] text-red-400">{error}</span>}
          {imported ? (
            <div className="flex items-center gap-2">
              <div className="flex h-[26px] items-center gap-1.5 px-2.5 rounded-md text-[11px] font-medium bg-bg-sidebar border border-border-strong/40 text-brand">
                <CheckCircle2 size={12} />
                Imported
              </div>
              {onGoToTemplate && (
                <button
                  onClick={() => onGoToTemplate(template.name)}
                  className="flex h-[26px] items-center gap-1.5 px-2.5 rounded-md text-[11px] font-medium bg-brand hover:bg-brand-hover text-white transition-colors shadow-sm border border-transparent"
                >
                  <ArrowRight size={12} />
                  Go to template
                </button>
              )}
            </div>
          ) : (
            <button
              onClick={onImport}
              disabled={importing}
              className="flex h-[26px] items-center gap-1.5 px-2.5 rounded-md text-[11px] font-medium bg-brand hover:bg-brand-hover text-white transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-sm border border-transparent"
            >
              {importing ? (
                <><Loader2 size={12} className="animate-spin" /> Importing…</>
              ) : (
                <><Download size={12} /> Import Template</>
              )}
            </button>
          )}
        </div>
      </div>

      {/* Body */}
      <div className="flex-1 overflow-y-auto custom-scrollbar">
        <div className="max-w-3xl mx-auto px-8 py-8 space-y-8">

          {/* Description */}
          <p className="text-[14px] text-text-base leading-relaxed pb-8 border-b border-border-strong/40">
            {template.description}
          </p>

          <div className="grid grid-cols-[1fr_260px] gap-8">

            {/* Left — Unified instruction preview */}
            <div>
              <div className="flex items-center gap-2 mb-3">
                <FileText size={13} className="text-brand" />
                <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
                  Project Instructions
                </span>
              </div>
              {instruction ? (
                <div className="bg-bg-input-dark border border-border-strong/40 rounded-lg px-4 py-3 overflow-x-auto">
                  <pre className="text-[11px] text-text-base font-mono leading-relaxed whitespace-pre-wrap">
                    {instruction}
                  </pre>
                </div>
              ) : (
                <p className="text-[12px] text-text-muted italic">No instructions included.</p>
              )}
            </div>

            {/* Right — metadata sidebar */}
            <div className="space-y-6">

              {/* Author */}
              <AuthorSection descriptor={template._author ?? { type: "provider", name: "Automatic", url: "https://automatic.sh" }} />

              {/* Dependency status */}
              {(template.skills.length > 0 || template.mcp_servers.length > 0) && (
                <div>
                  <div className="flex items-center gap-2 mb-2.5">
                    <PackagePlus size={12} className="text-text-muted" />
                    <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Dependencies</span>
                  </div>
                  {depLoading ? (
                    <div className="flex items-center gap-2 text-[11px] text-text-muted">
                      <Loader2 size={11} className="animate-spin" />
                      Checking…
                    </div>
                  ) : depReport ? (
                    <DependencyPanel report={depReport} />
                  ) : null}
                </div>
              )}

              {/* Skills */}
              {template.skills.length > 0 && (
                <div>
                  <div className="flex items-center gap-2 mb-2.5">
                    <Code size={12} className="text-brand" />
                    <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Skills</span>
                    <span className="text-[10px] bg-brand/15 text-brand px-1.5 py-0.5 rounded">
                      {template.skills.length}
                    </span>
                  </div>
                  <div className="space-y-1.5">
                    {template.skills.map((s) => {
                      const status = depReport?.skills.find((d) => d.name === s);
                      const isInstalled = status?.installed ?? null;
                      return (
                        <div key={s} className="flex items-center gap-2 px-2.5 py-1.5 rounded-lg bg-bg-input border border-border-strong/40 text-[12px] text-text-base">
                          <Code size={10} className="text-brand flex-shrink-0" />
                          <span className="truncate flex-1">{s}</span>
                          {isInstalled === true && (
                            <CheckCircle2 size={10} className="text-emerald-400 flex-shrink-0" />
                          )}
                          {isInstalled === false && (
                            <AlertTriangle size={10} className="text-amber-400 flex-shrink-0" />
                          )}
                        </div>
                      );
                    })}
                  </div>
                </div>
              )}

              {/* MCP Servers */}
              {template.mcp_servers.length > 0 && (
                <div>
                  <div className="flex items-center gap-2 mb-2.5">
                    <Server size={12} className="text-warning" />
                    <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">MCP Servers</span>
                    <span className="text-[10px] bg-warning/15 text-warning px-1.5 py-0.5 rounded">
                      {template.mcp_servers.length}
                    </span>
                  </div>
                  <div className="space-y-1.5">
                    {template.mcp_servers.map((s) => {
                      const isMissing = depReport?.missing_mcp_servers.includes(s) ?? null;
                      return (
                        <div key={s} className="flex items-center gap-2 px-2.5 py-1.5 rounded-lg bg-bg-input border border-border-strong/40 text-[12px] text-text-base">
                          <Server size={10} className="text-warning flex-shrink-0" />
                          <span className="truncate flex-1">{s}</span>
                          {isMissing === false && (
                            <CheckCircle2 size={10} className="text-emerald-400 flex-shrink-0" />
                          )}
                          {isMissing === true && (
                            <AlertTriangle size={10} className="text-amber-400 flex-shrink-0" />
                          )}
                        </div>
                      );
                    })}
                  </div>
                </div>
              )}

              {/* Tags */}
              {template.tags.length > 0 && (
                <div>
                  <div className="flex items-center gap-2 mb-2.5">
                    <Tag size={12} className="text-text-muted" />
                    <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Tags</span>
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    {template.tags.map((tag) => (
                      <span key={tag} className="text-[11px] px-2 py-0.5 rounded-full bg-bg-sidebar border border-border-strong/40 text-text-muted">
                        {tag}
                      </span>
                    ))}
                  </div>
                </div>
              )}

              {/* What import does */}
              <div className="p-3.5 rounded-lg bg-bg-input border border-border-strong/40">
                <div className="text-[10px] font-semibold text-text-muted tracking-wider uppercase mb-2">
                  What Import Does
                </div>
                <ul className="space-y-1.5 text-[11px] text-text-muted">
                  <li className="flex items-start gap-1.5">
                    <FolderOpen size={10} className="text-brand flex-shrink-0 mt-0.5" />
                    Adds an editable copy to your Project Templates
                  </li>
                  {instruction && (
                    <li className="flex items-start gap-1.5">
                      <FileText size={10} className="text-brand flex-shrink-0 mt-0.5" />
                      Includes unified project instructions
                    </li>
                  )}
                  {template.skills.length > 0 && (
                    <li className="flex items-start gap-1.5">
                      <Code size={10} className="text-brand flex-shrink-0 mt-0.5" />
                      Pre-configures {template.skills.length} skill{template.skills.length !== 1 ? "s" : ""}
                    </li>
                  )}
                  <li className="flex items-start gap-1.5">
                    <span className="text-brand flex-shrink-0 mt-0.5">·</span>
                    You choose which agents to use
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

// ── Main Component ────────────────────────────────────────────────────────────

export default function TemplateMarketplace({ resetKey, onNavigateToTemplate }: { resetKey?: number; onNavigateToTemplate?: (name: string) => void }) {
  const [allTemplates, setAllTemplates] = useState<BundledProjectTemplate[]>([]);
  const [results, setResults] = useState<BundledProjectTemplate[]>([]);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<BundledProjectTemplate | null>(null);
  const [importedNames, setImportedNames] = useState<Set<string>>(new Set());
  const [importing, setImporting] = useState(false);
  const [importError, setImportError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Return to landing when the nav item is clicked again
  useEffect(() => {
    if (resetKey !== undefined) {
      setSelected(null);
      setQuery("");
      setImportError(null);
    }
  }, [resetKey]);

  useEffect(() => {
    (async () => {
      try {
        const raw: string = await invoke("list_bundled_project_templates");
        const templates: BundledProjectTemplate[] = JSON.parse(raw);
        setAllTemplates(templates);
        setResults(templates);
      } catch (err) {
        console.error("Failed to load bundled templates:", err);
      }
      try {
        const localNames: string[] = await invoke("get_project_templates");
        setImportedNames(new Set(localNames));
      } catch { /* non-fatal */ }
      setLoading(false);
    })();
  }, []);

  const doSearch = useCallback(
    async (q: string) => {
      if (!q.trim()) { setResults(allTemplates); return; }
      try {
        const raw: string = await invoke("search_bundled_project_templates", { query: q });
        setResults(JSON.parse(raw));
      } catch {
        const lower = q.toLowerCase();
        setResults(allTemplates.filter((t) =>
          t.name.toLowerCase().includes(lower) ||
          t.display_name.toLowerCase().includes(lower) ||
          t.description.toLowerCase().includes(lower) ||
          t.category.toLowerCase().includes(lower) ||
          t.tags.some((tag) => tag.toLowerCase().includes(lower))
        ));
      }
    },
    [allTemplates]
  );

  useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => doSearch(query), 200);
    return () => { if (debounceRef.current) clearTimeout(debounceRef.current); };
  }, [query, doSearch]);

  const handleImport = useCallback(async () => {
    if (!selected) return;
    setImporting(true);
    setImportError(null);
    try {
      await invoke("import_bundled_project_template", { name: selected.name });
      setImportedNames((prev) => new Set([...prev, selected.name]));
    } catch (err: any) {
      setImportError(`Import failed: ${err}`);
    } finally {
      setImporting(false);
    }
  }, [selected]);

  // Detail view
  if (selected) {
    return (
      <TemplateDetail
        template={selected}
        imported={importedNames.has(selected.name)}
        importing={importing}
        error={importError}
        onImport={handleImport}
        onClose={() => { setSelected(null); setImportError(null); }}
        onGoToTemplate={onNavigateToTemplate}
      />
    );
  }

  // Landing
  return (
    <div className="flex h-full flex-col overflow-y-auto custom-scrollbar bg-bg-base">
      <div className="flex flex-col px-6 pt-10 pb-10 w-full">
        <div className="w-full max-w-4xl mx-auto">

          {/* Heading */}
          <div className="text-center mb-6 max-w-lg mx-auto">
            <div className="w-12 h-12 mx-auto mb-4 rounded-2xl bg-brand/10 border border-brand/20 flex items-center justify-center">
              <FolderOpen size={20} className="text-brand" strokeWidth={1.5} />
            </div>
            <h2 className="text-[18px] font-semibold text-text-base mb-1.5">Template Marketplace</h2>
            <p className="text-[13px] text-text-muted leading-relaxed">
              Pre-built project configurations for common stacks. Import a template to add it to
              your Project Templates library, then apply it to any project.
            </p>
          </div>

          {/* Search */}
          <div className="relative mb-6">
            <Search size={16} className="absolute left-4 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none" />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search by name, category, or tag…"
              autoFocus
              className="w-full bg-bg-input border border-border-strong/40 hover:border-border-strong focus:border-brand rounded-xl pl-11 pr-10 py-3 text-[14px] text-text-base placeholder-text-muted/60 outline-none transition-colors shadow-sm"
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

          {/* Results */}
          {loading ? (
            <div className="flex items-center justify-center py-16">
              <Loader2 size={20} className="animate-spin text-text-muted" />
            </div>
          ) : results.length === 0 ? (
            <div className="text-center py-16">
              <div className="w-12 h-12 rounded-full border-2 border-dashed border-border-strong/40 flex items-center justify-center mx-auto mb-4">
                <Search size={18} className="text-text-muted" />
              </div>
              <h3 className="text-[14px] font-medium text-text-base mb-1">No templates found</h3>
              <p className="text-[13px] text-text-muted">Try a different search term</p>
               <button onClick={() => setQuery("")} className="mt-4 text-[12px] text-brand hover:text-brand-hover transition-colors">
                 Clear search
               </button>
            </div>
          ) : (
            <>
              <div className="flex items-center justify-between mb-4">
                <h3 className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
                  {query.trim() ? `${results.length} result${results.length !== 1 ? "s" : ""}` : "Featured Templates"}
                </h3>
                {!query.trim() && importedNames.size > 0 && (
                  <span className="text-[11px] text-brand">{importedNames.size} imported</span>
                )}
              </div>
              <div className="grid grid-cols-2 gap-4">
                {results.map((t) => (
                  <TemplateCard
                    key={t.name}
                    template={t}
                    imported={importedNames.has(t.name)}
                    onClick={() => { setSelected(t); setImportError(null); }}
                  />
                ))}
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
