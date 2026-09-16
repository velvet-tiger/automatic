import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle, Layers, Plus, Search, X } from "lucide-react";
import type { ProfileContribution, Project } from "../../types";
import { PROFILE_RESOURCE_KINDS } from "../../types";

interface ProfilesPanelProps {
  project: Project;
  setProject: (next: Project) => void;
  setDirty: (v: boolean) => void;
  /** In the create wizard the project is not on disk yet, so attach/detach only edit state. */
  isCreating: boolean;
  selectedName: string | null;
  availableProfiles: string[];
  reloadProject: (name: string) => Promise<void>;
}

const KIND_LABELS: Record<(typeof PROFILE_RESOURCE_KINDS)[number], [string, string]> = {
  skills: ["skill", "skills"],
  mcp_servers: ["MCP server", "MCP servers"],
  providers: ["provider", "providers"],
  agents: ["agent", "agents"],
  user_agents: ["sub-agent", "sub-agents"],
  user_commands: ["command", "commands"],
  hooks: ["hook", "hooks"],
  rules: ["rule", "rules"],
};

/** "2 skills · 1 rule", or an explanation when the profile provides nothing. */
function summariseContribution(contribution: ProfileContribution | undefined): string {
  const parts: string[] = [];
  for (const kind of PROFILE_RESOURCE_KINDS) {
    const n = contribution?.[kind]?.length ?? 0;
    if (n === 0) continue;
    const [one, many] = KIND_LABELS[kind];
    parts.push(`${n} ${n === 1 ? one : many}`);
  }
  return parts.length > 0 ? parts.join(" · ") : "Adds nothing this project did not already have";
}

/**
 * Attach and detach profiles for one project. A profile keeps its entries
 * in step across every attached project; the entries it added here carry a
 * "Profile: name" badge in the other tabs.
 */
