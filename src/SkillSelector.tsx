import { useState } from "react";
import { Code, Plus, Search, Trash2, X } from "lucide-react";

interface SkillSelectorProps {
  /** Currently selected skills */
  skills: string[];
  /** All available skills to pick from */
  availableSkills: string[];
  /** Called when a skill is added */
  onAdd: (skill: string) => void;
  /** Called when a skill is removed by index */
  onRemove: (index: number) => void;
  /** Optional label override (default: "Skills") */
  label?: string;
  /** Empty-state message (default: "No skills configured.") */
  emptyMessage?: string;
}

/**
 * Shared skill selector used by both Projects and ProjectTemplates.
 * Renders:
 *   - A section header with an "Add" button
 *   - The current list of skills as styled card rows
 *   - A searchable dropdown panel when adding
 */
export function SkillSelector({
  skills,
  availableSkills,
  onAdd,
  onRemove,
  label = "Skills",
  emptyMessage = "No skills configured.",
}: SkillSelectorProps) {
  const [adding, setAdding] = useState(false);
  const [search, setSearch] = useState("");

  const unaddedSkills = availableSkills.filter((s) => !skills.includes(s));
  const filteredSkills = search.trim()
    ? unaddedSkills.filter((s) => s.toLowerCase().includes(search.toLowerCase()))
    : unaddedSkills;

  function handleAdd(skill: string) {
    onAdd(skill);
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
          <Code size={13} className="text-success" />
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
            {label}
          </span>
        </div>
        {availableSkills.length > 0 && (
          <button
            onClick={(e) => { e.stopPropagation(); setAdding(true); }}
            className="text-[11px] text-brand-light hover:text-text-base flex items-center gap-1 px-2 py-1 rounded border border-brand/50 hover:border-brand hover:bg-brand/15 transition-all"
          >
            <Plus size={11} /> Add
          </button>
        )}
      </div>

      {/* Empty state */}
      {skills.length === 0 && !adding && (
        <p className="text-[12px] text-text-muted italic pl-1">{emptyMessage}</p>
      )}

      {/* Current skills list */}
      <div className="space-y-2">
        {skills.map((skill, idx) => (
          <div
            key={skill}
            className="flex items-center gap-3 px-3 py-3 bg-bg-input border border-border-strong/40 rounded-lg group"
          >
            <div className="w-8 h-8 rounded-md bg-success/12 flex items-center justify-center flex-shrink-0">
              <Code size={15} className="text-success" />
            </div>
            <div className="flex-1 min-w-0">
              <div className="text-[13px] font-medium text-text-base">{skill}</div>
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
                if (e.key === "Enter" && filteredSkills.length === 1) handleAdd(filteredSkills[0]!);
              }}
              placeholder="Search skills..."
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
            {filteredSkills.length > 0 ? (
              filteredSkills.map((s) => (
                <button
                  key={s}
                  onClick={() => handleAdd(s)}
                  className="w-full flex items-center gap-2.5 px-3 py-2 hover:bg-bg-sidebar text-left transition-colors"
                >
                  <div className="w-5 h-5 rounded bg-success/10 flex items-center justify-center flex-shrink-0">
                    <Code size={11} className="text-success" />
                  </div>
                  <span className="text-[13px] text-text-base">{s}</span>
                </button>
              ))
            ) : (
              <p className="text-[12px] text-text-muted italic px-3 py-3">
                {unaddedSkills.length === 0 ? "All skills already added." : "No skills match."}
              </p>
            )}
          </div>

          {/* Footer */}
          <div className="border-t border-border-strong/40 px-3 py-2 flex items-center justify-between">
            <span className="text-[11px] text-text-muted">
              {filteredSkills.length} of {unaddedSkills.length} skill{unaddedSkills.length !== 1 ? "s" : ""}
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
