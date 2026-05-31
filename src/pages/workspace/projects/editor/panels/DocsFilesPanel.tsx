// Extracted verbatim from ProjectEditor.tsx (Phase 2E — behavior-preserving).

import { FileText, FolderOpen, FolderPlus, Plus, Trash2 } from "lucide-react";
import { getProjectRelativeDocPath } from "../../helpers";
import type { Project } from "../../types";

type DocEntry = { path: string; summary?: string };

interface DocsFilesPanelProps {
  project: Project;
  fileDocEntries: [string, DocEntry][];
  showDocPathForm: boolean;
  setShowDocPathForm: (v: boolean) => void;
  docNewPath: string;
  setDocNewPath: (v: string) => void;
  docNewPathSummary: string;
  setDocNewPathSummary: (v: string) => void;
  handleAddDocPath: () => Promise<void>;
  handleBrowseDocPath: () => Promise<void>;
  handleBrowseDocFile: () => Promise<void>;
  removeDocEntry: (key: string, isNote: boolean) => Promise<void>;
}

export function DocsFilesPanel({
  project,
  fileDocEntries,
  showDocPathForm,
  setShowDocPathForm,
  docNewPath,
  setDocNewPath,
  docNewPathSummary,
  setDocNewPathSummary,
  handleAddDocPath,
  handleBrowseDocPath,
  handleBrowseDocFile,
  removeDocEntry,
}: DocsFilesPanelProps) {
  return (
    <div className="flex-1 flex flex-col min-h-0 overflow-y-auto">
      {!project?.directory ? (
        <div className="flex-1 flex items-center justify-center">
          <p className="text-[13px] text-text-muted italic">
            Set a project directory to use documentation.
          </p>
        </div>
      ) : (
        <div className="space-y-4">
          {/* Header row */}
          <div className="flex items-start justify-between gap-4">
            <div className="min-w-0">
              <h3 className="text-[13px] font-semibold text-text-base">Files &amp; Directories</h3>
              <p className="mt-1 text-[12px] text-text-muted max-w-[820px]">
                Add local folders, specs, or standalone files to include as project documentation. Stored in <code className="font-mono text-[11px]">.automatic/docs.json</code> and surfaced to agents via MCP.
              </p>
            </div>
            {!showDocPathForm && (
              <button
                onClick={() => {
                  setShowDocPathForm(true);
                  setTimeout(() => {
                    const input = document.getElementById("docs-path-input") as HTMLInputElement | null;
                    input?.focus();
                  }, 50);
                }}
                className="shrink-0 flex items-center gap-1.5 px-3 py-1.5 bg-brand hover:bg-brand-hover text-white text-[12px] font-medium rounded-md shadow-sm transition-colors"
              >
                <Plus size={12} /> Add
              </button>
            )}
          </div>

          {/* Collapsible add form */}
          {showDocPathForm && (
            <div className="rounded-lg border border-brand/30 bg-bg-input overflow-hidden">
              <div className="flex items-center justify-between px-3 py-2 border-b border-border-strong/20">
                <span className="text-[11px] font-semibold uppercase tracking-wide text-text-muted">Add path</span>
                <button
                  onClick={() => {
                    setShowDocPathForm(false);
                    setDocNewPath("");
                    setDocNewPathSummary("");
                  }}
                  className="text-[11px] text-text-muted hover:text-text-base transition-colors"
                >
                  Cancel
                </button>
              </div>
              <div className="px-3 py-3">
                <div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_180px]">
                  <div className="space-y-3 min-w-0">
                    <div>
                      <label className="mb-1.5 block text-[11px] font-semibold uppercase tracking-wide text-text-muted">Path</label>
                      <input
                        id="docs-path-input"
                        type="text"
                        value={docNewPath}
                        onChange={(e) => setDocNewPath(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") { e.preventDefault(); void handleAddDocPath(); }
                          if (e.key === "Escape") { setShowDocPathForm(false); setDocNewPath(""); setDocNewPathSummary(""); }
                        }}
                        placeholder="/path/to/specs or ./docs/architecture.md"
                        className="w-full rounded-md border border-border-strong/40 bg-bg-sidebar px-3 py-2 text-[12px] text-text-base placeholder-text-muted outline-none transition-colors focus:border-brand/60"
                      />
                    </div>
                    <div>
                      <label className="mb-1.5 block text-[11px] font-semibold uppercase tracking-wide text-text-muted">Description</label>
                      <input
                        type="text"
                        value={docNewPathSummary}
                        onChange={(e) => setDocNewPathSummary(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") { e.preventDefault(); void handleAddDocPath(); }
                          if (e.key === "Escape") { setShowDocPathForm(false); setDocNewPath(""); setDocNewPathSummary(""); }
                        }}
                        placeholder="What should agents use this for?"
                        className="w-full rounded-md border border-border-strong/40 bg-bg-sidebar px-3 py-2 text-[12px] text-text-base placeholder-text-muted outline-none transition-colors focus:border-brand/60"
                      />
                    </div>
                  </div>
                  <div className="flex flex-col gap-2 justify-end">
                    <button
                      onClick={handleBrowseDocPath}
                      className="w-full rounded-md border border-border-strong/40 bg-bg-sidebar px-3 py-2 text-[12px] font-medium text-text-muted transition-colors hover:bg-surface-hover hover:text-text-base flex items-center justify-center gap-1.5"
                      title="Pick a directory"
                    >
                      <FolderOpen size={12} /> Browse Dir
                    </button>
                    <button
                      onClick={handleBrowseDocFile}
                      className="w-full rounded-md border border-border-strong/40 bg-bg-sidebar px-3 py-2 text-[12px] font-medium text-text-muted transition-colors hover:bg-surface-hover hover:text-text-base flex items-center justify-center gap-1.5"
                      title="Pick a file"
                    >
                      <FileText size={12} /> Browse File
                    </button>
                    <button
                      onClick={handleAddDocPath}
                      disabled={!docNewPath.trim()}
                      className="w-full inline-flex items-center justify-center gap-1.5 rounded-md bg-brand px-3 py-2 text-[12px] font-medium text-white shadow-sm transition-colors hover:bg-brand-hover disabled:cursor-not-allowed disabled:opacity-50"
                    >
                      <Plus size={12} /> Add path
                    </button>
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* Paths list */}
          {fileDocEntries.length === 0 ? (
            <div className="rounded-lg border border-dashed border-border-strong/40 px-4 py-10 text-center">
              <div className="mx-auto mb-3 flex h-11 w-11 items-center justify-center rounded-full border border-dashed border-border-strong/50 bg-bg-sidebar/50">
                <FolderPlus size={16} className="text-text-muted" />
              </div>
              <h5 className="text-[13px] font-medium text-text-base">No documentation paths yet</h5>
              <p className="mx-auto mt-1 max-w-[420px] text-[12px] leading-relaxed text-text-muted">
                Add architecture docs, spec folders, generated references, or any local files agents should read alongside the project.
              </p>
            </div>
          ) : (
            <div className="rounded-lg border border-border-strong/40 overflow-hidden">
              <div className="flex items-center justify-between gap-3 px-3 py-2.5 border-b border-border-strong/20 bg-bg-input">
                <span className="text-[11px] font-semibold uppercase tracking-wide text-text-muted">Included paths</span>
                <span className="text-[10px] font-semibold px-2 py-0.5 rounded-full bg-bg-sidebar border border-border-strong/40 text-text-muted">
                  {fileDocEntries.length}
                </span>
              </div>
              <div className="divide-y divide-border-strong/20">
                {fileDocEntries.map(([key, entry]) => {
                  const relativePath = getProjectRelativeDocPath(project.directory, entry.path);
                  const displayPath = relativePath
                    ? relativePath === "." ? project.directory : `./${relativePath}`
                    : entry.path;
                  return (
                    <div key={key} className="group flex items-center gap-3 px-3 py-2.5 transition-colors hover:bg-surface-hover/50">
                      <div className="flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-md border border-border-strong/30 bg-bg-sidebar/60">
                        <FolderOpen size={13} className="text-text-muted" />
                      </div>
                      <div className="min-w-0 flex-1">
                        <div className="flex flex-wrap items-baseline gap-2">
                          <span className="text-[12px] font-medium text-text-base truncate">{entry.summary || key}</span>
                          <span className="shrink-0 rounded-full border border-border-strong/30 bg-bg-sidebar px-1.5 py-px text-[10px] text-text-muted">
                            {relativePath ? "In project" : "Absolute path"}
                          </span>
                        </div>
                        <p className="mt-0.5 font-mono text-[11px] text-text-muted/80 truncate" title={displayPath}>
                          {displayPath}
                        </p>
                      </div>
                      <button
                        onClick={() => removeDocEntry(key, false)}
                        className="opacity-0 group-hover:opacity-100 rounded-md border border-transparent p-1.5 text-text-muted transition-all hover:border-error/20 hover:bg-error/10 hover:text-error"
                        title="Remove path"
                      >
                        <Trash2 size={13} />
                      </button>
                    </div>
                  );
                })}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
