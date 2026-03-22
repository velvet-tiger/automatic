/**
 * Feature flag module — resolves flags from two sources:
 *
 *   1. `.env` at build time: set `VITE_FLAGS=flag_a,flag_b` (comma-separated).
 *      Vite inlines the value into the JS bundle at compile time, so there is
 *      no runtime file I/O or network request for the base layer.
 *
 *   2. Remote API (optional, future): call `loadRemoteFlags(fetcher)` once
 *      after startup to merge flags from a backend endpoint. Remote flags take
 *      precedence over env flags. The `fetcher` is dependency-injected so the
 *      call site controls auth, caching, and error handling.
 *
 * Usage:
 *
 *   import { flag } from "./flags";
 *
 *   if (flag("my_feature")) { ... }
 *
 * Guards:
 *   - `flag()` never throws — unknown flags return false.
 *   - Remote load failures are caught and logged; env flags remain active.
 *   - All flag names are normalised to lowercase.
 */

// ─── State ────────────────────────────────────────────────────────────────────

/** Active flags resolved from env and (optionally) remote. */
const _flags = new Map<string, boolean>();

// ─── Bootstrap from .env ─────────────────────────────────────────────────────

/**
 * Parse the compile-time VITE_FLAGS value and seed the map.
 *
 * VITE_FLAGS is a comma-separated list of flag names, e.g.:
 *   VITE_FLAGS=new_dashboard,experimental_sync
 *
 * Any flag present in the list is considered enabled (true).
 * Flags absent from the list are not stored (treated as false by `flag()`).
 */
function _loadEnvFlags(): void {
  const raw: string = import.meta.env.VITE_FLAGS ?? "";
  if (!raw.trim()) return;

  for (const name of raw.split(",")) {
    const key = name.trim().toLowerCase();
    if (key) _flags.set(key, true);
  }

  if (_flags.size > 0) {
    console.info("[flags] env flags active:", [..._flags.keys()].join(", "));
  }
}

// Seed immediately on module load — synchronous, no I/O.
_loadEnvFlags();

// ─── Remote flag loader ───────────────────────────────────────────────────────

/**
 * A function that fetches remote flag state.
 * Receives the current env-derived flags so the remote source can base
 * decisions on what is already known locally.
 *
 * Should return a `Record<string, boolean>` — keys present with `true` are
 * enabled, keys present with `false` are explicitly disabled (override env),
 * and absent keys leave the env value unchanged.
 */
export type RemoteFlagFetcher = (
  envFlags: Readonly<Record<string, boolean>>
) => Promise<Record<string, boolean>>;

let _remoteLoaded = false;

/**
 * Merge remote flag overrides into the active flag set.
 *
 * Call once after the user session is established. Subsequent calls are
 * no-ops unless `force` is true.
 *
 * @param fetcher  Async function that returns `Record<string, boolean>`.
 * @param force    Re-fetch even if remote flags were already loaded.
 */
export async function loadRemoteFlags(
  fetcher: RemoteFlagFetcher,
  force = false
): Promise<void> {
  if (_remoteLoaded && !force) return;

  try {
    const snapshot = Object.fromEntries(_flags);
    const remote = await fetcher(snapshot);

    for (const [key, enabled] of Object.entries(remote)) {
      _flags.set(key.toLowerCase(), enabled);
    }

    _remoteLoaded = true;
    console.info("[flags] remote flags merged:", Object.keys(remote).join(", ") || "(none)");
  } catch (e) {
    console.warn("[flags] remote flag load failed — env flags remain active:", e);
  }
}

// ─── Public API ───────────────────────────────────────────────────────────────

/**
 * Returns true if the named flag is enabled, false otherwise.
 * Flag lookup is case-insensitive. Never throws.
 */
export function flag(name: string): boolean {
  return _flags.get(name.toLowerCase()) ?? false;
}

/**
 * Returns a snapshot of all active flags as a plain object.
 * Useful for debug logging or exposing to dev tools.
 */
export function allFlags(): Readonly<Record<string, boolean>> {
  return Object.fromEntries(_flags);
}

/**
 * Override a flag at runtime (dev/test use only).
 * Does not persist — reset on reload.
 */
export function _devSetFlag(name: string, enabled: boolean): void {
  if (import.meta.env.PROD) {
    console.warn("[flags] _devSetFlag is a no-op in production builds.");
    return;
  }
  _flags.set(name.toLowerCase(), enabled);
}
