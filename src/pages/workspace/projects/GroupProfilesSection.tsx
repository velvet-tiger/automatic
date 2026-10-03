import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle, Layers, Plus, Search, X } from "lucide-react";

interface GroupProfilesSectionProps {
  groupName: string;
  /** Names of the profiles attached to the group. */
  profiles: string[];
  /** Called after an attach or detach so the page can reload the group. */
  onChanged: () => void;
}

type GroupProfileCommand = "attach_profile_to_group" | "detach_profile_from_group";

/**
 * Profiles attached to a project group. Every member project receives them;
 * the backend updates and re-syncs the members as part of each attach or
 * detach. A member project cannot detach a profile its group provides.
 */
export function GroupProfilesSection({ groupName, profiles, onChanged }: GroupProfilesSectionProps) {
  const [library, setLibrary] = useState<string[]>([]);
  const [libraryLoaded, setLibraryLoaded] = useState(false);
  const [adding, setAdding] = useState(false);
  const [search, setSearch] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadLibrary = async () => {
    try {
      const names: string[] = await invoke("get_project_profiles");
      setLibrary([...names].sort((a, b) => a.localeCompare(b)));
      setLibraryLoaded(true);
    } catch (err) {
      setError(`Failed to load profiles: ${err}`);
    }
  };

  useEffect(() => {
    void loadLibrary();
    const handler = () => void loadLibrary();
    window.addEventListener("profiles-updated", handler);
    return () => window.removeEventListener("profiles-updated", handler);
  }, []);

  const unattached = library.filter((name) => !profiles.includes(name));
  const needle = search.trim().toLowerCase();
  const filtered = needle ? unattached.filter((name) => name.toLowerCase().includes(needle)) : unattached;

  const change = async (profileName: string, command: GroupProfileCommand) => {
    setAdding(false);
    setSearch("");
    setError(null);
    setBusy(true);
    try {
      await invoke(command, { groupName, profileName });
      onChanged();
    } catch (err) {
      const verb = command === "attach_profile_to_group" ? "attach" : "detach";
      setError(`Failed to ${verb} profile "${profileName}": ${err}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div>
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-2">
          <span className="text-[12px] font-semibold text-text-muted uppercase tracking-wider">Profiles</span>
          <span className="text-[11px] text-text-muted bg-bg-sidebar px-1.5 rounded">{profiles.length}</span>
        </div>
        <div className="relative">
          <button
            onClick={() => setAdding(!adding)}
            disabled={busy}
            className="flex items-center gap-1 text-[12px] text-brand hover:text-brand-hover transition-colors font-medium disabled:opacity-50"
          >
            <Plus size={12} /> Attach profile
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
                    if (e.key === "Enter" && filtered.length === 1) void change(filtered[0]!, "attach_profile_to_group");
                  }}
                  placeholder="Search profiles..."
                  aria-label="Search profiles"
                  autoFocus
                  className="flex-1 bg-bg-input border border-border-strong/40 focus:border-brand rounded px-2 py-1 text-[12px] text-text-base placeholder-text-muted/50 outline-none"
                />
              </div>
              <div className="py-1">
                {filtered.length === 0 ? (
                  <div className="px-3 py-2 text-[12px] text-text-muted italic">
                    {library.length === 0
                      ? "No profiles in the library yet."
                      : unattached.length === 0
                        ? "All profiles already attached."
                        : "No profiles match."}
                  </div>
                ) : (
                  filtered.map((name) => (
                    <button
                      key={name}
                      onClick={() => void change(name, "attach_profile_to_group")}
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

      {error && (
        <div className="mb-2 flex items-start gap-2 px-3 py-2 bg-danger/10 border border-danger/30 rounded-lg text-[12px] text-danger">
          <AlertTriangle size={13} className="flex-shrink-0 mt-0.5" />
          <span className="flex-1 break-words">{error}</span>
          <button onClick={() => setError(null)} className="text-danger/70 hover:text-danger" aria-label="Dismiss"><X size={12} /></button>
        </div>
      )}

      {profiles.length === 0 ? (
        <p className="text-[13px] text-text-muted italic opacity-60">
          No profiles attached. Every project in the group receives the profiles you attach here. They can only be removed here.
        </p>
      ) : (
        <ul className="space-y-1.5">
          {profiles.map((name) => {
            // Only once the library has loaded, so a slow load never flags every profile.
            const missing = libraryLoaded && !library.includes(name);
            return (
              <li
                key={name}
                className="flex items-center gap-2 px-3 py-1.5 rounded-md bg-bg-input border border-border-strong/30"
              >
                <Layers size={13} className="text-text-muted shrink-0" />
                <span className="flex-1 min-w-0 text-[13px] text-text-base truncate">
                  {name}
                  {missing && <span className="ml-2 text-[11px] text-text-muted">Missing from library</span>}
                </span>
                <button
                  onClick={() => void change(name, "detach_profile_from_group")}
                  disabled={busy}
                  className="flex items-center justify-center w-[20px] h-[20px] rounded text-text-muted hover:bg-red-500/10 hover:text-red-400 transition-colors shrink-0 disabled:opacity-50"
                  title={`Detach ${name} from the group`}
                  aria-label={`Detach ${name} from the group`}
                >
                  <X size={11} />
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
