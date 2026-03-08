//! Recommendations: a SQLite-backed store of per-project suggestions.
//!
//! Schema:
//!   recommendations (
//!     id          INTEGER PRIMARY KEY AUTOINCREMENT,
//!     project     TEXT    NOT NULL,
//!     kind        TEXT    NOT NULL,   -- category: "skill", "mcp_server", "agent", "rule", etc.
//!     title       TEXT    NOT NULL,   -- short human-readable headline
//!     body        TEXT    NOT NULL,   -- longer description or reasoning
//!     priority    TEXT    NOT NULL DEFAULT 'normal', -- "low" | "normal" | "high"
//!     status      TEXT    NOT NULL DEFAULT 'pending', -- "pending" | "dismissed" | "actioned"
//!     source      TEXT    NOT NULL DEFAULT '', -- originating agent / system
//!     created_at  TEXT    NOT NULL,
//!     updated_at  TEXT    NOT NULL
//!   )
//!
//! The table is created on first use (idempotent, "IF NOT EXISTS").
//! The DB file is the same `~/.automatic/activity.db` used by the activity log
//! so that a single file-handle / WAL state is shared.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── Types ─────────────────────────────────────────────────────────────────────

/// Priority level for a recommendation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecommendationPriority {
    Low,
    Normal,
    High,
}

impl RecommendationPriority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "low" => Self::Low,
            "high" => Self::High,
            _ => Self::Normal,
        }
    }
}

impl Default for RecommendationPriority {
    fn default() -> Self {
        Self::Normal
    }
}

/// Lifecycle status of a recommendation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecommendationStatus {
    Pending,
    Dismissed,
    Actioned,
}

impl RecommendationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Dismissed => "dismissed",
            Self::Actioned => "actioned",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "dismissed" => Self::Dismissed,
            "actioned" => Self::Actioned,
            _ => Self::Pending,
        }
    }
}

impl Default for RecommendationStatus {
    fn default() -> Self {
        Self::Pending
    }
}

/// A single recommendation row returned to callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    pub id: i64,
    /// The project this recommendation belongs to.
    pub project: String,
    /// Category of the recommendation (e.g. "skill", "mcp_server", "agent", "rule").
    pub kind: String,
    /// Short headline shown in the UI.
    pub title: String,
    /// Detailed description or reasoning.
    pub body: String,
    /// Importance level.
    pub priority: RecommendationPriority,
    /// Lifecycle status.
    pub status: RecommendationStatus,
    /// Which agent or system generated this recommendation (empty string if unknown).
    pub source: String,
    /// Optional JSON blob for extra structured data (e.g. skill registry id and source).
    /// Empty string when not set.
    pub metadata: String,
    /// ISO 8601 UTC creation timestamp.
    pub created_at: String,
    /// ISO 8601 UTC last-updated timestamp.
    pub updated_at: String,
}

/// Parameters for adding a new recommendation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddRecommendationParams {
    pub project: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub priority: RecommendationPriority,
    #[serde(default)]
    pub source: String,
    /// Optional JSON metadata blob (e.g. `{"id":"owner/repo/skill","source":"owner/repo"}`).
    #[serde(default)]
    pub metadata: String,
}

/// Filters for listing recommendations.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListRecommendationsFilter {
    /// Only return recommendations with this status.  `None` = all statuses.
    pub status: Option<RecommendationStatus>,
    /// Only return recommendations of this kind.  `None` = all kinds.
    pub kind: Option<String>,
    /// Only return recommendations from this source.  `None` = all sources.
    pub source: Option<String>,
    /// Maximum number of rows to return.  `None` = 100 (default).
    pub limit: Option<usize>,
}

// ── DB path ───────────────────────────────────────────────────────────────────

fn get_db_path() -> Result<PathBuf, String> {
    let dir = crate::core::get_automatic_dir()?;
    Ok(dir.join("activity.db"))
}

// ── Connection + schema ───────────────────────────────────────────────────────

