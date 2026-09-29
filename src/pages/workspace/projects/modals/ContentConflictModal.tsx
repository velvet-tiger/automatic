import { useEffect } from "react";
import { X } from "lucide-react";
import { SideBySideDiff, DiffColumnAction } from "../SideBySideDiff";

export interface ContentConflictModalProps {
  /** Header eyebrow, e.g. "Instruction File Conflict" or "Project Skill Conflict". */
  kindLabel: string;
  /** Subject shown in the header mono span (filename or skill name). */
  subject: string;
  diskContent: string;
  automaticContent: string;
  /** Primary action — favours on-disk content. */
  onAdopt: (adoptedContent: string) => void;
  onOverwrite: () => void;
  onClose: () => void;
  adoptTitle?: string;
  adoptDescription?: string;
  overwriteTitle?: string;
  overwriteDescription?: string;
  overwriteDescriptionEmpty?: string;
  modifiedMessage?: string;
}

/**
 * Side-by-side comparison dialog used when on-disk content diverges from
 * Automatic's stored copy. Favours adopting the on-disk version.
 */
export function ContentConflictModal({
  kindLabel,
  subject,
  diskContent,
  automaticContent,
  onAdopt,
  onOverwrite,
  onClose,
  adoptTitle = "Use existing file",
  adoptDescription = "Keep the on-disk content and load it into Automatic's editor.",
  overwriteTitle = "Overwrite with Automatic content",
  overwriteDescription = "Replace the on-disk file with Automatic's stored content.",
  overwriteDescriptionEmpty = "Discard external changes. Only Automatic's stored content will remain.",
  modifiedMessage,
}: ContentConflictModalProps) {
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onClose]);

  const hasAutomaticContent = automaticContent.trim().length > 0;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center"
      style={{ background: "rgba(0,0,0,0.6)" }}
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        className="flex flex-col bg-bg-sidebar border border-border-strong/40 rounded-xl shadow-2xl overflow-hidden"
        style={{ width: "min(1100px, 94vw)", maxHeight: "85vh" }}
      >
        <div className="flex items-center justify-between px-5 py-3 border-b border-border-strong flex-shrink-0">
          <div className="flex items-center gap-3">
            <span className="text-[11px] font-medium text-warning/70 uppercase tracking-wider">
              {kindLabel}
            </span>
            <span className="text-border-strong">/</span>
            <span className="text-[13px] font-mono text-text-base">{subject}</span>
          </div>
          <button
            onClick={onClose}
            className="text-text-muted hover:text-text-base transition-colors"
          >
            <X size={16} />
          </button>
        </div>

        <div className="px-5 py-4 space-y-4 overflow-y-auto flex-1 min-h-0">
          <p className="text-[13px] text-text-base">
            <span className="font-mono text-warning">{subject}</span>
            {" "}
            {modifiedMessage ?? "has been modified outside Automatic."}
          </p>

          <SideBySideDiff
            leftLabel="Automatic"
            rightLabel="On Disk"
            leftContent={automaticContent}
            rightContent={diskContent}
            favoured="right"
          />

          <div className="grid grid-cols-2 gap-2">
            <DiffColumnAction
              tone="danger"
              title={overwriteTitle}
              description={hasAutomaticContent ? overwriteDescription : overwriteDescriptionEmpty}
              onClick={onOverwrite}
            />
            <DiffColumnAction
              tone="success"
              title={adoptTitle}
              description={adoptDescription}
              onClick={() => onAdopt(diskContent)}
            />
          </div>
        </div>

        <div className="flex items-center justify-end gap-2 px-5 py-3 border-t border-border-strong flex-shrink-0">
          <button
            onClick={onClose}
            className="px-3 py-1.5 text-[12px] text-text-muted hover:text-text-base transition-colors"
          >
            Dismiss
          </button>
        </div>
      </div>
    </div>
  );
}
