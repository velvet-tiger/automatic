import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useRecentlyAdded } from "../../lib/useRecentlyAdded";
import { ask } from "@tauri-apps/plugin-dialog";
import { trackMcpServerCreated, trackMcpServerUpdated, trackMcpServerDeleted } from "../../lib/analytics";
import { AuthorSection, type AuthorDescriptor } from "../../components/AuthorPanel";
import { KvEditor, inputClass, smallInputClass, addBtnClass } from "../../components/KvField";
import McpServerImportDialog from "../../components/McpServerImportDialog";
import { handleExternalLinkClick } from "../../lib/externalLinks";
import featuredMcpServers from "../../../src-tauri/assets/discover/featured-mcp-servers.json";
import { AssetTable } from "../../components/AssetTable";
import { AssetDrawer } from "../../components/AssetDrawer";
import { BuiltInBadge, LockCell } from "../../components/ProtectionBadge";
import { useBulkSelection } from "../../lib/useBulkSelection";
import {
  Plus,
  ClipboardPaste,
  X,
  Server,
  Check,
  Trash2,
  Terminal,
  Variable,
  Globe,
  AlertTriangle,
  Shield,
  ShieldCheck,
  Loader2,
  Info,
  ExternalLink,
  Search,
} from "lucide-react";
import { ICONS } from "../../lib/icons";

/** Discover-catalogue env-var metadata used to surface a "Generate token" link
 *  next to each configured environment variable that has one. */
interface DiscoverEnvVarMeta {
  name: string;
  token_url?: string | null;
  token_url_label?: string | null;
}
interface DiscoverEntryMeta {
  slug: string;
  title: string;
  auth?: { method?: string; env_vars?: DiscoverEnvVarMeta[] } | null;
}
const DISCOVER_ENTRIES = featuredMcpServers as DiscoverEntryMeta[];

/** Find the Discover catalogue entry that matches a saved server name.
 *  Matches by slug first (the installer's default), then by title-derived key. */
function findDiscoverEntry(serverName: string): DiscoverEntryMeta | null {
  const needle = serverName.toLowerCase();
  return (
    DISCOVER_ENTRIES.find(
      (e) =>
        e.slug === needle ||
        e.title.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") === needle,
    ) ?? null
  );
}

/** Resolve token-acquisition links for the env vars present in a server's config. */
function tokenLinksForServer(
  serverName: string,
  env: Record<string, string>,
): Array<{ name: string; url: string; label: string }> {
  const entry = findDiscoverEntry(serverName);
  const vars = entry?.auth?.env_vars;
  if (!vars) return [];
  const out: Array<{ name: string; url: string; label: string }> = [];
  for (const key of Object.keys(env)) {
    const meta = vars.find((v) => v.name === key);
    if (meta?.token_url) {
      out.push({ name: key, url: meta.token_url, label: meta.token_url_label || "Get token" });
    }
  }
  return out;
}

type TransportType = "stdio" | "http" | "sse";

/** Metadata stored alongside config in Automatic's own JSON. Not written to agent files. */
interface McpAuthorMeta {
  /** Display name of the provider / author, e.g. "Anthropic", "Microsoft" */
  name: string;
  /** Optional homepage URL for the provider */
  url?: string;
  /** Optional GitHub repository URL */
  repository_url?: string;
}

interface McpServerConfig {
  type: TransportType;
  // stdio fields
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  cwd?: string;
  // http/sse fields
  url?: string;
  headers?: Record<string, string>;
  oauth?: {
    clientId?: string;
    clientSecret?: string;
    scope?: string;
    callbackPort?: number;
  };
  // common
  enabled?: boolean;
  timeout?: number;
  /** Automatic-internal: author/provider metadata. Stripped before writing to agent configs. */
  _author?: McpAuthorMeta;
  /** Automatic-internal: true for built-in servers that cannot be deleted or reconfigured. */
  _builtin?: boolean;
}

function parseGitHubRepo(url?: string): string | null {
  if (!url) return null;
  const match = url.match(/^https?:\/\/github\.com\/([^/]+\/[^/]+?)(?:\.git|\/)?$/i);
  return match ? match[1] : null;
}

interface McpOAuthTokenStatus {
  has_token: boolean;
  valid: boolean;
  revoked: boolean;
  message: string | null;
}

function emptyConfig(): McpServerConfig {
  return {
    type: "stdio",
    command: "",
    args: [],
    env: {},
    enabled: true,
  };
}

/** Normalize a loaded config so all optional fields have sensible defaults for the UI. */
function normalizeConfig(data: Partial<McpServerConfig> & { oauth?: any }): McpServerConfig {
  let type: TransportType = data.type || "stdio";
  if (!data.type && data.url && !data.command) {
    type = "http";
  }

  let oauth;
  if (data.oauth && typeof data.oauth === 'object') {
    oauth = {
      clientId: data.oauth.clientId || "",
      clientSecret: data.oauth.clientSecret || "",
      scope: data.oauth.scope || "",
      callbackPort: data.oauth.callbackPort || undefined,
    };
  }

  return {
    type,
    command: data.command || "",
    args: data.args || [],
    env: data.env || {},
    cwd: data.cwd || "",
    url: data.url || "",
    headers: data.headers || {},
    oauth,
    enabled: data.enabled !== false,
    timeout: data.timeout,
    // Preserve author metadata if present (Automatic-internal field)
    _author: data._author,
    _builtin: data._builtin,
  };
}