fn open_conn() -> Result<Connection, String> {
    let path = get_db_path()?;
    let conn =
        Connection::open(&path).map_err(|e| format!("Failed to open recommendations DB: {}", e))?;

    conn.execute_batch("PRAGMA journal_mode=WAL;")
        .map_err(|e| format!("Failed to set WAL mode: {}", e))?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS recommendations (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            project     TEXT    NOT NULL,
            kind        TEXT    NOT NULL,
            title       TEXT    NOT NULL,
            body        TEXT    NOT NULL DEFAULT '',
            priority    TEXT    NOT NULL DEFAULT 'normal',
            status      TEXT    NOT NULL DEFAULT 'pending',
            source      TEXT    NOT NULL DEFAULT '',
            metadata    TEXT    NOT NULL DEFAULT '',
            created_at  TEXT    NOT NULL,
            updated_at  TEXT    NOT NULL
        );
        CREATE INDEX IF NOT EXISTS recommendations_project_status
            ON recommendations (project, status);
        CREATE INDEX IF NOT EXISTS recommendations_project_created
            ON recommendations (project, created_at DESC);",
    )
    .map_err(|e| format!("Failed to create recommendations table: {}", e))?;

    // Migration: add `metadata` column to existing databases that predate it.
    let _ = conn
        .execute_batch("ALTER TABLE recommendations ADD COLUMN metadata TEXT NOT NULL DEFAULT '';");
    // Intentionally ignore errors — SQLite returns an error if the column already exists.

    Ok(conn)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn row_to_recommendation(row: &rusqlite::Row<'_>) -> rusqlite::Result<Recommendation> {
    Ok(Recommendation {
        id: row.get(0)?,
        project: row.get(1)?,
        kind: row.get(2)?,
        title: row.get(3)?,
        body: row.get(4)?,
        priority: RecommendationPriority::from_str(&row.get::<_, String>(5)?),
        status: RecommendationStatus::from_str(&row.get::<_, String>(6)?),
        source: row.get(7)?,
        metadata: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

// ── Write API ─────────────────────────────────────────────────────────────────

/// Add a new recommendation for the given project.  Returns the new row's id.
pub fn add_recommendation(params: AddRecommendationParams) -> Result<i64, String> {
    if !crate::core::is_valid_name(&params.project) {
        return Err("Invalid project name".into());
    }
    if params.kind.trim().is_empty() {
        return Err("Recommendation kind must not be empty".into());
    }
    if params.title.trim().is_empty() {
        return Err("Recommendation title must not be empty".into());
    }

    let conn = open_conn()?;
    let ts = now();
    conn.execute(
        "INSERT INTO recommendations (project, kind, title, body, priority, status, source, metadata, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?7, ?8, ?9)",
        params![
            params.project,
            params.kind,
            params.title.trim(),
            params.body,
            params.priority.as_str(),
            params.source,
            params.metadata,
            ts,
            ts,
        ],
    )
    .map_err(|e| format!("Failed to insert recommendation: {}", e))?;

    let id = conn.last_insert_rowid();
    Ok(id)
}

/// Update the status of a recommendation by id.
/// Valid transitions: any status → "dismissed" or "actioned".
pub fn update_recommendation_status(
    id: i64,
    new_status: RecommendationStatus,
) -> Result<(), String> {
    let conn = open_conn()?;
    let ts = now();
    let changed = conn
        .execute(
            "UPDATE recommendations SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![new_status.as_str(), ts, id],
        )
        .map_err(|e| format!("Failed to update recommendation status: {}", e))?;

    if changed == 0 {
        Err(format!("Recommendation id {} not found", id))
    } else {
        Ok(())
    }
}

/// Dismiss a recommendation (convenience wrapper).
pub fn dismiss_recommendation(id: i64) -> Result<(), String> {
    update_recommendation_status(id, RecommendationStatus::Dismissed)
}

/// Mark a recommendation as actioned (convenience wrapper).
pub fn action_recommendation(id: i64) -> Result<(), String> {
    update_recommendation_status(id, RecommendationStatus::Actioned)
}

/// Delete a single recommendation by id, regardless of status.
pub fn delete_recommendation(id: i64) -> Result<(), String> {
    let conn = open_conn()?;
    let changed = conn
        .execute("DELETE FROM recommendations WHERE id = ?1", params![id])
        .map_err(|e| format!("Failed to delete recommendation: {}", e))?;

    if changed == 0 {
        Err(format!("Recommendation id {} not found", id))
    } else {
        Ok(())
    }
}

/// Delete all recommendations for a project matching an optional status filter.
/// Returns the number of rows deleted.
pub fn clear_recommendations(
    project: &str,
    status: Option<RecommendationStatus>,
) -> Result<usize, String> {
    if !crate::core::is_valid_name(project) {
        return Err("Invalid project name".into());
    }

    let conn = open_conn()?;
    let deleted = if let Some(s) = status {
        conn.execute(
            "DELETE FROM recommendations WHERE project = ?1 AND status = ?2",
            params![project, s.as_str()],
        )
    } else {
        conn.execute(
            "DELETE FROM recommendations WHERE project = ?1",
            params![project],
        )
    }
    .map_err(|e| format!("Failed to clear recommendations: {}", e))?;

    Ok(deleted)
}

// ── Read API ──────────────────────────────────────────────────────────────────

/// Return recommendations for a project, with optional filtering.
pub fn list_recommendations(
    project: &str,
    filter: ListRecommendationsFilter,
) -> Result<Vec<Recommendation>, String> {
    if !crate::core::is_valid_name(project) {
        return Err("Invalid project name".into());
    }

    let conn = open_conn()?;
    let limit = filter.limit.unwrap_or(100) as i64;

    // Build SQL dynamically so we can handle any combination of optional filters
    // without a combinatorial explosion of match arms.
    let mut sql = String::from(
        "SELECT id, project, kind, title, body, priority, status, source, metadata, created_at, updated_at
         FROM recommendations
         WHERE project = ?1",
    );
    let mut param_idx = 2usize;
    let mut extras: Vec<String> = Vec::new();

    if filter.status.is_some() {
        extras.push(format!("status = ?{}", param_idx));
        param_idx += 1;
    }
    if filter.kind.is_some() {
        extras.push(format!("kind = ?{}", param_idx));
        param_idx += 1;
    }
    if filter.source.is_some() {
        extras.push(format!("source = ?{}", param_idx));
        param_idx += 1;
    }

    for clause in &extras {
        sql.push_str(" AND ");
        sql.push_str(clause);
    }
    sql.push_str(&format!(" ORDER BY created_at DESC LIMIT ?{}", param_idx));

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("Failed to prepare query: {}", e))?;

    // rusqlite requires all params to be bound positionally; we push each
    // optional value only when the filter is active.
    let mut raw_params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(project.to_string())];
    if let Some(s) = &filter.status {
        raw_params.push(Box::new(s.as_str().to_string()));
    }
    if let Some(k) = &filter.kind {
        raw_params.push(Box::new(k.clone()));
    }
    if let Some(src) = &filter.source {
        raw_params.push(Box::new(src.clone()));
    }
    raw_params.push(Box::new(limit));

    let refs: Vec<&dyn rusqlite::ToSql> = raw_params.iter().map(|b| b.as_ref()).collect();
    let rows = stmt
        .query_map(refs.as_slice(), row_to_recommendation)
        .map_err(|e| format!("Failed to execute query: {}", e))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| format!("Failed to read rows: {}", e))?;

    Ok(rows)
}

