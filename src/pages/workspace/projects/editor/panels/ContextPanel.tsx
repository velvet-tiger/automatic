// Extracted verbatim from ProjectEditor.tsx (Phase 2E — behavior-preserving).

import type { ReactNode } from "react";
import { AlertCircle, ArrowRight, Brain, Check, Code, Edit2, Plus, RefreshCw, ScrollText, Sparkles } from "lucide-react";
import { LineNumberedTextarea } from "../../../../../components/LineNumberedTextarea";
import { TokenPill } from "../../../../../components/TokenPill";
import type { Project } from "../../types";

interface ProjectContext {
  commands: Record<string, string>;
  entry_points: Record<string, string>;
  concepts: Record<string, { summary: string; files: string[] }>;
  conventions: Record<string, string>;
  gotchas: Record<string, string>;
}

interface ContextPanelProps {
  project: Project;
  selectedName: string | null;
  loadingContext: boolean;
  contextFileExists: boolean;
  contextEditing: boolean;
  setContextEditing: (v: boolean) => void;
  contextRaw: string;
  setContextRaw: (v: string) => void;
  contextDirty: boolean;
  setContextDirty: (v: boolean) => void;
  contextSaving: boolean;
  contextGenerating: boolean;
  contextJsonError: string | null;
  setContextJsonError: (v: string | null) => void;
  agentFeaturesEnabled: boolean;
  projectContext: ProjectContext | null;
  handleGenerateContext: () => void | Promise<void>;
  handleSaveContext: () => void | Promise<void>;
  loadContext: (name: string) => Promise<void>;
}

