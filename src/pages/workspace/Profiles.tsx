import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  Check,
  Copy,
  Folder,
  FolderGit2,
  Layers,
  Plus,
  ScrollText,
  Search,
  Trash2,
  X,
} from "lucide-react";
import { useRecentlyAdded } from "../../lib/useRecentlyAdded";
import { AuthorSection, type AuthorDescriptor } from "../../components/AuthorPanel";
import { SkillSelector } from "../../components/SkillSelector";
import { AgentSelector, type AgentInfo } from "../../components/AgentSelector";
import { McpSelector } from "../../components/McpSelector";
import { SubAgentSelector } from "../../components/SubAgentSelector";
import { CommandSelector } from "../../components/CommandSelector";
import { HookSelector } from "../../components/HookSelector";
import { AssetTable } from "../../components/AssetTable";
import { AssetDrawer } from "../../components/AssetDrawer";
import { useBulkSelection } from "../../lib/useBulkSelection";
import type { HookEntry, ProjectProfile, UserCommandEntry } from "./projects/types";
import { normaliseProfile } from "./projects/helpers";

// ── Profiles ──────────────────────────────────────────────────────────────────
//
// A profile is the live counterpart of a project template. Saving one brings
// every attached project back in step (the backend reconciles and re-syncs),
// so this page dispatches `profiles-updated` after every write that can
// change a project.

interface ProjectRef {
  name: string;
  directory: string;
}

interface ProjectSummary {
  name: string;
  directory: string;
}

function emptyProfile(name: string): ProjectProfile {
  return {
    name,
    description: "",
    skills: [],
    mcp_servers: [],
    providers: [],
    agents: [],
    user_agents: [],
    user_commands: [],
    hooks: [],
    rules: ["automatic-service"],
  };
}

function notifyProfilesUpdated() {
  window.dispatchEvent(new CustomEvent("profiles-updated"));
}

function summarise(p: ProjectProfile | undefined): string {
  if (!p) return "";
  const parts: string[] = [];
  const push = (n: number, one: string, many: string) => {
    if (n > 0) parts.push(`${n} ${n === 1 ? one : many}`);
  };
  push(p.skills.length, "skill", "skills");
  push(p.mcp_servers.length, "server", "servers");
  push(p.rules.length, "rule", "rules");
  push(p.hooks.length, "hook", "hooks");
  push(p.agents.length, "agent", "agents");
  return parts.join(" · ");
}

