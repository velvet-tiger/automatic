/** Sidebar drop target id for the "Other Projects" section. */
export const UNGROUPED_DROP_TARGET = "__ungrouped__";

/** What a sidebar project drag should do to group membership. */
export type GroupDropAction =
  | { kind: "none" }
  | { kind: "remove-all" }
  | { kind: "add"; to: string }
  | { kind: "move"; from: string; to: string };

/**
 * Decide the membership change for a project dropped in the workspace sidebar.
 *
 * A project may belong to several groups. A plain drag between groups moves
 * the project, as it always has. Holding the add modifier (Option on macOS,
 * Alt elsewhere) keeps the source membership and adds the target group too.
 * A drag that starts in "Other Projects" always adds, since there is no
 * source group to leave. Dropping on "Other Projects" removes the project
 * from every group.
 */
export function resolveGroupDrop(
  sourceGroup: string | null,
  targetGroup: string | null,
  addModifier: boolean,
): GroupDropAction {
  if (targetGroup === null || targetGroup === sourceGroup) return { kind: "none" };
  if (targetGroup === UNGROUPED_DROP_TARGET) {
    return sourceGroup === null ? { kind: "none" } : { kind: "remove-all" };
  }
  if (sourceGroup === null || addModifier) return { kind: "add", to: targetGroup };
  return { kind: "move", from: sourceGroup, to: targetGroup };
}
