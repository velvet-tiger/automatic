// Shared types for the Maildev plugin, mirroring the Rust types in
// src-tauri/src/plugins/maildev/types.rs.

export interface MaildevStatus {
  running: boolean;
  pid?: number | null;
  started_at?: string | null;
  exit_code?: number | null;
  /** Captured stderr tail, set only when the process exited unexpectedly. */
  error?: string | null;
}

/** Fixed admin (web UI) URL — this plugin always runs Maildev with defaults. */
export const MAILDEV_ADMIN_URL = "http://localhost:1080";
