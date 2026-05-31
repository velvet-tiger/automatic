// Extracted verbatim from ProjectEditor.tsx (Phase 2E — behavior-preserving).

import { Check, Edit2, Plus, ScrollText, Trash2, X } from "lucide-react";
import { LineNumberedTextarea } from "../../../../../components/LineNumberedTextarea";
import { TokenPill } from "../../../../../components/TokenPill";
import type { CustomRule, Project } from "../../types";

interface RulesPanelProps {
  project: Project;
  setProject: (next: Project) => void;
  setDirty: (v: boolean) => void;
  dirty: boolean;
  pluginLockedRules: string[];
  availableRules: { id: string; name: string }[];
  customRuleEditingIdx: number | null;
  setCustomRuleEditingIdx: (v: number | null) => void;
  customRuleEditName: string;
  setCustomRuleEditName: (v: string) => void;
  customRuleEditContent: string;
  setCustomRuleEditContent: (v: string) => void;
  globalRuleSearch: string;
  setGlobalRuleSearch: (v: string) => void;
  globalRuleAdding: boolean;
  setGlobalRuleAdding: (v: boolean) => void;
  globalRuleContentCache: Record<string, string>;
  syncStatus: string | null;
  handleSave: () => void | Promise<void>;
}

export function RulesPanel({
  project, setProject, setDirty, dirty,
  pluginLockedRules, availableRules,
  customRuleEditingIdx, setCustomRuleEditingIdx,
  customRuleEditName, setCustomRuleEditName,
  customRuleEditContent, setCustomRuleEditContent,
  globalRuleSearch, setGlobalRuleSearch,
  globalRuleAdding, setGlobalRuleAdding,
  globalRuleContentCache, syncStatus, handleSave,
}: RulesPanelProps) {
  const MANDATORY_RULE = "automatic-service";
  const isRuleLocked = (ruleId: string) =>
    pluginLockedRules.includes(ruleId) || ruleId === MANDATORY_RULE;

  const configuredRules = (project.file_rules || {})["_project"] || [];
  const projectRules = configuredRules.includes(MANDATORY_RULE)
    ? configuredRules
    : [MANDATORY_RULE, ...configuredRules];
  const customRules: CustomRule[] = project.custom_rules || [];

  const handleToggleProjectRule = (ruleId: string) => {
    const existing = (project.file_rules || {})["_project"] || [];
    if (existing.includes(ruleId) && isRuleLocked(ruleId)) return;
    const updated = existing.includes(ruleId)
      ? existing.filter(r => r !== ruleId)
      : [...existing, ruleId];
    const newFileRules: Record<string, string[]> = { ...(project.file_rules || {}), _project: updated };
    if (updated.length === 0) delete newFileRules["_project"];
    setProject({ ...project, file_rules: newFileRules });
    setDirty(true);
  };

  const handleAddCustomRule = () => {
    const newRule: CustomRule = { name: "New Rule", content: "" };
    setProject({ ...project, custom_rules: [...customRules, newRule] });
    setCustomRuleEditingIdx(customRules.length);
    setCustomRuleEditName("New Rule");
    setCustomRuleEditContent("");
    setDirty(true);
  };

  const handleDeleteCustomRule = (idx: number) => {
    const updated = customRules.filter((_, i) => i !== idx);
    setProject({ ...project, custom_rules: updated });
    if (customRuleEditingIdx === idx) {
      setCustomRuleEditingIdx(null);
    } else if (customRuleEditingIdx !== null && customRuleEditingIdx > idx) {
      setCustomRuleEditingIdx(customRuleEditingIdx - 1);
    }
    setDirty(true);
  };

  const handleStartEditCustomRule = (idx: number) => {
    setCustomRuleEditingIdx(idx);
    setCustomRuleEditName(customRules[idx]?.name ?? "");
    setCustomRuleEditContent(customRules[idx]?.content ?? "");
  };

  const handleCommitCustomRule = () => {
    if (customRuleEditingIdx === null) return;
    const updated = customRules.map((r, i) =>
      i === customRuleEditingIdx
        ? { name: customRuleEditName.trim() || "Untitled Rule", content: customRuleEditContent }
        : r
    );
    setProject({ ...project, custom_rules: updated });
    setCustomRuleEditingIdx(null);
    setDirty(true);
  };

  const totalActive = projectRules.length + customRules.filter(r => r.content.trim()).length;

  const unaddedRules = availableRules.filter(r => !projectRules.includes(r.id));
  const filteredRules = globalRuleSearch.trim()
    ? unaddedRules.filter(r =>
        r.name.toLowerCase().includes(globalRuleSearch.toLowerCase()) ||
        r.id.toLowerCase().includes(globalRuleSearch.toLowerCase())
      )
    : unaddedRules;
  const emptyDropdownMessage = availableRules.length === 0
    ? "No rules in the library yet."
    : unaddedRules.length === 0
      ? "All rules already added."
      : "No rules match.";

  return (
    <div className="flex gap-6">
      <div className="flex-1 min-w-0 space-y-8">

        {/* ── Section header ── */}
        <div className="flex items-center justify-between">
          <div>
            <h2 className="text-[15px] font-semibold text-text-base">Rules</h2>
            <p className="text-[12px] text-text-muted mt-0.5">
              Rules are injected into all agent instruction files when the project is synced.
            </p>
          </div>
          {totalActive > 0 && (
            <span className="text-[11px] text-brand bg-brand/10 px-2 py-0.5 rounded border border-brand/20">
              {totalActive} active
            </span>
          )}
        </div>

        {/* ── Custom Rules ── */}
        <section>
          <div className="flex items-center justify-between mb-3">
            <div className="flex items-center gap-2">
              <Edit2 size={13} className="text-text-muted" />
              <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Project Rules</span>
              {customRules.length > 0 && (
                <span className="text-[10px] bg-bg-sidebar border border-border-strong/40 rounded-full px-1.5 py-0.5 text-text-muted leading-none">
                  {customRules.length}
                </span>
              )}
            </div>
            <button
              onClick={handleAddCustomRule}
              className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium"
            >
              <Plus size={12} /> Add Rule
            </button>
          </div>
          <p className="text-[12px] text-text-muted mb-3">
            Write rules directly in this project. They are injected alongside any global rules selected below.
          </p>

          {customRules.length === 0 ? (
            <button
              onClick={handleAddCustomRule}
              className="w-full flex items-center justify-center gap-2 px-4 py-6 border border-dashed border-border-strong/60 hover:border-brand/40 rounded-lg text-text-muted hover:text-brand transition-colors text-[13px]"
            >
              <Plus size={14} /> Write your first project rule
            </button>
          ) : (
            <div className="space-y-2">
              {customRules.map((rule, idx) => {
                const isEditing = customRuleEditingIdx === idx;
                return (
                  <div
                    key={idx}
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
                          value={customRuleEditName}
                          onChange={(e) => setCustomRuleEditName(e.target.value)}
                          placeholder="Rule name"
                          className="w-full bg-bg-sidebar border border-border-strong/40 focus:border-brand rounded-md px-3 py-1.5 text-[13px] text-text-base placeholder-text-muted/50 outline-none transition-colors font-medium"
                        />
                        <LineNumberedTextarea
                          value={customRuleEditContent}
                          onChange={setCustomRuleEditContent}
                          placeholder="Write the rule content in Markdown…"
                          variant="inline"
                          rows={8}
                          className="w-full"
                        />
                        <div className="flex items-center justify-end gap-2 pt-1">
                          <button
                            onClick={() => setCustomRuleEditingIdx(null)}
                            className="px-3 py-1 text-[12px] text-text-muted hover:text-text-base transition-colors"
                          >
                            Cancel
                          </button>
                          <button
                            onClick={handleCommitCustomRule}
                            className="flex items-center gap-1 px-3 py-1 bg-brand hover:bg-brand-hover text-white text-[12px] font-medium rounded transition-colors"
                          >
                            <Check size={11} /> Save
                          </button>
                        </div>
                      </div>
                    ) : (
                      <div className="flex items-center gap-3 px-3 py-2.5">
                        <ScrollText size={14} className="flex-shrink-0 text-text-muted" />
                        <div className="flex-1 min-w-0">
                          <div className="text-[13px] font-medium text-text-base truncate">{rule.name || "Untitled Rule"}</div>
                          {rule.content.trim() ? (
                            <div className="text-[11px] text-text-muted truncate mt-0.5">
                              {rule.content.trim().split("\n")[0]}
                            </div>
                          ) : (
                            <div className="text-[11px] text-text-muted/60 italic mt-0.5">Empty — add content to activate</div>
                          )}
                        </div>
                        <TokenPill text={rule.content} />
                        <div className="flex items-center gap-1 flex-shrink-0">
                          <button
                            onClick={() => handleStartEditCustomRule(idx)}
                            className="p-1.5 text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
                            title="Edit"
                          >
                            <Edit2 size={12} />
                          </button>
                          <button
                            onClick={() => handleDeleteCustomRule(idx)}
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

        {/* ── Divider ── */}
        <div className="border-t border-border-strong/30" />

        {/* ── Global Rules ── */}
        <section>
          <div className="flex items-center justify-between mb-3">
            <div className="flex items-center gap-2">
              <ScrollText size={13} className="text-text-muted" />
              <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Global Rules</span>
              {projectRules.length > 0 && (
                <span className="text-[10px] bg-bg-sidebar border border-border-strong/40 rounded-full px-1.5 py-0.5 text-text-muted leading-none">
                  {projectRules.length}
                </span>
              )}
            </div>
            <div className="relative">
              <button
                onClick={() => setGlobalRuleAdding(!globalRuleAdding)}
                className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium"
              >
                <Plus size={12} /> Add from Library
              </button>
              {globalRuleAdding && (
                <div className="absolute right-0 top-full mt-1 w-72 bg-bg-sidebar border border-border-strong rounded-lg shadow-xl z-50 max-h-72 overflow-y-auto">
                  <div className="p-2 border-b border-border-strong/40">
                    <input
                      type="text"
                      value={globalRuleSearch}
                      onChange={(e) => setGlobalRuleSearch(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Escape") { setGlobalRuleAdding(false); setGlobalRuleSearch(""); }
                        if (e.key === "Enter" && filteredRules.length === 1) {
                          handleToggleProjectRule(filteredRules[0]!.id);
                          setGlobalRuleAdding(false);
                          setGlobalRuleSearch("");
                        }
                      }}
                      placeholder="Search rules..."
                      autoFocus
                      className="w-full bg-bg-input border border-border-strong/40 focus:border-brand rounded px-2 py-1 text-[12px] text-text-base placeholder-text-muted/50 outline-none"
                    />
                  </div>
                  <div className="py-1">
                    {filteredRules.length === 0 ? (
                      <div className="px-3 py-2 text-[12px] text-text-muted italic">
                        {emptyDropdownMessage}
                      </div>
                    ) : (
                      filteredRules.map((r) => (
                        <button
                          key={r.id}
                          onClick={() => {
                            handleToggleProjectRule(r.id);
                            setGlobalRuleAdding(false);
                            setGlobalRuleSearch("");
                          }}
                          className="w-full flex items-center gap-2 px-3 py-2 hover:bg-bg-input text-left transition-colors"
                        >
                          <ScrollText size={14} className="text-text-muted flex-shrink-0" />
                          <div className="min-w-0">
                            <div className="text-[12px] font-medium text-text-base truncate">{r.name}</div>
                            <div className="text-[11px] text-text-muted truncate">{r.id}</div>
                          </div>
                        </button>
                      ))
                    )}
                  </div>
                </div>
              )}
            </div>
          </div>

          {projectRules.length === 0 && !globalRuleAdding && (
            <p className="text-[12px] text-text-muted italic pl-1">No global rules selected.</p>
          )}
          <div className="space-y-2">
            {projectRules.map((ruleId) => {
              const meta = availableRules.find(r => r.id === ruleId);
              return (
                <div
                  key={ruleId}
                  className="bg-bg-input border border-border-strong/40 rounded-lg group flex items-center gap-3 px-3 py-2.5"
                >
                  <ScrollText size={14} className="flex-shrink-0 text-text-muted" />
                  <div className="flex-1 min-w-0">
                    <div className="text-[13px] font-medium text-text-base truncate">
                      {meta?.name ?? ruleId}
                    </div>
                    <div className="text-[11px] text-text-muted truncate">{ruleId}</div>
                  </div>
                  <TokenPill text={globalRuleContentCache[ruleId] ?? ""} />
                  {!isRuleLocked(ruleId) && (
                  <button
                    onClick={() => handleToggleProjectRule(ruleId)}
                    className="p-1.5 text-text-muted hover:text-danger hover:bg-danger/10 rounded transition-colors flex-shrink-0 opacity-0 group-hover:opacity-100"
                    title="Remove"
                  >
                    <X size={12} />
                  </button>
                  )}
                </div>
              );
            })}
          </div>

          {availableRules.length === 0 && (
            <div className="px-4 py-6 bg-bg-input border border-border-strong/40 rounded-lg text-center">
              <ScrollText size={18} className="mx-auto mb-2 text-text-muted" strokeWidth={1.5} />
              <p className="text-[13px] text-text-muted mb-1">No global rules yet.</p>
              <p className="text-[12px] text-text-muted/70">Create reusable rules in the Rules section of the sidebar.</p>
            </div>
          )}
        </section>

        {dirty && (
          <div className="flex justify-end">
            <button
              onClick={handleSave}
              disabled={syncStatus === "syncing"}
              className="flex items-center gap-1.5 px-4 py-2 bg-brand hover:bg-brand-hover text-white text-[13px] font-medium rounded shadow-sm transition-colors disabled:opacity-50"
            >
              <Check size={13} /> {syncStatus === "syncing" ? "Saving…" : "Save Changes"}
            </button>
          </div>
        )}
      </div>

      {/* Help sidebar */}
      <div className="w-52 flex-shrink-0">
        <div className="rounded-md bg-bg-input border border-border-strong/30 px-3 py-2.5 text-[11px] text-text-muted space-y-2.5 sticky top-0">
          <div>
            <p className="font-medium text-text-base text-[12px]">Write rules to separate files</p>
            <p className="leading-relaxed mt-1">
              Instead of embedding rules inline, each rule is saved as its own file under{" "}
              <code className="text-[10px] bg-bg-sidebar px-1 rounded">.automatic/instructions/</code>.
              The instruction file becomes a short index that lists them.
            </p>
          </div>
          <button
            role="switch"
            aria-checked={!!project.instructions_index_mode}
            onClick={() => {
              setProject({ ...project, instructions_index_mode: !project.instructions_index_mode });
              setDirty(true);
            }}
            className={`w-full flex items-center justify-between gap-2 px-2 py-1.5 rounded transition-colors ${
              project.instructions_index_mode ? "bg-brand/10 text-brand" : "bg-bg-sidebar text-text-muted"
            }`}
          >
            <span className="text-[11px] font-medium">{project.instructions_index_mode ? "Enabled" : "Disabled"}</span>
            <span
              className={`relative inline-flex h-4 w-7 items-center rounded-full transition-colors flex-shrink-0 ${
                project.instructions_index_mode ? "bg-brand" : "bg-border-strong/60"
              }`}
            >
              <span
                className={`inline-block h-3 w-3 transform rounded-full bg-white shadow transition-transform ${
                  project.instructions_index_mode ? "translate-x-3.5" : "translate-x-0.5"
                }`}
              />
            </span>
          </button>
        </div>
      </div>
    </div>
  );
}
