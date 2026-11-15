// Shared types for the Dev Servers plugin, mirroring the Rust types in
// src-tauri/src/plugins/dev_servers/types.rs.

/**
 * How a server is launched. `"command"` runs `script` as a whole command
 * line through the user's shell (e.g. `make serve`) instead of a
 * `package.json` script.
 */
export type PackageManager = "npm" | "pnpm" | "yarn" | "command";

export const PACKAGE_MANAGERS: PackageManager[] = ["npm", "pnpm", "yarn", "command"];

/** The name shown for a launch choice in the form's dropdown. */
export function packageManagerLabel(pm: PackageManager): string {
  return pm === "command" ? "Custom command" : pm;
}

/** The command line a server runs, as the user would type it. */
export function describeServerCommand(server: { package_manager: PackageManager; script: string }): string {
  return server.package_manager === "command" ? server.script : `${server.package_manager} run ${server.script}`;
}

export interface ServerConfig {
  id: string;
  name: string;
  package_manager: PackageManager;
  script: string;
  subdirectory: string;
  port?: number | null;
  created_at: string;
}

export interface DevServerStatus {
  id: string;
  project: string;
  /** The project's `local_key`, when it is registered and has one. */
  local_key?: string;
  name: string;
  package_manager: PackageManager;
  script: string;
  subdirectory: string;
  port?: number | null;
  running: boolean;
  pid?: number | null;
  started_at?: string | null;
  exit_code?: number | null;
  /** URLs detected in the server's own output, in first-seen order. */
  urls?: string[];
  /**
   * Most recent output line that looks like a crash. Present while a
   * watch-mode supervisor keeps the tree alive after the real server died,
   * so `running` alone would misreport it. Cleared when a URL is printed.
   */
  last_error?: string | null;
}

/** Strips the protocol and any trailing slash for compact display. */
export function formatServerUrlLabel(url: string): string {
  return url.replace(/^https?:\/\//, "").replace(/\/$/, "");
}

export interface NpmScriptEntry {
  name: string;
  command: string;
}

export type LogStream = "stdout" | "stderr";

export interface LogLine {
  stream: LogStream;
  text: string;
}
