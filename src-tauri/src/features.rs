//! Feature tracking: per-project work items with a six-stage lifecycle.
//!
//! Schema (features.db):
//!
//!   features (
//!     id           TEXT    PRIMARY KEY,           -- UUID v4
//!     project      TEXT    NOT NULL,
//!     title        TEXT    NOT NULL,
//!     description  TEXT    NOT NULL DEFAULT '',   -- markdown
//!     state        TEXT    NOT NULL DEFAULT 'backlog',
//!     priority     TEXT    NOT NULL DEFAULT 'medium',
//!     assignee     TEXT,
//!     tags         TEXT    NOT NULL DEFAULT '[]', -- JSON array
//!     linked_files TEXT    NOT NULL DEFAULT '[]', -- JSON array
//!     effort       TEXT,
//!     created_at   TEXT    NOT NULL,
//!     updated_at   TEXT    NOT NULL,
//!     created_by   TEXT,
//!     position     INTEGER NOT NULL DEFAULT 0,
//!     archived     INTEGER NOT NULL DEFAULT 0     -- boolean: 0 = active, 1 = archived
//!   )
//!
//!   feature_updates (
//!     id          INTEGER PRIMARY KEY AUTOINCREMENT,
//!     feature_id  TEXT    NOT NULL REFERENCES features(id) ON DELETE CASCADE,
//!     project     TEXT    NOT NULL,
//!     content     TEXT    NOT NULL,   -- markdown
//!     author      TEXT,
//!     timestamp   TEXT    NOT NULL
//!   )
//!
//! The DB file lives at `~/.automatic/features.db`
//! (dev: `~/.automatic-dev/features.db`).

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── Types ─────────────────────────────────────────────────────────────────────

/// The lifecycle states a feature can occupy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureState {
    Backlog,
    Todo,
    InProgress,
    Review,
    Complete,
    Cancelled,
}

impl FeatureState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Backlog => "backlog",
            Self::Todo => "todo",
            Self::InProgress => "in_progress",
            Self::Review => "review",
            Self::Complete => "complete",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "backlog" => Ok(Self::Backlog),
            "todo" => Ok(Self::Todo),
            "in_progress" => Ok(Self::InProgress),
            "review" => Ok(Self::Review),
            "complete" => Ok(Self::Complete),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(format!(
                "Invalid feature state '{}'. Valid states: backlog, todo, in_progress, review, complete, cancelled",
                other
            )),
        }
    }
}

/// Priority level for triage within a state column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeaturePriority {
    Low,
    Medium,
    High,
}

impl FeaturePriority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            other => Err(format!(
                "Invalid priority '{}'. Valid values: low, medium, high",
                other
            )),
        }
    }
}

/// T-shirt size effort estimate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffortSize {
    Xs,
    S,
    M,
    L,
    Xl,
}

impl EffortSize {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Xs => "xs",
            Self::S => "s",
            Self::M => "m",
            Self::L => "l",
            Self::Xl => "xl",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "xs" => Ok(Self::Xs),
            "s" => Ok(Self::S),
            "m" => Ok(Self::M),
            "l" => Ok(Self::L),
            "xl" => Ok(Self::Xl),
            other => Err(format!(
                "Invalid effort size '{}'. Valid values: xs, s, m, l, xl",
                other
            )),
        }
    }
}

/// A single feature (without updates). Returned by list/get operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feature {
    pub id: String,
    pub project: String,
    pub title: String,
    pub description: String,
    pub state: String,
    pub priority: String,
    pub assignee: Option<String>,
    pub tags: Vec<String>,
    pub linked_files: Vec<String>,
    pub effort: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub created_by: Option<String>,
    pub position: i64,
    /// Whether this feature is archived. Archived features are hidden from the
    /// Kanban board and from list/filter queries unless explicitly requested.
    pub archived: bool,
}

/// A feature together with its full update history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureWithUpdates {
    #[serde(flatten)]
    pub feature: Feature,
    pub updates: Vec<FeatureUpdate>,
}

/// A single progress update appended to a feature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureUpdate {
    pub id: i64,
    pub feature_id: String,
    pub project: String,
    pub content: String,
    pub author: Option<String>,
    pub timestamp: String,
}

