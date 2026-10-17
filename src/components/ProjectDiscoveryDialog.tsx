import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { homeDir } from "@tauri-apps/api/path";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { X, FolderSearch, Loader2, AlertTriangle, CheckCircle2, FileText } from "lucide-react";
import { AgentIcon } from "./AgentIcon";
import { useCurrentUser } from "../contexts/ProfileContext";

/** Mirrors `core::DirectoryStatus` in src-tauri/src/core/projects.rs. */
type DirectoryStatus =
  | { kind: "Available" }
  | { kind: "RegisteredHere"; name: string; local_key: string }
  | { kind: "OrphanConfig"; name: string };

/** Mirrors `core::project_discovery::DiscoveredProject`. */
interface DiscoveredProject {
  directory: string;
  name: string;
  relative_path: string;
  agents: { id: string; label: string }[];
  has_agents_md: boolean;
  /** Null when the folder could not be checked; `check_error` says why. */
  status: DirectoryStatus | null;
  check_error: string | null;
}

/** Mirrors `core::project_discovery::DiscoveryReport`. */
interface DiscoveryReport {
  root: string;
  projects: DiscoveredProject[];
  visited: number;
  unreadable: number;
  truncated: boolean;
}

/** Mirrors `commands::projects::AddedDiscoveredProject`. */
interface AddedDiscoveredProject {
  directory: string;
  local_key: string | null;
  /** The project's name; an imported config keeps its own. */
  name: string | null;
  read_only: boolean;
  error: string | null;
}

/** New folders and folders with an unregistered config can both be added. */
function isAddable(project: DiscoveredProject): boolean {
  return project.status?.kind === "Available" || project.status?.kind === "OrphanConfig";
}

export interface AddedProject {
  name: string;
  localKey: string;
}

interface ProjectDiscoveryDialogProps {
  isOpen: boolean;
  /** When set, added projects also join this group. Shown as a note. */
  groupName?: string | null;
  onClose: () => void;
  /**
   * Called once after an add, with every project that was created. Resolves
   * to a message when a follow-up step (such as joining a group) failed.
   */
  onAdded: (added: AddedProject[]) => Promise<string | null> | void;
}

/**
 * Scan a folder tree for agent projects and add the ones the user picks.
 * The scan starts only when the user asks: scanning a home folder makes
 * macOS ask for access to Desktop, Documents and Downloads.
 */
