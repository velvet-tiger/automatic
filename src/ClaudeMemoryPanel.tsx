import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  BookOpen,
  ChevronDown,
  ChevronRight,
  ArrowUpRight,
  RefreshCw,
  Plus,
} from "lucide-react";

// ── Types ────────────────────────────────────────────────────────────────────

interface ClaudeMemoryTopicFile {
  name: string;
  content: string;
}

interface ClaudeMemoryContent {
  memory_dir: string;
  memory_md: string | null;
  topic_files: ClaudeMemoryTopicFile[];
}

interface PromoteModalProps {
  initialKey: string;
  initialValue: string;
  projectName: string;
  onClose: () => void;
  onPromoted: () => void;
}

// ── Promote to Automatic Modal ───────────────────────────────────────────────

function PromoteModal({
  initialKey,
  initialValue,
  projectName,
  onClose,
  onPromoted,
}: PromoteModalProps) {
  const [key, setKey] = useState(initialKey);
  const [value, setValue] = useState(initialValue);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSave = async () => {
    const trimmedKey = key.trim();
    const trimmedValue = value.trim();
    if (!trimmedKey || !trimmedValue) return;
    try {
      setSaving(true);
      setError(null);
      await invoke("store_memory", {
        project: projectName,
        key: trimmedKey,
        value: trimmedValue,
        source: "claude-auto-memory",
      });
      onPromoted();
      onClose();
    } catch (err: any) {
      setError(`Failed to promote: ${err}`);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-bg-input border border-border-strong/40 rounded-xl shadow-xl w-full max-w-lg mx-4 p-5">
        <div className="flex items-center justify-between mb-4">
          <div>
            <h3 className="text-[14px] font-semibold text-text-base">Promote to Automatic Memory</h3>
            <p className="text-[11px] text-text-muted mt-0.5">
              Save this Claude learning as a structured Automatic memory entry.
            </p>
          </div>
          <button
            onClick={onClose}
            className="text-text-muted hover:text-text-base p-1 rounded transition-colors"
          >
            ×
          </button>
        </div>
        <div className="space-y-3">
          <div>
            <label className="text-[11px] font-medium text-text-muted uppercase tracking-wide mb-1 block">
              Key
            </label>
            <input
              type="text"
              value={key}
              onChange={(e) => setKey(e.target.value)}
              placeholder="e.g. conventions/naming"
              className="w-full text-[13px] text-text-base font-mono bg-bg-base border border-border-strong/40 rounded px-3 py-2 focus:outline-none focus:border-brand"
              autoFocus
            />
          </div>
          <div>
            <label className="text-[11px] font-medium text-text-muted uppercase tracking-wide mb-1 block">
              Value
            </label>
            <textarea
              value={value}
              onChange={(e) => setValue(e.target.value)}
              rows={6}
              className="w-full text-[13px] text-text-base font-mono bg-bg-base border border-border-strong/40 rounded px-3 py-2 focus:outline-none focus:border-brand resize-none custom-scrollbar"
            />
          </div>
          {error && (
            <p className="text-[12px] text-danger">{error}</p>
          )}
        </div>
        <div className="flex items-center justify-end gap-2 mt-4">
          <button
            onClick={onClose}
            className="px-3 py-1.5 text-[12px] font-medium text-text-muted hover:text-text-base bg-bg-sidebar hover:bg-surface rounded transition-colors"
          >
            Cancel
          </button>
          <button
            disabled={saving || key.trim() === "" || value.trim() === ""}
            onClick={handleSave}
            className="px-3 py-1.5 text-[12px] font-medium bg-brand hover:bg-brand-hover disabled:opacity-50 disabled:cursor-not-allowed text-white rounded transition-colors"
          >
            {saving ? "Saving…" : "Promote"}
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Topic File Section ───────────────────────────────────────────────────────

interface TopicFileSectionProps {
  file: ClaudeMemoryTopicFile;
  projectName: string;
  onPromoted: () => void;
}

function TopicFileSection({ file, projectName, onPromoted }: TopicFileSectionProps) {
  const [expanded, setExpanded] = useState(false);
  const [promoteModal, setPromoteModal] = useState<{ key: string; value: string } | null>(null);

  const suggestedKey = `claude-memory/${file.name.replace(/\.md$/, "")}`;

  return (
    <div className="border border-border-strong/40 rounded-lg overflow-hidden">
      <div
        className="flex items-center gap-2 px-3 py-2.5 bg-bg-input cursor-pointer hover:bg-surface transition-colors select-none"
        onClick={() => setExpanded((v) => !v)}
      >
        <span className="text-text-muted shrink-0">
          {expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        </span>
        <span className="text-[12px] font-semibold text-text-base font-mono flex-1 truncate">
          {file.name}
        </span>
        <button
          onClick={(e) => {
            e.stopPropagation();
            setPromoteModal({ key: suggestedKey, value: file.content });
          }}
          className="flex items-center gap-1 px-2 py-1 text-[11px] font-medium text-brand hover:bg-brand/10 rounded transition-colors shrink-0"
          title="Promote this file to Automatic memory"
        >
          <Plus size={11} /> Promote
        </button>
      </div>
      {expanded && (
        <div className="px-3 py-3 bg-bg-base border-t border-border-strong/40">
          <pre className="text-[12px] text-text-base whitespace-pre-wrap font-mono leading-relaxed max-h-72 overflow-y-auto custom-scrollbar">
            {file.content}
          </pre>
        </div>
      )}
      {promoteModal && (
        <PromoteModal
          initialKey={promoteModal.key}
          initialValue={promoteModal.value}
          projectName={projectName}
          onClose={() => setPromoteModal(null)}
          onPromoted={onPromoted}
        />
      )}
    </div>
  );
}

// ── Main Component ────────────────────────────────────────────────────────────

interface ClaudeMemoryPanelProps {
  /** Automatic project name (used as key for store_memory calls). */
  projectName: string;
  /** Absolute path to the project directory (used to derive the Claude memory path). */
  projectDirectory: string;
  /** Called when a memory entry has been successfully promoted. */
  onPromoted: () => void;
}

export function ClaudeMemoryPanel({
  projectName,
  projectDirectory,
  onPromoted,
}: ClaudeMemoryPanelProps) {
  const [content, setContent] = useState<ClaudeMemoryContent | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [memoryMdExpanded, setMemoryMdExpanded] = useState(true);
  const [promoteMemoryMd, setPromoteMemoryMd] = useState(false);

  const load = useCallback(async () => {
    if (!projectDirectory) return;
    try {
      setLoading(true);
      setError(null);
      const result = await invoke<ClaudeMemoryContent>("get_claude_memory", {
        project: projectName,
      });
      setContent(result);
    } catch (err: any) {
      setError(`${err}`);
    } finally {
      setLoading(false);
    }
  }, [projectName, projectDirectory]);

  useEffect(() => {
    load();
  }, [load]);

  if (!projectDirectory) return null;

  const hasAnyContent =
    content && (content.memory_md != null || content.topic_files.length > 0);

  return (
    <section className="mt-6 pt-5 border-t border-border-strong/40">
      {/* Header */}
      <div className="flex items-start justify-between mb-3">
        <div className="flex items-center gap-2">
          <BookOpen size={13} className="text-text-muted shrink-0 mt-0.5" />
          <div>
            <h3 className="text-[14px] font-medium text-text-base">Claude Auto-Memory</h3>
            <p className="text-[12px] text-text-muted mt-0.5">
              Learnings Claude Code wrote itself during previous sessions.{" "}
              <span className="text-text-muted/70">Promote entries to save them in Automatic.</span>
            </p>
          </div>
        </div>
        <button
          onClick={load}
          disabled={loading}
          className="p-1.5 text-text-muted hover:text-text-base hover:bg-surface rounded transition-colors disabled:opacity-40"
          title="Refresh"
        >
          <RefreshCw size={13} className={loading ? "animate-spin" : ""} />
        </button>
      </div>

      {/* Directory path */}
      {content && (
        <p className="text-[11px] font-mono text-text-muted mb-3 truncate">
          {content.memory_dir.replace(/^\/Users\/[^/]+/, "~")}
        </p>
      )}

      {/* Loading */}
      {loading && (
        <div className="text-[13px] text-text-muted text-center py-6">Loading…</div>
      )}

      {/* Error */}
      {!loading && error && (
        <div className="text-[12px] text-danger bg-danger/5 border border-danger/20 rounded p-3">
          {error}
        </div>
      )}

      {/* Empty state */}
      {!loading && !error && content && !hasAnyContent && (
        <div className="text-center py-8 bg-bg-input rounded-lg border border-border-strong/40 border-dashed">
          <BookOpen size={18} className="mx-auto text-text-muted mb-2" />
          <p className="text-[13px] font-medium text-text-base mb-1">No auto-memory yet</p>
          <p className="text-[12px] text-text-muted max-w-sm mx-auto">
            Claude Code hasn't written any auto-memory for this project. It will appear here once Claude saves learnings during a session.
          </p>
        </div>
      )}

      {/* Content */}
      {!loading && !error && content && hasAnyContent && (
        <div className="space-y-2">
          {/* MEMORY.md */}
          {content.memory_md != null && (
            <div className="border border-border-strong/40 rounded-lg overflow-hidden">
              <div
                className="flex items-center gap-2 px-3 py-2.5 bg-bg-input cursor-pointer hover:bg-surface transition-colors select-none"
                onClick={() => setMemoryMdExpanded((v) => !v)}
              >
                <span className="text-text-muted shrink-0">
                  {memoryMdExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                </span>
                <span className="text-[12px] font-semibold text-text-base font-mono flex-1">
                  MEMORY.md
                </span>
                <span className="text-[10px] px-1.5 py-0.5 rounded bg-bg-sidebar text-text-muted border border-border-strong/40 shrink-0">
                  index · loaded every session
                </span>
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    setPromoteMemoryMd(true);
                  }}
                  className="flex items-center gap-1 px-2 py-1 text-[11px] font-medium text-brand hover:bg-brand/10 rounded transition-colors shrink-0"
                  title="Promote MEMORY.md contents to Automatic memory"
                >
                  <Plus size={11} /> Promote
                </button>
              </div>
              {memoryMdExpanded && (
                <div className="px-3 py-3 bg-bg-base border-t border-border-strong/40">
                  <pre className="text-[12px] text-text-base whitespace-pre-wrap font-mono leading-relaxed max-h-72 overflow-y-auto custom-scrollbar">
                    {content.memory_md}
                  </pre>
                </div>
              )}
            </div>
          )}

          {/* Topic files */}
          {content.topic_files.map((file) => (
            <TopicFileSection
              key={file.name}
              file={file}
              projectName={projectName}
              onPromoted={onPromoted}
            />
          ))}

          {/* Footer: link to docs */}
          <div className="flex items-center gap-1 pt-1">
            <a
              href="https://code.claude.com/docs/en/memory#auto-memory"
              target="_blank"
              rel="noopener noreferrer"
              className="flex items-center gap-1 text-[11px] text-text-muted hover:text-brand transition-colors"
            >
              About Claude auto-memory <ArrowUpRight size={11} />
            </a>
          </div>
        </div>
      )}

      {/* Promote MEMORY.md modal */}
      {promoteMemoryMd && content?.memory_md != null && (
        <PromoteModal
          initialKey="claude-memory/MEMORY"
          initialValue={content.memory_md}
          projectName={projectName}
          onClose={() => setPromoteMemoryMd(false)}
          onPromoted={onPromoted}
        />
      )}
    </section>
  );
}
