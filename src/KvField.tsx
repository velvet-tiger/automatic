/**
 * Shared key-value field components used by McpServers and McpSelector.
 *
 * KvList   — renders existing key=value rows with an optional inline edit field
 *            and a remove (trash) button.
 *
 * Shared CSS constants are also exported so callers can compose inputs with a
 * consistent look.
 */

import { Trash2 } from "lucide-react";

// ── CSS class helpers ──────────────────────────────────────────────────────

export const inputClass =
  "w-full bg-bg-input border border-surface hover:border-border-strong focus:border-brand rounded-md px-3 py-2 text-[13px] text-text-base placeholder-text-muted/40 outline-none font-mono transition-colors";

export const smallInputClass =
  "flex-1 bg-bg-input border border-surface hover:border-border-strong focus:border-brand rounded-md px-3 py-1.5 text-[13px] text-text-base placeholder-text-muted/40 outline-none font-mono transition-colors";

export const addBtnClass =
  "px-3 py-1.5 bg-bg-sidebar hover:bg-surface text-text-base text-[12px] font-medium rounded border border-surface-hover transition-colors disabled:opacity-30 disabled:cursor-not-allowed";

// ── KvList ─────────────────────────────────────────────────────────────────

interface KvListProps {
  entries: [string, string][];
  onRemove: (key: string) => void;
  /** When provided, the value cell becomes an inline text input. */
  onEdit?: (key: string, value: string) => void;
  /** Highlight the key in brand colour. */
  colorKey?: boolean;
}

/**
 * Renders a list of key=value rows.
 *
 * - Each row shows `key = value` in a monospace pill.
 * - A trash icon appears on hover to remove the entry.
 * - When `onEdit` is provided the value becomes an editable input.
 * - When `colorKey` is true the key is rendered in the brand colour.
 */
export function KvList({ entries, onRemove, onEdit, colorKey }: KvListProps) {
  if (entries.length === 0) return null;
  return (
    <ul className="space-y-1 mb-2">
      {entries.map(([key, val]) => (
        <li
          key={key}
          className="group flex items-center gap-2 px-3 py-1.5 bg-bg-input rounded-md border border-surface hover:border-border-strong transition-colors text-[13px] font-mono"
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
