// Extracted verbatim from Projects.tsx (behavior-preserving refactor).

import { useState, useEffect, useMemo } from "react";
import { AlertCircle, FileText, X } from "lucide-react";
import type { UnifiedCandidate } from "../types";

interface SwitchToUnifiedModalProps {
  candidates: UnifiedCandidate[];
  busy: boolean;
  onConfirm: (filename: string) => void;
  onClose: () => void;
}

/**
 * Two-stage modal shown whenever the user switches a project to unified
 * instruction mode.  Stage 1 lets the user pick which existing file's content
 * becomes the unified source; stage 2 warns which files will be overwritten
 * before the change is committed.  Always presented (even when only one file
 * has content) so the user is never silently surprised by an overwrite.
 */
export function SwitchToUnifiedModal({
  candidates,
  busy,
  onConfirm,
  onClose,
}: SwitchToUnifiedModalProps) {
  const allFilenames = useMemo(() => candidates.map((c) => c.filename), [candidates]);

  // Pre-select the most-recently-modified non-empty file to help the user
  // pick the right source on first render — they can still change it.
  const initialSelection = useMemo<string | null>(() => {
    const nonEmpty = candidates.filter((c) => c.exists && c.user_content.trim().length > 0);
    if (nonEmpty.length === 0) {
      return candidates.find((c) => c.exists)?.filename ?? candidates[0]?.filename ?? null;
    }
    const newest = nonEmpty.reduce((best, c) => {
      const bestMs = best.modified_ms ?? 0;
      const cMs = c.modified_ms ?? 0;
      return cMs > bestMs ? c : best;
    }, nonEmpty[0]);
    return newest.filename;
  }, [candidates]);

  const [stage, setStage] = useState<"pick" | "confirm">("pick");
  const [selected, setSelected] = useState<string | null>(initialSelection);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => { if (e.key === "Escape" && !busy) onClose(); };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onClose, busy]);

  const newestMs = candidates.reduce<number | null>(
    (acc, c) => (c.modified_ms != null && (acc == null || c.modified_ms > acc) ? c.modified_ms : acc),
    null,
  );

  const formatModified = (ms: number | null): string => {
    if (ms == null) return "";
    try {
      return new Date(ms).toLocaleString();
    } catch {
      return "";
    }
  };

  const selectedCandidate = candidates.find((c) => c.filename === selected) ?? null;
  const targetFilenames = allFilenames.filter((f) => f !== selected);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center"
      style={{ background: "rgba(0,0,0,0.6)" }}
      onClick={(e) => { if (e.target === e.currentTarget && !busy) onClose(); }}
    >
      <div
        className="flex flex-col bg-bg-sidebar border border-border-strong/40 rounded-xl shadow-2xl overflow-hidden"
        style={{ width: "min(1100px, 94vw)", maxHeight: "85vh" }}
      >
        <div className="flex items-center justify-between px-5 py-3 border-b border-border-strong flex-shrink-0">
          <div className="flex items-center gap-3">
            <span className="text-[11px] font-medium text-warning/70 uppercase tracking-wider">
              Switch To Unified Mode
            </span>
            <span className="text-border-strong">/</span>
            <span className="text-[13px] text-text-base">
              {stage === "pick" ? "Choose A Source File" : "Confirm Overwrite"}
            </span>
          </div>
          <button
            onClick={() => { if (!busy) onClose(); }}
            className="text-text-muted hover:text-text-base transition-colors disabled:opacity-50"
            disabled={busy}
          >
            <X size={16} />
          </button>
        </div>

        {stage === "pick" ? (
          <div className="px-5 py-4 space-y-4 overflow-y-auto flex-1 min-h-0">
            <p className="text-[12px] text-text-muted leading-relaxed">
              Unified mode keeps a single shared body of instructions for every agent. Pick the
              file whose content should become the unified source. On the next step you will see
              exactly which other files will be replaced.
            </p>

            <div className="space-y-2">
              {candidates.map((c) => {
                const isSelected = selected === c.filename;
                const isNewest = newestMs != null && c.modified_ms === newestMs;
                const trimmed = c.user_content.trim();
                const lineCount = trimmed ? c.user_content.split("\n").length : 0;
                return (
                  <button
                    key={c.filename}
                    type="button"
                    onClick={() => { if (!busy) setSelected(c.filename); }}
                    disabled={busy}
                    className={`w-full text-left rounded-lg border overflow-hidden transition-colors disabled:opacity-50 ${
                      isSelected
                        ? "border-brand bg-brand/5"
                        : "border-border-strong/40 bg-bg-base/40 hover:border-border-strong/70"
                    }`}
                  >
                    <div className="flex items-center justify-between px-3 py-2 bg-bg-input/60 border-b border-border-strong/30">
                      <div className="flex items-center gap-2 min-w-0">
                        <span
                          className={`w-3.5 h-3.5 rounded-full border-2 flex items-center justify-center flex-shrink-0 ${
                            isSelected ? "border-brand bg-brand" : "border-text-muted"
                          }`}
                        >
                          {isSelected && <span className="w-1.5 h-1.5 rounded-full bg-white" />}
                        </span>
                        <FileText size={13} className="text-text-muted flex-shrink-0" />
                        <span className="text-[13px] font-mono text-text-base truncate">
                          {c.filename}
                        </span>
                        {c.agent_labels.length > 0 && (
                          <span className="text-[11px] text-text-muted truncate">
                            · {c.agent_labels.join(", ")}
                          </span>
                        )}
                        {!c.exists && (
                          <span className="text-[10px] text-text-subtle uppercase tracking-wider px-1.5 py-0.5 rounded bg-bg-input border border-border-strong/40">
                            Not on disk
                          </span>
                        )}
                        {isNewest && c.exists && candidates.filter((x) => x.exists).length > 1 && (
                          <span className="text-[10px] text-success uppercase tracking-wider px-1.5 py-0.5 rounded bg-success/10 border border-success/30">
                            Newest
                          </span>
                        )}
                      </div>
                      <div className="flex items-center gap-3 flex-shrink-0">
                        <span className="text-[11px] text-text-muted">
                          {lineCount} line{lineCount !== 1 ? "s" : ""}
                        </span>
                        {c.modified_ms != null && (
                          <span className="text-[11px] text-text-subtle">
                            {formatModified(c.modified_ms)}
                          </span>
                        )}
                      </div>
                    </div>
                    <pre className="max-h-48 overflow-y-auto bg-bg-base p-3 text-[12px] font-mono whitespace-pre-wrap leading-relaxed text-text-muted">
                      {trimmed || (
                        <em className="not-italic text-text-subtle">empty</em>
                      )}
                    </pre>
                  </button>
                );
              })}
            </div>
          </div>
        ) : (
          <div className="px-5 py-4 space-y-4 overflow-y-auto flex-1 min-h-0">
            <div className="rounded-lg border border-warning/40 bg-warning/5 px-3 py-2.5">
              <div className="flex items-start gap-2">
                <AlertCircle size={14} className="text-warning mt-0.5 flex-shrink-0" />
                <div className="text-[12px] text-text-base leading-relaxed">
                  <p className="font-medium text-warning mb-1">
                    This will overwrite all instruction files with the same content.
                  </p>
                  <p className="text-text-muted">
                    Switching to unified mode will replace the files below with the content of{" "}
                    <span className="font-mono text-text-base">{selected}</span>. This action cannot
                    be undone from inside Automatic.
                  </p>
                </div>
              </div>
            </div>

            <div className="rounded-lg border border-border-strong/40 bg-bg-base/40 overflow-hidden">
              <div className="px-3 py-2 bg-bg-input/60 border-b border-border-strong/30 text-[11px] font-semibold text-text-muted uppercase tracking-wider">
                Files To Be Replaced
              </div>
              <ul className="px-3 py-2 space-y-1">
                {targetFilenames.length === 0 ? (
                  <li className="text-[12px] text-text-subtle italic">
                    No other files — only{" "}
                    <span className="font-mono">{selected}</span> will be re-saved.
                  </li>
                ) : (
                  targetFilenames.map((f) => (
                    <li key={f} className="flex items-center gap-2 text-[12px] text-text-base">
                      <FileText size={12} className="text-text-muted flex-shrink-0" />
                      <span className="font-mono">{f}</span>
                    </li>
                  ))
                )}
              </ul>
            </div>

            {selectedCandidate && (
              <div className="rounded-lg border border-border-strong/40 bg-bg-base/40 overflow-hidden">
                <div className="px-3 py-2 bg-bg-input/60 border-b border-border-strong/30 text-[11px] font-semibold text-text-muted uppercase tracking-wider">
                  Source Content Preview · <span className="font-mono normal-case">{selected}</span>
                </div>
                <pre className="max-h-48 overflow-y-auto bg-bg-base p-3 text-[12px] font-mono whitespace-pre-wrap leading-relaxed text-text-muted">
                  {selectedCandidate.user_content.trim() || (
                    <em className="not-italic text-text-subtle">empty</em>
                  )}
                </pre>
              </div>
            )}
          </div>
        )}

        <div className="flex items-center justify-end gap-2 px-5 py-3 border-t border-border-strong flex-shrink-0">
          {stage === "confirm" && (
            <button
              onClick={() => { if (!busy) setStage("pick"); }}
              disabled={busy}
              className="px-3 py-1.5 text-[12px] text-text-muted hover:text-text-base transition-colors disabled:opacity-50"
            >
              Back
            </button>
          )}
          <button
            onClick={() => { if (!busy) onClose(); }}
            disabled={busy}
            className="px-3 py-1.5 text-[12px] text-text-muted hover:text-text-base transition-colors disabled:opacity-50"
          >
            Cancel
          </button>
          {stage === "pick" ? (
            <button
              onClick={() => { if (!busy && selected) setStage("confirm"); }}
              disabled={busy || !selected}
              className="px-3 py-1.5 text-[12px] font-medium rounded border border-brand/60 bg-brand/10 hover:bg-brand/20 text-brand transition-colors disabled:opacity-50"
            >
              Continue
            </button>
          ) : (
            <button
              onClick={() => { if (!busy && selected) onConfirm(selected); }}
              disabled={busy || !selected}
              className="px-3 py-1.5 text-[12px] font-medium rounded border border-danger/60 bg-danger/10 hover:bg-danger/20 text-danger transition-colors disabled:opacity-50"
            >
              {busy ? "Switching…" : "Confirm overwrite"}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
