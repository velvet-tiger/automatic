import { X, FolderSearch, Loader2 } from "lucide-react";
import { ProjectDiscoveryPanel, useProjectDiscovery, type AddedProject } from "./ProjectDiscovery";

export type { AddedProject } from "./ProjectDiscovery";

interface ProjectDiscoveryDialogProps {
  isOpen: boolean;
  /** When set, added projects also join this group. Shown as a note. */
  groupName?: string | null;
  onClose: () => void;
  /**
   * Called once after an add, with every project that was created. Resolves
   * to a message when a follow-up step (such as joining a group) failed.
   */
  onAdded: (added: AddedProject[]) => Promise<string | null> | void;
}

/** Scan a folder tree for agent projects and add the ones the user picks. */
export function ProjectDiscoveryDialog({ isOpen, groupName = null, onClose, onAdded }: ProjectDiscoveryDialogProps) {
  const discovery = useProjectDiscovery(isOpen);

  if (!isOpen) return null;

  const { adding, busy, selectedAddable, addedCount } = discovery;

  const handleClose = () => {
    if (adding) return;
    discovery.cancelScan();
    onClose();
  };

  // onAdded runs inside the add, so the dialog stays locked until any
  // group join and reload finish and their errors land on an open dialog.
  const addSelected = () => discovery.addSelected(onAdded);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/50" onClick={handleClose} />
      <div
        role="dialog"
        aria-label="Find projects"
        className="relative bg-bg-input border border-border-strong rounded-xl shadow-2xl w-full max-w-2xl mx-4 max-h-[85vh] flex flex-col"
      >
        <div className="flex items-center justify-between px-5 py-4 border-b border-border-strong/40">
          <div className="flex items-center gap-2">
            <FolderSearch size={16} className="text-text-muted" />
            <h2 className="text-[15px] font-semibold text-text-base">Find Projects</h2>
          </div>
          <button
            onClick={handleClose}
            disabled={adding}
            aria-label="Close"
            className="p-1 text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors disabled:opacity-50"
          >
            <X size={16} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto px-5 py-4 space-y-4">
          <p className="text-[13px] text-text-muted leading-relaxed">
            Choose a folder to search. Automatic looks for folders that hold agent files, such as{" "}
            <code className="font-mono text-[11px] bg-bg-sidebar px-1 rounded">CLAUDE.md</code> or{" "}
            <code className="font-mono text-[11px] bg-bg-sidebar px-1 rounded">AGENTS.md</code>. Nothing is
            changed until you add projects. Adding a project writes Automatic's files into its folder and
            imports the MCP servers it already uses.
          </p>
          {groupName && (
            <p className="text-[12px] text-text-muted">
              New projects will also join the group <span className="text-text-base">{groupName}</span>.
            </p>
          )}
          <ProjectDiscoveryPanel state={discovery} />
        </div>

        <div className="flex justify-end items-center gap-2 px-5 py-3 border-t border-border-strong/40">
          <button
            onClick={handleClose}
            disabled={adding}
            className="px-4 py-2 text-[13px] font-medium text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded-lg transition-colors disabled:opacity-50"
          >
            {addedCount > 0 ? "Done" : "Cancel"}
          </button>
          <button
            onClick={addSelected}
            disabled={busy || selectedAddable.length === 0}
            className="px-4 py-2 text-[13px] font-medium bg-brand hover:bg-brand-hover text-white rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-2"
          >
            {adding ? (
              <>
                <Loader2 size={14} className="animate-spin" />
                Adding…
              </>
            ) : (
              <>Add {selectedAddable.length > 0 ? `${selectedAddable.length} project${selectedAddable.length === 1 ? "" : "s"}` : "projects"}</>
            )}
          </button>
        </div>
      </div>
    </div>
  );
}
