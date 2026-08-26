import { useEffect, type ReactNode } from "react";
import { X } from "lucide-react";

interface AssetDrawerProps {
  open: boolean;
  onClose: () => void;
  /** While true, Escape does not close the drawer — avoids silently discarding unsaved edits. */
  isEditing?: boolean;
  widthClassName?: string;
  /**
   * Tailwind `top-*` utility for the close button's vertical offset. The
   * button is absolutely positioned against the panel, not the header row
   * inside `children`, so it only lands centered on a single-line ~44px
   * header (the default). Pass "top-4" when the header's first row is a
   * two-line title+subtitle stack (measured ~56-60px tall) so the button
   * stays centered against it instead of sitting a few pixels too high.
   */
  closeButtonTopClassName?: string;
  children: ReactNode;
}

/**
 * Right-hand slide-over drawer shell shared by every Library section's
 * table+drawer interface: scrim, sized panel, close button, and an
 * Escape-to-close handler guarded by `isEditing`.
 */
export function AssetDrawer({
  open,
  onClose,
  isEditing = false,
  widthClassName = "w-[80vw] max-w-[1200px] min-w-[640px]",
  closeButtonTopClassName = "top-2",
  children,
}: AssetDrawerProps) {
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !isEditing) onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, isEditing, onClose]);

  if (!open) return null;

  return (
    <>
      <div onClick={onClose} className="fixed inset-0 bg-black/40 z-40" aria-hidden="true">
        {/* Window drag strip — the scrim otherwise blocks the app's own top-bar drag region */}
        <div data-tauri-drag-region className="absolute top-0 inset-x-0 h-11 select-none" />
      </div>
      <div
        role="dialog"
        aria-modal="true"
        className={`fixed right-0 top-0 h-full ${widthClassName} bg-bg-base border-l border-border-strong/40 z-50 flex flex-col shadow-2xl`}
      >
        <button
          onClick={onClose}
          className={`absolute ${closeButtonTopClassName} right-2 z-10 p-1.5 rounded-md text-text-muted hover:text-text-base hover:bg-bg-sidebar transition-colors`}
          aria-label="Close"
          title="Close (Esc)"
        >
          <X size={14} />
        </button>
        <div className="flex-1 flex flex-col min-h-0">{children}</div>
      </div>
    </>
  );
}
