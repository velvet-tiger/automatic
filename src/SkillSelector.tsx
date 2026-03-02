import { useState } from "react";
import { Code, Plus, Search, Trash2, X, ExternalLink, GitFork, ChevronRight } from "lucide-react";
import { MarkdownPreview } from "./MarkdownPreview";

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
  /** Read the raw content of a global skill. When provided, clicking a skill toggles a preview panel. */
  onReadSkill?: (skill: string) => Promise<string>;
  /** Navigate to the skill library, pre-selecting the given skill. */
  onNavigateToSkill?: (skill: string) => void;
  /** Fork a global skill into this project's local skills. Called with skill name + its raw content. */
  onForkSkill?: (skill: string, content: string) => Promise<void>;
}

/**
 * Shared skill selector used by both Projects and ProjectTemplates.
 * Renders:
 *   - A section header with an "Add" button
 *   - The current list of skills as styled card rows
 *   - A searchable dropdown panel when adding
 *   - (optional) Inline skill preview with "View in library" and "Fork" actions
 */
export function SkillSelector({
  skills,
  availableSkills,
  onAdd,
  onRemove,
  label = "Skills",
  emptyMessage = "No skills configured.",
  onReadSkill,
  onNavigateToSkill,
  onForkSkill,
}: SkillSelectorProps) {
  const [adding, setAdding] = useState(false);
  const [search, setSearch] = useState("");

  // Expanded skill preview state
  const [expandedSkill, setExpandedSkill] = useState<string | null>(null);
  const [expandedContent, setExpandedContent] = useState<string>("");
  const [expandedLoading, setExpandedLoading] = useState(false);
  const [expandedError, setExpandedError] = useState<string | null>(null);
  const [forkingSkill, setForkingSkill] = useState<string | null>(null);

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

  async function handleToggleExpand(skill: string) {
    if (!onReadSkill) return;

    if (expandedSkill === skill) {
      // Collapse
      setExpandedSkill(null);
      setExpandedContent("");
      setExpandedError(null);
      return;
    }

    setExpandedSkill(skill);
    setExpandedContent("");
    setExpandedError(null);
    setExpandedLoading(true);
    try {
      const content = await onReadSkill(skill);
      setExpandedContent(content);
    } catch (err: any) {
      setExpandedError(String(err));
    } finally {
      setExpandedLoading(false);
    }
  }

  async function handleFork(skill: string) {
    if (!onForkSkill) return;
    setForkingSkill(skill);
    try {
      await onForkSkill(skill, expandedContent);
      // Keep the row open — the global skill stays in place.
      // The parent will show the new local copy in the Local Skills section.
    } catch (err: any) {
      // Surface in the expanded panel if the parent didn't handle it
      setExpandedError(String(err));
    } finally {
      setForkingSkill(null);
    }
  }

  // Strip YAML frontmatter for the markdown preview body
  function extractBody(raw: string): string {
    const match = raw.match(/^---\r?\n[\s\S]*?\r?\n---\r?\n?([\s\S]*)$/);
    return match ? match[1]!.trimStart() : raw;
  }

  return (
    <div>
      {/* Header */}
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-2">
          <Code size={13} className="text-icon-skill" />
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
            {label}
          </span>
        </div>
        {availableSkills.length > 0 && (
          <button
            onClick={(e) => { e.stopPropagation(); setAdding(true); }}
            className="text-[11px] text-brand hover:text-text-base flex items-center gap-1 px-2 py-1 rounded border border-brand/50 hover:border-brand hover:bg-brand/15 transition-all"
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
        {skills.map((skill, idx) => {
          const isExpanded = expandedSkill === skill;
          const isClickable = !!onReadSkill;

          return (
            <div
              key={skill}
              className={`bg-bg-input border rounded-lg group transition-colors ${
                isExpanded ? "border-brand/40" : "border-border-strong/40"
              }`}
            >
              {/* Row */}
              <div className="flex items-center gap-3 px-3 py-3">
                <div className="w-8 h-8 rounded-md bg-icon-skill/12 flex items-center justify-center flex-shrink-0">
                  <Code size={15} className="text-icon-skill" />
                </div>

                {/* Name — clickable to expand when onReadSkill is provided */}
                {isClickable ? (
                  <button
                    className="flex-1 flex items-center gap-2 text-left min-w-0"
                    onClick={() => handleToggleExpand(skill)}
                  >
                    <span className="text-[13px] font-medium text-text-base flex-1 truncate">{skill}</span>
                    <ChevronRight
                      size={12}
                      className={`text-text-muted flex-shrink-0 transition-transform ${isExpanded ? "rotate-90" : ""}`}
                    />
                  </button>
                ) : (
                  <div className="flex-1 min-w-0">
                    <div className="text-[13px] font-medium text-text-base">{skill}</div>
                  </div>
                )}

                <button
                  onClick={() => onRemove(idx)}
                  className="text-text-muted hover:text-danger opacity-0 group-hover:opacity-100 transition-all p-1 hover:bg-surface rounded"
                >
                  <Trash2 size={12} />
                </button>
              </div>

              {/* Expanded preview panel */}
              {isExpanded && (
                <div className="border-t border-border-strong/40">
                  {/* Action bar */}
                  <div className="flex items-center gap-3 px-3 py-2 border-b border-border-strong/30 bg-bg-sidebar/30">
                    {onNavigateToSkill && (
                      <button
                        onClick={() => onNavigateToSkill(skill)}
                        className="flex items-center gap-1 text-[11px] text-text-muted hover:text-brand transition-colors"
                        title="View this skill in the Skills library"
                      >
                        <ExternalLink size={11} />
                        View in library
                      </button>
                    )}
                    {onForkSkill && !expandedLoading && !expandedError && expandedContent && (
                      <>
                        {onNavigateToSkill && (
                          <span className="text-border-strong text-[11px]">·</span>
                        )}
                        <button
                          onClick={() => handleFork(skill)}
                          disabled={forkingSkill === skill}
                          className="flex items-center gap-1 text-[11px] text-text-muted hover:text-brand transition-colors disabled:opacity-50"
                          title="Copy this skill into the project's local skills so you can customise it"
                        >
                          <GitFork size={11} />
                          {forkingSkill === skill ? "Forking…" : "Fork to local"}
                        </button>
                      </>
                    )}
                  </div>

                  {/* Content */}
                  <div className="px-4 py-3 max-h-80 overflow-y-auto custom-scrollbar">
                    {expandedLoading && (
                      <p className="text-[12px] text-text-muted italic">Loading…</p>
                    )}
                    {expandedError && (
                      <p className="text-[12px] text-danger">{expandedError}</p>
                    )}
                    {!expandedLoading && !expandedError && expandedContent && (
                      <MarkdownPreview content={extractBody(expandedContent)} />
                    )}
                  </div>
                </div>
              )}
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
                  <div className="w-5 h-5 rounded bg-icon-skill/10 flex items-center justify-center flex-shrink-0">
                    <Code size={11} className="text-icon-skill" />
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
