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
} from "lucide-react";

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
}

// ── Helpers ──────────────────────────────────────────────────────────────────

const CATEGORY_COLOURS: Record<string, { bg: string; text: string; dot: string }> = {
  "Web Application":  { bg: "bg-[#5E6AD2]/15", text: "text-[#5E6AD2]", dot: "bg-[#5E6AD2]" },
  "API / Backend":    { bg: "bg-[#5E6AD2]/15", text: "text-[#5E6AD2]", dot: "bg-[#5E6AD2]" },
  "Data & Analytics": { bg: "bg-[#5E6AD2]/15", text: "text-[#5E6AD2]", dot: "bg-[#5E6AD2]" },
  "Desktop App":      { bg: "bg-[#5E6AD2]/15", text: "text-[#5E6AD2]", dot: "bg-[#5E6AD2]" },
  "Infrastructure":   { bg: "bg-[#5E6AD2]/15", text: "text-[#5E6AD2]", dot: "bg-[#5E6AD2]" },
  "Frontend":         { bg: "bg-[#5E6AD2]/15", text: "text-[#5E6AD2]", dot: "bg-[#5E6AD2]" },
};

function categoryStyle(cat: string) {
  return CATEGORY_COLOURS[cat] ?? { bg: "bg-[#5E6AD2]/15", text: "text-[#5E6AD2]", dot: "bg-[#5E6AD2]" };
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
      className="group text-left p-5 rounded-xl bg-[#1A1A1E] border border-[#33353A] hover:border-[#44474F] hover:bg-[#1E1F24] transition-all flex flex-col"
    >
      {/* Header */}
      <div className="flex items-start justify-between gap-3 mb-3">
        <div className="flex items-center gap-2.5 min-w-0">
          <span className={`w-2 h-2 rounded-full flex-shrink-0 mt-0.5 ${style.dot}`} />
          <div className="min-w-0">
            <div className="text-[14px] font-semibold text-[#F8F8FA] leading-snug truncate">
              {template.display_name}
            </div>
            <span className={`text-[10px] font-medium ${style.text}`}>
              {template.category}
            </span>
          </div>
        </div>
        <div className="flex items-center gap-1.5 flex-shrink-0 mt-0.5">
          {imported && <CheckCircle2 size={13} className="text-[#5E6AD2]" />}
          <ArrowRight size={13} className="text-[#33353A] group-hover:text-[#C8CAD0] transition-colors" />
        </div>
      </div>

      {/* Description */}
      <p className="text-[12px] text-[#C8CAD0] leading-relaxed line-clamp-3 flex-1">
        {template.description}
      </p>

      {/* Content pills */}
      <div className="flex flex-wrap gap-1.5 mt-3">
        {template.skills.length > 0 && (
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-[#2D2E36] border border-[#33353A] text-[10px] text-[#C8CAD0]">
            <Code size={10} />
            {template.skills.length} skill{template.skills.length !== 1 ? "s" : ""}
          </span>
        )}
        {template.mcp_servers.length > 0 && (
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-[#2D2E36] border border-[#33353A] text-[10px] text-[#C8CAD0]">
            <Server size={10} />
            {template.mcp_servers.length} MCP
          </span>
        )}
        {hasInstruction && (
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-[#2D2E36] border border-[#33353A] text-[10px] text-[#C8CAD0]">
            <FileText size={10} />
            Instructions
          </span>
        )}
      </div>

      {/* Tags */}
      <div className="flex flex-wrap gap-1 mt-2.5">
        {template.tags.slice(0, 4).map((tag) => (
          <span key={tag} className="text-[10px] px-1.5 py-0.5 rounded bg-[#2D2E36] text-[#C8CAD0]/70">
            {tag}
          </span>
        ))}
        {template.tags.length > 4 && (
          <span className="text-[10px] text-[#C8CAD0]/40">+{template.tags.length - 4}</span>
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
}: {
  template: BundledProjectTemplate;
  imported: boolean;
  importing: boolean;
  error: string | null;
  onImport: () => void;
  onClose: () => void;
}) {
  const style = categoryStyle(template.category);
  const instruction = template.unified_instruction?.trim() ?? "";

  return (
    <div className="flex h-full flex-col bg-[#222327]">
      {/* Header */}
      <div className="h-12 px-6 border-b border-[#33353A] flex items-center justify-between flex-shrink-0">
        <div className="flex items-center gap-3">
          <button
            onClick={onClose}
            className="flex items-center gap-1.5 text-[12px] text-[#C8CAD0] hover:text-[#F8F8FA] transition-colors"
          >
            <ArrowLeft size={13} />
            Back
          </button>
          <span className="text-[#33353A]">/</span>
          <div className="flex items-center gap-2.5">
            <span className={`w-2 h-2 rounded-full ${style.dot}`} />
            <span className="text-[14px] font-semibold text-[#F8F8FA]">{template.display_name}</span>
            <span className={`text-[10px] font-medium px-2 py-0.5 rounded-full ${style.bg} ${style.text}`}>
              {template.category}
            </span>
          </div>
        </div>
        <div className="flex items-center gap-2">
          {error && <span className="text-[12px] text-red-400">{error}</span>}
          {imported ? (
            <div className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[12px] font-medium bg-[#2D2E36] border border-[#33353A] text-[#5E6AD2]">
              <CheckCircle2 size={12} />
              Imported
            </div>
          ) : (
            <button
              onClick={onImport}
              disabled={importing}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[12px] font-medium bg-[#5E6AD2] hover:bg-[#6B78E3] text-white transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-sm"
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
          <p className="text-[14px] text-[#E5E6EA] leading-relaxed pb-8 border-b border-[#33353A]">
            {template.description}
          </p>

          <div className="grid grid-cols-[1fr_260px] gap-8">

            {/* Left — Unified instruction preview */}
            <div>
              <div className="flex items-center gap-2 mb-3">
                <FileText size={13} className="text-[#5E6AD2]" />
                <span className="text-[11px] font-semibold text-[#C8CAD0] tracking-wider uppercase">
                  Project Instructions
                </span>
              </div>
              {instruction ? (
                <div className="bg-[#111114] border border-[#33353A] rounded-lg px-4 py-3 overflow-x-auto">
                  <pre className="text-[11px] text-[#E5E6EA] font-mono leading-relaxed whitespace-pre-wrap">
                    {instruction}
                  </pre>
                </div>
              ) : (
                <p className="text-[12px] text-[#C8CAD0]/50 italic">No instructions included.</p>
              )}
            </div>

            {/* Right — metadata sidebar */}
            <div className="space-y-6">

              {/* Skills */}
              {template.skills.length > 0 && (
                <div>
                  <div className="flex items-center gap-2 mb-2.5">
                    <Code size={12} className="text-[#5E6AD2]" />
                    <span className="text-[11px] font-semibold text-[#C8CAD0] tracking-wider uppercase">Skills</span>
                    <span className="text-[10px] bg-[#5E6AD2]/15 text-[#5E6AD2] px-1.5 py-0.5 rounded">
                      {template.skills.length}
                    </span>
                  </div>
                  <div className="space-y-1.5">
                    {template.skills.map((s) => (
                      <div key={s} className="flex items-center gap-2 px-2.5 py-1.5 rounded-lg bg-[#1A1A1E] border border-[#33353A] text-[12px] text-[#F8F8FA]">
                        <Code size={10} className="text-[#5E6AD2] flex-shrink-0" />
                        <span className="truncate">{s}</span>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {/* MCP Servers */}
              {template.mcp_servers.length > 0 && (
                <div>
                  <div className="flex items-center gap-2 mb-2.5">
                    <Server size={12} className="text-[#F59E0B]" />
                    <span className="text-[11px] font-semibold text-[#C8CAD0] tracking-wider uppercase">MCP Servers</span>
                    <span className="text-[10px] bg-[#F59E0B]/15 text-[#F59E0B] px-1.5 py-0.5 rounded">
                      {template.mcp_servers.length}
                    </span>
                  </div>
                  <div className="space-y-1.5">
                    {template.mcp_servers.map((s) => (
                      <div key={s} className="flex items-center gap-2 px-2.5 py-1.5 rounded-lg bg-[#1A1A1E] border border-[#33353A] text-[12px] text-[#F8F8FA]">
                        <Server size={10} className="text-[#F59E0B] flex-shrink-0" />
                        <span className="truncate">{s}</span>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {/* Tags */}
              {template.tags.length > 0 && (
                <div>
                  <div className="flex items-center gap-2 mb-2.5">
                    <Tag size={12} className="text-[#C8CAD0]" />
                    <span className="text-[11px] font-semibold text-[#C8CAD0] tracking-wider uppercase">Tags</span>
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    {template.tags.map((tag) => (
                      <span key={tag} className="text-[11px] px-2 py-0.5 rounded-full bg-[#2D2E36] border border-[#33353A] text-[#C8CAD0]">
                        {tag}
                      </span>
                    ))}
                  </div>
                </div>
              )}

              {/* What import does */}
              <div className="p-3.5 rounded-lg bg-[#1A1A1E] border border-[#33353A]">
                <div className="text-[10px] font-semibold text-[#C8CAD0] tracking-wider uppercase mb-2">
                  What Import Does
                </div>
                <ul className="space-y-1.5 text-[11px] text-[#C8CAD0]">
                  <li className="flex items-start gap-1.5">
                    <FolderOpen size={10} className="text-[#5E6AD2] flex-shrink-0 mt-0.5" />
                    Adds an editable copy to your Project Templates
                  </li>
                  {instruction && (
                    <li className="flex items-start gap-1.5">
                      <FileText size={10} className="text-[#5E6AD2] flex-shrink-0 mt-0.5" />
                      Includes unified project instructions
                    </li>
                  )}
                  {template.skills.length > 0 && (
                    <li className="flex items-start gap-1.5">
                      <Code size={10} className="text-[#5E6AD2] flex-shrink-0 mt-0.5" />
                      Pre-configures {template.skills.length} skill{template.skills.length !== 1 ? "s" : ""}
                    </li>
                  )}
                  <li className="flex items-start gap-1.5">
                    <span className="text-[#5E6AD2] flex-shrink-0 mt-0.5">·</span>
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

export default function TemplateMarketplace({ resetKey }: { resetKey?: number }) {
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
      />
    );
  }

  // Landing
  return (
    <div className="flex h-full flex-col overflow-y-auto custom-scrollbar bg-[#222327]">
      <div className="flex flex-col px-6 pt-10 pb-10 w-full">
        <div className="w-full max-w-4xl mx-auto">

          {/* Heading */}
          <div className="text-center mb-6 max-w-lg mx-auto">
            <div className="w-12 h-12 mx-auto mb-4 rounded-2xl bg-[#5E6AD2]/10 border border-[#5E6AD2]/20 flex items-center justify-center">
              <FolderOpen size={20} className="text-[#5E6AD2]" strokeWidth={1.5} />
            </div>
            <h2 className="text-[18px] font-semibold text-[#F8F8FA] mb-1.5">Template Marketplace</h2>
            <p className="text-[13px] text-[#C8CAD0] leading-relaxed">
              Pre-built project configurations for common stacks. Import a template to add it to
              your Project Templates library, then apply it to any project.
            </p>
          </div>

          {/* Search */}
          <div className="relative mb-6">
            <Search size={16} className="absolute left-4 top-1/2 -translate-y-1/2 text-[#C8CAD0] pointer-events-none" />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search by name, category, or tag…"
              autoFocus
              className="w-full bg-[#1A1A1E] border border-[#33353A] hover:border-[#44474F] focus:border-[#5E6AD2] rounded-xl pl-11 pr-10 py-3 text-[14px] text-[#F8F8FA] placeholder-[#C8CAD0]/60 outline-none transition-colors shadow-sm"
            />
            {query && (
              <button
                onClick={() => setQuery("")}
                className="absolute right-4 top-1/2 -translate-y-1/2 text-[#C8CAD0] hover:text-[#F8F8FA] transition-colors"
              >
                <X size={14} />
              </button>
            )}
          </div>

          {/* Results */}
          {loading ? (
            <div className="flex items-center justify-center py-16">
              <Loader2 size={20} className="animate-spin text-[#C8CAD0]" />
            </div>
          ) : results.length === 0 ? (
            <div className="text-center py-16">
              <div className="w-12 h-12 rounded-full border-2 border-dashed border-[#33353A] flex items-center justify-center mx-auto mb-4">
                <Search size={18} className="text-[#C8CAD0]" />
              </div>
              <h3 className="text-[14px] font-medium text-[#F8F8FA] mb-1">No templates found</h3>
              <p className="text-[13px] text-[#C8CAD0]">Try a different search term</p>
               <button onClick={() => setQuery("")} className="mt-4 text-[12px] text-[#5E6AD2] hover:text-[#6B78E3] transition-colors">
                 Clear search
               </button>
            </div>
          ) : (
            <>
              <div className="flex items-center justify-between mb-4">
                <h3 className="text-[11px] font-semibold text-[#C8CAD0] tracking-wider uppercase">
                  {query.trim() ? `${results.length} result${results.length !== 1 ? "s" : ""}` : "Featured Templates"}
                </h3>
                {!query.trim() && importedNames.size > 0 && (
                  <span className="text-[11px] text-[#5E6AD2]">{importedNames.size} imported</span>
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
