// Extracted verbatim from Projects.tsx (behavior-preserving refactor).

import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { X } from "lucide-react";
import { computeLineDiff } from "../diff";
import type { DriftedFile } from "../types";

interface DriftDiffModalProps {
  file: DriftedFile;
  agentLabel: string;
  projectName?: string;
  onClose: () => void;
  onResolved?: () => void;
}

export function DriftDiffModal({ file, agentLabel, projectName, onClose, onResolved }: DriftDiffModalProps) {
  const diffLines = file.expected != null && file.actual != null
    ? computeLineDiff(file.expected, file.actual)
    : null;

  const [actionInProgress, setActionInProgress] = useState<string | null>(null);

  // Extract skill name from a stale drift path like ".agents/skills/my-skill"
  // or ".claude/skills/my-skill".  The skill name is the last path segment.
  const staleSkillName = file.reason === "stale"
    ? file.path.split("/").pop() ?? null
    : null;

  // Close on Escape
  useEffect(() => {
    const handler = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onClose]);

  const handleAdoptSkill = async () => {
    if (!projectName || !staleSkillName) return;
    setActionInProgress("adopt");
    try {
      await invoke("adopt_stale_skill", { name: projectName, skillName: staleSkillName });
      onResolved?.();
      onClose();
    } catch (err: any) {
      console.error("Failed to adopt stale skill:", err);
      setActionInProgress(null);
    }
  };

  const handleRemoveSkill = async () => {
    if (!projectName || !staleSkillName) return;
    setActionInProgress("remove");
    try {
      await invoke("remove_stale_skill", { name: projectName, skillName: staleSkillName });
      onResolved?.();
      onClose();
    } catch (err: any) {
      console.error("Failed to remove stale skill:", err);
      setActionInProgress(null);
    }
  };

  const handleSyncOverwrite = async () => {
    if (!projectName) return;
    setActionInProgress("overwrite");
    try {
      await invoke("sync_project", { name: projectName });
      onResolved?.();
      onClose();
    } catch (err: any) {
      console.error("Failed to sync project:", err);
      setActionInProgress(null);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center"
      style={{ background: "rgba(0,0,0,0.6)" }}
      onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      <div
        className="flex flex-col bg-bg-sidebar border border-border-strong/40 rounded-xl shadow-2xl overflow-hidden"
        style={{ width: "min(900px, 90vw)", maxHeight: "80vh" }}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-3 border-b border-border-strong flex-shrink-0">
          <div className="flex items-center gap-3">
            <span className="text-[11px] font-medium text-warning/70 uppercase tracking-wider">{agentLabel}</span>
            <span className="text-border-strong">/</span>
            <span className="text-[13px] font-mono text-text-base">{file.path}</span>
            <span className={`text-[10px] font-medium px-1.5 py-0.5 rounded uppercase tracking-wider ${
              file.reason === "modified"
                ? "bg-warning/15 text-warning"
                : file.reason === "missing"
                ? "bg-danger/15 text-danger"
                : file.reason === "stale"
                ? "bg-text-muted/15 text-text-muted"
                : "bg-text-muted/15 text-text-muted"
            }`}>{file.reason}</span>
          </div>
          <button
            onClick={onClose}
            className="text-text-muted hover:text-text-base transition-colors"
          >
            <X size={16} />
          </button>
        </div>

        {/* Legend */}
        {diffLines && (
          <div className="flex items-center gap-4 px-5 py-2 border-b border-border-strong flex-shrink-0 bg-bg-input">
            <div className="flex items-center gap-1.5">
              <div className="w-3 h-3 rounded-sm bg-success/20 border border-success/40" />
              <span className="text-[11px] text-text-muted">On disk (current)</span>
            </div>
            <div className="flex items-center gap-1.5">
              <div className="w-3 h-3 rounded-sm bg-danger/20 border border-danger/40" />
              <span className="text-[11px] text-text-muted">Automatic would generate (expected)</span>
            </div>
          </div>
        )}

        {/* Diff body */}
        <div className="overflow-auto flex-1 font-mono text-[12px]">
          {diffLines ? (
            <table className="w-full border-collapse">
              <tbody>
                {diffLines.map((line, idx) => (
                  <tr
                    key={idx}
                    className={
                      line.type === "added"
                        ? "bg-success/10 hover:bg-success/15"
                        : line.type === "removed"
                        ? "bg-danger/10 hover:bg-danger/15"
                        : "hover:bg-surface-hover"
                    }
                  >
                    {/* Line number: expected (a) */}
                    <td className="select-none text-right text-border-strong px-3 py-0.5 w-12 border-r border-border-strong min-w-[3rem]">
                      {line.lineNo.a ?? ""}
                    </td>
                    {/* Line number: actual (b) */}
                    <td className="select-none text-right text-border-strong px-3 py-0.5 w-12 border-r border-border-strong min-w-[3rem]">
                      {line.lineNo.b ?? ""}
                    </td>
                    {/* Sign */}
                    <td className={`select-none px-2 py-0.5 w-5 text-center font-bold ${
                      line.type === "added" ? "text-success" : line.type === "removed" ? "text-danger" : "text-border-strong"
                    }`}>
                      {line.type === "added" ? "+" : line.type === "removed" ? "−" : " "}
                    </td>
                    {/* Content */}
                    <td className={`px-3 py-0.5 whitespace-pre ${
                      line.type === "added"
                        ? "text-success"
                        : line.type === "removed"
                        ? "text-danger"
                        : "text-text-muted"
                    }`}>
                      {line.content}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          ) : file.reason === "stale" && file.actual ? (
            /* Stale skill with on-disk content: show a header + content preview */
            <div className="flex flex-col h-full">
              <div className="flex flex-col items-center py-4 text-text-muted border-b border-border-strong flex-shrink-0">
                <p className="text-[13px] font-medium text-text-base mb-1">Stale directory</p>
                <p className="text-[12px]">
                  This skill exists on disk but is not in the project config.
                  {staleSkillName && (
                    <> Choose how to resolve <span className="font-mono font-medium text-text-base">{staleSkillName}</span>.</>
                  )}
                </p>
              </div>
              <div className="flex items-center gap-2 px-5 py-1.5 border-b border-border-strong bg-bg-input flex-shrink-0">
                <span className="text-[11px] text-text-muted">Content on disk (SKILL.md)</span>
              </div>
              <table className="w-full border-collapse">
                <tbody>
                  {file.actual.split("\n").map((line, idx) => (
                    <tr key={idx} className="hover:bg-surface-hover">
                      <td className="select-none text-right text-border-strong px-3 py-0.5 w-12 border-r border-border-strong min-w-[3rem]">
                        {idx + 1}
                      </td>
                      <td className="px-3 py-0.5 whitespace-pre text-text-muted">
                        {line}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            /* Non-modified reasons: nothing to diff — show a descriptive message */
            <div className="flex flex-col items-center justify-center h-full py-16 text-text-muted">
              {file.reason === "missing" && (
                <>
                  <p className="text-[13px] font-medium text-text-base mb-2">File is missing on disk</p>
                  <p className="text-[12px]">Automatic would create this file. Sync the project to resolve.</p>
                </>
              )}
              {file.reason === "stale" && (
                <>
                  <p className="text-[13px] font-medium text-text-base mb-2">Stale directory</p>
                  <p className="text-[12px]">This skill directory exists on disk but is no longer in the project config.</p>
                  {staleSkillName && (
                    <p className="text-[12px] mt-3 text-text-base">
                      Choose how to resolve <span className="font-mono font-medium">{staleSkillName}</span>:
                    </p>
                  )}
                </>
              )}
              {file.reason === "unreadable" && (
                <>
                  <p className="text-[13px] font-medium text-text-base mb-2">File could not be read</p>
                  <p className="text-[12px]">Check file permissions and try again.</p>
                </>
              )}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center justify-between px-5 py-3 border-t border-border-strong flex-shrink-0">
          {/* Stale skill resolution actions */}
          {file.reason === "stale" && staleSkillName && projectName ? (
            <>
              <div className="flex items-center gap-2">
                <button
                  onClick={handleAdoptSkill}
                  disabled={actionInProgress !== null}
                  className="px-3 py-1.5 text-[12px] font-medium rounded bg-success/15 text-success border border-success/30 hover:bg-success/25 hover:border-success/50 transition-colors disabled:opacity-50"
                >
                  {actionInProgress === "adopt" ? "Adding..." : "Add to project"}
                </button>
                <button
                  onClick={handleSyncOverwrite}
                  disabled={actionInProgress !== null}
                  className="px-3 py-1.5 text-[12px] font-medium rounded bg-warning/15 text-warning border border-warning/30 hover:bg-warning/25 hover:border-warning/50 transition-colors disabled:opacity-50"
                >
                  {actionInProgress === "overwrite" ? "Syncing..." : "Overwrite (re-sync)"}
                </button>
                <button
                  onClick={handleRemoveSkill}
                  disabled={actionInProgress !== null}
                  className="px-3 py-1.5 text-[12px] font-medium rounded bg-danger/15 text-danger border border-danger/30 hover:bg-danger/25 hover:border-danger/50 transition-colors disabled:opacity-50"
                >
                  {actionInProgress === "remove" ? "Removing..." : "Remove from disk"}
                </button>
              </div>
              <button
                onClick={onClose}
                className="px-3 py-1.5 text-[12px] text-text-muted hover:text-text-base transition-colors"
              >
                Close
              </button>
            </>
          ) : (
            <div className="ml-auto">
              <button
                onClick={onClose}
                className="px-3 py-1.5 text-[12px] text-text-muted hover:text-text-base transition-colors"
              >
                Close
              </button>
            </div>
          )}
        </div>
      </div>

    </div>
  );
}