/// Partial update payload for `update_feature`. Only `Some` fields are applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeaturePatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub state: Option<String>,
    pub priority: Option<String>,
    /// `Some(None)` explicitly clears the assignee field.
    pub assignee: Option<Option<String>>,
    pub tags: Option<Vec<String>>,
    pub linked_files: Option<Vec<String>>,
    /// `Some(None)` explicitly clears the effort field.
    pub effort: Option<Option<String>>,
    /// Archive or unarchive. `Some(true)` archives; `Some(false)` unarchives.
    pub archived: Option<bool>,
}

// ── DB path ───────────────────────────────────────────────────────────────────

fn get_db_path() -> Result<PathBuf, String> {
    let dir = crate::core::get_automatic_dir()?;
    Ok(dir.join("features.db"))
}

// ── Connection + schema ───────────────────────────────────────────────────────

fn open_conn() -> Result<Connection, String> {
    let path = get_db_path()?;
    let conn = Connection::open(&path).map_err(|e| format!("Failed to open features DB: {}", e))?;

    // WAL mode for concurrent reads during writes; enforce FK constraints.
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
        .map_err(|e| format!("Failed to configure features DB: {}", e))?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS features (
            id           TEXT    PRIMARY KEY,
            project      TEXT    NOT NULL,
            title        TEXT    NOT NULL,
            description  TEXT    NOT NULL DEFAULT '',
            state        TEXT    NOT NULL DEFAULT 'backlog',
            priority     TEXT    NOT NULL DEFAULT 'medium',
            assignee     TEXT,
            tags         TEXT    NOT NULL DEFAULT '[]',
            linked_files TEXT    NOT NULL DEFAULT '[]',
            effort       TEXT,
            created_at   TEXT    NOT NULL,
            updated_at   TEXT    NOT NULL,
            created_by   TEXT,
            position     INTEGER NOT NULL DEFAULT 0,
            archived     INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS feature_updates (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            feature_id  TEXT    NOT NULL REFERENCES features(id) ON DELETE CASCADE,
            project     TEXT    NOT NULL,
            content     TEXT    NOT NULL,
            author      TEXT,
            timestamp   TEXT    NOT NULL
        );
        CREATE INDEX IF NOT EXISTS features_project_state
            ON features (project, state, position);
        CREATE INDEX IF NOT EXISTS feature_updates_feature
            ON feature_updates (feature_id, timestamp DESC);",
    )
    .map_err(|e| format!("Failed to create features schema: {}", e))?;

    // Additive migration: add `archived` column to existing databases that
    // pre-date this schema version. SQLite ignores the statement if the column
    // already exists when using the `IF NOT EXISTS` form via a guard check.
    let _ =
        conn.execute_batch("ALTER TABLE features ADD COLUMN archived INTEGER NOT NULL DEFAULT 0;");
    // Note: the above intentionally ignores the error — SQLite returns an error
    // if the column already exists, which is the normal case after first run.

    Ok(conn)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn decode_json_array(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn encode_json_array(items: &[String]) -> String {
    serde_json::to_string(items).unwrap_or_else(|_| "[]".to_string())
}

/// Map a rusqlite row to a `Feature`. Column order must match all SELECT statements.
fn row_to_feature(row: &rusqlite::Row<'_>) -> rusqlite::Result<Feature> {
    let tags_raw: String = row.get(7)?;
    let files_raw: String = row.get(8)?;
    let archived_int: i64 = row.get(14)?;
    Ok(Feature {
        id: row.get(0)?,
        project: row.get(1)?,
        title: row.get(2)?,
        description: row.get(3)?,
        state: row.get(4)?,
        priority: row.get(5)?,
        assignee: row.get(6)?,
        tags: decode_json_array(&tags_raw),
        linked_files: decode_json_array(&files_raw),
        effort: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
        created_by: row.get(12)?,
        position: row.get(13)?,
        archived: archived_int != 0,
    })
}

/// Return the next available position in a given (project, state) slot.
fn next_position(conn: &Connection, project: &str, state: &str) -> Result<i64, String> {
    let pos: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM features WHERE project = ?1 AND state = ?2",
            params![project, state],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to compute next position: {}", e))?;
    Ok(pos)
}

// ── Public API ────────────────────────────────────────────────────────────────

/// List all features for a project, optionally filtered by state.
///
/// By default only active (non-archived) features are returned.  Pass
/// `include_archived = true` to return **only** archived features instead.
/// Ordered by state lifecycle order then position within each state.
pub fn list_features(
    project: &str,
    state_filter: Option<&str>,
    include_archived: bool,
) -> Result<Vec<Feature>, String> {
    if !crate::core::is_valid_name(project) {
        return Err("Invalid project name".into());
    }
    let conn = open_conn()?;

    // archived filter: 0 for active features, 1 for archived features.
    let archived_val: i64 = if include_archived { 1 } else { 0 };

    let features = if let Some(state) = state_filter {
        let mut stmt = conn
            .prepare(
                "SELECT id, project, title, description, state, priority, assignee,
                        tags, linked_files, effort, created_at, updated_at, created_by, position, archived
                 FROM features
                 WHERE project = ?1 AND state = ?2 AND archived = ?3
                 ORDER BY position ASC",
            )
            .map_err(|e| format!("Failed to prepare list query: {}", e))?;

        let rows = stmt
            .query_map(params![project, state, archived_val], row_to_feature)
            .map_err(|e| format!("Failed to query features: {}", e))?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| format!("Failed to read feature rows: {}", e))?
    } else {
        let mut stmt = conn
            .prepare(
                "SELECT id, project, title, description, state, priority, assignee,
                        tags, linked_files, effort, created_at, updated_at, created_by, position, archived
                 FROM features
                 WHERE project = ?1 AND archived = ?2
                 ORDER BY
                   CASE state
                     WHEN 'backlog'     THEN 0
                     WHEN 'todo'        THEN 1
                     WHEN 'in_progress' THEN 2
                     WHEN 'review'      THEN 3
                     WHEN 'complete'    THEN 4
                     ELSE 5
                   END,
                   position ASC",
            )
            .map_err(|e| format!("Failed to prepare list query: {}", e))?;

        let rows = stmt
            .query_map(params![project, archived_val], row_to_feature)
            .map_err(|e| format!("Failed to query features: {}", e))?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| format!("Failed to read feature rows: {}", e))?
    };

    Ok(features)
}