/// Fetch a single recommendation by id.
pub fn get_recommendation(id: i64) -> Result<Recommendation, String> {
    let conn = open_conn()?;
    conn.query_row(
        "SELECT id, project, kind, title, body, priority, status, source, metadata, created_at, updated_at
         FROM recommendations WHERE id = ?1",
        params![id],
        row_to_recommendation,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => format!("Recommendation id {} not found", id),
        other => format!("Failed to fetch recommendation: {}", other),
    })
}

// ── AI recommendation throttle ────────────────────────────────────────────────

/// Store the timestamp of the most recent AI recommendation run for a project.
/// The value is an ISO 8601 UTC string.
pub fn set_ai_recommendations_timestamp(project: &str) -> Result<(), String> {
    if !crate::core::is_valid_name(project) {
        return Err("Invalid project name".into());
    }

    let conn = open_conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS recommendation_meta (
            project     TEXT PRIMARY KEY,
            last_ai_run TEXT NOT NULL
        );",
    )
    .map_err(|e| format!("Failed to create recommendation_meta table: {}", e))?;

    let ts = now();
    conn.execute(
        "INSERT INTO recommendation_meta (project, last_ai_run)
         VALUES (?1, ?2)
         ON CONFLICT(project) DO UPDATE SET last_ai_run = excluded.last_ai_run",
        params![project, ts],
    )
    .map_err(|e| format!("Failed to upsert ai recommendations timestamp: {}", e))?;

    Ok(())
}

