// Extracted verbatim from Projects.tsx (behavior-preserving refactor).

import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getToolPanel } from "../../../../plugins";
import { handleExternalLinkClick } from "../../../../lib/externalLinks";
import { CheckCircle2, ExternalLink, Globe, MinusCircle, Plus, Puzzle, RefreshCw, Terminal, Wrench, X } from "lucide-react";
import { projectToolKindLabel } from "../helpers";
import { ProjectToolAvatar } from "../EditorIcon";
import type { ProjectToolEntry } from "../types";

interface ProjectToolsTabProps {
  projectDir: string;
  /** Tool names explicitly added to this project (saved state). */
  projectTools: string[];
  /** Tool entries already loaded by the parent (avoids duplicate fetches). */
  entries: ProjectToolEntry[];
  loading: boolean;
  /** Called by auto-detect to request a re-fetch of entries after detection. */
  onReload: () => void;
  /** Called when the user adds or removes a tool, or auto-detects. New full list provided. */
  onToolsChange: (tools: string[]) => void;
}

export function ProjectToolsTab({ projectDir, projectTools, entries, loading, onReload, onToolsChange }: ProjectToolsTabProps) {
  const [detecting, setDetecting] = useState(false);
  const [detectStatus, setDetectStatus] = useState<string | null>(null);

  /** A tool is in this project only if explicitly listed in projectTools. */
  function isInProject(name: string): boolean {
    return projectTools.includes(name);
  }

  function addTool(name: string) {
    if (isInProject(name)) return;
    onToolsChange([...projectTools, name]);
  }

  function removeTool(name: string) {
    onToolsChange(projectTools.filter((t) => t !== name));
  }

  async function handleAutoDetect() {
    if (!projectDir) {
      setDetectStatus("Set a project directory first.");
      setTimeout(() => setDetectStatus(null), 3000);
      return;
    }
    setDetecting(true);
    setDetectStatus(null);
    try {
      const detected: string[] = await invoke("autodetect_tools_for_project", { projectDir });
      if (detected.length === 0) {
        setDetectStatus("No tools detected in this project directory.");
      } else {
        // Merge detected into existing list without duplicates.
        const merged = [...new Set([...projectTools, ...detected])];
        onToolsChange(merged);
        onReload();
        const added = detected.filter((n) => !projectTools.includes(n));
        setDetectStatus(
          added.length > 0
            ? `Detected and added: ${added.join(", ")}`
            : "Already up to date — no new tools found."
        );
      }
    } catch (err) {
      console.error("Auto-detect failed:", err);
      setDetectStatus("Detection failed. Check the console for details.");
    } finally {
      setDetecting(false);
      setTimeout(() => setDetectStatus(null), 5000);
    }
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center py-12 text-[12px] text-text-muted">
        Loading tools…
      </div>
    );
  }

  if (entries.length === 0) {
    return (
      <section>
        <div className="flex flex-col items-center justify-center gap-4 text-center py-12 px-6">
          <Wrench size={32} className="text-text-muted opacity-40" />
          <div>
            <p className="text-[13px] font-medium text-text-base mb-1">No tools registered</p>
            <p className="text-[12px] text-text-muted leading-relaxed max-w-[320px]">
              Tools are installed by plugins. Enable a plugin in{" "}
              <span className="font-medium text-text-base">Settings → Plugins</span>{" "}
              to register tools here.
            </p>
          </div>
        </div>
      </section>
    );
  }

  const activeEntries = entries.filter((e) => isInProject(e.name));
  const otherEntries  = entries.filter((e) => !isInProject(e.name));

  return (
    <section className="space-y-4">
      {/* Header row: info + auto-detect button */}
      <div className="flex items-center gap-3">
        <div className="flex-1 flex items-start gap-2.5 px-3 py-2.5 rounded-lg bg-bg-input border border-border-strong/30 text-[12px] text-text-muted">
          <Wrench size={13} className="flex-shrink-0 mt-0.5" />
          <span>
            Tools are registered by plugins. Add them manually or use Auto-detect to scan this project.
            {!projectDir && (
              <span className="text-warning ml-1">Set a project directory to enable detection.</span>
            )}
          </span>
        </div>
        <button
          onClick={handleAutoDetect}
          disabled={detecting || !projectDir}
          className="flex items-center gap-1.5 px-3 py-2 text-[12px] font-medium border border-border-strong/50 rounded-lg text-text-muted hover:text-text-base hover:bg-bg-input transition-colors disabled:opacity-40 flex-shrink-0"
          title={!projectDir ? "Set a project directory first" : "Scan the project directory for installed tools"}
        >
          <RefreshCw size={12} className={detecting ? "animate-spin" : ""} />
          {detecting ? "Detecting…" : "Auto-detect"}
        </button>
      </div>

      {/* Status message */}
      {detectStatus && (
        <p className="text-[12px] text-text-muted px-1">{detectStatus}</p>
      )}

      {/* Active in this project */}
      {activeEntries.length > 0 && (
        <div>
          <div className="flex items-center gap-2 mb-2">
            <CheckCircle2 size={12} className="text-green-400" />
            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
              In this project
            </span>
          </div>
          <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
            {activeEntries.map((entry) => (
              <ProjectToolRow
                key={entry.name}
                entry={entry}
                active
                onRemove={() => removeTool(entry.name)}
              />
            ))}
          </div>
        </div>
      )}

      {/* Available tools not in this project */}
      {otherEntries.length > 0 && (
        <div>
          <div className="flex items-center gap-2 mb-2">
            <MinusCircle size={12} className="text-text-muted" />
            <span className="text-[11px] font-semibold text-text-muted tracking-wider uppercase">
              Available tools
            </span>
          </div>
          <div className="bg-bg-input border border-border-strong/40 rounded-lg overflow-hidden divide-y divide-border-strong/20">
            {otherEntries.map((entry) => (
              <ProjectToolRow
                key={entry.name}
                entry={entry}
                active={false}
                onAdd={() => addTool(entry.name)}
              />
            ))}
          </div>
        </div>
      )}
    </section>
  );
}