export function ContextPanel({
  project, selectedName,
  loadingContext, contextFileExists,
  contextEditing, setContextEditing,
  contextRaw, setContextRaw,
  contextDirty, setContextDirty,
  contextSaving, contextGenerating,
  contextJsonError, setContextJsonError,
  agentFeaturesEnabled,
  projectContext,
  handleGenerateContext, handleSaveContext, loadContext,
}: ContextPanelProps) {
  return (
    <div className="flex-1 flex flex-col min-h-0">
      {!project?.directory ? (
        <div className="flex-1 flex items-center justify-center">
          <p className="text-[13px] text-text-muted italic">
            Set a project directory to use context.
          </p>
        </div>
      ) : loadingContext ? (
        <div className="flex-1 flex items-center justify-center text-text-muted">
          <RefreshCw size={14} className="animate-spin mr-2" />
          <span className="text-[13px]">Loading…</span>
        </div>
      ) : !contextFileExists && !contextEditing ? (
        <div className="flex-1 flex flex-col items-center justify-center text-center p-8">
          <div className="w-12 h-12 mx-auto mb-4 rounded-full border border-dashed border-border-strong flex items-center justify-center text-text-muted">
            <Brain size={20} strokeWidth={1.5} />
          </div>
          <h3 className="text-[14px] font-medium text-text-base mb-1">No context file</h3>
          <p className="text-[13px] text-text-muted mb-1 max-w-xs">
            Create <code className="font-mono text-[12px]">.automatic/context.json</code> to give agents structured knowledge about this project.
          </p>
          <p className="text-[12px] text-text-muted mb-5 max-w-sm">
            Define commands, entry points, architecture concepts, conventions, and gotchas.
          </p>
          <div className="flex items-center gap-2">
            <span className="relative group/keytip">
              <button
                onClick={handleGenerateContext}
                disabled={contextGenerating || !agentFeaturesEnabled}
                className="px-3 py-1.5 bg-brand hover:bg-brand-hover text-white text-[12px] font-medium rounded shadow-sm transition-colors flex items-center gap-1.5 disabled:opacity-50 disabled:cursor-not-allowed"
              >
                <Sparkles size={12} className={contextGenerating ? "animate-pulse" : ""} />
                {contextGenerating ? "Generating…" : "Generate with AI"}
              </button>
              {!agentFeaturesEnabled && (
                <span className="pointer-events-none absolute bottom-full left-1/2 -translate-x-1/2 mb-1.5 whitespace-nowrap rounded bg-bg-input-dark border border-border-strong/40 px-2 py-1 text-[11px] text-text-base shadow-md opacity-0 group-hover/keytip:opacity-100 transition-opacity z-10">
                  Enable Agent features to access
                </span>
              )}
            </span>
            <button
              onClick={() => {
                const template = JSON.stringify({
                  commands: { build: "npm run build", test: "npm test" },
                  entry_points: { app: "src/main.ts" },
                  concepts: { example: { summary: "Describe a key concept here", files: [] } },
                  conventions: { naming: "Describe a naming convention" },
                  gotchas: {},
                }, null, 2);
                setContextRaw(template);
                setContextEditing(true);
                setContextDirty(true);
                setContextJsonError(null);
              }}
              className="px-3 py-1.5 bg-bg-input hover:bg-surface-hover border border-border-strong/50 text-text-muted hover:text-text-base text-[12px] font-medium rounded shadow-sm transition-colors flex items-center gap-1.5"
            >
              <Plus size={12} /> Create manually
            </button>
          </div>
          {contextJsonError && (
            <div className="flex items-start gap-2 mt-4 px-4 py-2 bg-error/10 border border-error/30 rounded-lg max-w-sm">
              <AlertCircle size={12} className="text-error mt-0.5 flex-shrink-0" />
              <span className="text-[11px] text-error font-mono">{contextJsonError}</span>
            </div>
          )}
        </div>
      ) : (
        <div className="flex-1 flex flex-col min-h-0">
          <div className="flex items-center justify-between px-4 h-9 bg-bg-input border-b border-border-strong/40 flex-shrink-0">
            <div className="flex items-center gap-2 min-w-0">
              <span className="text-[11px] text-text-muted font-mono">
                .automatic/context.json
                {!contextFileExists ? " (new)" : ""}
                {contextEditing ? " — Editing" : ""}
                {contextDirty ? " (unsaved)" : ""}
              </span>
              <TokenPill text={contextRaw} />
            </div>
            <div className="flex items-center gap-1.5">
              <span className="relative group/keytip">
                <button
                  onClick={handleGenerateContext}
                  disabled={contextGenerating || contextSaving || !agentFeaturesEnabled}
                  className="flex items-center gap-1 px-2 py-0.5 text-[11px] text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  <Sparkles size={10} className={contextGenerating ? "animate-pulse text-brand" : ""} />
                  {contextGenerating ? "Generating…" : "Generate"}
                </button>
                {!agentFeaturesEnabled && (
                  <span className="pointer-events-none absolute bottom-full left-1/2 -translate-x-1/2 mb-1.5 whitespace-nowrap rounded bg-bg-input-dark border border-border-strong/40 px-2 py-1 text-[11px] text-text-base shadow-md opacity-0 group-hover/keytip:opacity-100 transition-opacity z-10">
                    Enable Agent features to access
                  </span>
                )}
              </span>
              <div className="w-px h-3 bg-border-strong/40" />
              {!contextEditing ? (
                <button
                  onClick={() => setContextEditing(true)}
                  className="flex items-center gap-1 px-2 py-0.5 text-[11px] text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
                >
                  <Edit2 size={10} /> Edit
                </button>
              ) : (
                <>
                  <button
                    onClick={() => {
                      setContextEditing(false);
                      setContextJsonError(null);
                      if (contextDirty && selectedName) {
                        if (contextFileExists) {
                          loadContext(selectedName);
                        } else {
                          setContextRaw("");
                          setContextDirty(false);
                        }
                      }
                    }}
                    className="px-2 py-0.5 text-[11px] text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
                  >
                    Cancel
                  </button>
                  <button
                    onClick={handleSaveContext}
                    disabled={!contextDirty || contextSaving}
                    className="flex items-center gap-1 px-2 py-0.5 text-[11px] bg-brand hover:bg-brand-hover text-white rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                  >
                    <Check size={10} /> {contextSaving ? "Saving…" : "Save"}
                  </button>
                </>
              )}
            </div>
          </div>

          {contextJsonError && (
            <div className="flex items-start gap-2 px-4 py-2 bg-error/10 border-b border-error/30 flex-shrink-0">
              <AlertCircle size={12} className="text-error mt-0.5 flex-shrink-0" />
              <span className="text-[11px] text-error font-mono">{contextJsonError}</span>
            </div>
          )}

          {contextEditing ? (
            <LineNumberedTextarea
              value={contextRaw}
              onChange={(v) => {
                setContextRaw(v);
                setContextDirty(true);
                setContextJsonError(null);
              }}
              className="flex-1 min-h-0"
              placeholder={`{\n  "commands": {},\n  "concepts": {},\n  "conventions": {},\n  "gotchas": {}\n}`}
            />
          ) : (
            <div className="flex-1 overflow-y-auto custom-scrollbar p-6 space-y-5">
              {(() => {
                const ctx = projectContext;
                if (!ctx) return <span className="text-[13px] text-text-muted italic">Empty file.</span>;
                const sections: ReactNode[] = [];

                if (Object.keys(ctx.commands).length > 0)
                  sections.push(
                    <div key="commands">
                      <div className="flex items-center gap-2 mb-2">
                        <Code size={12} className="text-text-muted" />
                        <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Commands</span>
                      </div>
                      <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
                        {Object.entries(ctx.commands).map(([name, cmd]) => (
                          <div key={name} className="flex items-start gap-3 px-4 py-2.5">
                            <span className="text-[12px] font-medium text-text-base w-32 flex-shrink-0 pt-px">{name}</span>
                            <code className="text-[11px] font-mono text-text-muted break-all">{cmd}</code>
                          </div>
                        ))}
                      </div>
                    </div>
                  );

                if (Object.keys(ctx.entry_points).length > 0)
                  sections.push(
                    <div key="entry_points">
                      <div className="flex items-center gap-2 mb-2">
                        <ArrowRight size={12} className="text-text-muted" />
                        <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Entry Points</span>
                      </div>
                      <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
                        {Object.entries(ctx.entry_points).map(([name, path]) => (
                          <div key={name} className="flex items-start gap-3 px-4 py-2.5">
                            <span className="text-[12px] font-medium text-text-base w-32 flex-shrink-0 pt-px">{name}</span>
                            <code className="text-[11px] font-mono text-text-muted break-all">{path}</code>
                          </div>
                        ))}
                      </div>
                    </div>
                  );

                if (Object.keys(ctx.concepts).length > 0)
                  sections.push(
                    <div key="concepts">
                      <div className="flex items-center gap-2 mb-2">
                        <Brain size={12} className="text-text-muted" />
                        <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Architecture Concepts</span>
                      </div>
                      <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
                        {Object.entries(ctx.concepts).map(([name, concept]) => (
                          <div key={name} className="px-4 py-3 space-y-1.5">
                            <span className="text-[12px] font-semibold text-text-base block">{name}</span>
                            <p className="text-[12px] text-text-muted leading-relaxed">{concept.summary}</p>
                            {concept.files.length > 0 && (
                              <div className="flex flex-wrap gap-1.5">
                                {concept.files.map((f) => (
                                  <code key={f} className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-bg-sidebar border border-border-strong/30 text-text-muted">{f}</code>
                                ))}
                              </div>
                            )}
                          </div>
                        ))}
                      </div>
                    </div>
                  );

                if (Object.keys(ctx.conventions).length > 0)
                  sections.push(
                    <div key="conventions">
                      <div className="flex items-center gap-2 mb-2">
                        <ScrollText size={12} className="text-text-muted" />
                        <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Conventions</span>
                      </div>
                      <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
                        {Object.entries(ctx.conventions).map(([name, desc]) => (
                          <div key={name} className="px-4 py-2.5 space-y-0.5">
                            <span className="text-[12px] font-medium text-text-base block">{name}</span>
                            <p className="text-[12px] text-text-muted leading-relaxed">{desc}</p>
                          </div>
                        ))}
                      </div>
                    </div>
                  );

                if (Object.keys(ctx.gotchas).length > 0)
                  sections.push(
                    <div key="gotchas">
                      <div className="flex items-center gap-2 mb-2">
                        <AlertCircle size={12} className="text-text-muted" />
                        <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Gotchas</span>
                      </div>
                      <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
                        {Object.entries(ctx.gotchas).map(([name, desc]) => (
                          <div key={name} className="px-4 py-2.5 space-y-0.5">
                            <span className="text-[12px] font-medium text-text-base block">{name}</span>
                            <p className="text-[12px] text-text-muted leading-relaxed">{desc}</p>
                          </div>
                        ))}
                      </div>
                    </div>
                  );

                return sections.length > 0
                  ? <>{sections}</>
                  : <span className="text-[13px] text-text-muted italic">Empty context file. Click Edit to add content.</span>;
              })()}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
