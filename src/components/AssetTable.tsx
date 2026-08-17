import type { ReactNode } from "react";
import { RecentlyAddedSectionLabel, RecentlyAddedDivider } from "./RecentlyAddedMarker";

export interface AssetTableColumn {
  key: string;
  header: ReactNode;
  /** e.g. "w-11", "w-16 text-right" — column headers otherwise flow naturally. */
  className?: string;
}

export interface AssetTableSelection {
  allSelected: boolean;
  someSelected: boolean;
  disabled: boolean;
  onToggleAll: () => void;
  ariaLabel: string;
}

interface AssetTableProps<T> {
  /** Already filtered/searched items, in display order. */
  items: T[];
  getId: (item: T) => string;
  /** True when the section has no data at all, before any filtering — shows emptyState instead of the table. */
  isEmpty: boolean;
  emptyState: ReactNode;
  /** Shown instead of the table when items.length === 0 but isEmpty is false (filtered/searched down to nothing). */
  noMatchState: ReactNode;
  columns: AssetTableColumn[];
  /** Returns a full `<tr>...</tr>` for one item. */
  renderRow: (item: T) => ReactNode;
  /** Omit to render the table without a checkbox column. */
  selection?: AssetTableSelection;
  /** Ids to render under a "Recently added" group above the rest. Omit to disable grouping. */
  recentIds?: Set<string>;
}

function SelectAllCheckbox({ allSelected, someSelected, disabled, onToggleAll, ariaLabel }: AssetTableSelection) {
  return (
    <input
      type="checkbox"
      checked={allSelected}
      ref={(el) => {
        if (el) el.indeterminate = someSelected;
      }}
      onChange={onToggleAll}
      disabled={disabled}
      aria-label={ariaLabel}
      className="cursor-pointer accent-brand disabled:opacity-30"
    />
  );
}

/**
 * Full-width table shell shared by every Library section's table+drawer
 * interface: sticky header, optional checkbox column, "recently added"
 * grouping, and empty/no-match states. Row content and column headers are
 * supplied by the caller — only the table's structural chrome is generic.
 */
export function AssetTable<T>({
  items,
  getId,
  isEmpty,
  emptyState,
  noMatchState,
  columns,
  renderRow,
  selection,
  recentIds,
}: AssetTableProps<T>) {
  return (
    <div className="flex-1 min-h-0 overflow-auto custom-scrollbar">
      {isEmpty ? (
        <div className="h-full flex flex-col items-center justify-center text-center p-8">{emptyState}</div>
      ) : items.length === 0 ? (
        <div className="h-full flex items-center justify-center px-4 py-6 text-center">{noMatchState}</div>
      ) : (
        (() => {
          const recent = recentIds ? items.filter((i) => recentIds.has(getId(i))) : [];
          const rest = recentIds ? items.filter((i) => !recentIds.has(getId(i))) : items;
          const colSpan = columns.length + (selection ? 1 : 0);
          return (
            <table className="w-full border-collapse text-[12px]">
              <thead className="sticky top-0 bg-bg-input/95 backdrop-blur z-10">
                <tr className="border-b border-border-strong/40 text-left text-[11px] font-medium uppercase tracking-wide text-text-muted">
                  {selection && (
                    <th className="px-3 py-2 w-9">
                      <SelectAllCheckbox {...selection} />
                    </th>
                  )}
                  {columns.map((col) => (
                    <th key={col.key} className={`px-3 py-2 font-medium ${col.className ?? ""}`}>
                      {col.header}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {recent.length > 0 && (
                  <tr className="bg-bg-input/30">
                    <td colSpan={colSpan} className="px-3 py-1.5">
                      <RecentlyAddedSectionLabel />
                    </td>
                  </tr>
                )}
                {recent.map(renderRow)}
                {recent.length > 0 && rest.length > 0 && (
                  <tr>
                    <td colSpan={colSpan} className="px-3 py-1">
                      <RecentlyAddedDivider />
                    </td>
                  </tr>
                )}
                {rest.map(renderRow)}
              </tbody>
            </table>
          );
        })()
      )}
    </div>
  );
}