/// Get a single feature by ID (without updates).
///
/// Accepts either a full UUID or a unique case-insensitive prefix.  If the
/// prefix matches more than one feature an error is returned so the caller
/// can disambiguate.
pub fn get_feature(project: &str, feature_id: &str) -> Result<Feature, String> {
    if !crate::core::is_valid_name(project) {
        return Err("Invalid project name".into());
    }
    let conn = open_conn()?;

    // 1. Try exact match first (fast path). Searches both active and archived
    //    so that detail panels can load archived features when the user has
    //    the "view archived" toggle on.
    let exact = conn.query_row(
        "SELECT id, project, title, description, state, priority, assignee,
                tags, linked_files, effort, created_at, updated_at, created_by, position, archived
         FROM features
         WHERE id = ?1 AND project = ?2",
        params![feature_id, project],
        row_to_feature,
    );

    match exact {
        Ok(f) => return Ok(f),
        Err(rusqlite::Error::QueryReturnedNoRows) => {} // fall through to prefix search
        Err(e) => return Err(format!("Failed to get feature: {}", e)),
    }

    // 2. Case-insensitive prefix search.
    let prefix_pattern = format!("{}%", feature_id.to_lowercase());
    let mut stmt = conn
        .prepare(
            "SELECT id, project, title, description, state, priority, assignee,
                    tags, linked_files, effort, created_at, updated_at, created_by, position, archived
             FROM features
             WHERE LOWER(id) LIKE ?1 AND project = ?2",
        )
        .map_err(|e| format!("Failed to prepare prefix query: {}", e))?;

    let matches: Vec<Feature> = stmt
        .query_map(params![prefix_pattern, project], row_to_feature)
        .map_err(|e| format!("Failed to query features by prefix: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    match matches.len() {
        0 => Err(format!(
            "Feature '{}' not found in project '{}'",
            feature_id, project
        )),
        1 => Ok(matches.into_iter().next().unwrap()),
        n => Err(format!(
            "Partial ID '{}' is ambiguous: {} features match in project '{}'. Provide more characters.",
            feature_id, n, project
        )),
    }
}

/// Get a feature together with its full update history (updates newest-first).
pub fn get_feature_with_updates(
    project: &str,
    feature_id: &str,
) -> Result<FeatureWithUpdates, String> {
    let feature = get_feature(project, feature_id)?;
    let updates = get_feature_updates(project, feature_id)?;
    Ok(FeatureWithUpdates { feature, updates })
}

/// Create a new feature in the project's backlog.
pub fn create_feature(
    project: &str,
    title: &str,
    description: &str,
    priority: &str,
    assignee: Option<&str>,
    tags: &[String],
    linked_files: &[String],
    effort: Option<&str>,
    created_by: Option<&str>,
) -> Result<Feature, String> {
    if !crate::core::is_valid_name(project) {
        return Err("Invalid project name".into());
    }
    if title.trim().is_empty() {
        return Err("Feature title must not be empty".into());
    }
    FeaturePriority::from_str(priority)?;
    if let Some(e) = effort {
        EffortSize::from_str(e)?;
    }

    let conn = open_conn()?;
    let id = new_id();
    let ts = now();
    let position = next_position(&conn, project, "backlog")?;
    let tags_json = encode_json_array(tags);
    let files_json = encode_json_array(linked_files);

    conn.execute(
        "INSERT INTO features
            (id, project, title, description, state, priority, assignee,
             tags, linked_files, effort, created_at, updated_at, created_by, position, archived)
         VALUES (?1, ?2, ?3, ?4, 'backlog', ?5, ?6, ?7, ?8, ?9, ?10, ?10, ?11, ?12, 0)",
        params![
            id,
            project,
            title.trim(),
            description,
            priority,
            assignee,
            tags_json,
            files_json,
            effort,
            ts,
            created_by,
            position,
        ],
    )
    .map_err(|e| format!("Failed to create feature: {}", e))?;

    crate::activity::log(
        project,
        crate::activity::ActivityEvent::FeatureCreated,
        &format!("Feature created: {}", title.trim()),
        &id,
    );

    get_feature(project, &id)
}

/// Apply a partial update to a feature's metadata fields.
pub fn update_feature(
    project: &str,
    feature_id: &str,
    patch: FeaturePatch,
) -> Result<Feature, String> {
    if !crate::core::is_valid_name(project) {
        return Err("Invalid project name".into());
    }

    // Validate new values before touching the DB.
    if let Some(ref p) = patch.priority {
        FeaturePriority::from_str(p)?;
    }
    if let Some(ref s) = patch.state {
        FeatureState::from_str(s)?;
    }
    if let Some(Some(ref e)) = patch.effort {
        EffortSize::from_str(e)?;
    }
    if let Some(ref t) = patch.title {
        if t.trim().is_empty() {
            return Err("Feature title must not be empty".into());
        }
    }

    let existing = get_feature(project, feature_id)?;
    let conn = open_conn()?;
    let ts = now();

    let new_title = patch.title.as_deref().unwrap_or(&existing.title);
    let new_description = patch
        .description
        .as_deref()
        .unwrap_or(&existing.description);
    let new_state = patch.state.as_deref().unwrap_or(&existing.state);
    let new_priority = patch.priority.as_deref().unwrap_or(&existing.priority);

    let new_assignee: Option<String> = match patch.assignee {
        Some(Some(ref a)) => Some(a.clone()),
        Some(None) => None,
        None => existing.assignee.clone(),
    };

    let new_tags = patch.tags.as_deref().unwrap_or(&existing.tags);
    let new_files = patch
        .linked_files
        .as_deref()
        .unwrap_or(&existing.linked_files);

    let new_effort: Option<String> = match patch.effort {
        Some(Some(ref e)) => Some(e.clone()),
        Some(None) => None,
        None => existing.effort.clone(),
    };

    let new_archived: bool = patch.archived.unwrap_or(existing.archived);
    let archived_val: i64 = if new_archived { 1 } else { 0 };

    let tags_json = encode_json_array(new_tags);
    let files_json = encode_json_array(new_files);

    conn.execute(
        "UPDATE features
         SET title = ?1, description = ?2, state = ?3, priority = ?4,
             assignee = ?5, tags = ?6, linked_files = ?7, effort = ?8, updated_at = ?9,
             archived = ?12
         WHERE id = ?10 AND project = ?11",
        params![
            new_title.trim(),
            new_description,
            new_state,
            new_priority,
            new_assignee,
            tags_json,
            files_json,
            new_effort,
            ts,
            feature_id,
            project,
            archived_val,
        ],
    )
    .map_err(|e| format!("Failed to update feature: {}", e))?;

    crate::activity::log(
        project,
        crate::activity::ActivityEvent::FeatureUpdated,
        &format!("Feature updated: {}", new_title.trim()),
        feature_id,
    );

    get_feature(project, feature_id)
}

/// Change a feature's state. Places the feature at the end of the target state column.
pub fn set_feature_state(
    project: &str,
    feature_id: &str,
    new_state: &str,
) -> Result<Feature, String> {
    if !crate::core::is_valid_name(project) {
        return Err("Invalid project name".into());
    }
    FeatureState::from_str(new_state)?;

    let existing = get_feature(project, feature_id)?;
    let conn = open_conn()?;
    let ts = now();
    let position = next_position(&conn, project, new_state)?;

    conn.execute(
        "UPDATE features SET state = ?1, position = ?2, updated_at = ?3 WHERE id = ?4 AND project = ?5",
        params![new_state, position, ts, feature_id, project],
    )
    .map_err(|e| format!("Failed to set feature state: {}", e))?;

    crate::activity::log(
        project,
        crate::activity::ActivityEvent::FeatureStateChanged,
        &format!("Feature '{}' moved to {}", existing.title, new_state),
        &format!("{} -> {}", existing.state, new_state),
    );

    get_feature(project, feature_id)
}

/// Move a feature to a new state and position (used by Kanban drag-and-drop).
/// Shifts other features in the target column to make room atomically.
pub fn move_feature(
    project: &str,
    feature_id: &str,
    new_state: &str,
    new_position: i64,
) -> Result<(), String> {
    if !crate::core::is_valid_name(project) {
        return Err("Invalid project name".into());
    }
    FeatureState::from_str(new_state)?;

    let existing = get_feature(project, feature_id)?;
    let conn = open_conn()?;
    let ts = now();

    conn.execute_batch("BEGIN IMMEDIATE;")
        .map_err(|e| format!("Failed to begin transaction: {}", e))?;

    let result = (|| -> Result<(), String> {
        // Shift items in the target column at or after new_position to make room.
        conn.execute(
            "UPDATE features
             SET position = position + 1
             WHERE project = ?1 AND state = ?2 AND position >= ?3 AND id != ?4",
            params![project, new_state, new_position, feature_id],
        )
        .map_err(|e| format!("Failed to shift positions: {}", e))?;

        conn.execute(
            "UPDATE features
             SET state = ?1, position = ?2, updated_at = ?3
             WHERE id = ?4 AND project = ?5",
            params![new_state, new_position, ts, feature_id, project],
        )
        .map_err(|e| format!("Failed to move feature: {}", e))?;

        Ok(())
    })();

    match result {
        Ok(()) => {
            conn.execute_batch("COMMIT;")
                .map_err(|e| format!("Failed to commit move: {}", e))?;
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK;");
            return Err(e);
        }
    }

    if existing.state != new_state {
        crate::activity::log(
            project,
            crate::activity::ActivityEvent::FeatureStateChanged,
            &format!("Feature '{}' moved to {}", existing.title, new_state),
            &format!("{} -> {}", existing.state, new_state),
        );
    }

    Ok(())
}

/// Permanently delete a feature and all its updates (cascade via FK).
pub fn delete_feature(project: &str, feature_id: &str) -> Result<(), String> {
    if !crate::core::is_valid_name(project) {
        return Err("Invalid project name".into());
    }

    let feature = get_feature(project, feature_id)?;

    let conn = open_conn()?;
    conn.execute(
        "DELETE FROM features WHERE id = ?1 AND project = ?2",
        params![feature_id, project],
    )
    .map_err(|e| format!("Failed to delete feature: {}", e))?;

    crate::activity::log(
        project,
        crate::activity::ActivityEvent::FeatureDeleted,
        &format!("Feature deleted: {}", feature.title),
        feature_id,
    );

    Ok(())
}

/// Archive a feature, hiding it from the Kanban board and default list views.
///
/// The feature's `state` is preserved unchanged so that it can be restored to
/// its original column when unarchived.
pub fn archive_feature(project: &str, feature_id: &str) -> Result<Feature, String> {
    if !crate::core::is_valid_name(project) {
        return Err("Invalid project name".into());
    }

    let existing = get_feature(project, feature_id)?;
    if existing.archived {
        return Err(format!("Feature '{}' is already archived", feature_id));
    }

    let conn = open_conn()?;
    let ts = now();

    conn.execute(
        "UPDATE features SET archived = 1, updated_at = ?1 WHERE id = ?2 AND project = ?3",
        params![ts, feature_id, project],
    )
    .map_err(|e| format!("Failed to archive feature: {}", e))?;

    crate::activity::log(
        project,
        crate::activity::ActivityEvent::FeatureUpdated,
        &format!("Feature '{}' archived", existing.title),
        feature_id,
    );

    get_feature(project, feature_id)
}

/// Unarchive a feature, restoring it to its preserved state in the Kanban board
/// and default list views.
pub fn unarchive_feature(project: &str, feature_id: &str) -> Result<Feature, String> {
    if !crate::core::is_valid_name(project) {
        return Err("Invalid project name".into());
    }

    let existing = get_feature(project, feature_id)?;
    if !existing.archived {
        return Err(format!("Feature '{}' is not archived", feature_id));
    }

    let conn = open_conn()?;
    let ts = now();

    conn.execute(
        "UPDATE features SET archived = 0, updated_at = ?1 WHERE id = ?2 AND project = ?3",
        params![ts, feature_id, project],
    )
    .map_err(|e| format!("Failed to unarchive feature: {}", e))?;

    crate::activity::log(
        project,
        crate::activity::ActivityEvent::FeatureUpdated,
        &format!("Feature '{}' unarchived", existing.title),
        feature_id,
    );

    get_feature(project, feature_id)
}

/// Append a progress update to a feature. Returns the new update record.
pub fn add_feature_update(
    project: &str,
    feature_id: &str,
    content: &str,
    author: Option<&str>,
) -> Result<FeatureUpdate, String> {
    if !crate::core::is_valid_name(project) {
        return Err("Invalid project name".into());
    }
    if content.trim().is_empty() {
        return Err("Update content must not be empty".into());
    }

    // Confirm feature exists in this project before inserting.
    get_feature(project, feature_id)?;

    let conn = open_conn()?;
    let ts = now();

    conn.execute(
        "INSERT INTO feature_updates (feature_id, project, content, author, timestamp)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![feature_id, project, content, author, ts],
    )
    .map_err(|e| format!("Failed to insert feature update: {}", e))?;

    let id = conn.last_insert_rowid();

    Ok(FeatureUpdate {
        id,
        feature_id: feature_id.to_string(),
        project: project.to_string(),
        content: content.to_string(),
        author: author.map(|s| s.to_string()),
        timestamp: ts,
    })
}

/// Get all updates for a feature, ordered newest-first.
pub fn get_feature_updates(project: &str, feature_id: &str) -> Result<Vec<FeatureUpdate>, String> {
    if !crate::core::is_valid_name(project) {
        return Err("Invalid project name".into());
    }
    let conn = open_conn()?;

    let mut stmt = conn
        .prepare(
            "SELECT id, feature_id, project, content, author, timestamp
             FROM feature_updates
             WHERE feature_id = ?1 AND project = ?2
             ORDER BY timestamp DESC",
        )
        .map_err(|e| format!("Failed to prepare updates query: {}", e))?;

    let rows = stmt
        .query_map(params![feature_id, project], |row| {
            Ok(FeatureUpdate {
                id: row.get(0)?,
                feature_id: row.get(1)?,
                project: row.get(2)?,
                content: row.get(3)?,
                author: row.get(4)?,
                timestamp: row.get(5)?,
            })
        })
        .map_err(|e| format!("Failed to query feature updates: {}", e))?;

    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| format!("Failed to read update rows: {}", e))
}

