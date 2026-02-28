import { useState } from "react";
import { Bot, Plus, Search, Trash2, X } from "lucide-react";
import { AgentIcon } from "./AgentIcon";

export interface AgentCapabilities {
  skills: boolean;
  instructions: boolean;
  mcp_servers: boolean;
}

export interface AgentInfo {
  id: string;
  label: string;
  description: string;
  capabilities?: AgentCapabilities;
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
          <Bot size={13} className="text-icon-agent" />
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
            {label}
          </span>
        </div>
        <button
          onClick={(e) => { e.stopPropagation(); setAdding(true); }}
          className="text-[11px] text-icon-agent-light hover:text-text-base flex items-center gap-1 px-2 py-1 rounded border border-brand/50 hover:border-brand hover:bg-brand/15 transition-all"
        >
          <Plus size={11} /> Add
        </button>
      </div>

      {/* Empty state */}
      {agentIds.length === 0 && !adding && (
        <p className="text-[12px] text-text-muted italic pl-1">{emptyMessage}</p>
      )}

      {/* Current agents list */}
      <div className="space-y-2">
        {agentIds.map((id, idx) => {
          const info = availableAgents.find((a) => a.id === id);
          return (
            <div
              key={id}
              className="flex items-center gap-3 px-3 py-3 bg-bg-input border border-border-strong/40 rounded-lg group"
            >
              <AgentIcon agentId={id} size={20} />
              <div className="flex-1 min-w-0">
                <div className="text-[13px] font-medium text-text-base">{info?.label ?? id}</div>
                {info?.description && (
                  <div className="text-[11px] text-text-muted mt-0.5">{info.description}</div>
                )}
              </div>
              <button
                onClick={() => onRemove(idx)}
                className="text-text-muted hover:text-danger opacity-0 group-hover:opacity-100 transition-all p-1 hover:bg-surface rounded"
              >
                <Trash2 size={12} />
              </button>
            </div>
          );
        })}
      </div>

      {/* Searchable add dropdown */}
      {adding && (
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
                if (e.key === "Enter" && filteredAgents.length === 1) handleAdd(filteredAgents[0]!.id);
              }}
              placeholder="Search agents..."
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
            {filteredAgents.length > 0 ? (
              filteredAgents.map((a) => (
                <button
                  key={a.id}
                  onClick={() => handleAdd(a.id)}
                  className="w-full flex items-center gap-2.5 px-3 py-2 hover:bg-bg-sidebar text-left transition-colors"
                >
                  <AgentIcon agentId={a.id} size={14} />
                  <div className="flex-1 min-w-0">
                    <span className="text-[13px] text-text-base font-medium">{a.label}</span>
                    {a.description && (
                      <span className="text-[11px] text-text-muted ml-2">{a.description}</span>
                    )}
                  </div>
                </button>
              ))
            ) : (
              <p className="text-[12px] text-text-muted italic px-3 py-3">
                {unaddedAgents.length === 0 ? "All agents already added." : "No agents match."}
              </p>
            )}
          </div>

          {/* Footer */}
          <div className="border-t border-border-strong/40 px-3 py-2 flex items-center justify-between">
            <span className="text-[11px] text-text-muted">
              {filteredAgents.length} of {unaddedAgents.length} agent{unaddedAgents.length !== 1 ? "s" : ""}
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
