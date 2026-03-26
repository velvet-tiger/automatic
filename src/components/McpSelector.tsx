import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Plus,
  Search,
  Server,
  Trash2,
  Power,
  X,
  ChevronDown,
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
 *   - A section header with an "Add" button (hidden when disableAdd=true)
 *   - The current list of servers as styled card rows
 *   - Clicking a server expands an inline read-only config card with a link
 *     back to the full MCP server configuration page
 *   - A searchable dropdown panel when adding
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

  function handleCancel() {
    setAdding(false);
    setSearch("");
  }

  function toggleExpand(srv: string) {
    setExpandedServer((prev) => (prev === srv ? null : srv));
  }

  return (
    <div>
      {/* Header */}
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-2">
          <Server size={13} className="text-icon-mcp" />
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
            {label}
          </span>
        </div>
        {!disableAdd && availableServers.length > 0 && (
          <button
            onClick={(e) => { e.stopPropagation(); setAdding(true); }}
            className="text-[11px] text-brand hover:text-text-base flex items-center gap-1 px-2 py-1 rounded border border-brand/50 hover:border-brand hover:bg-brand/15 transition-all"
          >
            <Plus size={11} /> Add
          </button>
        )}
      </div>

      {/* Empty state */}
      {servers.length === 0 && !adding && (
        <p className="text-[12px] text-text-muted italic pl-1">{emptyMessage}</p>
      )}

      {/* Current servers list */}
      <div className="space-y-1.5">
        {sortedServers.map(({ srv, idx }) => {
          const isExpanded = expandedServer === srv;
          const enabled = isServerEnabled ? isServerEnabled(srv) : true;
          const canToggleEnabled = !!onToggleEnabled && srv !== "automatic";
          return (
            <div key={srv}>
              {/* Row — clicking toggles inline card */}
              <div
                className={`flex items-center gap-3 px-3 py-3 bg-bg-input border group cursor-pointer transition-colors ${
                  isExpanded
                    ? "border-border-strong rounded-t-lg rounded-b-none"
                    : "border-border-strong/40 rounded-lg hover:border-border-strong/70"
                }`}
                onClick={() => toggleExpand(srv)}
              >
                <span className="text-text-muted flex-shrink-0">
                  {isExpanded ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
                </span>

                <div className="w-7 h-7 rounded-md bg-icon-mcp/12 flex items-center justify-center flex-shrink-0">
                  <Server size={14} className="text-icon-mcp" />
                </div>

                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2 min-w-0">
                    <div className="text-[13px] font-medium text-text-base truncate">{srv}</div>
                    {!enabled && (
                      <span className="text-[10px] font-medium px-1.5 py-0.5 rounded-full bg-text-muted/10 text-text-muted border border-border-strong/40 leading-none">
                        Disabled
                      </span>
                    )}
                  </div>
                  {canToggleEnabled && (
                    <div className="mt-0.5 text-[11px] text-text-muted">
                      {enabled ? "Synced into agent MCP config" : "Kept in Automatic only"}
                    </div>
                  )}
                </div>

                {canToggleEnabled && (
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      void onToggleEnabled(srv, !enabled);
                    }}
                    className={`flex items-center gap-1.5 px-2 py-1 rounded-md border text-[11px] font-medium transition-colors ${
                      enabled
                        ? "border-success/30 text-success hover:border-success/50 hover:bg-success/10"
                        : "border-border-strong/50 text-text-muted hover:border-border-strong hover:text-text-base hover:bg-surface"
                    }`}
                    title={enabled ? "Disable syncing for this project" : "Enable syncing for this project"}
                  >
                    <Power size={11} />
                    {enabled ? "On" : "Off"}
                  </button>
                )}

                {srv !== "automatic" && (
                <button
                  onClick={(e) => { e.stopPropagation(); onRemove(idx); }}
                  className={`text-text-muted hover:text-danger transition-all p-1 hover:bg-surface rounded ${showRemoveButtonAlways ? "opacity-100" : "opacity-0 group-hover:opacity-100"}`}
                >
                  <Trash2 size={12} />
                </button>
                )}
              </div>

              {/* Inline config card */}
              {isExpanded && (
                <div className="border border-t-0 border-border-strong rounded-b-lg overflow-hidden bg-bg-input">
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

      {/* Searchable add dropdown */}
      {adding && !disableAdd && (
        <div className="mt-2 bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden">
          {/* Search input */}
          <div className="flex items-center gap-2 px-3 py-2 border-b border-border-strong/40">
            <Search size={12} className="text-text-muted shrink-0" />
            <input
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") handleCancel();
                if (e.key === "Enter" && filteredServers.length === 1) handleAdd(filteredServers[0]!);
              }}
              placeholder="Search MCP servers..."
              autoFocus
              className="flex-1 bg-transparent outline-none text-[13px] text-text-base placeholder-text-muted/50"
            />
            {search && (
              <button
                onClick={() => setSearch("")}
                className="text-text-muted hover:text-text-base transition-colors"
              >
                <X size={11} />
              </button>
            )}
          </div>

          {/* Results list */}
          <div className="max-h-48 overflow-y-auto custom-scrollbar py-1">
            {filteredServers.length > 0 ? (
              filteredServers.map((s) => (
                <button
                  key={s}
                  onClick={() => handleAdd(s)}
                  className="w-full flex items-center gap-2.5 px-3 py-2 hover:bg-bg-sidebar text-left transition-colors"
                >
                  <div className="w-5 h-5 rounded bg-icon-mcp/10 flex items-center justify-center flex-shrink-0">
                    <Server size={11} className="text-icon-mcp" />
                  </div>
                  <span className="text-[13px] text-text-base">{s}</span>
                </button>
              ))
            ) : (
              <p className="text-[12px] text-text-muted italic px-3 py-3">
                {unaddedServers.length === 0 ? "All MCP servers already added." : "No servers match."}
              </p>
            )}
          </div>

          {/* Footer */}
          <div className="border-t border-border-strong/40 px-3 py-2 flex items-center justify-between">
            <span className="text-[11px] text-text-muted">
              {filteredServers.length} of {unaddedServers.length} server{unaddedServers.length !== 1 ? "s" : ""}
            </span>
            <button
              onClick={handleCancel}
              className="text-[11px] text-text-muted hover:text-text-base transition-colors"
            >
              Cancel
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
