import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Plus,
  Server,
  Trash2,
  X,
  ChevronRight,
  Terminal,
  Globe,
  Variable,
  ArrowUpRight,
  Loader2,
} from "lucide-react";

// ── Minimal config shape (mirrors McpServers.tsx) ──────────────────────────

interface McpServerConfig {
  type: "stdio" | "http" | "sse";
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  cwd?: string;
  url?: string;
  headers?: Record<string, string>;
  enabled?: boolean;
  timeout?: number;
}

// ── Props ──────────────────────────────────────────────────────────────────

interface McpSelectorProps {
  /** Currently selected MCP server names */
  servers: string[];
  /** All available MCP server names to pick from */
  availableServers: string[];
  /** Called when a server is added */
  onAdd: (server: string) => void;
  /** Called when a server is removed by index */
  onRemove: (index: number) => void;
  /** Whether the add button should be hidden (e.g. Warp-only projects) */
  disableAdd?: boolean;
  /** Optional label override (default: "MCP Servers") */
  label?: string;
  /** Empty-state message (default: "No MCP servers configured.") */
  emptyMessage?: string;
  /**
   * Optional callback to navigate to the full MCP server config page.
   * When provided, the inline card shows a "View full configuration" link.
   */
  onNavigateToMcpServer?: (serverName: string) => void;
  /** Keep remove buttons visible instead of only showing them on hover. */
  showRemoveButtonAlways?: boolean;
  /** Optional project-scoped enabled state for each server. */
  isServerEnabled?: (serverName: string) => boolean;
  /** Optional callback to toggle whether a server is synced into agent config files. */
  onToggleEnabled?: (serverName: string, enabled: boolean) => void | Promise<void>;
}

// ── Inline read-only config card ───────────────────────────────────────────

interface McpConfigCardProps {
  name: string;
  onNavigate?: (name: string) => void;
}

function McpConfigCard({ name, onNavigate }: McpConfigCardProps) {
  const [config, setConfig] = useState<McpServerConfig | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);

  // Kick off a single load on first render
  if (!loaded && !loading) {
    setLoading(true);
    invoke<string>("read_mcp_server_config", { name })
      .then((raw) => {
        setConfig(JSON.parse(raw) as McpServerConfig);
        setLoaded(true);
        setLoading(false);
      })
      .catch((err) => {
        setError(String(err));
        setLoaded(true);
        setLoading(false);
      });
  }

  const isStdio = config?.type === "stdio";
  const envEntries = Object.entries(config?.env ?? {});
  const headerEntries = Object.entries(config?.headers ?? {});

  return (
    <div className="text-[12px] divide-y divide-border-strong/30">
      {loading && (
        <div className="flex items-center gap-2 px-4 py-3 text-text-muted">
          <Loader2 size={12} className="animate-spin" />
          <span>Loading config…</span>
        </div>
      )}

      {error && (
        <div className="px-4 py-3 text-danger">{error}</div>
      )}

      {config && !loading && (
        <>
          {/* Type + enabled badge */}
          <div className="flex items-center justify-between px-4 py-2.5">
            <div className="flex items-center gap-1.5">
              {isStdio ? (
                <Terminal size={11} className="text-text-muted" />
              ) : (
                <Globe size={11} className="text-text-muted" />
              )}
              <span className="text-text-muted uppercase tracking-wider text-[10px] font-semibold">
                {config.type}
              </span>
            </div>
            <span
              className={`text-[10px] font-medium px-1.5 py-0.5 rounded-full ${
                config.enabled === false
                  ? "bg-text-muted/10 text-text-muted"
                  : "bg-success/10 text-success"
              }`}
            >
              {config.enabled === false ? "Disabled" : "Enabled"}
            </span>
          </div>

          {/* stdio: command + args */}
          {isStdio && config.command && (
            <div className="px-4 py-2.5">
              <p className="text-[10px] font-semibold text-text-muted uppercase tracking-wider mb-1">
                Command
              </p>
              <code className="text-text-base font-mono text-[12px] break-all">
                {config.command}
                {(config.args ?? []).length > 0 && (
                  <span className="text-text-muted"> {config.args!.join(" ")}</span>
                )}
              </code>
            </div>
          )}

          {/* stdio: working dir */}
          {isStdio && config.cwd && (
            <div className="px-4 py-2.5">
              <p className="text-[10px] font-semibold text-text-muted uppercase tracking-wider mb-1">
                Working Dir
              </p>
              <code className="text-text-base font-mono text-[12px] break-all">{config.cwd}</code>
            </div>
          )}

          {/* stdio: env vars */}
          {isStdio && envEntries.length > 0 && (
            <div className="px-4 py-2.5">
              <p className="text-[10px] font-semibold text-text-muted uppercase tracking-wider mb-1.5 flex items-center gap-1">
                <Variable size={10} /> Env vars
              </p>
              <ul className="space-y-1">
                {envEntries.map(([k, v]) => (
                  <li key={k} className="flex items-center gap-2 font-mono">
                    <span className="text-brand text-[11px]">{k}</span>
                    <span className="text-text-muted text-[11px]">=</span>
                    <span className="text-text-base text-[11px] truncate">
                      {v !== "" ? v : <em className="text-text-muted/60">empty</em>}
                    </span>
                  </li>
                ))}
              </ul>
            </div>
          )}

          {/* http/sse: url */}
          {!isStdio && config.url && (
            <div className="px-4 py-2.5">
              <p className="text-[10px] font-semibold text-text-muted uppercase tracking-wider mb-1">
                URL
              </p>
              <code className="text-text-base font-mono text-[12px] break-all">{config.url}</code>
            </div>
          )}

          {/* http/sse: headers */}
          {!isStdio && headerEntries.length > 0 && (
            <div className="px-4 py-2.5">
              <p className="text-[10px] font-semibold text-text-muted uppercase tracking-wider mb-1.5">
                Headers
              </p>
              <ul className="space-y-1">
                {headerEntries.map(([k, v]) => (
                  <li key={k} className="flex items-center gap-2 font-mono">
                    <span className="text-brand text-[11px]">{k}</span>
                    <span className="text-text-muted text-[11px]">:</span>
                    <span className="text-text-base text-[11px] truncate">{v}</span>
                  </li>
                ))}
              </ul>
            </div>
          )}

          {/* Footer: link to full config */}
          {onNavigate && (
            <div className="px-4 py-2.5">
              <button
                onClick={() => onNavigate(name)}
                className="flex items-center gap-1 text-brand hover:text-brand-hover text-[11px] font-medium transition-colors"
              >
                View full configuration
                <ArrowUpRight size={11} />
              </button>
            </div>
          )}
        </>
      )}
    </div>
  );
}

