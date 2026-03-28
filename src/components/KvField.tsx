/**
 * Shared key-value field components used by McpServers and McpSelector.
 *
 * KvEditor — unified editor that renders every row (existing + new) with the
 *             same [KEY input] [value input] [action] layout.
 *             Existing rows have a locked key; new row has both fields editable.
 *             When maskValue is true, value inputs render as password fields
 *             with a per-row Eye / EyeOff visibility toggle.
 *
 * KvList   — legacy read-only list with optional inline value edit.
 *             Kept for callers that haven't migrated yet.
 *
 * Shared CSS constants are also exported so callers can compose inputs with a
 * consistent look.
 */

import { Eye, EyeOff, Plus, Trash2 } from "lucide-react";
import { useState } from "react";

// ── CSS class helpers ──────────────────────────────────────────────────────

export const inputClass =
  "w-full bg-bg-input border border-border-strong/40 hover:border-border-strong focus:border-brand rounded-md px-3 py-2 text-[13px] text-text-base placeholder-text-muted/40 outline-none font-mono transition-colors";

export const smallInputClass =
  "flex-1 bg-bg-input border border-border-strong/40 hover:border-border-strong focus:border-brand rounded-md px-3 py-1.5 text-[13px] text-text-base placeholder-text-muted/40 outline-none font-mono transition-colors";

export const addBtnClass =
  "px-3 py-1.5 bg-bg-sidebar hover:bg-surface text-text-base text-[12px] font-medium rounded border border-surface-hover transition-colors disabled:opacity-30 disabled:cursor-not-allowed";

// ── KvEditor ───────────────────────────────────────────────────────────────

interface KvEditorProps {
  /** Current key-value map. */
  entries: Record<string, string>;
  /** Called with the full updated map on any change. */
  onChange: (updated: Record<string, string>) => void;
  /** When true, all fields are read-only (managed server). */
  readOnly?: boolean;
  /** Placeholder for the key column. */
  keyPlaceholder?: string;
  /** Placeholder for the value column. */
  valuePlaceholder?: string;
  /** Highlight existing keys in brand colour. */
  colorKey?: boolean;
  /**
   * When true, value inputs are rendered as password fields (characters
   * hidden) with a per-row Eye / EyeOff toggle to reveal the value.
   */
  maskValue?: boolean;
}

/**
 * Unified key-value editor.
 *
 * Every row uses the same two-field layout:
 *   [KEY input]  [value input]  [trash / add button]
 *
 * Existing rows: key is read-only, value is editable, button removes the entry.
 * New row (always shown at the bottom): both fields editable, button adds the entry.
 *
 * When maskValue is true an Eye / EyeOff button appears per row so users can
 * temporarily reveal a secret without exposing all values at once.
 */
