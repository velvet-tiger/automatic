// Extracted verbatim from ProjectEditor.tsx (Phase 2E — behavior-preserving).

import type { MouseEventHandler } from "react";
import { ExternalLink, Globe, Link as LinkIcon, Plus, Trash2 } from "lucide-react";
import type { Project } from "../../types";

type DocEntry = { path: string; summary?: string };

interface DocsLinksPanelProps {
  project: Project;
  linkDocEntries: [string, DocEntry][];
  showDocLinkForm: boolean;
  setShowDocLinkForm: (v: boolean) => void;
  docNewLinkUrl: string;
  setDocNewLinkUrl: (v: string) => void;
  docNewLinkLabel: string;
  setDocNewLinkLabel: (v: string) => void;
  handleAddDocLink: () => Promise<void>;
  removeDocEntry: (key: string, isNote: boolean) => Promise<void>;
  handleExternalLinkClick: (url: string, isExternal?: boolean) => MouseEventHandler<HTMLAnchorElement>;
}

export function DocsLinksPanel({
  project,
  linkDocEntries,
  showDocLinkForm,
  setShowDocLinkForm,
  docNewLinkUrl,
  setDocNewLinkUrl,
  docNewLinkLabel,
  setDocNewLinkLabel,
  handleAddDocLink,
  removeDocEntry,
  handleExternalLinkClick,
}: DocsLinksPanelProps) {
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
              <h3 className="text-[13px] font-semibold text-text-base">Links</h3>
              <p className="mt-1 text-[12px] text-text-muted max-w-[820px]">
                Add URLs to external documentation, design specs, or reference material so this project keeps its key web resources in one place.
              </p>
            </div>
            {!showDocLinkForm && (
              <button
                onClick={() => {
                  setShowDocLinkForm(true);
                  setTimeout(() => {
                    const input = document.getElementById("docs-link-input") as HTMLInputElement | null;
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
          {showDocLinkForm && (
            <div className="rounded-lg border border-brand/30 bg-bg-input overflow-hidden">
              <div className="flex items-center justify-between px-3 py-2 border-b border-border-strong/20">
                <span className="text-[11px] font-semibold uppercase tracking-wide text-text-muted">Add link</span>
                <button
                  onClick={() => {
                    setShowDocLinkForm(false);
                    setDocNewLinkUrl("");
                    setDocNewLinkLabel("");
                  }}
                  className="text-[11px] text-text-muted hover:text-text-base transition-colors"
                >
                  Cancel
                </button>
              </div>
              <div className="px-3 py-3">
                <div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_220px_120px]">
                  <div>
                    <label className="mb-1.5 block text-[11px] font-semibold uppercase tracking-wide text-text-muted">URL</label>
                    <input
                      id="docs-link-input"
                      type="url"
                      value={docNewLinkUrl}
                      onChange={(e) => setDocNewLinkUrl(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") { e.preventDefault(); void handleAddDocLink(); }
                        if (e.key === "Escape") { setShowDocLinkForm(false); setDocNewLinkUrl(""); setDocNewLinkLabel(""); }
                      }}
                      placeholder="https://docs.example.com/reference"
                      className="w-full rounded-md border border-border-strong/40 bg-bg-sidebar px-3 py-2 text-[12px] text-text-base placeholder-text-muted outline-none transition-colors focus:border-brand/60"
                    />
                  </div>
                  <div>
                    <label className="mb-1.5 block text-[11px] font-semibold uppercase tracking-wide text-text-muted">Label</label>
                    <input
                      type="text"
                      value={docNewLinkLabel}
                      onChange={(e) => setDocNewLinkLabel(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") { e.preventDefault(); void handleAddDocLink(); }
                        if (e.key === "Escape") { setShowDocLinkForm(false); setDocNewLinkUrl(""); setDocNewLinkLabel(""); }
                      }}
                      placeholder="What is this link useful for?"
                      className="w-full rounded-md border border-border-strong/40 bg-bg-sidebar px-3 py-2 text-[12px] text-text-base placeholder-text-muted outline-none transition-colors focus:border-brand/60"
                    />
                  </div>
                  <div className="flex items-end">
                    <button
                      onClick={handleAddDocLink}
                      disabled={!docNewLinkUrl.trim()}
                      className="w-full inline-flex items-center justify-center gap-1.5 rounded-md bg-brand px-3 py-2 text-[12px] font-medium text-white shadow-sm transition-colors hover:bg-brand-hover disabled:cursor-not-allowed disabled:opacity-50"
                    >
                      <Plus size={12} /> Add link
                    </button>
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* Links list */}
          {linkDocEntries.length === 0 ? (
            <div className="rounded-lg border border-dashed border-border-strong/40 px-4 py-10 text-center">
              <div className="mx-auto mb-3 flex h-11 w-11 items-center justify-center rounded-full border border-dashed border-border-strong/50 bg-bg-sidebar/50">
                <LinkIcon size={16} className="text-text-muted" />
              </div>
              <h5 className="text-[13px] font-medium text-text-base">No external references yet</h5>
              <p className="mx-auto mt-1 max-w-[420px] text-[12px] leading-relaxed text-text-muted">
                Add product docs, API references, Figma files, tickets, or other URLs that help explain how this project works.
              </p>
            </div>
          ) : (
            <div className="rounded-lg border border-border-strong/40 overflow-hidden">
              <div className="flex items-center justify-between gap-3 px-3 py-2.5 border-b border-border-strong/20 bg-bg-input">
                <span className="text-[11px] font-semibold uppercase tracking-wide text-text-muted">Saved links</span>
                <span className="text-[10px] font-semibold px-2 py-0.5 rounded-full bg-bg-sidebar border border-border-strong/40 text-text-muted">
                  {linkDocEntries.length}
                </span>
              </div>
              <div className="divide-y divide-border-strong/20">
                {linkDocEntries.map(([key, entry]) => (
                  <div key={key} className="group flex items-center gap-3 px-3 py-2.5 transition-colors hover:bg-surface-hover/50">
                    <div className="flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-md border border-border-strong/30 bg-bg-sidebar/60">
                      <Globe size={13} className="text-text-muted" />
                    </div>
                    <div className="min-w-0 flex-1">
                      <div className="flex flex-wrap items-baseline gap-2">
                        <span className="text-[12px] font-medium text-text-base truncate">{entry.summary || key}</span>
                        <span className="shrink-0 rounded-full border border-border-strong/30 bg-bg-sidebar px-1.5 py-px text-[10px] text-text-muted">external</span>
                      </div>
                      <p className="mt-0.5 font-mono text-[11px] text-text-muted/80 truncate" title={entry.path}>
                        {entry.path}
                      </p>
                    </div>
                    <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                       <a
                         href={entry.path}
                         target="_blank"
                         rel="noopener noreferrer"
                         onClick={handleExternalLinkClick(entry.path, true)}
                         className="rounded-md border border-border-strong/30 p-1.5 text-text-muted transition-colors hover:border-brand/30 hover:bg-brand/10 hover:text-brand"
                         title="Open link"
                       >
                        <ExternalLink size={13} />
                      </a>
                      <button
                        onClick={() => removeDocEntry(key, false)}
                        className="rounded-md border border-transparent p-1.5 text-text-muted transition-colors hover:border-error/20 hover:bg-error/10 hover:text-error"
                        title="Remove link"
                      >
                        <Trash2 size={13} />
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
