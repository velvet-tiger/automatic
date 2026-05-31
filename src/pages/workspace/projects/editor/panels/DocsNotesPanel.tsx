// Extracted verbatim from ProjectEditor.tsx (Phase 2E — behavior-preserving).

import { Check, FileText, Plus, RefreshCw, Trash2, X } from "lucide-react";
import { LineNumberedTextarea } from "../../../../../components/LineNumberedTextarea";
import type { Project } from "../../types";

type DocEntry = { path: string; summary?: string };

interface DocsNotesPanelProps {
  project: Project;
  docs: Record<string, DocEntry>;
  docNoteSelected: string | null;
  docNoteContent: string;
  setDocNoteContent: (v: string) => void;
  docNoteDirty: boolean;
  setDocNoteDirty: (v: boolean) => void;
  docNoteSaving: boolean;
  docNoteLoading: boolean;
  docNewNoteCreating: boolean;
  setDocNewNoteCreating: (v: boolean) => void;
  docNewNoteName: string;
  setDocNewNoteName: (v: string) => void;
  createDocNote: (noteName: string) => Promise<void>;
  loadDocNote: (key: string) => Promise<void>;
  saveDocNote: () => Promise<void>;
  removeDocEntry: (key: string, isNote: boolean) => Promise<void>;
}

export function DocsNotesPanel({
  project,
  docs,
  docNoteSelected,
  docNoteContent,
  setDocNoteContent,
  docNoteDirty,
  setDocNoteDirty,
  docNoteSaving,
  docNoteLoading,
  docNewNoteCreating,
  setDocNewNoteCreating,
  docNewNoteName,
  setDocNewNoteName,
  createDocNote,
  loadDocNote,
  saveDocNote,
  removeDocEntry,
}: DocsNotesPanelProps) {
  return (
    <div className="flex-1 flex min-h-0">
      {!project?.directory ? (
        <div className="flex-1 flex items-center justify-center">
          <p className="text-[13px] text-text-muted italic">
            Set a project directory to use documentation.
          </p>
        </div>
      ) : (
        <>
          {/* Left panel: note list */}
          <div className="w-52 flex-shrink-0 border-r border-border-strong/40 flex flex-col min-h-0">
            <div className="px-3 py-2.5 border-b border-border-strong/40 flex items-center justify-between flex-shrink-0">
              <span className="text-[11px] font-semibold text-text-muted uppercase tracking-wide">Notes</span>
              <button
                onClick={() => setDocNewNoteCreating(true)}
                className="p-0.5 rounded text-text-muted hover:text-brand hover:bg-brand/10 transition-colors"
                title="New note"
              >
                <Plus size={13} />
              </button>
            </div>

            {/* New note name input */}
            {docNewNoteCreating && (
              <div className="px-2 py-2 border-b border-border-strong/20 flex items-center gap-1">
                <input
                  autoFocus
                  type="text"
                  value={docNewNoteName}
                  onChange={(e) => setDocNewNoteName(e.target.value)}
                  onKeyDown={async (e) => {
                    if (e.key === "Enter") await createDocNote(docNewNoteName);
                    if (e.key === "Escape") {
                      setDocNewNoteCreating(false);
                      setDocNewNoteName("");
                    }
                  }}
                  placeholder="Note name…"
                  className="flex-1 px-2 py-1 text-[11px] bg-bg-input border border-brand/60 rounded text-text-base placeholder-text-muted focus:outline-none"
                />
                <button
                  onClick={() => createDocNote(docNewNoteName)}
                  disabled={!docNewNoteName.trim()}
                  className="p-1 rounded text-brand hover:bg-brand/10 transition-colors disabled:opacity-40"
                >
                  <Check size={11} />
                </button>
                <button
                  onClick={() => { setDocNewNoteCreating(false); setDocNewNoteName(""); }}
                  className="p-1 rounded text-text-muted hover:text-error hover:bg-error/10 transition-colors"
                >
                  <X size={11} />
                </button>
              </div>
            )}

            {/* Note list */}
            <div className="flex-1 overflow-y-auto">
              {(() => {
                const noteEntries = Object.entries(docs).filter(
                  ([, v]) => v.path.startsWith(".automatic/docs/")
                );
                if (noteEntries.length === 0) {
                  return (
                    <p className="px-3 py-4 text-[11px] text-text-muted italic">
                      No notes yet. Click + to create one.
                    </p>
                  );
                }
                return noteEntries.map(([key, entry]) => (
                  <div
                    key={key}
                    className={`group flex items-center gap-2 px-3 py-2 cursor-pointer transition-colors ${
                      docNoteSelected === key
                        ? "bg-brand/10 text-text-base"
                        : "hover:bg-surface-hover text-text-muted hover:text-text-base"
                    }`}
                    onClick={() => {
                      if (docNoteSelected !== key) loadDocNote(key);
                    }}
                  >
                    <FileText size={12} className="flex-shrink-0" />
                    <span className="flex-1 text-[12px] truncate">{entry.summary || key}</span>
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        removeDocEntry(key, true);
                      }}
                      className="opacity-0 group-hover:opacity-100 p-0.5 rounded text-text-muted hover:text-error hover:bg-error/10 transition-all"
                      title="Delete note"
                    >
                      <Trash2 size={10} />
                    </button>
                  </div>
                ));
              })()}
            </div>
          </div>

          {/* Right panel: editor */}
          <div className="flex-1 flex flex-col min-h-0">
            {docNoteSelected === null ? (
              <div className="flex-1 flex items-center justify-center text-center p-8">
                <div>
                  <FileText size={28} className="mx-auto mb-3 text-text-muted opacity-40" strokeWidth={1.5} />
                  <p className="text-[13px] text-text-muted">Select a note to edit, or create a new one.</p>
                </div>
              </div>
            ) : docNoteLoading ? (
              <div className="flex-1 flex items-center justify-center text-text-muted">
                <RefreshCw size={14} className="animate-spin mr-2" />
                <span className="text-[13px]">Loading…</span>
              </div>
            ) : (
              <>
                {/* Note toolbar */}
                <div className="flex items-center justify-between px-4 h-9 bg-bg-input border-b border-border-strong/40 flex-shrink-0">
                  <span className="text-[11px] text-text-muted font-mono">
                    .automatic/docs/{docNoteSelected}.md
                    {docNoteDirty ? " (unsaved)" : ""}
                  </span>
                  <button
                    onClick={saveDocNote}
                    disabled={!docNoteDirty || docNoteSaving}
                    className="flex items-center gap-1 px-2 py-0.5 text-[11px] text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                  >
                    <Check size={10} /> {docNoteSaving ? "Saving…" : "Save"}
                  </button>
                </div>
                {/* Markdown textarea */}
                <LineNumberedTextarea
                  value={docNoteContent}
                  onChange={(v) => {
                    setDocNoteContent(v);
                    setDocNoteDirty(true);
                  }}
                  className="flex-1 min-h-0"
                  placeholder="Write Markdown here…"
                />
              </>
            )}
          </div>
        </>
      )}
    </div>
  );
}
