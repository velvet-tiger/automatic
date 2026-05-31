// Extracted verbatim from ProjectEditor.tsx (Phase 2E — behavior-preserving).

import { Check, Plus, Webhook, X } from "lucide-react";
import type { AgentInfo, HookEntry, Project } from "../../types";

type ProjectTabId =
  | "summary" | "agents" | "commands" | "hooks" | "custom_agents" | "skills"
  | "mcp_servers" | "groups" | "project_file" | "rules" | "context"
  | "docs_files" | "docs_links" | "docs_notes" | "memory" | "activity"
  | "recommendations" | "tools" | "settings";

interface HooksPanelProps {
  project: Project;
  setProject: (next: Project) => void;
  setDirty: (v: boolean) => void;
  dirty: boolean;
  syncStatus: string | null;
  handleSave: () => void | Promise<void>;
  availableAgents: AgentInfo[];
  availableHooks: HookEntry[];
  hookAdding: boolean;
  setHookAdding: (v: boolean) => void;
  hookSearch: string;
  setHookSearch: (v: string) => void;
  selectTab: (tab: ProjectTabId) => void;
}

export function HooksPanel({
  project, setProject, setDirty, dirty, syncStatus, handleSave,
  availableAgents, availableHooks,
  hookAdding, setHookAdding, hookSearch, setHookSearch,
  selectTab,
}: HooksPanelProps) {
  const attachedHookIds = project.hooks ?? [];

  const projectAgentIds = new Set(project.agents);
  const hookCapableAgentIds = new Set(
    availableAgents
      .filter((a) => a.capabilities?.hooks)
      .map((a) => a.id),
  );
  const projectHasHookCapableAgent = project.agents.some((id) =>
    hookCapableAgentIds.has(id),
  );

  const matchesSearch = (entry: HookEntry) => {
    const q = hookSearch.toLowerCase();
    if (!q) return true;
    return (
      entry.id.toLowerCase().includes(q) ||
      entry.name.toLowerCase().includes(q) ||
      entry.event.toLowerCase().includes(q) ||
      entry.agent.toLowerCase().includes(q)
    );
  };

  const isCompatible = (entry: HookEntry) =>
    projectAgentIds.has(entry.agent) &&
    hookCapableAgentIds.has(entry.agent);

  const pickerCandidates = availableHooks
    .filter((h) => !attachedHookIds.includes(h.id))
    .filter(matchesSearch);

  const compatibleCandidates = pickerCandidates.filter(isCompatible);
  const incompatibleCandidates = pickerCandidates.filter(
    (h) => !isCompatible(h),
  );

  return (
    <div className="space-y-8">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-[15px] font-semibold text-text-base">Hooks</h2>
          <p className="text-[12px] text-text-muted mt-1">
            Lifecycle hooks that run on agent events. Synced
            per-agent into the project's settings on next sync.
          </p>
        </div>
        {attachedHookIds.length > 0 && (
          <span className="text-[11px] text-brand bg-brand/10 px-2 py-0.5 rounded border border-brand/20">
            {attachedHookIds.length} hook{attachedHookIds.length === 1 ? "" : "s"}
          </span>
        )}
      </div>

      {!projectHasHookCapableAgent && (
        <div className="rounded-lg border border-border-strong/40 bg-bg-input px-4 py-3 text-[12px] text-text-muted">
          None of this project's agents support hooks today.
          Add Claude Code or Codex CLI under{" "}
          <button
            onClick={() => selectTab("agents")}
            className="text-brand hover:underline"
          >
            Configuration → Providers
          </button>{" "}
          to enable hook syncing.
        </div>
      )}

      <section>
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-2">
            <div className="p-1 bg-icon-skill/10 rounded">
              <Webhook size={12} className="text-icon-skill" />
            </div>
            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Attached Hooks</span>
            {attachedHookIds.length > 0 && (
              <span className="text-[10px] bg-bg-sidebar border border-border-strong/40 rounded-full px-1.5 py-0.5 text-text-muted leading-none">
                {attachedHookIds.length}
              </span>
            )}
          </div>
          <div className="relative">
            <button
              onClick={() => setHookAdding(!hookAdding)}
              className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium"
            >
              <Plus size={12} /> Add from Library
            </button>
            {hookAdding && (
              <div className="absolute right-0 top-full mt-1 w-80 bg-bg-sidebar border border-border-strong rounded-lg shadow-xl z-50 max-h-80 overflow-y-auto">
                <div className="p-2 border-b border-border-strong/40">
                  <input
                    type="text"
                    value={hookSearch}
                    onChange={(e) => setHookSearch(e.target.value)}
                    placeholder="Search hooks..."
                    className="w-full bg-bg-input border border-border-strong/40 focus:border-brand rounded px-2 py-1 text-[12px] text-text-base placeholder-text-muted/50 outline-none"
                    autoFocus
                  />
                </div>
                <div className="py-1">
                  {availableHooks.length === 0 ? (
                    <div className="px-3 py-2 text-[12px] text-text-muted italic">
                      No hooks in the library yet.
                    </div>
                  ) : pickerCandidates.length === 0 ? (
                    <div className="px-3 py-2 text-[12px] text-text-muted italic">
                      All hooks already attached.
                    </div>
                  ) : (
                    <>
                      {compatibleCandidates.map((hook) => (
                        <button
                          key={hook.id}
                          onClick={() => {
                            setProject({
                              ...project,
                              hooks: [...attachedHookIds, hook.id],
                            });
                            setDirty(true);
                            setHookAdding(false);
                            setHookSearch("");
                          }}
                          className="w-full flex items-center gap-2 px-3 py-2 hover:bg-bg-input text-left transition-colors"
                        >
                          <Webhook size={14} className="text-text-muted flex-shrink-0" />
                          <div className="min-w-0 flex-1">
                            <div className="text-[12px] font-medium text-text-base truncate">
                              {hook.name}
                            </div>
                            <div className="text-[11px] text-text-muted truncate">
                              {hook.agent} · {hook.event}
                            </div>
                          </div>
                        </button>
                      ))}
                      {incompatibleCandidates.length > 0 && (
                        <>
                          <div className="px-3 pt-2 pb-1 text-[10px] uppercase tracking-wider text-text-muted/70">
                            Not compatible with this project
                          </div>
                          {incompatibleCandidates.map((hook) => {
                            const reason = !projectAgentIds.has(hook.agent)
                              ? `${hook.agent} not in this project`
                              : "agent does not support hooks";
                            return (
                              <button
                                key={hook.id}
                                onClick={() => {
                                  setProject({
                                    ...project,
                                    hooks: [...attachedHookIds, hook.id],
                                  });
                                  setDirty(true);
                                  setHookAdding(false);
                                  setHookSearch("");
                                }}
                                className="w-full flex items-center gap-2 px-3 py-2 hover:bg-bg-input text-left transition-colors opacity-60"
                                title={reason}
                              >
                                <Webhook size={14} className="text-text-muted flex-shrink-0" />
                                <div className="min-w-0 flex-1">
                                  <div className="text-[12px] font-medium text-text-base truncate">
                                    {hook.name}
                                  </div>
                                  <div className="text-[11px] text-text-muted truncate">
                                    {hook.agent} · {hook.event} — {reason}
                                  </div>
                                </div>
                              </button>
                            );
                          })}
                        </>
                      )}
                    </>
                  )}
                </div>
              </div>
            )}
          </div>
        </div>

        {attachedHookIds.length === 0 ? (
          <div className="text-[12px] text-text-muted/60 italic py-4 text-center">
            No hooks attached. Add hooks from your library to
            run scripts on agent lifecycle events for this project.
          </div>
        ) : (
          <div className="space-y-2">
            {attachedHookIds.map((hookId) => {
              const hook = availableHooks.find((h) => h.id === hookId);
              const missing = !hook;
              const incompatible = !!hook && !isCompatible(hook);
              return (
                <div
                  key={hookId}
                  className="bg-bg-input border border-border-strong/40 rounded-lg group hover:border-border-strong transition-colors"
                >
                  <div className="flex items-center gap-3 px-3 py-2.5">
                    <Webhook size={14} className="flex-shrink-0 text-text-muted" />
                    <div className="flex-1 min-w-0">
                      <div className="text-[13px] font-medium text-text-base truncate flex items-center gap-2">
                        {hook?.name ?? hookId}
                        {missing && (
                          <span className="text-[10px] text-warning bg-warning/10 border border-warning/30 rounded px-1.5 py-0.5">
                            Missing from library
                          </span>
                        )}
                        {!missing && incompatible && (
                          <span className="text-[10px] text-text-muted bg-bg-sidebar border border-border-strong/40 rounded px-1.5 py-0.5">
                            Skipped on sync
                          </span>
                        )}
                      </div>
                      <div className="text-[11px] text-text-muted truncate">
                        {hook
                          ? `${hook.agent} · ${hook.event}`
                          : "Hook was deleted or never existed — remove this entry."}
                      </div>
                    </div>
                    <button
                      onClick={() => {
                        const updated = attachedHookIds.filter((id) => id !== hookId);
                        setProject({
                          ...project,
                          hooks: updated.length > 0 ? updated : undefined,
                        });
                        setDirty(true);
                      }}
                      className="p-1.5 text-text-muted hover:text-danger hover:bg-danger/10 rounded transition-colors flex-shrink-0 opacity-0 group-hover:opacity-100"
                      title="Remove"
                    >
                      <X size={12} />
                    </button>
                  </div>
                </div>
              );
            })}
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
            <Check size={13} /> {syncStatus === "syncing" ? "Saving..." : "Save Changes"}
          </button>
        </div>
      )}
    </div>
  );
}
