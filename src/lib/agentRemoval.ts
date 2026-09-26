/**
 * Types and parsing for the agent-removal preview returned by the
 * `get_agent_cleanup_preview` and `remove_agent_from_project` commands.
 * Mirrors `RemovalEntry` in `src-tauri/src/agent/removal.rs`.
 */

/** "remove" deletes the agent's files; "keep" only stops syncing it. */
export type AgentRemovalMode = "remove" | "keep";

export type AgentRemovalAction = "delete" | "remove_empty_dir" | "strip" | "keep_shared";

export interface AgentRemovalEntry {
  path: string;
  relative_path: string;
  action: AgentRemovalAction;
  is_dir: boolean;
  /** Labels of remaining agents that still use the path (keep_shared only). */
  shared_with: string[];
}

const ACTIONS: readonly AgentRemovalAction[] = ["delete", "remove_empty_dir", "strip", "keep_shared"];

function isAction(value: unknown): value is AgentRemovalAction {
  return typeof value === "string" && (ACTIONS as readonly string[]).includes(value);
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string");
}

function parseEntry(value: unknown, index: number): AgentRemovalEntry {
  if (typeof value !== "object" || value === null) {
    throw new Error(`Removal entry ${index} is not an object`);
  }
  const record = value as Record<string, unknown>;
  const { path, relative_path, action, is_dir, shared_with } = record;
  if (typeof path !== "string" || typeof relative_path !== "string") {
    throw new Error(`Removal entry ${index} has no path`);
  }
  if (!isAction(action)) {
    throw new Error(`Removal entry ${index} has unknown action ${String(action)}`);
  }
  if (typeof is_dir !== "boolean" || !isStringArray(shared_with)) {
    throw new Error(`Removal entry ${index} is malformed`);
  }
  return { path, relative_path, action, is_dir, shared_with };
}

/** Parse the JSON string a removal command returns. Throws on bad input. */
export function parseAgentRemovalEntries(raw: string): AgentRemovalEntry[] {
  const value: unknown = JSON.parse(raw);
  if (!Array.isArray(value)) {
    throw new Error("Removal preview is not a list");
  }
  return value.map(parseEntry);
}

export interface AgentRemovalGroups {
  deleted: AgentRemovalEntry[];
  stripped: AgentRemovalEntry[];
  kept: AgentRemovalEntry[];
}

/** Split a preview into what Remove deletes, strips, and leaves for other agents. */
export function groupAgentRemovalEntries(entries: AgentRemovalEntry[]): AgentRemovalGroups {
  return {
    deleted: entries.filter((e) => e.action === "delete" || e.action === "remove_empty_dir"),
    stripped: entries.filter((e) => e.action === "strip"),
    kept: entries.filter((e) => e.action === "keep_shared"),
  };
}
