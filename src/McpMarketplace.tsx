import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Search,
  Server,
  CheckCircle2,
  Loader2,
  ArrowRight,
  ArrowLeft,
  X,
  Terminal,
  Globe,
  ExternalLink,
  KeyRound,
  Eye,
  EyeOff,
  Download,
} from "lucide-react";

// ── Types ────────────────────────────────────────────────────────────────────

interface McpEnvVar {
  key: string;
  label: string;
  description: string;
  secret: boolean;
  required: boolean;
}

interface McpMarketplaceEntry {
  name: string;
  display_name: string;
  description: string;
  category: string;
  tags: string[];
  transport: string;
  author: string;
  config: Record<string, unknown>;
  env_vars: McpEnvVar[];
  setup_notes: string;
  docs_url: string;
}

// ── Helpers ──────────────────────────────────────────────────────────────────

const CATEGORY_COLOURS: Record<string, { bg: string; text: string; dot: string }> = {
  "Files & Storage":    { bg: "bg-[#4ADE80]/15", text: "text-[#4ADE80]", dot: "bg-[#4ADE80]" },
  "Developer Tools":    { bg: "bg-[#5E6AD2]/15", text: "text-[#5E6AD2]", dot: "bg-[#5E6AD2]" },
  "Databases":          { bg: "bg-[#F59E0B]/15", text: "text-[#F59E0B]", dot: "bg-[#F59E0B]" },
  "Web & Search":       { bg: "bg-[#22D3EE]/15", text: "text-[#22D3EE]", dot: "bg-[#22D3EE]" },
  "Agent Tools":        { bg: "bg-[#A78BFA]/15", text: "text-[#A78BFA]", dot: "bg-[#A78BFA]" },
  "Communication":      { bg: "bg-[#F97316]/15", text: "text-[#F97316]", dot: "bg-[#F97316]" },
  "Project Management": { bg: "bg-[#EC4899]/15", text: "text-[#EC4899]", dot: "bg-[#EC4899]" },
};

function categoryStyle(cat: string) {
  return CATEGORY_COLOURS[cat] ?? { bg: "bg-[#8A8C93]/15", text: "text-[#8A8C93]", dot: "bg-[#8A8C93]" };
}

function TransportBadge({ transport }: { transport: string }) {
  const isStdio = transport === "stdio";
  return (
    <span className={`inline-flex items-center gap-1 text-[10px] px-1.5 py-0.5 rounded font-mono ${
      isStdio
        ? "bg-[#2D2E36] text-[#8A8C93]"
        : "bg-[#22D3EE]/10 text-[#22D3EE]"
    }`}>
      {isStdio ? <Terminal size={9} /> : <Globe size={9} />}
      {isStdio ? "stdio" : "http"}
    </span>
  );
}

// ── Card ─────────────────────────────────────────────────────────────────────

function McpCard({
  entry,
  installed,
  onClick,
}: {
  entry: McpMarketplaceEntry;
  installed: boolean;
  onClick: () => void;
}) {
  const style = categoryStyle(entry.category);
  const needsEnv = entry.env_vars.some((v) => v.required);

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
            <div className="text-[14px] font-semibold text-[#E0E1E6] leading-snug truncate">
              {entry.display_name}
            </div>
            <span className={`text-[10px] font-medium ${style.text}`}>
              {entry.category}
            </span>
          </div>
        </div>
        <div className="flex items-center gap-1.5 flex-shrink-0 mt-0.5">
          {installed && <CheckCircle2 size={13} className="text-[#4ADE80]" />}
          <ArrowRight size={13} className="text-[#33353A] group-hover:text-[#8A8C93] transition-colors" />
        </div>
      </div>

      {/* Description */}
      <p className="text-[12px] text-[#8A8C93] leading-relaxed line-clamp-3 flex-1">
        {entry.description}
      </p>

      {/* Footer row */}
      <div className="flex items-center justify-between mt-3">
        <div className="flex items-center gap-1.5">
          <TransportBadge transport={entry.transport} />
          {needsEnv && (
            <span className="inline-flex items-center gap-1 text-[10px] px-1.5 py-0.5 rounded bg-[#F59E0B]/10 text-[#F59E0B]">
              <KeyRound size={9} />
              API key
            </span>
          )}
        </div>
        <span className="text-[10px] text-[#8A8C93]/50">{entry.author}</span>
      </div>

      {/* Tags */}
      <div className="flex flex-wrap gap-1 mt-2.5">
        {entry.tags.slice(0, 4).map((tag) => (
          <span key={tag} className="text-[10px] px-1.5 py-0.5 rounded bg-[#2D2E36] text-[#8A8C93]/70">
            {tag}
          </span>
        ))}
        {entry.tags.length > 4 && (
          <span className="text-[10px] text-[#8A8C93]/40">+{entry.tags.length - 4}</span>
        )}
      </div>
    </button>
  );
}

