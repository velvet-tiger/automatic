// Extracted verbatim from ProjectEditor.tsx (Phase 2E — behavior-preserving).

import { invoke } from "@tauri-apps/api/core";
import { Check, Code, Edit2, Plus, RefreshCw, Search, Sparkles, Trash2, Upload, X } from "lucide-react";
import { SkillSelector } from "../../../../../components/SkillSelector";
import { LineNumberedTextarea } from "../../../../../components/LineNumberedTextarea";
import { SkillAddButton } from "../SkillAddButton";
import type { CustomSkill, DriftReport, Project, ProjectRecommendation } from "../../types";

interface SkillsPanelProps {
  project: Project;
  setProject: (next: Project) => void;
  selectedName: string | null;
  setProjectDetailsMap: React.Dispatch<React.SetStateAction<Map<string, Project>>>;
  setDirty: (v: boolean) => void;
  setSyncStatus: (v: string | null) => void;
  setError: (v: string | null) => void;
  setDriftReport: React.Dispatch<React.SetStateAction<DriftReport | null>>;
  setDriftByProject: React.Dispatch<React.SetStateAction<Record<string, boolean>>>;
  customSkillEditingIdx: number | null;
  setCustomSkillEditingIdx: (v: number | null) => void;
  customSkillEditName: string;
  setCustomSkillEditName: (v: string) => void;
  customSkillEditContent: string;
  setCustomSkillEditContent: (v: string) => void;
  availableSkills: string[];
  pluginLockedSkills: string[];
  addItem: (field: "skills" | "mcp_servers" | "providers" | "agents", value: string) => Promise<boolean>;
  removeItem: (field: "skills" | "mcp_servers" | "providers" | "agents", index: number) => void;
  loadAvailableSkills: () => Promise<void>;
  notifyProjectUpdated: () => void;
  aiSkillsSuggestions: ProjectRecommendation[];
  aiSkillsLoading: boolean;
  handleSuggestSkills: () => void | Promise<void>;
  removeRecommendation: (id: number) => void;
  onNavigateToSkill?: (skillName: string) => void;
  onNavigateToSkillStore?: (skillId: string) => void;
  onNavigateToSkillStoreWithResult?: (result: { id: string; name: string; source: string; installs: number }) => void;
}

