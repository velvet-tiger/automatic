import { useState, useCallback, useEffect, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { ChevronDown, ChevronRight, Layers, LayoutGrid, Plus, Trash2 } from "lucide-react";
import { trackProjectDeleted } from "../lib/analytics";
import { resolveGroupDrop, UNGROUPED_DROP_TARGET } from "../lib/groupDrop";
import {
  loadProjectSummaries,
  membersWithoutProject,
  projectKeyOf,
  projectLabelFor,
  projectLabels,
  projectLabelText,
  projectNameForKey,
} from "../lib/projectIdentity";
import { ProjectNameLabel } from "./ProjectNameLabel";
import type { ProjectSummary } from "../pages/workspace/projects/types";

// ── Types ────────────────────────────────────────────────────────────────────

interface ProjectGroup {
  name: string;
  description: string;
  projects: string[];
  created_at: string;
  updated_at: string;
}

interface SidebarGroup {
  name: string;
  /** Member project local_keys, as `read_group` returns them. */
  projects: string[];
}

interface WorkspaceSidebarProps {
  activeTab: string;
  onTabClick: (id: string) => void;
  onNavigateToProject: (projectKey: string) => void;
  /** Currently active group filter for the Projects page. */
  activeGroupFilter: string | null;
  /** Called when a group name is clicked — filters the Projects page to that group. */
  onFilterByGroup: (groupName: string | null) => void;
  /** local_key of the project currently open in the editor, if any. */
  activeProjectKey: string | null;
}

/** Label for the key that adds a dragged project to a group without moving it. */
const ADD_MODIFIER_LABEL = navigator.userAgent.includes("Mac") ? "Option" : "Alt";

// ── NavItem (matches App.tsx pattern) ────────────────────────────────────────

function NavItem({ id, icon: Icon, label, isActive, onClick }: {
  id: string;
  icon: React.ComponentType<{ size?: number; className?: string }>;
  label: string;
  isActive: boolean;
  onClick: (id: string) => void;
}) {
  return (
    <button
      onClick={() => onClick(id)}
      className={`w-full flex items-center gap-2.5 px-3 py-1.5 rounded-md text-[13px] font-medium transition-colors ${
        isActive
          ? "bg-bg-sidebar text-text-base"
          : "text-text-muted hover:bg-bg-sidebar hover:text-text-base"
      }`}
    >
      <Icon size={14} className={`shrink-0 ${isActive ? "text-text-base" : "text-text-muted"}`} />
      <span className="flex-1 text-left">{label}</span>
    </button>
  );
}

// ── WorkspaceSidebar ─────────────────────────────────────────────────────────

export default function WorkspaceSidebar({ activeTab, onTabClick, onNavigateToProject, activeGroupFilter, onFilterByGroup, activeProjectKey }: WorkspaceSidebarProps) {
  // ── Project + group data ─────────────────────────────────────────────────
  /** Project local_keys, sorted by display name. */
  const [projects, setProjects] = useState<string[]>([]);
  const [summaries, setSummaries] = useState<ProjectSummary[]>([]);
  const [groups, setGroups] = useState<SidebarGroup[]>([]);
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(() => {
    try {
      const saved = localStorage.getItem("automatic.collapsedGroups");
      return saved ? new Set(JSON.parse(saved)) : new Set();
    } catch {
      return new Set();
    }
  });

  // ── Group creation state ─────────────────────────────────────────────────
  const [creatingGroup, setCreatingGroup] = useState(false);
  const [newGroupName, setNewGroupName] = useState("");
  const newGroupInputRef = useRef<HTMLInputElement>(null);

  // ── Drag state ───────────────────────────────────────────────────────────
  const [dragGhost, setDragGhost] = useState<{ name: string; x: number; y: number; fromGroup: boolean; adding: boolean } | null>(null);
  const [dragOverGroup, setDragOverGroup] = useState<string | null>(null);

  // ── Load data ────────────────────────────────────────────────────────────
  const loadData = useCallback(async () => {
    try {
      const loadedSummaries = await loadProjectSummaries();
      const sorted = [...loadedSummaries].sort((a, b) => a.name.localeCompare(b.name));
      const sortedKeys = sorted.map(projectKeyOf);
      setSummaries(sorted);
      setProjects(sortedKeys);

      const groupNames: string[] = await invoke("list_groups");
      const loaded: SidebarGroup[] = [];
      for (const name of groupNames.sort((a, b) => a.localeCompare(b))) {
        try {
          const raw: string = await invoke("read_group", { name });
          const g = JSON.parse(raw);
          // Members are local_keys. A value no registered project has (a
          // deleted project) has no row to show, so it is left out here;
          // the group file keeps it until the group is edited elsewhere.
          const known = new Set(sortedKeys);
          loaded.push({
            name: g.name,
            projects: ((g.projects ?? []) as string[]).filter((member) => known.has(member)),
          });
        } catch {
          // Skip unreadable groups
        }
      }
      setGroups(loaded);
    } catch {
      // Non-fatal
    }
  }, []);

  useEffect(() => { loadData(); }, [loadData]);

  // Reload when any component signals a group change
  useEffect(() => {
    const handler = () => { loadData(); };
    window.addEventListener("groups-updated", handler);
    window.addEventListener("project-removed", handler);
    window.addEventListener("project-added", handler);
    return () => {
      window.removeEventListener("groups-updated", handler);
      window.removeEventListener("project-removed", handler);
      window.removeEventListener("project-added", handler);
    };
  }, [loadData]);

  /** Notify other components that groups changed. */
  const emitGroupsUpdated = () => {
    window.dispatchEvent(new CustomEvent("groups-updated"));
  };

  const emitProjectRemoved = (name: string, localKey: string) => {
    window.dispatchEvent(new CustomEvent("project-removed", { detail: { name, local_key: localKey } }));
  };

  /** Display name for a project key. */
  const nameForKey = (key: string): string => projectNameForKey(summaries, key) ?? key;
  /** Name plus a folder hint where names repeat. */
  const labels = useMemo(() => projectLabels(summaries), [summaries]);
  const labelForKey = (key: string) => projectLabelFor(labels, key, nameForKey(key));

  // ── Derived data ─────────────────────────────────────────────────────────
  const groupedProjectNames = new Set(groups.flatMap((g) => g.projects));
  const ungroupedProjects = projects.filter((n) => !groupedProjectNames.has(n));

  // ── Group collapse ───────────────────────────────────────────────────────
  const toggleGroupCollapse = (groupName: string) => {
    setCollapsedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(groupName)) next.delete(groupName);
      else next.add(groupName);
      localStorage.setItem("automatic.collapsedGroups", JSON.stringify([...next]));
      return next;
    });
  };

  // ── Group CRUD ───────────────────────────────────────────────────────────
  const handleCreateGroup = async () => {
    const name = newGroupName.trim();
    if (!name) { setCreatingGroup(false); return; }
    if (!/^[a-zA-Z0-9][a-zA-Z0-9 _-]*$/.test(name)) { return; }
    try {
      const now = new Date().toISOString();
      const newGroup: ProjectGroup = { name, description: "", projects: [], created_at: now, updated_at: now };
      await invoke("save_group", { name, data: JSON.stringify(newGroup) });
      setCreatingGroup(false);
      setNewGroupName("");
      await loadData();
      emitGroupsUpdated();
    } catch (err) {
      console.error("Failed to create group:", err);
    }
  };

  useEffect(() => {
    if (creatingGroup && newGroupInputRef.current) {
      newGroupInputRef.current.focus();
    }
  }, [creatingGroup]);

  // ── Drag: add project to group ───────────────────────────────────────────
  const addProjectToGroup = async (projectKey: string, groupName: string) => {
    try {
      const raw: string = await invoke("read_group", { name: groupName });
      const g: ProjectGroup = JSON.parse(raw);
      if (!g.projects.includes(projectKey)) {
        g.projects.push(projectKey);
        g.updated_at = new Date().toISOString();
        await invoke("save_group", { name: groupName, data: JSON.stringify(g) });
        for (const name of g.projects) {
          invoke("sync_project", { name }).catch(() => {});
        }
      }
    } catch (err) {
      console.error("Failed to add to group:", err);
    }
  };

  const removeProjectFromGroup = async (projectKey: string, groupName: string) => {
    try {
      const raw: string = await invoke("read_group", { name: groupName });
      const g: ProjectGroup = JSON.parse(raw);
      const before = g.projects;
      g.projects = membersWithoutProject(before, projectKey, summaries);
      g.updated_at = new Date().toISOString();
      await invoke("save_group", { name: groupName, data: JSON.stringify(g) });
      // Every member is a local_key, including the checkouts just removed.
      const toSync = [...before];
      for (const name of toSync) {
        invoke("sync_project", { name }).catch(() => {});
      }
    } catch (err) {
      console.error("Failed to remove from group:", err);
    }
  };

  const removeProjectFromAllGroups = async (projectKey: string) => {
    for (const group of groups) {
      if (group.projects.includes(projectKey)) {
        await removeProjectFromGroup(projectKey, group.name);
      }
    }
  };

  const handleRemoveProject = async (projectKey: string, event: React.MouseEvent<HTMLButtonElement>) => {
    event.preventDefault();
    event.stopPropagation();
    const projectName = nameForKey(projectKey);

    const confirmed = await ask(
      `Remove project "${projectLabelText(labelForKey(projectKey))}" from Automatic?\n\n(This only removes the project from this app. Your actual project files will NOT be deleted.)`,
      { title: "Remove Project", kind: "warning" },
    );
    if (!confirmed) return;

    try {
      await invoke("delete_project", { name: projectKey });
      trackProjectDeleted(projectName);
      await loadData();
      emitGroupsUpdated();
      emitProjectRemoved(projectName, projectKey);
    } catch (error) {
      console.error("Failed to remove project:", error);
    }
  };

  // ── Drag handlers ────────────────────────────────────────────────────────
  const handleDragStart = (projectKey: string, sourceGroup: string | null, e: React.PointerEvent) => {
    e.preventDefault();
    const projectName = projectLabelText(labelForKey(projectKey));
    const startX = e.clientX;
    const startY = e.clientY;

    const fromGroup = sourceGroup !== null;
    setDragGhost({ name: projectName, x: startX, y: startY, fromGroup, adding: e.altKey });

    const onMove = (ev: PointerEvent) => {
      setDragGhost({ name: projectName, x: ev.clientX, y: ev.clientY, fromGroup, adding: ev.altKey });

      // Resolve drop target from DOM
      const el = document.elementFromPoint(ev.clientX, ev.clientY);
      if (!el) { setDragOverGroup(null); return; }

      let current: Element | null = el;
      while (current) {
        const gn = current.getAttribute("data-sidebar-group");
        if (gn != null) {
          setDragOverGroup(gn);
          return;
        }
        current = current.parentElement;
      }
      setDragOverGroup(null);
    };

    const onUp = async (ev: PointerEvent) => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      setDragGhost(null);

      // Resolve final drop target
      const el = document.elementFromPoint(ev.clientX, ev.clientY);
      let targetGroup: string | null = null;
      if (el) {
        let current: Element | null = el;
        while (current) {
          const gn = current.getAttribute("data-sidebar-group");
          if (gn != null) { targetGroup = gn; break; }
          current = current.parentElement;
        }
      }
      setDragOverGroup(null);

      const action = resolveGroupDrop(sourceGroup, targetGroup, ev.altKey);
      if (action.kind === "none") return;
      if (action.kind === "remove-all") {
        await removeProjectFromAllGroups(projectKey);
      } else {
        if (action.kind === "move") {
          await removeProjectFromGroup(projectKey, action.from);
        }
        await addProjectToGroup(projectKey, action.to);
      }
      await loadData();
      emitGroupsUpdated();
    };

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };

  // ── Render ───────────────────────────────────────────────────────────────
  const renderProjectRow = (projectKey: string, sourceGroup: string | null) => {
    const isActive = activeTab === "projects" && projectKey === activeProjectKey;
    const label = labelForKey(projectKey);
    return (
      <div key={projectKey} className="group/project relative">
        <button
          onClick={() => onNavigateToProject(projectKey)}
          className={`w-full truncate rounded-md py-1.5 pl-[34px] pr-9 text-left text-[13px] font-normal transition-colors hover:bg-bg-sidebar hover:text-text-base ${
            isActive ? "bg-bg-sidebar text-text-base" : "text-text-muted"
          }`}
          onPointerDown={(e) => {
            if (e.button !== 0) return;
            const timeout = setTimeout(() => handleDragStart(projectKey, sourceGroup, e), 200);
            const cancel = () => { clearTimeout(timeout); window.removeEventListener("pointerup", cancel); };
            window.addEventListener("pointerup", cancel, { once: true });
          }}
        >
          <ProjectNameLabel label={label} />
        </button>
        <button
          type="button"
          onClick={(event) => void handleRemoveProject(projectKey, event)}
          className="pointer-events-none absolute right-1 top-1/2 flex h-6 w-6 -translate-y-1/2 items-center justify-center rounded text-text-muted/50 opacity-0 transition-[background-color,color,opacity] hover:bg-danger/10 hover:text-danger group-hover/project:pointer-events-auto group-hover/project:opacity-100 group-focus-within/project:pointer-events-auto group-focus-within/project:opacity-100"
          aria-label={`Remove ${projectLabelText(label)}`}
          title={`Remove ${projectLabelText(label)}`}
        >
          <Trash2 size={12} />
        </button>
      </div>
    );
  };

  return (
    <div className="flex min-h-full flex-col">
      <div className="flex-1">
        {/* View all projects — clears any group filter */}
        <div className="mb-3">
          <NavItem
            id="projects"
            icon={LayoutGrid}
            label="View All"
            isActive={activeTab === "projects" && activeGroupFilter === null && !activeProjectKey}
            onClick={() => {
              onFilterByGroup(null);
              onTabClick("projects");
            }}
          />
        </div>

        {/* Projects section header with create group button */}
        <div className="mb-1.5 flex items-center justify-between">
          <button
            onClick={() => { onFilterByGroup(null); onTabClick("projects"); }}
            className="flex items-center gap-1 px-3 py-1 transition-colors hover:text-text-base"
          >
            <span className="text-[11px] font-semibold tracking-wider text-text-muted/50 hover:text-text-muted/80">
              Projects
            </span>
          </button>
          <button
            onClick={() => { setCreatingGroup(true); setNewGroupName(""); }}
            className="mr-1 flex h-[22px] w-[22px] items-center justify-center rounded text-text-muted/50 transition-colors hover:bg-bg-sidebar hover:text-text-base"
            title="New group"
          >
            <Plus size={12} />
          </button>
        </div>

        {/* Inline group creation */}
        {creatingGroup && (
          <div className="mb-1 px-3">
            <input
              ref={newGroupInputRef}
              value={newGroupName}
              onChange={(e) => setNewGroupName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleCreateGroup();
                if (e.key === "Escape") { setCreatingGroup(false); setNewGroupName(""); }
              }}
              onBlur={handleCreateGroup}
              placeholder="Group name..."
              className="w-full rounded border border-border-strong/40 bg-bg-base px-2 py-1 text-[13px] text-text-base focus:border-brand focus:outline-none"
            />
          </div>
        )}

        {/* Groups first — matches overview "Show Groups" order */}
        <div className="mb-1 space-y-0.5">
          {groups.map((group) => {
            const isCollapsed = collapsedGroups.has(group.name);
            const isActiveFilter = activeGroupFilter === group.name && activeTab === "projects" && !activeProjectKey;
            const Chevron = isCollapsed ? ChevronRight : ChevronDown;
            const visibleProjects = group.projects
              .filter((p) => projects.includes(p))
              .sort((a, b) => nameForKey(a).localeCompare(nameForKey(b)));
            return (
              <div key={group.name} data-sidebar-group={group.name}>
                <button
                  onClick={() => {
                    toggleGroupCollapse(group.name);
                    onFilterByGroup(group.name);
                    onTabClick("projects");
                  }}
                  className={`flex w-full items-center gap-2 rounded-md py-1.5 pl-3 pr-3 text-[13px] font-medium transition-colors ${
                    isActiveFilter
                      ? "bg-bg-sidebar text-text-base"
                      : "text-text-muted hover:bg-bg-sidebar hover:text-text-base"
                  }`}
                >
                  <Chevron
                    size={14}
                    className={`shrink-0 ${isActiveFilter ? "text-text-base" : "text-text-muted/60"}`}
                  />
                  <span className="min-w-0 flex-1 truncate text-left">{group.name}</span>
                  <span className="shrink-0 text-[11px] tabular-nums text-text-muted/40">
                    {visibleProjects.length}
                  </span>
                </button>
                {!isCollapsed && (
                  <div>
                    {visibleProjects.map((projectKey) => renderProjectRow(projectKey, group.name))}
                    {visibleProjects.length === 0 && (
                      <div className="px-3 py-1.5 pl-[34px] text-[11px] italic text-text-muted/35">
                        Empty
                      </div>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>

        {/* Ungrouped projects — after groups, as "Other Projects" */}
        {ungroupedProjects.length > 0 && (
          <div className="mb-1 space-y-0.5" data-sidebar-group={UNGROUPED_DROP_TARGET}>
            <button
              onClick={() => {
                onFilterByGroup("__ungrouped__");
                onTabClick("projects");
              }}
              className={`flex w-full items-center gap-2 rounded-md py-1.5 pl-3 pr-3 text-[13px] font-medium transition-colors ${
                activeGroupFilter === "__ungrouped__" && activeTab === "projects" && !activeProjectKey
                  ? "bg-bg-sidebar text-text-base"
                  : "text-text-muted hover:bg-bg-sidebar hover:text-text-base"
              }`}
            >
              {/* Spacer matches group chevron width so labels align */}
              <span className="inline-block w-3.5 shrink-0" aria-hidden />
              <span className="min-w-0 flex-1 truncate text-left">Other Projects</span>
              <span className="shrink-0 text-[11px] tabular-nums text-text-muted/40">
                {ungroupedProjects.length}
              </span>
            </button>
            {ungroupedProjects.map((projectKey) => renderProjectRow(projectKey, null))}
          </div>
        )}
      </div>

      {/* Groups management — pinned to bottom when space allows */}
      <div className="mt-auto border-t border-border-strong/30 pt-2">
        <NavItem
          id="project-groups"
          icon={Layers}
          label="Groups"
          isActive={activeTab === "project-groups"}
          onClick={onTabClick}
        />
      </div>

      {/* Drag ghost */}
      {dragGhost && (
        <div
          className="pointer-events-none fixed z-[9999] rounded-md border border-border-strong/60 bg-bg-sidebar px-3 py-1.5 text-[13px] text-text-base shadow-lg"
          style={{ left: dragGhost.x + 12, top: dragGhost.y - 10 }}
        >
          {dragGhost.name}
          {dragGhost.fromGroup && (
            <div className="mt-0.5 text-[11px] text-text-muted">
              {dragGhost.adding
                ? "Adding to group"
                : `Hold ${ADD_MODIFIER_LABEL} to add without moving`}
            </div>
          )}
        </div>
      )}

      {/* Drop highlight overlay on groups during drag */}
      {dragGhost && dragOverGroup && dragOverGroup !== UNGROUPED_DROP_TARGET && (
        <style>{`
          [data-sidebar-group="${dragOverGroup}"] {
            background: rgba(99, 102, 241, 0.08);
            border-radius: 6px;
          }
        `}</style>
      )}
    </div>
  );
}
