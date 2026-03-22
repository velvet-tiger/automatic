import { useState, useEffect, useMemo, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AuthorSection } from "../../components/AuthorPanel";
import { handleExternalLinkClick } from "../../lib/externalLinks";
import {
  Search,
  Server,
  ArrowLeft,
  ArrowRight,
  ExternalLink,
  Github,
  Copy,
  Check,
  Globe,
  Terminal,
  Key,
  Package,
  Shield,
  Cloud,
  Monitor,
  Lock,
  Download,
  CheckCircle2,
  Loader2,
  BookOpen,
  Sparkles,
} from "lucide-react";
// ── Types ──────────────────────────────────────────────────────────────────

interface EnvVar {
  name: string;
  description: string;
  secret: boolean;
}

interface CompanionSkill {
  /** Skill name used as the key when saving, e.g. "aikido-security" */
  name: string;
  /** Human-readable display name */
  title: string;
  /** One-line description shown in the callout */
  description: string;
  /** Raw URL to download the skill content from */
  url: string;
  /**
   * GitHub owner or owner/repo used to record skill origin, e.g. "aikidosec".
   * Stored as the `source` in skills.json so the Skills view can resolve the
   * provider's GitHub profile instead of falling back to "local".
   */
  github_source: string;
}

interface McpServer {
  slug: string;
  name: string;
  title: string;
  description: string;
  provider: string;
  /** Optional brand domain for Brandfetch icon lookup, e.g. "github.com" */
  icon?: string;
  classification: string;
  repository_url: string | null;
  /** Optional link to the provider's documentation or setup guide for this server */
  docs_url?: string | null;
  remote: { transport: string; url: string } | null;
  local: {
    registry: string;
    package: string;
    version: string | null;
    transport: string;
    command: string;
  } | null;
  auth: { method: string; env_vars: EnvVar[] };
  /** Optional skill that should be installed alongside this server */
  companion_skill?: CompanionSkill | null;
}

// servers is loaded asynchronously in the component via invoke("search_mcp_marketplace")

// ── Constants ──────────────────────────────────────────────────────────────

const ACCENT = "var(--icon-mcp)"; // amber — ICONS.mcp.hex
const ACCENT_BG = "bg-icon-mcp/10";
const ACCENT_BORDER = "border-icon-mcp/20";

const CLASSIFICATIONS = ["all", "official", "reference", "community"] as const;

const CLASSIFICATION_LABELS: Record<string, string> = {
  all: "All",
  official: "Official",
  reference: "Reference",
  community: "Community",
};

// ── Transport filters ──────────────────────────────────────────────────────

const TRANSPORT_FILTERS = ["remote", "local", "auth"] as const;
type TransportFilter = (typeof TRANSPORT_FILTERS)[number];

const TRANSPORT_LABELS: Record<TransportFilter, string> = {
  remote: "Remote",
  local: "Local",
  auth: "Auth Required",
};

const CLASSIFICATION_COLORS: Record<string, string> = {
  official: "bg-brand/15 text-brand border-brand/20",
  reference: "bg-accent/15 text-accent border-accent/20",
  community: "bg-accent-hover/15 text-accent-hover border-accent-hover/20",
};

// ── Server Icon ────────────────────────────────────────────────────────────

const BRANDFETCH_CLIENT_ID = import.meta.env.VITE_BRANDFETCH_CLIENT_ID as string | undefined;

function brandfetchUrl(domain: string, px: number): string {
  const s = Math.min(px * 2, 64);
  return `https://cdn.brandfetch.io/${encodeURIComponent(domain)}/w/${s}/h/${s}/theme/dark/fallback/lettermark/type/icon?c=${BRANDFETCH_CLIENT_ID ?? ""}`;
}

/**
 * Renders the server's brand icon via Brandfetch if an `icon` domain is set,
 * with an amber Server icon as the fallback.
 */
function McpServerIcon({ server, size }: { server: McpServer; size: number }) {
  const [imgError, setImgError] = useState(false);

  if (server.icon && BRANDFETCH_CLIENT_ID && !imgError) {
    return (
      <img
        src={brandfetchUrl(server.icon, size)}
        alt={server.title}
        width={size}
        height={size}
        onError={() => setImgError(true)}
        className="flex-shrink-0 rounded-md object-contain"
        style={{ width: size, height: size }}
        draggable={false}
      />
    );
  }

  return (
    <div
      className={`flex-shrink-0 rounded-md ${ACCENT_BG} border ${ACCENT_BORDER} flex items-center justify-center`}
      style={{ width: size, height: size }}
    >
      <Server size={Math.round(size * 0.5)} style={{ color: ACCENT }} />
    </div>
  );
}

// ── Helpers ────────────────────────────────────────────────────────────────

function classificationBadge(classification: string) {
  const cls =
    CLASSIFICATION_COLORS[classification] ||
    "bg-surface text-text-muted border-border-strong/40";
  return (
    <span
      className={`inline-flex items-center px-2 py-0.5 rounded text-[10px] font-medium border ${cls}`}
    >
      {classification}
    </span>
  );
}

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }, [text]);

  return (
    <button
      onClick={handleCopy}
      className="flex-shrink-0 p-1.5 rounded hover:bg-surface transition-colors"
      title="Copy to clipboard"
    >
      {copied ? (
        <Check size={12} className="text-success" />
      ) : (
        <Copy size={12} className="text-text-muted" />
      )}
    </button>
  );
}