export function ProjectDiscoveryDialog({ isOpen, groupName = null, onClose, onAdded }: ProjectDiscoveryDialogProps) {
  const { userId } = useCurrentUser();
  const [root, setRoot] = useState("");
  const [scanning, setScanning] = useState(false);
  const [report, setReport] = useState<DiscoveryReport | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [adding, setAdding] = useState(false);
  const [outcomes, setOutcomes] = useState<Map<string, AddedDiscoveredProject>>(new Map());
  const [error, setError] = useState<string | null>(null);
  // Bumped on every scan and on close, so a scan that finishes late cannot
  // overwrite newer state.
  const scanSeq = useRef(0);

  useEffect(() => {
    if (!isOpen) return;
    setReport(null);
    setSelected(new Set());
    setOutcomes(new Map());
    setError(null);
    homeDir()
      .then((home) => setRoot((current) => current || home))
      .catch(() => {
        // No home folder resolved: leave the field empty for the user to choose.
      });
  }, [isOpen]);

  const addable = useMemo(
    () =>
      (report?.projects ?? []).filter(
        (p) => isAddable(p) && !outcomes.get(p.directory)?.local_key,
      ),
    [report, outcomes],
  );

  if (!isOpen) return null;

  const busy = scanning || adding;

  const handleClose = () => {
    if (adding) return;
    scanSeq.current += 1;
    setScanning(false);
    onClose();
  };

  const chooseFolder = async () => {
    const picked = await openDialog({ multiple: false, directory: true, defaultPath: root || undefined });
    if (typeof picked === "string") setRoot(picked);
  };

  const scan = async () => {
    const folder = root.trim();
    if (!folder) return;
    const seq = ++scanSeq.current;
    setScanning(true);
    setError(null);
    setReport(null);
    setOutcomes(new Map());
    try {
      const result = await invoke<DiscoveryReport>("discover_projects", { root: folder });
      if (seq !== scanSeq.current) return;
      setReport(result);
      // Nothing starts ticked. Adding a folder writes agent config into it
      // and imports its MCP servers, so the user picks each one.
      setSelected(new Set());
    } catch (err) {
      if (seq === scanSeq.current) setError(`The scan failed: ${String(err)}`);
    } finally {
      if (seq === scanSeq.current) setScanning(false);
    }
  };

  const toggle = (directory: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(directory)) next.delete(directory);
      else next.add(directory);
      return next;
    });
  };

  const selectedAddable = addable.filter((p) => selected.has(p.directory));
  const allSelected = addable.length > 0 && selectedAddable.length === addable.length;

  const toggleAll = () => {
    setSelected(allSelected ? new Set() : new Set(addable.map((p) => p.directory)));
  };

  const addSelected = async () => {
    if (selectedAddable.length === 0) return;
    setAdding(true);
    setError(null);
    try {
      const results = await invoke<AddedDiscoveredProject[]>("add_discovered_projects", {
        directories: selectedAddable.map((p) => p.directory),
        createdBy: userId ?? null,
      });
      setOutcomes((prev) => {
        const next = new Map(prev);
        for (const r of results) next.set(r.directory, r);
        return next;
      });
      const byDirectory = new Map(selectedAddable.map((p) => [p.directory, p.name]));
      const added = results
        .filter((r): r is AddedDiscoveredProject & { local_key: string } => r.local_key !== null)
        .map((r) => ({ name: r.name ?? byDirectory.get(r.directory) ?? r.directory, localKey: r.local_key }));
      setSelected(new Set());
      if (added.length > 0) {
        const problem = await onAdded(added);
        if (problem) setError(problem);
      }
    } catch (err) {
      setError(`Adding projects failed: ${String(err)}`);
    } finally {
      setAdding(false);
    }
  };

  const addedCount = [...outcomes.values()].filter((o) => o.local_key).length;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/50" onClick={handleClose} />
      <div
        role="dialog"
        aria-label="Find projects"
        className="relative bg-bg-input border border-border-strong rounded-xl shadow-2xl w-full max-w-2xl mx-4 max-h-[85vh] flex flex-col"
      >
        <div className="flex items-center justify-between px-5 py-4 border-b border-border-strong/40">
          <div className="flex items-center gap-2">
            <FolderSearch size={16} className="text-text-muted" />
            <h2 className="text-[15px] font-semibold text-text-base">Find Projects</h2>
          </div>
          <button
            onClick={handleClose}
            disabled={adding}
            aria-label="Close"
            className="p-1 text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors disabled:opacity-50"
          >
            <X size={16} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto px-5 py-4 space-y-4">
          <p className="text-[13px] text-text-muted leading-relaxed">
            Choose a folder to search. Automatic looks for folders that hold agent files, such as{" "}
            <code className="font-mono text-[11px] bg-bg-sidebar px-1 rounded">CLAUDE.md</code> or{" "}
            <code className="font-mono text-[11px] bg-bg-sidebar px-1 rounded">AGENTS.md</code>. Nothing is
            changed until you add projects. Adding a project writes Automatic's files into its folder and
            imports the MCP servers it already uses.
          </p>
          {groupName && (
            <p className="text-[12px] text-text-muted">
              New projects will also join the group <span className="text-text-base">{groupName}</span>.
            </p>
          )}

          <div className="flex items-center gap-2">
            <input
              value={root}
              onChange={(e) => setRoot(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !busy) void scan();
              }}
              aria-label="Folder to search"
              spellCheck={false}
              className="flex-1 min-w-0 px-3 py-2 rounded-md bg-bg-sidebar border border-border-strong/40 hover:border-border-strong focus:border-brand outline-none text-[12px] text-text-base font-mono"
            />
            <button
              onClick={chooseFolder}
              disabled={busy}
              className="px-3 py-2 text-[12px] font-medium text-text-muted hover:text-text-base border border-border-strong/50 hover:bg-bg-sidebar rounded-md transition-colors disabled:opacity-50"
            >
              Choose…
            </button>
            <button
              onClick={scan}
              disabled={busy || !root.trim()}
              className="px-3 py-2 text-[12px] font-medium bg-brand hover:bg-brand-hover text-white rounded-md transition-colors disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-1.5"
            >
              {scanning && <Loader2 size={12} className="animate-spin" />}
              {scanning ? "Searching…" : "Search"}
            </button>
          </div>

          {error && (
            <div className="flex items-start gap-2 rounded-lg border border-danger/25 bg-danger/8 px-3 py-2">
              <AlertTriangle size={12} className="text-danger mt-0.5 shrink-0" />
              <p className="text-[12px] text-danger">{error}</p>
            </div>
          )}

          {scanning && (
            <p className="text-[12px] text-text-muted">
              Searching folders. Large folders can take a minute.
            </p>
          )}

          {report && <ScanSummary report={report} />}

          {report && report.projects.length > 0 && (
            <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden">
              {addable.length > 0 && (
                <label className="flex items-center gap-2 px-3 py-2 border-b border-border-strong/40 text-[12px] text-text-muted cursor-pointer">
                  <input type="checkbox" checked={allSelected} onChange={toggleAll} disabled={busy} />
                  Select all
                </label>
              )}
              <ul>
                {report.projects.map((p) => (
                  <DiscoveredRow
                    key={p.directory}
                    project={p}
                    checked={selected.has(p.directory)}
                    onToggle={() => toggle(p.directory)}
                    disabled={busy}
                    outcome={outcomes.get(p.directory)}
                  />
                ))}
              </ul>
            </div>
          )}
        </div>

        <div className="flex justify-end items-center gap-2 px-5 py-3 border-t border-border-strong/40">
          <button
            onClick={handleClose}
            disabled={adding}
            className="px-4 py-2 text-[13px] font-medium text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded-lg transition-colors disabled:opacity-50"
          >
            {addedCount > 0 ? "Done" : "Cancel"}
          </button>
          <button
            onClick={addSelected}
            disabled={busy || selectedAddable.length === 0}
            className="px-4 py-2 text-[13px] font-medium bg-brand hover:bg-brand-hover text-white rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-2"
          >
            {adding ? (
              <>
                <Loader2 size={14} className="animate-spin" />
                Adding…
              </>
            ) : (
              <>Add {selectedAddable.length > 0 ? `${selectedAddable.length} project${selectedAddable.length === 1 ? "" : "s"}` : "projects"}</>
            )}
          </button>
        </div>
      </div>
    </div>
  );
}

