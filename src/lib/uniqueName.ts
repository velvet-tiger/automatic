/**
 * Pick a machine name that does not collide with any existing name.
 *
 * Matching is case-insensitive: the registry collapses case variants
 * (see `dedupe_ignore_ascii_case` in `core/projects.rs`) and the default
 * macOS filesystem is case-insensitive, so `Linear` and `linear` would land
 * on the same config file.
 *
 * Without `startAt`, `base` itself is returned when free, then `base-2`,
 * `base-3`, and so on. With `startAt`, the bare `base` is never returned and
 * suffixing begins at that number. That second form is for "add another
 * copy" flows where `base` is already taken by definition.
 */
export function nextAvailableName(
  base: string,
  existing: Iterable<string>,
  options: { startAt?: number } = {},
): string {
  const taken = new Set<string>();
  for (const name of existing) taken.add(name.toLowerCase());
  const isFree = (candidate: string) => !taken.has(candidate.toLowerCase());

  let suffix: number;
  if (options.startAt === undefined) {
    if (isFree(base)) return base;
    suffix = 2;
  } else {
    suffix = options.startAt;
  }

  // Terminates because `taken` is finite.
  for (;; suffix++) {
    const candidate = `${base}-${suffix}`;
    if (isFree(candidate)) return candidate;
  }
}
