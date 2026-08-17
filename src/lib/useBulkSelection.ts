import { useState } from "react";

export interface BulkSelection<T> {
  selectedIds: Set<string>;
  /** Items from the currently-visible set that are allowed to be deleted. */
  deletableItems: T[];
  totalSelected: number;
  allSelected: boolean;
  someSelected: boolean;
  isSelected: (id: string) => boolean;
  toggleSelected: (id: string) => void;
  toggleSelectAllVisible: () => void;
  clearSelection: () => void;
}

/**
 * Set<string>-based multi-select over a currently-visible (already
 * filtered/searched) item list. `isDeletable` excludes protected, bundled,
 * or plugin-owned items from select-all so an undeletable row can never
 * enter the selection through "select all".
 */
export function useBulkSelection<T>(
  visibleItems: T[],
  getId: (item: T) => string,
  isDeletable: (item: T) => boolean,
): BulkSelection<T> {
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());

  const deletableItems = visibleItems.filter(isDeletable);
  const selectedCount = deletableItems.filter((item) => selectedIds.has(getId(item))).length;
  const allSelected = deletableItems.length > 0 && selectedCount === deletableItems.length;
  const someSelected = selectedCount > 0 && !allSelected;
  const totalSelected = selectedIds.size;

  const toggleSelected = (id: string) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const toggleSelectAllVisible = () => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (allSelected) {
        for (const item of deletableItems) next.delete(getId(item));
      } else {
        for (const item of deletableItems) next.add(getId(item));
      }
      return next;
    });
  };

  const clearSelection = () => setSelectedIds(new Set());

  return {
    selectedIds,
    deletableItems,
    totalSelected,
    allSelected,
    someSelected,
    isSelected: (id: string) => selectedIds.has(id),
    toggleSelected,
    toggleSelectAllVisible,
    clearSelection,
  };
}
