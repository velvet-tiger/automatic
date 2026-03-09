import { useState, useEffect, useRef, useCallback, useMemo } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { MarkdownPreview } from "./MarkdownPreview";
import {
  Plus,
  X,
  Kanban,
  List,
  MessageSquare,
  Send,
  Trash2,
  Check,
  Tag,
  Link,
  AlertCircle,
  ChevronDown,
} from "lucide-react";

// ── Types ─────────────────────────────────────────────────────────────────────

export interface Feature {
  id: string;
  project: string;
  title: string;
  description: string;
  state: string;
  priority: string;
  assignee: string | null;
  tags: string[];
  linked_files: string[];
  effort: string | null;
  created_at: string;
  updated_at: string;
  created_by: string | null;
  position: number;
}

export interface FeatureUpdate {
  id: number;
  feature_id: string;
  project: string;
  content: string;
  author: string | null;
  timestamp: string;
}

export interface FeatureWithUpdates extends Feature {
  updates: FeatureUpdate[];
}

export interface FeaturePatch {
  title?: string;
  description?: string;
  state?: string;
  priority?: string;
  assignee?: string | null;
  tags?: string[];
  linked_files?: string[];
  effort?: string | null;
}

type ListSortKey = "title" | "state" | "priority" | "effort" | "assignee" | "updated";
type SortDirection = "asc" | "desc";

interface ListSort {
  key: ListSortKey;
  direction: SortDirection;
}

// ── Constants ────────────────────────────────────────────────────────────────

const STATES = [
  { id: "backlog", label: "Backlog" },
  { id: "todo", label: "To Do" },
  { id: "in_progress", label: "In Progress" },
  { id: "review", label: "Review" },
  { id: "complete", label: "Complete" },
  { id: "cancelled", label: "Cancelled" },
] as const;

const PRIORITIES = ["low", "medium", "high"] as const;
const EFFORTS = ["xs", "s", "m", "l", "xl"] as const;

const STATE_ORDER = new Map(STATES.map((state, index) => [state.id, index]));
const PRIORITY_ORDER = new Map(PRIORITIES.map((priority, index) => [priority, index]));
const EFFORT_ORDER = new Map(EFFORTS.map((effort, index) => [effort, index]));

const STATE_STYLES: Record<string, string> = {
  backlog: "bg-bg-sidebar text-text-muted",
  todo: "bg-brand/20 text-brand",
  in_progress: "bg-amber-500/20 text-amber-400",
  review: "bg-blue-500/20 text-blue-400",
  complete: "bg-green-500/20 text-success",
  cancelled: "bg-text-muted/10 text-text-muted line-through",
};

const PRIORITY_DOT: Record<string, string> = {
  low: "bg-text-muted",
  medium: "bg-amber-400",
  high: "bg-danger",
};

const DETAIL_MIN = 300;
const DETAIL_MAX = 700;
const DETAIL_DEFAULT = 420;

// ── Helpers ───────────────────────────────────────────────────────────────────

function formatDate(iso: string): string {
  try {
    return new Date(iso).toLocaleDateString(undefined, {
      month: "short",
      day: "numeric",
      year: "numeric",
    });
  } catch {
    return iso;
  }
}

function formatDateTime(iso: string): string {
  try {
    return new Date(iso).toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return iso;
  }
}

function compareText(a: string, b: string): number {
  return a.localeCompare(b, undefined, { numeric: true, sensitivity: "base" });
}

function compareNullableText(a: string | null, b: string | null): number {
  if (!a && !b) return 0;
  if (!a) return 1;
  if (!b) return -1;
  return compareText(a, b);
}

function compareNullableRank(
  a: string | null,
  b: string | null,
  order: ReadonlyMap<string, number>
): number {
  if (!a && !b) return 0;
  if (!a) return 1;
  if (!b) return -1;
  return (order.get(a) ?? Number.MAX_SAFE_INTEGER) - (order.get(b) ?? Number.MAX_SAFE_INTEGER);
}

function compareFeatures(a: Feature, b: Feature, key: ListSortKey): number {
  switch (key) {
    case "title":
      return compareText(a.title, b.title);
    case "state":
      return compareNullableRank(a.state, b.state, STATE_ORDER);
    case "priority":
      return compareNullableRank(a.priority, b.priority, PRIORITY_ORDER);
    case "effort":
      return compareNullableRank(a.effort, b.effort, EFFORT_ORDER);
    case "assignee":
      return compareNullableText(a.assignee, b.assignee);
    case "updated":
      return new Date(a.updated_at).getTime() - new Date(b.updated_at).getTime();
  }
}

// ── Portal Select ─────────────────────────────────────────────────────────────
// Renders the dropdown list via a portal mounted at document.body so it floats
// above all stacking contexts and window chrome in the Tauri WebView.

interface SelectOption {
  value: string;
  label: string;
}

interface SelectProps {
  value: string;
  onChange: (value: string) => void;
  options: SelectOption[];
  className?: string;
  size?: "sm" | "xs";
}

