import { useState, useEffect, useCallback, useId } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  BookOpen,
  ChevronDown,
  ChevronRight,
  ArrowUpRight,
  RefreshCw,
  Plus,
  Check,
  X,
} from "lucide-react";
import { handleExternalLinkClick } from "../lib/externalLinks";

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

/** The part of an Automatic memory entry the panel compares against. */
interface StoredMemory {
  value: string;
}

/** Automatic's memory entries for this project, by key. */
type StoredMemories = Record<string, StoredMemory>;

/**
 * Whether `record` has `key` as its own property. This is `Object.hasOwn`,
 * which the project's ES2020 lib target does not declare. It stops a key
 * such as "constructor" or "__proto__" matching an inherited property.
 */
function hasOwnKey(record: object, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(record, key);
}

/** What the user promoted a file as: the key they saved under, and the file content at that moment. */
export interface Promotion {
  key: string;
  content: string;
}

/**
 * How a Claude memory file relates to Automatic's memory store.
 * - "promoted": stored and up to date.
 * - "changed": promoted in this session, and the file has changed since.
 * - "differs": the suggested key is stored, but its value is not the file's
 *   content. The user may have edited it on promotion, or the file changed.
 */
export type PromotionStatus = "none" | "promoted" | "changed" | "differs";

/**
 * The status of a file with `content`. A promotion recorded this session
 * wins, because the user may have saved under another key or edited the
 * value. Otherwise the suggested key is compared with the file's trimmed
 * content, which is what PromoteModal saves by default.
 */
export function promotionStatus(
  stored: StoredMemories,
  suggestedKey: string,
  content: string,
  promotion?: Promotion,
): PromotionStatus {
  if (promotion) {
    if (!hasOwnKey(stored, promotion.key)) return "none";
    return content === promotion.content ? "promoted" : "changed";
  }
  if (!hasOwnKey(stored, suggestedKey)) return "none";
  return stored[suggestedKey].value === content.trim() ? "promoted" : "differs";
}

function PromotionBadge({ status }: { status: PromotionStatus }) {
  if (status === "none") return null;
  if (status === "promoted") {
    return (
      <span className="flex items-center gap-1 text-[10px] px-1.5 py-0.5 rounded bg-success/10 text-success border border-success/30 shrink-0">
        <Check size={10} /> Promoted
      </span>
    );
  }
  const changed = status === "changed";
  return (
    <span
      className="text-[10px] px-1.5 py-0.5 rounded bg-warning/10 text-warning border border-warning/30 shrink-0"
      title={
        changed
          ? "This file has changed since it was promoted. Promote it again to update the Automatic memory."
          : "Automatic has a memory under this file's suggested key with a different value."
      }
    >
      {changed ? "Changed since promoted" : "Differs from Automatic memory"}
    </span>
  );
}

interface PromoteModalProps {
  initialKey: string;
  initialValue: string;
  /** The project's local_key. */
  projectKey: string;
  /**
   * Automatic's existing memories, used to warn before replacing one.
   * Undefined while they are not known for this project: no warning shows.
   */
  storedMemories: StoredMemories | undefined;
  onClose: () => void;
  /** Called with the key the entry was saved under. */
  onPromoted: (key: string) => void;
}

// ── Promote to Automatic Modal ───────────────────────────────────────────────

