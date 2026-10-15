/**
 * Migrations for the tab and section ids the app saves in localStorage.
 *
 * Saved ids outlive the UI that wrote them: tabs and sections get renamed or
 * removed between releases. Restoring an id the current build does not render
 * leaves the content area blank, so every saved value is mapped to one that
 * exists before it is used.
 */

/** Tab every unknown or removed saved tab falls back to. */
export const FALLBACK_TAB = "projects";

/** Section every unknown or removed saved section falls back to. */
export const FALLBACK_SECTION = "workspace";

/** Removed tabs and the tab that replaced each one. */
const RENAMED_TABS: ReadonlyMap<string, string> = new Map([
  ["activity", FALLBACK_TAB],
  ["configuration", FALLBACK_TAB],
  ["dashboard", FALLBACK_TAB],
  // The Start section and its Getting Started page were removed.
  ["getting-started", FALLBACK_TAB],
  ["support", "settings"],
]);

/** Removed sections and the section that replaced each one. */
const RENAMED_SECTIONS: ReadonlyMap<string, string> = new Map([
  ["marketplace", "discover"],
  ["community", "discover"],
  ["start", FALLBACK_SECTION],
]);

/**
 * Map a saved tab id to one the current build renders.
 *
 * `knownTabs` is every tab id the app can show. A null, empty or unknown id
 * returns `FALLBACK_TAB`.
 */
export function migrateSavedTab(saved: string | null, knownTabs: ReadonlySet<string>): string {
  if (!saved) return FALLBACK_TAB;
  const migrated = RENAMED_TABS.get(saved) ?? saved;
  return knownTabs.has(migrated) ? migrated : FALLBACK_TAB;
}

/**
 * Map a saved section id to one the current build has.
 *
 * `knownSections` is every section id. A null, empty or unknown id returns
 * `FALLBACK_SECTION`.
 */
export function migrateSavedSection<S extends string>(saved: string | null, knownSections: readonly S[]): S {
  const migrated = saved ? RENAMED_SECTIONS.get(saved) ?? saved : FALLBACK_SECTION;
  const match = knownSections.find((s) => s === migrated);
  if (match) return match;
  const fallback = knownSections.find((s) => s === FALLBACK_SECTION);
  if (!fallback) {
    throw new Error(`migrateSavedSection: "${FALLBACK_SECTION}" is not among the known sections`);
  }
  return fallback;
}
