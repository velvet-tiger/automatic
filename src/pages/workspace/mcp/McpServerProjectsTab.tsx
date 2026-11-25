import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { FolderGit2, Layers, Plus, X } from "lucide-react";
import { smallInputClass } from "../../../components/KvField";

/** A registered checkout, as `get_project_summaries` returns it. */
export interface McpCheckout {
  local_key: string;
  id: string;
  name: string;
  directory: string;
}

/** How one project uses the server. Mirrors `core::McpServerProjectUse`. */
interface McpServerProjectUse {
  name: string;
  local_key?: string;
  directory: string;
  enabled: boolean;
  /** Profiles that provide the server here. Empty means the project added it. */
  profiles: string[];
  read_only: boolean;
}

/** Mirrors `core::McpServerUsage`. */
interface McpServerUsage {
  projects: McpServerProjectUse[];
  profiles: string[];
}

interface McpServerProjectsTabProps {
  serverName: string;
  /** Every project server: server name to the id of its project. */
  scopes: Record<string, string>;
  /** Project names by id, for projects a checkout here accepted. */
  projectNames: Record<string, string>;
  checkouts: McpCheckout[];
  /** Checkout local_key to the project id whose servers it accepted. */
  acceptedIds: Record<string, string>;
  scopeBusy: boolean;
  /** Unsaved edits block a scope change, which re-reads the server. */
  dirty: boolean;
  /** `localKey` limits the server to that checkout's project; null makes it global. */
  onChangeScope: (localKey: string | null) => Promise<void>;
  onError: (message: string) => void;
}

const sectionLabel = "block text-[11px] font-semibold text-text-muted tracking-wider uppercase mb-2";
const rowButton =
  "px-3 py-1.5 text-[12px] font-medium text-text-muted hover:text-text-base border border-border-strong/50 rounded transition-colors disabled:opacity-50";
const removeButton =
  "flex items-center gap-1 px-2 py-1 text-[11px] font-medium text-text-muted hover:text-danger rounded transition-colors disabled:opacity-50";

/** Name, plus the folder when another checkout shares the name. */
function checkoutLabel(checkout: { name: string; directory: string }, all: { name: string }[]): string {
  const shared = all.filter((c) => c.name === checkout.name).length > 1;
  return shared ? `${checkout.name} (${checkout.directory})` : checkout.name;
}

/**
 * The Projects tab of the MCP server editor: who may use the server, which
 * projects and profiles use it now, and controls to add or remove it.
 */
