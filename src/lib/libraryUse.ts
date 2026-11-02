/** Mirrors `core::LibraryItemKind`. */
import { invoke } from "@tauri-apps/api/core";

export type LibraryItemKind =
  | "mcp_server"
  | "hook"
  | "skill"
  | "sub_agent"
  | "command"
  | "rule"
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
  | { kind: "Unreadable" }
  | { kind: "NotChecked" };

/** Mirrors `core::ImportedProject`. */
export interface ImportedProject {
  name: string;
  local_key: string;
  read_only: boolean;
}

/** Adopt the config in `directory`, in the mode the user chose. */
export function importExistingProject(directory: string, readOnly: boolean): Promise<ImportedProject> {
  return invoke<ImportedProject>("import_existing_project", { directory, readOnly });
}

const KIND_LABELS: Record<LibraryItemKind, string> = {
  mcp_server: "MCP servers",
  hook: "Hooks",
  skill: "Skills",
  sub_agent: "Sub-agents",
  command: "Commands",
  rule: "Rules",
  profile: "Profiles",
};

export function libraryKindLabel(kind: LibraryItemKind): string {
  return KIND_LABELS[kind];
}

/** Why a flagged item of this kind carries more risk. */
export function libraryFlagReason(kind: LibraryItemKind): string {
  if (kind === "hook") return "runs commands";
  if (kind === "profile") return "provides these items, or could not be read";
  return "can hold a stored sign-in or stored values";
}

/**
 * Whether read-only mode is the starting choice. Normal mode is offered first
 * only for a config that was fully checked and does not ask for read-only. A
 * failed check cannot read the committed hint, so it lands here as well.
 */
export function startsReadOnly(use: ProjectLibraryUse | null | undefined): boolean {
  if (use?.kind !== "Readable") return true;
  return use.read_only_hint || use.groups.some((g) => g.not_checked > 0);
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
    servers > 0 ? `${plural(servers, "MCP server")} that can hold a stored sign-in or stored values` : null,
    hooks > 0 ? plural(hooks, "hook") : null,
  ].filter((part): part is string => part !== null);

  let line = `Uses ${plural(total, "item")} from your library`;
  if (risky.length > 0) line += `, including ${risky.join(" and ")}`;
  line += ".";
  if (unchecked > 0) line += ` ${plural(unchecked, "more name")} not checked.`;
  return line;
}
