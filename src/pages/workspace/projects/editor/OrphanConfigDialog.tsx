import { useEffect } from "react";
import { X, FolderInput, Trash2 } from "lucide-react";

interface OrphanConfigDialogProps {
  directory: string;
  existingName: string;
  busy: boolean;
  error: string | null;
  onImport: () => void;
  onStartFresh: () => void;
  onCancel: () => void;
}

export function OrphanConfigDialog({
  directory,
  existingName,
  busy,
  error,
  onImport,
  onStartFresh,
  onCancel,
}: OrphanConfigDialogProps) {
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !busy) onCancel();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [busy, onCancel]);

  const displayName = existingName.trim() || "(unnamed)";

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center"
      style={{ background: "rgba(0,0,0,0.6)" }}
      onClick={(e) => {
        if (e.target === e.currentTarget && !busy) onCancel();
      }}
    >
      <div
        className="flex flex-col bg-bg-sidebar border border-border-strong/40 rounded-xl shadow-2xl overflow-hidden"
        style={{ width: "min(560px, 92vw)", maxHeight: "85vh" }}
      >
        <div className="flex items-center justify-between px-5 py-3 border-b border-border-strong flex-shrink-0">
          <div className="flex items-center gap-3">
            <span className="text-[11px] font-medium text-warning/80 uppercase tracking-wider">
              Existing Configuration
            </span>
          </div>
          <button
            onClick={onCancel}
            disabled={busy}
            className="text-text-muted hover:text-text-base transition-colors disabled:opacity-40"
          >
            <X size={16} />
          </button>
        </div>

        <div className="px-5 py-4 space-y-4 overflow-y-auto flex-1 min-h-0">
          <p className="text-[13px] text-text-base leading-relaxed">
            This directory already has an Automatic configuration on disk, but
            it is not registered in Automatic.
          </p>

          <div className="rounded-lg border border-border-strong/40 bg-bg-input px-3 py-2 space-y-1">
            <div className="text-[11px] font-semibold uppercase tracking-wider text-text-muted">
              Directory
            </div>
            <div className="text-[12px] font-mono text-text-base break-all">
              {directory}
            </div>
            <div className="text-[11px] font-semibold uppercase tracking-wider text-text-muted pt-1">
              Project name in file
            </div>
            <div className="text-[12px] font-mono text-text-base">
              {displayName}
            </div>
          </div>

          <p className="text-[13px] text-text-base leading-relaxed">
            What do you want to do?
          </p>

          {error && (
            <div className="rounded-lg border border-danger/25 bg-danger/5 px-3 py-2 text-[12px] text-danger">
              {error}
            </div>
          )}
        </div>

        <div className="flex flex-col gap-2 px-5 py-3 border-t border-border-strong flex-shrink-0 bg-bg-input">
          <button
            onClick={onImport}
            disabled={busy}
            className="flex items-center justify-center gap-2 px-3 py-2 text-[12px] font-medium rounded bg-brand/15 text-brand border border-brand/30 hover:bg-brand/25 hover:border-brand/50 transition-colors disabled:opacity-50"
          >
            <FolderInput size={13} />
            {busy ? "Working…" : `Import '${displayName}' as-is`}
          </button>
          <button
            onClick={onStartFresh}
            disabled={busy}
            className="flex items-center justify-center gap-2 px-3 py-2 text-[12px] font-medium rounded bg-danger/10 text-danger border border-danger/25 hover:bg-danger/20 hover:border-danger/50 transition-colors disabled:opacity-50"
          >
            <Trash2 size={13} />
            {busy ? "Working…" : "Remove and start fresh"}
          </button>
          <button
            onClick={onCancel}
            disabled={busy}
            className="px-3 py-1.5 text-[12px] text-text-muted hover:text-text-base transition-colors disabled:opacity-40"
          >
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
