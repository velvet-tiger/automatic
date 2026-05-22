import { useEffect, useState } from "react";
import { Code, Plus, Trash2, X, ExternalLink, GitFork, ChevronRight } from "lucide-react";
import { MarkdownPreview } from "./MarkdownPreview";
import { TokenPill } from "./TokenPill";

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
  /** Keep remove buttons visible instead of only showing them on hover. */
  showRemoveButtonAlways?: boolean;
  /** Skill names that cannot be removed (e.g. provided by a plugin). */
  lockedSkills?: string[];
}

/**
 * Shared skill selector used by both Projects and ProjectTemplates.
 * Renders:
 *   - A section header with an "Add from Library" trigger
 *   - The current list of skills as compact rows
 *   - A floating searchable dropdown when adding
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
  showRemoveButtonAlways = false,
  lockedSkills = [],
}: SkillSelectorProps) {
  const [adding, setAdding] = useState(false);
  const [search, setSearch] = useState("");

  // Expanded skill preview state
  const [expandedSkill, setExpandedSkill] = useState<string | null>(null);
  const [expandedContent, setExpandedContent] = useState<string>("");
  const [expandedLoading, setExpandedLoading] = useState(false);
  const [expandedError, setExpandedError] = useState<string | null>(null);
  const [forkingSkill, setForkingSkill] = useState<string | null>(null);
  const [skillContentCache, setSkillContentCache] = useState<Record<string, string>>({});

  // Sort current skills alphabetically for display, keeping original indices for onRemove.
  const sortedSkills = skills
    .map((skill, idx) => ({ skill, idx }))
    .sort((a, b) => a.skill.localeCompare(b.skill, undefined, { sensitivity: "base" }));

  const unaddedSkills = availableSkills
    .filter((s) => !skills.includes(s) && s !== "automatic")
    .sort((a, b) => a.localeCompare(b, undefined, { sensitivity: "base" }));
  const filteredSkills = search.trim()
    ? unaddedSkills.filter((s) => s.toLowerCase().includes(search.toLowerCase()))
    : unaddedSkills;

  useEffect(() => {
    if (!onReadSkill || skills.length === 0) return;
    const readSkill = onReadSkill;

    let cancelled = false;

    async function warmSkillContent(): Promise<void> {
      for (const skill of skills) {
        if (skillContentCache[skill] !== undefined) continue;
        try {
          const content = await readSkill(skill);
          if (!cancelled) {
            setSkillContentCache((prev) => (prev[skill] !== undefined ? prev : { ...prev, [skill]: content }));
          }
        } catch {
          if (!cancelled) {
            setSkillContentCache((prev) => (prev[skill] !== undefined ? prev : { ...prev, [skill]: "" }));
          }
        }
      }
    }

    void warmSkillContent();

    return () => {
      cancelled = true;
    };
  }, [onReadSkill, skillContentCache, skills]);

  function handleAdd(skill: string) {
    onAdd(skill);
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

  const emptyDropdownMessage = availableSkills.length === 0
    ? "No skills in the library yet."
    : unaddedSkills.length === 0
      ? "All skills already added."
      : "No skills match.";

  return (
    <div>
      {/* Header */}
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-2">
          <Code size={13} className="text-icon-skill" />
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
            {label}
          </span>
          {skills.length > 0 && (
            <span className="text-[10px] bg-bg-sidebar border border-border-strong/40 rounded-full px-1.5 py-0.5 text-text-muted leading-none">
              {skills.length}
            </span>
          )}
        </div>
        <div className="relative">
          <button
            onClick={(e) => { e.stopPropagation(); setAdding(!adding); }}
            className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium"
          >
            <Plus size={12} /> Add from Library
          </button>
          {adding && (
            <div className="absolute right-0 top-full mt-1 w-72 bg-bg-sidebar border border-border-strong rounded-lg shadow-xl z-50 max-h-72 overflow-y-auto">
              <div className="p-2 border-b border-border-strong/40">
                <input
                  type="text"
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Escape") { setAdding(false); setSearch(""); }
                    if (e.key === "Enter" && filteredSkills.length === 1) handleAdd(filteredSkills[0]!);
                  }}
                  placeholder="Search skills..."
                  autoFocus
                  className="w-full bg-bg-input border border-border-strong/40 focus:border-brand rounded px-2 py-1 text-[12px] text-text-base placeholder-text-muted/50 outline-none"
                />
              </div>
              <div className="py-1">
                {filteredSkills.length === 0 ? (
                  <div className="px-3 py-2 text-[12px] text-text-muted italic">
                    {emptyDropdownMessage}
                  </div>
                ) : (
                  filteredSkills.map((s) => (
                    <button
                      key={s}
                      onClick={() => handleAdd(s)}
                      className="w-full flex items-center gap-2 px-3 py-2 hover:bg-bg-input text-left transition-colors"
                    >
                      <Code size={14} className="text-text-muted flex-shrink-0" />
                      <div className="min-w-0">
                        <div className="text-[12px] font-medium text-text-base truncate">{s}</div>
                      </div>
                    </button>
                  ))
                )}
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Empty state */}
      {skills.length === 0 && !adding && (
        <p className="text-[12px] text-text-muted italic pl-1">{emptyMessage}</p>
      )}

      {/* Current skills list */}
      <div className="space-y-2">
        {sortedSkills.map(({ skill, idx }) => {
          const isExpanded = expandedSkill === skill;
          const isClickable = !!onReadSkill;
          const isLocked = lockedSkills.includes(skill) || skill === "automatic";

          return (
            <div
              key={skill}
              className={`bg-bg-input border rounded-lg group transition-colors ${
                isExpanded ? "border-brand/40" : "border-border-strong/40"
              }`}
            >
              {/* Row */}
              <div className="flex items-center gap-3 px-3 py-2.5">
                <Code size={14} className="flex-shrink-0 text-text-muted" />

                {/* Name — clickable to expand when onReadSkill is provided */}
                {isClickable ? (
                  <button
                    className="flex-1 flex items-center gap-2 text-left min-w-0"
                    onClick={() => handleToggleExpand(skill)}
                  >
                    <div className="flex-1 min-w-0">
                      <div className="text-[13px] font-medium text-text-base truncate">{skill}</div>
                    </div>
                    <ChevronRight
                      size={12}
                      className={`text-text-muted flex-shrink-0 transition-transform ${isExpanded ? "rotate-90" : ""}`}
                    />
                  </button>
                ) : (
                  <div className="flex-1 min-w-0">
                    <div className="text-[13px] font-medium text-text-base truncate">{skill}</div>
                  </div>
                )}

                <TokenPill text={skillContentCache[skill] ?? ""} />

                {!isLocked && (
                  <button
                    onClick={() => onRemove(idx)}
                    className={`p-1.5 text-text-muted hover:text-danger hover:bg-danger/10 rounded transition-colors flex-shrink-0 ${showRemoveButtonAlways ? "opacity-100" : "opacity-0 group-hover:opacity-100"}`}
                    title="Remove"
                  >
                    {showRemoveButtonAlways ? <Trash2 size={12} /> : <X size={12} />}
                  </button>
                )}
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
    </div>
  );
}
