// Conflicts between a remote source's resources and the library, as
// `check_source_conflicts` reports them. The install dialog uses these to
// decide what starts selected and which replacements the user confirmed.

/** An existing library entry that installing a resource would replace. */
export interface SourceConflict {
  /** Singular type: "skill", "mcp_server", "rule", "template", "command", "agent". */
  resource_type: string;
  resource_name: string;
  /** "local", "source:{owner/repo}", or a protected owner such as "built-in". */
  existing_source: string;
  /** The existing entry's own name, which can differ in letter case. */
  existing_name: string;
  /** True when the entry can never be replaced. */
  blocked: boolean;
  /** Why the install conflicts, for display. */
  reason: string;
}

/** The plural keys the dialog groups resources under. */
export type ResourceGroupKey =
  | "skills"
  | "mcp_servers"
  | "rules"
  | "templates"
  | "commands"
  | "agents";

const SINGULAR_TYPE: Record<ResourceGroupKey, string> = {
  skills: "skill",
  mcp_servers: "mcp_server",
  rules: "rule",
  templates: "template",
  commands: "command",
  agents: "agent",
};

/** The `type:name` key the install command takes to confirm a replacement. */
export function conflictKey(group: ResourceGroupKey, name: string): string {
  return `${SINGULAR_TYPE[group]}:${name}`;
}

/** Index conflicts by their `type:name` key. */
export function indexConflicts(conflicts: SourceConflict[]): Map<string, SourceConflict> {
  return new Map(conflicts.map((c) => [`${c.resource_type}:${c.resource_name}`, c]));
}

interface SelectableGroup {
  key: ResourceGroupKey;
  items: Array<{ name: string }>;
}

/**
 * The selection the dialog starts with: every resource that replaces
 * nothing. A resource with a conflict starts unselected, so replacing an
 * existing entry always takes a deliberate click.
 */
export function initialSelection(
  groups: SelectableGroup[],
  conflicts: Map<string, SourceConflict>,
): Record<string, Set<string>> {
  const selection: Record<string, Set<string>> = {};
  for (const group of groups) {
    selection[group.key] = new Set(
      group.items
        .filter((item) => !conflicts.has(conflictKey(group.key, item.name)))
        .map((item) => item.name),
    );
  }
  return selection;
}

/** Whether the user may select this resource at all. */
export function isSelectable(
  group: ResourceGroupKey,
  name: string,
  conflicts: Map<string, SourceConflict>,
): boolean {
  return !conflicts.get(conflictKey(group, name))?.blocked;
}

/**
 * The overrides to send with an install: the key of each selected resource
 * whose conflict the user may confirm. Selecting it is the confirmation.
 */
export function overridesFor(
  selection: Record<string, Set<string>>,
  conflicts: Map<string, SourceConflict>,
): string[] {
  const overrides: string[] = [];
  for (const [group, names] of Object.entries(selection)) {
    for (const name of names) {
      const key = conflictKey(group as ResourceGroupKey, name);
      const conflict = conflicts.get(key);
      if (conflict && !conflict.blocked) {
        overrides.push(key);
      }
    }
  }
  return overrides.sort();
}

/** Drop every resource from the selection that has a conflict. */
export function withoutConflicts(
  selection: Record<string, Set<string>>,
  conflicts: Map<string, SourceConflict>,
): Record<string, Set<string>> {
  const next: Record<string, Set<string>> = {};
  for (const [group, names] of Object.entries(selection)) {
    next[group] = new Set(
      [...names].filter((name) => !conflicts.has(conflictKey(group as ResourceGroupKey, name))),
    );
  }
  return next;
}

/** One line telling the user what installing this resource would do. */
export function describeConflict(conflict: SourceConflict): string {
  if (conflict.blocked) {
    return `Cannot install: ${conflict.reason}`;
  }
  if (conflict.existing_source.startsWith("source:")) {
    const owner = conflict.existing_source.slice("source:".length);
    return `Replaces the version installed from ${owner}`;
  }
  return "Replaces your own version";
}
