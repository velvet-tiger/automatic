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
    /// A memory key was stored or updated.
    MemoryStored,
    /// A single memory key was deleted.
    MemoryDeleted,
    /// Memory entries were bulk-cleared (all or by pattern).
    MemoryCleared,
    /// A new feature was created in the project.
    FeatureCreated,
    /// A feature's metadata was updated.
    FeatureUpdated,
    /// A feature's state was changed (e.g. todo → in_progress).
    FeatureStateChanged,
    /// A feature was permanently deleted.
    FeatureDeleted,
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
            Self::MemoryStored => "memory_stored",
            Self::MemoryDeleted => "memory_deleted",
            Self::MemoryCleared => "memory_cleared",
            Self::FeatureCreated => "feature_created",
            Self::FeatureUpdated => "feature_updated",
            Self::FeatureStateChanged => "feature_state_changed",
            Self::FeatureDeleted => "feature_deleted",
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
    open_conn_at(&path)
}

fn open_conn_at(path: &PathBuf) -> Result<Connection, String> {
    let conn = Connection::open(path).map_err(|e| format!("Failed to open activity DB: {}", e))?;

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

// ── Path-injectable helpers used by tests ─────────────────────────────────────

#[cfg(test)]
/// Insert one activity row directly into `conn`, with no project-metadata
/// side-effect.  Returns the inserted timestamp.
fn insert_into(
    conn: &Connection,
    project: &str,
    event: ActivityEvent,
    label: &str,
    detail: &str,
) -> Result<String, String> {
    let ts = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO activity (project, event, label, detail, timestamp)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![project, event.as_str(), label, detail, ts],
    )
    .map_err(|e| format!("insert error: {}", e))?;
    Ok(ts)
}

#[cfg(test)]
/// Read the N most-recent rows for `project` from an open `conn`.
fn read_from(conn: &Connection, project: &str, limit: usize) -> Result<Vec<ActivityEntry>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, project, event, label, detail, timestamp
             FROM activity
             WHERE project = ?1
             ORDER BY id DESC
             LIMIT ?2",
        )
        .map_err(|e| format!("prepare error: {}", e))?;

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
        .map_err(|e| format!("query error: {}", e))?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(row.map_err(|e| format!("row error: {}", e))?);
    }
    Ok(entries)
}

