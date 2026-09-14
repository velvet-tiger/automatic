// Extracted from Templates.tsx so the Templates and Profiles pages share one picker.

import { useState } from "react";
import { InheritedBadge } from "./ProtectionBadge";
import { Bot, Plus, Search, Trash2, X } from "lucide-react";

/** Picker for workspace sub-agents (library machine names). */
export function SubAgentSelector({
  agentIds,
  available,
  onAdd,
  onRemove,
  locked = [],
  lockedLabel,
}: {
  agentIds: string[];
  available: { id: string; name: string }[];
  onAdd: (id: string) => void;
  onRemove: (idx: number) => void;
  /** Ids that cannot be removed (e.g. provided by a profile). */
  locked?: string[];
  /** Name of the profile that provides an id, when one does. Renders a badge. */
  lockedLabel?: (id: string) => string | undefined;
}) {
  const [adding, setAdding] = useState(false);
  const [search, setSearch] = useState("");

  const unadded = available.filter(a => !agentIds.includes(a.id));
  const filtered = search.trim()
    ? unadded.filter(a =>
        a.name.toLowerCase().includes(search.toLowerCase()) ||
        a.id.toLowerCase().includes(search.toLowerCase())
      )
    : unadded;

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
            Sub-Agents
          </span>
        </div>
        <button
          onClick={(e) => { e.stopPropagation(); setAdding(true); }}
          className="text-[11px] text-brand hover:text-text-base flex items-center gap-1 px-2 py-1 rounded border border-brand/50 hover:border-brand hover:bg-brand/15 transition-all"
        >
          <Plus size={11} /> Add
        </button>
      </div>

      {/* Empty state */}
      {agentIds.length === 0 && !adding && (
        <p className="text-[12px] text-text-muted italic pl-1">No sub-agents configured.</p>
      )}

      {/* Current list */}
      <div className="space-y-2">
        {agentIds.map((id, idx) => {
          const agent = available.find(a => a.id === id);
          const inheritedFrom = lockedLabel?.(id);
          const isLocked = locked.includes(id) || !!inheritedFrom;
          return (
            <div key={id} className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden">
              <div className="flex items-center gap-3 px-3 py-3 group">
                <Bot size={20} className="text-icon-agent flex-shrink-0" />
                <div className="flex-1 min-w-0">
                  <div className="text-[13px] font-medium text-text-base">{agent?.name ?? id}</div>
                </div>
                {inheritedFrom && <InheritedBadge profile={inheritedFrom} />}
                {!isLocked && (
                  <button
                    onClick={(e) => { e.stopPropagation(); onRemove(idx); }}
                    className="text-text-muted hover:text-danger opacity-0 group-hover:opacity-100 transition-all p-1 hover:bg-surface rounded flex-shrink-0"
                  >
                    <Trash2 size={12} />
                  </button>
                )}
              </div>
            </div>
          );
        })}
      </div>

      {/* Searchable add dropdown */}
      {adding && (
        <div className="mt-2 bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden">
          <div className="flex items-center gap-2 px-3 py-2 border-b border-border-strong/40">
            <Search size={12} className="text-text-muted shrink-0" />
            <input
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") handleCancel();
                if (e.key === "Enter" && filtered.length === 1) handleAdd(filtered[0]!.id);
              }}
              placeholder="Search sub-agents..."
              autoFocus
              className="flex-1 bg-transparent outline-none text-[13px] text-text-base placeholder-text-muted/50"
            />
            {search && (
              <button onClick={() => setSearch("")} className="text-text-muted hover:text-text-base transition-colors">
                <X size={11} />
              </button>
            )}
          </div>
          <div className="max-h-48 overflow-y-auto custom-scrollbar py-1">
            {filtered.length > 0 ? (
              filtered.map((a) => (
                <button
                  key={a.id}
                  onClick={() => handleAdd(a.id)}
                  className="w-full flex items-center gap-2.5 px-3 py-2 hover:bg-bg-sidebar text-left transition-colors"
                >
                  <Bot size={14} className="text-icon-agent flex-shrink-0" />
                  <span className="text-[13px] text-text-base font-medium">{a.name}</span>
                </button>
              ))
            ) : (
              <p className="text-[12px] text-text-muted italic px-3 py-3">
                {unadded.length === 0 ? "All sub-agents already added." : "No sub-agents match."}
              </p>
            )}
          </div>
          <div className="border-t border-border-strong/40 px-3 py-2 flex items-center justify-between">
            <span className="text-[11px] text-text-muted">
              {filtered.length} of {unadded.length} sub-agent{unadded.length !== 1 ? "s" : ""}
            </span>
            <button onClick={handleCancel} className="text-[11px] text-text-muted hover:text-text-base transition-colors">
              Cancel
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
