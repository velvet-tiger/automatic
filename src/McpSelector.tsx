import { useState } from "react";
import { Plus, Search, Server, Trash2, X } from "lucide-react";

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
}

/**
 * Shared MCP server selector used by both Projects and ProjectTemplates.
 * Renders:
 *   - A section header with an "Add" button (hidden when disableAdd=true)
 *   - The current list of servers as styled card rows
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
}: McpSelectorProps) {
  const [adding, setAdding] = useState(false);
  const [search, setSearch] = useState("");

  const unaddedServers = availableServers.filter((s) => !servers.includes(s));
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

  return (
    <div>
      {/* Header */}
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-2">
          <Server size={13} className="text-warning" />
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
            {label}
          </span>
        </div>
        {!disableAdd && availableServers.length > 0 && (
          <button
            onClick={(e) => { e.stopPropagation(); setAdding(true); }}
            className="text-[11px] text-brand-light hover:text-text-base flex items-center gap-1 px-2 py-1 rounded border border-brand/50 hover:border-brand hover:bg-brand/15 transition-all"
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
      <div className="space-y-2">
        {servers.map((srv, idx) => (
          <div
            key={srv}
            className="flex items-center gap-3 px-3 py-3 bg-bg-input border border-border-strong/40 rounded-lg group"
          >
            <div className="w-8 h-8 rounded-md bg-warning/12 flex items-center justify-center flex-shrink-0">
              <Server size={15} className="text-warning" />
            </div>
            <div className="flex-1 min-w-0">
              <div className="text-[13px] font-medium text-text-base">{srv}</div>
            </div>
            <button
              onClick={() => onRemove(idx)}
              className="text-text-muted hover:text-danger opacity-0 group-hover:opacity-100 transition-all p-1 hover:bg-surface rounded"
            >
              <Trash2 size={12} />
            </button>
          </div>
        ))}
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
                  <div className="w-5 h-5 rounded bg-warning/10 flex items-center justify-center flex-shrink-0">
                    <Server size={11} className="text-warning" />
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