// ── ToolInfoSidebar ───────────────────────────────────────────────────────────
// Compact right-hand sidebar shown on every tool detail page.

interface ToolInfoSidebarProps {
  entry: ProjectToolEntry;
  active: boolean;
  onAdd: () => void;
  onRemove: () => void;
}

export function ToolInfoSidebar({ entry, active, onAdd, onRemove }: ToolInfoSidebarProps) {
  return (
    <aside className="w-56 flex-shrink-0 flex flex-col gap-4">
      {/* Avatar + name */}
      <div className="flex flex-col items-center gap-2 text-center">
        <ProjectToolAvatar tool={entry} size={44} />
        <div>
          <p className="text-[13px] font-semibold text-text-base leading-tight">{entry.display_name}</p>
          <code className="text-[11px] font-mono text-text-muted">{entry.name}</code>
        </div>
        <span className="flex items-center gap-1 text-[10px] px-1.5 py-0.5 rounded bg-bg-input border border-border-strong/40 text-text-muted">
          <Terminal size={9} />
          {projectToolKindLabel(entry.kind)}
        </span>
      </div>

      {/* Status + action */}
      <div className="flex flex-col gap-2">
        {active ? (
          <>
            <span className="flex items-center justify-center gap-1 text-[11px] text-green-400">
              <CheckCircle2 size={12} /> Active in project
            </span>
            <button
              onClick={onRemove}
              className="w-full flex items-center justify-center gap-1.5 px-3 py-1.5 text-[12px] font-medium rounded-lg border border-danger/30 text-danger hover:bg-danger/10 transition-colors"
            >
              <X size={12} /> Remove
            </button>
          </>
        ) : (
          <>
            <span className="flex items-center justify-center gap-1 text-[11px] text-text-muted">
              <MinusCircle size={12} /> Not in project
            </span>
            <button
              onClick={onAdd}
              className="w-full flex items-center justify-center gap-1.5 px-3 py-1.5 text-[12px] font-medium rounded-lg border border-brand/40 text-brand hover:bg-brand/10 transition-colors"
            >
              <Plus size={12} /> Add to project
            </button>
          </>
        )}
      </div>

      {/* Metadata rows */}
      <div className="flex flex-col gap-1.5 text-[11px]">
        {entry.description && (
          <p className="text-text-muted leading-relaxed">{entry.description}</p>
        )}
        {entry.url && (
          <a
            href={entry.url}
            target="_blank"
            rel="noreferrer"
            onClick={handleExternalLinkClick(entry.url)}
            className="flex items-center gap-1 text-brand hover:underline truncate"
          >
            <Globe size={10} className="flex-shrink-0" />
            {entry.url}
            <ExternalLink size={10} className="flex-shrink-0" />
          </a>
        )}
        {entry.github_repo && (
          <a
            href={`https://github.com/${entry.github_repo}`}
            target="_blank"
            rel="noreferrer"
            onClick={handleExternalLinkClick(`https://github.com/${entry.github_repo}`)}
            className="flex items-center gap-1 text-brand hover:underline truncate"
          >
            <Globe size={10} className="flex-shrink-0" />
            {entry.github_repo}
            <ExternalLink size={10} className="flex-shrink-0" />
          </a>
        )}
        {entry.detect_dir && (
          <div className="flex items-center gap-1 text-text-muted font-mono">
            <span className="text-[9px] px-1 py-0.5 rounded bg-bg-input border border-border-strong/40 uppercase tracking-wider non-mono text-[9px]">dir</span>
            {entry.detect_dir}/
          </div>
        )}
        {entry.detect_binary && (
          <div className="flex items-center gap-1 text-text-muted font-mono">
            <span className="text-[9px] px-1 py-0.5 rounded bg-bg-input border border-border-strong/40 uppercase tracking-wider non-mono text-[9px]">bin</span>
            {entry.detect_binary}
            {entry.detected === true && <span className="non-mono text-green-400 text-[10px]">✓</span>}
            {entry.detected === false && <span className="non-mono text-text-muted text-[10px]">✗</span>}
          </div>
        )}
        {entry.plugin_id && (
          <div className="flex items-center gap-1 text-text-muted">
            <Puzzle size={10} className="flex-shrink-0" />
            <span className="font-mono">{entry.plugin_id}</span>
          </div>
        )}
      </div>
    </aside>
  );
}

