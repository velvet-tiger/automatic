import { useState, useCallback, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Folder, FolderOpen as FolderOpenIcon, Layers, Plus, FolderPlus } from "lucide-react";

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
  projects: string[];
}

interface WorkspaceSidebarProps {
  activeTab: string;
  onTabClick: (id: string) => void;
  onNavigateToProject: (name: string) => void;
  /** Currently active group filter for the Projects page. */
  activeGroupFilter: string | null;
  /** Called when a group name is clicked — filters the Projects page to that group. */
  onFilterByGroup: (groupName: string | null) => void;
  /** Called when "New Project" is clicked in the sidebar. */
  onCreateProject: () => void;
}

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

export default function WorkspaceSidebar({ activeTab, onTabClick, onNavigateToProject, activeGroupFilter, onFilterByGroup, onCreateProject }: WorkspaceSidebarProps) {
  // ── Project + group data ─────────────────────────────────────────────────
  const [projects, setProjects] = useState<string[]>([]);
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
  const [dragGhost, setDragGhost] = useState<{ name: string; x: number; y: number } | null>(null);
  const [dragOverGroup, setDragOverGroup] = useState<string | null>(null);

  // ── Load data ────────────────────────────────────────────────────────────
  const loadData = useCallback(async () => {
    try {
      const projectNames: string[] = await invoke("get_projects");
      projectNames.sort((a, b) => a.localeCompare(b));
      setProjects(projectNames);

      const groupNames: string[] = await invoke("list_groups");
      const loaded: SidebarGroup[] = [];
      for (const name of groupNames.sort((a, b) => a.localeCompare(b))) {
        try {
          const raw: string = await invoke("read_group", { name });
          const g = JSON.parse(raw);
          loaded.push({ name: g.name, projects: g.projects ?? [] });
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
    return () => window.removeEventListener("groups-updated", handler);
  }, [loadData]);

  /** Notify other components that groups changed. */
  const emitGroupsUpdated = () => {
    window.dispatchEvent(new CustomEvent("groups-updated"));
  };

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
  const addProjectToGroup = async (projectName: string, groupName: string) => {
    try {
      const raw: string = await invoke("read_group", { name: groupName });
      const g: ProjectGroup = JSON.parse(raw);
      if (!g.projects.includes(projectName)) {
        g.projects.push(projectName);
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

  const removeProjectFromGroup = async (projectName: string, groupName: string) => {
    try {
      const raw: string = await invoke("read_group", { name: groupName });
      const g: ProjectGroup = JSON.parse(raw);
      g.projects = g.projects.filter((p: string) => p !== projectName);
      g.updated_at = new Date().toISOString();
      await invoke("save_group", { name: groupName, data: JSON.stringify(g) });
      const toSync = [...g.projects, projectName];
      for (const name of toSync) {
        invoke("sync_project", { name }).catch(() => {});
      }
    } catch (err) {
      console.error("Failed to remove from group:", err);
    }
  };

  const removeProjectFromAllGroups = async (projectName: string) => {
    for (const group of groups) {
      if (group.projects.includes(projectName)) {
        await removeProjectFromGroup(projectName, group.name);
      }
    }
  };

  // ── Drag handlers ────────────────────────────────────────────────────────
  const handleDragStart = (projectName: string, sourceGroup: string | null, e: React.PointerEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startY = e.clientY;

    setDragGhost({ name: projectName, x: startX, y: startY });

    const onMove = (ev: PointerEvent) => {
      setDragGhost({ name: projectName, x: ev.clientX, y: ev.clientY });

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

      // "__ungrouped__" means remove from all groups
      if (targetGroup === "__ungrouped__") {
        if (sourceGroup) {
          await removeProjectFromAllGroups(projectName);
          await loadData();
          emitGroupsUpdated();
        }
        return;
      }

      // No change if same group or no target
      if (!targetGroup || targetGroup === sourceGroup) return;

      // Move: remove from source, add to target
      if (sourceGroup) {
        await removeProjectFromGroup(projectName, sourceGroup);
      }
      await addProjectToGroup(projectName, targetGroup);
      await loadData();
      emitGroupsUpdated();
    };

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };

  // ── Render ───────────────────────────────────────────────────────────────
  return (
    <>
      {/* New Project action */}
      <button
        onClick={onCreateProject}
        className="w-full flex items-center gap-2.5 px-3 py-2 mb-2 rounded-md text-[13px] font-medium text-text-muted hover:bg-bg-sidebar hover:text-text-base transition-colors"
      >
        <FolderPlus size={14} className="shrink-0 text-text-muted" />
        <span className="flex-1 text-left">New Project</span>
      </button>

      {/* Projects section header with create group button */}
      <div className="flex items-center justify-between mb-1">
        <button
          onClick={() => { onFilterByGroup(null); onTabClick("projects"); }}
          className="flex items-center gap-1 px-3 py-1 hover:text-text-base transition-colors"
        >
          <span className="text-[11px] font-semibold tracking-wider text-text-muted/60 hover:text-text-muted">Projects</span>
        </button>
        <button
          onClick={() => { setCreatingGroup(true); setNewGroupName(""); }}
          className="flex items-center justify-center w-[22px] h-[22px] rounded text-text-muted/50 hover:text-text-base hover:bg-bg-sidebar transition-colors mr-1"
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
            className="w-full text-[13px] bg-bg-base border border-border-strong/40 rounded px-2 py-1 text-text-base focus:outline-none focus:border-brand"
          />
        </div>
      )}

      {/* Ungrouped projects — top level */}
      {ungroupedProjects.length > 0 && (
        <div className="mb-1" data-sidebar-group="__ungrouped__">
          {ungroupedProjects.map((projectName) => (
            <button
              key={projectName}
              onClick={() => onNavigateToProject(projectName)}
              className="w-full text-left pl-[34px] pr-3 py-1.5 text-[13px] text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded-md transition-colors truncate"
              onPointerDown={(e) => {
                if (e.button !== 0) return;
                const timeout = setTimeout(() => handleDragStart(projectName, null, e), 200);
                const cancel = () => { clearTimeout(timeout); window.removeEventListener("pointerup", cancel); };
                window.addEventListener("pointerup", cancel, { once: true });
              }}
            >
              {projectName}
            </button>
          ))}
        </div>
      )}

      {/* Groups */}
      <div className="mb-2">
        {groups.map((group) => {
          const isCollapsed = collapsedGroups.has(group.name);
          const isActiveFilter = activeGroupFilter === group.name && activeTab === "projects";
          const GroupIcon = isCollapsed ? Folder : FolderOpenIcon;
          return (
          <div key={group.name} data-sidebar-group={group.name}>
            <button
              onClick={() => { toggleGroupCollapse(group.name); onFilterByGroup(group.name); onTabClick("projects"); }}
              className={`w-full flex items-center gap-2 pl-[12px] pr-3 py-1.5 rounded-md text-[13px] font-medium transition-colors ${
                isActiveFilter
                  ? "bg-bg-sidebar text-text-base"
                  : "text-text-muted hover:bg-bg-sidebar hover:text-text-base"
              }`}
            >
              <GroupIcon size={14} className={`shrink-0 ${isActiveFilter ? "text-text-base" : "text-text-muted"}`} />
              <span className="flex-1 text-left truncate">{group.name}</span>
              <span className="text-[11px] text-text-muted/50 shrink-0">{group.projects.length}</span>
            </button>
            {!isCollapsed && (
              <div>
                {group.projects
                  .filter((p) => projects.includes(p))
                  .sort((a, b) => a.localeCompare(b))
                  .map((projectName) => (
                    <button
                      key={projectName}
                      onClick={() => onNavigateToProject(projectName)}
                      className="w-full text-left pl-[34px] pr-3 py-1.5 text-[13px] text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded-md transition-colors truncate"
                      onPointerDown={(e) => {
                        if (e.button !== 0) return;
                        const timeout = setTimeout(() => handleDragStart(projectName, group.name, e), 200);
                        const cancel = () => { clearTimeout(timeout); window.removeEventListener("pointerup", cancel); };
                        window.addEventListener("pointerup", cancel, { once: true });
                      }}
                    >
                      {projectName}
                    </button>
                  ))}
                {group.projects.filter((p) => projects.includes(p)).length === 0 && (
                  <div className="pl-[34px] pr-3 py-1.5 text-[11px] text-text-muted/40 italic">Empty</div>
                )}
              </div>
            )}
          </div>
        );})}
      </div>

      <ul className="space-y-0.5">
        <NavItem id="project-groups" icon={Layers} label="Groups" isActive={activeTab === "project-groups"} onClick={onTabClick} />
      </ul>

      {/* Drag ghost */}
      {dragGhost && (
        <div
          className="fixed z-[9999] pointer-events-none px-3 py-1.5 rounded-md bg-bg-sidebar border border-border-strong/60 text-[13px] text-text-base shadow-lg"
          style={{ left: dragGhost.x + 12, top: dragGhost.y - 10 }}
        >
          {dragGhost.name}
        </div>
      )}

      {/* Drop highlight overlay on groups during drag */}
      {dragGhost && dragOverGroup && dragOverGroup !== "__ungrouped__" && (
        <style>{`
          [data-sidebar-group="${dragOverGroup}"] {
            background: rgba(99, 102, 241, 0.08);
            border-radius: 6px;
          }
        `}</style>
      )}
    </>
  );
}