/** Strip empty optional fields before saving. Keeps _author metadata for Automatic's own store. */
function cleanConfig(config: McpServerConfig): Record<string, unknown> {
  const out: Record<string, unknown> = { type: config.type };

  if (config.type === "stdio") {
    if (config.command) out.command = config.command;
    if (config.args && config.args.length > 0) out.args = config.args;
    if (config.env && Object.keys(config.env).length > 0) out.env = config.env;
    if (config.cwd) out.cwd = config.cwd;
  } else {
    if (config.url) out.url = config.url;
    if (config.headers && Object.keys(config.headers).length > 0) out.headers = config.headers;

    if (config.oauth) {
      const cleanOauth: Record<string, unknown> = {};
      if (config.oauth.clientId) cleanOauth.clientId = config.oauth.clientId;
      if (config.oauth.clientSecret) cleanOauth.clientSecret = config.oauth.clientSecret;
      if (config.oauth.scope) cleanOauth.scope = config.oauth.scope;
      if (config.oauth.callbackPort) cleanOauth.callbackPort = config.oauth.callbackPort;

      if (Object.keys(cleanOauth).length > 0) {
        out.oauth = cleanOauth;
      }
    }
  }

  if (config.enabled === false) out.enabled = false;
  if (config.timeout && config.timeout > 0) out.timeout = config.timeout;

  // Preserve author metadata in the Automatic store. The sync layer is responsible
  // for stripping _author before writing to agent config files.
  if (config._author) out._author = config._author;

  return out;
}

// ── Reusable field components ──────────────────────────────────────────────
// KvList, inputClass, smallInputClass, addBtnClass are imported from KvField.tsx

// Only the hardcoded "automatic" server is protected against deletion in the
// list view — the richer _builtin/_author checks below require reading each
// server's config, which the bare name list here does not carry.
const isDeletable = (name: string) => name !== "automatic";

// ── OAuth Authentication Section ────────────────────────────────────────────

