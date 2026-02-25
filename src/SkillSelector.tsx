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
          <Code size={13} className="text-[#4ADE80]" />
          <span className="text-[11px] font-semibold text-[#8A8C93] tracking-wider uppercase">
            {label}
          </span>
        </div>
        {availableSkills.length > 0 && (
          <button
            onClick={(e) => { e.stopPropagation(); setAdding(true); }}
            className="text-[11px] text-[#5E6AD2] hover:text-[#6B78E3] flex items-center gap-1 transition-colors"
          >
            <Plus size={12} /> Add
          </button>
        )}
      </div>

      {/* Empty state */}
      {skills.length === 0 && !adding && (
        <p className="text-[12px] text-[#8A8C93]/50 italic pl-1">{emptyMessage}</p>
      )}

      {/* Current skills list */}
      <div className="space-y-2">
        {skills.map((skill, idx) => (
          <div
            key={skill}
            className="flex items-center gap-3 px-3 py-3 bg-[#1A1A1E] border border-[#33353A] rounded-lg group"
          >
            <div className="w-8 h-8 rounded-md bg-[#4ADE80]/12 flex items-center justify-center flex-shrink-0">
              <Code size={15} className="text-[#4ADE80]" />
            </div>
            <div className="flex-1 min-w-0">
              <div className="text-[13px] font-medium text-[#E0E1E6]">{skill}</div>
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
                if (e.key === "Enter" && filteredSkills.length === 1) handleAdd(filteredSkills[0]!);
              }}
              placeholder="Search skills..."
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
            {filteredSkills.length > 0 ? (
              filteredSkills.map((s) => (
                <button
                  key={s}
                  onClick={() => handleAdd(s)}
                  className="w-full flex items-center gap-2.5 px-3 py-2 hover:bg-[#2D2E36] text-left transition-colors"
                >
                  <div className="w-5 h-5 rounded bg-[#4ADE80]/10 flex items-center justify-center flex-shrink-0">
                    <Code size={11} className="text-[#4ADE80]" />
                  </div>
                  <span className="text-[13px] text-[#E0E1E6]">{s}</span>
                </button>
              ))
            ) : (
              <p className="text-[12px] text-[#8A8C93] italic px-3 py-3">
                {unaddedSkills.length === 0 ? "All skills already added." : "No skills match."}
              </p>
            )}
          </div>

          {/* Footer */}
          <div className="border-t border-[#33353A] px-3 py-2 flex items-center justify-between">
            <span className="text-[11px] text-[#8A8C93]">
              {filteredSkills.length} of {unaddedSkills.length} skill{unaddedSkills.length !== 1 ? "s" : ""}
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
