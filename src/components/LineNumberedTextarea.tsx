import { useRef, useCallback } from "react";

interface LineNumberedTextareaProps {
  value: string;
  onChange: (value: string) => void;
  /** Extra classes forwarded to the outer wrapper div (e.g. flex-sizing). */
  className?: string;
  placeholder?: string;
  spellCheck?: boolean;
  autoFocus?: boolean;
  /**
   * "full" (default) — fills its flex container; uses bg-bg-base with no
   *   border or border-radius. Used for full-bleed panel editors.
   * "inline" — sized by `rows`; uses bg-bg-sidebar with a border and rounded
   *   corners. Used for inline form editors embedded inside lists/panels.
   */
  variant?: "full" | "inline";
  /**
   * When `variant="inline"`, controls the initial visible row count (same
   * semantics as the native textarea `rows` attribute).
   */
  rows?: number;
}

// px font size and unitless line-height used by both the gutter and textarea.
// Kept in sync so line numbers always align regardless of scroll position.
const FONT_SIZE_PX = 13;
const LINE_HEIGHT = 1.625; // Tailwind leading-relaxed
const LINE_HEIGHT_PX = FONT_SIZE_PX * LINE_HEIGHT; // ≈ 21.1 px

/**
 * A textarea that renders a line-number gutter on the left.
 *
 * The gutter scrolls in sync with the textarea so line numbers always align
 * with the visible content regardless of scroll position.
 *
 * All body editors in the app use this component to ensure a consistent
 * editing experience across file types (Markdown, JSON, YAML, etc.).
 */
export function LineNumberedTextarea({
  value,
  onChange,
  className = "",
  placeholder,
  spellCheck = false,
  autoFocus = false,
  variant = "full",
  rows,
}: LineNumberedTextareaProps): React.ReactElement {
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const gutterRef = useRef<HTMLDivElement>(null);

  const lineCount = value.split("\n").length;

  const syncScroll = useCallback(() => {
    if (textareaRef.current && gutterRef.current) {
      gutterRef.current.scrollTop = textareaRef.current.scrollTop;
    }
  }, []);

  const isInline = variant === "inline";

  // For inline variant, derive a min-height from rows so the field starts at a
  // comfortable size. Padding: py-2 = 8px top + 8px bottom = 16px total.
  const inlineStyle =
    isInline && rows != null
      ? { minHeight: rows * LINE_HEIGHT_PX + 16 }
      : undefined;

  const wrapperClass = [
    "flex overflow-hidden",
    isInline
      ? "rounded-md border border-border-strong/40 focus-within:border-brand transition-colors"
      : "",
    className,
  ]
    .filter(Boolean)
    .join(" ");

  const gutterClass = [
    "flex-shrink-0 overflow-hidden select-none w-10 border-r border-border-strong/30",
    "font-mono leading-relaxed text-text-muted text-right pr-2",
    isInline ? "bg-bg-sidebar pt-2 pb-2" : "bg-bg-input pt-4 pb-4",
  ].join(" ");

  // font-size applied via inline style so LINE_HEIGHT_PX stays consistent with
  // the JS constant above.
  const gutterFontStyle = { fontSize: FONT_SIZE_PX };

  const textareaClass = [
    "flex-1 resize-none outline-none font-mono leading-relaxed custom-scrollbar placeholder-text-muted/30 text-text-base",
    isInline
      ? "bg-bg-sidebar px-3 py-2 resize-y min-h-0"
      : "p-4 bg-bg-base min-h-0",
  ].join(" ");

  const textareaFontStyle = { fontSize: FONT_SIZE_PX };

  return (
    <div className={wrapperClass} style={inlineStyle}>
      {/* Line-number gutter */}
      <div
        ref={gutterRef}
        aria-hidden
        className={gutterClass}
        style={gutterFontStyle}
      >
        {Array.from({ length: lineCount }, (_, i) => (
          <div key={i}>{i + 1}</div>
        ))}
      </div>

      {/* Editor */}
      <textarea
        ref={textareaRef}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onScroll={syncScroll}
        placeholder={placeholder}
        spellCheck={spellCheck}
        autoFocus={autoFocus}
        className={textareaClass}
        style={textareaFontStyle}
      />
    </div>
  );
}
