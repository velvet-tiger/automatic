//! Task log: the entries shown in the app's floating task log panel.
//!
//! Entries live in SQLite at `~/.automatic/task_log.db`. The file is separate
//! from `activity.db` so that this `core` module does not depend on the
//! `activity` module, and so that each database file has one owner for its
//! schema.
//!
//! Schema:
//!   task_log (
//!     seq       INTEGER PRIMARY KEY AUTOINCREMENT,  -- insertion order
//!     id        TEXT    NOT NULL UNIQUE,            -- frontend-generated id
//!     timestamp TEXT    NOT NULL,                   -- ISO 8601
//!     message   TEXT    NOT NULL,
//!     status    TEXT    NOT NULL,
//!     agent     TEXT                                -- NULL for non-AI entries
//!   )
//!   task_log_meta ( key TEXT PRIMARY KEY, value TEXT NOT NULL )
//!
//! Ordering: entries are returned oldest-first by `seq`. Writing an entry
//! whose `id` already exists updates the row and keeps its `seq`, so an
//! updated entry keeps its position. Only the newest `MAX_ENTRIES` rows are
//! kept.
//!
//! Legacy import: earlier versions stored the log in `task_log.json`. The
//! first open of the database imports that file once. See
//! [`import_legacy_json`] for the failure behaviour.

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::Duration;

use super::paths::get_automatic_dir;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Maximum number of entries retained. Oldest entries are pruned when this
/// limit is exceeded.
const MAX_ENTRIES: usize = 500;

const DB_FILE: &str = "task_log.db";
const LEGACY_JSON_FILE: &str = "task_log.json";
const LEGACY_JSON_MIGRATED_FILE: &str = "task_log.json.migrated";

/// `task_log_meta` key that records the outcome of the legacy JSON import.
const META_LEGACY_IMPORT: &str = "legacy_json_import";
const LEGACY_IMPORT_DONE: &str = "imported";
const LEGACY_IMPORT_NO_FILE: &str = "no_file";
const LEGACY_IMPORT_SKIPPED_MALFORMED: &str = "skipped_malformed";

// ── Types ─────────────────────────────────────────────────────────────────────

/// A single persisted task log entry. Mirrors the shape used by the frontend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedTaskLogEntry {
    pub id: String,
    /// ISO-8601 timestamp string (serialised by the frontend before calling
    /// `append_task_log_entry`).
    pub timestamp: String,
    pub message: String,
    pub status: String,
    /// Display label of the Automatic agent that performed the operation
    /// (e.g. `"Claude"`, `"OpenAI"`). Absent for non-AI entries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

// ── Connection + schema ───────────────────────────────────────────────────────

/// Open the task log database in the Automatic data directory. Creates the
/// directory and the schema, and runs the legacy import, on first use.
fn open_conn() -> Result<Connection, String> {
    let dir = get_automatic_dir()?;
    fs::create_dir_all(&dir).map_err(|e| {
        format!(
            "Failed to create the Automatic data directory {}: {}",
            dir.display(),
            e
        )
    })?;
    let mut conn = open_conn_at(&dir.join(DB_FILE))?;
    import_legacy_json(&mut conn, &dir)?;
    Ok(conn)
}

fn open_conn_at(path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(path)
        .map_err(|e| format!("Failed to open task log DB {}: {}", path.display(), e))?;

    // Tauri runs commands on several threads. A second writer waits for the
    // first instead of failing with SQLITE_BUSY.
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|e| format!("Failed to set the task log DB busy timeout: {}", e))?;

    conn.execute_batch("PRAGMA journal_mode=WAL;")
        .map_err(|e| format!("Failed to set WAL mode on the task log DB: {}", e))?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS task_log (
            seq       INTEGER PRIMARY KEY AUTOINCREMENT,
            id        TEXT    NOT NULL UNIQUE,
            timestamp TEXT    NOT NULL,
            message   TEXT    NOT NULL,
            status    TEXT    NOT NULL,
            agent     TEXT
        );
        CREATE TABLE IF NOT EXISTS task_log_meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );",
    )
    .map_err(|e| format!("Failed to create the task log tables: {}", e))?;

    Ok(conn)
}

fn begin_write(conn: &mut Connection) -> Result<Transaction<'_>, String> {
    // IMMEDIATE takes the write lock up front. Two threads that both find the
    // import marker missing then run one after the other, and the second one
    // sees the marker the first one wrote.
    conn.transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to start a task log transaction: {}", e))
}

// ── Row helpers ───────────────────────────────────────────────────────────────