// ── MCP-formatted output helpers ──────────────────────────────────────────────

/// Format a list of features as human-readable markdown for MCP responses.
///
/// `archived_view` should be `true` when the list was fetched with
/// `include_archived = true` so the output header is labelled accordingly
/// and agents do not mistake archived features for active ones.
pub fn format_features_markdown(
    features: &[Feature],
    project: &str,
    archived_view: bool,
) -> String {
    if features.is_empty() {
        return if archived_view {
            format!("No archived features found for project '{}'.\n", project)
        } else {
            format!("No active features found for project '{}'.\n", project)
        };
    }

    let heading = if archived_view {
        format!("# Archived Features for '{}'\n\n", project)
    } else {
        format!("# Features for '{}'\n\n", project)
    };
    let mut out = heading;

    if archived_view {
        out.push_str(
            "> These features are archived. They are hidden from the Kanban board. \
             Use `automatic_unarchive_feature` to restore one.\n\n",
        );
    }

    out.push_str(&format!("{} feature(s)\n\n", features.len()));

    let states = [
        "backlog",
        "todo",
        "in_progress",
        "review",
        "complete",
        "cancelled",
    ];
    let state_labels = [
        "Backlog",
        "To Do",
        "In Progress",
        "Review",
        "Complete",
        "Cancelled",
    ];

    for (state, label) in states.iter().zip(state_labels.iter()) {
        let group: Vec<&Feature> = features.iter().filter(|f| f.state == *state).collect();
        if group.is_empty() {
            continue;
        }
        out.push_str(&format!("## {} ({})\n\n", label, group.len()));
        for f in group {
            out.push_str(&format!("- **{}** `{}`\n", f.title, f.id));
            out.push_str(&format!(
                "  Priority: {} | Effort: {} | Assignee: {}\n",
                f.priority,
                f.effort.as_deref().unwrap_or("—"),
                f.assignee.as_deref().unwrap_or("unassigned")
            ));
            if !f.tags.is_empty() {
                out.push_str(&format!("  Tags: {}\n", f.tags.join(", ")));
            }
        }
        out.push('\n');
    }

    out
}