function hasRemote(s: McpServer): boolean {
  return s.remote !== null;
}

function hasLocal(s: McpServer): boolean {
  return s.local !== null;
}

function hasAuth(s: McpServer): boolean {
  return s.auth.method !== "none" && s.auth.env_vars.length > 0;
}

// ── Component ──────────────────────────────────────────────────────────────

/** Build config name from server title: "GitHub (Anthropic Reference)" → "github-anthropic-reference" */
function configName(server: McpServer): string {
  return server.title.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
}

/** Build a save-ready config JSON from marketplace data. Prefers local, falls back to remote.
 *  Embeds `_author` metadata so Automatic can display the provider in the MCP Servers view.
 */
function buildConfig(server: McpServer): Record<string, unknown> {
  // Author metadata derived from marketplace data
  const _author: Record<string, string> = { name: server.provider };
  if (server.repository_url) _author.repository_url = server.repository_url;

  if (server.local) {
    const parts = server.local.command.split(/\s+/);
    const cmd = parts[0] || "";
    const args = parts.slice(1);
    const env: Record<string, string> = {};
    server.auth.env_vars.forEach((v) => {
      env[v.name] = "";
    });
    const cfg: Record<string, unknown> = { type: "stdio", command: cmd, _author };
    if (args.length > 0) cfg.args = args;
    if (Object.keys(env).length > 0) cfg.env = env;
    return cfg;
  }
  if (server.remote) {
    const type = server.remote.transport === "sse" ? "sse" : "http";
    const env: Record<string, string> = {};
    server.auth.env_vars.forEach((v) => { env[v.name] = ""; });
    const cfg: Record<string, unknown> = { type, url: server.remote.url, _author };
    if (Object.keys(env).length > 0) cfg.env = env;
    return cfg;
  }
  return { type: "stdio", command: "", _author };
}