function Select({ value, onChange, options, className = "", size = "sm" }: SelectProps) {
  const [open, setOpen] = useState(false);
  const [menuStyle, setMenuStyle] = useState<React.CSSProperties>({});
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  const selectedLabel = options.find((o) => o.value === value)?.label ?? value;

  const openMenu = () => {
    if (!triggerRef.current) return;
    const rect = triggerRef.current.getBoundingClientRect();
    const viewportH = window.innerHeight;
    const spaceBelow = viewportH - rect.bottom;
    const estimatedH = options.length * 28 + 8;
    const showAbove = spaceBelow < estimatedH && rect.top > estimatedH;

    setMenuStyle({
      position: "fixed",
      left: rect.left,
      width: Math.max(rect.width, 120),
      zIndex: 99999,
      ...(showAbove
        ? { bottom: viewportH - rect.top }
        : { top: rect.bottom + 2 }),
    });
    setOpen(true);
  };

  // Close on outside click or Escape
  useEffect(() => {
    if (!open) return;
    const handleClick = (e: MouseEvent) => {
      if (
        menuRef.current && !menuRef.current.contains(e.target as Node) &&
        triggerRef.current && !triggerRef.current.contains(e.target as Node)
      ) {
        setOpen(false);
      }
    };
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", handleClick);
    document.addEventListener("keydown", handleKey);
    return () => {
      document.removeEventListener("mousedown", handleClick);
      document.removeEventListener("keydown", handleKey);
    };
  }, [open]);

  const textSize = size === "xs" ? "text-[11px]" : "text-[12px]";
  const padding = size === "xs" ? "px-2 py-0.5" : "px-2 py-1";

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        onClick={open ? () => setOpen(false) : openMenu}
        className={`flex items-center gap-1 bg-bg-input border border-border-strong/40 rounded ${padding} ${textSize} text-text-base hover:border-brand/50 transition-colors whitespace-nowrap ${className}`}
      >
        <span className="flex-1 text-left">{selectedLabel}</span>
        <ChevronDown size={10} className={`text-text-muted transition-transform shrink-0 ${open ? "rotate-180" : ""}`} />
      </button>

      {open && createPortal(
        <div
          ref={menuRef}
          style={menuStyle}
          className="bg-bg-input border border-border-strong/60 rounded-md shadow-xl overflow-hidden py-1"
        >
          {options.map((opt) => (
            <button
              key={opt.value}
              type="button"
              onClick={() => { onChange(opt.value); setOpen(false); }}
              className={`w-full text-left px-3 py-1.5 text-[12px] transition-colors ${
                opt.value === value
                  ? "bg-brand/20 text-brand"
                  : "text-text-base hover:bg-bg-sidebar"
              }`}
            >
              {opt.label}
            </button>
          ))}
        </div>,
        document.body
      )}
    </>
  );
}

// ── Sub-components ────────────────────────────────────────────────────────────

function StateBadge({ state }: { state: string }) {
  const label = STATES.find((s) => s.id === state)?.label ?? state;
  return (
    <span
      className={`inline-flex items-center px-1.5 py-0.5 rounded text-[11px] font-medium ${
        STATE_STYLES[state] ?? "bg-bg-sidebar text-text-muted"
      }`}
    >
      {label}
    </span>
  );
}

function PriorityDot({ priority }: { priority: string }) {
  return (
    <span
      className={`inline-block w-2 h-2 rounded-full shrink-0 ${
        PRIORITY_DOT[priority] ?? "bg-text-muted"
      }`}
      title={`Priority: ${priority}`}
    />
  );
}

// ── Feature Detail Panel ──────────────────────────────────────────────────────

interface DetailPanelProps {
  projectName: string;
  featureId: string;
  onClose: () => void;
  onUpdated: (f: Feature) => void;
  onDeleted: (id: string) => void;
}