// ── ProjectToolRow ─────────────────────────────────────────────────────────────

interface ProjectToolRowProps {
  entry: ProjectToolEntry;
  active: boolean;
  onAdd?: () => void;
  onRemove?: () => void;
}

function ProjectToolRow({ entry, active, onAdd, onRemove }: ProjectToolRowProps) {
  return (
    <div className="flex items-center gap-3 px-4 py-3 hover:bg-surface-hover transition-colors group">
      <ProjectToolAvatar tool={entry} size={28} />
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2 flex-wrap">
          <span className="text-[13px] font-medium text-text-base truncate">
            {entry.display_name}
          </span>
          <span className="flex items-center gap-1 text-[10px] px-1.5 py-0.5 rounded bg-bg-sidebar border border-border-strong/40 text-text-muted">
            <Terminal size={9} />
            {projectToolKindLabel(entry.kind)}
          </span>
        </div>
        {entry.description && (
          <p className="text-[11px] text-text-muted mt-0.5 truncate">{entry.description}</p>
        )}
        <div className="flex items-center gap-3 mt-1 flex-wrap">
          {entry.detect_dir && (
            <span className="flex items-center gap-1 text-[11px] text-text-muted font-mono">
              <Globe size={10} />
              {entry.detect_dir}/
            </span>
          )}
          {entry.detect_binary && (
            <span className="flex items-center gap-1 text-[11px] text-text-muted font-mono">
              <Terminal size={10} />
              {entry.detect_binary}
            </span>
          )}
          {entry.plugin_id && (
            <span className="flex items-center gap-1 text-[11px] text-text-muted">
              <Puzzle size={10} />
              {entry.plugin_id}
            </span>
          )}
          {entry.url && (
            <a
              href={entry.url}
              target="_blank"
              rel="noreferrer"
              onClick={handleExternalLinkClick(entry.url)}
              className="flex items-center gap-1 text-[11px] text-brand hover:underline"
            >
              <ExternalLink size={10} />
              Docs
            </a>
          )}
        </div>
      </div>
      <div className="flex-shrink-0 flex items-center gap-2">
        {active ? (
          <>
            <span className="flex items-center gap-1 text-[11px] text-green-400">
              <CheckCircle2 size={11} />
              Active
            </span>
            <button
              onClick={onRemove}
              className="opacity-0 group-hover:opacity-100 p-1 rounded text-text-muted hover:text-danger hover:bg-danger/10 transition-all"
              title="Remove from project"
            >
              <X size={12} />
            </button>
          </>
        ) : (
          <button
            onClick={onAdd}
            className="flex items-center gap-1 text-[11px] font-medium px-2 py-1 rounded border border-border-strong/50 text-text-muted hover:text-text-base hover:bg-bg-sidebar transition-colors"
            title="Add to project"
          >
            <Plus size={11} />
            Add
          </button>
        )}
      </div>
    </div>
  );
}

// ── ProjectToolDetailPanel ────────────────────────────────────────────────────

interface ProjectToolDetailPanelProps {
  entry: ProjectToolEntry;
  projectDir: string;
  active: boolean;
  onAdd: () => void;
  onRemove: () => void;
}

export function ProjectToolDetailPanel({ entry, projectDir, active, onAdd, onRemove }: ProjectToolDetailPanelProps) {
  const CustomPanel = getToolPanel(entry.name);
  
  if (CustomPanel) {
    return (
      <CustomPanel
        projectDir={projectDir}
        sidebar={<ToolInfoSidebar entry={entry} active={active} onAdd={onAdd} onRemove={onRemove} />}
      />
    );
  }

  return (
    <div className="flex gap-6 items-start">
      <section className="flex-1 min-w-0">
        <div className="flex flex-col items-center justify-center gap-3 py-16 text-center text-text-muted">
          <ProjectToolAvatar tool={entry} size={40} />
          <p className="text-[12px]">No additional detail view for this tool.</p>
        </div>
      </section>
      <ToolInfoSidebar entry={entry} active={active} onAdd={onAdd} onRemove={onRemove} />
    </div>
  );
}
