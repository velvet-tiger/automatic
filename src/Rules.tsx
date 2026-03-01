import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { Plus, X, Edit2, FileText, Check, ScrollText, RefreshCw, FolderGit2 } from "lucide-react";
import { ICONS } from "./icons";
import { AuthorSection } from "./AuthorPanel";

interface RuleEntry {
  id: string;
  name: string;
}

interface Rule {
  name: string;
  content: string;
}

interface RuleProjectStatus {
  name: string;
  synced: boolean;
}

// Per-project sync state.
// "needs-sync" = rule has changed, project not yet updated (shown yellow).
// "syncing"    = update in progress.
// "synced"     = project is up to date with the current rule content (green).
// "error"      = last sync attempt failed.
type SyncState = "needs-sync" | "syncing" | "synced" | "error";

export default function Rules() {
  const [rules, setRules] = useState<RuleEntry[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [displayName, setDisplayName] = useState("");
  const [ruleContent, setRuleContent] = useState("");
  const [isEditing, setIsEditing] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  const [newMachineName, setNewMachineName] = useState("");
  const [newDisplayName, setNewDisplayName] = useState("");
  const [error, setError] = useState<string | null>(null);

  // Projects referencing this rule
  const [referencingProjects, setReferencingProjects] = useState<string[]>([]);
  const [projectSyncState, setProjectSyncState] = useState<Record<string, SyncState>>({});
  const [syncAllState, setSyncAllState] = useState<SyncState>("needs-sync");

  useEffect(() => {
    loadRules();
  }, []);

  const loadRules = async () => {
    try {
      const result: RuleEntry[] = await invoke("get_rules");
      setRules(result.sort((a, b) => a.name.localeCompare(b.name)));
      setError(null);
    } catch (err: any) {
      setError(`Failed to load rules: ${err}`);
    }
  };

  // Load referencing projects with their actual on-disk sync status.
  const loadReferencingProjects = async (id: string) => {
    try {
      const statuses: RuleProjectStatus[] = await invoke("get_projects_referencing_rule", { ruleName: id });
      const sorted = statuses.sort((a, b) => a.name.localeCompare(b.name));
      setReferencingProjects(sorted.map(s => s.name));
      const initial: Record<string, SyncState> = {};
      for (const s of sorted) initial[s.name] = s.synced ? "synced" : "needs-sync";
      setProjectSyncState(initial);
      // Aggregate: all synced → "synced", otherwise "needs-sync".
      const allSynced = sorted.length > 0 && sorted.every(s => s.synced);
      setSyncAllState(allSynced ? "synced" : "needs-sync");
    } catch (err: any) {
      // Non-fatal — the usage panel just won't show.
      console.error("Failed to load referencing projects:", err);
      setReferencingProjects([]);
    }
  };

  const loadRule = async (id: string) => {
    try {
      const raw: string = await invoke("read_rule", { machineName: id });
      const rule: Rule = JSON.parse(raw);
      setSelectedId(id);
      setDisplayName(rule.name);
      setRuleContent(rule.content);
      setIsEditing(false);
      setIsCreating(false);
      setError(null);
      await loadReferencingProjects(id);
    } catch (err: any) {
      setError(`Failed to read rule: ${err}`);
    }
  };

  // Mark all referencing projects as needing a sync (called after saving the rule).
  const markAllNeedsSync = () => {
    setProjectSyncState(prev => {
      const next: Record<string, SyncState> = {};
      for (const p of Object.keys(prev)) next[p] = "needs-sync";
      return next;
    });
    setSyncAllState("needs-sync");
  };

  const handleSave = async () => {
    if (isCreating) {
      const id = newMachineName.trim();
      const name = newDisplayName.trim();
      if (!id || !name) return;
      try {
        await invoke("save_rule", { machineName: id, name, content: ruleContent });
        // Insert into the sidebar list in-place (sorted), then select — no
        // loadRules() call so there is no async gap that could lose selection.
        const newEntry: RuleEntry = { id, name };
        setRules(prev =>
          [...prev.filter(r => r.id !== id), newEntry].sort((a, b) =>
            a.name.localeCompare(b.name)
          )
        );
        setIsCreating(false);
        setIsEditing(false);
        setSelectedId(id);
        setDisplayName(name);
        setReferencingProjects([]);
        setProjectSyncState({});
        setSyncAllState("needs-sync");
        setError(null);
      } catch (err: any) {
        setError(`Failed to save rule: ${err}`);
      }
    } else if (selectedId) {
      try {
        await invoke("save_rule", { machineName: selectedId, name: displayName, content: ruleContent });
        setIsEditing(false);
        // Update sidebar entry in-place — no loadRules so selection is preserved.
        setRules(prev =>
          prev
            .map(r => (r.id === selectedId ? { ...r, name: displayName } : r))
            .sort((a, b) => a.name.localeCompare(b.name))
        );
        // Rule content changed — all referencing projects need re-syncing.
        markAllNeedsSync();
        setError(null);
      } catch (err: any) {
        setError(`Failed to save rule: ${err}`);
      }
    }
  };

  const handleDelete = async (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    const confirmed = await ask(`Delete rule "${id}"?`, { title: "Delete Rule", kind: "warning" });
    if (!confirmed) return;
    try {
      await invoke("delete_rule", { machineName: id });
      if (selectedId === id) {
        setSelectedId(null);
        setDisplayName("");
        setRuleContent("");
        setIsEditing(false);
        setReferencingProjects([]);
        setProjectSyncState({});
        setSyncAllState("needs-sync");
      }
      await loadRules();
      setError(null);
    } catch (err: any) {
      setError(`Failed to delete rule: ${err}`);
    }
  };

  const startCreateNew = () => {
    setSelectedId(null);
    setDisplayName("");
    setRuleContent("");
    setIsCreating(true);
    setIsEditing(true);
    setNewMachineName("");
    setNewDisplayName("");
    setReferencingProjects([]);
    setProjectSyncState({});
    setSyncAllState("needs-sync");
  };

  const handleSyncProject = async (projectName: string) => {
    if (!selectedId) return;
    setProjectSyncState(prev => ({ ...prev, [projectName]: "syncing" }));
    try {
      await invoke("sync_rule_to_project", { ruleName: selectedId, projectName });
      setProjectSyncState(prev => ({ ...prev, [projectName]: "synced" }));
      // Recalculate aggregate state.
      setSyncAllState(prev => {
        if (prev === "syncing") return "syncing";
        // Check if all are now synced.
        return "synced";
      });
    } catch (err: any) {
      setProjectSyncState(prev => ({ ...prev, [projectName]: "error" }));
      setError(`Failed to sync rule to project "${projectName}": ${err}`);
    }
  };

  const handleSyncAll = async () => {
    if (!selectedId || referencingProjects.length === 0) return;
    setSyncAllState("syncing");
    const initialStates: Record<string, SyncState> = {};
    for (const p of referencingProjects) initialStates[p] = "syncing";
    setProjectSyncState(initialStates);

    let hadError = false;
    for (const projectName of referencingProjects) {
      try {
        await invoke("sync_rule_to_project", { ruleName: selectedId, projectName });
        setProjectSyncState(prev => ({ ...prev, [projectName]: "synced" }));
      } catch (err: any) {
        setProjectSyncState(prev => ({ ...prev, [projectName]: "error" }));
        hadError = true;
        setError(`Failed to sync rule to project "${projectName}": ${err}`);
      }
    }
    setSyncAllState(hadError ? "error" : "synced");
  };

  const selectedEntry = rules.find(r => r.id === selectedId);

  return (
    <div className="flex h-full w-full bg-bg-base">
      {/* Left Sidebar - Rule List */}
      <div className="w-64 flex-shrink-0 flex flex-col border-r border-border-strong/40 bg-bg-input/50">
        <div className="h-11 px-4 border-b border-border-strong/40 flex justify-between items-center bg-bg-base/30">
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Rules</span>
          <button
            onClick={startCreateNew}
            className="text-text-muted hover:text-text-base transition-colors p-1 hover:bg-bg-sidebar rounded"
            title="Create New Rule"
          >
            <Plus size={14} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto py-2 custom-scrollbar">
          {rules.length === 0 && !isCreating ? (
            <div className="px-4 py-3 text-[13px] text-text-muted text-center">No rules yet.</div>
          ) : (
            <ul className="space-y-1 px-2">
              {isCreating && (
                <li className="flex items-center gap-3 px-3 py-2.5 rounded-lg bg-bg-sidebar">
                  <div className={ICONS.rule.iconBox}>
                    <ScrollText size={15} className={ICONS.rule.iconColor} />
                  </div>
                  <span className="text-[13px] text-text-base italic">New Rule...</span>
                </li>
              )}
              {rules.map(entry => {
                const isActive = selectedId === entry.id && !isCreating;
                return (
                  <li key={entry.id} className="group relative">
                    <button
                      onClick={() => loadRule(entry.id)}
                      className={`w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-left transition-colors ${
                        isActive
                          ? "bg-bg-sidebar text-text-base"
                          : "text-text-muted hover:bg-bg-sidebar/60 hover:text-text-base"
                      }`}
                    >
                      <div className={ICONS.rule.iconBox}>
                        <ScrollText size={15} className={ICONS.rule.iconColor} />
                      </div>
                      <div className="flex-1 min-w-0">
                        <div className={`text-[13px] font-medium truncate ${isActive ? "text-text-base" : "text-text-base"}`}>
                          {entry.name}
                        </div>
                        <div className="text-[10px] text-text-muted truncate">{entry.id}</div>
                      </div>
                    </button>
                    <button
                      onClick={(e) => handleDelete(entry.id, e)}
                      className="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-text-muted hover:text-danger opacity-0 group-hover:opacity-100 hover:bg-surface rounded transition-all"
                      title="Delete Rule"
                    >
                      <X size={12} />
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </div>

      {/* Right Area - Editor/Viewer */}
      <div className="flex-1 flex flex-col min-w-0 bg-bg-base">
        {error && (
          <div className="bg-red-500/10 text-red-400 p-3 text-[13px] border-b border-red-500/20 flex items-center justify-between">
            {error}
            <button onClick={() => setError(null)}><X size={14} /></button>
          </div>
        )}

        {(selectedId || isCreating) ? (
          <div className="flex-1 flex flex-col h-full min-h-0">
            {/* Header */}
            <div className="min-h-[44px] px-6 border-b border-border-strong/40 flex justify-between items-center gap-4 py-2 flex-shrink-0">
              <div className="flex items-center gap-3 min-w-0 flex-1">
                <FileText size={14} className={ICONS.rule.iconColor + " flex-shrink-0"} />
                {isCreating ? (
                  <div className="flex flex-col gap-1.5 min-w-0">
                    <input
                      type="text"
                      placeholder="Display Name"
                      value={newDisplayName}
                      onChange={(e) => setNewDisplayName(e.target.value)}
                      autoFocus
                      className="bg-transparent border-none outline-none text-[14px] font-medium text-text-base placeholder-text-muted/50 w-72"
                    />
                    <div className="flex items-center gap-2">
                      <input
                        type="text"
                        placeholder="machine-name (lowercase, hyphens)"
                        value={newMachineName}
                        onChange={(e) => setNewMachineName(e.target.value.toLowerCase().replace(/[^a-z0-9-]/g, ''))}
                        className="bg-transparent border-none outline-none text-[11px] text-text-muted placeholder-text-muted/40 font-mono w-72"
                      />
                    </div>
                  </div>
                ) : isEditing ? (
                  <div className="flex flex-col gap-0.5 min-w-0">
                    <input
                      type="text"
                      value={displayName}
                      onChange={(e) => setDisplayName(e.target.value)}
                      className="bg-transparent border-none outline-none text-[14px] font-medium text-text-base placeholder-text-muted/50 w-72"
                      placeholder="Display Name"
                    />
                    <span className="text-[10px] text-text-muted font-mono">{selectedId}</span>
                  </div>
                ) : (
                  <div className="flex flex-col gap-0.5 min-w-0">
                    <h3 className="text-[14px] font-medium text-text-base truncate">{selectedEntry?.name || displayName}</h3>
                    <span className="text-[10px] text-text-muted font-mono">{selectedId}</span>
                  </div>
                )}
              </div>

              <div className="flex items-center gap-2 flex-shrink-0">
                {!isEditing ? (
                  <button
                    onClick={() => setIsEditing(true)}
                    className="flex items-center gap-1.5 px-3 py-1.5 hover:bg-bg-sidebar text-text-muted hover:text-text-base rounded text-[12px] font-medium transition-colors"
                  >
                    <Edit2 size={12} /> Edit
                  </button>
                ) : (
                  <>
                    {!isCreating && (
                      <button
                        onClick={() => {
                          setIsEditing(false);
                          if (selectedId) loadRule(selectedId);
                        }}
                        className="px-3 py-1.5 hover:bg-bg-sidebar text-text-muted hover:text-text-base rounded text-[12px] font-medium transition-colors"
                      >
                        Cancel
                      </button>
                    )}
                    <button
                      onClick={handleSave}
                      disabled={isCreating ? (!newMachineName.trim() || !newDisplayName.trim()) : false}
                      className="flex items-center gap-1.5 px-3 py-1.5 bg-brand hover:bg-brand-hover text-white rounded text-[12px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-sm"
                    >
                      <Check size={12} /> Save
                    </button>
                  </>
                )}
              </div>
            </div>

            {/* Editor Body — flex column so the projects panel is always pinned at the bottom */}
            <div className="flex-1 min-h-0 flex flex-col">
              {isEditing ? (
                <textarea
                  value={ruleContent}
                  onChange={(e) => setRuleContent(e.target.value)}
                  className="flex-1 w-full p-6 resize-none outline-none font-mono text-[13px] bg-bg-base text-text-base leading-relaxed custom-scrollbar placeholder-text-muted/30"
                  placeholder="Write your rule content here in Markdown. Rules are reusable content blocks that can be appended to project instruction files..."
                  spellCheck={false}
                />
              ) : (
                <>
                  {/* Scrollable content area */}
                  <div className="flex-1 min-h-0 overflow-y-auto custom-scrollbar">
                    {/* Author section */}
                    <div className="px-6 pt-4 pb-3 border-b border-border-strong/40">
                      <AuthorSection descriptor={{ type: "local" }} />
                    </div>
                    <div className="p-6 font-mono text-[13px] whitespace-pre-wrap text-text-base leading-relaxed">
                      {ruleContent || <span className="text-text-muted italic">This rule is empty. Click edit to add content.</span>}
                    </div>
                  </div>

                  {/* Used by projects panel — pinned at bottom, always visible */}
                  {!isCreating && referencingProjects.length > 0 && (
                    <div className="flex-shrink-0 border-t border-border-strong/40 px-6 py-4 bg-bg-input/30">
                      <div className="flex items-center justify-between mb-3">
                        <div className="flex items-center gap-2">
                          <FolderGit2 size={13} className="text-text-muted" />
                          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
                            Used in {referencingProjects.length} {referencingProjects.length === 1 ? "project" : "projects"}
                          </span>
                        </div>
                        {syncAllState === "synced" ? (
                          <span className="flex items-center gap-1.5 px-3 py-1 text-[11px] font-medium text-success">
                            In sync
                          </span>
                        ) : (
                          <button
                            onClick={handleSyncAll}
                            disabled={syncAllState === "syncing"}
                            className={`flex items-center gap-1.5 px-3 py-1 rounded text-[11px] font-medium transition-colors ${
                              syncAllState === "error"
                                ? "text-danger bg-danger/10"
                                : syncAllState === "needs-sync"
                                ? "text-warning bg-warning/10 hover:bg-warning/20"
                                : "text-text-muted hover:text-text-base hover:bg-bg-sidebar"
                            } disabled:opacity-50 disabled:cursor-not-allowed`}
                            title="Push this rule's latest content to all referencing projects"
                          >
                            <RefreshCw size={11} className={syncAllState === "syncing" ? "animate-spin" : ""} />
                            {syncAllState === "error" ? "Some failed" : "Update all"}
                          </button>
                        )}
                      </div>
                      {/* Max 3 rows visible; scrollable if more */}
                      <ul className="space-y-1.5 max-h-[108px] overflow-y-auto custom-scrollbar">
                        {referencingProjects.map(projectName => {
                          const state = projectSyncState[projectName] ?? "needs-sync";
                          return (
                            <li key={projectName} className="flex items-center justify-between gap-3 py-1">
                              <div className="flex items-center gap-2 min-w-0">
                                <div className={`w-1.5 h-1.5 rounded-full flex-shrink-0 ${
                                  state === "synced" ? "bg-success" : state === "error" ? "bg-danger" : "bg-warning"
                                }`} />
                                <span className="text-[13px] text-text-base truncate">{projectName}</span>
                              </div>
                              {state !== "synced" && (
                                <button
                                  onClick={() => handleSyncProject(projectName)}
                                  disabled={state === "syncing"}
                                  className={`flex items-center gap-1.5 px-2.5 py-1 rounded text-[11px] font-medium transition-colors flex-shrink-0 ${
                                    state === "error"
                                      ? "text-danger bg-danger/10"
                                      : state === "needs-sync"
                                      ? "text-warning bg-warning/10 hover:bg-warning/20"
                                      : "text-text-muted hover:text-text-base hover:bg-bg-sidebar"
                                  } disabled:opacity-50 disabled:cursor-not-allowed`}
                                  title={`Push rule to ${projectName}`}
                                >
                                  <RefreshCw size={10} className={state === "syncing" ? "animate-spin" : ""} />
                                  {state === "error" ? "Failed" : "Update"}
                                </button>
                              )}
                            </li>
                          );
                        })}
                      </ul>
                    </div>
                  )}
                </>
              )}
            </div>
          </div>
        ) : (
          <div className="flex-1 flex flex-col items-center justify-center text-center p-8">
            <div className="w-16 h-16 mx-auto mb-6 rounded-2xl bg-icon-rule/12 border border-icon-rule/20 flex items-center justify-center">
              <ScrollText size={24} className={ICONS.rule.iconColor} strokeWidth={1.5} />
            </div>
            <h2 className="text-lg font-medium text-text-base mb-2">No Rule Selected</h2>
            <p className="text-[14px] text-text-muted mb-8 leading-relaxed max-w-sm">
              Rules are reusable content blocks that can be appended to project instruction files. Add rules to share common guidelines across projects.
            </p>
            <button
              onClick={startCreateNew}
              className="px-4 py-2 bg-brand hover:bg-brand-hover text-white text-[13px] font-medium rounded shadow-sm transition-colors"
            >
              Create Rule
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
