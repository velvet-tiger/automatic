// Extracted verbatim from ProjectEditor.tsx (Phase 2E — behavior-preserving).

import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { Check, Edit2, FileText, Files, LayoutTemplate, Plus, RefreshCw, Sparkles, SplitSquareHorizontal } from "lucide-react";
import { LineNumberedTextarea } from "../../../../../components/LineNumberedTextarea";
import { MarkdownPreview } from "../../../../../components/MarkdownPreview";
import { TokenPill } from "../../../../../components/TokenPill";
import type { Project, ProjectFileInfo, UnifiedCandidate, UnifiedInspection } from "../../types";

interface ProjectFilePanelProps {
  project: Project;
  setProject: (next: Project) => void;
  setDirty: (v: boolean) => void;
  selectedName: string | null;
  projectFiles: ProjectFileInfo[];
  activeProjectFile: string | null;
  setActiveProjectFile: (v: string | null) => void;
  projectFileContent: string;
  setProjectFileContent: (v: string) => void;
  projectFileEditing: boolean;
  setProjectFileEditing: (v: boolean) => void;
  projectFileDirty: boolean;
  setProjectFileDirty: (v: boolean) => void;
  projectFileSaving: boolean;
  projectFileGenerating: boolean;
  projectFileUpdating: boolean;
  agentFeaturesEnabled: boolean;
  availableTemplates: string[];
  showTemplatePicker: boolean;
  setShowTemplatePicker: (v: boolean) => void;
  loadProjectFiles: (name: string) => Promise<void>;
  loadProjectFileContent: (name: string, filename: string) => Promise<void>;
  notifyProjectUpdated: () => void;
  handleGenerateInstruction: () => void | Promise<void>;
  handleUpdateInstruction: () => void | Promise<void>;
  handleSaveProjectFile: () => void | Promise<void>;
  handleApplyTemplate: (template: string) => void | Promise<void>;
  setUnifiedSourcePicker: (v: UnifiedCandidate[] | null) => void;
}

