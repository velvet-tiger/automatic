/** Mirrors `core::LibraryItemKind`. */
export type LibraryItemKind =
  | "mcp_server"
  | "hook"
  | "skill"
  | "sub_agent"
  | "command"
  | "rule"
  | "provider"
  | "profile";

/** Mirrors `core::LibraryUseGroup`. */
export interface LibraryUseGroup {
  kind: LibraryItemKind;
  total: number;
  flagged_total: number;
  names: { name: string; flagged: boolean }[];
  not_checked: number;
}

/** Mirrors `core::ProjectLibraryUse`. */
export type ProjectLibraryUse =
  | { kind: "Readable"; read_only_hint: boolean; groups: LibraryUseGroup[] }
  | { kind: "Unreadable" };

const KIND_LABELS: Record<LibraryItemKind, string> = {
  mcp_server: "MCP servers",
  hook: "Hooks",
  skill: "Skills",
  sub_agent: "Sub-agents",
  command: "Commands",
  rule: "Rules",
  provider: "Providers",
  profile: "Profiles",
};

export function libraryKindLabel(kind: LibraryItemKind): string {
  return KIND_LABELS[kind];
}

/** Why a flagged item of this kind carries more risk. */
export function libraryFlagReason(kind: LibraryItemKind): string {
  return kind === "hook" ? "runs commands" : "has stored values";
}

function plural(count: number, one: string): string {
  return `${count} ${one}${count === 1 ? "" : "s"}`;
}

/** One line for a place too small for the full list. Null when nothing matches. */
export function libraryUseSummary(groups: LibraryUseGroup[]): string | null {
  const total = groups.reduce((sum, g) => sum + g.total, 0);
  const unchecked = groups.reduce((sum, g) => sum + g.not_checked, 0);
  if (total === 0 && unchecked === 0) return null;

  const servers = groups.find((g) => g.kind === "mcp_server")?.flagged_total ?? 0;
  const hooks = groups.find((g) => g.kind === "hook")?.flagged_total ?? 0;
  const risky = [
    servers > 0 ? `${plural(servers, "MCP server")} with stored values` : null,
    hooks > 0 ? plural(hooks, "hook") : null,
  ].filter((part): part is string => part !== null);

  let line = `Uses ${plural(total, "item")} from your library`;
  if (risky.length > 0) line += `, including ${risky.join(" and ")}`;
  line += ".";
  if (unchecked > 0) line += ` ${plural(unchecked, "more name")} not checked.`;
  return line;
}