/// Format a single feature with all its updates as markdown for MCP responses.
pub fn format_feature_detail_markdown(fw: &FeatureWithUpdates) -> String {
    let f = &fw.feature;
    let mut out = format!("# {}\n\n", f.title);

    if f.archived {
        out.push_str(
            "> **Archived** — This feature is hidden from the Kanban board. \
             Use `automatic_unarchive_feature` to restore it.\n\n",
        );
    }

    out.push_str(&format!("**ID:** `{}`\n", f.id));
    out.push_str(&format!("**Project:** {}\n", f.project));
    out.push_str(&format!("**Archived:** {}\n", f.archived));
    out.push_str(&format!("**State:** {}\n", f.state));
    out.push_str(&format!("**Priority:** {}\n", f.priority));
    out.push_str(&format!(
        "**Effort:** {}\n",
        f.effort.as_deref().unwrap_or("—")
    ));
    out.push_str(&format!(
        "**Assignee:** {}\n",
        f.assignee.as_deref().unwrap_or("unassigned")
    ));
    if !f.tags.is_empty() {
        out.push_str(&format!("**Tags:** {}\n", f.tags.join(", ")));
    }
    if !f.linked_files.is_empty() {
        out.push_str(&format!(
            "**Linked files:** {}\n",
            f.linked_files.join(", ")
        ));
    }
    out.push_str(&format!("**Created:** {}\n", f.created_at));
    out.push_str(&format!("**Updated:** {}\n", f.updated_at));

    if !f.description.is_empty() {
        out.push_str("\n## Description\n\n");
        out.push_str(&f.description);
        out.push('\n');
    }

    if fw.updates.is_empty() {
        out.push_str("\n## Updates\n\nNo updates yet.\n");
    } else {
        out.push_str(&format!("\n## Updates ({})\n\n", fw.updates.len()));
        for u in &fw.updates {
            out.push_str(&format!(
                "### {} — {}\n\n{}\n\n",
                u.timestamp,
                u.author.as_deref().unwrap_or("unknown"),
                u.content
            ));
        }
    }

    out
}
