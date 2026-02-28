import { useState, useEffect, useMemo, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Search,
  Server,
  ArrowLeft,
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
} from "lucide-react";
import serversData from "./featured-mcp-servers.json";

// ── Types ──────────────────────────────────────────────────────────────────

interface EnvVar {
  name: string;
  description: string;
  secret: boolean;
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
  remote: { transport: string; url: string } | null;
  local: {
    registry: string;
    package: string;
    version: string | null;
    transport: string;
    command: string;
  } | null;
  auth: { method: string; env_vars: EnvVar[] };
}

const servers: McpServer[] = serversData as McpServer[];

// ── Constants ──────────────────────────────────────────────────────────────

const ACCENT = "var(--warning)"; // amber — ICONS.mcp.hex
const ACCENT_BG = "bg-warning/10";
const ACCENT_BORDER = "border-warning/20";

const CLASSIFICATIONS = ["all", "official", "reference", "community"] as const;

const CLASSIFICATION_LABELS: Record<string, string> = {
  all: "All",
  official: "Official",
  reference: "Reference",
  community: "Community",
};

const CLASSIFICATION_COLORS: Record<string, string> = {
  official: "bg-brand/15 text-brand-light border-brand/20",
  reference: "bg-accent/15 text-accent-light border-accent/20",
  community: "bg-accent-hover/15 text-accent-hover-light border-accent-hover/20",
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

/** Build a save-ready config JSON from marketplace data. Prefers local, falls back to remote. */
function buildConfig(server: McpServer): Record<string, unknown> {
  if (server.local) {
    const parts = server.local.command.split(/\s+/);
    const cmd = parts[0] || "";
    const args = parts.slice(1);
    const env: Record<string, string> = {};
    server.auth.env_vars.forEach((v) => {
      env[v.name] = "";
    });
    const cfg: Record<string, unknown> = { type: "stdio", command: cmd };
    if (args.length > 0) cfg.args = args;
    if (Object.keys(env).length > 0) cfg.env = env;
    return cfg;
  }
  if (server.remote) {
    const type = server.remote.transport === "sse" ? "sse" : "http";
    return { type, url: server.remote.url };
  }
  return { type: "stdio", command: "" };
}

export default function McpMarketplace({
  resetKey,
}: {
  resetKey?: number;
}) {
  const [query, setQuery] = useState("");
  const [classification, setClassification] = useState<string>("all");
  const [selected, setSelected] = useState<McpServer | null>(null);
  const [setupTab, setSetupTab] = useState<"remote" | "local" | "auth">(
    "local"
  );
  const [installedServers, setInstalledServers] = useState<Set<string>>(new Set());
  const [installing, setInstalling] = useState(false);
  const [installError, setInstallError] = useState<string | null>(null);

  // Load installed MCP servers
  const loadInstalled = useCallback(async () => {
    try {
      const result: string[] = await invoke("list_mcp_server_configs");
      setInstalledServers(new Set(result));
    } catch {
      // non-fatal
    }
  }, []);

  useEffect(() => { loadInstalled(); }, [loadInstalled]);

  // Reset when the nav item is re-clicked
  useEffect(() => {
    if (resetKey !== undefined) {
      setSelected(null);
      setQuery("");
      setClassification("all");
      setInstallError(null);
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

  // Filter servers
  const filtered = useMemo(() => {
    let list = servers;
    if (classification !== "all") {
      list = list.filter((s) => s.classification === classification);
    }
    if (query.trim()) {
      const q = query.toLowerCase();
      list = list.filter(
        (s) =>
          s.title.toLowerCase().includes(q) ||
          s.description.toLowerCase().includes(q) ||
          s.provider.toLowerCase().includes(q) ||
          s.slug.toLowerCase().includes(q)
      );
    }
    return list;
  }, [query, classification]);

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
                      className="flex items-center gap-2 px-4 py-2.5 rounded-lg bg-warning hover:bg-warning-hover text-bg-input font-medium text-[13px] transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
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
                  className="flex items-center gap-1.5 text-[12px] hover:text-text-base transition-colors"
                  style={{ color: ACCENT }}
                >
                  <Github size={12} />
                  Repository
                  <ExternalLink size={10} />
                </a>
              )}
              <div className="flex items-center gap-1.5 text-[12px] text-text-muted">
                {hasRemote(selected) && (
                  <span className="flex items-center gap-1 px-2 py-0.5 rounded bg-warning/10 text-warning text-[10px] font-medium">
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
                        ? "border-warning text-text-base"
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
                    <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded bg-warning/10 text-warning text-[11px] font-mono font-medium">
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
        <div className="max-w-xl mx-auto w-full mb-6">
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
              className="w-full bg-bg-input border border-border-strong/40 hover:border-border-strong focus:border-warning rounded-xl pl-11 pr-4 py-3 text-[14px] text-text-base placeholder-text-muted/60 outline-none transition-colors shadow-sm"
            />
          </div>
          <div className="flex gap-2 justify-center">
            {CLASSIFICATIONS.map((c) => (
              <button
                key={c}
                onClick={() => setClassification(c)}
                className={`px-3 py-1.5 rounded-full text-[12px] font-medium border transition-colors ${
                  classification === c
                    ? "bg-warning/15 text-warning border-warning/30"
                    : "bg-bg-sidebar border-border-strong/40 text-text-muted hover:text-text-base hover:border-border-strong"
                }`}
              >
                {CLASSIFICATION_LABELS[c]}
              </button>
            ))}
          </div>
        </div>

        {/* Results count — full width */}
        <div className="flex items-center justify-between mb-3">
          <h3 className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
            {filtered.length} Server{filtered.length !== 1 ? "s" : ""}
          </h3>
          <div className="flex items-center gap-3 text-[10px] text-text-muted">
            <span className="flex items-center gap-1">
              <Cloud size={9} />
              Remote
            </span>
            <span className="flex items-center gap-1">
              <Monitor size={9} />
              Local
            </span>
            <span className="flex items-center gap-1">
              <Lock size={9} />
              Auth
            </span>
          </div>
        </div>

        {/* Server grid — full width */}
        {filtered.length === 0 ? (
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
          <div className="grid grid-cols-3 gap-4 items-stretch">
            {filtered.map((server) => (
              <button
                key={server.slug}
                onClick={() => handleSelect(server)}
                className="group flex flex-col text-left p-5 rounded-xl bg-bg-input border border-border-strong/40 hover:border-border-strong hover:bg-surface-hover transition-colors"
              >
                {/* Header: icon + title + badge */}
                <div className="flex gap-3 mb-3">
                  <McpServerIcon server={server} size={36} />
                  <div className="min-w-0">
                    <div className="flex items-center gap-1.5 mb-1">
                      <span className="text-[14px] font-medium text-text-base leading-snug truncate">
                        {server.title}
                      </span>
                      {installedServers.has(configName(server)) && (
                        <CheckCircle2 size={13} className="text-success flex-shrink-0" />
                      )}
                    </div>
                    {classificationBadge(server.classification)}
                  </div>
                </div>
                {/* Description */}
                <p className="text-[12px] text-text-muted leading-relaxed line-clamp-3 flex-1">
                  {server.description}
                </p>
                {/* Footer — pinned to bottom */}
                <div className="flex items-center justify-between gap-4 mt-3 pt-2.5 border-t border-border-strong/40/40">
                  <span className="text-[11px] text-text-muted truncate">
                    {server.provider}
                  </span>
                  <div className="flex items-center gap-2.5 flex-shrink-0">
                    {hasRemote(server) && (
                      <Cloud size={11} className="text-warning/60" />
                    )}
                    {hasLocal(server) && (
                      <Monitor size={11} className="text-success/60" />
                    )}
                    {hasAuth(server) && (
                      <Lock size={11} className="text-danger/60" />
                    )}
                  </div>
                </div>
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