/// Return the ISO 8601 UTC timestamp of the last AI recommendations run for a
/// project, or `None` if it has never been run.
pub fn get_ai_recommendations_timestamp(project: &str) -> Result<Option<String>, String> {
    if !crate::core::is_valid_name(project) {
        return Err("Invalid project name".into());
    }

    let conn = open_conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS recommendation_meta (
            project     TEXT PRIMARY KEY,
            last_ai_run TEXT NOT NULL
        );",
    )
    .map_err(|e| format!("Failed to create recommendation_meta table: {}", e))?;

    let result: rusqlite::Result<String> = conn.query_row(
        "SELECT last_ai_run FROM recommendation_meta WHERE project = ?1",
        params![project],
        |row| row.get(0),
    );

    match result {
        Ok(ts) => Ok(Some(ts)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!(
            "Failed to query ai recommendations timestamp: {}",
            e
        )),
    }
}

/// Returns `true` if AI recommendations have been run for this project within
/// the last 24 hours (once-per-day throttle).
pub fn ai_recommendations_throttled(project: &str) -> Result<bool, String> {
    match get_ai_recommendations_timestamp(project)? {
        None => Ok(false),
        Some(ts) => {
            let last = chrono::DateTime::parse_from_rfc3339(&ts)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .map_err(|e| format!("Failed to parse timestamp '{}': {}", ts, e))?;
            let elapsed = chrono::Utc::now().signed_duration_since(last);
            Ok(elapsed.num_hours() < 24)
        }
    }
}

/// Return all pending recommendations across every project, ordered by
/// priority (high first) then creation time (newest first).
/// Used by the dashboard to show a global recommendations pane.
pub fn list_all_pending_recommendations(limit: usize) -> Result<Vec<Recommendation>, String> {
    let conn = open_conn()?;
    let limit = limit as i64;

    let mut stmt = conn
        .prepare(
            "SELECT id, project, kind, title, body, priority, status, source, metadata, created_at, updated_at
             FROM recommendations
             WHERE status = 'pending'
             ORDER BY
               CASE priority WHEN 'high' THEN 0 WHEN 'normal' THEN 1 ELSE 2 END ASC,
               created_at DESC
             LIMIT ?1",
        )
        .map_err(|e| format!("Failed to prepare query: {}", e))?;

    let rows = stmt
        .query_map(params![limit], row_to_recommendation)
        .map_err(|e| format!("Failed to execute query: {}", e))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| format!("Failed to read rows: {}", e))?;

    Ok(rows)
}

/// Delete pending recommendations for a project that were generated by the
/// automatic system evaluator and match the given kind.  Called when the
/// condition that triggered the recommendation has been resolved.
pub fn clear_system_recommendations_by_kind(project: &str, kind: &str) -> Result<(), String> {
    if !crate::core::is_valid_name(project) {
        return Err("Invalid project name".into());
    }

    let conn = open_conn()?;
    conn.execute(
        "DELETE FROM recommendations WHERE project = ?1 AND kind = ?2 AND source = 'automatic-system' AND status = 'pending'",
        params![project, kind],
    )
    .map_err(|e| format!("Failed to clear system recommendations: {}", e))?;

    Ok(())
}

