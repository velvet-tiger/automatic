import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { Plus, X, Edit2, Check, Layers, FolderOpen } from "lucide-react";

// ── Types ─────────────────────────────────────────────────────────────────────

interface ProjectGroup {
  name: string;
  description: string;
  projects: string[];
  created_at: string;
  updated_at: string;
}

interface ProjectGroupsProps {
  onNavigateToProject?: (projectName: string) => void;
  initialGroup?: string | null;
  onInitialGroupConsumed?: () => void;
}

// ── Component ─────────────────────────────────────────────────────────────────

export default function ProjectGroups({ onNavigateToProject, initialGroup, onInitialGroupConsumed }: ProjectGroupsProps) {
  const [groups, setGroups] = useState<string[]>([]);
  const [selectedName, setSelectedName] = useState<string | null>(null);
  const [group, setGroup] = useState<ProjectGroup | null>(null);
  const [allProjects, setAllProjects] = useState<string[]>([]);

  // Edit state
  const [isEditing, setIsEditing] = useState(false);
  const [editDescription, setEditDescription] = useState("");

  // Create state
  const [isCreating, setIsCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [newDescription, setNewDescription] = useState("");

  const [error, setError] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);

  // Load group list and all available projects on mount.
  useEffect(() => {
    loadGroups();
    loadAllProjects();
  }, []);

  // Select initial group when provided
  useEffect(() => {
    if (initialGroup && groups.includes(initialGroup) && selectedName !== initialGroup) {
      loadGroup(initialGroup);
      if (onInitialGroupConsumed) onInitialGroupConsumed();
    }
  }, [initialGroup, groups]);

  const loadGroups = async () => {
    try {
      const result: string[] = await invoke("list_groups");
      setGroups(result.sort((a, b) => a.localeCompare(b)));
      setError(null);
    } catch (err: any) {
      setError(`Failed to load groups: ${err}`);
    }
  };

  const loadAllProjects = async () => {
    try {
      const result: string[] = await invoke("get_projects");
      setAllProjects(result.sort((a, b) => a.localeCompare(b)));
    } catch {
      // Non-fatal.
    }
  };

  const loadGroup = async (name: string) => {
    try {
      const raw: string = await invoke("read_group", { name });
      const g: ProjectGroup = JSON.parse(raw);
      setGroup(g);
      setEditDescription(g.description);
      setSelectedName(name);
      setIsEditing(false);
      setError(null);
    } catch (err: any) {
      setError(`Failed to load group: ${err}`);
    }
  };

  const handleSelectGroup = (name: string) => {
    if (selectedName === name && !isCreating) return;
    setIsCreating(false);
    loadGroup(name);
  };

  // ── Save description ──────────────────────────────────────────────────────

  const handleSaveDescription = async () => {
    if (!group) return;
    setIsSaving(true);
    try {
      const updated: ProjectGroup = {
        ...group,
        description: editDescription,
        updated_at: new Date().toISOString(),
      };
      await invoke("save_group", {
        name: group.name,
        data: JSON.stringify(updated),
      });
      setGroup(updated);
      setIsEditing(false);
      setError(null);
    } catch (err: any) {
      setError(`Failed to save group: ${err}`);
    } finally {
      setIsSaving(false);
    }
  };

  // ── Create group ──────────────────────────────────────────────────────────

  const handleStartCreate = () => {
    setIsCreating(true);
    setSelectedName(null);
    setGroup(null);
    setNewName("");
    setNewDescription("");
    setError(null);
  };

  const handleCreateGroup = async () => {
    const name = newName.trim();
    if (!name) {
      setError("Group name is required.");
      return;
    }
    if (!/^[a-zA-Z0-9][a-zA-Z0-9 _-]*$/.test(name)) {
      setError("Name may only contain letters, numbers, spaces, hyphens, and underscores.");
      return;
    }
    setIsSaving(true);
    try {
      const now = new Date().toISOString();
      const newGroup: ProjectGroup = {
        name,
        description: newDescription.trim(),
        projects: [],
        created_at: now,
        updated_at: now,
      };
      await invoke("save_group", { name, data: JSON.stringify(newGroup) });
      await loadGroups();
      setIsCreating(false);
      loadGroup(name);
    } catch (err: any) {
      setError(`Failed to create group: ${err}`);
    } finally {
      setIsSaving(false);
    }
  };

  // ── Delete group ──────────────────────────────────────────────────────────

  const handleDeleteGroup = async (name: string) => {
    const confirmed = await ask(`Delete group "${name}"? This will not delete the projects themselves.`, {
      title: "Delete Group",
      kind: "warning",
    });
    if (!confirmed) return;
    try {
      await invoke("delete_group", { name });
      setSelectedName(null);
      setGroup(null);
      await loadGroups();
    } catch (err: any) {
      setError(`Failed to delete group: ${err}`);
    }
  };

  // ── Project membership ────────────────────────────────────────────────────

  const handleAddProject = async (projectName: string) => {
    if (!group) return;
    if (group.projects.includes(projectName)) return;
    const updated: ProjectGroup = {
      ...group,
      projects: [...group.projects, projectName],
      updated_at: new Date().toISOString(),
    };
    await persistGroup(updated, [projectName]);
  };

  const handleRemoveProject = async (projectName: string) => {
    if (!group) return;
    const updated: ProjectGroup = {
      ...group,
      projects: group.projects.filter((p) => p !== projectName),
      updated_at: new Date().toISOString(),
    };
    await persistGroup(updated, [projectName]);
  };

  /** Save the group then re-sync each affected project so instruction files
   *  are updated immediately without requiring a manual sync. */
  const persistGroup = async (updated: ProjectGroup, syncProjects: string[] = []) => {
    try {
      await invoke("save_group", {
        name: updated.name,
        data: JSON.stringify(updated),
      });
      setGroup(updated);
      setError(null);
      // Fire-and-forget syncs — errors are non-fatal (instruction files will be
      // updated on the next explicit sync if this fails).
      for (const name of syncProjects) {
        invoke("sync_project", { name }).catch((e: unknown) => {
          console.warn(`Group sync: could not re-sync project '${name}':`, e);
        });
      }
    } catch (err: any) {
      setError(`Failed to update group: ${err}`);
    }
  };

  // ── Projects not yet in this group ───────────────────────────────────────

  const availableProjects = allProjects.filter(
    (p) => group && !group.projects.includes(p)
  );

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <div className="flex h-full">
      {/* Sidebar list */}
      <aside className="w-52 flex-shrink-0 border-r border-border-strong/40 flex flex-col">
        <div className="flex items-center justify-between px-4 py-3 border-b border-border-strong/40">
          <span className="text-[12px] font-semibold text-text-muted uppercase tracking-wider">Groups</span>
          <button
            onClick={handleStartCreate}
            className="flex items-center justify-center w-[22px] h-[22px] rounded-md text-text-muted hover:bg-bg-sidebar hover:text-text-base transition-colors"
            title="New group"
          >
            <Plus size={13} />
          </button>
        </div>
        <ul className="flex-1 overflow-y-auto py-1 custom-scrollbar">
          {groups.length === 0 && !isCreating && (
            <li className="px-4 py-3 text-[12px] text-text-muted">No groups yet.</li>
          )}
          {groups.map((name) => (
            <li key={name}>
              <button
                onClick={() => handleSelectGroup(name)}
                className={`w-full flex items-center gap-2 px-3 py-1.5 text-[13px] transition-colors ${
                  selectedName === name && !isCreating
                    ? "bg-bg-sidebar text-text-base"
                    : "text-text-muted hover:bg-bg-sidebar hover:text-text-base"
                }`}
              >
                <Layers size={13} className="shrink-0" />
                <span className="flex-1 text-left truncate">{name}</span>
              </button>
            </li>
          ))}
        </ul>
      </aside>

      {/* Main panel */}
      <div className="flex-1 flex flex-col min-w-0 overflow-y-auto custom-scrollbar">
        {error && (
          <div className="mx-6 mt-4 px-3 py-2 rounded bg-red-500/10 border border-red-500/30 text-red-400 text-[12px]">
            {error}
          </div>
        )}

        {/* Create form */}
        {isCreating && (
          <div className="p-6 max-w-xl">
            <h2 className="text-[15px] font-semibold text-text-base mb-4">New Project Group</h2>
            <div className="space-y-3">
              <div>
                <label className="block text-[12px] font-medium text-text-muted mb-1">
                  Name <span className="text-red-400">*</span>
                </label>
                <input
                  type="text"
                  value={newName}
                  onChange={(e) => setNewName(e.target.value)}
                  placeholder="e.g. monorepo-services"
                  className="w-full px-3 py-1.5 rounded-md bg-bg-input border border-border-strong/50 text-text-base text-[13px] placeholder:text-text-muted/50 focus:outline-none focus:border-brand/60"
                  autoFocus
                  onKeyDown={(e) => {
                    if (e.key === "Enter") handleCreateGroup();
                    if (e.key === "Escape") setIsCreating(false);
                  }}
                />
              </div>
              <div>
                <label className="block text-[12px] font-medium text-text-muted mb-1">
                  Description
                  <span className="ml-1.5 font-normal opacity-60">— injected into all member project instructions</span>
                </label>
                <input
                  type="text"
                  value={newDescription}
                  onChange={(e) => setNewDescription(e.target.value)}
                  placeholder="e.g. Backend services sharing a common API contract"
                  className="w-full px-3 py-1.5 rounded-md bg-bg-input border border-border-strong/50 text-text-base text-[13px] placeholder:text-text-muted/50 focus:outline-none focus:border-brand/60"
                  onKeyDown={(e) => {
                    if (e.key === "Enter") handleCreateGroup();
                    if (e.key === "Escape") setIsCreating(false);
                  }}
                />
              </div>
              <div className="flex gap-2 pt-1">
                <button
                  onClick={handleCreateGroup}
                  disabled={isSaving || !newName.trim()}
                  className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[12px] font-medium bg-brand hover:bg-brand-hover text-white transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  <Check size={13} />
                  Create
                </button>
                <button
                  onClick={() => setIsCreating(false)}
                  className="px-3 py-1.5 rounded-md text-[12px] font-medium text-text-muted hover:bg-bg-sidebar hover:text-text-base transition-colors"
                >
                  Cancel
                </button>
              </div>
            </div>
          </div>
        )}

        {/* Group detail */}
        {group && !isCreating && (
          <div className="p-6 space-y-6 max-w-2xl">
            {/* Header */}
            <div className="flex items-start justify-between">
              <div className="flex items-center gap-2">
                <Layers size={18} className="text-text-muted shrink-0" />
                <h2 className="text-[16px] font-semibold text-text-base">{group.name}</h2>
              </div>
              <button
                onClick={() => handleDeleteGroup(group.name)}
                className="flex items-center justify-center w-[26px] h-[26px] rounded-md text-text-muted hover:bg-red-500/10 hover:text-red-400 transition-colors"
                title="Delete group"
              >
                <X size={14} />
              </button>
            </div>

            {/* Description */}
            <div>
              <div className="flex items-center gap-2 mb-1">
                <span className="text-[12px] font-semibold text-text-muted uppercase tracking-wider">Description</span>
                {!isEditing && (
                  <button
                    onClick={() => setIsEditing(true)}
                    className="text-text-muted hover:text-text-base transition-colors"
                    title="Edit description"
                  >
                    <Edit2 size={11} />
                  </button>
                )}
              </div>
              {isEditing ? (
                <div className="space-y-2">
                  <input
                    type="text"
                    value={editDescription}
                    onChange={(e) => setEditDescription(e.target.value)}
                    placeholder="e.g. Backend services sharing a common API contract"
                    className="w-full px-3 py-1.5 rounded-md bg-bg-input border border-border-strong/50 text-text-base text-[13px] placeholder:text-text-muted/50 focus:outline-none focus:border-brand/60"
                    autoFocus
                    onKeyDown={(e) => {
                      if (e.key === "Enter") handleSaveDescription();
                      if (e.key === "Escape") {
                        setEditDescription(group.description);
                        setIsEditing(false);
                      }
                    }}
                  />
                  <div className="flex gap-2">
                    <button
                      onClick={handleSaveDescription}
                      disabled={isSaving}
                      className="flex items-center gap-1.5 px-2.5 py-1 rounded text-[11px] font-medium bg-brand hover:bg-brand-hover text-white transition-colors disabled:opacity-50"
                    >
                      <Check size={11} />
                      Save
                    </button>
                    <button
                      onClick={() => {
                        setEditDescription(group.description);
                        setIsEditing(false);
                      }}
                      className="px-2.5 py-1 rounded text-[11px] text-text-muted hover:text-text-base transition-colors"
                    >
                      Cancel
                    </button>
                  </div>
                </div>
              ) : (
                <p className="text-[13px] text-text-muted">
                  {group.description.trim() || (
                    <span className="italic opacity-60">No description. Add one — it will be injected into all member project instructions.</span>
                  )}
                </p>
              )}
            </div>

            {/* Projects in group */}
            <div>
              <div className="flex items-center gap-2 mb-3">
                <span className="text-[12px] font-semibold text-text-muted uppercase tracking-wider">Projects</span>
                <span className="text-[11px] text-text-muted bg-bg-sidebar px-1.5 rounded">
                  {group.projects.length}
                </span>
              </div>

              {group.projects.length === 0 ? (
                <p className="text-[13px] text-text-muted italic opacity-60">
                  No projects in this group yet. Add projects below.
                </p>
              ) : (
                <ul className="space-y-1.5">
                  {group.projects.map((projectName) => (
                    <li
                      key={projectName}
                      className="flex items-center gap-2 px-3 py-1.5 rounded-md bg-bg-input border border-border-strong/30"
                    >
                      <FolderOpen size={13} className="text-text-muted shrink-0" />
                      <span
                        className={`flex-1 text-[13px] text-text-base truncate ${onNavigateToProject ? "cursor-pointer hover:text-brand transition-colors" : ""}`}
                        onClick={() => onNavigateToProject?.(projectName)}
                        title={onNavigateToProject ? `Open ${projectName}` : projectName}
                      >
                        {projectName}
                      </span>
                      <button
                        onClick={() => handleRemoveProject(projectName)}
                        className="flex items-center justify-center w-[20px] h-[20px] rounded text-text-muted hover:bg-red-500/10 hover:text-red-400 transition-colors shrink-0"
                        title={`Remove ${projectName} from group`}
                      >
                        <X size={11} />
                      </button>
                    </li>
                  ))}
                </ul>
              )}

              {/* Add project picker */}
              {availableProjects.length > 0 && (
                <div className="mt-3">
                  <AddProjectPicker
                    projects={availableProjects}
                    onAdd={handleAddProject}
                  />
                </div>
              )}
            </div>

            {/* Info callout */}
            <div className="rounded-md bg-bg-input border border-border-strong/30 px-3 py-2.5 text-[12px] text-text-muted space-y-1">
              <p className="font-medium text-text-base">How groups work</p>
              <p>
                When a project in this group is synced, Automatic injects a context block into its
                agent instruction files. The block lists all related projects — with their
                descriptions and relative paths — so your agent can recognise and navigate between them.
              </p>
            </div>
          </div>
        )}

        {/* Empty state */}
        {!group && !isCreating && (
          <div className="flex-1 flex flex-col items-center justify-center text-center p-8">
            <Layers size={32} className="text-text-muted/30 mb-3" />
            <p className="text-[14px] font-medium text-text-base mb-1">No group selected</p>
            <p className="text-[13px] text-text-muted max-w-sm">
              Select a group from the sidebar, or create one to start linking related projects.
            </p>
            <button
              onClick={handleStartCreate}
              className="mt-4 flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[12px] font-medium bg-brand hover:bg-brand-hover text-white transition-colors"
            >
              <Plus size={13} />
              New Group
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

// ── AddProjectPicker ──────────────────────────────────────────────────────────

interface AddProjectPickerProps {
  projects: string[];
  onAdd: (name: string) => void;
}

function AddProjectPicker({ projects, onAdd }: AddProjectPickerProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");

  const filtered = projects.filter((p) =>
    p.toLowerCase().includes(query.toLowerCase())
  );

  if (!open) {
    return (
      <button
        onClick={() => setOpen(true)}
        className="flex items-center gap-1.5 px-2.5 py-1 rounded text-[12px] text-text-muted hover:bg-bg-sidebar hover:text-text-base transition-colors border border-dashed border-border-strong/40"
      >
        <Plus size={12} />
        Add project
      </button>
    );
  }

  return (
    <div className="border border-border-strong/40 rounded-md bg-bg-input overflow-hidden">
      <input
        type="text"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        placeholder="Search projects…"
        className="w-full px-3 py-1.5 bg-transparent border-b border-border-strong/30 text-[13px] text-text-base placeholder:text-text-muted/50 focus:outline-none"
        autoFocus
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            setOpen(false);
            setQuery("");
          }
        }}
      />
      <ul className="max-h-48 overflow-y-auto custom-scrollbar">
        {filtered.length === 0 && (
          <li className="px-3 py-2 text-[12px] text-text-muted">No matching projects.</li>
        )}
        {filtered.map((name) => (
          <li key={name}>
            <button
              onClick={() => {
                onAdd(name);
                setOpen(false);
                setQuery("");
              }}
              className="w-full flex items-center gap-2 px-3 py-1.5 text-[13px] text-text-muted hover:bg-bg-sidebar hover:text-text-base transition-colors text-left"
            >
              <FolderOpen size={12} className="shrink-0" />
              {name}
            </button>
          </li>
        ))}
      </ul>
      <div className="border-t border-border-strong/30 px-3 py-1.5">
        <button
          onClick={() => {
            setOpen(false);
            setQuery("");
          }}
          className="text-[11px] text-text-muted hover:text-text-base transition-colors"
        >
          Cancel
        </button>
      </div>
    </div>
  );
}
