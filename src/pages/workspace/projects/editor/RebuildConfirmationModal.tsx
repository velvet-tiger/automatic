// Extracted verbatim from Projects.tsx (behavior-preserving refactor).

import { useEffect } from "react";
import { X } from "lucide-react";
import type { RebuildPreview } from "../types";

interface RebuildConfirmationModalProps {
  preview: RebuildPreview;
  busy: boolean;
  onConfirm: () => void;
  onClose: () => void;
}

export function RebuildConfirmationModal({ preview, busy, onConfirm, onClose }: RebuildConfirmationModalProps) {
  const categories = Array.isArray(preview.categories) ? preview.categories : [];

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !busy) onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [busy, onClose]);

  const changedCategories = categories.filter((category) => category.added.length > 0 || category.removed.length > 0);
  const unchangedCategories = categories.filter((category) => category.added.length === 0 && category.removed.length === 0);

  const renderPills = (items: string[], tone: "neutral" | "success" | "danger") => {
    if (items.length === 0) {
      return <span className="text-[11px] italic text-text-muted/70">none</span>;
    }

    const toneClass = tone === "success"
      ? "bg-success/10 text-success border-success/20"
      : tone === "danger"
      ? "bg-danger/10 text-danger border-danger/20"
      : "bg-bg-input text-text-base border-border-strong/40";

    return (
      <div className="flex flex-wrap gap-1.5">
        {items.map((item) => (
          <span key={item} className={`inline-flex items-center px-2 py-0.5 rounded-full border text-[11px] font-medium ${toneClass}`}>
            {item}
          </span>
        ))}
      </div>
    );
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center"
      style={{ background: "rgba(0,0,0,0.6)" }}
      onClick={(e) => { if (e.target === e.currentTarget && !busy) onClose(); }}
    >
      <div
        className="flex flex-col bg-bg-sidebar border border-border-strong/40 rounded-xl shadow-2xl overflow-hidden"
        style={{ width: "min(860px, 92vw)", maxHeight: "85vh" }}
      >
        <div className="flex items-center justify-between px-5 py-3 border-b border-border-strong flex-shrink-0">
          <div className="flex items-center gap-3">
            <span className="text-[11px] font-medium text-warning/70 uppercase tracking-wider">Rebuild Project</span>
            <span className="text-border-strong">/</span>
            <span className="text-[13px] font-mono text-text-base">{preview.project_name}</span>
          </div>
          <button onClick={onClose} disabled={busy} className="text-text-muted hover:text-text-base transition-colors disabled:opacity-40">
            <X size={16} />
          </button>
        </div>

        <div className="px-5 py-4 space-y-4 overflow-y-auto flex-1 min-h-0">
          <div className="rounded-lg border border-warning/25 bg-warning/5 px-4 py-3">
            <p className="text-[13px] text-text-base leading-relaxed">
              Rebuild will replace Automatic's saved project state with what it can detect from the current project files on disk.
            </p>
            <p className="text-[12px] text-text-muted mt-1">
              This compares feature state, not file contents.
            </p>
          </div>

          {changedCategories.length > 0 ? (
            <div className="space-y-3">
              {changedCategories.map((category) => (
                <div key={category.key} className="rounded-lg border border-border-strong/40 overflow-hidden">
                  <div className="px-3 py-2 bg-bg-input border-b border-border-strong/30 flex items-center justify-between gap-3">
                    <span className="text-[12px] font-semibold text-text-base">{category.label}</span>
                    <div className="flex items-center gap-2 text-[11px] font-medium">
                      {category.added.length > 0 && <span className="text-success">+{category.added.length}</span>}
                      {category.removed.length > 0 && <span className="text-danger">-{category.removed.length}</span>}
                    </div>
                  </div>
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-px bg-border-strong/20">
                    <div className="bg-bg-sidebar p-3 space-y-2">
                      <div className="text-[11px] font-semibold uppercase tracking-wider text-text-muted">Automatic now</div>
                      {renderPills(category.automatic, "neutral")}
                    </div>
                    <div className="bg-bg-sidebar p-3 space-y-2">
                      <div className="text-[11px] font-semibold uppercase tracking-wider text-text-muted">Detected on disk</div>
                      {renderPills(category.disk, "neutral")}
                    </div>
                  </div>
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-px bg-border-strong/20 border-t border-border-strong/30">
                    <div className="bg-bg-sidebar p-3 space-y-2">
                      <div className="text-[11px] font-semibold uppercase tracking-wider text-success">Will be added</div>
                      {renderPills(category.added, "success")}
                    </div>
                    <div className="bg-bg-sidebar p-3 space-y-2">
                      <div className="text-[11px] font-semibold uppercase tracking-wider text-danger">Will be removed</div>
                      {renderPills(category.removed, "danger")}
                    </div>
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <div className="rounded-lg border border-border-strong/40 bg-bg-input px-4 py-3 text-[13px] text-text-muted">
              No feature differences detected. Rebuild will just refresh Automatic's stored snapshots and derived state.
            </div>
          )}

          {unchangedCategories.length > 0 && (
            <div className="rounded-lg border border-border-strong/30 bg-bg-input/40 px-4 py-3">
              <div className="text-[11px] font-semibold uppercase tracking-wider text-text-muted mb-2">Unchanged</div>
              <div className="flex flex-wrap gap-1.5">
                {unchangedCategories.map((category) => (
                  <span key={category.key} className="inline-flex items-center px-2 py-0.5 rounded-full border border-border-strong/30 text-[11px] text-text-muted">
                    {category.label}
                  </span>
                ))}
              </div>
            </div>
          )}
        </div>

        <div className="flex items-center justify-end gap-2 px-5 py-3 border-t border-border-strong flex-shrink-0 bg-bg-input">
          <button
            onClick={onClose}
            disabled={busy}
            className="px-3 py-1.5 text-[12px] text-text-muted hover:text-text-base transition-colors disabled:opacity-40"
          >
            Cancel
          </button>
          <button
            onClick={onConfirm}
            disabled={busy}
            className="px-3 py-1.5 text-[12px] font-medium rounded bg-warning/15 text-warning border border-warning/30 hover:bg-warning/25 hover:border-warning/50 transition-colors disabled:opacity-50"
          >
            {busy ? "Rebuilding..." : "Confirm Rebuild"}
          </button>
        </div>
      </div>
    </div>
  );
}
