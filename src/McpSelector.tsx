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
          <Server size={13} className="text-[#F59E0B]" />
          <span className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase">
            {label}
          </span>
        </div>
        {!disableAdd && availableServers.length > 0 && (
          <button
            onClick={(e) => { e.stopPropagation(); setAdding(true); }}
            className="text-[11px] text-[#5E6AD2] hover:text-[#6B78E3] flex items-center gap-1 transition-colors"
          >
            <Plus size={12} /> Add
          </button>
        )}
      </div>

      {/* Empty state */}
      {servers.length === 0 && !adding && (
        <p className="text-[12px] text-[#8A8C93]/50 italic pl-1">{emptyMessage}</p>
      )}

      {/* Current servers list */}
      <div className="space-y-2">
        {servers.map((srv, idx) => (
          <div
            key={srv}
            className="flex items-center gap-3 px-3 py-3 bg-[#1A1A1E] border border-[#33353A] rounded-lg group"
          >
            <div className="w-8 h-8 rounded-md bg-[#F59E0B]/12 flex items-center justify-center flex-shrink-0">
              <Server size={15} className="text-[#F59E0B]" />
            </div>
            <div className="flex-1 min-w-0">
              <div className="text-[13px] font-medium text-[#E0E1E6]">{srv}</div>
            </div>
            <button
              onClick={() => onRemove(idx)}
              className="text-[#8A8C93] hover:text-[#FF6B6B] opacity-0 group-hover:opacity-100 transition-all p-1 hover:bg-[#33353A] rounded"
            >
              <Trash2 size={12} />
            </button>
          </div>
        ))}
      </div>

      {/* Searchable add dropdown */}
      {adding && !disableAdd && (
        <div className="mt-2 bg-[#1A1A1E] border border-[#33353A] rounded-lg overflow-hidden">
          {/* Search input */}
          <div className="flex items-center gap-2 px-3 py-2 border-b border-[#33353A]">
            <Search size={12} className="text-[#8A8C93] shrink-0" />
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
              className="flex-1 bg-transparent outline-none text-[13px] text-[#E0E1E6] placeholder-[#8A8C93]/50"
            />
            {search && (
              <button
                onClick={() => setSearch("")}
                className="text-[#8A8C93] hover:text-[#E0E1E6] transition-colors"
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
                  className="w-full flex items-center gap-2.5 px-3 py-2 hover:bg-[#2D2E36] text-left transition-colors"
                >
                  <div className="w-5 h-5 rounded bg-[#F59E0B]/10 flex items-center justify-center flex-shrink-0">
                    <Server size={11} className="text-[#F59E0B]" />
                  </div>
                  <span className="text-[13px] text-[#E0E1E6]">{s}</span>
                </button>
              ))
            ) : (
              <p className="text-[12px] text-[#8A8C93] italic px-3 py-3">
                {unaddedServers.length === 0 ? "All MCP servers already added." : "No servers match."}
              </p>
            )}
          </div>

          {/* Footer */}
          <div className="border-t border-[#33353A] px-3 py-2 flex items-center justify-between">
            <span className="text-[11px] text-[#8A8C93]">
              {filteredServers.length} of {unaddedServers.length} server{unaddedServers.length !== 1 ? "s" : ""}
            </span>
            <button
              onClick={handleCancel}
              className="text-[11px] text-[#8A8C93] hover:text-[#E0E1E6] transition-colors"
            >
              Cancel
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
