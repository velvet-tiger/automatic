// Shared types for the Dev Servers plugin, mirroring the Rust types in
// src-tauri/src/plugins/dev_servers/types.rs.

export type PackageManager = "npm" | "pnpm" | "yarn";

export const PACKAGE_MANAGERS: PackageManager[] = ["npm", "pnpm", "yarn"];

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
