/**
 * One-time move of project-scoped localStorage entries from project names to
 * project `local_key`s (stage 3b step 3 of the project identity plan).
 *
 * Before this step the frontend stored:
 *   - `automatic.projects.selected`      → a project name
 *   - `automatic.projects.order`         → a JSON array of project names
 *   - `automatic.projects.<name>.build.{view,filters,sort,columnWidths}`
 *
 * After it, every one of these holds or embeds a `local_key`. A rename no
 * longer touches them, because the key does not change.
 *
 * The migration is idempotent: a value that is already a key is left alone,
 * and an unknown value (a deleted project) is kept as it is. It records a
 * version flag only when every project has a key, so a project whose key is
 * minted later is still migrated on a later load.
 */
import type { ProjectSummary } from "../pages/workspace/projects/types";

export const PROJECT_STORAGE_VERSION_KEY = "automatic.projects.storageVersion";
/** Bump when a later step changes the stored shape again. */
export const PROJECT_STORAGE_VERSION = "2";

export const SELECTED_PROJECT_STORAGE_KEY = "automatic.projects.selected";
export const PROJECT_ORDER_STORAGE_KEY = "automatic.projects.order";

const BUILD_KEY_PATTERN = /^automatic\.projects\.(.+)\.build\.(view|filters|sort|columnWidths)$/;

/** The part of the DOM `Storage` interface the migration needs. */
export type ProjectStorage = Pick<Storage, "length" | "key" | "getItem" | "setItem" | "removeItem">;

/**
 * Map of project name → `local_key`, for projects that have a key. A name
 * shared by two projects is left out: it cannot be moved to one key safely.
 */
export function nameToKeyMap(summaries: readonly ProjectSummary[]): Map<string, string> {
  const map = new Map<string, string>();
  const ambiguous = new Set<string>();
  for (const s of summaries) {
    if (s.local_key === "") continue;
    if (map.has(s.name)) ambiguous.add(s.name);
    map.set(s.name, s.local_key);
  }
  for (const name of ambiguous) map.delete(name);
  return map;
}

/** Replace a stored name with its key. Keys and unknown values pass through. */
export function migrateIdentifier(
  value: string,
  keys: ReadonlySet<string>,
  nameToKey: ReadonlyMap<string, string>,
): string {
  if (keys.has(value)) return value;
  return nameToKey.get(value) ?? value;
}

/** Migrate the stored project order. Duplicates after mapping are dropped. */
export function migrateOrder(
  order: readonly string[],
  keys: ReadonlySet<string>,
  nameToKey: ReadonlyMap<string, string>,
): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  for (const entry of order) {
    const mapped = migrateIdentifier(entry, keys, nameToKey);
    if (seen.has(mapped)) continue;
    seen.add(mapped);
    out.push(mapped);
  }
  return out;
}

/**
 * Plan the renames of `automatic.projects.<name>.build.*` storage keys.
 * Returns `[oldStorageKey, newStorageKey]` pairs. Pure.
 */
export function planBuildKeyMoves(
  storageKeys: readonly string[],
  keys: ReadonlySet<string>,
  nameToKey: ReadonlyMap<string, string>,
): Array<[string, string]> {
  const moves: Array<[string, string]> = [];
  for (const storageKey of storageKeys) {
    const match = BUILD_KEY_PATTERN.exec(storageKey);
    if (!match) continue;
    const identifier = match[1]!;
    const suffix = match[2]!;
    if (keys.has(identifier)) continue;
    const key = nameToKey.get(identifier);
    if (key === undefined) continue;
    moves.push([storageKey, `automatic.projects.${key}.build.${suffix}`]);
  }
  return moves;
}

function listStorageKeys(storage: ProjectStorage): string[] {
  const out: string[] = [];
  for (let i = 0; i < storage.length; i++) {
    const k = storage.key(i);
    if (k !== null) out.push(k);
  }
  return out;
}

/**
 * Apply the migration to `storage`. Safe to call on every summaries load:
 * it returns at once when the version flag is already set.
 *
 * Returns true when the flag is set (fully migrated), false when some
 * project had no key yet and a later load should run it again.
 */
export function migrateProjectStorageOnce(
  storage: ProjectStorage,
  summaries: readonly ProjectSummary[],
): boolean {
  if (storage.getItem(PROJECT_STORAGE_VERSION_KEY) === PROJECT_STORAGE_VERSION) return true;

  const keys = new Set(summaries.filter((s) => s.local_key !== "").map((s) => s.local_key));
  const nameToKey = nameToKeyMap(summaries);

  const selected = storage.getItem(SELECTED_PROJECT_STORAGE_KEY);
  if (selected !== null) {
    const mapped = migrateIdentifier(selected, keys, nameToKey);
    if (mapped !== selected) storage.setItem(SELECTED_PROJECT_STORAGE_KEY, mapped);
  }

  const rawOrder = storage.getItem(PROJECT_ORDER_STORAGE_KEY);
  if (rawOrder !== null) {
    let parsed: unknown = null;
    try {
      parsed = JSON.parse(rawOrder);
    } catch {
      // A corrupt order is dropped below; the list falls back to A–Z.
    }
    if (Array.isArray(parsed) && parsed.every((v): v is string => typeof v === "string")) {
      storage.setItem(PROJECT_ORDER_STORAGE_KEY, JSON.stringify(migrateOrder(parsed, keys, nameToKey)));
    } else {
      storage.removeItem(PROJECT_ORDER_STORAGE_KEY);
    }
  }

  for (const [oldKey, newKey] of planBuildKeyMoves(listStorageKeys(storage), keys, nameToKey)) {
    const value = storage.getItem(oldKey);
    // A value already stored under the key is newer than the name's copy.
    if (value !== null && storage.getItem(newKey) === null) storage.setItem(newKey, value);
    storage.removeItem(oldKey);
  }

  const complete = summaries.every((s) => s.local_key !== "");
  if (complete) storage.setItem(PROJECT_STORAGE_VERSION_KEY, PROJECT_STORAGE_VERSION);
  return complete;
}