/// Insert each entry, or update the row with the same `id` in place, then
/// prune to the newest `MAX_ENTRIES` rows.
fn upsert_entries(tx: &Transaction<'_>, entries: &[PersistedTaskLogEntry]) -> Result<(), String> {
    {
        let mut stmt = tx
            .prepare(
                "INSERT INTO task_log (id, timestamp, message, status, agent)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET
                     timestamp = excluded.timestamp,
                     message   = excluded.message,
                     status    = excluded.status,
                     agent     = excluded.agent",
            )
            .map_err(|e| format!("Failed to prepare the task log insert: {}", e))?;
        for entry in entries {
            stmt.execute(params![
                entry.id,
                entry.timestamp,
                entry.message,
                entry.status,
                entry.agent
            ])
            .map_err(|e| format!("Failed to write task log entry '{}': {}", entry.id, e))?;
        }
    }

    tx.execute(
        "DELETE FROM task_log
         WHERE seq NOT IN (
             SELECT seq FROM task_log ORDER BY seq DESC LIMIT ?1
         )",
        params![MAX_ENTRIES as i64],
    )
    .map_err(|e| format!("Failed to prune old task log entries: {}", e))?;

    Ok(())
}

fn read_entries(conn: &Connection) -> Result<Vec<PersistedTaskLogEntry>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, timestamp, message, status, agent
             FROM task_log
             ORDER BY seq ASC",
        )
        .map_err(|e| format!("Failed to prepare the task log query: {}", e))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(PersistedTaskLogEntry {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                message: row.get(2)?,
                status: row.get(3)?,
                agent: row.get(4)?,
            })
        })
        .map_err(|e| format!("Failed to query the task log: {}", e))?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(row.map_err(|e| format!("Failed to read a task log row: {}", e))?);
    }
    Ok(entries)
}

// ── Legacy JSON import ────────────────────────────────────────────────────────

/// Import `task_log.json` from `dir` into the database, once.
///
/// The entries and the `legacy_json_import` marker are written in one
/// transaction. A crash before the commit leaves the database unchanged and
/// the next open imports again. A crash after the commit leaves the marker
/// set and the next open does nothing. Entries are written by `id`, so even a
/// repeated import cannot create duplicates.
///
/// After a successful import the file is renamed to `task_log.json.migrated`.
/// The file is never deleted. If the rename fails, or a `.migrated` file
/// already exists, the file stays where it is and the reason is logged.
///
/// Failure behaviour:
/// - The file cannot be read: returns an error and writes no marker, so the
///   next open tries again.
/// - The file is not valid task log JSON: logs the problem to stderr, leaves
///   the file untouched and records `skipped_malformed`. The import is not
///   tried again, because entries imported later would sort after newer
///   entries. The file stays on disk for manual recovery.
fn import_legacy_json(conn: &mut Connection, dir: &Path) -> Result<(), String> {
    if legacy_import_state(conn)?.is_some() {
        return Ok(());
    }

    let json_path = dir.join(LEGACY_JSON_FILE);
    let tx = begin_write(conn)?;

    // Checked again under the write lock: another thread may have finished
    // the import between the check above and this transaction.
    if legacy_import_state(&tx)?.is_some() {
        return Ok(());
    }

    let outcome = match read_legacy_json(&json_path)? {
        LegacyJson::Missing => LEGACY_IMPORT_NO_FILE,
        LegacyJson::Malformed(reason) => {
            eprintln!(
                "[task-log] {} is not valid task log JSON and was not imported. \
                 The file was left untouched. Reason: {}",
                json_path.display(),
                reason
            );
            LEGACY_IMPORT_SKIPPED_MALFORMED
        }
        LegacyJson::Entries(entries) => {
            upsert_entries(&tx, &entries)?;
            LEGACY_IMPORT_DONE
        }
    };

    tx.execute(
        "INSERT INTO task_log_meta (key, value) VALUES (?1, ?2)",
        params![META_LEGACY_IMPORT, outcome],
    )
    .map_err(|e| format!("Failed to record the task log import state: {}", e))?;
    tx.commit()
        .map_err(|e| format!("Failed to commit the task log import: {}", e))?;

    if outcome == LEGACY_IMPORT_DONE {
        retire_legacy_json(&json_path, &dir.join(LEGACY_JSON_MIGRATED_FILE));
    }
    Ok(())
}

fn legacy_import_state(conn: &Connection) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT value FROM task_log_meta WHERE key = ?1",
        params![META_LEGACY_IMPORT],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| format!("Failed to read the task log import state: {}", e))
}