export default function McpMarketplace({
  resetKey,
  initialSlug,
  onInitialSlugConsumed,
  initialQuery,
  onInitialQueryConsumed,
}: {
  resetKey?: number;
  initialSlug?: string | null;
  onInitialSlugConsumed?: () => void;
  initialQuery?: string | null;
  onInitialQueryConsumed?: () => void;
}) {
  const [servers, setServers] = useState<McpServer[]>([]);
  const [serversLoading, setServersLoading] = useState(true);
  const [query, setQuery] = useState("");
  const [classification, setClassification] = useState<string>("all");
  const [transportFilter, setTransportFilter] = useState<TransportFilter | null>(null);
  const [selected, setSelected] = useState<McpServer | null>(null);
  const [setupTab, setSetupTab] = useState<"remote" | "local" | "auth">(
    "local"
  );
  const [installedServers, setInstalledServers] = useState<Set<string>>(new Set());
  const [installing, setInstalling] = useState(false);
  const [installError, setInstallError] = useState<string | null>(null);
  const [installedSkills, setInstalledSkills] = useState<Set<string>>(new Set());
  const [installingSkill, setInstallingSkill] = useState(false);
  const [skillInstallError, setSkillInstallError] = useState<string | null>(null);

  // Load marketplace catalogue from ~/.automatic/marketplace/mcp-servers.json
  const loadServers = useCallback(async () => {
    setServersLoading(true);
    try {
      const json: string = await invoke("search_mcp_marketplace", { query: "" });
      setServers(JSON.parse(json) as McpServer[]);
    } catch {
      // non-fatal: leave servers empty
    } finally {
      setServersLoading(false);
    }
  }, []);

  // Load installed MCP servers
  const loadInstalled = useCallback(async () => {
    try {
      const result: string[] = await invoke("list_mcp_server_configs");
      setInstalledServers(new Set(result));
    } catch {
      // non-fatal
    }
  }, []);

  // Load installed skills
  const loadInstalledSkills = useCallback(async () => {
    try {
      const result: string[] = await invoke("list_skills");
      setInstalledSkills(new Set(result));
    } catch {
      // non-fatal
    }
  }, []);

  useEffect(() => { loadServers(); loadInstalled(); loadInstalledSkills(); }, [loadServers, loadInstalled, loadInstalledSkills]);

  // Install a companion skill from a raw URL, recording its remote origin so
  // it shows as provider-managed rather than local in the Skills view.
  const handleInstallSkill = useCallback(async (skill: CompanionSkill) => {
    setInstallingSkill(true);
    setSkillInstallError(null);
    try {
      const response = await fetch(skill.url);
      if (!response.ok) throw new Error(`Failed to fetch skill: ${response.status}`);
      const content = await response.text();
      // Use import_remote_skill so the skill source is recorded (shows as
      // managed by the provider rather than "local" in the Skills view).
      await invoke("import_remote_skill", {
        name: skill.name,
        content,
        source: skill.github_source,
        id: `${skill.github_source}/${skill.name}`,
      });
      setInstalledSkills((prev) => new Set([...prev, skill.name]));
    } catch (err: unknown) {
      setSkillInstallError(`Failed to install skill: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setInstallingSkill(false);
    }
  }, []);

  // Reset when the nav item is re-clicked
  useEffect(() => {
    if (resetKey !== undefined) {
      setSelected(null);
      setQuery("");
      setClassification("all");
      setTransportFilter(null);
      setInstallError(null);
      setSkillInstallError(null);
    }
  }, [resetKey]);

  // Install a server config
  const handleInstall = useCallback(async (server: McpServer) => {
    setInstalling(true);
    setInstallError(null);
    try {
      const name = configName(server);
      const data = JSON.stringify(buildConfig(server));
      await invoke("save_mcp_server_config", { name, data });
      setInstalledServers((prev) => new Set([...prev, name]));
    } catch (err: any) {
      setInstallError(`Failed to add server: ${err}`);
    } finally {
      setInstalling(false);
    }
  }, []);

  // Base list after applying the search query only (used for cross-filter counts)
  const queryFiltered = useMemo(() => {
    if (!query.trim()) return servers;
    const q = query.toLowerCase();
    return servers.filter(
      (s) =>
        s.title.toLowerCase().includes(q) ||
        s.description.toLowerCase().includes(q) ||
        s.provider.toLowerCase().includes(q) ||
        s.slug.toLowerCase().includes(q)
    );
  }, [query, servers]);

  // Classification badge counts: how many match each classification given the
  // active transport filter (+ query), so clicking a badge shows a truthful count.
  const tagCounts = useMemo(() => {
    const base = transportFilter === "remote" ? queryFiltered.filter(hasRemote)
               : transportFilter === "local"  ? queryFiltered.filter(hasLocal)
               : transportFilter === "auth"   ? queryFiltered.filter(hasAuth)
               : queryFiltered;
    const counts: Record<string, number> = { all: base.length };
    for (const s of base) {
      counts[s.classification] = (counts[s.classification] ?? 0) + 1;
    }
    return counts;
  }, [queryFiltered, transportFilter]);

  // Transport badge counts: how many match each transport given the active
  // classification filter (+ query), so clicking a badge shows a truthful count.
  const transportCounts = useMemo(() => {
    const base = classification === "all"
      ? queryFiltered
      : queryFiltered.filter((s) => s.classification === classification);
    return {
      remote: base.filter(hasRemote).length,
      local:  base.filter(hasLocal).length,
      auth:   base.filter(hasAuth).length,
    };
  }, [queryFiltered, classification]);

  // Final filtered + sorted list
  const filtered = useMemo(() => {
    let list = queryFiltered;
    if (classification !== "all") {
      list = list.filter((s) => s.classification === classification);
    }
    if (transportFilter === "remote") list = list.filter(hasRemote);
    if (transportFilter === "local")  list = list.filter(hasLocal);
    if (transportFilter === "auth")   list = list.filter(hasAuth);
    return [...list].sort((a, b) => a.title.localeCompare(b.title));
  }, [queryFiltered, classification, transportFilter]);

  // When selecting a server, pick the best default tab
  const handleSelect = useCallback((server: McpServer) => {
    setSelected(server);
    if (hasRemote(server)) {
      setSetupTab("remote");
    } else if (hasLocal(server)) {
      setSetupTab("local");
    } else if (hasAuth(server)) {
      setSetupTab("auth");
    } else {
      setSetupTab("local");
    }
  }, []);

  // ── Auto-select from deep-link ───────────────────────────────────────────
  useEffect(() => {
    if (!initialSlug) return;
    const server = servers.find((s) => s.slug === initialSlug);
    if (server) handleSelect(server);
    onInitialSlugConsumed?.();
  }, [initialSlug, servers, handleSelect, onInitialSlugConsumed]);

  // ── Pre-populate search from deep-link query ─────────────────────────────
  useEffect(() => {
    if (!initialQuery) return;
    setQuery(initialQuery);
    onInitialQueryConsumed?.();
  }, [initialQuery, onInitialQueryConsumed]);

  // ── Detail view ──────────────────────────────────────────────────────────

  if (selected) {
    const tabs: { id: "remote" | "local" | "auth"; label: string; icon: typeof Globe; available: boolean }[] = [
      { id: "remote", label: "Remote", icon: Cloud, available: hasRemote(selected) },
      { id: "local", label: "Local Install", icon: Monitor, available: hasLocal(selected) },
      { id: "auth", label: "Authentication", icon: Lock, available: hasAuth(selected) },
    ];

    return (
      <div className="flex h-full flex-col overflow-y-auto custom-scrollbar bg-bg-base">
        <div className="flex flex-col items-center px-8 pt-8 pb-10 w-full">
          <div className="w-full max-w-2xl">
            {/* Back button */}
            <button
              onClick={() => setSelected(null)}
              className="flex items-center gap-1.5 text-[12px] text-text-muted hover:text-text-base transition-colors mb-6"
            >
              <ArrowLeft size={13} />
              Back to directory
            </button>

            {/* Header */}
            <div className="flex items-start gap-4 mb-6">
              <McpServerIcon server={selected} size={48} />
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2.5 mb-1.5">
                  <h1 className="text-[20px] font-semibold text-text-base leading-tight">
                    {selected.title}
                  </h1>
                  {classificationBadge(selected.classification)}
                </div>
                <p className="text-[13px] text-text-muted leading-relaxed">
                  {selected.description}
                </p>
              </div>
            </div>

            {/* Install button */}
            {(() => {
              const name = configName(selected);
              const isInstalled = installedServers.has(name);
              return (
                <div className="mb-6">
                  {isInstalled ? (
                    <div className="flex items-center gap-2 px-4 py-2.5 rounded-lg bg-success/10 border border-success/20">
                      <CheckCircle2 size={15} className="text-success" />
                      <span className="text-[13px] text-success font-medium">
                        Added to MCP Servers
                      </span>
                      <span className="text-[11px] text-text-muted ml-1">
                        as "{name}"
                      </span>
                    </div>
                  ) : (
                    <button
                      onClick={() => handleInstall(selected)}
                      disabled={installing}
                      className="flex items-center gap-2 px-4 py-2.5 rounded-lg bg-icon-mcp hover:bg-icon-mcp-hover text-white font-medium text-[13px] transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                    >
                      {installing ? (
                        <>
                          <Loader2 size={14} className="animate-spin" />
                          Adding...
                        </>
                      ) : (
                        <>
                          <Download size={14} />
                          Add to MCP Servers
                        </>
                      )}
                    </button>
                  )}
                  {installError && (
                    <p className="mt-2 text-[12px] text-red-400">{installError}</p>
                  )}
                </div>
              );
            })()}

            {/* Companion skill callout */}
            {selected.companion_skill && (() => {
              const skill = selected.companion_skill!;
              const isSkillInstalled = installedSkills.has(skill.name);
              return (
                <div className="mb-6 p-4 rounded-xl border border-icon-mcp/25 bg-icon-mcp/6">
                  <div className="flex items-start gap-3">
                    <div className="flex-shrink-0 w-8 h-8 rounded-lg bg-icon-mcp/15 border border-icon-mcp/20 flex items-center justify-center">
                      <Sparkles size={14} style={{ color: ACCENT }} />
                    </div>
                    <div className="flex-1 min-w-0">
                      <p className="text-[13px] font-medium text-text-base mb-0.5">
                        {skill.title}
                      </p>
                      <p className="text-[12px] text-text-muted leading-relaxed mb-3">
                        {skill.description}
                      </p>
                      {isSkillInstalled ? (
                        <div className="flex items-center gap-1.5 text-[12px] text-success">
                          <CheckCircle2 size={13} />
                          Skill installed
                        </div>
                      ) : (
                        <div className="flex flex-col gap-1.5">
                          <button
                            onClick={() => handleInstallSkill(skill)}
                            disabled={installingSkill}
                            className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-icon-mcp/15 hover:bg-icon-mcp/25 border border-icon-mcp/25 text-[12px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                            style={{ color: ACCENT }}
                          >
                            {installingSkill ? (
                              <>
                                <Loader2 size={12} className="animate-spin" />
                                Installing…
                              </>
                            ) : (
                              <>
                                <Download size={12} />
                                Install Companion Skill
                              </>
                            )}
                          </button>
                          {skillInstallError && (
                            <p className="text-[11px] text-red-400">{skillInstallError}</p>
                          )}
                        </div>
                      )}
                    </div>
                  </div>
                </div>
              );
            })()}

            {/* Author */}
            <div className="mb-6 pb-6 border-b border-border-strong/40">
              <AuthorSection descriptor={{
                type: "provider",
                name: selected.provider,
                repository_url: selected.repository_url ?? undefined,
              }} />
            </div>

            {/* Meta row */}
            <div className="flex items-center gap-4 mb-6 pb-6 border-b border-border-strong/40">
              <div className="flex items-center gap-1.5 text-[12px] text-text-muted">
                <Package size={12} style={{ color: ACCENT }} />
                <span className="text-text-base font-medium">{selected.provider}</span>
              </div>
              {selected.repository_url && (
                <a
                  href={selected.repository_url}
                  target="_blank"
                  rel="noopener noreferrer"
                  onClick={handleExternalLinkClick(selected.repository_url)}
                  className="flex items-center gap-1.5 text-[12px] hover:text-text-base transition-colors"
                  style={{ color: ACCENT }}
                >
                  <Github size={12} />
                  Repository
                  <ExternalLink size={10} />
                </a>
              )}
              {selected.docs_url && (
                <a
                  href={selected.docs_url}
                  target="_blank"
                  rel="noopener noreferrer"
                  onClick={handleExternalLinkClick(selected.docs_url)}
                  className="flex items-center gap-1.5 text-[12px] hover:text-text-base transition-colors"
                  style={{ color: ACCENT }}
                >
                  <BookOpen size={12} />
                  Documentation
                  <ExternalLink size={10} />
                </a>
              )}
              <div className="flex items-center gap-1.5 text-[12px] text-text-muted">
                {hasRemote(selected) && (
                  <span className="flex items-center gap-1 px-2 py-0.5 rounded bg-icon-mcp/10 text-icon-mcp text-[10px] font-medium">
                    <Cloud size={10} />
                    Remote
                  </span>
                )}
                {hasLocal(selected) && (
                  <span className="flex items-center gap-1 px-2 py-0.5 rounded bg-success/10 text-success text-[10px] font-medium">
                    <Monitor size={10} />
                    Local
                  </span>
                )}
                {hasAuth(selected) && (
                  <span className="flex items-center gap-1 px-2 py-0.5 rounded bg-danger/10 text-danger text-[10px] font-medium">
                    <Lock size={10} />
                    Auth
                  </span>
                )}
              </div>
            </div>

            {/* Setup tabs */}
            <div className="mb-4">
              <div className="flex gap-1 border-b border-border-strong/40">
                {tabs.map((tab) => (
                  <button
                    key={tab.id}
                    onClick={() => tab.available && setSetupTab(tab.id)}
                    disabled={!tab.available}
                    className={`flex items-center gap-1.5 px-4 py-2.5 text-[12px] font-medium border-b-2 transition-colors -mb-[1px] ${
                      setupTab === tab.id && tab.available
                        ? "border-icon-mcp text-text-base"
                        : tab.available
                        ? "border-transparent text-text-muted hover:text-text-base"
                        : "border-transparent text-text-muted cursor-not-allowed"
                    }`}
                  >
                    <tab.icon size={13} />
                    {tab.label}
                    {!tab.available && (
                      <span className="text-[9px] text-text-muted ml-1">N/A</span>
                    )}
                  </button>
                ))}
              </div>
            </div>

            {/* Tab content */}
            <div className="bg-bg-input border border-border-strong/40 rounded-xl p-6">
              {/* Remote tab */}
              {setupTab === "remote" && selected.remote && (
                <div className="space-y-5">
                  <div>
                    <h3 className="text-[11px] font-semibold text-text-muted uppercase tracking-wider mb-3">
                      Remote Endpoint
                    </h3>
                    <p className="text-[12px] text-text-muted mb-4">
                      Connect directly to the hosted server — no local installation needed.
                    </p>
                  </div>

                  <div>
                    <label className="text-[11px] font-medium text-text-muted mb-1.5 block">
                      Transport
                    </label>
                    <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded bg-icon-mcp/10 text-icon-mcp text-[11px] font-mono font-medium">
                      <Globe size={11} />
                      {selected.remote.transport}
                    </span>
                  </div>

                  <div>
                    <label className="text-[11px] font-medium text-text-muted mb-1.5 block">
                      URL
                    </label>
                    <div className="flex items-center gap-2 bg-bg-base border border-border-strong/40 rounded-md px-3 py-2">
                      <code className="flex-1 text-[12px] font-mono text-text-base truncate">
                        {selected.remote.url}
                      </code>
                      <CopyButton text={selected.remote.url} />
                    </div>
                  </div>

                  {/* OAuth callout */}
                  {selected.auth.method === "oauth" && (
                    <div className="flex gap-3 rounded-lg border border-brand/25 bg-brand/8 px-4 py-3">
                      <Shield size={14} className="text-brand mt-0.5 shrink-0" />
                      <div className="space-y-1">
                        <p className="text-[12px] font-medium text-text-base">
                          Authentication required after install
                        </p>
                        <p className="text-[11px] text-text-muted leading-relaxed">
                          This server uses OAuth. After adding it, open the MCP Servers page and click
                          <strong> Authenticate</strong> to complete the OAuth flow. Your token will be stored
                          securely in the system keychain and agents will connect through a local proxy.
                        </p>
                        {selected.docs_url && (
                          <a
                            href={selected.docs_url}
                            target="_blank"
                            rel="noopener noreferrer"
                            onClick={handleExternalLinkClick(selected.docs_url)}
                            className="inline-flex items-center gap-1 text-[11px] text-brand hover:text-brand/80 transition-colors"
                          >
                            <BookOpen size={11} />
                            View setup documentation
                            <ExternalLink size={10} />
                          </a>
                        )}
                      </div>
                    </div>
                  )}

                  {/* Example config */}
                  <div>
                    <label className="text-[11px] font-medium text-text-muted mb-1.5 block">
                      Example Configuration
                    </label>
                    <div className="relative">
                      <pre className="bg-bg-base border border-border-strong/40 rounded-md px-4 py-3 font-mono text-[11px] text-text-base leading-relaxed overflow-x-auto whitespace-pre">
{`{
  "${selected.title.toLowerCase().replace(/[^a-z0-9]+/g, "-")}": {
    "type": "${selected.remote.transport === "sse" ? "sse" : "http"}",
    "url": "${selected.remote.url}"${hasAuth(selected) ? `,
    "headers": {
      "Authorization": "Bearer <YOUR_TOKEN>"
    }` : ""}
  }
}`}
                      </pre>
                      <div className="absolute top-2 right-2">
                        <CopyButton
                          text={JSON.stringify(
                            {
                              [selected.title.toLowerCase().replace(/[^a-z0-9]+/g, "-")]: {
                                type: selected.remote.transport === "sse" ? "sse" : "http",
                                url: selected.remote.url,
                                ...(hasAuth(selected)
                                  ? { headers: { Authorization: "Bearer <YOUR_TOKEN>" } }
                                  : {}),
                              },
                            },
                            null,
                            2
                          )}
                        />
                      </div>
                    </div>
                  </div>
                </div>
              )}

              {/* Local install tab */}
              {setupTab === "local" && selected.local && (
                <div className="space-y-5">
                  <div>
                    <h3 className="text-[11px] font-semibold text-text-muted uppercase tracking-wider mb-3">
                      Local Installation
                    </h3>
                    <p className="text-[12px] text-text-muted mb-4">
                      Run the server locally on your machine via stdio transport.
                    </p>
                  </div>

                  <div className="grid grid-cols-3 gap-4">
                    <div>
                      <label className="text-[11px] font-medium text-text-muted mb-1.5 block">
                        Registry
                      </label>
                      <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded bg-surface text-text-base text-[11px] font-mono">
                        <Package size={11} />
                        {selected.local.registry}
                      </span>
                    </div>
                    <div>
                      <label className="text-[11px] font-medium text-text-muted mb-1.5 block">
                        Transport
                      </label>
                      <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded bg-surface text-text-base text-[11px] font-mono">
                        <Terminal size={11} />
                        {selected.local.transport}
                      </span>
                    </div>
                    {selected.local.version && (
                      <div>
                        <label className="text-[11px] font-medium text-text-muted mb-1.5 block">
                          Version
                        </label>
                        <span className="inline-flex items-center px-2.5 py-1 rounded bg-surface text-text-base text-[11px] font-mono">
                          {selected.local.version}
                        </span>
                      </div>
                    )}
                  </div>

                  <div>
                    <label className="text-[11px] font-medium text-text-muted mb-1.5 block">
                      Package
                    </label>
                    <div className="flex items-center gap-2 bg-bg-base border border-border-strong/40 rounded-md px-3 py-2">
                      <code className="flex-1 text-[12px] font-mono text-text-base truncate">
                        {selected.local.package}
                      </code>
                      <CopyButton text={selected.local.package} />
                    </div>
                  </div>

                  <div>
                    <label className="text-[11px] font-medium text-text-muted mb-1.5 block">
                      Install / Run Command
                    </label>
                    <div className="flex items-center gap-2 bg-bg-base border border-border-strong/40 rounded-md px-3 py-2">
                      <code className="flex-1 text-[12px] font-mono text-success truncate">
                        $ {selected.local.command}
                      </code>
                      <CopyButton text={selected.local.command} />
                    </div>
                  </div>

                  {/* Example config */}
                  <div>
                    <label className="text-[11px] font-medium text-text-muted mb-1.5 block">
                      Example Configuration
                    </label>
                    <div className="relative">
                      <pre className="bg-bg-base border border-border-strong/40 rounded-md px-4 py-3 font-mono text-[11px] text-text-base leading-relaxed overflow-x-auto whitespace-pre">
{(() => {
  const parts = selected.local!.command.split(/\s+/);
  const cmd = parts[0] || "";
  const args = parts.slice(1);
  const envObj: Record<string, string> = {};
  selected.auth.env_vars.forEach((v) => {
    envObj[v.name] = v.secret ? "<YOUR_VALUE>" : "";
  });
  const configObj: Record<string, unknown> = {
    type: "stdio",
    command: cmd,
  };
  if (args.length > 0) configObj.args = args;
  if (Object.keys(envObj).length > 0) configObj.env = envObj;
  return JSON.stringify(
    { [selected.title.toLowerCase().replace(/[^a-z0-9]+/g, "-")]: configObj },
    null,
    2
  );
})()}
                      </pre>
                      <div className="absolute top-2 right-2">
                        <CopyButton
                          text={(() => {
                            const parts = selected.local!.command.split(/\s+/);
                            const cmd = parts[0] || "";
                            const args = parts.slice(1);
                            const envObj: Record<string, string> = {};
                            selected.auth.env_vars.forEach((v) => {
                              envObj[v.name] = v.secret ? "<YOUR_VALUE>" : "";
                            });
                            const configObj: Record<string, unknown> = {
                              type: "stdio",
                              command: cmd,
                            };
                            if (args.length > 0) configObj.args = args;
                            if (Object.keys(envObj).length > 0) configObj.env = envObj;
                            return JSON.stringify(
                              { [selected.title.toLowerCase().replace(/[^a-z0-9]+/g, "-")]: configObj },
                              null,
                              2
                            );
                          })()}
                        />
                      </div>
                    </div>
                  </div>
                </div>
              )}

              {/* Auth tab */}
              {setupTab === "auth" && hasAuth(selected) && (
                <div className="space-y-5">
                  <div>
                    <h3 className="text-[11px] font-semibold text-text-muted uppercase tracking-wider mb-3">
                      Authentication
                    </h3>
                    <p className="text-[12px] text-text-muted mb-4">
                      This server requires authentication. Set the following environment variables.
                    </p>
                  </div>

                  <div>
                    <label className="text-[11px] font-medium text-text-muted mb-1.5 block">
                      Method
                    </label>
                    <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded bg-danger/10 text-danger text-[11px] font-medium">
                      <Shield size={11} />
                      {selected.auth.method}
                    </span>
                  </div>

                  <div>
                    <label className="text-[11px] font-medium text-text-muted mb-2 block">
                      Environment Variables
                    </label>
                    <div className="space-y-2">
                      {selected.auth.env_vars.map((v) => (
                        <div
                          key={v.name}
                          className="flex items-start gap-3 bg-bg-base border border-border-strong/40 rounded-md px-3 py-2.5"
                        >
                          <div className="flex-1 min-w-0">
                            <div className="flex items-center gap-2 mb-0.5">
                              <code className="text-[12px] font-mono text-text-base font-medium">
                                {v.name}
                              </code>
                              {v.secret && (
                                <span className="flex items-center gap-0.5 px-1.5 py-0.5 rounded bg-danger/10 text-danger text-[9px] font-medium">
                                  <Key size={8} />
                                  secret
                                </span>
                              )}
                            </div>
                            <p className="text-[11px] text-text-muted leading-relaxed">
                              {v.description}
                            </p>
                          </div>
                          <CopyButton text={v.name} />
                        </div>
                      ))}
                    </div>
                  </div>
                </div>
              )}

              {/* Fallback for unavailable tabs */}
              {setupTab === "remote" && !selected.remote && (
                <div className="text-center py-8">
                  <Cloud size={24} className="mx-auto mb-3 text-surface" />
                  <p className="text-[13px] text-text-muted">
                    No remote endpoint available for this server.
                  </p>
                  <p className="text-[11px] text-text-muted mt-1">
                    Use local installation instead.
                  </p>
                </div>
              )}
              {setupTab === "local" && !selected.local && (
                <div className="text-center py-8">
                  <Monitor size={24} className="mx-auto mb-3 text-surface" />
                  <p className="text-[13px] text-text-muted">
                    No local installation available for this server.
                  </p>
                  <p className="text-[11px] text-text-muted mt-1">
                    Use the remote endpoint instead.
                  </p>
                </div>
              )}
              {setupTab === "auth" && !hasAuth(selected) && (
                <div className="text-center py-8">
                  <Lock size={24} className="mx-auto mb-3 text-surface" />
                  <p className="text-[13px] text-text-muted">
                    No authentication required for this server.
                  </p>
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    );
  }

  // ── Landing / directory view ─────────────────────────────────────────────

  return (
    <div className="flex h-full flex-col overflow-y-auto custom-scrollbar bg-bg-base">
      <div className="flex flex-col px-6 pt-10 pb-10 w-full">
        {/* Header — centered, constrained width */}
        <div className="text-center mb-6 max-w-lg mx-auto">
          <div
            className={`w-12 h-12 mx-auto mb-4 rounded-2xl ${ACCENT_BG} ${ACCENT_BORDER} border flex items-center justify-center`}
          >
            <Server size={20} style={{ color: ACCENT }} strokeWidth={1.5} />
          </div>
          <h2 className="text-[18px] font-semibold text-text-base mb-1.5">
            MCP Server Directory
          </h2>
          <p className="text-[13px] text-text-muted leading-relaxed">
            Browse popular MCP servers. View setup instructions, install
            commands, and authentication requirements.
          </p>
        </div>

        {/* Search + filters — constrained */}
        <div className="max-w-2xl mx-auto w-full mb-6">
          <div className="relative mb-3">
            <Search
              size={16}
              className="absolute left-4 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none"
            />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search servers..."
              autoFocus
              className="w-full bg-bg-input border border-border-strong/40 hover:border-border-strong focus:border-icon-mcp rounded-xl pl-11 pr-4 py-3 text-[14px] text-text-base placeholder-text-muted/60 outline-none transition-colors shadow-sm"
            />
          </div>

          {/* Classification filters */}
          <div className="flex gap-2 flex-wrap justify-center mb-2">
            {CLASSIFICATIONS.map((c) => (
              <button
                key={c}
                onClick={() => setClassification(c)}
                className={`flex items-center gap-1.5 px-3 py-1.5 rounded-full text-[12px] font-medium border transition-colors ${
                  classification === c
                    ? "bg-icon-mcp/15 text-icon-mcp border-icon-mcp/30"
                    : "bg-bg-sidebar border-border-strong/40 text-text-muted hover:text-text-base hover:border-border-strong"
                }`}
              >
                {CLASSIFICATION_LABELS[c]}
                <span className={`text-[10px] px-1.5 py-0.5 rounded-full font-mono ${
                  classification === c
                    ? "bg-icon-mcp/20 text-icon-mcp"
                    : "bg-border-strong/20 text-text-muted"
                }`}>
                  {tagCounts[c] ?? 0}
                </span>
              </button>
            ))}
          </div>

          {/* Transport / capability filters */}
          <div className="flex gap-2 flex-wrap justify-center">
            {TRANSPORT_FILTERS.map((f) => (
              <button
                key={f}
                onClick={() => setTransportFilter(transportFilter === f ? null : f)}
                className={`flex items-center gap-1.5 px-3 py-1.5 rounded-full text-[12px] font-medium border transition-colors ${
                  transportFilter === f
                    ? f === "remote"
                      ? "bg-icon-mcp/15 text-icon-mcp border-icon-mcp/30"
                      : f === "local"
                      ? "bg-success/15 text-success border-success/30"
                      : "bg-danger/15 text-danger border-danger/30"
                    : "bg-bg-sidebar border-border-strong/40 text-text-muted hover:text-text-base hover:border-border-strong"
                }`}
              >
                {f === "remote" && <Cloud size={11} />}
                {f === "local"  && <Monitor size={11} />}
                {f === "auth"   && <Lock size={11} />}
                {TRANSPORT_LABELS[f]}
                <span className={`text-[10px] px-1.5 py-0.5 rounded-full font-mono ${
                  transportFilter === f
                    ? f === "remote"
                      ? "bg-icon-mcp/20 text-icon-mcp"
                      : f === "local"
                      ? "bg-success/20 text-success"
                      : "bg-danger/20 text-danger"
                    : "bg-border-strong/20 text-text-muted"
                }`}>
                  {transportCounts[f]}
                </span>
              </button>
            ))}
          </div>
        </div>

        {/* Results count — full width */}
        <div className="flex items-center justify-between mb-3">
          <h3 className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
            {filtered.length !== servers.length
              ? <>{filtered.length} <span className="text-text-muted/50 font-normal normal-case">of</span> {servers.length} servers</>
              : <>{servers.length} server{servers.length !== 1 ? "s" : ""}</>
            }
          </h3>
          {installedServers.size > 0 && (
            <span className="text-[11px] text-success">{installedServers.size} added</span>
          )}
        </div>

        {/* Server grid — full width */}
        {serversLoading ? (
          <div className="flex items-center justify-center py-16">
            <Loader2 size={24} className="animate-spin text-text-muted" />
          </div>
        ) : filtered.length === 0 ? (
          <div className="text-center py-12">
            <Server
              size={32}
              className="mx-auto mb-4 text-surface"
            />
            <p className="text-[13px] text-text-muted">
              No servers match your search.
            </p>
          </div>
        ) : (
          <div className="grid grid-cols-3 2xl:grid-cols-4 gap-4">
            {filtered.map((server) => {
              const isInstalled = installedServers.has(configName(server));
              return (
                <button
                  key={server.slug}
                  onClick={() => handleSelect(server)}
                  className="group w-full h-full text-left p-5 rounded-xl bg-bg-input border border-border-strong/40 hover:border-border-strong hover:bg-surface-hover transition-all flex flex-col"
                >
                  {/* Header: icon + title + category + installed badge + arrow */}
                  <div className="flex items-start justify-between gap-3 mb-4 min-h-[52px]">
                    <div className="flex items-center gap-2.5 min-w-0">
                      <McpServerIcon server={server} size={36} />
                      <div className="min-w-0">
                        <div className="text-[14px] font-semibold text-text-base leading-snug truncate">
                          {server.title}
                        </div>
                        {classificationBadge(server.classification)}
                      </div>
                    </div>
                    <div className="flex items-center gap-1.5 flex-shrink-0 mt-0.5">
                      {isInstalled && (
                        <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-success/10 border border-success/20 text-[10px] font-medium text-success">
                          <CheckCircle2 size={10} />
                          Added
                        </span>
                      )}
                      <ArrowRight size={13} className="text-surface group-hover:text-text-muted transition-colors" />
                    </div>
                  </div>

                  {/* Description */}
                  <div className="flex-1 min-h-0">
                    <p className="text-[12px] text-text-muted leading-relaxed line-clamp-3">
                      {server.description}
                    </p>
                  </div>

                  {/* Transport pills */}
                  <div className="flex flex-wrap gap-1.5 mt-3">
                    {hasRemote(server) && (
                      <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-bg-sidebar border border-border-strong/40 text-[10px] text-text-muted">
                        <Cloud size={10} />
                        Remote
                      </span>
                    )}
                    {hasLocal(server) && (
                      <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-bg-sidebar border border-border-strong/40 text-[10px] text-text-muted">
                        <Monitor size={10} />
                        Local
                      </span>
                    )}
                    {hasAuth(server) && (
                      <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-bg-sidebar border border-border-strong/40 text-[10px] text-text-muted">
                        <Lock size={10} />
                        Auth
                      </span>
                    )}
                  </div>

                  {/* Footer — provider */}
                  <div className="flex items-center justify-between gap-2 mt-2.5 pt-2.5 border-t border-border-strong/40">
                    <span className="text-[10px] text-text-muted truncate">
                      {server.provider}
                    </span>
                  </div>
                </button>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
