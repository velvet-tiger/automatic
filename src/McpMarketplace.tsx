import { useState, useEffect, useRef, useCallback } from "react";
import {
  Search,
  Server,
  Loader2,
  ArrowRight,
  ArrowLeft,
  X,
  Terminal,
  Globe,
  ExternalLink,
  KeyRound,
  Package,
  Github,
} from "lucide-react";

// ── Registry API types ────────────────────────────────────────────────────────

interface RegistryEnvVar {
  name: string;
  description?: string;
  isRequired?: boolean;
  isSecret?: boolean;
  format?: string;
}

interface RegistryPackage {
  registryType: string; // "npm" | "pypi" | "oci" | ...
  identifier: string;
  version?: string;
  transport?: { type: string };
  environmentVariables?: RegistryEnvVar[];
}

interface RegistryRemote {
  type: string; // "streamable-http" | "sse"
  url: string;
  headers?: { name: string; description?: string; isRequired?: boolean; isSecret?: boolean }[];
}

interface RegistryRepository {
  url?: string;
  source?: string;
  subfolder?: string;
}

interface RegistryServer {
  name: string;
  title?: string;
  description?: string;
  version?: string;
  websiteUrl?: string;
  repository?: RegistryRepository;
  packages?: RegistryPackage[];
  remotes?: RegistryRemote[];
}

interface RegistryEntry {
  server: RegistryServer;
  _meta?: {
    "io.modelcontextprotocol.registry/official"?: {
      status: string;
      publishedAt: string;
      updatedAt: string;
      isLatest: boolean;
    };
  };
}

interface RegistryResponse {
  servers: RegistryEntry[];
  metadata: {
    nextCursor?: string;
    count: number;
  };
}

// ── Constants ─────────────────────────────────────────────────────────────────

const REGISTRY_BASE = "https://registry.modelcontextprotocol.io";
const PAGE_SIZE = 20;

// ── Helpers ──────────────────────────────────────────────────────────────────

function displayName(entry: RegistryEntry): string {
  return entry.server.title || entry.server.name.split("/").pop() || entry.server.name;
}

function transports(entry: RegistryEntry): string[] {
  const types = new Set<string>();
  (entry.server.packages ?? []).forEach((p) => {
    if (p.transport?.type) types.add(p.transport.type);
  });
  (entry.server.remotes ?? []).forEach((r) => types.add(r.type));
  return [...types];
}

function registryUrl(name: string): string {
  return `${REGISTRY_BASE}/?name=${encodeURIComponent(name)}`;
}

async function fetchRegistry(params: Record<string, string>): Promise<RegistryResponse> {
  const url = new URL(`${REGISTRY_BASE}/v0/servers`);
  Object.entries(params).forEach(([k, v]) => url.searchParams.set(k, v));
  const res = await fetch(url.toString());
  if (!res.ok) throw new Error(`Registry returned ${res.status}`);
  return res.json();
}

// ── Transport badge ───────────────────────────────────────────────────────────

function TransportBadge({ type }: { type: string }) {
  const isStdio = type === "stdio";
  return (
    <span className={`inline-flex items-center gap-1 text-[10px] px-1.5 py-0.5 rounded font-mono ${
      isStdio ? "bg-[#2D2E36] text-[#8A8C93]" : "bg-[#22D3EE]/10 text-[#22D3EE]"
    }`}>
      {isStdio ? <Terminal size={9} /> : <Globe size={9} />}
      {type}
    </span>
  );
}

// ── Package type badge ────────────────────────────────────────────────────────

function PackageTypeBadge({ type }: { type: string }) {
  const colours: Record<string, string> = {
    npm:  "bg-[#CB3837]/10 text-[#CB3837]",
    pypi: "bg-[#3775A9]/10 text-[#3775A9]",
    oci:  "bg-[#4ADE80]/10 text-[#4ADE80]",
  };
  return (
    <span className={`inline-flex items-center gap-1 text-[10px] px-1.5 py-0.5 rounded font-mono ${colours[type] ?? "bg-[#2D2E36] text-[#8A8C93]"}`}>
      <Package size={9} />
      {type}
    </span>
  );
}