// ── Secret input ──────────────────────────────────────────────────────────────

function SecretInput({
  value,
  onChange,
  placeholder,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
}) {
  const [visible, setVisible] = useState(false);
  return (
    <div className="relative">
      <input
        type={visible ? "text" : "password"}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="w-full bg-[#111114] border border-[#33353A] hover:border-[#44474F] focus:border-[#5E6AD2] rounded-md px-3 py-2 pr-9 text-[13px] text-[#E0E1E6] placeholder-[#8A8C93]/40 outline-none font-mono transition-colors"
      />
      <button
        type="button"
        onClick={() => setVisible((v) => !v)}
        className="absolute right-2.5 top-1/2 -translate-y-1/2 text-[#8A8C93] hover:text-[#E0E1E6] transition-colors"
      >
        {visible ? <EyeOff size={13} /> : <Eye size={13} />}
      </button>
    </div>
  );
}

// ── Detail panel ─────────────────────────────────────────────────────────────

function McpDetail({
  entry,
  installed,
  onClose,
  onInstalled,
}: {
  entry: McpMarketplaceEntry;
  installed: boolean;
  onClose: () => void;
  onInstalled: (name: string) => void;
}) {
  const style = categoryStyle(entry.category);

  // Build initial env state from the entry's declared vars
  const [envValues, setEnvValues] = useState<Record<string, string>>(() =>
    Object.fromEntries(entry.env_vars.map((v) => [v.key, ""]))
  );
  const [installing, setInstalling] = useState(false);
  const [installError, setInstallError] = useState<string | null>(null);
  const [justInstalled, setJustInstalled] = useState(false);

  const canInstall = entry.env_vars
    .filter((v) => v.required)
    .every((v) => envValues[v.key]?.trim());

  const handleInstall = async () => {
    setInstalling(true);
    setInstallError(null);
    try {
      await invoke("install_mcp_marketplace_entry", {
        name: entry.name,
        envValues,
      });
      setJustInstalled(true);
      onInstalled(entry.name);
    } catch (err: any) {
      setInstallError(`Install failed: ${err}`);
    } finally {
      setInstalling(false);
    }
  };

  const configPreview = JSON.stringify(entry.config, null, 2);

  return (
    <div className="flex h-full flex-col bg-[#222327]">
      {/* Header */}
      <div className="h-12 px-6 border-b border-[#33353A] flex items-center justify-between flex-shrink-0">
        <div className="flex items-center gap-3">
          <button
            onClick={onClose}
            className="flex items-center gap-1.5 text-[12px] text-[#8A8C93] hover:text-[#E0E1E6] transition-colors"
          >
            <ArrowLeft size={13} />
            Back
          </button>
          <span className="text-[#33353A]">/</span>
          <div className="flex items-center gap-2.5">
            <span className={`w-2 h-2 rounded-full ${style.dot}`} />
            <span className="text-[14px] font-semibold text-[#E0E1E6]">{entry.display_name}</span>
            <span className={`text-[10px] font-medium px-2 py-0.5 rounded-full ${style.bg} ${style.text}`}>
              {entry.category}
            </span>
            <TransportBadge transport={entry.transport} />
          </div>
        </div>
        <div className="flex items-center gap-2">
          {installError && <span className="text-[12px] text-red-400">{installError}</span>}
          {installed || justInstalled ? (
            <div className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[12px] font-medium bg-[#2D2E36] border border-[#33353A] text-[#4ADE80]">
              <CheckCircle2 size={12} />
              Installed
            </div>
          ) : (
            <button
              onClick={handleInstall}
              disabled={installing || !canInstall}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[12px] font-medium bg-[#5E6AD2] hover:bg-[#6B78E3] text-white transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-sm"
              title={!canInstall ? "Fill in required API keys first" : undefined}
            >
              {installing ? (
                <><Loader2 size={12} className="animate-spin" /> Installing…</>
              ) : (
                <><Download size={12} /> Install</>
              )}
            </button>
          )}
          {entry.docs_url && (
            <a
              href={entry.docs_url}
              target="_blank"
              rel="noopener noreferrer"
              className="p-1.5 rounded-md text-[#8A8C93] hover:text-[#E0E1E6] hover:bg-[#2D2E36] transition-colors"
              title="View docs"
            >
              <ExternalLink size={14} />
            </a>
          )}
        </div>
      </div>

      {/* Body */}
      <div className="flex-1 overflow-y-auto custom-scrollbar">
        <div className="max-w-3xl mx-auto px-8 py-8 space-y-8">

          {/* Description */}
          <p className="text-[14px] text-[#C0C1C6] leading-relaxed pb-8 border-b border-[#33353A]">
            {entry.description}
          </p>

          <div className="grid grid-cols-[1fr_280px] gap-8">

            {/* Left column */}
            <div className="space-y-7">

              {/* ENV VARS — shown first if any, since they block install */}
              {entry.env_vars.length > 0 && (
                <div>
                  <div className="flex items-center gap-2 mb-3">
                    <KeyRound size={13} className="text-[#F59E0B]" />
                    <span className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase">
                      Credentials
                    </span>
                    {entry.env_vars.some((v) => v.required) && (
                      <span className="text-[10px] text-[#F59E0B]">required to install</span>
                    )}
                  </div>
                  <div className="space-y-4">
                    {entry.env_vars.map((v) => (
                      <div key={v.key}>
                        <div className="flex items-center gap-1.5 mb-1">
                          <label className="text-[12px] font-medium text-[#E0E1E6]">{v.label}</label>
                          {v.required && (
                            <span className="text-[10px] text-[#F59E0B]">required</span>
                          )}
                        </div>
                        <p className="text-[11px] text-[#8A8C93] mb-1.5">{v.description}</p>
                        {v.secret ? (
                          <SecretInput
                            value={envValues[v.key] ?? ""}
                            onChange={(val) => setEnvValues((prev) => ({ ...prev, [v.key]: val }))}
                            placeholder={v.key}
                          />
                        ) : (
                          <input
                            type="text"
                            value={envValues[v.key] ?? ""}
                            onChange={(e) => setEnvValues((prev) => ({ ...prev, [v.key]: e.target.value }))}
                            placeholder={v.key}
                            className="w-full bg-[#111114] border border-[#33353A] hover:border-[#44474F] focus:border-[#5E6AD2] rounded-md px-3 py-2 text-[13px] text-[#E0E1E6] placeholder-[#8A8C93]/40 outline-none font-mono transition-colors"
                          />
                        )}
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {/* Setup notes */}
              {entry.setup_notes && (
                <div>
                  <div className="flex items-center gap-2 mb-3">
                    <Server size={13} className="text-[#5E6AD2]" />
                    <span className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase">
                      Setup Notes
                    </span>
                  </div>
                  <p className="text-[13px] text-[#C0C1C6] leading-relaxed">
                    {entry.setup_notes}
                  </p>
                </div>
              )}

              {/* Config preview */}
              <div>
                <div className="flex items-center gap-2 mb-3">
                  <Terminal size={13} className="text-[#5E6AD2]" />
                  <span className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase">
                    Config Preview
                  </span>
                </div>
                <div className="bg-[#111114] border border-[#33353A] rounded-lg px-4 py-3 overflow-x-auto">
                  <pre className="text-[11px] text-[#C0C1C6] font-mono leading-relaxed">
                    {configPreview}
                  </pre>
                </div>
              </div>

            </div>

            {/* Right sidebar */}
            <div className="space-y-6">

              {/* Author */}
              <div>
                <div className="text-[10px] font-semibold text-[#8A8C93] tracking-wider uppercase mb-1.5">
                  Author
                </div>
                <span className="text-[13px] text-[#E0E1E6]">{entry.author}</span>
              </div>

              {/* Tags */}
              <div>
                <div className="text-[10px] font-semibold text-[#8A8C93] tracking-wider uppercase mb-2">
                  Tags
                </div>
                <div className="flex flex-wrap gap-1.5">
                  {entry.tags.map((tag) => (
                    <span key={tag} className="text-[11px] px-2 py-0.5 rounded-full bg-[#2D2E36] border border-[#33353A] text-[#8A8C93]">
                      {tag}
                    </span>
                  ))}
                </div>
              </div>

              {/* What install does */}
              <div className="p-3.5 rounded-lg bg-[#1A1A1E] border border-[#33353A]">
                <div className="text-[10px] font-semibold text-[#8A8C93] tracking-wider uppercase mb-2">
                  What Install Does
                </div>
                <ul className="space-y-1.5 text-[11px] text-[#8A8C93]">
                  <li className="flex items-start gap-1.5">
                    <Server size={10} className="text-[#5E6AD2] flex-shrink-0 mt-0.5" />
                    Saves the config to your MCP Servers library
                  </li>
                  {entry.env_vars.length > 0 && (
                    <li className="flex items-start gap-1.5">
                      <KeyRound size={10} className="text-[#5E6AD2] flex-shrink-0 mt-0.5" />
                      Merges your credentials into the config
                    </li>
                  )}
                  <li className="flex items-start gap-1.5">
                    <span className="text-[#5E6AD2] flex-shrink-0">·</span>
                    Assign it to projects from MCP Servers
                  </li>
                </ul>
              </div>

              {/* Docs link */}
              {entry.docs_url && (
                <a
                  href={entry.docs_url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="flex items-center gap-1.5 text-[12px] text-[#5E6AD2] hover:text-[#6B78E3] transition-colors"
                >
                  <ExternalLink size={12} />
                  View documentation
                </a>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Main Component ────────────────────────────────────────────────────────────

export default function McpMarketplace({ resetKey }: { resetKey?: number }) {
  const [allEntries, setAllEntries] = useState<McpMarketplaceEntry[]>([]);
  const [results, setResults] = useState<McpMarketplaceEntry[]>([]);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<McpMarketplaceEntry | null>(null);
  const [installedNames, setInstalledNames] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(true);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Return to landing when the nav item is clicked again
  useEffect(() => {
    if (resetKey !== undefined) {
      setSelected(null);
      setQuery("");
    }
  }, [resetKey]);

  useEffect(() => {
    (async () => {
      try {
        const raw: string = await invoke("list_mcp_marketplace");
        const entries: McpMarketplaceEntry[] = JSON.parse(raw);
        setAllEntries(entries);
        setResults(entries);
      } catch (err) {
        console.error("Failed to load MCP marketplace:", err);
      }
      try {
        const names: string[] = await invoke("list_mcp_server_configs");
        setInstalledNames(new Set(names));
      } catch { /* non-fatal */ }
      setLoading(false);
    })();
  }, []);

  const doSearch = useCallback(
    async (q: string) => {
      if (!q.trim()) { setResults(allEntries); return; }
      try {
        const raw: string = await invoke("search_mcp_marketplace", { query: q });
        setResults(JSON.parse(raw));
      } catch {
        const lower = q.toLowerCase();
        setResults(allEntries.filter((e) =>
          e.name.toLowerCase().includes(lower) ||
          e.display_name.toLowerCase().includes(lower) ||
          e.description.toLowerCase().includes(lower) ||
          e.category.toLowerCase().includes(lower) ||
          e.author.toLowerCase().includes(lower) ||
          e.tags.some((t) => t.toLowerCase().includes(lower))
        ));
      }
    },
    [allEntries]
  );

  useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => doSearch(query), 200);
    return () => { if (debounceRef.current) clearTimeout(debounceRef.current); };
  }, [query, doSearch]);

  const handleInstalled = useCallback((name: string) => {
    setInstalledNames((prev) => new Set([...prev, name]));
  }, []);

  // Detail view
  if (selected) {
    return (
      <McpDetail
        entry={selected}
        installed={installedNames.has(selected.name)}
        onClose={() => setSelected(null)}
        onInstalled={handleInstalled}
      />
    );
  }

  // Landing
  return (
    <div className="flex h-full flex-col overflow-y-auto custom-scrollbar bg-[#222327]">
      <div className="flex flex-col items-center px-8 pt-12 pb-10 w-full">
        <div className="w-full max-w-4xl">

          {/* Heading */}
          <div className="text-center mb-8">
            <div className="w-14 h-14 mx-auto mb-5 rounded-2xl bg-[#F59E0B]/10 border border-[#F59E0B]/20 flex items-center justify-center">
              <Server size={22} className="text-[#F59E0B]" strokeWidth={1.5} />
            </div>
            <h2 className="text-[20px] font-semibold text-[#E0E1E6] mb-2">MCP Marketplace</h2>
            <p className="text-[13px] text-[#8A8C93] leading-relaxed max-w-md mx-auto">
              Curated MCP servers ready to install. Each one adds a new capability to your agents —
              files, databases, APIs, browsers, and more.
            </p>
          </div>

          {/* Search */}
          <div className="relative mb-8">
            <Search size={16} className="absolute left-4 top-1/2 -translate-y-1/2 text-[#8A8C93] pointer-events-none" />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search by name, category, or tag…"
              autoFocus
              className="w-full bg-[#1A1A1E] border border-[#33353A] hover:border-[#44474F] focus:border-[#5E6AD2] rounded-xl pl-11 pr-10 py-3.5 text-[14px] text-[#E0E1E6] placeholder-[#8A8C93]/60 outline-none transition-colors shadow-sm"
            />
            {query && (
              <button
                onClick={() => setQuery("")}
                className="absolute right-4 top-1/2 -translate-y-1/2 text-[#8A8C93] hover:text-[#E0E1E6] transition-colors"
              >
                <X size={14} />
              </button>
            )}
          </div>

          {/* Results */}
          {loading ? (
            <div className="flex items-center justify-center py-16">
              <Loader2 size={20} className="animate-spin text-[#8A8C93]" />
            </div>
          ) : results.length === 0 ? (
            <div className="text-center py-16">
              <div className="w-12 h-12 rounded-full border-2 border-dashed border-[#33353A] flex items-center justify-center mx-auto mb-4">
                <Search size={18} className="text-[#8A8C93]" />
              </div>
              <h3 className="text-[14px] font-medium text-[#E0E1E6] mb-1">No servers found</h3>
              <p className="text-[13px] text-[#8A8C93]">Try a different search term</p>
              <button onClick={() => setQuery("")} className="mt-4 text-[12px] text-[#5E6AD2] hover:text-[#6B78E3] transition-colors">
                Clear search
              </button>
            </div>
          ) : (
            <>
              <div className="flex items-center justify-between mb-4">
                <h3 className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase">
                  {query.trim() ? `${results.length} result${results.length !== 1 ? "s" : ""}` : "Featured Servers"}
                </h3>
                {!query.trim() && installedNames.size > 0 && (
                  <span className="text-[11px] text-[#4ADE80]">{installedNames.size} installed</span>
                )}
              </div>
              <div className="grid grid-cols-2 gap-4">
                {results.map((e) => (
                  <McpCard
                    key={e.name}
                    entry={e}
                    installed={installedNames.has(e.name)}
                    onClick={() => setSelected(e)}
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
