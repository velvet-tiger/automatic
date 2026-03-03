//! Activity log: a SQLite-backed append-only record of events across all projects.
//!
//! Schema:
//!   activity (
//!     id        INTEGER PRIMARY KEY AUTOINCREMENT,
//!     project   TEXT    NOT NULL,
//!     event     TEXT    NOT NULL,   -- machine-readable kind: "sync", "skill_added", etc.
//!     label     TEXT    NOT NULL,   -- human-readable one-liner
//!     detail    TEXT    NOT NULL,   -- secondary info (skill name, count, etc.)
//!     timestamp TEXT    NOT NULL    -- ISO 8601 UTC
//!   )
//!
//! The DB file lives at `~/.automatic/activity.db`.  The table is created on
//! first use (idempotent, "IF NOT EXISTS").  Reads return up to `limit` rows
//! ordered newest-first.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── Types ─────────────────────────────────────────────────────────────────────

/// A single activity row returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEntry {
    pub id: i64,
    pub project: String,
    pub event: String,
    pub label: String,
    pub detail: String,
    pub timestamp: String,
}

/// Machine-readable event kinds.  New variants can be added freely; the DB
/// stores the string so old rows are never invalidated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityEvent {
    /// Project agent configs were written to disk (sync or save).
    ProjectSynced,
    /// A global skill was added to the project.
    SkillAdded,
    /// A global skill was removed from the project.
    SkillRemoved,
    /// An MCP server was added to the project.
    McpServerAdded,
    /// An MCP server was removed from the project.
    McpServerRemoved,
    /// An agent tool was added to the project.
    AgentAdded,
    /// An agent tool was removed from the project.
    AgentRemoved,
    /// The project was created for the first time.
    ProjectCreated,
    /// Project description or settings were updated.
    ProjectUpdated,
}

impl ActivityEvent {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProjectSynced => "sync",
            Self::SkillAdded => "skill_added",
            Self::SkillRemoved => "skill_removed",
            Self::McpServerAdded => "mcp_server_added",
            Self::McpServerRemoved => "mcp_server_removed",
            Self::AgentAdded => "agent_added",
            Self::AgentRemoved => "agent_removed",
            Self::ProjectCreated => "project_created",
            Self::ProjectUpdated => "project_updated",
        }
    }
}

// ── DB path ───────────────────────────────────────────────────────────────────

fn get_db_path() -> Result<PathBuf, String> {
    let dir = crate::core::get_automatic_dir()?;
    Ok(dir.join("activity.db"))
}

// ── Connection + schema ───────────────────────────────────────────────────────

fn open_conn() -> Result<Connection, String> {
    let path = get_db_path()?;
    let conn = Connection::open(&path).map_err(|e| format!("Failed to open activity DB: {}", e))?;

    // Enable WAL mode so concurrent reads during a write don't block.
    conn.execute_batch("PRAGMA journal_mode=WAL;")
        .map_err(|e| format!("Failed to set WAL mode: {}", e))?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS activity (
            id        INTEGER PRIMARY KEY AUTOINCREMENT,
            project   TEXT    NOT NULL,
            event     TEXT    NOT NULL,
            label     TEXT    NOT NULL,
            detail    TEXT    NOT NULL DEFAULT '',
            timestamp TEXT    NOT NULL
        );
        CREATE INDEX IF NOT EXISTS activity_project_ts
            ON activity (project, timestamp DESC);",
    )
    .map_err(|e| format!("Failed to create activity table: {}", e))?;

    Ok(conn)
}

// ── Write ─────────────────────────────────────────────────────────────────────

/// Append one event to the activity log.  Non-fatal: errors are logged to
/// stderr but never bubble up to callers so a DB write failure never blocks
/// the main operation.
pub fn log(project: &str, event: ActivityEvent, label: &str, detail: &str) {
    if let Err(e) = log_inner(project, event, label, detail) {
        eprintln!("[activity] log error: {}", e);
    }
}

/// Maximum number of entries retained per project.  Oldest rows beyond this
/// limit are pruned immediately after each insert.
const MAX_ENTRIES_PER_PROJECT: usize = 500;

