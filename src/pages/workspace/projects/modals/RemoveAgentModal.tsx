import { useEffect } from "react";
import { File, Folder, X } from "lucide-react";
import {
  groupAgentRemovalEntries,
  type AgentRemovalEntry,
  type AgentRemovalMode,
} from "../../../../lib/agentRemoval";

interface RemoveAgentModalProps {
  agentLabel: string;
  /** False when the project has no folder yet, so no file can change. */
  hasDirectory: boolean;
  /** What Remove would do. `null` while it is still loading. */
  preview: AgentRemovalEntry[] | null;
  /** Set when the preview could not be built; Remove is then unavailable. */
  previewError: string | null;
  busy: boolean;
  onChoose: (mode: AgentRemovalMode) => void;
  onClose: () => void;
}

function EntryRow({ entry, note }: { entry: AgentRemovalEntry; note: string | null }) {
  const Icon = entry.is_dir ? Folder : File;
  return (
    <li className="flex items-center gap-2 px-3 py-1.5">
      <Icon size={13} className="shrink-0 text-text-muted" />
      <span className="min-w-0 flex-1 truncate font-mono text-[12px] text-text-base" title={entry.path}>
        {entry.relative_path}
        {entry.is_dir ? "/" : ""}
      </span>
      {note && <span className="shrink-0 text-[11px] text-text-muted">{note}</span>}
    </li>
  );
}

function deletedNote(entry: AgentRemovalEntry): string | null {
  if (entry.action === "remove_empty_dir") return "Empty folder";
  return entry.is_dir ? "Whole folder" : null;
}

function EntryList({
  title,
  entries,
  note,
}: {
  title: string;
  entries: AgentRemovalEntry[];
  note: (entry: AgentRemovalEntry) => string | null;
}) {
  if (entries.length === 0) return null;
  return (
    <div className="space-y-1.5">
      <div className="text-[11px] font-semibold uppercase tracking-wider text-text-muted">{title}</div>
      <ul className="divide-y divide-border-strong/20 overflow-hidden rounded-lg border border-border-strong/40 bg-bg-input">
        {entries.map((entry) => (
          <EntryRow key={entry.path} entry={entry} note={note(entry)} />
        ))}
      </ul>
    </div>
  );
}

/**
 * Asks how to remove an agent from a project: Remove deletes its files,
 * Keep only stops syncing it. Lists exactly what Remove would change.
 */
export function RemoveAgentModal({
  agentLabel,
  hasDirectory,
  preview,
  previewError,
  busy,
  onChoose,
  onClose,
}: RemoveAgentModalProps) {
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !busy) onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [busy, onClose]);

  const groups = groupAgentRemovalEntries(preview ?? []);
  const hasChanges = groups.deleted.length > 0 || groups.stripped.length > 0;
  const removeDisabled = busy || (hasDirectory && (preview === null || previewError !== null));

  const renderBody = () => {
    if (!hasDirectory) {
      return (
        <p className="text-[13px] leading-relaxed text-text-base">
          This project has no folder yet, so no files will change.
        </p>
      );
    }
    if (previewError) {
      return (
        <div className="rounded-lg border border-danger/25 bg-danger/5 px-3 py-2 text-[12px] text-danger">
          {previewError}
        </div>
      );
    }
    if (preview === null) {
      return <p className="text-[12px] text-text-muted">Checking {agentLabel}'s files…</p>;
    }
    return (
      <div className="space-y-4">
        {hasChanges ? (
          <p className="text-[13px] leading-relaxed text-text-base">
            <span className="font-medium">Remove</span> deletes these files, including any you added inside
            these folders. This cannot be undone.
          </p>
        ) : (
          <p className="text-[13px] leading-relaxed text-text-base">
            Automatic found no {agentLabel} files to delete in this project.
          </p>
        )}
        <EntryList title="Deleted" entries={groups.deleted} note={deletedNote} />
        <EntryList
          title="Automatic's settings taken out, rest of the file kept"
          entries={groups.stripped}
          note={() => null}
        />
        <EntryList
          title="Left in place for other agents"
          entries={groups.kept}
          note={(entry) => `Used by ${entry.shared_with.join(", ")}`}
        />
      </div>
    );
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center"
      style={{ background: "rgba(0,0,0,0.6)" }}
      onClick={(e) => {
        if (e.target === e.currentTarget && !busy) onClose();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={`Remove ${agentLabel}`}
        className="flex flex-col overflow-hidden rounded-xl border border-border-strong/40 bg-bg-sidebar shadow-2xl"
        style={{ width: "min(620px, 92vw)", maxHeight: "85vh" }}
      >
        <div className="flex flex-shrink-0 items-center justify-between border-b border-border-strong px-5 py-3">
          <span className="text-[13px] font-medium text-text-base">Remove {agentLabel} from this project</span>
          <button
            onClick={onClose}
            disabled={busy}
            aria-label="Close"
            className="text-text-muted transition-colors hover:text-text-base disabled:opacity-40"
          >
            <X size={16} />
          </button>
        </div>

        <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-5 py-4">
          {renderBody()}
          {hasDirectory && (
            <p className="text-[12px] text-text-muted">
              <span className="font-medium text-text-base">Keep</span> stops syncing {agentLabel} and leaves all
              its files as they are.
            </p>
          )}
        </div>

        <div className="flex flex-shrink-0 items-center justify-end gap-2 border-t border-border-strong bg-bg-input px-5 py-3">
          <button
            onClick={onClose}
            disabled={busy}
            className="px-3 py-1.5 text-[12px] text-text-muted transition-colors hover:text-text-base disabled:opacity-40"
          >
            Cancel
          </button>
          {hasDirectory && (
            <button
              onClick={() => onChoose("keep")}
              disabled={busy}
              className="rounded border border-border-strong/50 bg-bg-sidebar px-3 py-1.5 text-[12px] font-medium text-text-base transition-colors hover:border-border-strong disabled:opacity-50"
            >
              Keep
            </button>
          )}
          <button
            onClick={() => onChoose("remove")}
            disabled={removeDisabled}
            className="rounded border border-danger/30 bg-danger/15 px-3 py-1.5 text-[12px] font-medium text-danger transition-colors hover:border-danger/50 hover:bg-danger/25 disabled:opacity-50"
          >
            {busy ? "Working…" : "Remove"}
          </button>
        </div>
      </div>
    </div>
  );
}