// ── Card ─────────────────────────────────────────────────────────────────────

function McpCard({ entry, onClick }: { entry: RegistryEntry; onClick: () => void }) {
  const name = displayName(entry);
  const types = transports(entry);
  const hasEnvVars = (entry.server.packages ?? []).some(
    (p) => (p.environmentVariables ?? []).length > 0
  );

  return (
    <button
      onClick={onClick}
      className="group text-left p-5 rounded-xl bg-[#1A1A1E] border border-[#33353A] hover:border-[#44474F] hover:bg-[#1E1F24] transition-all flex flex-col"
    >
      <div className="flex items-start justify-between gap-3 mb-3">
        <div className="min-w-0">
          <div className="text-[14px] font-semibold text-[#E0E1E6] leading-snug truncate">
            {name}
          </div>
          <div className="text-[10px] text-[#8A8C93] truncate mt-0.5">{entry.server.name}</div>
        </div>
        <ArrowRight size={13} className="text-[#33353A] group-hover:text-[#8A8C93] transition-colors flex-shrink-0 mt-0.5" />
      </div>

      <p className="text-[12px] text-[#8A8C93] leading-relaxed line-clamp-3 flex-1">
        {entry.server.description || "No description provided."}
      </p>

      <div className="flex items-center gap-1.5 mt-3 flex-wrap">
        {types.map((t) => <TransportBadge key={t} type={t} />)}
        {(entry.server.packages ?? []).map((p, i) => (
          <PackageTypeBadge key={i} type={p.registryType} />
        ))}
        {hasEnvVars && (
          <span className="inline-flex items-center gap-1 text-[10px] px-1.5 py-0.5 rounded bg-[#F59E0B]/10 text-[#F59E0B]">
            <KeyRound size={9} />
            env vars
          </span>
        )}
      </div>
    </button>
  );
}

// ── Detail view ───────────────────────────────────────────────────────────────

