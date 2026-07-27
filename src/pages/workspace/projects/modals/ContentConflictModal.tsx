import { useEffect, useMemo } from "react";
import { X } from "lucide-react";
import { computeLineDiff, buildSideBySideDiffRows, type DiffLine } from "../diff";

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
  const diffLines = useMemo(
    () => (hasAutomaticContent ? computeLineDiff(automaticContent, diskContent) : null),
    [automaticContent, diskContent, hasAutomaticContent],
  );
  const sideBySideRows = useMemo(
    () => (diffLines ? buildSideBySideDiffRows(diffLines) : []),
    [diffLines],
  );
  const addedCount = diffLines?.filter((line) => line.type === "added").length ?? 0;
  const removedCount = diffLines?.filter((line) => line.type === "removed").length ?? 0;
  const diskLineCount = diskContent.split("\n").length;
  const automaticLineCount = hasAutomaticContent ? automaticContent.split("\n").length : 0;
  const noDiff = diffLines ? addedCount === 0 && removedCount === 0 : false;

  const renderDiffCell = (line: DiffLine | null, side: "left" | "right") => {
    const isBlank = line == null;
    const lineNumber = side === "left" ? line?.lineNo.a : line?.lineNo.b;
    const isChanged = line != null && line.type !== "same";
    const toneClass = isBlank
      ? "bg-bg-base/30 text-text-subtle/40"
      : line.type === "added"
        ? "bg-success/10 text-success"
        : line.type === "removed"
          ? "bg-danger/10 text-danger"
          : "bg-bg-base text-text-muted";

    return (
      <div
        className={`grid grid-cols-[3rem_1fr] border-b border-border-strong/20 last:border-b-0 ${toneClass}`}
      >
        <div className="select-none border-r border-border-strong/20 px-2 py-1 text-right text-[11px] text-border-strong">
          {lineNumber ?? ""}
        </div>
        <div
          className={`px-3 py-1 whitespace-pre-wrap break-words leading-relaxed ${
            isChanged && side === "left" && line?.type === "removed"
              ? "decoration-danger/40 line-through"
              : ""
          } ${isBlank ? "italic" : ""}`}
        >
          {isBlank ? " " : line?.content || " "}
        </div>
      </div>
    );
  };

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

          <div className="rounded-lg border border-border-strong/40 overflow-hidden">
            {!hasAutomaticContent ? (
              <>
                <div className="bg-bg-input px-3 py-2 flex items-center justify-between border-b border-border-strong/30">
                  <span className="text-[11px] font-medium text-text-muted uppercase tracking-wider">
                    Side-By-Side Comparison
                  </span>
                  <span className="text-[11px] text-text-muted">Automatic is empty on the left</span>
                </div>
                <div className="grid grid-cols-1 xl:grid-cols-2 max-h-[28rem] overflow-y-auto">
                  <div className="border-b border-border-strong/30 xl:border-b-0 xl:border-r xl:border-border-strong/30">
                    <div className="sticky top-0 z-10 flex items-center justify-between border-b border-border-strong/20 bg-danger/5 px-3 py-2">
                      <span className="text-[11px] font-medium uppercase tracking-wider text-danger">
                        Automatic
                      </span>
                      <span className="text-[11px] text-text-muted">0 lines</span>
                    </div>
                    <div className="bg-bg-base px-3 py-4 text-[12px] font-mono leading-relaxed text-text-subtle italic">
                      empty
                    </div>
                  </div>
                  <div>
                    <div className="sticky top-0 z-10 flex items-center justify-between border-b border-border-strong/20 bg-success/5 px-3 py-2">
                      <span className="text-[11px] font-medium uppercase tracking-wider text-success">
                        On Disk
                      </span>
                      <span className="text-[11px] text-text-muted">
                        {diskLineCount} line{diskLineCount !== 1 ? "s" : ""}
                      </span>
                    </div>
                    <pre className="bg-bg-base p-3 text-[12px] font-mono whitespace-pre-wrap leading-relaxed text-text-muted">
                      {diskContent.trim() || <em className="not-italic text-text-subtle">empty</em>}
                    </pre>
                  </div>
                </div>
              </>
            ) : noDiff ? (
              <>
                <div className="bg-bg-input px-3 py-2 flex items-center justify-between border-b border-border-strong/30">
                  <span className="text-[11px] font-medium text-text-muted uppercase tracking-wider">
                    Automatic And Disk Match
                  </span>
                  <span className="text-[11px] text-text-muted">
                    {diskLineCount} line{diskLineCount !== 1 ? "s" : ""}
                  </span>
                </div>
                <pre className="max-h-72 overflow-y-auto bg-bg-base p-3 text-[12px] font-mono whitespace-pre-wrap leading-relaxed text-text-muted">
                  {diskContent.trim() || <em className="not-italic text-text-subtle">empty</em>}
                </pre>
              </>
            ) : (
              <>
                <div className="bg-bg-input px-3 py-2 flex items-center justify-between border-b border-border-strong/30">
                  <span className="text-[11px] font-medium text-text-muted uppercase tracking-wider">
                    Side-By-Side Diff
                  </span>
                  <span className="text-[11px] text-text-muted flex items-center gap-2">
                    {addedCount > 0 && <span className="text-success">+{addedCount}</span>}
                    {removedCount > 0 && <span className="text-danger">−{removedCount}</span>}
                    <span className="text-border-strong/60">·</span>
                    <span>
                      {automaticLineCount} vs {diskLineCount} lines
                    </span>
                  </span>
                </div>
                <div className="grid grid-cols-1 xl:grid-cols-2 max-h-[28rem] overflow-y-auto">
                  <div className="border-b border-border-strong/30 xl:border-b-0 xl:border-r xl:border-border-strong/30">
                    <div className="sticky top-0 z-10 flex items-center justify-between border-b border-border-strong/20 bg-danger/5 px-3 py-2">
                      <span className="text-[11px] font-medium uppercase tracking-wider text-danger">
                        Automatic
                      </span>
                      <span className="text-[11px] text-text-muted">
                        {automaticLineCount} line{automaticLineCount !== 1 ? "s" : ""}
                      </span>
                    </div>
                    <div className="font-mono text-[12px]">
                      {sideBySideRows.map((row, idx) => (
                        <div key={`left-${idx}`}>{renderDiffCell(row.left, "left")}</div>
                      ))}
                    </div>
                  </div>
                  <div>
                    <div className="sticky top-0 z-10 flex items-center justify-between border-b border-border-strong/20 bg-success/5 px-3 py-2">
                      <span className="text-[11px] font-medium uppercase tracking-wider text-success">
                        On Disk
                      </span>
                      <span className="text-[11px] text-text-muted">
                        {diskLineCount} line{diskLineCount !== 1 ? "s" : ""}
                      </span>
                    </div>
                    <div className="font-mono text-[12px]">
                      {sideBySideRows.map((row, idx) => (
                        <div key={`right-${idx}`}>{renderDiffCell(row.right, "right")}</div>
                      ))}
                    </div>
                  </div>
                </div>
              </>
            )}
          </div>

          <div className="grid grid-cols-1 gap-2">
            <button
              onClick={() => onAdopt(diskContent)}
              className="flex flex-col items-start gap-0.5 px-4 py-3 rounded-lg border border-success/30 bg-success/5 hover:bg-success/10 hover:border-success/50 transition-colors text-left"
            >
              <span className="text-[13px] font-medium text-success">{adoptTitle}</span>
              <span className="text-[12px] text-text-muted">{adoptDescription}</span>
            </button>

            <button
              onClick={onOverwrite}
              className="flex flex-col items-start gap-0.5 px-4 py-3 rounded-lg border border-danger/30 bg-danger/5 hover:bg-danger/10 hover:border-danger/50 transition-colors text-left"
            >
              <span className="text-[13px] font-medium text-danger">{overwriteTitle}</span>
              <span className="text-[12px] text-text-muted">
                {hasAutomaticContent ? overwriteDescription : overwriteDescriptionEmpty}
              </span>
            </button>
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