export default function Profiles({
  onNavigateToProject,
}: {
  onNavigateToProject?: (projectName: string) => void;
}) {
  const [profiles, setProfiles] = useState<string[]>([]);
  const [recentRefresh, setRecentRefresh] = useState(0);
  const recentIds = useRecentlyAdded("profiles", recentRefresh);
  const [profileData, setProfileData] = useState<Record<string, ProjectProfile>>({});

  const [selectedName, setSelectedName] = useState<string | null>(null);
  const [profile, setProfile] = useState<ProjectProfile | null>(null);
  /** The profile as last saved, so a save can tell whether `agents` changed. */
  const [savedProfile, setSavedProfile] = useState<ProjectProfile | null>(null);
  const [dirty, setDirty] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameName, setRenameName] = useState("");
  const [saveStatus, setSaveStatus] = useState<string | null>(null);

  const [availableAgents, setAvailableAgents] = useState<AgentInfo[]>([]);
  const [availableUserAgents, setAvailableUserAgents] = useState<{ id: string; name: string }[]>([]);
  const [availableUserCommands, setAvailableUserCommands] = useState<UserCommandEntry[]>([]);
  const [availableHooks, setAvailableHooks] = useState<HookEntry[]>([]);
  const [availableSkills, setAvailableSkills] = useState<string[]>([]);
  const [availableMcpServers, setAvailableMcpServers] = useState<string[]>([]);
  const [availableRules, setAvailableRules] = useState<{ id: string; name: string }[]>([]);

  const [referencingProjects, setReferencingProjects] = useState<ProjectRef[]>([]);
  const [allProjects, setAllProjects] = useState<ProjectSummary[]>([]);
  const [showAttachPicker, setShowAttachPicker] = useState(false);
  const [attachTarget, setAttachTarget] = useState<string | null>(null);
  const [attachStatus, setAttachStatus] = useState<string | null>(null);

  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [bulkDeleting, setBulkDeleting] = useState(false);
  const [bulkProgress, setBulkProgress] = useState<{ done: number; total: number } | null>(null);

  useEffect(() => {
    loadProfiles();
    loadInventory();
    loadAllProjects();
  }, []);

  const loadProfiles = async () => {
    try {
      const names: string[] = await invoke("get_project_profiles");
      names.sort((a, b) => a.localeCompare(b));
      setProfiles(names);
      setError(null);
      const entries = await Promise.all(
        names.map(async (name) => {
          try {
            const raw: string = await invoke("read_project_profile", { name });
            return [name, normaliseProfile(name, JSON.parse(raw))] as const;
          } catch {
            return [name, emptyProfile(name)] as const;
          }
        }),
      );
      setProfileData(Object.fromEntries(entries));
    } catch (err) {
      setError(`Failed to load profiles: ${err}`);
    }
  };

  const loadInventory = async () => {
    const quietly = async (fn: () => Promise<void>) => {
      try { await fn(); } catch { /* an empty picker is fine */ }
    };
    await Promise.all([
      quietly(async () => {
        const result: AgentInfo[] = await invoke("list_agents");
        setAvailableAgents([...result].sort((a, b) => a.label.localeCompare(b.label)));
      }),
      quietly(async () => {
        const result: { id: string; name: string }[] = await invoke("get_subagents");
        setAvailableUserAgents([...result].sort((a, b) => a.name.localeCompare(b.name)));
      }),
      quietly(async () => {
        const result: UserCommandEntry[] = await invoke("get_user_commands");
        setAvailableUserCommands([...result].sort((a, b) => a.id.localeCompare(b.id)));
      }),
      quietly(async () => {
        const result: HookEntry[] = await invoke("get_hooks");
        setAvailableHooks([...result].sort((a, b) => a.id.localeCompare(b.id)));
      }),
      quietly(async () => {
        const result: { name: string }[] = await invoke("get_skills");
        setAvailableSkills(result.map((e) => e.name).sort());
      }),
      quietly(async () => {
        const result: string[] = await invoke("list_mcp_server_configs");
        setAvailableMcpServers([...result].sort());
      }),
      quietly(async () => {
        const result: { id: string; name: string }[] = await invoke("get_rules");
        setAvailableRules([...result].sort((a, b) => a.name.localeCompare(b.name)));
      }),
    ]);
  };

  const loadAllProjects = async () => {
    try {
      const names: string[] = await invoke("get_projects");
      const loaded = await Promise.all(
        names.map(async (name) => {
          try {
            const raw: string = await invoke("read_project", { name });
            const parsed = JSON.parse(raw) as { directory?: string };
            return { name, directory: parsed.directory ?? "" };
          } catch {
            return { name, directory: "" };
          }
        }),
      );
      setAllProjects(loaded.sort((a, b) => a.name.localeCompare(b.name)));
    } catch { /* picker stays empty */ }
  };

  const loadReferencingProjects = async (name: string) => {
    try {
      const refs: ProjectRef[] = await invoke("get_projects_referencing_profile", { profileName: name });
      setReferencingProjects(refs);
    } catch {
      setReferencingProjects([]);
    }
  };

  const selectProfile = async (name: string) => {
    try {
      const raw: string = await invoke("read_project_profile", { name });
      const loaded = normaliseProfile(name, JSON.parse(raw));
      setSelectedName(name);
      setProfile(loaded);
      setSavedProfile(loaded);
      setDirty(false);
      setIsCreating(false);
      setError(null);
      setShowAttachPicker(false);
      setAttachStatus(null);
      setConfirmDelete(null);
      await loadReferencingProjects(name);
    } catch (err) {
      setError(`Failed to read profile: ${err}`);
    }
  };

  const updateField = <K extends keyof ProjectProfile>(key: K, value: ProjectProfile[K]) => {
    if (!profile) return;
    // Editing transfers authorship to the local user; drop any imported author.
    setProfile({ ...profile, [key]: value, _author: undefined });
    setDirty(true);
  };

  const handleSave = async () => {
    if (!profile) return;
    const name = isCreating ? newName.trim() : selectedName;
    if (!name) return;

    // Changing agents adds or removes agent config files in every attached
    // project, so that one edit deserves a confirmation.
    const agentsChanged =
      !!savedProfile &&
      JSON.stringify([...savedProfile.agents].sort()) !== JSON.stringify([...profile.agents].sort());
    if (agentsChanged && referencingProjects.length > 0) {
      const n = referencingProjects.length;
      const confirmed = await ask(
        `This profile's agents changed. Saving will add or remove agent configuration files in ${n} attached project${n === 1 ? "" : "s"}. Continue?`,
        { title: "Update attached projects", kind: "warning" },
      );
      if (!confirmed) return;
    }

    try {
      setSaveStatus("saving");
      const toSave: ProjectProfile = { ...profile, name };
      await invoke("save_project_profile", { name, data: JSON.stringify(toSave, null, 2) });
      setSelectedName(name);
      setSavedProfile(toSave);
      if (isCreating) {
        setIsCreating(false);
        await loadProfiles();
        setRecentRefresh((prev) => prev + 1);
      } else {
        setProfileData((prev) => ({ ...prev, [name]: toSave }));
      }
      await loadReferencingProjects(name);
      setDirty(false);
      setError(null);
      notifyProfilesUpdated();
      setSaveStatus(referencingProjects.length > 0 ? "Saved and projects updated" : "Saved");
      setTimeout(() => setSaveStatus(null), 3000);
    } catch (err) {
      setSaveStatus(null);
      setError(`Failed to save profile: ${err}`);
    }
  };

  const performDelete = async (name: string) => {
    try {
      await invoke("delete_project_profile", { name });
      if (selectedName === name) {
        setSelectedName(null);
        setProfile(null);
        setSavedProfile(null);
        setDirty(false);
      }
      await loadProfiles();
      setError(null);
      notifyProfilesUpdated();
    } catch (err) {
      setError(`Failed to delete profile: ${err}`);
    }
  };

  const attachedCountWarning = (count: number) =>
    count > 0
      ? `\n\nIt is attached to ${count} project${count === 1 ? "" : "s"}. Each one will lose every entry this profile provides and be re-synced.`
      : "";

  const handleDelete = async (name: string) => {
    if (confirmDelete !== name) {
      setConfirmDelete(name);
      return;
    }
    setConfirmDelete(null);
    await performDelete(name);
  };

  const handleRowDelete = async (name: string, e: React.MouseEvent) => {
    e.stopPropagation();
    let count = 0;
    try {
      const refs: ProjectRef[] = await invoke("get_projects_referencing_profile", { profileName: name });
      count = refs.length;
    } catch { /* fall through with no count */ }
    const confirmed = await ask(`Delete profile "${name}"?${attachedCountWarning(count)}`, {
      title: "Delete Profile",
      kind: "warning",
    });
    if (!confirmed) return;
    await performDelete(name);
  };

  const handleBulkDelete = async () => {
    const targets = profiles.filter((name) => selection.selectedIds.has(name));
    if (targets.length === 0) return;

    const preview = targets.slice(0, 10).map((t) => `• ${t}`).join("\n");
    const overflow = targets.length > 10 ? `\n…and ${targets.length - 10} more.` : "";
    const message = `Delete ${targets.length} profile${targets.length === 1 ? "" : "s"}?\n\n${preview}${overflow}\n\nAttached projects lose every entry each profile provides and are re-synced. This cannot be undone.`;
    const confirmed = await ask(message, { title: "Delete Profiles", kind: "warning" });
    if (!confirmed) return;

    setBulkDeleting(true);
    setBulkProgress({ done: 0, total: targets.length });
    const failed: { name: string; error: string }[] = [];
    for (let i = 0; i < targets.length; i++) {
      const name = targets[i]!;
      try {
        await invoke("delete_project_profile", { name });
      } catch (err) {
        failed.push({ name, error: String(err) });
      }
      setBulkProgress({ done: i + 1, total: targets.length });
    }

    if (selectedName && targets.includes(selectedName)) {
      setSelectedName(null);
      setProfile(null);
      setSavedProfile(null);
      setDirty(false);
    }

    await loadProfiles();
    selection.clearSelection();
    setBulkDeleting(false);
    setBulkProgress(null);
    notifyProfilesUpdated();
    if (failed.length > 0) {
      const detail = failed.slice(0, 5).map((f) => `${f.name}: ${f.error}`).join("\n");
      const more = failed.length > 5 ? `\n…and ${failed.length - 5} more.` : "";
      setError(`Failed to delete ${failed.length} profile${failed.length === 1 ? "" : "s"}:\n${detail}${more}`);
    } else {
      setError(null);
    }
  };

  const handleDuplicate = async () => {
    if (!profile || !selectedName) return;
    const base = `${selectedName}-copy`;
    let candidate = base;
    let i = 2;
    while (profiles.includes(candidate)) candidate = `${base}-${i++}`;
    try {
      const copy: ProjectProfile = { ...profile, name: candidate, _author: undefined };
      await invoke("save_project_profile", { name: candidate, data: JSON.stringify(copy, null, 2) });
      await loadProfiles();
      await selectProfile(candidate);
      setError(null);
    } catch (err) {
      setError(`Failed to duplicate profile: ${err}`);
    }
  };

  const attachToProject = async (projectName: string) => {
    if (!selectedName) return;
    try {
      await invoke("attach_profile_to_project", { projectName, profileName: selectedName });
      await loadReferencingProjects(selectedName);
      setShowAttachPicker(false);
      setError(null);
      notifyProfilesUpdated();
      setAttachStatus(`Attached to "${projectName}"`);
      setTimeout(() => setAttachStatus(null), 3000);
    } catch (err) {
      setError(`Failed to attach profile: ${err}`);
    }
  };

  const startCreate = () => {
    setSelectedName(null);
    setProfile(emptyProfile(""));
    setSavedProfile(null);
    setReferencingProjects([]);
    setDirty(true);
    setIsCreating(true);
    setNewName("");
  };

  const startRename = () => {
    if (!selectedName || isCreating) return;
    setRenameName(selectedName);
    setIsRenaming(true);
  };

  const handleRename = async () => {
    const trimmed = renameName.trim();
    if (!selectedName || !trimmed || trimmed === selectedName) {
      setIsRenaming(false);
      return;
    }
    try {
      await invoke("rename_project_profile", { oldName: selectedName, newName: trimmed });
      setSelectedName(trimmed);
      setIsRenaming(false);
      setError(null);
      await loadProfiles();
      await selectProfile(trimmed);
      notifyProfilesUpdated();
    } catch (err) {
      setError(`Failed to rename profile: ${err}`);
      setIsRenaming(false);
    }
  };

  type ListField = "skills" | "mcp_servers" | "providers" | "agents" | "user_agents" | "user_commands" | "hooks";

  const addItem = (key: ListField, item: string) => {
    if (!profile || !item.trim()) return;
    if (profile[key].includes(item.trim())) return;
    updateField(key, [...profile[key], item.trim()]);
  };

  const removeItem = (key: ListField, idx: number) => {
    if (!profile) return;
    updateField(key, profile[key].filter((_, i) => i !== idx));
  };

  const searchLower = search.trim().toLowerCase();
  const filteredProfiles = profiles.filter((name) => !searchLower || name.toLowerCase().includes(searchLower));
  const selection = useBulkSelection(filteredProfiles, (name) => name, () => true);
  const drawerOpen = !!profile;

  const closeDrawer = () => {
    setSelectedName(null);
    setProfile(null);
    setSavedProfile(null);
    setDirty(false);
    setIsCreating(false);
    setIsRenaming(false);
  };

  const renderTableRow = (name: string) => {
    const data = profileData[name];
    const isRowSelected = selection.isSelected(name);
    const isFocused = selectedName === name && !isCreating;

    return (
      <tr
        key={name}
        onClick={() => { if (!isCreating) selectProfile(name); }}
        className={`group cursor-pointer border-b border-border-strong/20 last:border-b-0 transition-colors ${
          isFocused ? "bg-bg-sidebar/60" : "hover:bg-bg-input/70"
        }`}
      >
        <td className="px-3 py-2 w-9" onClick={(e) => e.stopPropagation()}>
          <input
            type="checkbox"
            checked={isRowSelected}
            onChange={() => selection.toggleSelected(name)}
            aria-label={`Select ${name}`}
            className="cursor-pointer accent-brand"
          />
        </td>
        <td className="px-3 py-2 w-11">
          <div className="w-8 h-8 rounded-md bg-brand/15 flex items-center justify-center flex-shrink-0">
            <Layers size={15} className="text-brand" />
          </div>
        </td>
        <td className="px-3 py-2 min-w-0">
          <div className="flex items-center gap-2 min-w-0">
            <span className="text-[13px] font-medium text-text-base truncate">{name}</span>
            {recentIds.has(name) && (
              <span className="shrink-0 px-1.5 py-0.5 rounded bg-brand/15 text-brand text-[9px] font-semibold uppercase tracking-wider">New</span>
            )}
          </div>
          {data?.description && (
            <div className="text-[11px] text-text-muted truncate">{data.description}</div>
          )}
        </td>
        <td className="px-3 py-2 text-[11px] text-text-muted">
          {summarise(data)}
        </td>
        <td className="px-3 py-2 w-16 text-right" onClick={(e) => e.stopPropagation()}>
          <button
            onClick={(e) => handleRowDelete(name, e)}
            className="opacity-0 group-hover:opacity-100 p-1 text-text-muted hover:text-danger rounded transition-all"
            title="Delete profile"
          >
            <X size={13} />
          </button>
        </td>
      </tr>
    );
  };

  // ── Render ──────────────────────────────────────────────────────────────────

  return (
    <div className="flex h-full w-full flex-col bg-bg-base">
      {/* Toolbar */}
      <div className="shrink-0 border-b border-border-strong/40 bg-bg-input/40">
        <div className="flex items-center justify-between px-4 pt-3 pb-2 gap-3">
          <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
            Profiles
          </span>

          <div className="flex items-center gap-2 shrink-0">
            <div className="relative">
              <Search size={12} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none" />
              <input
                type="text"
                placeholder="Search profiles…"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                className="w-56 h-7 pl-7 pr-7 rounded-md bg-bg-input border border-border-strong/50 hover:border-border-strong focus:outline-none focus:ring-1 focus:ring-brand/60 focus:border-brand/60 text-[12px] text-text-base placeholder-text-muted/60 transition-colors"
              />
              {search && (
                <button
                  onClick={() => setSearch("")}
                  className="absolute right-2 top-1/2 -translate-y-1/2 text-text-muted hover:text-text-base transition-colors"
                >
                  <X size={11} />
                </button>
              )}
            </div>

            <button
              onClick={startCreate}
              className="flex items-center gap-1.5 h-7 px-2.5 rounded-md bg-brand hover:bg-brand-hover text-white text-[12px] font-medium transition-colors"
              title="New Profile"
            >
              <Plus size={12} /> New Profile
            </button>
          </div>
        </div>

        {selection.totalSelected > 0 && (
          <div className="flex items-center justify-between px-4 py-2 border-t border-border-strong/30 bg-brand/5">
            <span className="text-[12px] text-text-base">
              {selection.totalSelected} profile{selection.totalSelected === 1 ? "" : "s"} selected
              {bulkProgress && (
                <span className="ml-2 text-text-muted">
                  · Deleting {bulkProgress.done}/{bulkProgress.total}…
                </span>
              )}
            </span>
            <div className="flex items-center gap-2">
              <button
                onClick={selection.clearSelection}
                disabled={bulkDeleting}
                className="h-7 px-2.5 rounded-md text-[12px] text-text-muted hover:text-text-base hover:bg-bg-sidebar transition-colors disabled:opacity-50"
              >
                Clear selection
              </button>
              <button
                onClick={handleBulkDelete}
                disabled={bulkDeleting}
                className="flex items-center gap-1.5 h-7 px-2.5 rounded-md bg-danger/90 hover:bg-danger text-white text-[12px] font-medium transition-colors disabled:opacity-50 disabled:cursor-wait"
              >
                <X size={12} /> Delete selected
              </button>
            </div>
          </div>
        )}
      </div>

      {error && (
        <div className="bg-red-500/10 text-red-400 p-3 text-[13px] border-b border-red-500/20 flex items-center justify-between shrink-0">
          {error}
          <button onClick={() => setError(null)}><X size={14} /></button>
        </div>
      )}

      {/* Table */}
      <AssetTable
        items={filteredProfiles}
        getId={(name) => name}
        isEmpty={profiles.length === 0}
        emptyState={
          <>
            <div className="w-14 h-14 mx-auto mb-5 rounded-2xl bg-brand/12 border border-brand/20 flex items-center justify-center">
              <Layers size={22} className="text-brand" strokeWidth={1.5} />
            </div>
            <h2 className="text-[15px] font-medium text-text-base mb-2">No profiles yet</h2>
            <p className="text-[13px] text-text-muted leading-relaxed max-w-xs mb-6">
              A profile bundles skills, MCP servers, rules, hooks, sub-agents, commands and agents,
              and keeps every project that attaches it in step.
            </p>
            <button
              onClick={startCreate}
              className="flex items-center gap-2 px-4 py-2 bg-brand hover:bg-brand-hover text-white rounded-lg text-[13px] font-medium transition-colors"
            >
              <Plus size={14} /> New Profile
            </button>
          </>
        }
        noMatchState={
          <p className="text-[13px] text-text-muted">
            {searchLower ? `No profiles match "${search}".` : "No profiles yet."}
          </p>
        }
        columns={[
          { key: "icon", header: "", className: "w-11" },
          { key: "name", header: "Name" },
          { key: "summary", header: "" },
          { key: "actions", header: "", className: "w-16" },
        ]}
        renderRow={renderTableRow}
        selection={{
          allSelected: selection.allSelected,
          someSelected: selection.someSelected,
          disabled: selection.deletableItems.length === 0,
          onToggleAll: selection.toggleSelectAllVisible,
          ariaLabel: "Select all visible profiles",
        }}
        recentIds={recentIds}
      />

      {/* Drawer */}
      <AssetDrawer open={drawerOpen} onClose={closeDrawer} isEditing={dirty}>
        {profile && (
          <div className="flex-1 flex flex-col h-full">
            <div className="h-11 pl-6 pr-10 border-b border-border-strong/40 flex justify-between items-center">
              <div className="flex items-center gap-3">
                {isCreating ? (
                  <input
                    type="text"
                    placeholder="profile-name"
                    value={newName}
                    onChange={(e) => setNewName(e.target.value)}
                    autoFocus
                    className="bg-transparent border-none outline-none text-[14px] font-medium text-text-base placeholder-text-muted/50 w-64"
                  />
                ) : isRenaming ? (
                  <input
                    type="text"
                    value={renameName}
                    onChange={(e) => setRenameName(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") handleRename();
                      if (e.key === "Escape") setIsRenaming(false);
                    }}
                    onBlur={handleRename}
                    autoFocus
                    className="bg-transparent border-none outline-none text-[14px] font-medium text-text-base placeholder-text-muted/50 w-64"
                  />
                ) : (
                  <h3
                    className="text-[14px] font-medium text-text-base cursor-text"
                    onDoubleClick={startRename}
                    title="Double-click to rename"
                  >
                    {selectedName}
                  </h3>
                )}
              </div>

              <div className="flex items-center gap-2">
                {saveStatus && (
                  <span className={`text-[12px] ${saveStatus === "saving" ? "text-text-muted" : "text-icon-skill"}`}>
                    {saveStatus === "saving" ? "Saving..." : saveStatus}
                  </span>
                )}
                {attachStatus && (
                  <span className="text-[12px] text-icon-skill">{attachStatus}</span>
                )}
                {!isCreating && selectedName && (
                  <>
                    <button
                      onClick={handleDuplicate}
                      className="flex h-[26px] items-center gap-1.5 px-2.5 bg-bg-input hover:bg-surface-hover text-text-base rounded text-[11px] font-medium border border-border-strong transition-colors shadow-sm"
                    >
                      <Copy size={12} /> Duplicate
                    </button>
                    <button
                      onClick={() => {
                        setAttachTarget(null);
                        setShowAttachPicker(true);
                      }}
                      className="flex h-[26px] items-center gap-1.5 px-2.5 bg-brand hover:bg-brand-hover text-white rounded text-[11px] font-medium transition-colors shadow-sm"
                    >
                      Attach to project...
                    </button>
                  </>
                )}
                {dirty && (
                  <button
                    onClick={handleSave}
                    disabled={isCreating && !newName.trim()}
                    className="flex h-[26px] items-center gap-1.5 px-2.5 bg-brand hover:bg-brand-hover text-white rounded text-[11px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-sm border border-transparent"
                  >
                    <Check size={12} /> Save
                  </button>
                )}
              </div>
            </div>

            <div className="flex-1 overflow-y-auto p-6 custom-scrollbar" onClick={() => setConfirmDelete(null)}>
              <div className="max-w-2xl space-y-8">
                <section className="pb-2 border-b border-border-strong/40">
                  <AuthorSection descriptor={(profile._author as AuthorDescriptor | undefined) ?? { type: "local" }} />
                </section>

                <div>
                  <label className="block text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-2">
                    Description
                  </label>
                  <textarea
                    value={profile.description}
                    onChange={(e) => updateField("description", e.target.value)}
                    placeholder="What is this profile for? Which projects should attach it?"
                    rows={2}
                    className="w-full bg-bg-input border border-border-strong/40 hover:border-border-strong focus:border-brand rounded-md px-3 py-2 text-[13px] text-text-base placeholder-text-muted/40 outline-none resize-none transition-colors"
                  />
                </div>

                <AgentSelector
                  agentIds={profile.agents}
                  availableAgents={availableAgents}
                  onAdd={(id) => addItem("agents", id)}
                  onRemove={(idx) => removeItem("agents", idx)}
                />

                <SubAgentSelector
                  agentIds={profile.user_agents}
                  available={availableUserAgents}
                  onAdd={(id) => addItem("user_agents", id)}
                  onRemove={(idx) => removeItem("user_agents", idx)}
                />

                <CommandSelector
                  commandIds={profile.user_commands}
                  available={availableUserCommands}
                  onAdd={(id) => addItem("user_commands", id)}
                  onRemove={(idx) => removeItem("user_commands", idx)}
                />

                <HookSelector
                  hookIds={profile.hooks}
                  available={availableHooks}
                  onAdd={(id) => addItem("hooks", id)}
                  onRemove={(idx) => removeItem("hooks", idx)}
                />

                {/* Rules */}
                <div>
                  <div className="flex items-center gap-2 mb-2">
                    <ScrollText size={12} className="text-accent-hover" />
                    <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Rules</span>
                    {profile.rules.length > 0 && (
                      <span className="text-[10px] text-accent-hover bg-accent-hover/10 px-1.5 py-0.5 rounded">
                        {profile.rules.length}
                      </span>
                    )}
                  </div>
                  {availableRules.length > 0 ? (
                    <div className="flex flex-wrap gap-1.5">
                      {availableRules.map((rule) => {
                        const isSelected = profile.rules.includes(rule.id);
                        return (
                          <button
                            key={rule.id}
                            onClick={() => {
                              const updated = isSelected
                                ? profile.rules.filter((r) => r !== rule.id)
                                : [...profile.rules, rule.id];
                              updateField("rules", updated);
                            }}
                            className={`px-2.5 py-1 text-[12px] rounded border transition-colors flex items-center gap-1.5 ${
                              isSelected
                                ? "bg-bg-sidebar border-brand/40 font-medium text-text-base"
                                : "bg-bg-sidebar border-border-strong/40 text-text-muted hover:text-text-base hover:border-border-strong"
                            }`}
                          >
                            <ScrollText size={10} className={isSelected ? "text-brand" : ""} />
                            {rule.name}
                            {isSelected && <Check size={10} className="text-brand" />}
                          </button>
                        );
                      })}
                    </div>
                  ) : (
                    <p className="text-[11px] text-text-muted italic">
                      No rules created yet. Create rules in the Rules section to attach them here.
                    </p>
                  )}
                </div>

                <SkillSelector
                  skills={profile.skills}
                  availableSkills={availableSkills}
                  onAdd={(s) => addItem("skills", s)}
                  onRemove={(idx) => removeItem("skills", idx)}
                  showRemoveButtonAlways
                />

                <McpSelector
                  servers={profile.mcp_servers}
                  availableServers={availableMcpServers}
                  onAdd={(s) => addItem("mcp_servers", s)}
                  onRemove={(idx) => removeItem("mcp_servers", idx)}
                  showRemoveButtonAlways
                />

                {/* Delete */}
                {!isCreating && selectedName && !dirty && (
                  <div className="pt-2 border-t border-border-strong/40 flex items-center justify-between">
                    <p className="text-[11px] text-text-muted">
                      {referencingProjects.length > 0
                        ? `Deleting this profile removes what it added from ${referencingProjects.length} project${referencingProjects.length === 1 ? "" : "s"}.`
                        : "This profile is not attached to any project."}
                    </p>
                    {confirmDelete === selectedName ? (
                      <div className="flex items-center gap-2">
                        <span className="text-[11px] text-text-muted">Are you sure?</span>
                        <button
                          onClick={() => handleDelete(selectedName)}
                          className="px-2.5 py-1 text-[12px] font-medium text-white bg-danger hover:bg-danger-hover rounded transition-colors"
                        >
                          Delete
                        </button>
                        <button
                          onClick={() => setConfirmDelete(null)}
                          className="px-2.5 py-1 text-[12px] text-text-muted hover:text-text-base bg-bg-sidebar hover:bg-surface rounded transition-colors"
                        >
                          Cancel
                        </button>
                      </div>
                    ) : (
                      <button
                        onClick={(e) => { e.stopPropagation(); handleDelete(selectedName); }}
                        className="flex items-center gap-1.5 px-3 py-1.5 text-text-muted hover:text-danger text-[12px] transition-colors"
                      >
                        <Trash2 size={12} /> Delete
                      </button>
                    )}
                  </div>
                )}
              </div>
            </div>

            {/* Used by projects, pinned at the bottom */}
            {!isCreating && referencingProjects.length > 0 && (
              <div className="flex-shrink-0 border-t border-border-strong/40 px-6 py-4 bg-bg-input/30">
                <div className="flex items-center gap-2 mb-3">
                  <FolderGit2 size={13} className="text-text-muted" />
                  <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
                    Used in {referencingProjects.length} {referencingProjects.length === 1 ? "project" : "projects"}
                  </span>
                </div>
                <ul className="space-y-1.5 max-h-[108px] overflow-y-auto custom-scrollbar">
                  {referencingProjects.map((p) => (
                    <li key={p.name} className="flex items-center justify-between gap-3 py-1">
                      <button
                        onClick={() => onNavigateToProject?.(p.name)}
                        className="flex items-center gap-2 min-w-0 text-left hover:text-brand transition-colors"
                        title={p.directory || undefined}
                      >
                        <Folder size={12} className="text-text-muted flex-shrink-0" />
                        <span className="text-[13px] text-text-base truncate">{p.name}</span>
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            )}
          </div>
        )}
      </AssetDrawer>

      {showAttachPicker && profile && (
        <AttachToProjectModal
          projects={allProjects}
          attachedProjectNames={referencingProjects.map((p) => p.name)}
          selected={attachTarget}
          onSelect={setAttachTarget}
          onCancel={() => {
            setShowAttachPicker(false);
            setAttachTarget(null);
          }}
          onConfirm={() => {
            if (!attachTarget) return;
            const target = attachTarget;
            setAttachTarget(null);
            attachToProject(target);
          }}
        />
      )}
    </div>
  );
}

function AttachToProjectModal({
  projects,
  attachedProjectNames,
  selected,
  onSelect,
  onCancel,
  onConfirm,
}: {
  projects: ProjectSummary[];
  attachedProjectNames: string[];
  selected: string | null;
  onSelect: (name: string) => void;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const [filter, setFilter] = useState("");
  const trimmed = filter.trim().toLowerCase();
  const visible = trimmed
    ? projects.filter((p) => p.name.toLowerCase().includes(trimmed))
    : projects;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/50" onClick={onCancel} />
      <div className="relative bg-bg-input border border-border-strong rounded-xl shadow-2xl w-full max-w-md mx-4 flex flex-col max-h-[80vh]">
        <div className="flex items-center justify-between px-5 py-4 border-b border-border-strong/40 flex-shrink-0">
          <h2 className="text-[15px] font-semibold text-text-base">Attach Profile to Project</h2>
          <button
            onClick={onCancel}
            className="p-1 text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
          >
            <X size={16} />
          </button>
        </div>

        <div className="px-5 pt-3 pb-2 flex-shrink-0">
          <p className="text-[12px] text-text-muted leading-relaxed mb-3">
            Select a project. The profile's entries are added now and kept in step whenever the profile is saved.
            Anything the project already has stays its own.
          </p>
          {projects.length > 0 && (
            <div className="flex items-center gap-2 px-3 py-2 bg-bg-base border border-border-strong/40 rounded-md">
              <Search size={12} className="text-text-muted shrink-0" />
              <input
                type="text"
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
                placeholder="Filter projects..."
                autoFocus
                className="flex-1 bg-transparent outline-none text-[13px] text-text-base placeholder-text-muted/50"
              />
              {filter && (
                <button onClick={() => setFilter("")} className="text-text-muted hover:text-text-base transition-colors">
                  <X size={11} />
                </button>
              )}
            </div>
          )}
        </div>

        <div className="flex-1 overflow-y-auto custom-scrollbar px-3 pb-3 min-h-0">
          {projects.length === 0 ? (
            <div className="px-3 py-8 text-[12px] text-text-muted text-center">
              No projects yet. Create a project first.
            </div>
          ) : visible.length === 0 ? (
            <div className="px-3 py-8 text-[12px] text-text-muted text-center">
              No projects match.
            </div>
          ) : (
            <ul className="space-y-1">
              {visible.map((p) => {
                const isSelected = selected === p.name;
                const alreadyAttached = attachedProjectNames.includes(p.name);
                return (
                  <li key={p.name}>
                    <button
                      onClick={() => { if (!alreadyAttached) onSelect(p.name); }}
                      onDoubleClick={() => { if (!alreadyAttached) { onSelect(p.name); onConfirm(); } }}
                      disabled={alreadyAttached}
                      className={`w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-left transition-colors ${
                        isSelected
                          ? "bg-brand/15 border border-brand/40"
                          : "border border-transparent hover:bg-bg-sidebar"
                      } disabled:cursor-default disabled:hover:bg-transparent`}
                    >
                      <Folder size={14} className={isSelected ? "text-brand flex-shrink-0" : "text-text-muted flex-shrink-0"} />
                      <div className="flex-1 min-w-0">
                        <div className="text-[13px] font-medium text-text-base truncate">{p.name}</div>
                        {p.directory && (
                          <div className="text-[11px] text-text-muted truncate">{p.directory}</div>
                        )}
                      </div>
                      {alreadyAttached && (
                        <span className="flex items-center gap-1 text-[10px] text-icon-skill flex-shrink-0">
                          <Check size={11} /> Attached
                        </span>
                      )}
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>

        <div className="flex items-center justify-end gap-2 px-5 py-3 border-t border-border-strong/40 flex-shrink-0">
          <button
            onClick={onCancel}
            className="flex h-[28px] items-center px-3 text-[12px] text-text-muted hover:text-text-base bg-bg-sidebar hover:bg-surface rounded transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={onConfirm}
            disabled={!selected}
            className="flex h-[28px] items-center gap-1.5 px-3 bg-brand hover:bg-brand-hover text-white rounded text-[12px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-sm"
          >
            <Check size={12} /> Attach
          </button>
        </div>
      </div>
    </div>
  );
}