#[cfg(test)]
/// Count all rows for `project` in `conn`.
fn count_from(conn: &Connection, project: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM activity WHERE project = ?1",
        params![project],
        |row| row.get(0),
    )
    .unwrap_or(0)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn fresh_conn(dir: &std::path::Path) -> Connection {
        let path = dir.join("activity.db").to_path_buf();
        open_conn_at(&path).unwrap()
    }

    // ── basic insert / read ──────────────────────────────────────────────────

    #[test]
    fn log_and_read_single_entry() {
        let dir = tempdir().unwrap();
        let conn = fresh_conn(dir.path());

        insert_into(
            &conn,
            "proj",
            ActivityEvent::MemoryStored,
            "Memory stored: k1",
            "k1",
        )
        .unwrap();

        let entries = read_from(&conn, "proj", 10).unwrap();
        assert_eq!(entries.len(), 1);

        let e = &entries[0];
        assert_eq!(e.project, "proj");
        assert_eq!(e.event, "memory_stored");
        assert_eq!(e.label, "Memory stored: k1");
        assert_eq!(e.detail, "k1");
        assert!(!e.timestamp.is_empty());
    }

    #[test]
    fn log_multiple_events_newest_first() {
        let dir = tempdir().unwrap();
        let conn = fresh_conn(dir.path());

        insert_into(&conn, "proj", ActivityEvent::MemoryStored, "a", "").unwrap();
        insert_into(&conn, "proj", ActivityEvent::MemoryDeleted, "b", "").unwrap();
        insert_into(&conn, "proj", ActivityEvent::MemoryCleared, "c", "").unwrap();

        let entries = read_from(&conn, "proj", 10).unwrap();
        assert_eq!(entries.len(), 3);
        // Newest first — "memory_cleared" was inserted last.
        assert_eq!(entries[0].event, "memory_cleared");
        assert_eq!(entries[1].event, "memory_deleted");
        assert_eq!(entries[2].event, "memory_stored");
    }

    // ── event kind strings ───────────────────────────────────────────────────

    #[test]
    fn memory_event_strings_are_correct() {
        assert_eq!(ActivityEvent::MemoryStored.as_str(), "memory_stored");
        assert_eq!(ActivityEvent::MemoryDeleted.as_str(), "memory_deleted");
        assert_eq!(ActivityEvent::MemoryCleared.as_str(), "memory_cleared");
    }

    #[test]
    fn all_event_kind_strings_are_unique() {
        let all = [
            ActivityEvent::ProjectSynced,
            ActivityEvent::SkillAdded,
            ActivityEvent::SkillRemoved,
            ActivityEvent::McpServerAdded,
            ActivityEvent::McpServerRemoved,
            ActivityEvent::AgentAdded,
            ActivityEvent::AgentRemoved,
            ActivityEvent::ProjectCreated,
            ActivityEvent::ProjectUpdated,
            ActivityEvent::MemoryStored,
            ActivityEvent::MemoryDeleted,
            ActivityEvent::MemoryCleared,
            ActivityEvent::FeatureCreated,
            ActivityEvent::FeatureUpdated,
            ActivityEvent::FeatureStateChanged,
            ActivityEvent::FeatureDeleted,
        ];
        let strings: Vec<&str> = all.iter().map(|e| e.as_str()).collect();
        let unique: std::collections::HashSet<&str> = strings.iter().copied().collect();
        assert_eq!(
            strings.len(),
            unique.len(),
            "duplicate event kind strings found"
        );
    }

    // ── project isolation ────────────────────────────────────────────────────

    #[test]
    fn entries_are_scoped_to_project() {
        let dir = tempdir().unwrap();
        let conn = fresh_conn(dir.path());

        insert_into(&conn, "project-a", ActivityEvent::MemoryStored, "a", "").unwrap();
        insert_into(&conn, "project-b", ActivityEvent::MemoryDeleted, "b", "").unwrap();

        let a = read_from(&conn, "project-a", 10).unwrap();
        let b = read_from(&conn, "project-b", 10).unwrap();

        assert_eq!(a.len(), 1);
        assert_eq!(a[0].project, "project-a");
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].project, "project-b");
    }

    // ── limit ────────────────────────────────────────────────────────────────

    #[test]
    fn read_respects_limit() {
        let dir = tempdir().unwrap();
        let conn = fresh_conn(dir.path());

        for i in 0..10 {
            insert_into(
                &conn,
                "proj",
                ActivityEvent::MemoryStored,
                &format!("label {}", i),
                "",
            )
            .unwrap();
        }

        let entries = read_from(&conn, "proj", 3).unwrap();
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn count_returns_correct_total() {
        let dir = tempdir().unwrap();
        let conn = fresh_conn(dir.path());

        for _ in 0..5 {
            insert_into(&conn, "proj", ActivityEvent::MemoryStored, "x", "").unwrap();
        }

        assert_eq!(count_from(&conn, "proj"), 5);
    }

    // ── detail field ────────────────────────────────────────────────────────

    #[test]
    fn detail_field_is_stored_and_retrieved() {
        let dir = tempdir().unwrap();
        let conn = fresh_conn(dir.path());

        insert_into(
            &conn,
            "proj",
            ActivityEvent::MemoryCleared,
            "Memory cleared",
            "42 entries matching 'foo'",
        )
        .unwrap();

        let entries = read_from(&conn, "proj", 1).unwrap();
        assert_eq!(entries[0].detail, "42 entries matching 'foo'");
    }

    #[test]
    fn empty_detail_is_allowed() {
        let dir = tempdir().unwrap();
        let conn = fresh_conn(dir.path());

        insert_into(
            &conn,
            "proj",
            ActivityEvent::MemoryDeleted,
            "Memory deleted: k",
            "",
        )
        .unwrap();

        let entries = read_from(&conn, "proj", 1).unwrap();
        assert_eq!(entries[0].detail, "");
    }
}