export function SkillsPanel(props: SkillsPanelProps) {
  const {
    project, setProject, selectedName, setProjectDetailsMap, setDirty,
    setSyncStatus, setError, setDriftReport, setDriftByProject,
    customSkillEditingIdx, setCustomSkillEditingIdx,
    customSkillEditName, setCustomSkillEditName,
    customSkillEditContent, setCustomSkillEditContent,
    availableSkills, pluginLockedSkills,
    addItem, removeItem, loadAvailableSkills, notifyProjectUpdated,
    aiSkillsSuggestions, aiSkillsLoading, handleSuggestSkills,
    removeRecommendation,
    onNavigateToSkill, onNavigateToSkillStore, onNavigateToSkillStoreWithResult,
  } = props;

  const customSkills: CustomSkill[] = project.custom_skills || [];

  const saveProjectWithSkills = async (updatedProject: Project) => {
    if (!selectedName) return;
    const toSave = { ...updatedProject, name: selectedName, updated_at: new Date().toISOString() };
    setProject(toSave);
    try {
      setSyncStatus("syncing");
      await invoke("save_project", {
        name: selectedName,
        data: JSON.stringify(toSave, null, 2),
      });
      setProjectDetailsMap((prev) => new Map(prev).set(selectedName, toSave));
      setDirty(false);
      setSyncStatus(toSave.directory && toSave.agents.length > 0 ? "Saved & synced" : "Saved");
      if (toSave.directory && toSave.agents.length > 0) {
        setDriftReport({ drifted: false, agents: [] });
        setDriftByProject((prev) => ({ ...prev, [selectedName]: false }));
      }
    } catch (err: unknown) {
      setError(`Save failed: ${err}`);
      setSyncStatus(null);
    }
  };

  const handleAddCustomSkill = () => {
    const newSkill: CustomSkill = {
      name: "new-skill",
      content: "---\nname: New Skill\ndescription: Describe what this skill does and when to use it.\n---\n\nWrite the skill instructions here.\n",
    };
    setProject({ ...project, custom_skills: [...customSkills, newSkill] });
    setCustomSkillEditingIdx(customSkills.length);
    setCustomSkillEditName(newSkill.name);
    setCustomSkillEditContent(newSkill.content);
    setDirty(true);
  };

  const handleDeleteCustomSkill = async (idx: number) => {
    const updated = customSkills.filter((_, i) => i !== idx);
    if (customSkillEditingIdx === idx) {
      setCustomSkillEditingIdx(null);
    } else if (customSkillEditingIdx !== null && customSkillEditingIdx > idx) {
      setCustomSkillEditingIdx(customSkillEditingIdx - 1);
    }
    await saveProjectWithSkills({ ...project, custom_skills: updated.length > 0 ? updated : undefined });
  };

  const handleStartEditCustomSkill = (idx: number) => {
    setCustomSkillEditingIdx(idx);
    setCustomSkillEditName(customSkills[idx]?.name ?? "");
    setCustomSkillEditContent(customSkills[idx]?.content ?? "");
  };

  const handleCommitCustomSkill = async () => {
    if (customSkillEditingIdx === null) return;
    const updated = customSkills.map((skill, i) =>
      i === customSkillEditingIdx
        ? {
            name: customSkillEditName.trim().toLowerCase().replace(/\s+/g, "-") || "untitled-skill",
            content: customSkillEditContent,
          }
        : skill
    );
    setCustomSkillEditingIdx(null);
    await saveProjectWithSkills({ ...project, custom_skills: updated });
  };

  return (
    <>
      {/* ── Project Skills (custom, inline) ──────────────── */}
      <section className="mb-6">
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-2">
            <Code size={13} className="text-text-muted" />
            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Project Skills</span>
            {customSkills.length > 0 && (
              <span className="text-[10px] bg-bg-sidebar border border-border-strong/40 rounded-full px-1.5 py-0.5 text-text-muted leading-none">
                {customSkills.length}
              </span>
            )}
          </div>
          <button
            onClick={handleAddCustomSkill}
            className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium"
          >
            <Plus size={12} /> Add Skill
          </button>
        </div>

        {customSkills.length === 0 ? (
          <button
            onClick={handleAddCustomSkill}
            className="w-full flex items-center justify-center gap-2 px-4 py-6 border border-dashed border-border-strong/60 hover:border-brand/40 rounded-lg text-text-muted hover:text-brand transition-colors text-[13px]"
          >
            <Plus size={14} /> Create a project-scoped skill
          </button>
        ) : (
          <div className="space-y-2">
            {customSkills.map((skill, idx) => {
              const isEditing = customSkillEditingIdx === idx;
              return (
                <div
                  key={`${skill.name}-${idx}`}
                  className={`rounded-lg border transition-colors ${
                    isEditing
                      ? "border-brand/40 bg-bg-input"
                      : "border-border-strong/40 bg-bg-input hover:border-border-strong"
                  }`}
                >
                  {isEditing ? (
                    <div className="p-3 space-y-2">
                      <input
                        type="text"
                        value={customSkillEditName}
                        onChange={(e) => setCustomSkillEditName(e.target.value)}
                        placeholder="skill-name (lowercase, hyphens)"
                        className="w-full bg-bg-sidebar border border-border-strong/40 focus:border-brand rounded-md px-3 py-1.5 text-[13px] text-text-base placeholder-text-muted/50 outline-none transition-colors font-mono"
                      />
                      <LineNumberedTextarea
                        value={customSkillEditContent}
                        onChange={setCustomSkillEditContent}
                        placeholder="Write the skill content as Markdown with YAML frontmatter..."
                        variant="inline"
                        rows={12}
                        className="w-full"
                      />
                      <div className="flex items-center justify-end gap-2 pt-1">
                        <button
                          onClick={() => setCustomSkillEditingIdx(null)}
                          className="px-3 py-1 text-[12px] text-text-muted hover:text-text-base transition-colors"
                        >
                          Cancel
                        </button>
                        <button
                          onClick={handleCommitCustomSkill}
                          className="flex items-center gap-1 px-3 py-1 bg-brand hover:bg-brand-hover text-white text-[12px] font-medium rounded transition-colors"
                        >
                          <Check size={11} /> Save
                        </button>
                      </div>
                    </div>
                  ) : (
                    <div className="flex items-center gap-3 px-3 py-2.5">
                      <Code size={14} className="flex-shrink-0 text-text-muted" />
                      <div className="flex-1 min-w-0">
                        <div className="text-[13px] font-medium text-text-base truncate font-mono">{skill.name || "untitled-skill"}</div>
                        {skill.content.trim() ? (
                          <div className="text-[11px] text-text-muted truncate mt-0.5">
                            {skill.content.trim().split("\n").find(l => l.trim() && !l.startsWith("---"))?.slice(0, 60) || "Custom skill"}
                          </div>
                        ) : (
                          <div className="text-[11px] text-text-muted/60 italic mt-0.5">Empty</div>
                        )}
                      </div>
                      <div className="flex items-center gap-1 flex-shrink-0">
                        <button
                          onClick={() => handleStartEditCustomSkill(idx)}
                          className="p-1.5 text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
                          title="Edit"
                        >
                          <Edit2 size={12} />
                        </button>
                        <button
                          onClick={async () => {
                            if (!selectedName) return;
                            try {
                              setSyncStatus("syncing");
                              await invoke("save_skill", { name: skill.name, content: skill.content });
                              const remainingCustom = customSkills.filter((_, i) => i !== idx);
                              const updatedProject = {
                                ...project,
                                skills: [...project.skills, skill.name],
                                custom_skills: remainingCustom.length > 0 ? remainingCustom : undefined,
                              };
                              if (customSkillEditingIdx === idx) {
                                setCustomSkillEditingIdx(null);
                              } else if (customSkillEditingIdx !== null && customSkillEditingIdx > idx) {
                                setCustomSkillEditingIdx(customSkillEditingIdx - 1);
                              }
                              await saveProjectWithSkills(updatedProject);
                              await loadAvailableSkills();
                              setSyncStatus(`Imported "${skill.name}" to global registry`);
                              setTimeout(() => setSyncStatus(null), 4000);
                            } catch (err: unknown) {
                              setSyncStatus(`Import failed: ${err}`);
                              setTimeout(() => setSyncStatus(null), 4000);
                            }
                          }}
                          className="p-1.5 text-text-muted hover:text-success hover:bg-success/10 rounded transition-colors"
                          title="Import to global skill registry"
                        >
                          <Upload size={12} />
                        </button>
                        <button
                          onClick={() => handleDeleteCustomSkill(idx)}
                          className="p-1.5 text-text-muted hover:text-danger hover:bg-danger/10 rounded transition-colors"
                          title="Delete"
                        >
                          <Trash2 size={12} />
                        </button>
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </section>

      {/* Global Skills */}
      <section>
        <SkillSelector
          skills={project.skills}
          availableSkills={availableSkills}
          onAdd={(s) => addItem("skills", s)}
          onRemove={(i) => removeItem("skills", i)}
          showRemoveButtonAlways
          lockedSkills={pluginLockedSkills}
          emptyMessage="No skills attached."
          onReadSkill={async (skillName) => {
            const content: string = await invoke("read_skill", { name: skillName });
            return content;
          }}
          onNavigateToSkill={onNavigateToSkill}
          onForkSkill={async (skillName, content) => {
            if (!selectedName) return;
            try {
              const existingCustomNames = new Set((project.custom_skills ?? []).map(s => s.name));
              const taken = new Set([...project.skills, ...existingCustomNames]);
              let copyName = `${skillName}-copy`;
              let n = 2;
              while (taken.has(copyName)) {
                copyName = `${skillName}-copy-${n}`;
                n++;
              }
              const newCustomSkill: CustomSkill = { name: copyName, content };
              const forkedProject = {
                ...project,
                name: selectedName,
                custom_skills: [...(project.custom_skills ?? []), newCustomSkill],
                updated_at: new Date().toISOString(),
              };
              setProject(forkedProject);
              await invoke("save_project", {
                name: selectedName,
                data: JSON.stringify(forkedProject, null, 2),
              });
              setProjectDetailsMap((prev) => new Map(prev).set(selectedName, forkedProject));
              setDirty(false);
              notifyProjectUpdated();
              setSyncStatus(`Forked "${skillName}" → project skill "${copyName}"`);
              setTimeout(() => setSyncStatus(null), 5000);
            } catch (err: unknown) {
              setError(`Fork failed: ${err}`);
            }
          }}
        />
      </section>

      {/* ── AI skill suggestions ──────────────────────────── */}
      <section>
        <div className="flex items-center gap-2">
          <Sparkles size={12} className="text-text-muted" />
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">AI Suggestions</span>
          {aiSkillsSuggestions.length > 0 && !aiSkillsLoading && (
            <span className="text-[10px] font-semibold px-1.5 py-0.5 rounded-full bg-brand/10 text-brand border border-brand/20 leading-none">
              {aiSkillsSuggestions.length}
            </span>
          )}
          <div className="flex-1" />
          <button
            onClick={handleSuggestSkills}
            disabled={aiSkillsLoading}
            className="flex items-center gap-1.5 px-2.5 py-1.5 text-[11px] font-medium text-text-muted hover:text-text-base border border-border-strong/50 rounded-md disabled:opacity-40 transition-colors"
            title="Ask AI to suggest skills based on this project's configuration"
          >
            <Sparkles size={11} className={aiSkillsLoading ? "animate-pulse" : ""} />
            {aiSkillsLoading ? "Analysing…" : "Suggest skills"}
          </button>
        </div>

        {aiSkillsLoading && (
          <div className="mt-2 bg-bg-input border border-border-strong/40 rounded-lg px-4 py-4 flex items-center gap-3">
            <RefreshCw size={13} className="text-brand animate-spin flex-shrink-0" />
            <p className="text-[12px] text-text-muted">Searching the skill library and Discover…</p>
          </div>
        )}

        {!aiSkillsLoading && aiSkillsSuggestions.length === 0 && (
          <p className="mt-1.5 text-[12px] text-text-muted">
            Click "Suggest skills" to get AI-powered recommendations based on this project.
          </p>
        )}

        {!aiSkillsLoading && aiSkillsSuggestions.length > 0 && (
          <div className="mt-2 bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
            {aiSkillsSuggestions.map((rec) => (
              <div key={rec.id} className="flex items-start gap-3 px-4 py-3 group hover:bg-surface-hover transition-colors">
                <Sparkles size={13} className="flex-shrink-0 mt-0.5 text-brand" />
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2 mb-0.5">
                    <span className="text-[13px] font-semibold text-text-base font-mono">{rec.title}</span>
                    {rec.priority === "high" && (
                      <span className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-warning/15 text-warning border border-warning/20 leading-none">High</span>
                    )}
                  </div>
                  <p className="text-[12px] text-text-muted leading-relaxed">{rec.body}</p>
                  <div className="flex items-center gap-2 mt-2">
                    {(onNavigateToSkillStoreWithResult || onNavigateToSkillStore) && (
                      <button
                        onClick={() => {
                          if (rec.metadata && onNavigateToSkillStoreWithResult) {
                            try {
                              const meta = JSON.parse(rec.metadata) as { id: string; name: string; source: string; installs: number };
                              if (meta.id && meta.name && meta.source) {
                                onNavigateToSkillStoreWithResult(meta);
                                return;
                              }
                            } catch {
                              // fall through to plain query
                            }
                          }
                          onNavigateToSkillStore?.(rec.title);
                        }}
                        className="text-[11px] font-medium text-text-muted hover:text-text-base border border-border-strong/50 rounded px-2 py-1 transition-colors flex items-center gap-1"
                      >
                        <Search size={10} /> View
                      </button>
                    )}
                    <SkillAddButton
                      rec={rec}
                      alreadyAdded={project.skills.includes(rec.title)}
                      onAdd={async (skillName) => {
                        const added = await addItem("skills", skillName);
                        if (!added) return false;
                        try {
                          await invoke("action_recommendation", { id: rec.id });
                          removeRecommendation(rec.id);
                        } catch (err) {
                          console.error("Failed to mark recommendation as actioned:", err);
                        }
                        return true;
                      }}
                    />
                  </div>
                </div>
                <button
                  onClick={async () => {
                    await invoke("dismiss_recommendation", { id: rec.id });
                    removeRecommendation(rec.id);
                  }}
                  className="flex-shrink-0 p-1 text-text-muted hover:text-text-base transition-colors opacity-0 group-hover:opacity-100"
                  title="Dismiss"
                >
                  <X size={12} />
                </button>
              </div>
            ))}
          </div>
        )}
      </section>
    </>
  );
}