export function McpServerProjectsTab({
  serverName,
  scopes,
  projectNames,
  checkouts,
  acceptedIds,
  scopeBusy,
  dirty,
  onChangeScope,
  onError,
}: McpServerProjectsTabProps) {
  const [usage, setUsage] = useState<McpServerUsage | null>(null);
  const [allProfiles, setAllProfiles] = useState<string[]>([]);
  const [scopeTarget, setScopeTarget] = useState("");
  const [projectTarget, setProjectTarget] = useState("");
  const [profileTarget, setProfileTarget] = useState("");
  const [busy, setBusy] = useState(false);

  const scopeId = scopes[serverName];

  const loadUsage = useCallback(async () => {
    try {
      const [found, profiles] = await Promise.all([
        invoke<McpServerUsage>("get_mcp_server_usage", { name: serverName }),
        invoke<string[]>("get_project_profiles"),
      ]);
      setUsage(found);
      setAllProfiles(profiles);
    } catch (err: unknown) {
      onError(`Failed to load where this server is used: ${err}`);
    }
  }, [serverName, onError]);

  useEffect(() => {
    setUsage(null);
    setScopeTarget("");
    setProjectTarget("");
    setProfileTarget("");
    void loadUsage();
  }, [loadUsage]);

  /** Run one add or remove, then reload the lists. */
  const run = async (action: () => Promise<unknown>, failure: string) => {
    setBusy(true);
    try {
      await action();
    } catch (err: unknown) {
      onError(`${failure}: ${err}`);
    } finally {
      await loadUsage();
      setBusy(false);
    }
  };

  const addToProject = () =>
    run(async () => {
      await invoke("attach_mcp_server_to_project", { projectName: projectTarget, serverName });
      setProjectTarget("");
    }, "Failed to add the server to the project");

  const removeFromProject = async (project: McpServerProjectUse) => {
    if (!project.local_key) return;
    const confirmed = await ask(
      `Remove ${serverName} from ${project.name}? Automatic removes it from the project's agent config files when it syncs.`,
      { title: "Remove from project", kind: "warning" },
    );
    if (!confirmed) return;
    const key = project.local_key;
    await run(
      () => invoke("detach_mcp_server_from_project", { projectName: key, serverName }),
      "Failed to remove the server from the project",
    );
  };

  const addToProfile = () =>
    run(async () => {
      await invoke("add_mcp_server_to_profile", { profileName: profileTarget, serverName });
      setProfileTarget("");
    }, "Failed to add the server to the profile");

  const removeFromProfile = async (profile: string) => {
    const confirmed = await ask(
      `Remove ${serverName} from the ${profile} profile? Every project that uses the profile loses the server, unless it added the server itself.`,
      { title: "Remove from profile", kind: "warning" },
    );
    if (!confirmed) return;
    await run(
      () => invoke("remove_mcp_server_from_profile", { profileName: profile, serverName }),
      "Failed to remove the server from the profile",
    );
  };

  const sortedCheckouts = [...checkouts].sort((a, b) => a.name.localeCompare(b.name));
  const usedKeys = new Set((usage?.projects ?? []).map((p) => p.local_key).filter(Boolean));
  // A project server can only go to a checkout that accepted its project's servers.
  const addableCheckouts = sortedCheckouts.filter(
    (c) => !usedKeys.has(c.local_key) && (scopeId === undefined || acceptedIds[c.local_key] === scopeId),
  );
  const listedProfiles = new Set((usage?.profiles ?? []).map((p) => p.toLowerCase()));
  const addableProfiles = allProfiles
    .filter((p) => !listedProfiles.has(p.toLowerCase()))
    .sort((a, b) => a.localeCompare(b));

  return (
    <div className="space-y-8">
      {/* Who may use it */}
      <section>
        <label className={sectionLabel}>Available to</label>
        {scopeId !== undefined ? (
          <div className="flex items-center justify-between gap-3">
            <p className="text-[13px] text-text-base">
              {projectNames[scopeId] !== undefined ? (
                <>
                  Only <span className="font-medium">{projectNames[scopeId]}</span> can see this server.
                </>
              ) : (
                "This server belongs to a project that no folder on this machine uses yet. The project may have been removed, it may live on another machine, or a folder of it can still accept it from its project page. You can also make the server available to all projects, or delete it."
              )}
            </p>
            <button
              onClick={() => void onChangeScope(null)}
              disabled={scopeBusy || dirty}
              title={dirty ? "Save your changes first" : undefined}
              className={`flex-shrink-0 ${rowButton}`}
            >
              Make available to all projects
            </button>
          </div>
        ) : (
          <div className="flex items-center justify-between gap-3">
            <p className="text-[13px] text-text-base">All projects.</p>
            <div className="flex items-center gap-2 flex-shrink-0">
              <select
                value={scopeTarget}
                onChange={(e) => setScopeTarget(e.target.value)}
                disabled={scopeBusy || dirty}
                aria-label="Project to limit this server to"
                className={smallInputClass}
              >
                <option value="">Limit to one project…</option>
                {sortedCheckouts.map((checkout) => {
                  const label = checkoutLabel(checkout, checkouts);
                  // The backend refuses a checkout that has not accepted its id while the
                  // id owns servers or another checkout accepted it.
                  const blocked =
                    acceptedIds[checkout.local_key] !== checkout.id &&
                    (Object.values(scopes).includes(checkout.id) ||
                      Object.values(acceptedIds).includes(checkout.id));
                  return (
                    <option key={checkout.local_key} value={checkout.local_key} disabled={blocked}>
                      {blocked ? `${label}: accept its project servers first` : label}
                    </option>
                  );
                })}
              </select>
              <button
                onClick={() => void onChangeScope(scopeTarget).then(() => setScopeTarget(""))}
                disabled={!scopeTarget || scopeBusy || dirty}
                title={dirty ? "Save your changes first" : undefined}
                className={rowButton}
              >
                Limit
              </button>
            </div>
          </div>
        )}
      </section>

      {/* Projects that use it */}
      <section>
        <label className={sectionLabel}>
          <span className="flex items-center gap-1.5">
            <FolderGit2 size={12} /> Used in projects{usage ? ` (${usage.projects.length})` : ""}
          </span>
        </label>
        {usage === null ? (
          <p className="text-[12px] text-text-muted">Loading…</p>
        ) : usage.projects.length === 0 ? (
          <p className="text-[13px] text-text-muted">No project uses this server yet.</p>
        ) : (
          <ul className="rounded-lg border border-border-strong/40 bg-bg-input divide-y divide-border-strong/20">
            {usage.projects.map((project) => (
              <li key={project.local_key ?? project.directory} className="flex items-center justify-between gap-3 px-3 py-2">
                <div className="min-w-0">
                  <div className="flex items-center gap-2 min-w-0">
                    <span className="text-[13px] text-text-base truncate">{checkoutLabel(project, usage.projects)}</span>
                    {!project.enabled && (
                      <span className="shrink-0 px-1.5 py-0.5 rounded bg-surface text-text-muted text-[10px] font-medium">
                        Disabled
                      </span>
                    )}
                    {project.read_only && (
                      <span className="shrink-0 px-1.5 py-0.5 rounded bg-surface text-text-muted text-[10px] font-medium">
                        Read-only
                      </span>
                    )}
                  </div>
                  {project.profiles.length > 0 && (
                    <p className="text-[11px] text-text-muted truncate">
                      From profile {project.profiles.join(", ")}
                    </p>
                  )}
                </div>
                {project.profiles.length === 0 && !project.read_only && project.local_key && (
                  <button
                    onClick={() => void removeFromProject(project)}
                    disabled={busy}
                    className={removeButton}
                    aria-label={`Remove from ${project.name}`}
                  >
                    <X size={11} /> Remove
                  </button>
                )}
              </li>
            ))}
          </ul>
        )}
        <div className="mt-2 flex items-center gap-2">
          <select
            value={projectTarget}
            onChange={(e) => setProjectTarget(e.target.value)}
            disabled={busy || addableCheckouts.length === 0}
            aria-label="Project to add this server to"
            className={smallInputClass}
          >
            <option value="">
              {addableCheckouts.length === 0 ? "No other projects can use this server" : "Add to project…"}
            </option>
            {addableCheckouts.map((checkout) => (
              <option key={checkout.local_key} value={checkout.local_key}>
                {checkoutLabel(checkout, checkouts)}
              </option>
            ))}
          </select>
          <button onClick={() => void addToProject()} disabled={!projectTarget || busy} className={`flex items-center gap-1 ${rowButton}`}>
            <Plus size={12} /> Add
          </button>
        </div>
      </section>

      {/* Profiles that list it */}
      <section>
        <label className={sectionLabel}>
          <span className="flex items-center gap-1.5">
            <Layers size={12} /> In profiles{usage ? ` (${usage.profiles.length})` : ""}
          </span>
        </label>
        {usage !== null && usage.profiles.length === 0 && (
          <p className="text-[13px] text-text-muted">No profile includes this server.</p>
        )}
        {usage !== null && usage.profiles.length > 0 && (
          <ul className="rounded-lg border border-border-strong/40 bg-bg-input divide-y divide-border-strong/20">
            {usage.profiles.map((profile) => (
              <li key={profile} className="flex items-center justify-between gap-3 px-3 py-2">
                <span className="text-[13px] text-text-base truncate">{profile}</span>
                <button
                  onClick={() => void removeFromProfile(profile)}
                  disabled={busy}
                  className={removeButton}
                  aria-label={`Remove from profile ${profile}`}
                >
                  <X size={11} /> Remove
                </button>
              </li>
            ))}
          </ul>
        )}
        {scopeId !== undefined ? (
          <p className="mt-2 text-[11px] text-text-muted">
            Profiles are shared by every project, so a server limited to one project cannot go in a profile.
            Make it available to all projects first.
          </p>
        ) : (
          <div className="mt-2 flex items-center gap-2">
            <select
              value={profileTarget}
              onChange={(e) => setProfileTarget(e.target.value)}
              disabled={busy || addableProfiles.length === 0}
              aria-label="Profile to add this server to"
              className={smallInputClass}
            >
              <option value="">
                {addableProfiles.length === 0 ? "No other profiles" : "Add to profile…"}
              </option>
              {addableProfiles.map((profile) => (
                <option key={profile} value={profile}>
                  {profile}
                </option>
              ))}
            </select>
            <button onClick={() => void addToProfile()} disabled={!profileTarget || busy} className={`flex items-center gap-1 ${rowButton}`}>
              <Plus size={12} /> Add
            </button>
          </div>
        )}
      </section>
    </div>
  );
}