enum LegacyJson {
    Missing,
    Malformed(String),
    Entries(Vec<PersistedTaskLogEntry>),
}

fn read_legacy_json(path: &Path) -> Result<LegacyJson, String> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(LegacyJson::Missing),
        // Non-UTF-8 content is a malformed file, not a transient read failure.
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
            return Ok(LegacyJson::Malformed(e.to_string()))
        }
        Err(e) => {
            return Err(format!(
                "Failed to read the legacy task log {}: {}",
                path.display(),
                e
            ))
        }
    };
    match serde_json::from_str::<Vec<PersistedTaskLogEntry>>(&raw) {
        Ok(entries) => Ok(LegacyJson::Entries(entries)),
        Err(e) => Ok(LegacyJson::Malformed(e.to_string())),
    }
}

/// Rename the imported JSON file out of the way. Best effort: the entries are
/// already committed, so a failure here must not fail the caller.
fn retire_legacy_json(json_path: &Path, migrated_path: &Path) {
    if migrated_path.exists() {
        // Renaming would overwrite an earlier backup.
        eprintln!(
            "[task-log] {} already exists, so {} was imported and left in place",
            migrated_path.display(),
            json_path.display()
        );
        return;
    }
    if let Err(e) = fs::rename(json_path, migrated_path) {
        eprintln!(
            "[task-log] imported {} but could not rename it to {}: {}",
            json_path.display(),
            migrated_path.display(),
            e
        );
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Read all persisted entries, oldest first. Returns an empty Vec when
/// nothing has been logged yet.
///
/// The first call after an upgrade also imports the legacy `task_log.json`,
/// because the frontend reads the log before it writes to it.
pub fn read_task_log() -> Result<Vec<PersistedTaskLogEntry>, String> {
    let conn = open_conn()?;
    read_entries(&conn)
}

/// Write a batch of entries, then prune to the newest `MAX_ENTRIES` entries.
/// An entry whose `id` is already stored replaces that entry and keeps its
/// position. New entries are added at the end.
///
/// The whole batch is written in one transaction.
pub fn append_task_log_entries(new_entries: Vec<PersistedTaskLogEntry>) -> Result<(), String> {
    let mut conn = open_conn()?;
    let tx = begin_write(&mut conn)?;
    upsert_entries(&tx, &new_entries)?;
    tx.commit()
        .map_err(|e| format!("Failed to commit task log entries: {}", e))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::paths::with_test_home;
    use tempfile::tempdir;

    fn entry(id: &str, message: &str, status: &str) -> PersistedTaskLogEntry {
        PersistedTaskLogEntry {
            id: id.to_string(),
            timestamp: "2026-10-12T00:00:00.000Z".to_string(),
            message: message.to_string(),
            status: status.to_string(),
            agent: None,
        }
    }

    fn ids(entries: &[PersistedTaskLogEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.id.as_str()).collect()
    }

    /// Run `test` with an empty temporary home. Passes the Automatic data
    /// directory inside it, already created.
    fn in_test_home<T>(test: impl FnOnce(&Path) -> T) -> T {
        let home = tempdir().unwrap();
        with_test_home(home.path().to_path_buf(), || {
            let dir = get_automatic_dir().unwrap();
            fs::create_dir_all(&dir).unwrap();
            test(&dir)
        })
    }

    fn write_legacy_json(dir: &Path, entries: &[PersistedTaskLogEntry]) {
        fs::write(
            dir.join(LEGACY_JSON_FILE),
            serde_json::to_string_pretty(entries).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn read_is_empty_before_anything_is_logged() {
        in_test_home(|dir| {
            assert!(read_task_log().unwrap().is_empty());
            assert!(dir.join(DB_FILE).exists());
        });
    }

    #[test]
    fn append_adds_entries_in_order_across_calls() {
        in_test_home(|_| {
            append_task_log_entries(vec![entry("a", "first", "info")]).unwrap();
            append_task_log_entries(vec![
                entry("b", "second", "info"),
                entry("c", "third", "info"),
            ])
            .unwrap();

            let entries = read_task_log().unwrap();
            assert_eq!(ids(&entries), vec!["a", "b", "c"]);
            assert_eq!(entries[0], entry("a", "first", "info"));
        });
    }

    #[test]
    fn agent_round_trips_and_is_omitted_from_json_when_absent() {
        in_test_home(|_| {
            let mut with_agent = entry("a", "by an agent", "success");
            with_agent.agent = Some("Claude".to_string());
            append_task_log_entries(vec![with_agent.clone(), entry("b", "no agent", "info")])
                .unwrap();

            let entries = read_task_log().unwrap();
            assert_eq!(entries[0], with_agent);
            assert_eq!(entries[1].agent, None);
            let json = serde_json::to_value(&entries[1]).unwrap();
            assert!(json.get("agent").is_none());
        });
    }

    #[test]
    fn existing_id_is_updated_in_place_and_keeps_its_position() {
        in_test_home(|_| {
            append_task_log_entries(vec![
                entry("a", "start", "running"),
                entry("b", "other", "info"),
            ])
            .unwrap();

            let mut finished = entry("a", "done", "success");
            finished.agent = Some("Claude".to_string());
            append_task_log_entries(vec![finished.clone(), entry("c", "later", "info")]).unwrap();

            let entries = read_task_log().unwrap();
            assert_eq!(ids(&entries), vec!["a", "b", "c"]);
            assert_eq!(entries[0], finished);
        });
    }

    #[test]
    fn duplicate_id_within_one_batch_keeps_the_last_value() {
        in_test_home(|_| {
            append_task_log_entries(vec![
                entry("a", "start", "running"),
                entry("a", "done", "success"),
            ])
            .unwrap();

            let entries = read_task_log().unwrap();
            assert_eq!(entries, vec![entry("a", "done", "success")]);
        });
    }

    #[test]
    fn prunes_to_the_newest_max_entries() {
        in_test_home(|_| {
            let batch: Vec<_> = (0..MAX_ENTRIES + 10)
                .map(|i| entry(&format!("e{}", i), "m", "info"))
                .collect();
            append_task_log_entries(batch).unwrap();
            append_task_log_entries(vec![entry("last", "m", "info")]).unwrap();

            let entries = read_task_log().unwrap();
            assert_eq!(entries.len(), MAX_ENTRIES);
            assert_eq!(entries[0].id, "e11");
            assert_eq!(entries[MAX_ENTRIES - 1].id, "last");
        });
    }

    #[test]
    fn updating_an_old_entry_does_not_save_it_from_pruning() {
        in_test_home(|_| {
            let batch: Vec<_> = (0..MAX_ENTRIES)
                .map(|i| entry(&format!("e{}", i), "m", "info"))
                .collect();
            append_task_log_entries(batch).unwrap();

            // The update keeps "e0" in the oldest position, so the new entry
            // pushes it out.
            append_task_log_entries(vec![entry("e0", "updated", "success")]).unwrap();
            append_task_log_entries(vec![entry("new", "m", "info")]).unwrap();

            let entries = read_task_log().unwrap();
            assert_eq!(entries.len(), MAX_ENTRIES);
            assert_eq!(entries[0].id, "e1");
            assert_eq!(entries[MAX_ENTRIES - 1].id, "new");
        });
    }

    #[test]
    fn legacy_json_is_imported_once_and_renamed() {
        in_test_home(|dir| {
            let mut second = entry("b", "second", "success");
            second.agent = Some("Claude".to_string());
            let legacy = vec![entry("a", "first", "info"), second];
            write_legacy_json(dir, &legacy);

            assert_eq!(read_task_log().unwrap(), legacy);
            assert!(!dir.join(LEGACY_JSON_FILE).exists());
            assert!(dir.join(LEGACY_JSON_MIGRATED_FILE).exists());

            append_task_log_entries(vec![entry("c", "third", "info")]).unwrap();
            assert_eq!(ids(&read_task_log().unwrap()), vec!["a", "b", "c"]);
        });
    }

    #[test]
    fn append_before_any_read_imports_legacy_entries_first() {
        in_test_home(|dir| {
            write_legacy_json(dir, &[entry("a", "old", "info")]);

            append_task_log_entries(vec![entry("b", "new", "info")]).unwrap();

            assert_eq!(ids(&read_task_log().unwrap()), vec!["a", "b"]);
        });
    }

    #[test]
    fn second_import_run_is_a_no_op() {
        in_test_home(|dir| {
            write_legacy_json(dir, &[entry("a", "old", "info")]);
            assert_eq!(read_task_log().unwrap().len(), 1);

            // A JSON file that appears after the import (for example one
            // restored from the backup) must not be imported again.
            write_legacy_json(
                dir,
                &[entry("a", "changed", "error"), entry("z", "extra", "info")],
            );

            assert_eq!(read_task_log().unwrap(), vec![entry("a", "old", "info")]);
            assert!(dir.join(LEGACY_JSON_FILE).exists());
        });
    }

    #[test]
    fn import_interrupted_before_commit_runs_again_without_duplicates() {
        in_test_home(|dir| {
            let legacy = vec![entry("a", "first", "info"), entry("b", "second", "info")];
            write_legacy_json(dir, &legacy);

            // A crash mid-import: entries written, transaction never
            // committed. Dropping the transaction rolls it back.
            {
                let mut conn = open_conn_at(&dir.join(DB_FILE)).unwrap();
                let tx = begin_write(&mut conn).unwrap();
                upsert_entries(&tx, &legacy).unwrap();
            }

            assert_eq!(read_task_log().unwrap(), legacy);
            assert!(dir.join(LEGACY_JSON_MIGRATED_FILE).exists());
        });
    }

    #[test]
    fn import_interrupted_after_commit_is_not_repeated() {
        in_test_home(|dir| {
            let legacy = vec![entry("a", "first", "info")];
            write_legacy_json(dir, &legacy);

            // A crash after the commit and before the rename: the marker is
            // set and the JSON file is still in place.
            {
                let mut conn = open_conn_at(&dir.join(DB_FILE)).unwrap();
                let tx = begin_write(&mut conn).unwrap();
                upsert_entries(&tx, &legacy).unwrap();
                tx.execute(
                    "INSERT INTO task_log_meta (key, value) VALUES (?1, ?2)",
                    params![META_LEGACY_IMPORT, LEGACY_IMPORT_DONE],
                )
                .unwrap();
                tx.commit().unwrap();
            }

            append_task_log_entries(vec![entry("b", "second", "info")]).unwrap();

            assert_eq!(ids(&read_task_log().unwrap()), vec!["a", "b"]);
            assert!(dir.join(LEGACY_JSON_FILE).exists());
        });
    }

    #[test]
    fn legacy_import_keeps_only_the_newest_max_entries() {
        in_test_home(|dir| {
            let legacy: Vec<_> = (0..MAX_ENTRIES + 5)
                .map(|i| entry(&format!("e{}", i), "m", "info"))
                .collect();
            write_legacy_json(dir, &legacy);

            let entries = read_task_log().unwrap();
            assert_eq!(entries.len(), MAX_ENTRIES);
            assert_eq!(entries[0].id, "e5");
        });
    }

    #[test]
    fn malformed_legacy_json_is_skipped_and_left_untouched() {
        in_test_home(|dir| {
            let json_path = dir.join(LEGACY_JSON_FILE);
            fs::write(&json_path, "{ not a task log").unwrap();

            assert!(read_task_log().unwrap().is_empty());
            append_task_log_entries(vec![entry("a", "new", "info")]).unwrap();

            assert_eq!(ids(&read_task_log().unwrap()), vec!["a"]);
            assert_eq!(fs::read_to_string(&json_path).unwrap(), "{ not a task log");
            assert!(!dir.join(LEGACY_JSON_MIGRATED_FILE).exists());

            let conn = open_conn_at(&dir.join(DB_FILE)).unwrap();
            assert_eq!(
                legacy_import_state(&conn).unwrap().as_deref(),
                Some(LEGACY_IMPORT_SKIPPED_MALFORMED)
            );
        });
    }

    #[test]
    fn existing_migrated_backup_is_never_overwritten() {
        in_test_home(|dir| {
            fs::write(dir.join(LEGACY_JSON_MIGRATED_FILE), "earlier backup").unwrap();
            write_legacy_json(dir, &[entry("a", "old", "info")]);

            assert_eq!(ids(&read_task_log().unwrap()), vec!["a"]);
            assert_eq!(
                fs::read_to_string(dir.join(LEGACY_JSON_MIGRATED_FILE)).unwrap(),
                "earlier backup"
            );
            assert!(dir.join(LEGACY_JSON_FILE).exists());
        });
    }

    #[test]
    fn unreadable_legacy_json_is_an_error_and_is_retried() {
        in_test_home(|dir| {
            // A directory at the file's path makes the read fail with an
            // error that is neither "not found" nor "invalid data".
            let json_path = dir.join(LEGACY_JSON_FILE);
            fs::create_dir(&json_path).unwrap();

            let err = read_task_log().unwrap_err();
            assert!(
                err.contains("Failed to read the legacy task log"),
                "{}",
                err
            );

            fs::remove_dir(&json_path).unwrap();
            write_legacy_json(dir, &[entry("a", "old", "info")]);
            assert_eq!(ids(&read_task_log().unwrap()), vec!["a"]);
        });
    }
}