// ── Main component ─────────────────────────────────────────────────────────

/**
 * Shared MCP server selector used by both Projects and ProjectTemplates.
 * Renders:
 *   - A section header with an "Add from Library" trigger (hidden when disableAdd=true)
 *   - The current list of servers as compact rows
 *   - Clicking a row expands an inline read-only config card with a link
 *     back to the full MCP server configuration page
 *   - A floating searchable dropdown when adding
 */
export function McpSelector({
  servers,
  availableServers,
  onAdd,
  onRemove,
  disableAdd = false,
  label = "MCP Servers",
  emptyMessage = "No MCP servers configured.",
  onNavigateToMcpServer,
  showRemoveButtonAlways = false,
  isServerEnabled,
  onToggleEnabled,
}: McpSelectorProps) {
  const [adding, setAdding] = useState(false);
  const [search, setSearch] = useState("");
  const [expandedServer, setExpandedServer] = useState<string | null>(null);

  // Sort current servers alphabetically for display, keeping original indices for onRemove.
  const sortedServers = servers
    .map((srv, idx) => ({ srv, idx }))
    .sort((a, b) => a.srv.localeCompare(b.srv, undefined, { sensitivity: "base" }));

  const unaddedServers = availableServers
    .filter((s) => !servers.includes(s) && s !== "automatic")
    .sort((a, b) => a.localeCompare(b, undefined, { sensitivity: "base" }));
  const filteredServers = search.trim()
    ? unaddedServers.filter((s) => s.toLowerCase().includes(search.toLowerCase()))
    : unaddedServers;

  function handleAdd(server: string) {
    onAdd(server);
    setAdding(false);
    setSearch("");
  }

  function toggleExpand(srv: string) {
    setExpandedServer((prev) => (prev === srv ? null : srv));
  }

  const emptyDropdownMessage = availableServers.length === 0
    ? "No MCP servers in the library yet."
    : unaddedServers.length === 0
      ? "All MCP servers already added."
      : "No servers match.";

  return (
    <div>
      {/* Header */}
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-2">
          <Server size={13} className="text-icon-mcp" />
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
            {label}
          </span>
          {servers.length > 0 && (
            <span className="text-[10px] bg-bg-sidebar border border-border-strong/40 rounded-full px-1.5 py-0.5 text-text-muted leading-none">
              {servers.length}
            </span>
          )}
        </div>
        {!disableAdd && (
          <div className="relative">
            <button
              onClick={(e) => { e.stopPropagation(); setAdding(!adding); }}
              className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium"
            >
              <Plus size={12} /> Add from Library
            </button>
            {adding && (
              <div className="absolute right-0 top-full mt-1 w-72 bg-bg-sidebar border border-border-strong rounded-lg shadow-xl z-50 max-h-72 overflow-y-auto">
                <div className="p-2 border-b border-border-strong/40">
                  <input
                    type="text"
                    value={search}
                    onChange={(e) => setSearch(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Escape") { setAdding(false); setSearch(""); }
                      if (e.key === "Enter" && filteredServers.length === 1) handleAdd(filteredServers[0]!);
                    }}
                    placeholder="Search MCP servers..."
                    autoFocus
                    className="w-full bg-bg-input border border-border-strong/40 focus:border-brand rounded px-2 py-1 text-[12px] text-text-base placeholder-text-muted/50 outline-none"
                  />
                </div>
                <div className="py-1">
                  {filteredServers.length === 0 ? (
                    <div className="px-3 py-2 text-[12px] text-text-muted italic">
                      {emptyDropdownMessage}
                    </div>
                  ) : (
                    filteredServers.map((s) => (
                      <button
                        key={s}
                        onClick={() => handleAdd(s)}
                        className="w-full flex items-center gap-2 px-3 py-2 hover:bg-bg-input text-left transition-colors"
                      >
                        <Server size={14} className="text-text-muted flex-shrink-0" />
                        <div className="min-w-0">
                          <div className="text-[12px] font-medium text-text-base truncate">{s}</div>
                        </div>
                      </button>
                    ))
                  )}
                </div>
              </div>
            )}
          </div>
        )}
      </div>

      {/* Empty state */}
      {servers.length === 0 && !adding && (
        <p className="text-[12px] text-text-muted italic pl-1">{emptyMessage}</p>
      )}

      {/* Current servers list */}
      <div className="space-y-2">
        {sortedServers.map(({ srv, idx }) => {
          const isExpanded = expandedServer === srv;
          const enabled = isServerEnabled ? isServerEnabled(srv) : true;
          const canToggleEnabled = !!onToggleEnabled && srv !== "automatic";
          const isLocked = srv === "automatic";
          return (
            <div
              key={srv}
              className={`bg-bg-input border rounded-lg group transition-colors ${
                isExpanded ? "border-brand/40" : "border-border-strong/40"
              }`}
            >
              {/* Row */}
              <div className="flex items-center gap-3 px-3 py-2.5">
                <Server size={14} className="flex-shrink-0 text-text-muted" />

                <button
                  className="flex-1 flex items-center gap-2 text-left min-w-0"
                  onClick={() => toggleExpand(srv)}
                >
                  <div className="flex-1 min-w-0">
                    <div className="text-[13px] font-medium text-text-base truncate">{srv}</div>
                    {canToggleEnabled && !enabled && (
                      <div className="text-[11px] text-text-muted truncate">
                        Kept in Automatic only — not synced
                      </div>
                    )}
                  </div>
                  <ChevronRight
                    size={12}
                    className={`text-text-muted flex-shrink-0 transition-transform ${isExpanded ? "rotate-90" : ""}`}
                  />
                </button>

                {canToggleEnabled && (
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      void onToggleEnabled(srv, !enabled);
                    }}
                    className={`text-[10px] font-medium px-1.5 py-0.5 rounded-full border leading-none transition-colors flex-shrink-0 ${
                      enabled
                        ? "border-success/30 text-success bg-success/10 hover:bg-success/15"
                        : "border-border-strong/40 text-text-muted bg-bg-sidebar hover:bg-surface"
                    }`}
                    title={enabled ? "Disable syncing for this project" : "Enable syncing for this project"}
                  >
                    {enabled ? "On" : "Off"}
                  </button>
                )}

                {!isLocked && (
                  <button
                    onClick={(e) => { e.stopPropagation(); onRemove(idx); }}
                    className={`p-1.5 text-text-muted hover:text-danger hover:bg-danger/10 rounded transition-colors flex-shrink-0 ${showRemoveButtonAlways ? "opacity-100" : "opacity-0 group-hover:opacity-100"}`}
                    title="Remove"
                  >
                    {showRemoveButtonAlways ? <Trash2 size={12} /> : <X size={12} />}
                  </button>
                )}
              </div>

              {/* Inline config card */}
              {isExpanded && (
                <div className="border-t border-border-strong/40">
                  <McpConfigCard
                    name={srv}
                    onNavigate={onNavigateToMcpServer}
                  />
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
