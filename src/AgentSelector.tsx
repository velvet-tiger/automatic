import { useState } from "react";
import { Bot, Plus, Search, Trash2, X } from "lucide-react";

export interface AgentInfo {
  id: string;
  label: string;
  description: string;
}

interface AgentSelectorProps {
  /** IDs of currently selected agents */
  agentIds: string[];
  /** All available agents to pick from */
  availableAgents: AgentInfo[];
  /** Called when an agent is added (passes the agent id) */
  onAdd: (id: string) => void;
  /** Called when an agent is removed by index */
  onRemove: (index: number) => void;
  /** Optional label override (default: "Agent Tools") */
  label?: string;
  /** Empty-state message (default: "No agents configured.") */
  emptyMessage?: string;
}

/**
 * Shared agent selector used by both Projects and ProjectTemplates.
 * Renders:
 *   - A section header with an "Add" button
 *   - The current list of agents as styled card rows (label + description)
 *   - A searchable dropdown panel when adding
 */
export function AgentSelector({
  agentIds,
  availableAgents,
  onAdd,
  onRemove,
  label = "Agent Tools",
  emptyMessage = "No agents configured.",
}: AgentSelectorProps) {
  const [adding, setAdding] = useState(false);
  const [search, setSearch] = useState("");

  const unaddedAgents = availableAgents.filter((a) => !agentIds.includes(a.id));
  const filteredAgents = search.trim()
    ? unaddedAgents.filter(
        (a) =>
          a.label.toLowerCase().includes(search.toLowerCase()) ||
          a.description.toLowerCase().includes(search.toLowerCase())
      )
    : unaddedAgents;

  function handleAdd(id: string) {
    onAdd(id);
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
          <Bot size={13} className="text-[#5E6AD2]" />
          <span className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase">
            {label}
          </span>
        </div>
        <button
          onClick={(e) => { e.stopPropagation(); setAdding(true); }}
          className="text-[11px] text-[#5E6AD2] hover:text-[#6B78E3] flex items-center gap-1 transition-colors"
        >
          <Plus size={12} /> Add
        </button>
      </div>

      {/* Empty state */}
      {agentIds.length === 0 && !adding && (
        <p className="text-[12px] text-[#8A8C93]/50 italic pl-1">{emptyMessage}</p>
      )}

      {/* Current agents list */}
      <div className="space-y-2">
        {agentIds.map((id, idx) => {
          const info = availableAgents.find((a) => a.id === id);
          return (
            <div
              key={id}
              className="flex items-center gap-3 px-3 py-3 bg-[#1A1A1E] border border-[#33353A] rounded-lg group"
            >
              <div className="w-8 h-8 rounded-md bg-[#5E6AD2]/15 flex items-center justify-center flex-shrink-0">
                <Bot size={15} className="text-[#5E6AD2]" />
              </div>
              <div className="flex-1 min-w-0">
                <div className="text-[13px] font-medium text-[#E0E1E6]">{info?.label ?? id}</div>
                {info?.description && (
                  <div className="text-[11px] text-[#8A8C93] mt-0.5">{info.description}</div>
                )}
              </div>
              <button
                onClick={() => onRemove(idx)}
                className="text-[#8A8C93] hover:text-[#FF6B6B] opacity-0 group-hover:opacity-100 transition-all p-1 hover:bg-[#33353A] rounded"
              >
                <Trash2 size={12} />
              </button>
            </div>
          );
        })}
      </div>

      {/* Searchable add dropdown */}
      {adding && (
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
                if (e.key === "Enter" && filteredAgents.length === 1) handleAdd(filteredAgents[0]!.id);
              }}
              placeholder="Search agents..."
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
            {filteredAgents.length > 0 ? (
              filteredAgents.map((a) => (
                <button
                  key={a.id}
                  onClick={() => handleAdd(a.id)}
                  className="w-full flex items-center gap-2.5 px-3 py-2 hover:bg-[#2D2E36] text-left transition-colors"
                >
                  <div className="w-5 h-5 rounded bg-[#5E6AD2]/15 flex items-center justify-center flex-shrink-0">
                    <Bot size={11} className="text-[#5E6AD2]" />
                  </div>
                  <div className="flex-1 min-w-0">
                    <span className="text-[13px] text-[#E0E1E6] font-medium">{a.label}</span>
                    {a.description && (
                      <span className="text-[11px] text-[#8A8C93] ml-2">{a.description}</span>
                    )}
                  </div>
                </button>
              ))
            ) : (
              <p className="text-[12px] text-[#8A8C93] italic px-3 py-3">
                {unaddedAgents.length === 0 ? "All agents already added." : "No agents match."}
              </p>
            )}
          </div>

          {/* Footer */}
          <div className="border-t border-[#33353A] px-3 py-2 flex items-center justify-between">
            <span className="text-[11px] text-[#8A8C93]">
              {filteredAgents.length} of {unaddedAgents.length} agent{unaddedAgents.length !== 1 ? "s" : ""}
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
