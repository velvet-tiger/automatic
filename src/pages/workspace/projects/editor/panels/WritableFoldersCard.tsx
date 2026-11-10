import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { homeDir } from "@tauri-apps/api/path";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { FolderPlus, X } from "lucide-react";
import type { Project, WritableFoldersFindings } from "../../types";
import { extraFolderState, folderName, folderParent, groupFolders, savedStatus } from "../../writableFolders";

/** The agent id the folders apply to. Matches `CLAUDE_AGENT` in the backend. */
const CLAUDE_AGENT = "claude";

/** What `set_project_writable_folders` returns. */
interface WritableFoldersSync {
  errors: string[];
}

interface WritableFoldersCardProps {
  project: Project;
  /** local_key of the open project. */
  projectKey: string;
  /** Reload the per-checkout fields and the notices after a change. */
  onChanged: () => Promise<void>;
  onNavigateToGroup?: (groupName: string) => void;
  /**
   * Changes whenever the editor's problems check reports again. The card
   * reads its findings again then, so a decision made in the notice list,
   * or a group change, shows here too.
   */
  checksVersion?: unknown;
}

/**
 * "Folders Claude can edit": the extra folders this checkout lets Claude
 * Code edit, the folders its groups add, and where they were saved. A change
 * is saved at once, because the list is stored only on this computer.
 * Findings that need action go to the notice list, not here.
 */
export function WritableFoldersCard({
  project,
  projectKey,
  onChanged,
  onNavigateToGroup,
  checksVersion,
}: WritableFoldersCardProps) {
  const usesClaude = project.agents.includes(CLAUDE_AGENT);
  const extras = project.local_writable_folders ?? [];
  const extrasKey = extras.join("\n");
  const [findings, setFindings] = useState<WritableFoldersFindings | null>(null);
  const [home, setHome] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadFindings = useCallback(async () => {
    try {
      const result = await invoke<WritableFoldersFindings | null>("read_project_writable_folders", { name: projectKey });
      setFindings(result);
    } catch (err) {
      setFindings(null);
      setError(`Could not check the folders Claude can edit: ${err}`);
    }
  }, [projectKey]);

  useEffect(() => {
    if (usesClaude) void loadFindings();
  }, [usesClaude, loadFindings, extrasKey, checksVersion]);

  useEffect(() => {
    homeDir()
      .then(setHome)
      .catch((err: unknown) => console.warn("Could not read the home folder:", err));
  }, []);

  if (!usesClaude) return null;

  const save = async (folders: string[]) => {
    setBusy(true);
    setError(null);
    try {
      const sync = await invoke<WritableFoldersSync>("set_project_writable_folders", { name: projectKey, folders });
      if (sync.errors.length > 0) setError(sync.errors.join(" "));
      await onChanged();
      await loadFindings();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const addFolder = async () => {
    let picked: string | string[] | null;
    try {
      picked = await openDialog({ multiple: false, directory: true });
    } catch (err) {
      setError(`Could not open the folder picker: ${err}`);
      return;
    }
    if (typeof picked !== "string" || extras.includes(picked)) return;
    await save([...extras, picked]);
  };

  const status = findings ? savedStatus(findings) : null;
  const groups = findings ? groupFolders(findings) : [];

  return (
    <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden">
      <div className="px-4 py-3 flex items-center justify-between gap-4">
        <div className="min-w-0">
          <div className="text-[13px] font-medium text-text-base">Folders Claude can edit</div>
          <div className="text-[11px] text-text-muted mt-0.5 leading-relaxed">
            Claude Code can refuse to edit anything outside this project&apos;s folder. Automatic tells it which other
            folders it may edit. This applies on this computer only.
          </div>
        </div>
        {status && (
          <div className="flex items-center gap-1.5 text-[11px] text-text-muted flex-shrink-0">
            <span className={`inline-block w-1.5 h-1.5 rounded-full ${status.ok ? "bg-success" : "bg-warning"}`} />
            {status.text}
          </div>
        )}
      </div>

      <div className="px-4 py-3 border-t border-border-strong/30">
        <div className="text-[11px] font-semibold text-text-base mb-1">Other folders</div>
        {extras.length === 0 && <div className="text-[11px] text-text-muted">None yet.</div>}
        <ul className="space-y-1.5">
          {extras.map((path) => {
            const state = extraFolderState(path, findings);
            return (
              <li key={path} className="flex items-center justify-between gap-3 px-2.5 py-1.5 rounded-md bg-bg-sidebar">
                <div className="min-w-0">
                  <div className="text-[12px] font-medium text-text-base truncate">{folderName(path)}</div>
                  <div className="text-[10px] text-text-muted opacity-75 truncate">{folderParent(path, home)}</div>
                </div>
                <div className="flex items-center gap-2 flex-shrink-0">
                  {state.kind === "missing" && (
                    <span className="text-[10px] px-1.5 rounded border border-warning/40 text-warning">Not on this computer</span>
                  )}
                  {state.kind === "refused" && (
                    <span
                      title={`Claude Code may not edit this folder, because ${state.reason}.`}
                      className="text-[10px] px-1.5 rounded border border-warning/40 text-warning"
                    >
                      Can&apos;t be shared
                    </span>
                  )}
                  <button
                    aria-label={`Remove ${folderName(path)}`}
                    title="Remove"
                    disabled={busy}
                    onClick={() => void save(extras.filter((p) => p !== path))}
                    className="text-text-muted hover:text-text-base disabled:opacity-50"
                  >
                    <X size={13} />
                  </button>
                </div>
              </li>
            );
          })}
        </ul>
        <button
          disabled={busy}
          onClick={() => void addFolder()}
          className="mt-2.5 flex items-center gap-1.5 px-3 py-1.5 text-[11px] font-medium rounded-md border border-border-strong/40 bg-bg-sidebar text-text-muted hover:text-text-base hover:border-border-strong/60 transition-colors disabled:opacity-50"
        >
          <FolderPlus size={12} />
          Add folder…
        </button>
        {error && <p className="mt-2 text-[11px] text-danger">{error}</p>}
      </div>

      {groups.length > 0 && (
        <div className="px-4 py-3 border-t border-border-strong/30">
          <div className="text-[11px] font-semibold text-text-base mb-1">From your groups</div>
          <ul className="space-y-0.5 text-[11px] text-text-muted">
            {groups.map((line) => (
              <li key={line.group}>
                <span className="font-medium text-text-base">{line.group}</span>
                {": "}
                {line.switchedOn ? (
                  line.projects.join(", ")
                ) : (
                  <>
                    off.{" "}
                    {onNavigateToGroup ? (
                      <button onClick={() => onNavigateToGroup(line.group)} className="text-brand underline">
                        Open the group
                      </button>
                    ) : (
                      "Open the group"
                    )}{" "}
                    to let agents edit across it.
                  </>
                )}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