function ScanSummary({ report }: { report: DiscoveryReport }) {
  const count = report.projects.length;
  return (
    <div className="space-y-1">
      <p className="text-[12px] text-text-muted">
        {count === 0
          ? "No projects found in this folder."
          : `Found ${count} project${count === 1 ? "" : "s"}.`}
      </p>
      {report.truncated && (
        <p className="text-[12px] text-warning">
          The search stopped after {report.visited.toLocaleString()} folders. Choose a smaller folder to see
          everything in it.
        </p>
      )}
      {report.unreadable > 0 && (
        <p className="text-[12px] text-text-muted">
          {report.unreadable.toLocaleString()} folder{report.unreadable === 1 ? " was" : "s were"} skipped
          because Automatic could not open {report.unreadable === 1 ? "it" : "them"}.
        </p>
      )}
    </div>
  );
}

interface DiscoveredRowProps {
  project: DiscoveredProject;
  checked: boolean;
  onToggle: () => void;
  disabled: boolean;
  outcome: AddedDiscoveredProject | undefined;
}

function DiscoveredRow({ project, checked, onToggle, disabled, outcome }: DiscoveredRowProps) {
  const added = Boolean(outcome?.local_key);
  const available = isAddable(project) && !added;
  return (
    <li className="flex items-start gap-3 px-3 py-2 border-b border-border-strong/20 last:border-b-0">
      <input
        type="checkbox"
        className="mt-1"
        checked={available && checked}
        onChange={onToggle}
        disabled={disabled || !available}
        aria-label={`Add ${project.name}`}
      />
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span className="text-[13px] font-medium text-text-base truncate">{project.name || project.directory}</span>
          <div className="flex items-center gap-1 shrink-0">
            {project.agents.map((a) => (
              <span key={a.id} title={a.label}>
                <AgentIcon agentId={a.id} size={14} />
              </span>
            ))}
            {project.has_agents_md && (
              <span title="AGENTS.md" className="text-text-muted">
                <FileText size={12} />
              </span>
            )}
          </div>
        </div>
        <p className="text-[11px] text-text-muted font-mono truncate" title={project.directory}>
          {project.relative_path || project.directory}
        </p>
        <RowStatus project={project} outcome={outcome} />
      </div>
    </li>
  );
}

function RowStatus({ project, outcome }: { project: DiscoveredProject; outcome: AddedDiscoveredProject | undefined }) {
  if (outcome?.local_key) {
    return (
      <p className="flex items-center gap-1 text-[11px] text-success mt-0.5">
        <CheckCircle2 size={11} /> {outcome.read_only ? "Added as read only" : "Added"}
      </p>
    );
  }
  if (outcome?.error) {
    return <p className="text-[11px] text-danger mt-0.5">{outcome.error}</p>;
  }
  if (project.check_error) {
    return <p className="text-[11px] text-danger mt-0.5">Could not check this folder: {project.check_error}</p>;
  }
  if (project.status?.kind === "RegisteredHere") {
    return <p className="text-[11px] text-text-muted mt-0.5">Already added as {project.status.name}</p>;
  }
  if (project.status?.kind === "OrphanConfig") {
    return (
      <p className="text-[11px] text-text-muted mt-0.5">
        Has an existing Automatic config. It will be imported as{" "}
        <span className="text-text-base">{project.status.name}</span>.
      </p>
    );
  }
  return null;
}