function DetailPanel({
  projectName,
  featureId,
  onClose,
  onUpdated,
  onDeleted,
}: DetailPanelProps) {
  const [feature, setFeature] = useState<FeatureWithUpdates | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Edit state
  const [editTitle, setEditTitle] = useState("");
  const [editDescription, setEditDescription] = useState("");
  const [editState, setEditState] = useState("");
  const [editPriority, setEditPriority] = useState("");
  const [editAssignee, setEditAssignee] = useState("");
  const [editEffort, setEditEffort] = useState("");
  const [editTagsRaw, setEditTagsRaw] = useState("");
  const [editLinkedFilesRaw, setEditLinkedFilesRaw] = useState("");
  const [saving, setSaving] = useState(false);
  const [descPreview, setDescPreview] = useState(false);

  // Update input
  const [updateContent, setUpdateContent] = useState("");
  const [submittingUpdate, setSubmittingUpdate] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const fw = await invoke<FeatureWithUpdates>("get_feature_with_updates", {
        project: projectName,
        featureId,
      });
      setFeature(fw);
      setEditTitle(fw.title);
      setEditDescription(fw.description);
      setEditState(fw.state);
      setEditPriority(fw.priority);
      setEditAssignee(fw.assignee ?? "");
      setEditEffort(fw.effort ?? "");
      setEditTagsRaw(fw.tags.join(", "));
      setEditLinkedFilesRaw(fw.linked_files.join("\n"));
    } catch (err: any) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [projectName, featureId]);

  useEffect(() => {
    load();
  }, [load]);

  const handleSave = async () => {
    if (!feature) return;
    setSaving(true);
    setError(null);
    try {
      const tags = editTagsRaw
        .split(",")
        .map((t) => t.trim())
        .filter(Boolean);
      const linked_files = editLinkedFilesRaw
        .split("\n")
        .map((f) => f.trim())
        .filter(Boolean);
      const patch: FeaturePatch = {
        title: editTitle,
        description: editDescription,
        state: editState,
        priority: editPriority,
        assignee: editAssignee.trim() || null,
        effort: editEffort || null,
        tags,
        linked_files,
      };
      const updated = await invoke<Feature>("update_feature", {
        project: projectName,
        featureId,
        patch,
      });
      setFeature((prev) =>
        prev ? { ...prev, ...updated } : null
      );
      onUpdated(updated);
    } catch (err: any) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  };

  const handleAddUpdate = async () => {
    if (!updateContent.trim()) return;
    setSubmittingUpdate(true);
    setError(null);
    try {
      const update = await invoke<FeatureUpdate>("add_feature_update", {
        project: projectName,
        featureId,
        content: updateContent.trim(),
        author: "user",
      });
      setFeature((prev) =>
        prev ? { ...prev, updates: [update, ...prev.updates] } : null
      );
      setUpdateContent("");
    } catch (err: any) {
      setError(String(err));
    } finally {
      setSubmittingUpdate(false);
    }
  };

  const handleDelete = async () => {
    if (!feature) return;
    const confirmed = await ask(
      `Delete feature "${feature.title}"? This cannot be undone.`,
      { title: "Delete Feature", kind: "warning" }
    );
    if (!confirmed) return;
    try {
      await invoke("delete_feature", { project: projectName, featureId });
      onDeleted(featureId);
    } catch (err: any) {
      setError(String(err));
    }
  };

  if (loading) {
    return (
      <div className="flex-1 flex items-center justify-center text-text-muted text-[13px]">
        Loading…
      </div>
    );
  }

  if (!feature) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center gap-2 text-text-muted text-[13px]">
        <AlertCircle size={22} />
        <span>{error ?? "Feature not found"}</span>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full overflow-hidden">
      {/* Header */}
      <div className="flex items-center gap-2 px-4 py-3 border-b border-border-strong/40 shrink-0">
        <input
          value={editTitle}
          onChange={(e) => setEditTitle(e.target.value)}
          className="flex-1 bg-transparent text-[14px] font-semibold text-text-base outline-none border-b border-transparent focus:border-border-strong/60 transition-colors"
          placeholder="Feature title"
        />
        <button
          onClick={onClose}
          className="text-text-muted hover:text-text-base transition-colors"
        >
          <X size={14} />
        </button>
      </div>

      {/* Error banner */}
      {error && (
        <div className="mx-4 mt-3 px-3 py-2 bg-danger/10 border border-danger/30 rounded text-[12px] text-danger flex items-center gap-2 shrink-0">
          <AlertCircle size={12} />
          <span className="flex-1">{error}</span>
          <button onClick={() => setError(null)}>
            <X size={11} />
          </button>
        </div>
      )}

      {/* Scrollable body */}
      <div className="flex-1 overflow-y-auto custom-scrollbar px-4 py-3 space-y-4">
        {/* State + priority row */}
        <div className="flex items-center gap-3 flex-wrap">
          <div className="flex items-center gap-1.5">
            <span className="text-[11px] text-text-muted">State</span>
            <Select
              value={editState}
              onChange={setEditState}
              options={STATES.map((s) => ({ value: s.id, label: s.label }))}
            />
          </div>
          <div className="flex items-center gap-1.5">
            <span className="text-[11px] text-text-muted">Priority</span>
            <Select
              value={editPriority}
              onChange={setEditPriority}
              options={PRIORITIES.map((p) => ({ value: p, label: p.charAt(0).toUpperCase() + p.slice(1) }))}
            />
          </div>
          <div className="flex items-center gap-1.5">
            <span className="text-[11px] text-text-muted">Effort</span>
            <Select
              value={editEffort}
              onChange={setEditEffort}
              options={[
                { value: "", label: "—" },
                ...EFFORTS.map((e) => ({ value: e, label: e.toUpperCase() })),
              ]}
            />
          </div>
        </div>

        {/* Assignee */}
        <div>
          <label className="block text-[11px] text-text-muted mb-1">Assignee</label>
          <input
            value={editAssignee}
            onChange={(e) => setEditAssignee(e.target.value)}
            placeholder="Agent id or name"
            className="w-full bg-bg-input border border-border-strong/40 rounded px-2.5 py-1.5 text-[13px] text-text-base outline-none focus:border-brand/50 transition-colors"
          />
        </div>

        {/* Tags */}
        <div>
          <label className="block text-[11px] text-text-muted mb-1 flex items-center gap-1">
            <Tag size={11} /> Tags (comma-separated)
          </label>
          <input
            value={editTagsRaw}
            onChange={(e) => setEditTagsRaw(e.target.value)}
            placeholder="e.g. frontend, auth, bug"
            className="w-full bg-bg-input border border-border-strong/40 rounded px-2.5 py-1.5 text-[13px] text-text-base outline-none focus:border-brand/50 transition-colors"
          />
        </div>

        {/* Linked files */}
        <div>
          <label className="block text-[11px] text-text-muted mb-1 flex items-center gap-1">
            <Link size={11} /> Linked files (one per line)
          </label>
          <textarea
            value={editLinkedFilesRaw}
            onChange={(e) => setEditLinkedFilesRaw(e.target.value)}
            rows={2}
            placeholder="src/components/Foo.tsx"
            className="w-full bg-bg-input border border-border-strong/40 rounded px-2.5 py-1.5 text-[13px] text-text-base outline-none focus:border-brand/50 transition-colors resize-none font-mono"
          />
        </div>

        {/* Description */}
        <div>
          <div className="flex items-center justify-between mb-1">
            <label className="text-[11px] text-text-muted">Description</label>
            <button
              onClick={() => setDescPreview((p) => !p)}
              className="text-[11px] text-text-muted hover:text-text-base transition-colors"
            >
              {descPreview ? "Edit" : "Preview"}
            </button>
          </div>
          {descPreview ? (
            <div className="bg-bg-input border border-border-strong/40 rounded px-3 py-2 min-h-[80px] text-[13px]">
              <MarkdownPreview content={editDescription || "*No description*"} />
            </div>
          ) : (
            <textarea
              value={editDescription}
              onChange={(e) => setEditDescription(e.target.value)}
              rows={5}
              placeholder="Describe the feature in markdown…"
              className="w-full bg-bg-input border border-border-strong/40 rounded px-2.5 py-1.5 text-[13px] text-text-base outline-none focus:border-brand/50 transition-colors resize-none font-mono"
            />
          )}
        </div>

        {/* Save button */}
        <div className="flex items-center justify-between">
          <button
            onClick={handleSave}
            disabled={saving || !editTitle.trim()}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded bg-brand hover:bg-brand-hover disabled:opacity-50 text-white text-[12px] font-medium transition-colors"
          >
            <Check size={12} />
            {saving ? "Saving…" : "Save changes"}
          </button>
          <button
            onClick={handleDelete}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded text-danger hover:bg-danger/10 text-[12px] font-medium transition-colors"
          >
            <Trash2 size={12} />
            Delete
          </button>
        </div>

        {/* Divider */}
        <div className="border-t border-border-strong/40" />

        {/* Updates */}
        <div>
          <h3 className="text-[12px] font-semibold text-text-muted uppercase tracking-wider mb-2 flex items-center gap-1.5">
            <MessageSquare size={12} />
            Updates ({feature.updates.length})
          </h3>

          {/* Add update */}
          <div className="flex gap-2 mb-3">
            <textarea
              value={updateContent}
              onChange={(e) => setUpdateContent(e.target.value)}
              rows={2}
              placeholder="Log a progress update, decision, or blocker…"
              className="flex-1 bg-bg-input border border-border-strong/40 rounded px-2.5 py-1.5 text-[13px] text-text-base outline-none focus:border-brand/50 transition-colors resize-none"
              onKeyDown={(e) => {
                if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                  e.preventDefault();
                  handleAddUpdate();
                }
              }}
            />
            <button
              onClick={handleAddUpdate}
              disabled={submittingUpdate || !updateContent.trim()}
              className="self-end flex items-center justify-center w-8 h-8 rounded bg-brand hover:bg-brand-hover disabled:opacity-50 text-white transition-colors shrink-0"
              title="Add update (⌘↵)"
            >
              <Send size={13} />
            </button>
          </div>

          {/* Update timeline */}
          {feature.updates.length === 0 ? (
            <p className="text-[12px] text-text-muted">No updates yet.</p>
          ) : (
            <div className="space-y-3">
              {feature.updates.map((u) => (
                <div
                  key={u.id}
                  className="border border-border-strong/40 rounded-lg overflow-hidden"
                >
                  <div className="flex items-center gap-2 px-3 py-1.5 bg-bg-input text-[11px] text-text-muted">
                    <span className="font-medium text-text-base">
                      {u.author ?? "unknown"}
                    </span>
                    <span>·</span>
                    <span>{formatDateTime(u.timestamp)}</span>
                  </div>
                  <div className="px-3 py-2 text-[13px]">
                    <MarkdownPreview content={u.content} />
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Metadata footer */}
        <div className="border-t border-border-strong/40 pt-3 text-[11px] text-text-muted space-y-0.5">
          <div>Created {formatDate(feature.created_at)}{feature.created_by ? ` by ${feature.created_by}` : ""}</div>
          <div>Updated {formatDate(feature.updated_at)}</div>
          <div className="font-mono opacity-60">{feature.id}</div>
        </div>
      </div>
    </div>
  );
}

// ── Create Feature Panel ───────────────────────────────────────────────────────

interface CreateFeaturePanelProps {
  projectName: string;
  onCreated: (f: Feature) => void;
  onCancel: () => void;
}

function CreateFeaturePanel({ projectName, onCreated, onCancel }: CreateFeaturePanelProps) {
  const [editTitle, setEditTitle] = useState("");
  const [editDescription, setEditDescription] = useState("");
  const [editState, setEditState] = useState("backlog");
  const [editPriority, setEditPriority] = useState("medium");
  const [editAssignee, setEditAssignee] = useState("");
  const [editEffort, setEditEffort] = useState("");
  const [editTagsRaw, setEditTagsRaw] = useState("");
  const [editLinkedFilesRaw, setEditLinkedFilesRaw] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [descPreview, setDescPreview] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const handleCreate = async () => {
    if (!editTitle.trim()) return;
    setSaving(true);
    setError(null);
    try {
      const tags = editTagsRaw.split(",").map((tag) => tag.trim()).filter(Boolean);
      const linked_files = editLinkedFilesRaw.split("\n").map((file) => file.trim()).filter(Boolean);
      const feature = await invoke<Feature>("create_feature", {
        project: projectName,
        title: editTitle.trim(),
        description: editDescription,
        state: editState,
        priority: editPriority,
        assignee: editAssignee.trim() || null,
        effort: editEffort || null,
        tags,
        linked_files,
      });
      onCreated(feature);
    } catch (err: any) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="flex flex-col h-full overflow-hidden">
      <div className="flex items-center gap-2 px-4 py-3 border-b border-border-strong/40 shrink-0">
        <input
          ref={inputRef}
          value={editTitle}
          onChange={(e) => setEditTitle(e.target.value)}
          className="flex-1 bg-transparent text-[14px] font-semibold text-text-base outline-none border-b border-transparent focus:border-border-strong/60 transition-colors"
          placeholder="Feature title"
        />
        <button onClick={onCancel} className="text-text-muted hover:text-text-base transition-colors">
          <X size={14} />
        </button>
      </div>

        {error && (
          <div className="mx-4 mt-3 px-3 py-2 bg-danger/10 border border-danger/30 rounded text-[12px] text-danger flex items-center gap-2 shrink-0">
            <AlertCircle size={12} />
            <span className="flex-1">{error}</span>
            <button onClick={() => setError(null)}>
              <X size={11} />
            </button>
          </div>
        )}

      <div className="flex-1 overflow-y-auto custom-scrollbar px-4 py-3 space-y-4">
        <div className="flex items-center gap-3 flex-wrap">
          <div className="flex items-center gap-1.5">
            <span className="text-[11px] text-text-muted">State</span>
            <Select
              value={editState}
              onChange={setEditState}
              options={STATES.map((s) => ({ value: s.id, label: s.label }))}
            />
          </div>
          <div className="flex items-center gap-1.5">
            <span className="text-[11px] text-text-muted">Priority</span>
            <Select
              value={editPriority}
              onChange={setEditPriority}
              options={PRIORITIES.map((p) => ({ value: p, label: p.charAt(0).toUpperCase() + p.slice(1) }))}
            />
          </div>
          <div className="flex items-center gap-1.5">
            <span className="text-[11px] text-text-muted">Effort</span>
            <Select
              value={editEffort}
              onChange={setEditEffort}
              options={[
                { value: "", label: "—" },
                ...EFFORTS.map((e) => ({ value: e, label: e.toUpperCase() })),
              ]}
            />
          </div>
        </div>

        <div>
          <label className="block text-[11px] text-text-muted mb-1">Assignee</label>
          <input
            value={editAssignee}
            onChange={(e) => setEditAssignee(e.target.value)}
            placeholder="Agent id or name"
            className="w-full bg-bg-input border border-border-strong/40 rounded px-2.5 py-1.5 text-[13px] text-text-base outline-none focus:border-brand/50 transition-colors"
          />
        </div>

        <div>
          <label className="block text-[11px] text-text-muted mb-1 flex items-center gap-1">
            <Tag size={11} /> Tags (comma-separated)
          </label>
          <input
            value={editTagsRaw}
            onChange={(e) => setEditTagsRaw(e.target.value)}
            placeholder="e.g. frontend, auth, bug"
            className="w-full bg-bg-input border border-border-strong/40 rounded px-2.5 py-1.5 text-[13px] text-text-base outline-none focus:border-brand/50 transition-colors"
          />
        </div>

        <div>
          <label className="block text-[11px] text-text-muted mb-1 flex items-center gap-1">
            <Link size={11} /> Linked files (one per line)
          </label>
          <textarea
            value={editLinkedFilesRaw}
            onChange={(e) => setEditLinkedFilesRaw(e.target.value)}
            rows={2}
            placeholder="src/components/Foo.tsx"
            className="w-full bg-bg-input border border-border-strong/40 rounded px-2.5 py-1.5 text-[13px] text-text-base outline-none focus:border-brand/50 transition-colors resize-none font-mono"
          />
        </div>

        <div>
          <div className="flex items-center justify-between mb-1">
            <label className="text-[11px] text-text-muted">Description</label>
            <button
              onClick={() => setDescPreview((prev) => !prev)}
              className="text-[11px] text-text-muted hover:text-text-base transition-colors"
            >
              {descPreview ? "Edit" : "Preview"}
            </button>
          </div>
          {descPreview ? (
            <div className="bg-bg-input border border-border-strong/40 rounded px-3 py-2 min-h-[80px] text-[13px]">
              <MarkdownPreview content={editDescription || "*No description*"} />
            </div>
          ) : (
            <textarea
              value={editDescription}
              onChange={(e) => setEditDescription(e.target.value)}
              rows={5}
              placeholder="Describe the feature in markdown…"
              className="w-full bg-bg-input border border-border-strong/40 rounded px-2.5 py-1.5 text-[13px] text-text-base outline-none focus:border-brand/50 transition-colors resize-none font-mono"
            />
          )}
        </div>

        <div className="flex items-center justify-between">
          <button
            onClick={handleCreate}
            disabled={saving || !editTitle.trim()}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded bg-brand hover:bg-brand-hover disabled:opacity-50 text-white text-[12px] font-medium transition-colors"
          >
            <Check size={12} />
            {saving ? "Creating…" : "Create feature"}
          </button>
          <button
            onClick={onCancel}
            className="px-3 py-1.5 text-[12px] text-text-muted hover:text-text-base transition-colors"
          >
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}

// ── List View ─────────────────────────────────────────────────────────────────

interface ListViewProps {
  features: Feature[];
  selectedId: string | null;
  filterState: string | null;
  filterPriority: string | null;
  sort: ListSort | null;
  onSelect: (id: string) => void;
  onFilterState: (s: string | null) => void;
  onFilterPriority: (p: string | null) => void;
  onSort: (sort: ListSort) => void;
  onAddNew: () => void;
}

interface SortableHeaderProps {
  label: string;
  column: ListSortKey;
  sort: ListSort | null;
  onSort: (sort: ListSort) => void;
}

function SortableHeader({ label, column, sort, onSort }: SortableHeaderProps) {
  const isActive = sort?.key === column;
  const nextDirection: SortDirection = isActive && sort.direction === "asc" ? "desc" : "asc";

  return (
    <button
      type="button"
      onClick={() => onSort({ key: column, direction: nextDirection })}
      className="group inline-flex items-center gap-1 text-inherit transition-colors hover:text-text-base"
    >
      <span>{label}</span>
      <ChevronDown
        size={12}
        className={`shrink-0 transition-all ${
          isActive
            ? sort.direction === "asc"
              ? "opacity-100 rotate-180 text-text-base"
              : "opacity-100 text-text-base"
            : "opacity-0 group-hover:opacity-50"
        }`}
      />
    </button>
  );
}

function ListView({
  features,
  selectedId,
  filterState,
  filterPriority,
  sort,
  onSelect,
  onFilterState,
  onFilterPriority,
  onSort,
  onAddNew,
}: ListViewProps) {
  const filtered = useMemo(() => {
    const visible = features.filter((f) => {
      if (filterState && f.state !== filterState) return false;
      if (filterPriority && f.priority !== filterPriority) return false;
      return true;
    });

    if (!sort) {
      return visible;
    }

    const direction = sort.direction === "asc" ? 1 : -1;

    return visible
      .map((feature, index) => ({ feature, index }))
      .sort((a, b) => {
        const comparison = compareFeatures(a.feature, b.feature, sort.key) * direction;
        return comparison !== 0 ? comparison : a.index - b.index;
      })
      .map(({ feature }) => feature);
  }, [features, filterPriority, filterState, sort]);

  return (
    <div className="flex flex-col h-full">
      {/* Filter bar */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-border-strong/40 shrink-0 flex-wrap">
        <span className="text-[11px] text-text-muted">Filter:</span>
        <Select
          value={filterState ?? ""}
          onChange={(v) => onFilterState(v || null)}
          size="xs"
          options={[
            { value: "", label: "All states" },
            ...STATES.map((s) => ({ value: s.id, label: s.label })),
          ]}
        />
        <Select
          value={filterPriority ?? ""}
          onChange={(v) => onFilterPriority(v || null)}
          size="xs"
          options={[
            { value: "", label: "All priorities" },
            ...PRIORITIES.map((p) => ({ value: p, label: p.charAt(0).toUpperCase() + p.slice(1) })),
          ]}
        />
      </div>

      {/* List */}
      <div className="flex-1 overflow-y-auto custom-scrollbar">
        {filtered.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full gap-3 text-text-muted py-12">
            <Kanban size={22} />
            <span className="text-[13px]">No features</span>
            <button
              onClick={onAddNew}
              className="text-[12px] text-brand hover:text-brand-hover transition-colors"
            >
              Create the first one
            </button>
          </div>
        ) : (
          <table className="w-full text-[13px]">
            <thead className="sticky top-0 bg-bg-base z-10">
              <tr className="border-b border-border-strong/40">
                <th className="text-left px-3 py-2 text-[11px] font-semibold text-text-muted uppercase tracking-wider">
                  <SortableHeader label="Title" column="title" sort={sort} onSort={onSort} />
                </th>
                <th className="text-left px-2 py-2 text-[11px] font-semibold text-text-muted uppercase tracking-wider w-24">
                  <SortableHeader label="State" column="state" sort={sort} onSort={onSort} />
                </th>
                <th className="text-left px-2 py-2 text-[11px] font-semibold text-text-muted uppercase tracking-wider w-20">
                  <SortableHeader label="Priority" column="priority" sort={sort} onSort={onSort} />
                </th>
                <th className="text-left px-2 py-2 text-[11px] font-semibold text-text-muted uppercase tracking-wider w-16">
                  <SortableHeader label="Effort" column="effort" sort={sort} onSort={onSort} />
                </th>
                <th className="text-left px-2 py-2 text-[11px] font-semibold text-text-muted uppercase tracking-wider w-28">
                  <SortableHeader label="Assignee" column="assignee" sort={sort} onSort={onSort} />
                </th>
                <th className="text-left px-2 py-2 text-[11px] font-semibold text-text-muted uppercase tracking-wider w-24">
                  <SortableHeader label="Updated" column="updated" sort={sort} onSort={onSort} />
                </th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((f) => (
                <tr
                  key={f.id}
                  onClick={() => onSelect(f.id)}
                  className={`border-b border-border-strong/20 cursor-pointer transition-colors ${
                    selectedId === f.id
                      ? "bg-bg-sidebar"
                      : "hover:bg-bg-sidebar/50"
                  }`}
                >
                  <td className="px-3 py-2">
                    <div className="flex items-center gap-2">
                      <PriorityDot priority={f.priority} />
                      <span className="font-medium text-text-base truncate max-w-[200px]">
                        {f.title}
                      </span>
                      {f.tags.length > 0 && (
                        <span className="text-[10px] text-text-muted">
                          {f.tags.slice(0, 2).join(", ")}
                          {f.tags.length > 2 && " +"}
                        </span>
                      )}
                    </div>
                  </td>
                  <td className="px-2 py-2">
                    <StateBadge state={f.state} />
                  </td>
                  <td className="px-2 py-2 text-text-muted capitalize">
                    {f.priority}
                  </td>
                  <td className="px-2 py-2 text-text-muted uppercase text-[11px]">
                    {f.effort ?? "—"}
                  </td>
                  <td className="px-2 py-2 text-text-muted truncate max-w-[110px]">
                    {f.assignee ?? "—"}
                  </td>
                  <td className="px-2 py-2 text-text-muted">
                    {formatDate(f.updated_at)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}

// ── Kanban Card ───────────────────────────────────────────────────────────────

interface KanbanCardProps {
  feature: Feature;
  isSelected: boolean;
  isDragging: boolean;
  onSelect: (id: string) => void;
  onGripDown: (feature: Feature, e: React.PointerEvent) => void;
}

function KanbanCard({ feature, isSelected, isDragging, onSelect, onGripDown }: KanbanCardProps) {
  return (
    <div
      onClick={() => { if (!isDragging) onSelect(feature.id); }}
      onPointerDown={(e) => {
        // Only initiate drag if left-clicking on the card itself (not its interactive children)
        if (e.button !== 0) return;
        onGripDown(feature, e);
      }}
      className={`group bg-bg-base border rounded-lg p-3 cursor-grab active:cursor-grabbing touch-none transition-colors select-none ${
        isDragging
          ? "opacity-40"
          : isSelected
          ? "border-brand/60 bg-bg-sidebar"
          : "border-border-strong/40 hover:border-border-strong/60"
      }`}
    >
      <div className="flex items-start gap-2 mb-2">
        <span className="text-[13px] font-medium text-text-base leading-snug flex-1">
          {feature.title}
        </span>
        <PriorityDot priority={feature.priority} />
      </div>
      <div className="flex items-center gap-1.5 flex-wrap">
        {feature.effort && (
          <span className="text-[10px] bg-bg-sidebar text-text-muted px-1.5 py-0.5 rounded uppercase">
            {feature.effort}
          </span>
        )}
        {feature.assignee && (
          <span className="text-[10px] text-text-muted truncate max-w-[80px]">
            {feature.assignee}
          </span>
        )}
        {feature.tags.slice(0, 2).map((tag) => (
          <span
            key={tag}
            className="text-[10px] bg-brand/10 text-brand px-1.5 py-0.5 rounded"
          >
            {tag}
          </span>
        ))}
      </div>
    </div>
  );
}

// ── Kanban View ───────────────────────────────────────────────────────────────
// Uses pointer events instead of HTML5 DnD — WKWebView (Tauri) does not fire
// dragstart reliably, but pointer events work fine (same approach as Projects.tsx).

interface KanbanViewProps {
  features: Feature[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onMove: (featureId: string, newState: string, newPosition: number) => void;
}

function KanbanView({
  features,
  selectedId,
  onSelect,
  onMove,
}: KanbanViewProps) {
  // id of the card currently being dragged
  const [draggingId, setDraggingId] = useState<string | null>(null);
  // stateId of the column the pointer is currently hovering over
  const [dragOverState, setDragOverState] = useState<string | null>(null);
  // ghost label that follows the cursor
  const [ghost, setGhost] = useState<{ title: string; x: number; y: number } | null>(null);
  // refs to each column element, keyed by stateId
  const colRefs = useRef<Record<string, HTMLDivElement | null>>({});

  const grouped = STATES.reduce(
    (acc, s) => {
      acc[s.id] = features.filter((f) => f.state === s.id);
      return acc;
    },
    {} as Record<string, Feature[]>
  );

  /** Walk point (x, y) through colRefs to find which state column it's over. */
  const resolveColumn = (x: number, y: number): string | null => {
    for (const stateId of Object.keys(colRefs.current)) {
      const el = colRefs.current[stateId];
      if (!el) continue;
      const r = el.getBoundingClientRect();
      if (x >= r.left && x <= r.right && y >= r.top && y <= r.bottom) {
        return stateId;
      }
    }
    return null;
  };

  const handleGripDown = (feature: Feature, e: React.PointerEvent) => {
    e.preventDefault();
    e.stopPropagation();
    (e.target as HTMLElement).setPointerCapture(e.pointerId);

    setDraggingId(feature.id);
    setGhost({ title: feature.title, x: e.clientX, y: e.clientY });

    const onPointerMove = (ev: PointerEvent) => {
      setGhost({ title: feature.title, x: ev.clientX, y: ev.clientY });
      setDragOverState(resolveColumn(ev.clientX, ev.clientY));
    };

    const onPointerUp = (ev: PointerEvent) => {
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", onPointerUp);

      const targetState = resolveColumn(ev.clientX, ev.clientY);

      setDraggingId(null);
      setDragOverState(null);
      setGhost(null);

      if (!targetState) return;
      const colFeatures = grouped[targetState] ?? [];
      const newPosition = colFeatures.length;
      onMove(feature.id, targetState, newPosition);
    };

    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
  };

  return (
    <>
      <div className="flex h-full overflow-x-auto gap-3 p-3 custom-scrollbar">
        {STATES.map((s) => {
          const col = grouped[s.id] ?? [];
          const isOver = dragOverState === s.id;
          return (
            <div
              key={s.id}
              ref={(el: HTMLDivElement | null) => { colRefs.current[s.id] = el; }}
              className="flex flex-col shrink-0 w-[220px]"
            >
              {/* Column header */}
              <div
                className={`flex items-center justify-between px-2 py-1.5 rounded-t border border-border-strong/40 ${
                  STATE_STYLES[s.id] ?? "bg-bg-sidebar text-text-muted"
                }`}
              >
                <span className="text-[11px] font-semibold">{s.label}</span>
                <span className="text-[10px] opacity-70">{col.length}</span>
              </div>

              {/* Cards */}
              <div
                className={`flex-1 overflow-y-auto custom-scrollbar border-x border-b border-border-strong/40 rounded-b p-1.5 space-y-1.5 min-h-[120px] transition-colors ${
                  isOver ? "bg-bg-sidebar/60 ring-1 ring-brand/30" : "bg-bg-input"
                }`}
              >
                {col.map((f) => (
                  <KanbanCard
                    key={f.id}
                    feature={f}
                    isSelected={selectedId === f.id}
                    isDragging={draggingId === f.id}
                    onSelect={onSelect}
                    onGripDown={handleGripDown}
                  />
                ))}

                {col.length === 0 && (
                  <div className="flex items-center justify-center h-16 text-[11px] text-text-muted opacity-50">
                    Drop here
                  </div>
                )}
              </div>
            </div>
          );
        })}
      </div>

      {/* Drag ghost — follows cursor while dragging */}
      {ghost && (
        <div
          className="fixed pointer-events-none z-50 px-2.5 py-1.5 bg-bg-base border border-brand/60 rounded-lg shadow-xl text-[12px] font-medium text-text-base max-w-[200px] truncate"
          style={{ left: ghost.x + 12, top: ghost.y - 16 }}
        >
          {ghost.title}
        </div>
      )}
    </>
  );
}

// ── Main Component ────────────────────────────────────────────────────────────

interface FeaturesProps {
  projectName: string;
}

function buildViewStorageKey(projectName: string): string {
  return `automatic.projects.${projectName}.build.view`;
}

function buildFilterStorageKey(projectName: string): string {
  return `automatic.projects.${projectName}.build.filters`;
}

function buildSortStorageKey(projectName: string): string {
  return `automatic.projects.${projectName}.build.sort`;
}

function readStoredFilters(projectName: string): { filterState: string | null; filterPriority: string | null } {
  try {
    const raw = localStorage.getItem(buildFilterStorageKey(projectName));
    if (!raw) {
      return { filterState: null, filterPriority: null };
    }
    const parsed = JSON.parse(raw) as { filterState?: unknown; filterPriority?: unknown };
    return {
      filterState: typeof parsed.filterState === "string" && parsed.filterState.length > 0 ? parsed.filterState : null,
      filterPriority: typeof parsed.filterPriority === "string" && parsed.filterPriority.length > 0 ? parsed.filterPriority : null,
    };
  } catch {
    return { filterState: null, filterPriority: null };
  }
}

function readStoredSort(projectName: string): ListSort | null {
  try {
    const raw = localStorage.getItem(buildSortStorageKey(projectName));
    if (!raw) {
      return null;
    }

    const parsed = JSON.parse(raw) as { key?: unknown; direction?: unknown };
    const validKeys: ListSortKey[] = ["title", "state", "priority", "effort", "assignee", "updated"];

    if (
      typeof parsed.key === "string" &&
      validKeys.includes(parsed.key as ListSortKey) &&
      (parsed.direction === "asc" || parsed.direction === "desc")
    ) {
      return {
        key: parsed.key as ListSortKey,
        direction: parsed.direction,
      };
    }

    return null;
  } catch {
    return null;
  }
}

export default function Features({ projectName }: FeaturesProps) {
  const [features, setFeatures] = useState<Feature[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [view, setView] = useState<"list" | "kanban">(() => {
    try {
      const stored = localStorage.getItem(buildViewStorageKey(projectName));
      return stored === "kanban" ? "kanban" : "list";
    } catch {
      return "list";
    }
  });
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [isCreating, setIsCreating] = useState(false);
  const [filterState, setFilterState] = useState<string | null>(() => readStoredFilters(projectName).filterState);
  const [filterPriority, setFilterPriority] = useState<string | null>(() => readStoredFilters(projectName).filterPriority);
  const [sort, setSort] = useState<ListSort | null>(() => readStoredSort(projectName));

  // Resizable split pane
  const [detailWidth, setDetailWidth] = useState(DETAIL_DEFAULT);
  const resizing = useRef(false);
  const startX = useRef(0);
  const startWidth = useRef(0);

  const loadFeatures = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<Feature[]>("list_features", {
        project: projectName,
        state: null,
      });
      setFeatures(result);
    } catch (err: any) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [projectName]);

  // Silent background refresh — does not touch loading/error state so the UI
  // does not flash. Used by the polling interval.
  const refreshFeatures = useCallback(async () => {
    try {
      const result = await invoke<Feature[]>("list_features", {
        project: projectName,
        state: null,
      });
      setFeatures(result);
    } catch {
      // Silently ignore transient poll failures.
    }
  }, [projectName]);

  // Initial load
  useEffect(() => {
    loadFeatures();
  }, [loadFeatures]);

  // Poll every 5 seconds while the component is mounted so agent-driven state
  // changes and new features appear without requiring a manual refresh.
  useEffect(() => {
    const interval = setInterval(refreshFeatures, 5_000);
    return () => clearInterval(interval);
  }, [refreshFeatures]);

  useEffect(() => {
    try {
      const stored = localStorage.getItem(buildViewStorageKey(projectName));
      setView(stored === "kanban" ? "kanban" : "list");
    } catch {
      setView("list");
    }
  }, [projectName]);

  useEffect(() => {
    try {
      localStorage.setItem(buildViewStorageKey(projectName), view);
    } catch {
      // Ignore storage failures and keep the in-memory preference.
    }
  }, [projectName, view]);

  useEffect(() => {
    const stored = readStoredFilters(projectName);
    setFilterState(stored.filterState);
    setFilterPriority(stored.filterPriority);
  }, [projectName]);

  useEffect(() => {
    setSort(readStoredSort(projectName));
  }, [projectName]);

  useEffect(() => {
    try {
      localStorage.setItem(
        buildFilterStorageKey(projectName),
        JSON.stringify({ filterState, filterPriority })
      );
    } catch {
      // Ignore storage failures and keep the in-memory filters.
    }
  }, [filterPriority, filterState, projectName]);

  useEffect(() => {
    try {
      if (!sort) {
        localStorage.removeItem(buildSortStorageKey(projectName));
        return;
      }

      localStorage.setItem(buildSortStorageKey(projectName), JSON.stringify(sort));
    } catch {
      // Ignore storage failures and keep the in-memory sort.
    }
  }, [projectName, sort]);

  // Drag resize handlers
  const handleResizeMouseDown = (e: React.MouseEvent) => {
    resizing.current = true;
    startX.current = e.clientX;
    startWidth.current = detailWidth;
    e.preventDefault();
  };

  useEffect(() => {
    const onMouseMove = (e: MouseEvent) => {
      if (!resizing.current) return;
      const delta = startX.current - e.clientX;
      const next = Math.min(
        DETAIL_MAX,
        Math.max(DETAIL_MIN, startWidth.current + delta)
      );
      setDetailWidth(next);
    };
    const onMouseUp = () => {
      resizing.current = false;
    };
    window.addEventListener("mousemove", onMouseMove);
    window.addEventListener("mouseup", onMouseUp);
    return () => {
      window.removeEventListener("mousemove", onMouseMove);
      window.removeEventListener("mouseup", onMouseUp);
    };
  }, []);

  const handleSelect = (id: string) => {
    setSelectedId((prev) => (prev === id ? null : id));
  };

  const handleCreated = (f: Feature) => {
    setFeatures((prev) => [f, ...prev]);
    setIsCreating(false);
    setSelectedId(f.id);
  };

  const handleUpdated = (f: Feature) => {
    setFeatures((prev) => prev.map((x) => (x.id === f.id ? f : x)));
  };

  const handleDeleted = (id: string) => {
    setFeatures((prev) => prev.filter((f) => f.id !== id));
    setSelectedId(null);
  };

  const handleMove = async (
    featureId: string,
    newState: string,
    newPosition: number
  ) => {
    // Optimistic update
    setFeatures((prev) =>
      prev.map((f) =>
        f.id === featureId
          ? { ...f, state: newState, position: newPosition }
          : f
      )
    );
    try {
      await invoke("move_feature", {
        project: projectName,
        featureId,
        newState,
        newPosition,
      });
      // Reload to get server-side canonical ordering
      await loadFeatures();
    } catch (err: any) {
      setError(String(err));
      await loadFeatures(); // revert optimistic update
    }
  };

  const detailOpen = !!selectedId;

  return (
    <div className="flex h-full overflow-hidden">
      {/* Left pane — list or kanban */}
      <div className="flex-1 flex flex-col min-w-0 overflow-hidden">
        {/* Toolbar */}
        <div className="flex items-center gap-2 px-3 py-2 border-b border-border-strong/40 shrink-0">
          <span className="text-[13px] font-medium text-text-base">Features</span>
          {!loading && (
            <span className="text-[11px] text-text-muted">
              {features.length} total
            </span>
          )}
          <div className="flex-1" />
          {/* View switcher */}
          <div className="flex items-center border border-border-strong/40 rounded overflow-hidden">
            <button
              onClick={() => setView("list")}
              className={`flex items-center gap-1 px-2 py-1 text-[11px] transition-colors ${
                view === "list"
                  ? "bg-bg-sidebar text-text-base"
                  : "text-text-muted hover:text-text-base"
              }`}
            >
              <List size={11} /> List
            </button>
            <button
              onClick={() => setView("kanban")}
              className={`flex items-center gap-1 px-2 py-1 text-[11px] transition-colors ${
                view === "kanban"
                  ? "bg-bg-sidebar text-text-base"
                  : "text-text-muted hover:text-text-base"
              }`}
            >
              <Kanban size={11} /> Board
            </button>
          </div>
          <button
            onClick={() => setIsCreating(true)}
            disabled={isCreating}
            className="flex items-center gap-1 px-2.5 py-1 rounded bg-brand hover:bg-brand-hover disabled:opacity-50 disabled:cursor-default text-white text-[11px] font-medium transition-colors"
          >
            <Plus size={11} /> New feature
          </button>
        </div>

        {/* Error banner */}
        {error && (
          <div className="mx-3 mt-2 px-3 py-2 bg-danger/10 border border-danger/30 rounded text-[12px] text-danger flex items-center gap-2 shrink-0">
            <AlertCircle size={12} />
            <span className="flex-1">{error}</span>
            <button onClick={() => setError(null)}>
              <X size={11} />
            </button>
          </div>
        )}

        {/* Content */}
        {loading ? (
          <div className="flex-1 flex items-center justify-center text-text-muted text-[13px]">
            Loading features…
          </div>
        ) : view === "list" ? (
          <ListView
            features={features}
            selectedId={selectedId}
            filterState={filterState}
            filterPriority={filterPriority}
            sort={sort}
            onSelect={handleSelect}
            onFilterState={setFilterState}
            onFilterPriority={setFilterPriority}
            onSort={setSort}
            onAddNew={() => setIsCreating(true)}
            isCreating={isCreating}
            projectName={projectName}
            onCreated={handleCreated}
            onCancelCreate={() => setIsCreating(false)}
          />
        ) : (
          <KanbanView
            features={features}
            selectedId={selectedId}
            onSelect={handleSelect}
            onMove={handleMove}
            isCreating={isCreating}
            projectName={projectName}
            onCreated={handleCreated}
            onCancelCreate={() => setIsCreating(false)}
          />
        )}
      </div>

      {/* Resize handle + detail panel */}
      {detailOpen && (
        <>
          <div
            onMouseDown={handleResizeMouseDown}
            className="w-px bg-border-strong/40 hover:bg-brand/50 cursor-col-resize transition-colors shrink-0"
          />
          <div
            className="flex flex-col border-l border-border-strong/40 overflow-hidden shrink-0"
            style={{ width: detailWidth }}
          >
            {selectedId && (
              <DetailPanel
                projectName={projectName}
                featureId={selectedId}
                onClose={() => setSelectedId(null)}
                onUpdated={handleUpdated}
                onDeleted={handleDeleted}
              />
            )}
          </div>
        </>
      )}
    </div>
  );
}
