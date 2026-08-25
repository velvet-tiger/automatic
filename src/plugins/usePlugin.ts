/**
 * usePlugin — React hook for checking whether a bundled plugin is enabled.
 *
 * Plugin state lives in a module-level store shared by every consumer.
 * `usePlugin` subscribes to that store via `useSyncExternalStore` for the
 * component's whole mounted lifetime, so a later `refreshPlugins()` call
 * (e.g. after toggling a plugin in Settings) re-renders every consumer
 * immediately — including ones, like the sidebar, that mounted long before
 * the toggle.
 *
 * Usage:
 *   const aiPlayground = usePlugin("ai_playground");
 *   if (aiPlayground) { ... }
 *
 * Returns `false` while loading (safe for conditional rendering).
 */

import { useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";

// ── Module-level store ────────────────────────────────────────────────────────

/** All plugin enabled states, keyed by plugin id. */
const _pluginCache = new Map<string, boolean>();
/** Components currently subscribed via useSyncExternalStore. */
const _subscribers = new Set<() => void>();

function _notify(): void {
  for (const cb of _subscribers) cb();
}

async function _fetchAndApply(): Promise<void> {
  try {
    const entries = await invoke<Array<{ id: string; enabled: boolean }>>("list_app_plugins");
    _pluginCache.clear();
    for (const entry of entries) {
      _pluginCache.set(entry.id, entry.enabled);
    }
  } catch (e) {
    console.warn("[usePlugin] Failed to load plugin state:", e);
  }
  _notify();
  if (typeof window !== "undefined") {
    window.dispatchEvent(new CustomEvent("plugins-updated"));
  }
}

// Start loading immediately on module import (before any component mounts).
void _fetchAndApply();

// ── Hook ──────────────────────────────────────────────────────────────────────

function _subscribe(onStoreChange: () => void): () => void {
  _subscribers.add(onStoreChange);
  return () => _subscribers.delete(onStoreChange);
}

/**
 * Returns whether the plugin with the given id is enabled.
 *
 * - Returns `false` while plugin state is being loaded (safe to gate on).
 * - If the plugin id is unknown, returns `false`.
 * - Stays subscribed for the component's lifetime; call `refreshPlugins()`
 *   after toggling a plugin so every mounted consumer picks up the change.
 */
export function usePlugin(id: string): boolean {
  return useSyncExternalStore(_subscribe, () => _pluginCache.get(id) ?? false);
}

/**
 * Re-fetch plugin state from the backend and notify every subscribed
 * `usePlugin` consumer. Call this after the user toggles a plugin in
 * Settings so nav items, tool cards, etc. update without an app restart.
 */
export async function refreshPlugins(): Promise<void> {
  await _fetchAndApply();
}

/**
 * Synchronous snapshot — safe to call outside React components.
 * Returns `false` if not yet loaded or unknown.
 */
export function pluginEnabled(id: string): boolean {
  return _pluginCache.get(id) ?? false;
}
