import type { ReactNode } from "react";
import { X } from "lucide-react";

export const FIELD_CLASS =
  "w-full bg-bg-base border border-border-strong/40 hover:border-border-strong focus:border-brand rounded-md px-3 py-2 text-[13px] text-text-base placeholder-text-muted/50 outline-none transition-colors";
export const LABEL_CLASS = "block text-[12px] font-medium text-text-base mb-1";
export const HINT_CLASS = "text-[11px] text-text-muted mt-1";
export const PRIMARY_BUTTON_CLASS =
  "flex h-[28px] items-center gap-1.5 px-3 bg-brand hover:bg-brand-hover text-white rounded text-[12px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed";
export const SECONDARY_BUTTON_CLASS =
  "flex h-[28px] items-center gap-1.5 px-3 text-[12px] text-text-base bg-bg-sidebar hover:bg-surface-hover border border-border-strong/40 rounded transition-colors";

/** Centered modal used by every context dialog. */
export function ContextDialog({
  title,
  onClose,
  children,
  footer,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
  footer: ReactNode;
}) {
  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center" role="dialog" aria-modal="true" aria-label={title}>
      <div className="absolute inset-0 bg-black/50" onClick={onClose} />
      <div
        className="relative bg-bg-input border border-border-strong rounded-xl shadow-2xl w-full max-w-md mx-4 flex flex-col max-h-[80vh]"
        onKeyDown={(e) => { if (e.key === "Escape") onClose(); }}
      >
        <div className="flex items-center justify-between px-5 py-4 border-b border-border-strong/40">
          <h2 className="text-[15px] font-semibold text-text-base">{title}</h2>
          <button onClick={onClose} aria-label="Close" className="p-1 text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors">
            <X size={16} />
          </button>
        </div>
        <div className="px-5 py-4 space-y-4 overflow-y-auto custom-scrollbar">{children}</div>
        <div className="flex items-center justify-end gap-2 px-5 py-3 border-t border-border-strong/40">{footer}</div>
      </div>
    </div>
  );
}

/** Inline error line used inside dialogs. */
export function DialogError({ message }: { message: string | null }) {
  if (!message) return null;
  return <p className="text-[12px] text-danger">{message}</p>;
}
