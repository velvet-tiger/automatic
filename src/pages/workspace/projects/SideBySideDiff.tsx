import { useMemo, type ReactNode } from "react";
import { computeLineDiff, buildSideBySideDiffRows, type DiffLine } from "./diff";

export type DiffSide = "left" | "right";

export interface SideBySideDiffProps {
  leftLabel: string;
  rightLabel: string;
  leftContent: string;
  rightContent: string;
  /**
   * The side the user is expected to keep. Its changed lines render green
   * with "+"; the other side's changed lines render red, struck through,
   * with "−". Callers choose this per flow because the same left/right
   * layout means opposite things in drift (Automatic wins) and conflict
   * (disk wins) dialogs.
   */
  favoured: DiffSide;
}

function lineCount(content: string): number {
  return content.length === 0 ? 0 : content.split("\n").length;
}

function pluralLines(count: number): string {
  return `${count} line${count !== 1 ? "s" : ""}`;
}

/**
 * Two-column line diff with the left and right content kept level row by
 * row. Each row holds both cells, so a line that wraps on one side pushes
 * the other side down with it instead of drifting out of alignment.
 */
export function SideBySideDiff({
  leftLabel,
  rightLabel,
  leftContent,
  rightContent,
  favoured,
}: SideBySideDiffProps) {
  const hasLeftContent = leftContent.trim().length > 0;
  const diffLines = useMemo(
    () => (hasLeftContent ? computeLineDiff(leftContent, rightContent) : null),
    [leftContent, rightContent, hasLeftContent],
  );
  const rows = useMemo(() => (diffLines ? buildSideBySideDiffRows(diffLines) : []), [diffLines]);

  // computeLineDiff reports left-only lines as "removed" and right-only lines as "added".
  const leftOnlyCount = diffLines?.filter((line) => line.type === "removed").length ?? 0;
  const rightOnlyCount = diffLines?.filter((line) => line.type === "added").length ?? 0;
  const favouredCount = favoured === "left" ? leftOnlyCount : rightOnlyCount;
  const replacedCount = favoured === "left" ? rightOnlyCount : leftOnlyCount;
  const leftLineCount = lineCount(leftContent);
  const rightLineCount = lineCount(rightContent);
  const noDiff = diffLines != null && leftOnlyCount === 0 && rightOnlyCount === 0;

  const sideTone = (side: DiffSide) =>
    side === favoured
      ? { header: "bg-success/5 text-success", cell: "bg-success/10 text-success", sign: "+" }
      : { header: "bg-danger/5 text-danger", cell: "bg-danger/10 text-danger", sign: "−" };

  const renderColumnHeader = (side: DiffSide, label: string, count: number) => (
    <div
      className={`flex items-center justify-between px-3 py-2 ${sideTone(side).header} ${
        side === "left" ? "border-r border-border-strong/30" : ""
      }`}
    >
      <span className="text-[11px] font-medium uppercase tracking-wider">{label}</span>
      <span className="text-[11px] text-text-muted">{pluralLines(count)}</span>
    </div>
  );

  const renderCell = (line: DiffLine | null, side: DiffSide) => {
    const lineNumber = side === "left" ? line?.lineNo.a : line?.lineNo.b;
    const isChanged = line != null && line.type !== "same";
    const tone = sideTone(side);
    const toneClass =
      line == null ? "bg-bg-base/30" : isChanged ? tone.cell : "bg-bg-base text-text-muted";

    return (
      <div
        className={`grid grid-cols-[3rem_1.25rem_1fr] min-w-0 ${toneClass} ${
          side === "left" ? "border-r border-border-strong/30" : ""
        }`}
      >
        <div className="select-none border-r border-border-strong/20 px-2 py-1 text-right text-[11px] text-border-strong">
          {lineNumber ?? ""}
        </div>
        <div className="select-none py-1 text-center font-bold">{isChanged ? tone.sign : ""}</div>
        <div
          className={`min-w-0 pr-3 py-1 whitespace-pre-wrap break-words leading-relaxed ${
            isChanged && side !== favoured ? "line-through decoration-danger/40" : ""
          }`}
        >
          {line == null ? " " : line.content || " "}
        </div>
      </div>
    );
  };

  let body: ReactNode;
  if (!hasLeftContent) {
    body = (
      <div className="grid grid-cols-2 max-h-[28rem] overflow-y-auto">
        <div className="sticky top-0 z-10 col-span-2 grid grid-cols-2 border-b border-border-strong/20">
          {renderColumnHeader("left", leftLabel, 0)}
          {renderColumnHeader("right", rightLabel, rightLineCount)}
        </div>
        <div className="border-r border-border-strong/30 bg-bg-base px-3 py-4 text-[12px] font-mono leading-relaxed text-text-subtle italic">
          empty
        </div>
        <pre className="min-w-0 bg-bg-base p-3 text-[12px] font-mono whitespace-pre-wrap break-words leading-relaxed text-text-muted">
          {rightContent.trim() || <em className="not-italic text-text-subtle">empty</em>}
        </pre>
      </div>
    );
  } else if (noDiff) {
    body = (
      <pre className="max-h-72 overflow-y-auto bg-bg-base p-3 text-[12px] font-mono whitespace-pre-wrap leading-relaxed text-text-muted">
        {rightContent.trim() || <em className="not-italic text-text-subtle">empty</em>}
      </pre>
    );
  } else {
    body = (
      <div className="max-h-[28rem] overflow-y-auto">
        <div className="sticky top-0 z-10 grid grid-cols-2 border-b border-border-strong/20">
          {renderColumnHeader("left", leftLabel, leftLineCount)}
          {renderColumnHeader("right", rightLabel, rightLineCount)}
        </div>
        <div className="font-mono text-[12px]">
          {rows.map((row, idx) => (
            <div
              key={idx}
              className="grid grid-cols-2 border-b border-border-strong/20 last:border-b-0"
            >
              {renderCell(row.left, "left")}
              {renderCell(row.right, "right")}
            </div>
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="rounded-lg border border-border-strong/40 overflow-hidden">
      <div className="bg-bg-input px-3 py-2 flex items-center justify-between border-b border-border-strong/30">
        <span className="text-[11px] font-medium text-text-muted uppercase tracking-wider">
          {!hasLeftContent
            ? "Side-By-Side Comparison"
            : noDiff
              ? `${leftLabel} And ${rightLabel} Match`
              : "Side-By-Side Diff"}
        </span>
        <span className="text-[11px] text-text-muted flex items-center gap-2">
          {!hasLeftContent ? (
            <span>{leftLabel} is empty on the left</span>
          ) : noDiff ? (
            <span>{pluralLines(rightLineCount)}</span>
          ) : (
            <>
              {favouredCount > 0 && <span className="text-success">+{favouredCount}</span>}
              {replacedCount > 0 && <span className="text-danger">−{replacedCount}</span>}
              <span className="text-border-strong/60">·</span>
              <span>
                {leftLineCount} vs {rightLineCount} lines
              </span>
            </>
          )}
        </span>
      </div>
      {body}
    </div>
  );
}

export interface DiffColumnActionProps {
  /** Matches the colour of the column the button sits under. */
  tone: "success" | "danger";
  title: string;
  description: string;
  onClick: () => void;
  disabled?: boolean;
}

/** Resolution button placed directly under the diff column it acts on. */
export function DiffColumnAction({ tone, title, description, onClick, disabled }: DiffColumnActionProps) {
  const toneClass =
    tone === "success"
      ? "border-success/30 bg-success/5 hover:bg-success/10 hover:border-success/50"
      : "border-danger/30 bg-danger/5 hover:bg-danger/10 hover:border-danger/50";
  const titleClass = tone === "success" ? "text-success" : "text-danger";

  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className={`flex flex-col items-start gap-0.5 px-4 py-3 rounded-lg border transition-colors text-left disabled:opacity-50 ${toneClass}`}
    >
      <span className={`text-[13px] font-medium ${titleClass}`}>{title}</span>
      <span className="text-[12px] text-text-muted">{description}</span>
    </button>
  );
}
