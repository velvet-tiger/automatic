import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { homeDir } from "@tauri-apps/api/path";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Loader2, AlertTriangle, CheckCircle2, FileText } from "lucide-react";
import { AgentIcon } from "./AgentIcon";
import { useCurrentUser } from "../contexts/ProfileContext";

// Shared by the Find Projects dialog and the first-run wizard's projects step.

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

export interface AddedProject {
  name: string;
  localKey: string;
}

/** The outcome of one add: the projects created, and how many folders failed. */
export interface AddResult {
  added: AddedProject[];
  failedCount: number;
}

/** New folders and folders with an unregistered config can both be added. */
function isAddable(project: DiscoveredProject): boolean {
  return project.status?.kind === "Available" || project.status?.kind === "OrphanConfig";
}

/**
 * State and actions for searching a folder tree and adding the folders the
 * user ticks. State resets each time `active` turns true. The scan starts
 * only when the user asks: scanning a home folder makes macOS ask for access
 * to Desktop, Documents and Downloads.
 */
export function useProjectDiscovery(active: boolean) {
  const { userId } = useCurrentUser();
  const [root, setRoot] = useState("");
  const [scanning, setScanning] = useState(false);
  const [report, setReport] = useState<DiscoveryReport | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [adding, setAdding] = useState(false);
  const [outcomes, setOutcomes] = useState<Map<string, AddedDiscoveredProject>>(new Map());
  const [error, setError] = useState<string | null>(null);
  // Bumped on every scan and on cancel, so a scan that finishes late cannot
  // overwrite newer state.
  const scanSeq = useRef(0);

  useEffect(() => {
    if (!active) return;
    setReport(null);
    setSelected(new Set());
    setOutcomes(new Map());
    setError(null);
    homeDir()
      .then((home) => setRoot((current) => current || home))
      .catch(() => {
        // No home folder resolved: leave the field empty for the user to choose.
      });
  }, [active]);

  const addable = useMemo(
    () => (report?.projects ?? []).filter((p) => isAddable(p) && !outcomes.get(p.directory)?.local_key),
    [report, outcomes],
  );
  const selectedAddable = addable.filter((p) => selected.has(p.directory));
  const allSelected = addable.length > 0 && selectedAddable.length === addable.length;
  const addedCount = [...outcomes.values()].filter((o) => o.local_key).length;
  const busy = scanning || adding;

  /** Drop any scan in flight; its result will be ignored. */
  const cancelScan = () => {
    scanSeq.current += 1;
    setScanning(false);
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

  const toggleAll = () => {
    setSelected(allSelected ? new Set() : new Set(addable.map((p) => p.directory)));
  };

  /**
   * Add the ticked folders. Each folder's result is shown on its row.
   * Returns null when nothing was ticked or the call itself failed (the
   * error is then shown).
   */
  const addSelected = async (): Promise<AddResult | null> => {
    if (selectedAddable.length === 0) return null;
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
      // Failed folders stay ticked so the user can retry or untick them.
      const failed = new Set(results.filter((r) => r.local_key === null).map((r) => r.directory));
      setSelected(failed);
      return { added, failedCount: failed.size };
    } catch (err) {
      setError(`Adding projects failed: ${String(err)}`);
      return null;
    } finally {
      setAdding(false);
    }
  };

  return {
    root,
    setRoot,
    scanning,
    report,
    selected,
    adding,
    outcomes,
    error,
    setError,
    addable,
    selectedAddable,
    allSelected,
    addedCount,
    busy,
    cancelScan,
    chooseFolder,
    scan,
    toggle,
    toggleAll,
    addSelected,
  };
}

export type ProjectDiscoveryState = ReturnType<typeof useProjectDiscovery>;

/**
 * Tell the rest of the app that projects were added: the sidebar reloads on
 * `project-added`, the Projects list on `project-updated`. One event of each
 * covers a whole batch; both listeners reload everything.
 */
export function announceAddedProjects(added: AddedProject[]): void {
  const last = added[added.length - 1];
  if (!last) return;
  window.dispatchEvent(new CustomEvent("project-added", { detail: { name: last.name, local_key: last.localKey } }));
  window.dispatchEvent(new CustomEvent("project-updated"));
}

/**
 * The folder field, Search button, error, summary and results list.
 * `scrollList` caps the list's height for hosts that cannot scroll, such as
 * the full-screen first-run wizard.
 */
export function ProjectDiscoveryPanel({ state, scrollList = false }: { state: ProjectDiscoveryState; scrollList?: boolean }) {
  const { root, setRoot, scanning, report, selected, outcomes, error, addable, allSelected, busy } = state;
  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2">
        <input
          value={root}
          onChange={(e) => setRoot(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !busy) void state.scan();
          }}
          aria-label="Folder to search"
          spellCheck={false}
          className="flex-1 min-w-0 px-3 py-2 rounded-md bg-bg-sidebar border border-border-strong/40 hover:border-border-strong focus:border-brand outline-none text-[12px] text-text-base font-mono"
        />
        <button
          onClick={state.chooseFolder}
          disabled={busy}
          className="px-3 py-2 text-[12px] font-medium text-text-muted hover:text-text-base border border-border-strong/50 hover:bg-bg-sidebar rounded-md transition-colors disabled:opacity-50"
        >
          Choose…
        </button>
        <button
          onClick={state.scan}
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

      {scanning && <p className="text-[12px] text-text-muted">Searching folders. Large folders can take a minute.</p>}

      {report && <ScanSummary report={report} />}

      {report && report.projects.length > 0 && (
        <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden">
          {addable.length > 0 && (
            <label className="flex items-center gap-2 px-3 py-2 border-b border-border-strong/40 text-[12px] text-text-muted cursor-pointer">
              <input type="checkbox" checked={allSelected} onChange={state.toggleAll} disabled={busy} />
              Select all
            </label>
          )}
          <ul className={scrollList ? "max-h-[32vh] overflow-y-auto" : undefined}>
            {report.projects.map((p) => (
              <DiscoveredRow
                key={p.directory}
                project={p}
                checked={selected.has(p.directory)}
                onToggle={() => state.toggle(p.directory)}
                disabled={busy}
                outcome={outcomes.get(p.directory)}
              />
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

function ScanSummary({ report }: { report: DiscoveryReport }) {
  const count = report.projects.length;
  return (
    <div className="space-y-1">
      <p className="text-[12px] text-text-muted">
        {count === 0 ? "No projects found in this folder." : `Found ${count} project${count === 1 ? "" : "s"}.`}
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