export function ProfilesPanel({
  project, setProject, setDirty, isCreating, selectedName, availableProfiles, reloadProject,
}: ProfilesPanelProps) {
  const [adding, setAdding] = useState(false);
  const [search, setSearch] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const attached = project.profiles ?? [];
  const contributions = project.profile_contributions ?? {};
  const unattached = availableProfiles.filter((name) => !attached.includes(name));
  const filtered = search.trim()
    ? unattached.filter((name) => name.toLowerCase().includes(search.toLowerCase()))
    : unattached;

  // Saved projects go through the backend so the profile is applied and the
  // project re-synced at once. The wizard only edits state; the first save
  // reconciles.
  const persisted = !isCreating && !!selectedName;

  const attach = async (profileName: string) => {
    setAdding(false);
    setSearch("");
    setError(null);
    if (!persisted) {
      setProject({ ...project, profiles: [...attached, profileName] });
      setDirty(true);
      return;
    }
    setBusy(profileName);
    try {
      await invoke("attach_profile_to_project", { projectName: selectedName, profileName });
      await reloadProject(selectedName!);
    } catch (err) {
      setError(`Failed to attach profile "${profileName}": ${err}`);
    } finally {
      setBusy(null);
    }
  };

  const detach = async (profileName: string) => {
    setError(null);
    if (!persisted) {
      setProject({ ...project, profiles: attached.filter((p) => p !== profileName) });
      setDirty(true);
      return;
    }
    setBusy(profileName);
    try {
      await invoke("detach_profile_from_project", { projectName: selectedName, profileName });
      await reloadProject(selectedName!);
    } catch (err) {
      setError(`Failed to detach profile "${profileName}": ${err}`);
    } finally {
      setBusy(null);
    }
  };

  return (
    <section className="flex gap-6">
      <div className="flex-1 min-w-0 space-y-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Layers size={13} className="text-text-muted" />
            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Profiles</span>
            {attached.length > 0 && (
              <span className="text-[10px] font-semibold px-1.5 py-0.5 rounded-full bg-bg-sidebar text-text-muted border border-border-strong/30 leading-none">
                {attached.length}
              </span>
            )}
          </div>
          <div className="relative">
            <button
              onClick={() => setAdding(!adding)}
              disabled={busy !== null}
              className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium disabled:opacity-50"
            >
              <Plus size={12} /> Attach from Library
            </button>
            {adding && (
              <div className="absolute right-0 top-full mt-1 w-72 bg-bg-sidebar border border-border-strong rounded-lg shadow-xl z-50 max-h-72 overflow-y-auto">
                <div className="p-2 border-b border-border-strong/40 flex items-center gap-2">
                  <Search size={12} className="text-text-muted shrink-0" />
                  <input
                    type="text"
                    value={search}
                    onChange={(e) => setSearch(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Escape") { setAdding(false); setSearch(""); }
                      if (e.key === "Enter" && filtered.length === 1) attach(filtered[0]!);
                    }}
                    placeholder="Search profiles..."
                    autoFocus
                    className="flex-1 bg-bg-input border border-border-strong/40 focus:border-brand rounded px-2 py-1 text-[12px] text-text-base placeholder-text-muted/50 outline-none"
                  />
                </div>
                <div className="py-1">
                  {filtered.length === 0 ? (
                    <div className="px-3 py-2 text-[12px] text-text-muted italic">
                      {availableProfiles.length === 0
                        ? "No profiles in the library yet."
                        : unattached.length === 0
                          ? "All profiles already attached."
                          : "No profiles match."}
                    </div>
                  ) : (
                    filtered.map((name) => (
                      <button
                        key={name}
                        onClick={() => attach(name)}
                        className="w-full flex items-center gap-2 px-3 py-2 hover:bg-bg-input text-left transition-colors"
                      >
                        <Layers size={14} className="text-text-muted flex-shrink-0" />
                        <span className="text-[12px] font-medium text-text-base truncate">{name}</span>
                      </button>
                    ))
                  )}
                </div>
              </div>
            )}
          </div>
        </div>

        <p className="text-[12px] text-text-muted">
          A profile keeps its skills, MCP servers, rules, hooks, sub-agents, commands and agents in step across every project that attaches it.
          Entries a profile provides carry a badge in the other tabs and are managed from the profile.
        </p>

        {error && (
          <div className="flex items-start gap-2 px-3 py-2 bg-danger/10 border border-danger/30 rounded-lg text-[12px] text-danger">
            <AlertTriangle size={13} className="flex-shrink-0 mt-0.5" />
            <span className="flex-1">{error}</span>
            <button onClick={() => setError(null)} className="text-danger/70 hover:text-danger"><X size={12} /></button>
          </div>
        )}

        {attached.length === 0 ? (
          <div className="px-4 py-6 bg-bg-input border border-border-strong/40 rounded-lg text-center">
            <Layers size={18} className="mx-auto mb-2 text-text-muted" strokeWidth={1.5} />
            <p className="text-[13px] text-text-muted mb-1">No profiles attached.</p>
            <p className="text-[12px] text-text-muted/70">Create profiles in the Profiles section of the Library, then attach them here.</p>
          </div>
        ) : (
          <div className="space-y-2">
            {attached.map((name) => {
              const missing = availableProfiles.length > 0 && !availableProfiles.includes(name);
              return (
                <div
                  key={name}
                  className="bg-bg-input border border-border-strong/40 rounded-lg group flex items-center gap-3 px-3 py-2.5 hover:border-border-strong transition-colors"
                >
                  <Layers size={14} className="flex-shrink-0 text-text-muted" />
                  <div className="flex-1 min-w-0">
                    <div className="text-[13px] font-medium text-text-base truncate flex items-center gap-2">
                      {name}
                      {missing && (
                        <span className="text-[10px] text-warning bg-warning/10 border border-warning/30 rounded px-1.5 py-0.5">
                          Missing from library
                        </span>
                      )}
                    </div>
                    <div className="text-[11px] text-text-muted truncate">
                      {persisted ? summariseContribution(contributions[name]) : "Applied when the project is saved"}
                    </div>
                  </div>
                  <button
                    onClick={() => detach(name)}
                    disabled={busy !== null}
                    className="p-1.5 text-text-muted hover:text-danger hover:bg-danger/10 rounded transition-colors flex-shrink-0 opacity-0 group-hover:opacity-100 disabled:opacity-50"
                    title="Detach profile"
                  >
                    <X size={12} />
                  </button>
                </div>
              );
            })}
          </div>
        )}
      </div>

      <div className="w-52 flex-shrink-0">
        <div className="rounded-md bg-bg-input border border-border-strong/30 px-3 py-2.5 text-[11px] text-text-muted space-y-2.5 sticky top-0">
          <div>
            <p className="font-medium text-text-base text-[12px]">How profiles work</p>
            <p className="leading-relaxed mt-1">
              Save a profile and every attached project is updated and re-synced.
              Detaching removes every entry the profile provides, including any this project had before attaching it. Entries no profile lists stay.
            </p>
          </div>
        </div>
      </div>
    </section>
  );
}
