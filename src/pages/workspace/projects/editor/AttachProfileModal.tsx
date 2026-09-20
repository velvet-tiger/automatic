import { useState } from "react";
import { Check, Layers, Search, X } from "lucide-react";
import type { ProjectProfile } from "../types";

export function AttachProfileModal({
  profiles,
  attached,
  selected,
  onSelect,
  onCancel,
  onConfirm,
}: {
  profiles: ProjectProfile[];
  attached: string[];
  selected: string | null;
  onSelect: (name: string) => void;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const [filter, setFilter] = useState("");

  const available = profiles.filter((p) => !attached.includes(p.name));
  const trimmed = filter.trim().toLowerCase();
  const visible = trimmed
    ? available.filter(
        (p) =>
          p.name.toLowerCase().includes(trimmed) ||
          (p.description ?? "").toLowerCase().includes(trimmed)
      )
    : available;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/50" onClick={onCancel} />
      <div className="relative bg-bg-input border border-border-strong rounded-xl shadow-2xl w-full max-w-md mx-4 flex flex-col max-h-[80vh]">
        <div className="flex items-center justify-between px-5 py-4 border-b border-border-strong/40 flex-shrink-0">
          <h2 className="text-[15px] font-semibold text-text-base">Attach Profile</h2>
          <button
            onClick={onCancel}
            className="p-1 text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
          >
            <X size={16} />
          </button>
        </div>

        <div className="px-5 pt-3 pb-2 flex-shrink-0">
          <p className="text-[12px] text-text-muted leading-relaxed mb-3">
            Attach a profile to this project. The profile keeps its skills, MCP servers, rules and other
            resources in step across every project that attaches it.
          </p>
          {available.length > 0 && (
            <div className="flex items-center gap-2 px-3 py-2 bg-bg-base border border-border-strong/40 rounded-md">
              <Search size={12} className="text-text-muted shrink-0" />
              <input
                type="text"
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
                placeholder="Filter profiles..."
                autoFocus
                className="flex-1 bg-transparent outline-none text-[13px] text-text-base placeholder-text-muted/50"
              />
              {filter && (
                <button
                  onClick={() => setFilter("")}
                  className="text-text-muted hover:text-text-base transition-colors"
                >
                  <X size={11} />
                </button>
              )}
            </div>
          )}
        </div>

        <div className="flex-1 overflow-y-auto custom-scrollbar px-3 pb-3 min-h-0">
          {profiles.length === 0 ? (
            <div className="px-3 py-8 text-[12px] text-text-muted text-center">
              No profiles yet. Create one in the Profiles section of the Library.
            </div>
          ) : available.length === 0 ? (
            <div className="px-3 py-8 text-[12px] text-text-muted text-center">
              Every profile is already attached to this project.
            </div>
          ) : visible.length === 0 ? (
            <div className="px-3 py-8 text-[12px] text-text-muted text-center">
              No profiles match.
            </div>
          ) : (
            <ul className="space-y-1">
              {visible.map((p) => {
                const isSelected = selected === p.name;
                const counts: { label: string; n: number }[] = [
                  { label: "skills", n: p.skills?.length ?? 0 },
                  { label: "MCP", n: p.mcp_servers?.length ?? 0 },
                  { label: "rules", n: p.rules?.length ?? 0 },
                  { label: "hooks", n: p.hooks?.length ?? 0 },
                  { label: "agents", n: p.agents?.length ?? 0 },
                  { label: "sub-agents", n: p.user_agents?.length ?? 0 },
                  { label: "commands", n: p.user_commands?.length ?? 0 },
                  { label: "providers", n: p.providers?.length ?? 0 },
                ].filter((c) => c.n > 0);
                return (
                  <li key={p.name}>
                    <button
                      onClick={() => onSelect(p.name)}
                      onDoubleClick={() => { onSelect(p.name); onConfirm(); }}
                      className={`w-full flex items-start gap-3 px-3 py-2.5 rounded-lg text-left transition-colors ${
                        isSelected
                          ? "bg-brand/15 border border-brand/40"
                          : "border border-transparent hover:bg-bg-sidebar"
                      }`}
                    >
                      <Layers
                        size={14}
                        className={`mt-0.5 flex-shrink-0 ${isSelected ? "text-brand" : "text-text-muted"}`}
                      />
                      <div className="flex-1 min-w-0">
                        <div className="text-[13px] font-medium text-text-base truncate">{p.name}</div>
                        {p.description && (
                          <div className="text-[11px] text-text-muted truncate">{p.description}</div>
                        )}
                        {counts.length > 0 && (
                          <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5 mt-1">
                            {counts.map((c) => (
                              <span key={c.label} className="text-[10px] text-text-muted">
                                {c.n} {c.label}
                              </span>
                            ))}
                          </div>
                        )}
                      </div>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>

        <div className="flex items-center justify-end gap-2 px-5 py-3 border-t border-border-strong/40 flex-shrink-0">
          <button
            onClick={onCancel}
            className="flex h-[28px] items-center px-3 text-[12px] text-text-muted hover:text-text-base bg-bg-sidebar hover:bg-surface rounded transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={onConfirm}
            disabled={!selected}
            className="flex h-[28px] items-center gap-1.5 px-3 bg-brand hover:bg-brand-hover text-white rounded text-[12px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-sm"
          >
            <Check size={12} /> Attach
          </button>
        </div>
      </div>
    </div>
  );
}