export function KvEditor({
  entries,
  onChange,
  readOnly = false,
  keyPlaceholder = "KEY",
  valuePlaceholder = "value",
  colorKey = false,
  maskValue = false,
}: KvEditorProps) {
  const [newKey, setNewKey] = useState("");
  const [newVal, setNewVal] = useState("");

  // Set of keys whose values are currently revealed (only meaningful when maskValue=true).
  const [visibleKeys, setVisibleKeys] = useState<Set<string>>(new Set());
  // Visibility state for the new-entry value field.
  const [newValVisible, setNewValVisible] = useState(false);

  const toggleKeyVisibility = (key: string) => {
    setVisibleKeys((prev) => {
      const next = new Set(prev);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  };

  const rowClass = "flex items-center gap-2";

  const keyInputClass =
    "w-36 flex-shrink-0 bg-bg-input border border-border-strong/40 rounded-md px-3 py-1.5 text-[13px] font-mono outline-none transition-colors";

  const valInputClass =
    "flex-1 min-w-0 bg-bg-input border border-border-strong/40 hover:border-border-strong focus:border-brand rounded-md px-3 py-1.5 text-[13px] text-text-base placeholder-text-muted/40 outline-none font-mono transition-colors";

  const iconBtnClass =
    "flex-shrink-0 p-1.5 text-text-muted hover:text-text-base transition-colors rounded";

  const handleEditValue = (key: string, value: string) => {
    onChange({ ...entries, [key]: value });
  };

  const handleRemove = (key: string) => {
    const { [key]: _, ...rest } = entries;
    // Also clean up visibility state for the removed key.
    setVisibleKeys((prev) => {
      const next = new Set(prev);
      next.delete(key);
      return next;
    });
    onChange(rest);
  };

  const handleAdd = () => {
    const k = newKey.trim();
    if (!k) return;
    onChange({ ...entries, [k]: newVal });
    setNewKey("");
    setNewVal("");
    setNewValVisible(false);
  };

  return (
    <div className="space-y-1.5">
      {/* Existing rows */}
      {Object.entries(entries).map(([key, val]) => {
        const isVisible = !maskValue || visibleKeys.has(key);
        return (
          <div key={key} className={rowClass}>
            <input
              type="text"
              value={key}
              readOnly
              className={`${keyInputClass} ${colorKey ? "text-brand" : "text-text-base"} opacity-70 cursor-default`}
              tabIndex={-1}
            />
            <input
              type={isVisible ? "text" : "password"}
              value={val}
              readOnly={readOnly}
              onChange={(e) => !readOnly && handleEditValue(key, e.target.value)}
              placeholder={valuePlaceholder}
              className={`${valInputClass} ${readOnly ? "opacity-60 cursor-not-allowed" : ""}`}
            />
            {maskValue && !readOnly && (
              <button
                type="button"
                onClick={() => toggleKeyVisibility(key)}
                className={iconBtnClass}
                title={isVisible ? "Hide value" : "Reveal value"}
              >
                {isVisible ? <EyeOff size={13} /> : <Eye size={13} />}
              </button>
            )}
            {!readOnly && (
              <button
                type="button"
                onClick={() => handleRemove(key)}
                className={`${iconBtnClass} hover:text-danger`}
                title="Remove"
              >
                <Trash2 size={13} />
              </button>
            )}
          </div>
        );
      })}

      {/* New entry row */}
      {!readOnly && (
        <div className={rowClass}>
          <input
            type="text"
            value={newKey}
            onChange={(e) => setNewKey(e.target.value)}
            placeholder={keyPlaceholder}
            className={`${keyInputClass} text-text-base hover:border-border-strong focus:border-brand`}
          />
          <input
            type={maskValue && !newValVisible ? "password" : "text"}
            value={newVal}
            onChange={(e) => setNewVal(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && newKey.trim()) {
                e.preventDefault();
                handleAdd();
              }
            }}
            placeholder={valuePlaceholder}
            className={valInputClass}
          />
          {maskValue && (
            <button
              type="button"
              onClick={() => setNewValVisible((v) => !v)}
              className={iconBtnClass}
              title={newValVisible ? "Hide value" : "Reveal value"}
            >
              {newValVisible ? <EyeOff size={13} /> : <Eye size={13} />}
            </button>
          )}
          <button
            type="button"
            onClick={handleAdd}
            disabled={!newKey.trim()}
            className={`${iconBtnClass} disabled:opacity-30 disabled:cursor-not-allowed`}
            title="Add"
          >
            <Plus size={13} />
          </button>
        </div>
      )}
    </div>
  );
}

// ── KvList (legacy) ────────────────────────────────────────────────────────

interface KvListProps {
  entries: [string, string][];
  onRemove: (key: string) => void;
  /** When provided, the value cell becomes an inline text input. */
  onEdit?: (key: string, value: string) => void;
  /** Highlight the key in brand colour. */
  colorKey?: boolean;
}

/**
 * Legacy read-only list of key=value pills.
 * Prefer KvEditor for new usage.
 */
export function KvList({ entries, onRemove, onEdit, colorKey }: KvListProps) {
  if (entries.length === 0) return null;
  return (
    <ul className="space-y-1 mb-2">
      {entries.map(([key, val]) => (
        <li
          key={key}
          className="group flex items-center gap-2 px-3 py-1.5 bg-bg-input rounded-md border border-border-strong/40 hover:border-border-strong transition-colors text-[13px] font-mono"
        >
          <span className={`flex-shrink-0 ${colorKey ? "text-brand" : "text-text-base"}`}>
            {key}
          </span>
          <span className="text-text-muted flex-shrink-0">=</span>
          {onEdit ? (
            <input
              type="text"
              value={val}
              onChange={(e) => onEdit(key, e.target.value)}
              className="flex-1 min-w-0 bg-transparent outline-none text-text-base placeholder-text-muted/40"
              placeholder="value"
            />
          ) : (
            <span className="flex-1 truncate text-text-base">{val}</span>
          )}
          <button
            onClick={() => onRemove(key)}
            className="text-text-muted hover:text-danger opacity-0 group-hover:opacity-100 transition-all flex-shrink-0"
          >
            <Trash2 size={12} />
          </button>
        </li>
      ))}
    </ul>
  );
}