function McpDetail({ entry, onClose }: { entry: RegistryEntry; onClose: () => void }) {
  const name = displayName(entry);
  const types = transports(entry);
  const repoUrl = entry.server.repository?.url;
  const websiteUrl = entry.server.websiteUrl;

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
          <span className="text-[14px] font-semibold text-[#E0E1E6] truncate">{name}</span>
          {types.map((t) => <TransportBadge key={t} type={t} />)}
        </div>
        <div className="flex items-center gap-2">
          {repoUrl && (
            <a
              href={repoUrl}
              target="_blank"
              rel="noopener noreferrer"
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[12px] font-medium bg-[#2D2E36] border border-[#33353A] hover:border-[#44474F] text-[#E0E1E6] transition-colors"
            >
              <Github size={12} />
              Repository
            </a>
          )}
          <a
            href={registryUrl(entry.server.name)}
            target="_blank"
            rel="noopener noreferrer"
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[12px] font-medium bg-[#5E6AD2] hover:bg-[#6B78E3] text-white transition-colors"
          >
            <ExternalLink size={12} />
            View on Registry
          </a>
        </div>
      </div>

      {/* Body */}
      <div className="flex-1 overflow-y-auto custom-scrollbar">
        <div className="max-w-3xl mx-auto px-8 py-8 space-y-8">

          {/* Description */}
          <p className="text-[14px] text-[#C0C1C6] leading-relaxed pb-8 border-b border-[#33353A]">
            {entry.server.description || "No description provided."}
          </p>

          <div className="grid grid-cols-[1fr_240px] gap-8">

            {/* Left — packages + remotes */}
            <div className="space-y-7">

              {(entry.server.packages ?? []).length > 0 && (
                <div>
                  <div className="flex items-center gap-2 mb-3">
                    <Package size={13} className="text-[#5E6AD2]" />
                    <span className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase">Packages</span>
                  </div>
                  <div className="space-y-4">
                    {entry.server.packages!.map((pkg, i) => (
                      <div key={i} className="bg-[#111114] border border-[#33353A] rounded-lg p-4">
                        <div className="flex items-center gap-2 mb-2">
                          <PackageTypeBadge type={pkg.registryType} />
                          {pkg.transport && <TransportBadge type={pkg.transport.type} />}
                        </div>
                        <code className="text-[12px] text-[#E0E1E6] font-mono">{pkg.identifier}</code>
                        {pkg.version && (
                          <span className="ml-2 text-[11px] text-[#8A8C93]">v{pkg.version}</span>
                        )}
                        {(pkg.environmentVariables ?? []).length > 0 && (
                          <div className="mt-3 space-y-2">
                            <div className="text-[10px] font-semibold text-[#8A8C93] uppercase tracking-wider">
                              Environment Variables
                            </div>
                            {pkg.environmentVariables!.map((ev) => (
                              <div key={ev.name} className="flex items-start gap-2 flex-wrap">
                                <code className="text-[11px] font-mono text-[#5E6AD2] flex-shrink-0">{ev.name}</code>
                                {ev.isRequired && (
                                  <span className="text-[10px] text-[#F59E0B] flex-shrink-0">required</span>
                                )}
                                {ev.isSecret && (
                                  <span className="inline-flex items-center gap-0.5 text-[10px] text-[#8A8C93] flex-shrink-0">
                                    <KeyRound size={9} /> secret
                                  </span>
                                )}
                                {ev.description && (
                                  <span className="text-[11px] text-[#8A8C93]">{ev.description}</span>
                                )}
                              </div>
                            ))}
                          </div>
                        )}
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {(entry.server.remotes ?? []).length > 0 && (
                <div>
                  <div className="flex items-center gap-2 mb-3">
                    <Globe size={13} className="text-[#22D3EE]" />
                    <span className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase">Remote Endpoints</span>
                  </div>
                  <div className="space-y-3">
                    {entry.server.remotes!.map((remote, i) => (
                      <div key={i} className="bg-[#111114] border border-[#33353A] rounded-lg p-4">
                        <div className="flex items-center gap-2 mb-2">
                          <TransportBadge type={remote.type} />
                        </div>
                        <code className="text-[12px] text-[#E0E1E6] font-mono break-all">{remote.url}</code>
                        {(remote.headers ?? []).length > 0 && (
                          <div className="mt-3 space-y-1.5">
                            <div className="text-[10px] font-semibold text-[#8A8C93] uppercase tracking-wider">Headers</div>
                            {remote.headers!.map((h) => (
                              <div key={h.name} className="flex items-center gap-2 flex-wrap">
                                <code className="text-[11px] font-mono text-[#5E6AD2]">{h.name}</code>
                                {h.isRequired && <span className="text-[10px] text-[#F59E0B]">required</span>}
                                {h.description && <span className="text-[11px] text-[#8A8C93]">{h.description}</span>}
                              </div>
                            ))}
                          </div>
                        )}
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>

            {/* Right sidebar */}
            <div className="space-y-6">
              <div>
                <div className="text-[10px] font-semibold text-[#8A8C93] uppercase tracking-wider mb-1.5">Registry ID</div>
                <code className="text-[11px] text-[#C0C1C6] font-mono break-all">{entry.server.name}</code>
              </div>

              {entry.server.version && (
                <div>
                  <div className="text-[10px] font-semibold text-[#8A8C93] uppercase tracking-wider mb-1.5">Version</div>
                  <span className="text-[13px] text-[#E0E1E6]">{entry.server.version}</span>
                </div>
              )}

              {websiteUrl && (
                <div>
                  <div className="text-[10px] font-semibold text-[#8A8C93] uppercase tracking-wider mb-1.5">Website</div>
                  <a
                    href={websiteUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="flex items-center gap-1.5 text-[12px] text-[#5E6AD2] hover:text-[#6B78E3] transition-colors"
                  >
                    <ExternalLink size={12} />
                    <span className="truncate">{websiteUrl.replace(/^https?:\/\//, "")}</span>
                  </a>
                </div>
              )}

              {repoUrl && (
                <div>
                  <div className="text-[10px] font-semibold text-[#8A8C93] uppercase tracking-wider mb-1.5">Repository</div>
                  <a
                    href={repoUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="flex items-center gap-1.5 text-[12px] text-[#5E6AD2] hover:text-[#6B78E3] transition-colors"
                  >
                    <Github size={12} className="flex-shrink-0" />
                    <span className="truncate">{repoUrl.replace("https://github.com/", "")}</span>
                  </a>
                </div>
              )}

              <div className="p-3.5 rounded-lg bg-[#1A1A1E] border border-[#33353A]">
                <div className="text-[10px] font-semibold text-[#8A8C93] tracking-wider uppercase mb-2">
                  How to use
                </div>
                <p className="text-[11px] text-[#8A8C93] leading-relaxed">
                  View this server on the MCP Registry for full installation instructions and documentation.
                </p>
                <a
                  href={registryUrl(entry.server.name)}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="mt-2 flex items-center gap-1 text-[11px] text-[#5E6AD2] hover:text-[#6B78E3] transition-colors"
                >
                  <ExternalLink size={10} />
                  registry.modelcontextprotocol.io
                </a>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Main Component ────────────────────────────────────────────────────────────

export default function McpMarketplace({ resetKey }: { resetKey?: number }) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<RegistryEntry[]>([]);
  const [featured, setFeatured] = useState<RegistryEntry[]>([]);
  const [selected, setSelected] = useState<RegistryEntry | null>(null);
  const [loading, setLoading] = useState(false);
  const [featuredLoading, setFeaturedLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [loadingMore, setLoadingMore] = useState(false);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Return to landing when nav item is clicked again
  useEffect(() => {
    if (resetKey !== undefined) {
      setSelected(null);
      setQuery("");
      setResults([]);
      setNextCursor(null);
      setError(null);
    }
  }, [resetKey]);

  // Load featured servers on mount — pull the most recent page from the registry
  useEffect(() => {
    (async () => {
      setFeaturedLoading(true);
      try {
        const data = await fetchRegistry({ limit: String(PAGE_SIZE) });
        setFeatured(data.servers);
      } catch {
        setFeatured([]);
      } finally {
        setFeaturedLoading(false);
      }
    })();
  }, []);

  const doSearch = useCallback(async (q: string) => {
    if (!q.trim()) {
      setResults([]);
      setNextCursor(null);
      setError(null);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const data = await fetchRegistry({ search: q, limit: String(PAGE_SIZE) });
      setResults(data.servers);
      setNextCursor(data.metadata.nextCursor ?? null);
    } catch (err: any) {
      setError(`Search failed: ${err.message ?? err}`);
      setResults([]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => doSearch(query), 300);
    return () => { if (debounceRef.current) clearTimeout(debounceRef.current); };
  }, [query, doSearch]);

  const loadMore = useCallback(async () => {
    if (!nextCursor || loadingMore) return;
    setLoadingMore(true);
    try {
      const params: Record<string, string> = { limit: String(PAGE_SIZE), cursor: nextCursor };
      if (query.trim()) params.search = query;
      const data = await fetchRegistry(params);
      setResults((prev) => [...prev, ...data.servers]);
      setNextCursor(data.metadata.nextCursor ?? null);
    } catch (err: any) {
      setError(`Failed to load more: ${err.message ?? err}`);
    } finally {
      setLoadingMore(false);
    }
  }, [nextCursor, loadingMore, query]);

  // Detail view
  if (selected) {
    return <McpDetail entry={selected} onClose={() => setSelected(null)} />;
  }

  const showingSearch = !!query.trim();
  const showingResults = showingSearch ? results : featured;
  const isLoadingMain = showingSearch ? loading : featuredLoading;

  return (
    <div className="flex h-full flex-col overflow-y-auto custom-scrollbar bg-[#222327]">
      <div className="flex flex-col items-center px-8 pt-12 pb-10 w-full">
        <div className="w-full max-w-4xl">

          {/* Heading */}
          <div className="text-center mb-8">
            <div className="w-14 h-14 mx-auto mb-5 rounded-2xl bg-[#F59E0B]/10 border border-[#F59E0B]/20 flex items-center justify-center">
              <Server size={22} className="text-[#F59E0B]" strokeWidth={1.5} />
            </div>
            <h2 className="text-[20px] font-semibold text-[#E0E1E6] mb-2">MCP Registry</h2>
            <p className="text-[13px] text-[#8A8C93] leading-relaxed max-w-md mx-auto">
              Browse and discover MCP servers from the{" "}
              <a
                href="https://registry.modelcontextprotocol.io"
                target="_blank"
                rel="noopener noreferrer"
                className="text-[#5E6AD2] hover:text-[#6B78E3] transition-colors"
              >
                official MCP Registry
              </a>
              . Live data — always up to date.
            </p>
          </div>

          {/* Search */}
          <div className="relative mb-8">
            <Search size={16} className="absolute left-4 top-1/2 -translate-y-1/2 text-[#8A8C93] pointer-events-none" />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search servers by name, description…"
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

          {/* Error */}
          {error && (
            <div className="mb-6 px-4 py-3 rounded-lg bg-red-500/10 border border-red-500/20 text-[13px] text-red-400">
              {error}
            </div>
          )}

          {/* Results */}
          {isLoadingMain ? (
            <div className="flex items-center justify-center py-16">
              <Loader2 size={20} className="animate-spin text-[#8A8C93]" />
            </div>
          ) : showingResults.length === 0 && showingSearch ? (
            <div className="text-center py-16">
              <div className="w-12 h-12 rounded-full border-2 border-dashed border-[#33353A] flex items-center justify-center mx-auto mb-4">
                <Search size={18} className="text-[#8A8C93]" />
              </div>
              <h3 className="text-[14px] font-medium text-[#E0E1E6] mb-1">No servers found</h3>
              <p className="text-[13px] text-[#8A8C93]">Try a different search term</p>
              <button
                onClick={() => setQuery("")}
                className="mt-4 text-[12px] text-[#5E6AD2] hover:text-[#6B78E3] transition-colors"
              >
                Clear search
              </button>
            </div>
          ) : showingResults.length > 0 ? (
            <>
              <div className="flex items-center justify-between mb-4">
                <h3 className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase">
                  {showingSearch
                    ? `${results.length} result${results.length !== 1 ? "s" : ""}`
                    : "Recently Updated"}
                </h3>
              </div>
              <div className="grid grid-cols-2 gap-4">
                {showingResults.map((e, i) => (
                  <McpCard key={`${e.server.name}-${i}`} entry={e} onClick={() => setSelected(e)} />
                ))}
              </div>

              {/* Load more (search only — featured doesn't paginate) */}
              {showingSearch && nextCursor && (
                <div className="mt-6 flex justify-center">
                  <button
                    onClick={loadMore}
                    disabled={loadingMore}
                    className="flex items-center gap-2 px-4 py-2 rounded-lg bg-[#2D2E36] border border-[#33353A] hover:border-[#44474F] text-[13px] text-[#E0E1E6] transition-colors disabled:opacity-50"
                  >
                    {loadingMore && <Loader2 size={13} className="animate-spin" />}
                    Load more
                  </button>
                </div>
              )}
            </>
          ) : null}
        </div>
      </div>
    </div>
  );
}
