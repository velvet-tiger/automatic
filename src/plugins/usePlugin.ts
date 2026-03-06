/**
 * usePlugin — React hook for checking whether a bundled plugin is enabled.
 *
 * Plugin state is loaded once from the Tauri backend and cached in module-level
 * state so that all consumers within the same render cycle see the same values
 * without triggering extra invokes.
 *
 * Usage:
 *   const aiPlayground = usePlugin("ai_playground");
 *   if (aiPlayground) { ... }
 *
 * Returns `false` while loading (safe for conditional rendering).
 */

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

// ── Module-level cache ────────────────────────────────────────────────────────

/** All plugin enabled states, keyed by plugin id. */
const _pluginCache = new Map<string, boolean>();
let _loaded = false;
/** Callbacks waiting for the initial load to complete. */
const _listeners: Array<() => void> = [];

async function _ensureLoaded(): Promise<void> {
  if (_loaded) return;

  try {
    const entries = await invoke<Array<{ id: string; enabled: boolean }>>("list_app_plugins");
    for (const entry of entries) {
      _pluginCache.set(entry.id, entry.enabled);
    }
  } catch (e) {
    console.warn("[usePlugin] Failed to load plugin state:", e);
  }

  _loaded = true;
  for (const cb of _listeners) cb();
  _listeners.length = 0;
}

// Start loading immediately on module import (before any component mounts).
_ensureLoaded();

// ── Hook ──────────────────────────────────────────────────────────────────────

/**
 * Returns whether the plugin with the given id is enabled.
 *
 * - Returns `false` while plugin state is being loaded (safe to gate on).
 * - If the plugin id is unknown, returns `false`.
 * - Use `refreshPlugins()` to invalidate the cache after toggling plugins
 *   programmatically outside of SettingsPlugins.
 */
export function usePlugin(id: string): boolean {
  const [enabled, setEnabled] = useState<boolean>(() => _pluginCache.get(id) ?? false);

  useEffect(() => {
    let cancelled = false;

    async function resolve() {
      if (!_loaded) {
        // Wait for initial load.
        await new Promise<void>((resolve) => _listeners.push(resolve));
      }
      if (!cancelled) {
        setEnabled(_pluginCache.get(id) ?? false);
      }
    }

    resolve();

    return () => {
      cancelled = true;
    };
  }, [id]);

  return enabled;
}

/**
 * Invalidate the plugin cache and re-fetch from the backend.
 * Call this after the user toggles a plugin in Settings if other components
 * need to react immediately (e.g. showing/hiding nav items).
 */
export async function refreshPlugins(): Promise<void> {
  _loaded = false;
  _pluginCache.clear();
  await _ensureLoaded();
}

/**
 * Synchronous snapshot — safe to call outside React components.
 * Returns `false` if not yet loaded or unknown.
 */
export function pluginEnabled(id: string): boolean {
  return _pluginCache.get(id) ?? false;
}