function PromoteModal({
  initialKey,
  initialValue,
  projectKey,
  storedMemories,
  onClose,
  onPromoted,
}: PromoteModalProps) {
  const [key, setKey] = useState(initialKey);
  const [value, setValue] = useState(initialValue);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const keyInputId = useId();
  const valueInputId = useId();

  const handleSave = async () => {
    const trimmedKey = key.trim();
    const trimmedValue = value.trim();
    if (!trimmedKey || !trimmedValue) return;
    try {
      setSaving(true);
      setError(null);
      await invoke("store_memory", {
        project: projectKey,
        key: trimmedKey,
        value: trimmedValue,
        source: "claude-auto-memory",
      });
      onPromoted(trimmedKey);
      onClose();
    } catch (err: unknown) {
      setError(`Failed to promote: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setSaving(false);
    }
  };

  const replacesExisting = storedMemories !== undefined && hasOwnKey(storedMemories, key.trim());

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Promote to Automatic Memory"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
    >
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
            <label htmlFor={keyInputId} className="text-[11px] font-medium text-text-muted uppercase tracking-wide mb-1 block">
              Key
            </label>
            <input
              id={keyInputId}
              type="text"
              value={key}
              onChange={(e) => setKey(e.target.value)}
              placeholder="e.g. conventions/naming"
              className="w-full text-[13px] text-text-base font-mono bg-bg-base border border-border-strong/40 rounded px-3 py-2 focus:outline-none focus:border-brand"
              autoFocus
            />
          </div>
          <div>
            <label htmlFor={valueInputId} className="text-[11px] font-medium text-text-muted uppercase tracking-wide mb-1 block">
              Value
            </label>
            <textarea
              id={valueInputId}
              value={value}
              onChange={(e) => setValue(e.target.value)}
              rows={6}
              className="w-full text-[13px] text-text-base font-mono bg-bg-base border border-border-strong/40 rounded px-3 py-2 focus:outline-none focus:border-brand resize-none custom-scrollbar"
            />
          </div>
          {replacesExisting && (
            <p className="text-[12px] text-warning">
              A memory with this key already exists. Promoting replaces its value.
            </p>
          )}
          {error && (
            <p role="alert" className="text-[12px] text-danger">{error}</p>
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
            {saving ? "Saving…" : replacesExisting ? "Replace" : "Promote"}
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Topic File Section ───────────────────────────────────────────────────────

interface TopicFileSectionProps {
  file: ClaudeMemoryTopicFile;
  projectKey: string;
  storedMemories: StoredMemories | undefined;
  /** What this file was promoted as in this session, if it was. */
  promotion: Promotion | undefined;
  onPromoted: (key: string) => void;
}

function TopicFileSection({ file, projectKey, storedMemories, promotion, onPromoted }: TopicFileSectionProps) {
  const [expanded, setExpanded] = useState(false);
  const [promoteModal, setPromoteModal] = useState<{ key: string; value: string } | null>(null);

  const suggestedKey = `claude-memory/${file.name.replace(/\.md$/, "")}`;
  const status: PromotionStatus = storedMemories
    ? promotionStatus(storedMemories, suggestedKey, file.content, promotion)
    : "none";

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
        <PromotionBadge status={status} />
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
          projectKey={projectKey}
          storedMemories={storedMemories}
          onClose={() => setPromoteModal(null)}
          onPromoted={onPromoted}
        />
      )}
    </div>
  );
}

// ── Main Component ────────────────────────────────────────────────────────────

/** The key MEMORY.md is suggested to be promoted under. */
const MEMORY_MD_KEY = "claude-memory/MEMORY";
const MEMORY_MD_FILE = "MEMORY.md";

interface ClaudeMemoryPanelProps {
  /** The project's local_key (used as the project for store_memory calls). */
  projectKey: string;
  /** Absolute path to the project directory (used to derive the Claude memory path). */
  projectDirectory: string;
  /**
   * Automatic's memories for this project, used to mark files already
   * promoted. Undefined while they are not known for this project: no file
   * is marked and no replace warning shows.
   */
  storedMemories?: StoredMemories;
  /** Called when a memory entry has been successfully promoted. */
  onPromoted: () => void;
}

export function ClaudeMemoryPanel({
  projectKey,
  projectDirectory,
  storedMemories,
  onPromoted,
}: ClaudeMemoryPanelProps) {
  const [content, setContent] = useState<ClaudeMemoryContent | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [memoryMdExpanded, setMemoryMdExpanded] = useState(true);
  const [promoteMemoryMd, setPromoteMemoryMd] = useState(false);
  // The key of the last successful promotion. The new entry is listed in the
  // Agent Memory list at the top of the tab, usually out of view, so the
  // panel confirms the save itself.
  const [lastPromotedKey, setLastPromotedKey] = useState<string | null>(null);
  // What each file (by name) was promoted as in this session.
  const [promotions, setPromotions] = useState<Record<string, Promotion>>({});

  const handlePromoted = useCallback(
    (fileName: string, content: string) => (key: string) => {
      setLastPromotedKey(key);
      setPromotions((prev) => ({ ...prev, [fileName]: { key, content } }));
      onPromoted();
    },
    [onPromoted],
  );

  const load = useCallback(async () => {
    if (!projectDirectory) return;
    try {
      setLoading(true);
      setError(null);
      const result = await invoke<ClaudeMemoryContent>("get_claude_memory", {
        project: projectKey,
      });
      setContent(result);
    } catch (err: unknown) {
      setError(`Failed to read Claude auto-memory: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setLoading(false);
    }
  }, [projectKey, projectDirectory]);

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

      {/* Promotion confirmation */}
      {lastPromotedKey && (
        <div
          role="status"
          className="flex items-start gap-2 text-[12px] text-text-base bg-success/10 border border-success/30 rounded p-3 mb-3"
        >
          <Check size={13} className="text-success shrink-0 mt-0.5" />
          <span className="flex-1">
            Promoted to Automatic memory as{" "}
            <span className="font-mono">{lastPromotedKey}</span>. It is listed under Agent Memory at the top of this tab.
          </span>
          <button
            onClick={() => setLastPromotedKey(null)}
            className="text-text-muted hover:text-text-base p-0.5 rounded transition-colors shrink-0"
            aria-label="Dismiss"
          >
            <X size={12} />
          </button>
        </div>
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
                <PromotionBadge
                  status={
                    storedMemories
                      ? promotionStatus(
                          storedMemories,
                          MEMORY_MD_KEY,
                          content.memory_md,
                          hasOwnKey(promotions, MEMORY_MD_FILE) ? promotions[MEMORY_MD_FILE] : undefined,
                        )
                      : "none"
                  }
                />
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
              projectKey={projectKey}
              storedMemories={storedMemories}
              promotion={hasOwnKey(promotions, file.name) ? promotions[file.name] : undefined}
              onPromoted={handlePromoted(file.name, file.content)}
            />
          ))}

          {/* Footer: link to docs */}
          <div className="flex items-center gap-1 pt-1">
            <a
              href="https://code.claude.com/docs/en/memory#auto-memory"
              target="_blank"
              rel="noopener noreferrer"
              onClick={handleExternalLinkClick("https://code.claude.com/docs/en/memory#auto-memory")}
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
          initialKey={MEMORY_MD_KEY}
          initialValue={content.memory_md}
          projectKey={projectKey}
          storedMemories={storedMemories}
          onClose={() => setPromoteMemoryMd(false)}
          onPromoted={handlePromoted(MEMORY_MD_FILE, content.memory_md)}
        />
      )}
    </section>
  );
}
