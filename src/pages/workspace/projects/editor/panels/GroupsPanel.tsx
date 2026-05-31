// Extracted verbatim from ProjectEditor.tsx (Phase 2E — behavior-preserving).

import { ExternalLink, Layers, Plus, RefreshCw, Trash2, X } from "lucide-react";

interface GroupsPanelProps {
  projectName: string;
  projectGroupMemberships: string[];
  allGroups: string[];
  loadingGroups: boolean;
  reloadGroups: (projectName: string) => Promise<void>;
  onAddToGroup: (groupName: string, projectName: string) => Promise<void> | void;
  onRemoveFromGroup: (groupName: string, projectName: string) => Promise<void> | void;
  onRemoveFromAllGroups: (projectName: string) => Promise<void> | void;
  onNavigateToGroup?: (groupName: string) => void;
}

export function GroupsPanel({
  projectName,
  projectGroupMemberships,
  allGroups,
  loadingGroups,
  reloadGroups,
  onAddToGroup,
  onRemoveFromGroup,
  onRemoveFromAllGroups,
  onNavigateToGroup,
}: GroupsPanelProps) {
  return (
    <section className="flex gap-6">
      {/* Main content */}
      <div className="flex-1 min-w-0 space-y-4">
        {/* Header */}
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Layers size={13} className="text-text-muted" />
            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Project Groups</span>
            {projectGroupMemberships.length > 0 && (
              <span className="text-[10px] font-semibold px-1.5 py-0.5 rounded-full bg-bg-sidebar text-text-muted border border-border-strong/30 leading-none">
                {projectGroupMemberships.length}
              </span>
            )}
          </div>
          <button
            onClick={() => reloadGroups(projectName)}
            disabled={loadingGroups}
            className="text-[11px] text-text-muted hover:text-text-base transition-colors flex items-center gap-1 disabled:opacity-40"
            title="Refresh"
          >
            <RefreshCw size={11} className={loadingGroups ? "animate-spin" : ""} />
            Refresh
          </button>
        </div>

        {/* Current memberships */}
        {loadingGroups ? (
          <div className="bg-bg-input border border-border-strong/40 rounded-lg px-4 py-6 flex items-center gap-2 text-text-muted text-[12px]">
            <RefreshCw size={12} className="animate-spin" />
            Loading groups…
          </div>
        ) : projectGroupMemberships.length === 0 ? (
          <div className="bg-bg-input border border-border-strong/40 rounded-lg px-4 py-8 text-center">
            <Layers size={18} className="text-text-muted/40 mx-auto mb-2" />
            <p className="text-[13px] font-medium text-text-base mb-1">Not in any group</p>
            <p className="text-[12px] text-text-muted">
              Add this project to a group to link it with related projects.
              When synced, agents will see context about all projects in the group.
            </p>
          </div>
        ) : (
          <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
            {projectGroupMemberships.map((groupName) => (
              <div
                key={groupName}
                className="flex items-center gap-3 px-4 py-2.5 hover:bg-surface-hover transition-colors"
              >
                <Layers size={13} className="text-text-muted flex-shrink-0" />
                <span className="flex-1 text-[13px] text-text-base">{groupName}</span>
                <button
                  onClick={() => onNavigateToGroup?.(groupName)}
                  className="flex-shrink-0 px-2 py-0.5 text-[11px] font-medium text-brand hover:text-brand-hover hover:bg-brand/10 rounded transition-colors flex items-center gap-1"
                  title={`View ${groupName} group`}
                >
                  View
                  <ExternalLink size={10} />
                </button>
                <button
                  onClick={() => onRemoveFromGroup(groupName, projectName)}
                  className="flex-shrink-0 p-1 text-text-muted hover:text-red-400 transition-colors"
                  title={`Remove from "${groupName}"`}
                >
                  <X size={12} />
                </button>
              </div>
            ))}
          </div>
        )}

        {/* Remove from all groups button */}
        {projectGroupMemberships.length > 1 && (
          <button
            onClick={() => onRemoveFromAllGroups(projectName)}
            className="w-full flex items-center justify-center gap-2 px-4 py-2 text-[12px] text-red-400 hover:text-red-300 hover:bg-red-500/10 rounded-lg transition-colors"
          >
            <Trash2 size={12} />
            Remove from all groups
          </button>
        )}

        {/* Add to a group picker */}
        {(() => {
          const available = allGroups.filter((g) => !projectGroupMemberships.includes(g));
          if (available.length === 0 && allGroups.length > 0 && !loadingGroups) return null;
          return (
            <div>
              <p className="text-[11px] font-semibold text-text-muted uppercase tracking-wider mb-2">
                {allGroups.length === 0 ? "No groups exist yet" : "Add to group"}
              </p>
              {allGroups.length === 0 ? (
                <p className="text-[12px] text-text-muted">
                  Create groups from the <strong>Groups</strong> section in the sidebar to start linking related projects.
                </p>
              ) : available.length > 0 ? (
                <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
                  {available.map((groupName) => (
                    <button
                      key={groupName}
                      onClick={() => onAddToGroup(groupName, projectName)}
                      className="w-full flex items-center gap-3 px-4 py-2.5 text-left hover:bg-surface-hover transition-colors"
                    >
                      <Plus size={12} className="text-text-muted flex-shrink-0" />
                      <span className="flex-1 text-[13px] text-text-muted">{groupName}</span>
                    </button>
                  ))}
                </div>
              ) : null}
            </div>
          );
        })()}
      </div>

      {/* Help sidebar */}
      <div className="w-52 flex-shrink-0">
        <div className="rounded-md bg-bg-input border border-border-strong/30 px-3 py-2.5 text-[11px] text-text-muted space-y-1.5 sticky top-0">
          <p className="font-medium text-text-base text-[12px]">How groups work</p>
          <p className="leading-relaxed">
            When this project is synced, Automatic injects a context block into its agent instruction
            files listing all related projects — with their descriptions and relative paths — so
            agents can recognise and navigate between them.
          </p>
        </div>
      </div>
    </section>
  );
}