/// Check whether a pending system-generated recommendation for the given kind
/// already exists, to avoid inserting duplicates.
pub fn has_pending_system_recommendation(project: &str, kind: &str) -> Result<bool, String> {
    if !crate::core::is_valid_name(project) {
        return Err("Invalid project name".into());
    }

    let conn = open_conn()?;
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM recommendations WHERE project = ?1 AND kind = ?2 AND source = 'automatic-system' AND status = 'pending'",
            params![project, kind],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to query recommendations: {}", e))?;

    Ok(count > 0)
}

/// Return the count of recommendations for a project, grouped by status.
pub fn count_recommendations(project: &str) -> Result<RecommendationCounts, String> {
    if !crate::core::is_valid_name(project) {
        return Err("Invalid project name".into());
    }

    let conn = open_conn()?;
    let mut stmt = conn
        .prepare("SELECT status, COUNT(*) FROM recommendations WHERE project = ?1 GROUP BY status")
        .map_err(|e| format!("Failed to prepare count query: {}", e))?;

    let mut counts = RecommendationCounts::default();
    let rows = stmt
        .query_map(params![project], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|e| format!("Failed to execute count query: {}", e))?;

    for row in rows {
        let (status, count) = row.map_err(|e| format!("Failed to read count row: {}", e))?;
        match status.as_str() {
            "pending" => counts.pending = count,
            "dismissed" => counts.dismissed = count,
            "actioned" => counts.actioned = count,
            _ => {}
        }
    }

    Ok(counts)
}