export function ProjectFilePanel(props: ProjectFilePanelProps) {
  const {
    project, setProject, setDirty, selectedName,
    projectFiles, activeProjectFile, setActiveProjectFile,
    projectFileContent, setProjectFileContent,
    projectFileEditing, setProjectFileEditing,
    projectFileDirty, setProjectFileDirty,
    projectFileSaving, projectFileGenerating, projectFileUpdating,
    agentFeaturesEnabled,
    availableTemplates, showTemplatePicker, setShowTemplatePicker,
    loadProjectFiles, loadProjectFileContent, notifyProjectUpdated,
    handleGenerateInstruction, handleUpdateInstruction, handleSaveProjectFile, handleApplyTemplate,
    setUnifiedSourcePicker,
  } = props;

  return (
    <>
      {project.directory && project.agents.length > 0 ? (
        <div className="flex-1 flex flex-col min-h-0">
          {/* Mode toggle bar */}
          <div className="flex items-center gap-3 px-4 py-2.5 border-b border-border-strong/40 bg-bg-input/30 flex-shrink-0">
            <span className="text-[11px] text-text-muted">Mode:</span>
            <div className="flex rounded overflow-hidden border border-border-strong/40">
              <button
                onClick={async () => {
                  if (project.instruction_mode === "unified" || !selectedName) {
                    return;
                  }
                  let inspection: UnifiedInspection;
                  try {
                    const raw = await invoke<string>("inspect_unified_candidates", {
                      name: selectedName,
                    });
                    inspection = JSON.parse(raw) as UnifiedInspection;
                  } catch (e) {
                    console.error("inspect_unified_candidates failed", e);
                    return;
                  }
                  if (inspection.candidates.length === 0) {
                    const updated = { ...project, instruction_mode: "unified", updated_at: new Date().toISOString() };
                    setProject(updated);
                    setDirty(false);
                    await invoke("save_project", { name: selectedName, data: JSON.stringify(updated, null, 2) });
                    await loadProjectFiles(selectedName);
                    notifyProjectUpdated();
                    return;
                  }
                  setUnifiedSourcePicker(inspection.candidates);
                }}
                className={`flex items-center gap-1.5 px-2.5 py-1 text-[11px] font-medium transition-colors ${
                  (project.instruction_mode || "per-agent") === "unified"
                    ? "bg-brand text-white"
                    : "bg-bg-sidebar text-text-muted hover:text-text-base"
                }`}
              >
                <Files size={11} />
                Unified
              </button>
              <button
                onClick={async () => {
                  if (project.instruction_mode !== "per-agent" && selectedName) {
                    const updated = { ...project, instruction_mode: "per-agent", updated_at: new Date().toISOString() };
                    setProject(updated);
                    setDirty(false);
                    await invoke("save_project", { name: selectedName, data: JSON.stringify(updated, null, 2) });
                    await loadProjectFiles(selectedName);
                    notifyProjectUpdated();
                  }
                }}
                className={`flex items-center gap-1.5 px-2.5 py-1 text-[11px] font-medium transition-colors ${
                  (project.instruction_mode || "per-agent") === "per-agent"
                    ? "bg-brand text-white"
                    : "bg-bg-sidebar text-text-muted hover:text-text-base"
                }`}
              >
                <SplitSquareHorizontal size={11} />
                Per Agent
              </button>
            </div>
            {(project.instruction_mode || "per-agent") === "unified" && projectFiles.length > 0 && projectFiles[0]!.target_files && (
              <span className="text-[10px] text-text-muted">
                Writes to: {projectFiles[0]!.target_files.join(", ")}
              </span>
            )}
          </div>

          <div className="flex-1 flex min-h-0">
          {/* File sidebar — hidden in unified mode */}
          {(project.instruction_mode || "per-agent") === "per-agent" && projectFiles.length > 0 && (
            <div className="w-52 flex-shrink-0 border-r border-border-strong/40 bg-bg-input/50 flex flex-col">
              <div className="h-9 px-3 border-b border-border-strong/40 flex items-center justify-between">
                <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">Files</span>
                <button
                  onClick={() => setShowTemplatePicker(!showTemplatePicker)}
                  className="text-text-muted hover:text-text-base p-0.5 hover:bg-bg-sidebar rounded transition-colors"
                  title="Start from template"
                >
                  <LayoutTemplate size={12} />
                </button>
              </div>
              <div className="flex-1 overflow-y-auto py-1.5 custom-scrollbar">
                <ul className="space-y-0.5 px-1.5">
                  {projectFiles.map((f) => (
                    <li key={f.filename}>
                      <button
                        onClick={async () => {
                          if (projectFileDirty && !(await ask("Discard unsaved changes?", { title: "Unsaved Changes", kind: "warning" }))) return;
                          setActiveProjectFile(f.filename);
                          if (selectedName) await loadProjectFileContent(selectedName, f.filename);
                        }}
                        className={`w-full text-left px-2.5 py-1.5 rounded-md text-[13px] font-medium transition-colors flex items-center gap-2 ${
                          activeProjectFile === f.filename
                            ? "bg-bg-sidebar text-text-base"
                            : "text-text-muted hover:bg-bg-sidebar/50 hover:text-text-base"
                        }`}
                      >
                        <FileText size={13} className={activeProjectFile === f.filename ? "text-text-base" : f.exists ? "text-text-muted" : "text-text-muted"} />
                        <div className="min-w-0">
                          <div className={`truncate ${!f.exists ? "opacity-50" : ""}`}>{f.filename}</div>
                          <div className="text-[10px] text-text-muted truncate">{f.agents.join(", ")}</div>
                        </div>
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
              {showTemplatePicker && availableTemplates.length > 0 && (
                <div className="border-t border-border-strong/40 p-2">
                  <p className="text-[10px] text-text-muted mb-1.5">Apply template:</p>
                  <div className="space-y-0.5">
                    {availableTemplates.map((t) => (
                      <button
                        key={t}
                        onClick={() => handleApplyTemplate(t)}
                        className="w-full text-left px-2 py-1 text-[12px] bg-bg-sidebar hover:bg-brand text-text-base hover:text-white rounded transition-colors flex items-center gap-1.5"
                      >
                        <LayoutTemplate size={10} />
                        {t}
                      </button>
                    ))}
                  </div>
                </div>
              )}
            </div>
          )}

          {/* Editor area */}
          {projectFiles.length > 0 && activeProjectFile ? (() => {
            const activeFile = projectFiles.find(f => f.filename === activeProjectFile);
            const fileExists = activeFile?.exists ?? false;

            if (!fileExists && !projectFileEditing) {
              return (
                <div className="flex-1 flex flex-col items-center justify-center text-center p-8">
                  <div className="w-12 h-12 mx-auto mb-4 rounded-full border border-dashed border-border-strong flex items-center justify-center text-text-muted">
                    <FileText size={20} strokeWidth={1.5} />
                  </div>
                  <h3 className="text-[14px] font-medium text-text-base mb-1">
                    {activeProjectFile === "_unified" ? "Shared File" : activeProjectFile}
                  </h3>
                  <p className="text-[13px] text-text-muted mb-5 max-w-xs">
                    This file doesn't exist yet. Create it to provide project instructions for {activeFile?.agents.join(" & ")}.
                  </p>
                  <div className="flex items-center gap-2">
                    <span className="relative group/keytip">
                      <button
                        onClick={handleGenerateInstruction}
                        disabled={projectFileGenerating || !agentFeaturesEnabled}
                        className="px-3 py-1.5 bg-brand hover:bg-brand-hover text-white text-[12px] font-medium rounded shadow-sm transition-colors flex items-center gap-1.5 disabled:opacity-50 disabled:cursor-not-allowed"
                      >
                        <Sparkles size={12} className={projectFileGenerating ? "animate-pulse" : ""} />
                        {projectFileGenerating ? "Generating…" : "Generate with AI"}
                      </button>
                      {!agentFeaturesEnabled && (
                        <span className="pointer-events-none absolute bottom-full left-1/2 -translate-x-1/2 mb-1.5 whitespace-nowrap rounded bg-bg-input-dark border border-border-strong/40 px-2 py-1 text-[11px] text-text-base shadow-md opacity-0 group-hover/keytip:opacity-100 transition-opacity z-10">
                          Enable Agent features to access
                        </span>
                      )}
                    </span>
                    <button
                      onClick={() => {
                        setProjectFileContent("");
                        setProjectFileEditing(true);
                        setProjectFileDirty(true);
                      }}
                      className="px-3 py-1.5 bg-bg-sidebar hover:bg-surface text-text-base text-[12px] font-medium rounded border border-border-strong/40 transition-colors flex items-center gap-1.5"
                    >
                      <Plus size={12} /> Create File
                    </button>
                    {availableTemplates.length > 0 && (
                      <button
                        onClick={() => setShowTemplatePicker(!showTemplatePicker)}
                        className="px-3 py-1.5 bg-bg-sidebar hover:bg-surface text-text-base text-[12px] font-medium rounded border border-border-strong/40 transition-colors flex items-center gap-1.5"
                      >
                        <LayoutTemplate size={12} /> From Template
                      </button>
                    )}
                  </div>
                  {showTemplatePicker && availableTemplates.length > 0 && (
                    <div className="mt-3 p-2 bg-bg-input rounded-md border border-border-strong/40">
                      <div className="flex flex-wrap gap-1.5">
                        {availableTemplates.map((t) => (
                          <button
                            key={t}
                            onClick={() => handleApplyTemplate(t)}
                            className="px-2 py-1 text-[12px] bg-bg-sidebar hover:bg-brand text-text-base hover:text-white rounded transition-colors flex items-center gap-1.5"
                          >
                            <LayoutTemplate size={10} />
                            {t}
                          </button>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              );
            }

            return (
              <div className="flex-1 flex min-w-0 min-h-0">
                <div className="flex-1 flex flex-col min-w-0">
                  <div className="flex items-center justify-between px-4 h-9 bg-bg-input border-b border-border-strong/40 flex-shrink-0">
                    <div className="flex items-center gap-2 min-w-0">
                      <span className="text-[11px] text-text-muted">
                        {activeProjectFile === "_unified"
                          ? <>{projectFileEditing ? "Editing" : ""}{projectFileDirty ? " (unsaved)" : ""}</>
                          : <>{activeProjectFile}{!fileExists ? " (new)" : ""}{projectFileEditing ? " — Editing" : ""}{projectFileDirty ? " (unsaved)" : ""}</>
                        }
                      </span>
                      <TokenPill text={projectFileContent} />
                    </div>
                    <div className="flex items-center gap-1.5">
                      {(fileExists || projectFileContent.trim().length > 0) && (
                        <span className="relative group/keytip">
                          <button
                            onClick={handleUpdateInstruction}
                            disabled={projectFileUpdating || projectFileGenerating || projectFileSaving || !agentFeaturesEnabled || !projectFileContent.trim()}
                            className="flex items-center gap-1 px-2 py-0.5 text-[11px] text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                          >
                            <RefreshCw size={10} className={projectFileUpdating ? "animate-spin text-brand" : ""} />
                            {projectFileUpdating ? "Updating…" : "Update"}
                          </button>
                          {!agentFeaturesEnabled && (
                            <span className="pointer-events-none absolute bottom-full left-1/2 -translate-x-1/2 mb-1.5 whitespace-nowrap rounded bg-bg-input-dark border border-border-strong/40 px-2 py-1 text-[11px] text-text-base shadow-md opacity-0 group-hover/keytip:opacity-100 transition-opacity z-10">
                              Enable Agent features to access
                            </span>
                          )}
                        </span>
                      )}
                      <span className="relative group/keytip">
                        <button
                          onClick={handleGenerateInstruction}
                          disabled={projectFileGenerating || projectFileSaving || !agentFeaturesEnabled}
                          className="flex items-center gap-1 px-2 py-0.5 text-[11px] text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                        >
                          <Sparkles size={10} className={projectFileGenerating ? "animate-pulse text-brand" : ""} />
                          {projectFileGenerating ? "Generating…" : "Generate"}
                        </button>
                        {!agentFeaturesEnabled && (
                          <span className="pointer-events-none absolute bottom-full left-1/2 -translate-x-1/2 mb-1.5 whitespace-nowrap rounded bg-bg-input-dark border border-border-strong/40 px-2 py-1 text-[11px] text-text-base shadow-md opacity-0 group-hover/keytip:opacity-100 transition-opacity z-10">
                            Enable Agent features to access
                          </span>
                        )}
                      </span>
                      <span className="w-px h-3 bg-border-strong/40" />
                      {!projectFileEditing ? (
                        <button
                          onClick={() => setProjectFileEditing(true)}
                          className="flex items-center gap-1 px-2 py-0.5 text-[11px] text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
                        >
                          <Edit2 size={10} /> Edit
                        </button>
                      ) : (
                        <>
                          <button
                            onClick={() => {
                              setProjectFileEditing(false);
                              if (projectFileDirty && selectedName && activeProjectFile) {
                                if (fileExists) {
                                  loadProjectFileContent(selectedName, activeProjectFile);
                                } else {
                                  setProjectFileContent("");
                                  setProjectFileDirty(false);
                                }
                              }
                            }}
                            className="px-2 py-0.5 text-[11px] text-text-muted hover:text-text-base hover:bg-bg-sidebar rounded transition-colors"
                          >
                            Cancel
                          </button>
                          <button
                            onClick={handleSaveProjectFile}
                            disabled={!projectFileDirty || projectFileSaving}
                            className="flex items-center gap-1 px-2 py-0.5 text-[11px] bg-brand hover:bg-brand-hover text-white rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                          >
                            <Check size={10} /> {projectFileSaving ? "Saving..." : "Save"}
                          </button>
                        </>
                      )}
                    </div>
                  </div>

                  {projectFileEditing ? (
                    <LineNumberedTextarea
                      value={projectFileContent}
                      onChange={(v) => {
                        setProjectFileContent(v);
                        setProjectFileDirty(true);
                      }}
                      className="flex-1 min-h-0"
                      placeholder="Write your project instructions here..."
                    />
                  ) : (
                    <div className="flex-1 overflow-y-auto custom-scrollbar bg-bg-base min-h-0">
                      {projectFileContent
                        ? <MarkdownPreview content={projectFileContent} />
                        : <span className="block p-4 text-[13px] text-text-muted italic">Empty file.</span>
                      }
                    </div>
                  )}
                </div>
              </div>
            );
          })() : (
            <div className="flex-1 flex items-center justify-center">
              <p className="text-[13px] text-text-muted italic">No project files configured. Add agent tools on the Agents tab first.</p>
            </div>
          )}
          </div>
        </div>
      ) : (
        <div className="flex-1 flex items-center justify-center">
          <p className="text-[13px] text-text-muted italic">
            Set a project directory and add agent tools on the Details and Agents tabs to manage project files.
          </p>
        </div>
      )}
    </>
  );
}
