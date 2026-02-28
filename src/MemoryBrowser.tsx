import { useState, useMemo, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  Trash2,
  Edit2,
  Copy,
  Check,
  Search,
  X,
  ChevronDown,
  ChevronRight,
  SortAsc,
  SortDesc,
  Plus,
  Brain,
} from "lucide-react";

// ── Types ───────────────────────────────────────────────────────────────────

export interface MemoryEntry {
  value: string;
  timestamp: string;
  source: string | null;
}

export type MemoryRecord = Record<string, MemoryEntry>;

type SortField = "key" | "date";
type SortDir = "asc" | "desc";

const PAGE_SIZE = 50;

// ── Sub-components ───────────────────────────────────────────────────────────

interface MemoryRowProps {
  memoryKey: string;
  entry: MemoryEntry;
  isExpanded: boolean;
  isEditing: boolean;
  isCopied: boolean;
  editValue: string;
  saving: boolean;
  onToggle: () => void;
  onEdit: () => void;
  onCancelEdit: () => void;
  onSave: () => void;
  onDelete: () => void;
  onCopy: () => void;
  onEditValueChange: (v: string) => void;
}

function MemoryRow({
  memoryKey,
  entry,
  isExpanded,
  isEditing,
  isCopied,
  editValue,
  saving,
  onToggle,
  onEdit,
  onCancelEdit,
  onSave,
  onDelete,
  onCopy,
  onEditValueChange,
}: MemoryRowProps) {
  const preview = entry.value.replace(/\n+/g, " ").trim();

  return (
    <div className="border border-border-strong/40 rounded-lg overflow-hidden group">
      {/* Header row — always visible */}
      <div
        className="flex items-center gap-2 px-3 py-2.5 bg-bg-input cursor-pointer hover:bg-surface transition-colors select-none"
        onClick={onToggle}
      >
        <span className="text-text-muted shrink-0">
          {isExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        </span>

        {/* Key */}
        <span className="text-[12px] font-semibold text-text-base font-mono truncate flex-1 min-w-0">
          {memoryKey}
        </span>

        {/* Source badge */}
        {entry.source && (
          <span className="text-[10px] px-1.5 py-0.5 rounded bg-bg-sidebar text-text-muted border border-border-strong/40 shrink-0 hidden sm:inline">
            {entry.source}
          </span>
        )}

        {/* Date */}
        <span className="text-[11px] text-text-muted shrink-0 hidden md:inline">
          {new Date(entry.timestamp).toLocaleDateString(undefined, {
            month: "short",
            day: "numeric",
            year: "numeric",
          })}
        </span>

        {/* Preview — shown only when collapsed */}
        {!isExpanded && (
          <span className="text-[11px] text-text-muted truncate max-w-xs hidden lg:inline">
            {preview.slice(0, 80)}{preview.length > 80 ? "…" : ""}
          </span>
        )}

        {/* Action buttons — shown on hover */}
        <div
          className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity shrink-0"
          onClick={(e) => e.stopPropagation()}
        >
          <button
            onClick={onCopy}
            className="text-text-muted hover:text-text-base p-1.5 hover:bg-bg-base rounded transition-colors"
            title="Copy value"
          >
            {isCopied ? (
              <Check size={13} className="text-success" />
            ) : (
              <Copy size={13} />
            )}
          </button>
          <button
            onClick={isEditing ? onCancelEdit : onEdit}
            className={`p-1.5 rounded transition-colors ${
              isEditing
                ? "text-brand bg-brand/10 hover:bg-brand/20"
                : "text-text-muted hover:text-text-base hover:bg-bg-base"
            }`}
            title={isEditing ? "Cancel edit" : "Edit memory"}
          >
            <Edit2 size={13} />
          </button>
          <button
            onClick={onDelete}
            className="text-text-muted hover:text-danger p-1.5 hover:bg-bg-base rounded transition-colors"
            title="Delete memory"
          >
            <Trash2 size={13} />
          </button>
        </div>
      </div>

      {/* Expanded body */}
      {isExpanded && (
        <div className="px-3 py-3 bg-bg-base border-t border-border-strong/40">
          {isEditing ? (
            <div className="space-y-2">
              <textarea
                value={editValue}
                onChange={(e) => onEditValueChange(e.target.value)}
                className="w-full text-[12px] text-text-base font-mono bg-bg-input p-3 rounded border border-brand resize-none focus:outline-none focus:ring-1 focus:ring-brand custom-scrollbar"
                rows={Math.min(Math.max(editValue.split("\n").length + 1, 4), 20)}
                autoFocus
              />
              <div className="flex items-center justify-end gap-2">
                <button
                  onClick={onCancelEdit}
                  className="px-3 py-1 text-[12px] font-medium text-text-muted hover:text-text-base bg-bg-sidebar hover:bg-surface rounded transition-colors"
                >
                  Cancel
                </button>
                <button
                  disabled={saving || editValue.trim() === ""}
                  onClick={onSave}
                  className="px-3 py-1 text-[12px] font-medium bg-brand hover:bg-brand-hover disabled:opacity-50 disabled:cursor-not-allowed text-white rounded transition-colors"
                >
                  {saving ? "Saving…" : "Save"}
                </button>
              </div>
            </div>
          ) : (
            <pre className="text-[12px] text-text-base whitespace-pre-wrap font-mono leading-relaxed max-h-72 overflow-y-auto custom-scrollbar">
              {entry.value}
            </pre>
          )}
        </div>
      )}
    </div>
  );
}

// ── Add Memory Modal ─────────────────────────────────────────────────────────

interface AddMemoryModalProps {
  onClose: () => void;
  onSave: (key: string, value: string) => Promise<void>;
  saving: boolean;
}

function AddMemoryModal({ onClose, onSave, saving }: AddMemoryModalProps) {
  const [key, setKey] = useState("");
  const [value, setValue] = useState("");

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-bg-input border border-border-strong/40 rounded-xl shadow-xl w-full max-w-lg mx-4 p-5">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-[14px] font-semibold text-text-base">Add Memory</h3>
          <button onClick={onClose} className="text-text-muted hover:text-text-base p-1 rounded transition-colors">
            <X size={16} />
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
              placeholder="Describe the memory…"
              rows={6}
              className="w-full text-[13px] text-text-base font-mono bg-bg-base border border-border-strong/40 rounded px-3 py-2 focus:outline-none focus:border-brand resize-none custom-scrollbar"
            />
          </div>
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
            onClick={() => onSave(key.trim(), value.trim())}
            className="px-3 py-1.5 text-[12px] font-medium bg-brand hover:bg-brand-hover disabled:opacity-50 disabled:cursor-not-allowed text-white rounded transition-colors"
          >
            {saving ? "Saving…" : "Add Memory"}
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Main Component ────────────────────────────────────────────────────────────

interface MemoryBrowserProps {
  projectName: string;
  memories: MemoryRecord;
  loading: boolean;
  onRefresh: () => Promise<void>;
  onError: (msg: string) => void;
}

export function MemoryBrowser({
  projectName,
  memories,
  loading,
  onRefresh,
  onError,
}: MemoryBrowserProps) {
  // ── Local state ──────────────────────────────────────────────────────────
  const [query, setQuery] = useState("");
  const [sortField, setSortField] = useState<SortField>("date");
  const [sortDir, setSortDir] = useState<SortDir>("desc");
  const [page, setPage] = useState(0);
  const [expandedKeys, setExpandedKeys] = useState<Set<string>>(new Set());
  const [editingKey, setEditingKey] = useState<string | null>(null);
  const [editingValue, setEditingValue] = useState("");
  const [savingMemory, setSavingMemory] = useState(false);
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const [showAddModal, setShowAddModal] = useState(false);

  // ── Derived data ─────────────────────────────────────────────────────────

  const filtered = useMemo(() => {
    const q = query.toLowerCase().trim();
    const entries = Object.entries(memories);
    const matched = q
      ? entries.filter(
          ([k, v]) =>
            k.toLowerCase().includes(q) ||
            v.value.toLowerCase().includes(q) ||
            (v.source ?? "").toLowerCase().includes(q)
        )
      : entries;

    matched.sort(([ka, va], [kb, vb]) => {
      let cmp = 0;
      if (sortField === "key") {
        cmp = ka.localeCompare(kb);
      } else {
        cmp = new Date(va.timestamp).getTime() - new Date(vb.timestamp).getTime();
      }
      return sortDir === "asc" ? cmp : -cmp;
    });

    return matched;
  }, [memories, query, sortField, sortDir]);

  const totalPages = Math.ceil(filtered.length / PAGE_SIZE);
  const paginated = useMemo(
    () => filtered.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE),
    [filtered, page]
  );

  // Reset page when query/sort changes
  const safeSetQuery = useCallback((v: string) => {
    setQuery(v);
    setPage(0);
  }, []);

  // ── Sort toggle ──────────────────────────────────────────────────────────

  const toggleSort = (field: SortField) => {
    if (sortField === field) {
      setSortDir((d) => (d === "asc" ? "desc" : "asc"));
    } else {
      setSortField(field);
      setSortDir(field === "date" ? "desc" : "asc");
    }
    setPage(0);
  };

  // ── Row interaction handlers ──────────────────────────────────────────────

  const toggleExpand = (key: string) => {
    setExpandedKeys((prev) => {
      const next = new Set(prev);
      if (next.has(key)) {
        next.delete(key);
        if (editingKey === key) {
          setEditingKey(null);
          setEditingValue("");
        }
      } else {
        next.add(key);
      }
      return next;
    });
  };

  const beginEdit = (key: string) => {
    setEditingKey(key);
    setEditingValue(memories[key].value);
    setExpandedKeys((prev) => new Set(prev).add(key));
  };

  const cancelEdit = () => {
    setEditingKey(null);
    setEditingValue("");
  };

  const saveEdit = async () => {
    if (!editingKey) return;
    try {
      setSavingMemory(true);
      await invoke("store_memory", {
        project: projectName,
        key: editingKey,
        value: editingValue,
        source: memories[editingKey]?.source ?? null,
      });
      setEditingKey(null);
      setEditingValue("");
      await onRefresh();
    } catch (err: any) {
      onError(`Failed to save memory: ${err}`);
    } finally {
      setSavingMemory(false);
    }
  };

  const deleteMemory = async (key: string) => {
    try {
      await invoke("delete_memory", { project: projectName, key });
      if (editingKey === key) cancelEdit();
      setExpandedKeys((prev) => {
        const next = new Set(prev);
        next.delete(key);
        return next;
      });
      await onRefresh();
    } catch (err: any) {
      onError(`Failed to delete memory: ${err}`);
    }
  };

  const copyMemory = (key: string) => {
    navigator.clipboard.writeText(memories[key].value);
    setCopiedKey(key);
    setTimeout(() => setCopiedKey(null), 1500);
  };

  const clearAll = async () => {
    if (
      !(await ask(
        "Are you sure you want to clear all memory for this project? This cannot be undone.",
        { title: "Clear Memories", kind: "warning" }
      ))
    )
      return;
    try {
      await invoke("clear_memories", {
        project: projectName,
        confirm: true,
        pattern: null,
      });
      setExpandedKeys(new Set());
      setEditingKey(null);
      await onRefresh();
    } catch (err: any) {
      onError(`Failed to clear memories: ${err}`);
    }
  };

  const addMemory = async (key: string, value: string) => {
    try {
      setSavingMemory(true);
      await invoke("store_memory", {
        project: projectName,
        key,
        value,
        source: null,
      });
      setShowAddModal(false);
      await onRefresh();
    } catch (err: any) {
      onError(`Failed to add memory: ${err}`);
    } finally {
      setSavingMemory(false);
    }
  };

  // ── Render ───────────────────────────────────────────────────────────────

  const totalCount = Object.keys(memories).length;
  const SortIcon = sortDir === "asc" ? SortAsc : SortDesc;

  return (
    <section>
      {/* Header */}
      <div className="flex items-start justify-between mb-4">
        <div>
          <h3 className="text-[14px] font-medium text-text-base">Agent Memory</h3>
          <p className="text-[12px] text-text-muted mt-0.5">
            Persistent context and learnings stored by agents.
            {totalCount > 0 && (
              <span className="ml-1 text-text-muted/60">
                {totalCount} {totalCount === 1 ? "entry" : "entries"}
              </span>
            )}
          </p>
        </div>
        <div className="flex items-center gap-2 shrink-0">
          <button
            onClick={() => setShowAddModal(true)}
            className="flex items-center gap-1.5 px-3 py-1.5 bg-brand hover:bg-brand-hover text-white rounded text-[12px] font-medium transition-colors"
          >
            <Plus size={12} /> Add
          </button>
          {totalCount > 0 && (
            <button
              onClick={clearAll}
              className="flex items-center gap-1.5 px-3 py-1.5 bg-danger/10 hover:bg-danger/20 text-danger rounded text-[12px] font-medium border border-danger/20 transition-colors"
            >
              <Trash2 size={12} /> Clear All
            </button>
          )}
        </div>
      </div>

      {/* Loading state */}
      {loading ? (
        <div className="text-[13px] text-text-muted text-center py-8">Loading memories…</div>
      ) : totalCount === 0 ? (
        /* Empty state */
        <div className="text-center py-12 bg-bg-input rounded-lg border border-border-strong/40 border-dashed">
          <div className="w-12 h-12 mx-auto mb-3 rounded-full bg-bg-sidebar flex items-center justify-center">
            <Brain size={20} className="text-text-muted" />
          </div>
          <h4 className="text-[13px] font-medium text-text-base mb-1">No memories yet</h4>
          <p className="text-[12px] text-text-muted max-w-sm mx-auto mb-4">
            Agents haven't stored any learnings or context for this project yet.
          </p>
          <button
            onClick={() => setShowAddModal(true)}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 bg-brand hover:bg-brand-hover text-white rounded text-[12px] font-medium transition-colors"
          >
            <Plus size={12} /> Add manually
          </button>
        </div>
      ) : (
        <>
          {/* Toolbar: search + sort */}
          <div className="flex items-center gap-2 mb-3">
            <div className="relative flex-1">
              <Search
                size={13}
                className="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none"
              />
              <input
                type="text"
                value={query}
                onChange={(e) => safeSetQuery(e.target.value)}
                placeholder="Search keys, values, sources…"
                className="w-full pl-8 pr-8 py-1.5 text-[12px] bg-bg-input border border-border-strong/40 rounded focus:outline-none focus:border-brand text-text-base"
              />
              {query && (
                <button
                  onClick={() => safeSetQuery("")}
                  className="absolute right-2 top-1/2 -translate-y-1/2 text-text-muted hover:text-text-base transition-colors"
                >
                  <X size={13} />
                </button>
              )}
            </div>

            {/* Sort buttons */}
            <button
              onClick={() => toggleSort("key")}
              className={`flex items-center gap-1 px-2.5 py-1.5 rounded text-[11px] font-medium border transition-colors ${
                sortField === "key"
                  ? "bg-brand/10 border-brand/30 text-brand"
                  : "bg-bg-input border-border-strong/40 text-text-muted hover:text-text-base"
              }`}
              title="Sort by key"
            >
              {sortField === "key" ? <SortIcon size={12} /> : <SortAsc size={12} />}
              Key
            </button>
            <button
              onClick={() => toggleSort("date")}
              className={`flex items-center gap-1 px-2.5 py-1.5 rounded text-[11px] font-medium border transition-colors ${
                sortField === "date"
                  ? "bg-brand/10 border-brand/30 text-brand"
                  : "bg-bg-input border-border-strong/40 text-text-muted hover:text-text-base"
              }`}
              title="Sort by date"
            >
              {sortField === "date" ? <SortIcon size={12} /> : <SortDesc size={12} />}
              Date
            </button>
          </div>

          {/* Result count */}
          {query && (
            <p className="text-[11px] text-text-muted mb-2">
              {filtered.length} {filtered.length === 1 ? "result" : "results"} for &ldquo;{query}&rdquo;
            </p>
          )}

          {/* Records list */}
          {filtered.length === 0 ? (
            <div className="text-center py-8 text-[13px] text-text-muted">
              No memories match your search.
            </div>
          ) : (
            <div className="space-y-1">
              {paginated.map(([key, entry]) => (
                <MemoryRow
                  key={key}
                  memoryKey={key}
                  entry={entry}
                  isExpanded={expandedKeys.has(key)}
                  isEditing={editingKey === key}
                  isCopied={copiedKey === key}
                  editValue={editingKey === key ? editingValue : ""}
                  saving={savingMemory}
                  onToggle={() => toggleExpand(key)}
                  onEdit={() => beginEdit(key)}
                  onCancelEdit={cancelEdit}
                  onSave={saveEdit}
                  onDelete={() => deleteMemory(key)}
                  onCopy={() => copyMemory(key)}
                  onEditValueChange={setEditingValue}
                />
              ))}
            </div>
          )}

          {/* Pagination */}
          {totalPages > 1 && (
            <div className="flex items-center justify-between mt-4 pt-3 border-t border-border-strong/40">
              <p className="text-[11px] text-text-muted">
                Showing {page * PAGE_SIZE + 1}–{Math.min((page + 1) * PAGE_SIZE, filtered.length)} of {filtered.length}
              </p>
              <div className="flex items-center gap-1">
                <button
                  disabled={page === 0}
                  onClick={() => setPage((p) => p - 1)}
                  className="px-2.5 py-1 text-[11px] font-medium border border-border-strong/40 rounded text-text-muted hover:text-text-base hover:bg-surface disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
                >
                  Previous
                </button>
                {/* Page numbers — show a window of 5 */}
                {Array.from({ length: totalPages }, (_, i) => i)
                  .filter(
                    (i) =>
                      i === 0 ||
                      i === totalPages - 1 ||
                      Math.abs(i - page) <= 2
                  )
                  .reduce<(number | "...")[]>((acc, i, idx, arr) => {
                    if (idx > 0 && i - (arr[idx - 1] as number) > 1) acc.push("...");
                    acc.push(i);
                    return acc;
                  }, [])
                  .map((item, idx) =>
                    item === "..." ? (
                      <span key={`ellipsis-${idx}`} className="px-1 text-[11px] text-text-muted">
                        …
                      </span>
                    ) : (
                      <button
                        key={item}
                        onClick={() => setPage(item as number)}
                        className={`px-2.5 py-1 text-[11px] font-medium border rounded transition-colors ${
                          page === item
                            ? "bg-brand border-brand text-white"
                            : "border-border-strong/40 text-text-muted hover:text-text-base hover:bg-surface"
                        }`}
                      >
                        {(item as number) + 1}
                      </button>
                    )
                  )}
                <button
                  disabled={page >= totalPages - 1}
                  onClick={() => setPage((p) => p + 1)}
                  className="px-2.5 py-1 text-[11px] font-medium border border-border-strong/40 rounded text-text-muted hover:text-text-base hover:bg-surface disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
                >
                  Next
                </button>
              </div>
            </div>
          )}
        </>
      )}

      {/* Add Memory Modal */}
      {showAddModal && (
        <AddMemoryModal
          onClose={() => setShowAddModal(false)}
          onSave={addMemory}
          saving={savingMemory}
        />
      )}
    </section>
  );
}