/// Counts per status for a single project.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecommendationCounts {
    pub pending: i64,
    pub dismissed: i64,
    pub actioned: i64,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    /// Create an in-memory SQLite connection and initialise the schema.
    /// Returns a PathBuf that resolves to `:memory:` — callers must override
    /// `get_db_path` indirection via the `with_conn` helpers below.
    fn setup_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory DB");
        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS recommendations (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                project     TEXT    NOT NULL,
                kind        TEXT    NOT NULL,
                title       TEXT    NOT NULL,
                body        TEXT    NOT NULL DEFAULT '',
                priority    TEXT    NOT NULL DEFAULT 'normal',
                status      TEXT    NOT NULL DEFAULT 'pending',
                source      TEXT    NOT NULL DEFAULT '',
                metadata    TEXT    NOT NULL DEFAULT '',
                created_at  TEXT    NOT NULL,
                updated_at  TEXT    NOT NULL
            );
            CREATE INDEX IF NOT EXISTS recommendations_project_status
                ON recommendations (project, status);",
        )
        .unwrap();
        conn
    }

    // ── In-process helpers that bypass the file-system path ──────────────────

    fn insert_rec(conn: &Connection, project: &str, kind: &str, title: &str) -> i64 {
        let ts = now();
        conn.execute(
            "INSERT INTO recommendations (project, kind, title, body, priority, status, source, metadata, created_at, updated_at)
             VALUES (?1, ?2, ?3, '', 'normal', 'pending', '', '', ?4, ?5)",
            params![project, kind, title, ts, ts],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn fetch_rec(conn: &Connection, id: i64) -> Option<Recommendation> {
        conn.query_row(
            "SELECT id, project, kind, title, body, priority, status, source, metadata, created_at, updated_at
             FROM recommendations WHERE id = ?1",
            params![id],
            row_to_recommendation,
        )
        .ok()
    }

    fn list_recs(conn: &Connection, project: &str, status: Option<&str>) -> Vec<Recommendation> {
        if let Some(s) = status {
            let mut stmt = conn
                .prepare(
                    "SELECT id, project, kind, title, body, priority, status, source, metadata, created_at, updated_at
                     FROM recommendations WHERE project = ?1 AND status = ?2 ORDER BY created_at DESC",
                )
                .unwrap();
            stmt.query_map(params![project, s], row_to_recommendation)
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        } else {
            let mut stmt = conn
                .prepare(
                    "SELECT id, project, kind, title, body, priority, status, source, metadata, created_at, updated_at
                     FROM recommendations WHERE project = ?1 ORDER BY created_at DESC",
                )
                .unwrap();
            stmt.query_map(params![project], row_to_recommendation)
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        }
    }

    fn set_status(conn: &Connection, id: i64, status: &str) {
        let ts = now();
        conn.execute(
            "UPDATE recommendations SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![status, ts, id],
        )
        .unwrap();
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[test]
    fn schema_is_created() {
        let conn = setup_conn();
        // If the table doesn't exist this would panic.
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM recommendations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn insert_and_fetch() {
        let conn = setup_conn();
        let id = insert_rec(&conn, "my-project", "skill", "Try the php-pro skill");
        let rec = fetch_rec(&conn, id).expect("should exist");
        assert_eq!(rec.project, "my-project");
        assert_eq!(rec.kind, "skill");
        assert_eq!(rec.title, "Try the php-pro skill");
        assert_eq!(rec.status, RecommendationStatus::Pending);
        assert_eq!(rec.priority, RecommendationPriority::Normal);
    }

    #[test]
    fn list_all_for_project() {
        let conn = setup_conn();
        insert_rec(&conn, "proj-a", "skill", "First");
        insert_rec(&conn, "proj-a", "mcp_server", "Second");
        insert_rec(&conn, "proj-b", "skill", "Other project");

        let recs = list_recs(&conn, "proj-a", None);
        assert_eq!(recs.len(), 2, "should only return proj-a records");
        assert!(recs.iter().all(|r| r.project == "proj-a"));
    }

    #[test]
    fn list_filtered_by_status() {
        let conn = setup_conn();
        let id1 = insert_rec(&conn, "proj-a", "skill", "Pending one");
        let id2 = insert_rec(&conn, "proj-a", "skill", "Will be dismissed");
        set_status(&conn, id2, "dismissed");

        let pending = list_recs(&conn, "proj-a", Some("pending"));
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, id1);

        let dismissed = list_recs(&conn, "proj-a", Some("dismissed"));
        assert_eq!(dismissed.len(), 1);
        assert_eq!(dismissed[0].id, id2);
    }

    #[test]
    fn dismiss_transitions_status() {
        let conn = setup_conn();
        let id = insert_rec(&conn, "proj", "rule", "Add eslint rule");

        let ts = now();
        conn.execute(
            "UPDATE recommendations SET status = 'dismissed', updated_at = ?1 WHERE id = ?2",
            params![ts, id],
        )
        .unwrap();

        let rec = fetch_rec(&conn, id).unwrap();
        assert_eq!(rec.status, RecommendationStatus::Dismissed);
    }

    #[test]
    fn action_transitions_status() {
        let conn = setup_conn();
        let id = insert_rec(&conn, "proj", "agent", "Add cursor agent");

        let ts = now();
        conn.execute(
            "UPDATE recommendations SET status = 'actioned', updated_at = ?1 WHERE id = ?2",
            params![ts, id],
        )
        .unwrap();

        let rec = fetch_rec(&conn, id).unwrap();
        assert_eq!(rec.status, RecommendationStatus::Actioned);
    }

    #[test]
    fn delete_removes_record() {
        let conn = setup_conn();
        let id = insert_rec(&conn, "proj", "skill", "Deletable rec");
        assert!(fetch_rec(&conn, id).is_some());

        conn.execute("DELETE FROM recommendations WHERE id = ?1", params![id])
            .unwrap();
        assert!(fetch_rec(&conn, id).is_none());
    }

    #[test]
    fn delete_nonexistent_returns_zero_rows() {
        let conn = setup_conn();
        let changed = conn
            .execute(
                "DELETE FROM recommendations WHERE id = ?1",
                params![9999i64],
            )
            .unwrap();
        assert_eq!(changed, 0);
    }

    #[test]
    fn clear_by_project() {
        let conn = setup_conn();
        insert_rec(&conn, "proj-x", "skill", "A");
        insert_rec(&conn, "proj-x", "skill", "B");
        insert_rec(&conn, "proj-y", "skill", "C");

        conn.execute(
            "DELETE FROM recommendations WHERE project = ?1",
            params!["proj-x"],
        )
        .unwrap();

        assert_eq!(list_recs(&conn, "proj-x", None).len(), 0);
        assert_eq!(list_recs(&conn, "proj-y", None).len(), 1);
    }

    #[test]
    fn clear_by_project_and_status() {
        let conn = setup_conn();
        let id1 = insert_rec(&conn, "proj", "skill", "Pending");
        let id2 = insert_rec(&conn, "proj", "skill", "Dismissed");
        set_status(&conn, id2, "dismissed");

        // Only clear dismissed
        conn.execute(
            "DELETE FROM recommendations WHERE project = ?1 AND status = 'dismissed'",
            params!["proj"],
        )
        .unwrap();

        assert!(fetch_rec(&conn, id1).is_some(), "pending should survive");
        assert!(fetch_rec(&conn, id2).is_none(), "dismissed should be gone");
    }

    #[test]
    fn count_by_status() {
        let conn = setup_conn();
        let id1 = insert_rec(&conn, "proj", "skill", "P1");
        let id2 = insert_rec(&conn, "proj", "skill", "P2");
        let id3 = insert_rec(&conn, "proj", "skill", "D1");
        set_status(&conn, id3, "dismissed");
        let id4 = insert_rec(&conn, "proj", "skill", "A1");
        set_status(&conn, id4, "actioned");
        // id1, id2 remain pending
        let _ = id1;
        let _ = id2;

        let mut stmt = conn
            .prepare(
                "SELECT status, COUNT(*) FROM recommendations WHERE project = ?1 GROUP BY status",
            )
            .unwrap();
        let mut counts = RecommendationCounts::default();
        let rows: Vec<(String, i64)> = stmt
            .query_map(params!["proj"], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        for (s, c) in rows {
            match s.as_str() {
                "pending" => counts.pending = c,
                "dismissed" => counts.dismissed = c,
                "actioned" => counts.actioned = c,
                _ => {}
            }
        }
        assert_eq!(counts.pending, 2);
        assert_eq!(counts.dismissed, 1);
        assert_eq!(counts.actioned, 1);
    }

    #[test]
    fn priority_round_trips() {
        for (input, expected) in &[
            ("low", RecommendationPriority::Low),
            ("normal", RecommendationPriority::Normal),
            ("high", RecommendationPriority::High),
            ("unknown", RecommendationPriority::Normal), // fallback
        ] {
            assert_eq!(RecommendationPriority::from_str(input), *expected);
        }
    }

    #[test]
    fn status_round_trips() {
        for (input, expected) in &[
            ("pending", RecommendationStatus::Pending),
            ("dismissed", RecommendationStatus::Dismissed),
            ("actioned", RecommendationStatus::Actioned),
            ("unknown", RecommendationStatus::Pending), // fallback
        ] {
            assert_eq!(RecommendationStatus::from_str(input), *expected);
        }
    }

    #[test]
    fn records_are_isolated_between_projects() {
        let conn = setup_conn();
        insert_rec(&conn, "alpha", "skill", "Alpha rec");
        insert_rec(&conn, "beta", "skill", "Beta rec");

        let alpha = list_recs(&conn, "alpha", None);
        let beta = list_recs(&conn, "beta", None);

        assert_eq!(alpha.len(), 1);
        assert_eq!(beta.len(), 1);
        assert_eq!(alpha[0].project, "alpha");
        assert_eq!(beta[0].project, "beta");
    }
}