fn log_inner(project: &str, event: ActivityEvent, label: &str, detail: &str) -> Result<(), String> {
    let conn = open_conn()?;
    let ts = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO activity (project, event, label, detail, timestamp)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![project, event.as_str(), label, detail, ts],
    )
    .map_err(|e| format!("Failed to insert activity: {}", e))?;

    // Purge oldest rows for this project, keeping at most MAX_ENTRIES_PER_PROJECT.
    // Uses a subquery to find the id threshold so the DELETE is index-friendly.
    conn.execute(
        "DELETE FROM activity
         WHERE project = ?1
           AND id NOT IN (
               SELECT id FROM activity
               WHERE project = ?1
               ORDER BY id DESC
               LIMIT ?2
           )",
        params![project, MAX_ENTRIES_PER_PROJECT as i64],
    )
    .map_err(|e| format!("Failed to purge old activity entries: {}", e))?;

    // Keep project's last_activity in sync with the newest activity timestamp.
    // This is best-effort metadata update and should not fail activity logging.
    if let Ok(raw) = crate::core::read_project(project) {
        if let Ok(mut parsed) = serde_json::from_str::<crate::core::Project>(&raw) {
            parsed.last_activity = Some(ts);
            if let Ok(updated) = serde_json::to_string_pretty(&parsed) {
                let _ = crate::core::save_project(project, &updated);
            }
        }
    }

    Ok(())
}

// ── Read ──────────────────────────────────────────────────────────────────────

/// Return the `limit` most-recent activity entries for `project`.
pub fn get_project_activity(project: &str, limit: usize) -> Result<Vec<ActivityEntry>, String> {
    let conn = open_conn()?;
    let mut stmt = conn
        .prepare(
            "SELECT id, project, event, label, detail, timestamp
             FROM activity
             WHERE project = ?1
             ORDER BY id DESC
             LIMIT ?2",
        )
        .map_err(|e| format!("Failed to prepare activity query: {}", e))?;

    let rows = stmt
        .query_map(params![project, limit as i64], |row| {
            Ok(ActivityEntry {
                id: row.get(0)?,
                project: row.get(1)?,
                event: row.get(2)?,
                label: row.get(3)?,
                detail: row.get(4)?,
                timestamp: row.get(5)?,
            })
        })
        .map_err(|e| format!("Failed to query activity: {}", e))?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(row.map_err(|e| format!("Failed to read activity row: {}", e))?);
    }
    Ok(entries)
}

/// Return a page of activity entries for `project`, ordered newest-first.
/// `offset` is the zero-based row offset (i.e. skip first `offset` rows).
pub fn get_project_activity_paged(
    project: &str,
    limit: usize,
    offset: usize,
) -> Result<Vec<ActivityEntry>, String> {
    let conn = open_conn()?;
    let mut stmt = conn
        .prepare(
            "SELECT id, project, event, label, detail, timestamp
             FROM activity
             WHERE project = ?1
             ORDER BY id DESC
             LIMIT ?2 OFFSET ?3",
        )
        .map_err(|e| format!("Failed to prepare paged activity query: {}", e))?;

    let rows = stmt
        .query_map(params![project, limit as i64, offset as i64], |row| {
            Ok(ActivityEntry {
                id: row.get(0)?,
                project: row.get(1)?,
                event: row.get(2)?,
                label: row.get(3)?,
                detail: row.get(4)?,
                timestamp: row.get(5)?,
            })
        })
        .map_err(|e| format!("Failed to query paged activity: {}", e))?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(row.map_err(|e| format!("Failed to read paged activity row: {}", e))?);
    }
    Ok(entries)
}

/// Return the total number of activity entries for `project`.
pub fn get_project_activity_count(project: &str) -> Result<i64, String> {
    let conn = open_conn()?;
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM activity WHERE project = ?1",
            params![project],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to count activity rows: {}", e))?;
    Ok(count)
}

/// Return the `limit` most-recent activity entries across ALL projects,
/// ordered newest-first.  Used by the global Dashboard.
pub fn get_all_activity(limit: usize) -> Result<Vec<ActivityEntry>, String> {
    let conn = open_conn()?;
    let mut stmt = conn
        .prepare(
            "SELECT id, project, event, label, detail, timestamp
             FROM activity
             ORDER BY id DESC
             LIMIT ?1",
        )
        .map_err(|e| format!("Failed to prepare global activity query: {}", e))?;

    let rows = stmt
        .query_map(params![limit as i64], |row| {
            Ok(ActivityEntry {
                id: row.get(0)?,
                project: row.get(1)?,
                event: row.get(2)?,
                label: row.get(3)?,
                detail: row.get(4)?,
                timestamp: row.get(5)?,
            })
        })
        .map_err(|e| format!("Failed to query global activity: {}", e))?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(row.map_err(|e| format!("Failed to read activity row: {}", e))?);
    }
    Ok(entries)
}