function OAuthSection({ serverName, url }: { serverName: string; url: string }) {
  const [hasToken, setHasToken] = useState(false);
  const [tokenStatus, setTokenStatus] = useState<McpOAuthTokenStatus | null>(null);
  const [action, setAction] = useState<"authorize" | "revoke" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [checking, setChecking] = useState(true);
  const [checkingStatus, setCheckingStatus] = useState(false);
  const loading = action !== null;
  // Track whether the user cancelled so we can ignore the late-arriving invoke result.
  const cancelledRef = useRef(false);

  // Check for existing token on mount and when serverName changes.
  useEffect(() => {
    if (!serverName) return;
    setChecking(true);
    setTokenStatus(null);
    invoke<boolean>("has_mcp_oauth_token", { serverName })
      .then(setHasToken)
      .catch(() => setHasToken(false))
      .finally(() => setChecking(false));
  }, [serverName]);

  useEffect(() => {
    if (!serverName || !url || !hasToken) {
      setCheckingStatus(false);
      setTokenStatus(null);
      return;
    }

    let active = true;
    setCheckingStatus(true);
    invoke<McpOAuthTokenStatus>("get_mcp_oauth_token_status", { serverName, mcpUrl: url })
      .then((status) => {
        if (!active) return;
        setTokenStatus(status);
      })
      .catch((statusError) => {
        if (!active) return;
        setTokenStatus({
          has_token: true,
          valid: false,
          revoked: false,
          message: String(statusError),
        });
      })
      .finally(() => {
        if (active) setCheckingStatus(false);
      });

    return () => {
      active = false;
    };
  }, [serverName, url, hasToken]);

  const handleAuthorize = async () => {
    if (!serverName || !url) return;
    cancelledRef.current = false;
    setAction("authorize");
    setError(null);
    try {
      await invoke("authorize_mcp_server", { serverName, mcpUrl: url });
      if (!cancelledRef.current) setHasToken(true);
      if (!cancelledRef.current) setTokenStatus(null);
    } catch (e) {
      if (!cancelledRef.current) setError(String(e));
    } finally {
      if (!cancelledRef.current) setAction(null);
    }
  };

  const handleCancel = () => {
    cancelledRef.current = true;
    setAction(null);
    setError(null);
  };

  const handleRevoke = async () => {
    if (!serverName) return;
    setAction("revoke");
    setError(null);
    try {
      await invoke("revoke_mcp_oauth_token", { serverName });
      setHasToken(false);
      setTokenStatus(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setAction(null);
    }
  };

  if (checking) return null;
  // No URL and no stored token → nothing actionable. Wait for the user to
  // enter a URL before claiming auth is required.
  if (!hasToken && !url) return null;

  return (
    <section>
      <label className="block text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-2">
        OAuth Authentication
      </label>

      {hasToken ? (
        (() => {
          const tokenInvalid = !checkingStatus && !!tokenStatus && !tokenStatus.valid;
          const invalidMessage = tokenStatus?.message ?? "Stored token is no longer accepted by the MCP server. Re-authenticate to restore access.";
          const authorizing = action === "authorize";
          return (
            <div className="space-y-3">
              <div
                className={`flex items-start gap-3 rounded-lg border px-4 py-3 ${
                  tokenInvalid
                    ? "border-warning/25 bg-warning/8"
                    : "border-green-500/25 bg-green-500/8"
                }`}
              >
                {tokenInvalid ? (
                  <AlertTriangle size={14} className="text-warning shrink-0 mt-0.5" />
                ) : (
                  <ShieldCheck size={14} className="text-green-500 shrink-0 mt-0.5" />
                )}
                <div className="flex-1 min-w-0">
                  <p className="text-[12px] font-medium text-text-base">
                    {tokenInvalid ? "Authentication expired" : "Authenticated"}
                  </p>
                  <p className="text-[11px] text-text-muted">
                    {tokenInvalid
                      ? invalidMessage
                      : "OAuth token stored in system keychain. Agents will connect through the Automatic proxy."}
                  </p>
                </div>
                <div className="flex items-center gap-2 shrink-0">
                  {authorizing ? (
                    <>
                      <span className="flex items-center gap-1.5 px-3 py-1.5 text-[11px] font-medium text-text-muted whitespace-nowrap">
                        <Loader2 size={12} className="animate-spin" /> Waiting...
                      </span>
                      <button
                        onClick={handleCancel}
                        className="px-3 py-1.5 text-[11px] font-medium text-text-muted hover:text-danger border border-border-strong/40 hover:border-danger/30 rounded transition-colors whitespace-nowrap"
                      >
                        Cancel
                      </button>
                    </>
                  ) : (
                    <>
                      {tokenInvalid && (
                        <button
                          onClick={handleAuthorize}
                          disabled={!url || loading}
                          className="px-3 py-1.5 text-[11px] font-medium bg-brand hover:bg-brand-hover text-white rounded transition-colors disabled:opacity-50 whitespace-nowrap"
                        >
                          Re-authenticate
                        </button>
                      )}
                      <button
                        onClick={handleRevoke}
                        disabled={loading}
                        className="px-3 py-1.5 text-[11px] font-medium text-text-muted hover:text-danger border border-border-strong/40 hover:border-danger/30 rounded transition-colors disabled:opacity-50"
                      >
                        {action === "revoke" ? <Loader2 size={12} className="animate-spin" /> : "Revoke"}
                      </button>
                    </>
                  )}
                </div>
              </div>

              {checkingStatus && (
                <div className="flex items-center gap-2 rounded-lg border border-border-strong/30 bg-bg-tertiary/60 px-4 py-3 text-[11px] text-text-muted">
                  <Loader2 size={12} className="animate-spin shrink-0" />
                  Checking whether the stored token is still accepted by the MCP server...
                </div>
              )}

              {error && (
                <div className="flex items-start gap-2 rounded-lg border border-danger/25 bg-danger/8 px-4 py-3">
                  <AlertTriangle size={12} className="text-danger mt-0.5 shrink-0" />
                  <p className="text-[11px] text-danger">{error}</p>
                </div>
              )}
            </div>
          );
        })()
      ) : (
        <div className="space-y-3">
          <div className="flex items-center gap-3 rounded-lg border border-brand/25 bg-brand/8 px-4 py-3">
            <Shield size={14} className="text-brand shrink-0" />
            <div className="flex-1">
              <p className="text-[12px] font-medium text-text-base">Authentication required</p>
              <p className="text-[11px] text-text-muted">
                This server requires OAuth. Authenticate to store the token securely in your system keychain.
                Agents will connect through a local proxy — no secrets are written to project files.
              </p>
            </div>
            {loading ? (
              <div className="flex items-center gap-2">
                <span className="flex items-center gap-1.5 px-4 py-1.5 text-[12px] font-medium text-text-muted whitespace-nowrap">
                  <Loader2 size={12} className="animate-spin" /> Waiting...
                </span>
                <button
                  onClick={handleCancel}
                  className="px-3 py-1.5 text-[11px] font-medium text-text-muted hover:text-danger border border-border-strong/40 hover:border-danger/30 rounded transition-colors whitespace-nowrap"
                >
                  Cancel
                </button>
              </div>
            ) : (
              <button
                onClick={handleAuthorize}
                disabled={!url}
                className="px-4 py-1.5 text-[12px] font-medium bg-brand hover:bg-brand-hover text-white rounded shadow-sm transition-colors disabled:opacity-50 whitespace-nowrap"
              >
                Authenticate
              </button>
            )}
          </div>

          {error && (
            <div className="flex items-start gap-2 rounded-lg border border-danger/25 bg-danger/8 px-4 py-3">
              <AlertTriangle size={12} className="text-danger mt-0.5 shrink-0" />
              <p className="text-[11px] text-danger">{error}</p>
            </div>
          )}
        </div>
      )}

    </section>
  );
}

// ── Main component ─────────────────────────────────────────────────────────

interface Project {
  name: string;
  agents: string[];
  mcp_servers: string[];
}

interface McpServersProps {
  /** When set, auto-selects this server on mount (used when navigating from Projects). */
  initialServer?: string | null;
  /** Called after the initialServer has been consumed so the parent can clear it. */
  onInitialServerConsumed?: () => void;
}

export default function McpServers({ initialServer = null, onInitialServerConsumed }: McpServersProps = {}) {
  const [servers, setServers] = useState<string[]>([]);
  const [recentRefresh, setRecentRefresh] = useState(0);
  const recentIds = useRecentlyAdded("mcp_servers", recentRefresh);
  const [selectedName, setSelectedName] = useState<string | null>(null);
  const [config, setConfig] = useState<McpServerConfig | null>(null);
  const [dirty, setDirty] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [opencodeWarning, setOpencodeWarning] = useState<string[]>([]);
  const [importOpen, setImportOpen] = useState(false);
  const [search, setSearch] = useState("");
  const [bulkDeleting, setBulkDeleting] = useState(false);
  const [bulkProgress, setBulkProgress] = useState<{ done: number; total: number } | null>(null);

  // Inline add state
  const [newArg, setNewArg] = useState("");


  useEffect(() => {
    loadServers();
    checkOpencodeProjects();
  }, []);

  // When navigated here from another page (e.g. Projects), auto-select the
  // requested server once the list has loaded.
  useEffect(() => {
    if (initialServer && servers.includes(initialServer)) {
      selectServer(initialServer);
      onInitialServerConsumed?.();
    }
  }, [initialServer, servers]);

  const loadServers = async () => {
    try {
      const result: string[] = await invoke("list_mcp_server_configs");
      setServers(result.sort());
      setError(null);
    } catch (err: any) {
      setError(`Failed to load servers: ${err}`);
    }
  };

  const checkOpencodeProjects = async () => {
    try {
      const projectNames: string[] = await invoke("list_projects");
      const affectedProjects: string[] = [];

      for (const name of projectNames) {
        const raw: string = await invoke("read_project", { name });
        const project: Project = JSON.parse(raw);

        // Check if project uses OpenCode and has MCP servers configured
        if (project.agents.includes("opencode") && project.mcp_servers.length > 0) {
          affectedProjects.push(project.name);
        }
      }

      setOpencodeWarning(affectedProjects);
    } catch (err: any) {
      // Silently fail - warning is non-critical
      console.error("Failed to check OpenCode projects:", err);
    }
  };

  const selectServer = async (name: string) => {
    try {
      const raw: string = await invoke("read_mcp_server_config", { name });
      const data = JSON.parse(raw);
      setSelectedName(name);
      setConfig(normalizeConfig(data));
      setDirty(false);
      setIsCreating(false);
      setError(null);
      resetInlineState();
    } catch (err: any) {
      setError(`Failed to read server: ${err}`);
    }
  };

  const resetInlineState = () => {
    setNewArg("");
  };

  const closeDrawer = () => {
    setSelectedName(null);
    setConfig(null);
    setDirty(false);
    setIsCreating(false);
    setNewName("");
    resetInlineState();
  };

  const updateConfig = (patch: Partial<McpServerConfig>) => {
    if (!config) return;
    setConfig({ ...config, ...patch });
    setDirty(true);
  };

  const handleSave = async () => {
    if (!config) return;
    const name = isCreating ? newName.trim() : selectedName;
    if (!name) return;
    try {
      await invoke("save_mcp_server_config", {
        name,
        data: JSON.stringify(cleanConfig(config)),
      });
      if (isCreating) {
        trackMcpServerCreated(name);
      } else {
        trackMcpServerUpdated(name);
      }
      setDirty(false);
      setSelectedName(name);
      if (isCreating) {
        setIsCreating(false);
        await loadServers();
        setRecentRefresh(prev => prev + 1);
      }
      setError(null);
    } catch (err: any) {
      setError(`Failed to save server: ${err}`);
    }
  };

  const handleDelete = async (name: string, e: React.MouseEvent) => {
    e.stopPropagation();
    const confirmed = await ask(`Delete MCP server "${name}"?`, { title: "Delete Server", kind: "warning" });
    if (!confirmed) return;
    try {
      await invoke("delete_mcp_server_config", { name });
      trackMcpServerDeleted(name);
      if (selectedName === name) closeDrawer();
      await loadServers();
      setError(null);
    } catch (err: any) {
      setError(`Failed to delete server: ${err}`);
    }
  };

  const handleBulkDelete = async () => {
    const targets = servers.filter(name => selection.selectedIds.has(name) && isDeletable(name));
    if (targets.length === 0) return;

    const preview = targets.slice(0, 10).map(t => `• ${t}`).join("\n");
    const overflow = targets.length > 10 ? `\n…and ${targets.length - 10} more.` : "";
    const message = `Delete ${targets.length} MCP server${targets.length === 1 ? "" : "s"}?\n\n${preview}${overflow}\n\nThis cannot be undone.`;
    const confirmed = await ask(message, { title: "Delete MCP Servers", kind: "warning" });
    if (!confirmed) return;

    setBulkDeleting(true);
    setBulkProgress({ done: 0, total: targets.length });
    const failed: { name: string; error: string }[] = [];
    for (let i = 0; i < targets.length; i++) {
      const name = targets[i]!;
      try {
        await invoke("delete_mcp_server_config", { name });
        trackMcpServerDeleted(name);
      } catch (err: any) {
        failed.push({ name, error: String(err) });
      }
      setBulkProgress({ done: i + 1, total: targets.length });
    }

    if (selectedName && targets.includes(selectedName)) {
      closeDrawer();
    }

    await loadServers();
    selection.clearSelection();
    setBulkDeleting(false);
    setBulkProgress(null);
    if (failed.length > 0) {
      const detail = failed.slice(0, 5).map(f => `${f.name}: ${f.error}`).join("\n");
      const more = failed.length > 5 ? `\n…and ${failed.length - 5} more.` : "";
      setError(`Failed to delete ${failed.length} server${failed.length === 1 ? "" : "s"}:\n${detail}${more}`);
    } else {
      setError(null);
    }
  };

  const startCreate = () => {
    setSelectedName(null);
    setConfig(emptyConfig());
    setDirty(true);
    setIsCreating(true);
    setNewName("");
    resetInlineState();
  };

  const addArg = () => {
    if (!config || !newArg) return;
    updateConfig({ args: [...(config.args || []), newArg] });
    setNewArg("");
  };

  const removeArg = (idx: number) => {
    if (!config) return;
    updateConfig({ args: (config.args || []).filter((_, i) => i !== idx) });
  };

  const setTransport = (type: TransportType) => {
    if (!config) return;
    setConfig({
      ...config,
      type,
      ...(type === "stdio"
        ? { command: config.command || "", args: config.args || [], cwd: config.cwd || "" }
        : { url: config.url || "", headers: config.headers || {} }),
    });
    setDirty(true);
  };

  const isStdio = config?.type === "stdio";
  const githubRepo = parseGitHubRepo(config?._author?.repository_url);
  /** Server was installed from the MCP Directory or is built-in — lock core settings. */
  const isManaged = (!!config?._author && !githubRepo) || !!config?._builtin;
  /** Built-in server (e.g. Automatic itself) — lock everything including delete. */
  const isBuiltin = !!config?._builtin;

  const searchLower = search.trim().toLowerCase();
  const filteredServers = servers.filter(name => !searchLower || name.toLowerCase().includes(searchLower));

  const selection = useBulkSelection(filteredServers, name => name, isDeletable);
  const drawerOpen = !!config;

  const renderTableRow = (name: string) => {
    const isRowSelected = selection.isSelected(name);
    const isFocused = selectedName === name && !isCreating;
    const deletable = isDeletable(name);
    return (
      <tr
        key={name}
        onClick={() => selectServer(name)}
        className={`group cursor-pointer border-b border-border-strong/20 last:border-b-0 transition-colors ${
          isFocused ? "bg-bg-sidebar/60" : "hover:bg-bg-input/70"
        }`}
      >
        <td className="px-3 py-2 w-9" onClick={(e) => e.stopPropagation()}>
          {deletable ? (
            <input
              type="checkbox"
              checked={isRowSelected}
              onChange={() => selection.toggleSelected(name)}
              aria-label={`Select ${name}`}
              className="cursor-pointer accent-brand"
            />
          ) : (
            <LockCell tooltip="Built-in server — cannot be deleted." />
          )}
        </td>
        <td className="px-3 py-2 w-11">
          <div className={ICONS.mcp.iconBox}>
            <Server size={15} className={ICONS.mcp.iconColor} />
          </div>
        </td>
        <td className="px-3 py-2 min-w-0">
          <div className="flex items-center gap-2 min-w-0">
            <span className="text-[13px] font-medium text-text-base truncate">{name}</span>
            {recentIds.has(name) && (
              <span className="shrink-0 px-1.5 py-0.5 rounded bg-brand/15 text-brand text-[9px] font-semibold uppercase tracking-wider">New</span>
            )}
          </div>
        </td>
        <td className="px-3 py-2 w-16 text-right" onClick={(e) => e.stopPropagation()}>
          {deletable ? (
            <button
              onClick={(e) => handleDelete(name, e)}
              className="opacity-0 group-hover:opacity-100 p-1 text-text-muted hover:text-danger rounded transition-all"
              title="Delete server"
            >
              <X size={13} />
            </button>
          ) : null}
        </td>
      </tr>
    );
  };

  return (
    <div className="flex h-full w-full flex-col bg-bg-base">

      {/* ── Top Toolbar ──────────────────────────────────────────────────── */}
      <div className="shrink-0 border-b border-border-strong/40 bg-bg-input/40">
        <div className="flex items-center justify-between px-4 pt-3 pb-2 gap-3">
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
            MCP Servers
          </span>

          <div className="flex items-center gap-2 shrink-0">
            <div className="relative">
              <Search size={12} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none" />
              <input
                type="text"
                placeholder="Search servers…"
                value={search}
                onChange={e => setSearch(e.target.value)}
                className="w-56 h-7 pl-7 pr-7 rounded-md bg-bg-input border border-border-strong/50 hover:border-border-strong focus:outline-none focus:ring-1 focus:ring-brand/60 focus:border-brand/60 text-[12px] text-text-base placeholder-text-muted/60 transition-colors"
              />
              {search && (
                <button
                  onClick={() => setSearch("")}
                  className="absolute right-2 top-1/2 -translate-y-1/2 text-text-muted hover:text-text-base transition-colors"
                >
                  <X size={11} />
                </button>
              )}
            </div>

            <button
              onClick={() => setImportOpen(true)}
              className="flex items-center gap-1.5 h-7 px-2.5 rounded-md border border-border-strong/50 bg-bg-input hover:bg-bg-sidebar text-[12px] text-text-base transition-colors"
              title="Import MCP Server from JSON"
            >
              <ClipboardPaste size={12} /> Import
            </button>

            <button
              onClick={startCreate}
              className="flex items-center gap-1.5 h-7 px-2.5 rounded-md bg-brand hover:bg-brand-hover text-white text-[12px] font-medium transition-colors"
              title="New Server"
            >
              <Plus size={12} /> New Server
            </button>
          </div>
        </div>

        {/* Selection action bar — appears whenever anything is selected */}
        {selection.totalSelected > 0 && (
          <div className="flex items-center justify-between px-4 py-2 border-t border-border-strong/30 bg-brand/5">
            <span className="text-[12px] text-text-base">
              {selection.totalSelected} server{selection.totalSelected === 1 ? "" : "s"} selected
              {bulkProgress && (
                <span className="ml-2 text-text-muted">
                  · Deleting {bulkProgress.done}/{bulkProgress.total}…
                </span>
              )}
            </span>
            <div className="flex items-center gap-2">
              <button
                onClick={selection.clearSelection}
                disabled={bulkDeleting}
                className="h-7 px-2.5 rounded-md text-[12px] text-text-muted hover:text-text-base hover:bg-bg-sidebar transition-colors disabled:opacity-50"
              >
                Clear selection
              </button>
              <button
                onClick={handleBulkDelete}
                disabled={bulkDeleting}
                className="flex items-center gap-1.5 h-7 px-2.5 rounded-md bg-danger/90 hover:bg-danger text-white text-[12px] font-medium transition-colors disabled:opacity-50 disabled:cursor-wait"
              >
                <X size={12} /> Delete selected
              </button>
            </div>
          </div>
        )}
      </div>

      {/* Error + OpenCode banners */}
      {error && (
        <div className="bg-red-500/10 text-red-400 p-3 text-[13px] border-b border-red-500/20 flex items-center justify-between shrink-0">
          {error}
          <button onClick={() => setError(null)}>
            <X size={14} />
          </button>
        </div>
      )}

      {opencodeWarning.length > 0 && (
        <div className="bg-amber-500/10 text-amber-400 p-3 text-[13px] border-b border-amber-500/20 flex items-start gap-3 shrink-0">
          <AlertTriangle size={16} className="flex-shrink-0 mt-0.5" />
          <div className="flex-1">
            <div className="font-medium mb-1">OpenCode Restart Required for New MCP Servers</div>
            <div className="text-[12px] text-amber-300/90 leading-relaxed">
              The following project{opencodeWarning.length > 1 ? 's are' : ' is'} using OpenCode with MCP servers configured: <span className="font-medium">{opencodeWarning.join(", ")}</span>.
              OpenCode requires a restart to pick up new MCP servers. After syncing, restart OpenCode for any newly added servers to become available.
            </div>
          </div>
          <button onClick={() => setOpencodeWarning([])} className="text-amber-300 hover:text-amber-200">
            <X size={14} />
          </button>
        </div>
      )}

      {/* ── Table ────────────────────────────────────────────────────────── */}
      <AssetTable
        items={filteredServers}
        getId={name => name}
        isEmpty={servers.length === 0}
        emptyState={
          <>
            <div className="w-14 h-14 mx-auto mb-5 rounded-2xl bg-icon-mcp/12 border border-icon-mcp/20 flex items-center justify-center">
              <Server size={22} className={ICONS.mcp.iconColor} strokeWidth={1.5} />
            </div>
            <h2 className="text-[15px] font-medium text-text-base mb-2">No MCP servers yet</h2>
            <p className="text-[13px] text-text-muted leading-relaxed max-w-xs mb-6">
              Configure Model Context Protocol servers that give your agents access to filesystems,
              databases, and developer tools. Add them here, then assign them to projects.
            </p>
            <button
              onClick={startCreate}
              className="flex items-center gap-2 px-4 py-2 bg-brand hover:bg-brand-hover text-white rounded-lg text-[13px] font-medium transition-colors"
            >
              <Plus size={14} /> New Server
            </button>
          </>
        }
        noMatchState={
          <p className="text-[13px] text-text-muted">
            {searchLower ? `No servers match "${search}".` : "No servers configured."}
          </p>
        }
        columns={[
          { key: "icon", header: "", className: "w-11" },
          { key: "name", header: "Name" },
          { key: "actions", header: "", className: "w-16" },
        ]}
        renderRow={renderTableRow}
        selection={{
          allSelected: selection.allSelected,
          someSelected: selection.someSelected,
          disabled: selection.deletableItems.length === 0,
          onToggleAll: selection.toggleSelectAllVisible,
          ariaLabel: "Select all visible deletable servers",
        }}
        recentIds={recentIds}
      />

      {/* ── Drawer ───────────────────────────────────────────────────────── */}
      <AssetDrawer open={drawerOpen} onClose={closeDrawer} isEditing={dirty}>
        {config && (
          <div className="flex-1 flex flex-col h-full min-h-0">
            {/* Header */}
            <div className="h-11 pl-6 pr-10 border-b border-surface flex justify-between items-center shrink-0">
              <div className="flex items-center gap-3">
                <Server size={14} className={ICONS.mcp.iconColor} />
                {isCreating ? (
                  <input
                    type="text"
                    placeholder="server-name (no spaces/slashes)"
                    value={newName}
                    onChange={(e) => setNewName(e.target.value)}
                    autoCapitalize="none"
                    autoCorrect="off"
                    spellCheck={false}
                    autoFocus
                    className="bg-transparent border-none outline-none text-[14px] font-medium text-text-base placeholder-text-muted/50 w-64"
                  />
                ) : (
                  <h3 className="text-[14px] font-medium text-text-base">{selectedName}</h3>
                )}
              </div>

              <div className="flex items-center gap-2">
                {isBuiltin && <BuiltInBadge />}
                {dirty && !isBuiltin && (
                  <button
                    onClick={handleSave}
                    disabled={isCreating && !newName.trim()}
                    className="flex items-center gap-1.5 px-3 py-1.5 bg-brand hover:bg-brand-hover text-white rounded text-[12px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-sm"
                  >
                    <Check size={12} /> Save
                  </button>
                )}
              </div>
            </div>

            {/* Body */}
            <div className="flex-1 overflow-y-auto p-6 custom-scrollbar">
              <div className="max-w-2xl space-y-8">

                {/* Beta notice — library servers only */}
                {isManaged && (
                  <div className="flex items-start gap-3 px-4 py-3 rounded-lg bg-warning/10 border border-warning/20">
                    <Info size={15} className="flex-shrink-0 mt-0.5 text-warning" />
                    <p className="text-[12px] leading-relaxed text-warning/80">
                      <span className="font-semibold text-warning">MCP configuration and authorisation is in beta.</span>{" "}
                      Some settings may change and certain authentication flows are not yet fully supported.
                    </p>
                  </div>
                )}

                {/* Author */}
                {(() => {
                  const descriptor: AuthorDescriptor = isBuiltin
                    ? { type: "provider", name: "Automatic", url: "https://automatic.sh" }
                    : githubRepo
                    ? { type: "github", repo: githubRepo, url: config._author?.repository_url }
                    : config._author
                    ? { type: "provider", name: config._author.name, url: config._author.url ?? config._author.repository_url }
                    : { type: "local" };
                  return (
                    <section className="pb-2 border-b border-border-strong/40">
                      <AuthorSection descriptor={descriptor} />
                    </section>
                  );
                })()}

                {/* Transport Type + Enabled */}
                <section>
                  <div className="flex items-center justify-between mb-2">
                    <label className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
                      Type
                    </label>
                    <div className="flex items-center gap-2">
                      <span
                        className={`text-[12px] font-medium transition-colors ${
                          config.enabled !== false ? "text-success" : "text-text-muted"
                        } ${isBuiltin ? "opacity-60" : ""}`}
                      >
                        {config.enabled !== false ? "Enabled" : "Disabled"}
                      </span>
                      <button
                        role="switch"
                        aria-checked={config.enabled !== false}
                        onClick={() => !isBuiltin && updateConfig({ enabled: !config.enabled })}
                        disabled={isBuiltin}
                        title={isBuiltin ? "Built-in server — always enabled" : config.enabled !== false ? "Click to disable" : "Click to enable"}
                        className={`relative flex-shrink-0 w-9 h-5 rounded-full transition-colors focus:outline-none focus:ring-1 focus:ring-brand/60 ${
                          config.enabled !== false ? "bg-success" : "bg-surface-active"
                        } ${isBuiltin ? "opacity-60 cursor-not-allowed" : ""}`}
                      >
                        <span
                          className={`absolute top-0.5 w-4 h-4 rounded-full bg-white shadow transition-all ${
                            config.enabled !== false ? "left-[18px]" : "left-0.5"
                          }`}
                        />
                      </button>
                    </div>
                  </div>
                  <div role="tablist" aria-label="Transport type" className="inline-flex gap-1 p-1 bg-bg-sidebar rounded-lg">
                    <button
                      role="tab"
                      aria-selected={isStdio}
                      onClick={() => !isManaged && setTransport("stdio")}
                      disabled={isManaged}
                      className={`px-3 py-1.5 rounded-md text-[12px] font-medium transition-colors ${
                        isStdio
                          ? "bg-bg-base text-text-base shadow-sm"
                          : "text-text-muted hover:text-text-base"
                      } ${isManaged ? "cursor-not-allowed" : ""} ${isManaged && !isStdio ? "opacity-60" : ""}`}
                    >
                      Local
                    </button>
                    <button
                      role="tab"
                      aria-selected={!isStdio}
                      onClick={() => !isManaged && setTransport(config.type === "sse" ? "sse" : "http")}
                      disabled={isManaged}
                      className={`px-3 py-1.5 rounded-md text-[12px] font-medium transition-colors ${
                        !isStdio
                          ? "bg-bg-base text-text-base shadow-sm"
                          : "text-text-muted hover:text-text-base"
                      } ${isManaged ? "cursor-not-allowed" : ""} ${isManaged && isStdio ? "opacity-60" : ""}`}
                    >
                      Remote
                    </button>
                  </div>
                  <p className="mt-1.5 text-[11px] text-text-muted">
                    {isStdio
                      ? "Launches a local process and communicates via stdin/stdout."
                      : "Connects to a remote MCP server over HTTP."}
                  </p>
                </section>

                {/* HTTP vs SSE sub-option for remote */}
                {!isStdio && (
                  <section>
                    <label className="block text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-2">
                      Transport Protocol
                    </label>
                    <div role="tablist" aria-label="Transport protocol" className="inline-flex gap-1 p-1 bg-bg-sidebar rounded-lg">
                      <button
                        role="tab"
                        aria-selected={config.type === "http"}
                        onClick={() => !isManaged && setTransport("http")}
                        disabled={isManaged}
                        className={`px-3 py-1.5 rounded-md text-[12px] font-medium transition-colors ${
                          config.type === "http"
                            ? "bg-bg-base text-text-base shadow-sm"
                            : "text-text-muted hover:text-text-base"
                        } ${isManaged ? "cursor-not-allowed" : ""} ${isManaged && config.type !== "http" ? "opacity-60" : ""}`}
                      >
                        Streamable HTTP
                      </button>
                      <button
                        role="tab"
                        aria-selected={config.type === "sse"}
                        onClick={() => !isManaged && setTransport("sse")}
                        disabled={isManaged}
                        className={`px-3 py-1.5 rounded-md text-[12px] font-medium transition-colors ${
                          config.type === "sse"
                            ? "bg-bg-base text-text-base shadow-sm"
                            : "text-text-muted hover:text-text-base"
                        } ${isManaged ? "cursor-not-allowed" : ""} ${isManaged && config.type !== "sse" ? "opacity-60" : ""}`}
                      >
                        SSE (legacy)
                      </button>
                    </div>
                  </section>
                )}

                {/* ── stdio fields ────────────────────────────────────────── */}
                {isStdio && (
                  <>
                    <section>
                      <label className="block text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-2">
                        <span className="flex items-center gap-1.5">
                          <Terminal size={12} /> Command
                        </span>
                      </label>
                      <input
                        type="text"
                        value={config.command || ""}
                        onChange={(e) => !isManaged && updateConfig({ command: e.target.value })}
                        autoCapitalize="none"
                        autoCorrect="off"
                        spellCheck={false}
                        readOnly={isManaged}
                        placeholder="e.g. npx, node, /usr/local/bin/mcp-server"
                        className={`${inputClass} ${isManaged ? "opacity-60 cursor-not-allowed" : ""}`}
                      />
                    </section>

                    <section>
                      <label className="block text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-2">
                        Arguments
                      </label>
                      {(config.args || []).length > 0 && (
                      <ul className="space-y-1 mb-2">
                        {(config.args || []).map((arg, i) => (
                          <li
                            key={i}
                            className={`group flex items-center justify-between px-3 py-1.5 bg-bg-input rounded-md border border-border-strong/40 text-[13px] text-text-base font-mono ${isManaged ? "opacity-60" : ""}`}
                          >
                            <span className="truncate">{arg}</span>
                            {!isManaged && (
                              <button
                                onClick={() => removeArg(i)}
                                className="text-text-muted hover:text-danger opacity-0 group-hover:opacity-100 transition-all flex-shrink-0 ml-2"
                              >
                                <Trash2 size={12} />
                              </button>
                            )}
                          </li>
                        ))}
                      </ul>
                      )}
                      {!isManaged && (
                      <div className="flex gap-2">
                        <input
                          type="text"
                          value={newArg}
                          onChange={(e) => setNewArg(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === "Enter" && newArg) {
                              e.preventDefault();
                              addArg();
                            }
                          }}
                          placeholder="Add argument..."
                          className={smallInputClass}
                        />
                        <button onClick={addArg} disabled={!newArg} className={addBtnClass}>
                          Add
                        </button>
                      </div>
                      )}
                    </section>

                    {(!isManaged || config.cwd) && (
                    <section>
                      <label className="block text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-2">
                        Working Directory
                      </label>
                      <input
                        type="text"
                        value={config.cwd || ""}
                        onChange={(e) => !isManaged && updateConfig({ cwd: e.target.value })}
                        readOnly={isManaged}
                        placeholder="Optional — defaults to system default"
                        className={`${inputClass} ${isManaged ? "opacity-60 cursor-not-allowed" : ""}`}
                      />
                    </section>
                    )}

                  </>
                )}

                {/* ── Environment Variables — stdio always; remote when env keys exist ── */}
                {(isStdio || Object.keys(config.env || {}).length > 0) && (
                  <section>
                    <label className="block text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-2">
                      <span className="flex items-center gap-1.5">
                        <Variable size={12} /> Environment Variables
                      </span>
                    </label>
                    <KvEditor
                      entries={config.env || {}}
                      onChange={(updated) => updateConfig({ env: updated })}
                      keyPlaceholder="KEY"
                      valuePlaceholder="value"
                      colorKey
                      maskValue
                    />
                    <p className="mt-2 text-[11px] text-text-muted leading-relaxed">
                      Values are encrypted at rest but{" "}
                      <strong className="font-semibold text-text-base">written as plaintext to your project</strong>.
                      {" "}Leave a value empty to write{" "}
                      <code className="font-mono">{"${KEY}"}</code>
                      {" "}instead, inheriting from your shell at runtime.
                    </p>
                    {(() => {
                      const links = tokenLinksForServer(selectedName || "", config.env || {});
                      if (links.length === 0) return null;
                      return (
                        <div className="mt-2 flex flex-col gap-1">
                          {links.map((l) => (
                            <a
                              key={l.name}
                              href={l.url}
                              target="_blank"
                              rel="noopener noreferrer"
                              onClick={handleExternalLinkClick(l.url)}
                              className="inline-flex items-center gap-1 text-[11px] text-brand hover:text-brand-hover w-fit"
                            >
                              {l.label} for <code className="font-mono">{l.name}</code>
                              <ExternalLink size={10} />
                            </a>
                          ))}
                        </div>
                      );
                    })()}
                  </section>
                )}

                {/* ── http/sse fields ─────────────────────────────────────── */}
                {!isStdio && (
                  <>
                    <section>
                      <label className="block text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-2">
                        <span className="flex items-center gap-1.5">
                          <Globe size={12} /> URL
                        </span>
                      </label>
                      <input
                        type="text"
                        value={config.url || ""}
                        onChange={(e) => !isManaged && updateConfig({ url: e.target.value })}
                        readOnly={isManaged}
                        placeholder={
                          config.type === "sse"
                            ? "https://example.com/sse"
                            : "https://example.com/mcp"
                        }
                        className={`${inputClass} ${isManaged ? "opacity-60 cursor-not-allowed" : ""}`}
                      />
                    </section>

                    {(!isManaged || Object.keys(config.headers || {}).length > 0) && (
                    <section>
                      <label className="block text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-2">
                        HTTP Headers
                      </label>
                      <KvEditor
                        entries={config.headers || {}}
                        onChange={(updated) => updateConfig({ headers: updated })}
                        readOnly={isManaged}
                        keyPlaceholder="Header-Name"
                        valuePlaceholder="value"
                        colorKey
                      />
                    </section>
                    )}

                    {/* OAuth Authentication — only for servers with no env-var-based auth */}
                    {Object.keys(config.env || {}).length === 0 && (
                      <OAuthSection key={selectedName || ""} serverName={selectedName || ""} url={config.url || ""} />
                    )}
                  </>
                )}

                {/* ── Timeout (common) ────────────────────────────────────── */}
                {(!isManaged || config.timeout) && (
                <section>
                  <label className="block text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-2">
                    Timeout (ms)
                  </label>
                  <input
                    type="number"
                    value={config.timeout ?? ""}
                    onChange={(e) =>
                      !isManaged && updateConfig({
                        timeout: e.target.value ? parseInt(e.target.value, 10) : undefined,
                      })
                    }
                    readOnly={isManaged}
                    placeholder="Optional — e.g. 5000"
                    className={`w-48 bg-bg-input border border-border-strong/40 hover:border-border-strong focus:border-brand rounded-md px-3 py-2 text-[13px] text-text-base placeholder-text-muted/40 outline-none font-mono transition-colors ${isManaged ? "opacity-60 cursor-not-allowed" : ""}`}
                  />
                </section>
                )}
              </div>
            </div>
          </div>
        )}
      </AssetDrawer>

      <McpServerImportDialog
        isOpen={importOpen}
        existingNames={servers}
        onClose={() => setImportOpen(false)}
        onImported={async (names) => {
          await loadServers();
          setRecentRefresh((prev) => prev + 1);
          const first = names[0];
          if (first) await selectServer(first);
        }}
      />
    </div>
  );
}
