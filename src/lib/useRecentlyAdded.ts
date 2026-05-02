import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * Returns the set of asset IDs added to the library within the last 7 days.
 *
 * The set is stable across re-renders; it is only re-fetched when `assetType`
 * changes or when `refreshToken` is incremented by the caller.
 */
export function useRecentlyAdded(
  assetType: string,
  refreshToken: number = 0,
): Set<string> {
  const [recentIds, setRecentIds] = useState<Set<string>>(new Set());

  useEffect(() => {
    invoke<string[]>("get_recently_added_items", { assetType })
      .then((ids) => setRecentIds(new Set(ids)))
      .catch(() => setRecentIds(new Set()));
  }, [assetType, refreshToken]);

  return recentIds;
}
