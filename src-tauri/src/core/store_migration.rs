//! Startup migration: move per-project stores from name keys to project
//! keys.
//!
//! Stage 3b step 2 of the project identity plan
//! (`automatic-meta/general/plans/projects/project-identity.md`). Runs from
//! `ensure_project_keys`, under its lock, after every registry entry has its
//! keys. Re-keys:
//!
//! | Store | From | To |
//! |---|---|---|
//! | `memory/<name>.json` | name | `<id>.json`, merged when it exists |
//! | `features.db` `features`, `feature_updates` | name | `id` |
//! | `activity.db` `activity`, `recommendations`, `recommendation_meta` | name | `local_key` |
//! | `groups/*.json` members | name | `id` |
//! | `dev-servers/<name>.json` | name | `<local_key>.json` |
//!
//! Only data for a registered project that has both keys moves. A stored
//! value maps to a project when it equals the project's name ignoring case,
//! like the registry resolver. A value two projects' names match is left
//! alone and reported. Everything else (orphans, values already keyed) is
//! left alone, so the pass is idempotent and runs on every start: a value
//! written under a name by an older process is picked up next time.
//!
//! **Backups.** Before a store changes, it is copied into
//! `backups/identity-<UTC timestamp>/` under the Automatic data directory:
//! directories file by file, databases with SQLite's `VACUUM INTO` (a
//! consistent copy of a live database; a raw file copy of a WAL database is
//! not). Only stores with something to migrate are copied, and no backup
//! directory is made when nothing needs migrating. A store whose backup
//! fails is not migrated; the failure is reported.
//!
//! **Failure policy.** Problems are collected in the report and never stop
//! the other stores. A memory file that does not parse is reported and left
//! alone: merging it as empty would lose data.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{Map, Value};

use super::*;

/// What one run of [`migrate_project_stores`] did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StoreMigrationReport {
    /// Where the stores that changed were copied first. `None` when nothing
    /// needed migrating.
    pub backup_dir: Option<PathBuf>,
    /// Memory files merged into their project's `id` file.
    pub memory_files: usize,
    /// `features` and `feature_updates` rows re-keyed.
    pub feature_rows: usize,
    /// `activity` rows re-keyed.
    pub activity_rows: usize,
    /// `recommendations` and `recommendation_meta` rows re-keyed.
    pub recommendation_rows: usize,
    /// Group files whose members were rewritten.
    pub group_files: usize,
    /// Dev-server files renamed to their project's `local_key`.
    pub dev_server_files: usize,
    /// Everything that was skipped or failed, one line each.
    pub problems: Vec<String>,
}

impl StoreMigrationReport {
    /// Whether any store changed.
    pub fn changed_anything(&self) -> bool {
        self.memory_files
            + self.feature_rows
            + self.activity_rows
            + self.recommendation_rows
            + self.group_files
            + self.dev_server_files
            > 0
    }
}

/// A registered project whose data can move: it has both keys.
#[derive(Debug, Clone)]
struct Target {
    id: String,
    local_key: String,
}

/// Maps stored values to projects. See the module docs for the rules.
struct Targets {
    all: Vec<ProjectSummary>,
}

impl Targets {
    /// The project whose data is stored under `value`, if it can move.
    /// `Err` when two registered projects' names match `value`.
    fn for_value(&self, value: &str) -> Result<Option<Target>, String> {
        if value.is_empty() || self.all.iter().any(|s| s.id == value || s.local_key == value) {
            return Ok(None);
        }
        let named: Vec<&ProjectSummary> = self
            .all
            .iter()
            .filter(|s| same_project_name(&s.name, value))
            .collect();
        match named.as_slice() {
            [] => Ok(None),
            [only] if !only.id.is_empty() && !only.local_key.is_empty() => Ok(Some(Target {
                id: only.id.clone(),
                local_key: only.local_key.clone(),
            })),
            [_] => Ok(None),
            many => Err(format!(
                "'{}' matches more than one project ({}); its data was left under the name",
                value,
                many.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ")
            )),
        }
    }

    /// `for_value`, with an ambiguous value reported and treated as
    /// unmapped.
    fn map_reporting(&self, value: &str, problems: &mut Vec<String>) -> Option<Target> {
        match self.for_value(value) {
            Ok(target) => target,
            Err(e) => {
                if !problems.contains(&e) {
                    problems.push(e);
                }
                None
            }
        }
    }
}

/// Re-key every per-project store under the user's Automatic directory.
/// Call with the registry lock held.
pub(crate) fn migrate_project_stores(summaries: Vec<ProjectSummary>) -> StoreMigrationReport {
    match get_automatic_dir() {
        Ok(dir) => migrate_project_stores_in(&dir, summaries, chrono::Utc::now()),
        Err(e) => StoreMigrationReport {
            problems: vec![format!("Could not locate the Automatic directory: {}", e)],
            ..Default::default()
        },
    }
}

/// [`migrate_project_stores`] against an explicit data directory. `now`
/// names the backup directory.
pub(crate) fn migrate_project_stores_in(
    automatic_dir: &Path,
    summaries: Vec<ProjectSummary>,
    now: chrono::DateTime<chrono::Utc>,
) -> StoreMigrationReport {
    let targets = Targets { all: summaries };
    let mut report = StoreMigrationReport::default();
    let mut backup = Backup::new(automatic_dir, now);

    migrate_memory(&automatic_dir.join("memory"), &targets, &mut backup, &mut report);
    migrate_features_db(&automatic_dir.join("features.db"), &targets, &mut backup, &mut report);
    migrate_activity_db(&automatic_dir.join("activity.db"), &targets, &mut backup, &mut report);
    migrate_groups(&automatic_dir.join("groups"), &targets, &mut backup, &mut report);
    migrate_dev_servers(&automatic_dir.join("dev-servers"), &targets, &mut backup, &mut report);

    report.backup_dir = backup.created;
    report
}

// ── Backups ─────────────────────────────────────────────────────────────────

/// The run's backup directory, made on first use.
struct Backup {
    root: PathBuf,
    stamp: String,
    created: Option<PathBuf>,
}

impl Backup {
    fn new(automatic_dir: &Path, now: chrono::DateTime<chrono::Utc>) -> Self {
        Self {
            root: automatic_dir.join("backups"),
            stamp: now.format("%Y%m%dT%H%M%SZ").to_string(),
            created: None,
        }
    }

    /// The backup directory, created on the first call. A directory left by
    /// an earlier run in the same second gets a numeric suffix.
    fn dir(&mut self) -> Result<PathBuf, String> {
        if let Some(dir) = &self.created {
            return Ok(dir.clone());
        }
        fs::create_dir_all(&self.root)
            .map_err(|e| format!("Could not create {}: {}", self.root.display(), e))?;
        for attempt in 1..1000 {
            let name = if attempt == 1 {
                format!("identity-{}", self.stamp)
            } else {
                format!("identity-{}-{}", self.stamp, attempt)
            };
            let dir = self.root.join(name);
            match fs::create_dir(&dir) {
                Ok(()) => {
                    self.created = Some(dir.clone());
                    return Ok(dir);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("Could not create {}: {}", dir.display(), e)),
            }
        }
        Err(format!("Could not pick a free backup directory in {}", self.root.display()))
    }

    /// Copy every file in `source` into `<backup>/<label>/`.
    fn copy_dir(&mut self, source: &Path, label: &str) -> Result<(), String> {
        let target = self.dir()?.join(label);
        copy_dir_files(source, &target)
    }

    /// Write a consistent copy of the open database to `<backup>/<label>`.
    fn copy_db(&mut self, conn: &Connection, label: &str) -> Result<(), String> {
        let target = self.dir()?.join(label);
        let target_str = target
            .to_str()
            .ok_or_else(|| format!("Backup path {} is not valid UTF-8", target.display()))?;
        conn.execute("VACUUM INTO ?1", params![target_str])
            .map_err(|e| format!("Could not back up to {}: {}", target.display(), e))?;
        Ok(())
    }
}

fn copy_dir_files(source: &Path, target: &Path) -> Result<(), String> {
    fs::create_dir_all(target).map_err(|e| format!("Could not create {}: {}", target.display(), e))?;
    let entries =
        fs::read_dir(source).map_err(|e| format!("Could not read {}: {}", source.display(), e))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Could not read {}: {}", source.display(), e))?;
        let path = entry.path();
        if path.is_dir() {
            copy_dir_files(&path, &target.join(entry.file_name()))?;
        } else if path.is_file() {
            fs::copy(&path, target.join(entry.file_name()))
                .map_err(|e| format!("Could not copy {}: {}", path.display(), e))?;
        }
    }
    Ok(())
}

// ── Shared file helpers ─────────────────────────────────────────────────────

/// Stems of the `*.json` files in `dir`, sorted. A missing directory has
/// none.
fn json_stems(dir: &Path) -> Result<Vec<String>, String> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut stems = Vec::new();
    let entries = fs::read_dir(dir).map_err(|e| format!("Could not read {}: {}", dir.display(), e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            if is_valid_name(stem) {
                stems.push(stem.to_string());
            }
        }
    }
    stems.sort();
    Ok(stems)
}

/// Write `contents` to `path` through a temporary file and a rename, so a
/// crash leaves either the old file or the new one, never half of one.
fn write_atomically(path: &Path, contents: &str) -> Result<(), String> {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("Invalid path {}", path.display()))?;
    let temp = path.with_file_name(format!(".{}.migrating", file_name));
    fs::write(&temp, contents).map_err(|e| format!("Could not write {}: {}", temp.display(), e))?;
    fs::rename(&temp, path).map_err(|e| {
        let _ = fs::remove_file(&temp);
        format!("Could not move {} to {}: {}", temp.display(), path.display(), e)
    })
}

// ── Memory ──────────────────────────────────────────────────────────────────

fn migrate_memory(dir: &Path, targets: &Targets, backup: &mut Backup, report: &mut StoreMigrationReport) {
    let stems = match json_stems(dir) {
        Ok(stems) => stems,
        Err(e) => {
            report.problems.push(format!("memory: {}", e));
            return;
        }
    };
    let pending: Vec<(String, Target)> = stems
        .into_iter()
        .filter_map(|stem| {
            targets
                .map_reporting(&stem, &mut report.problems)
                .map(|t| (stem, t))
        })
        .collect();
    if pending.is_empty() {
        return;
    }
    if let Err(e) = backup.copy_dir(dir, "memory") {
        report.problems.push(format!("memory: not migrated because the backup failed: {}", e));
        return;
    }
    for (stem, target) in pending {
        match merge_memory_file(dir, &stem, &target) {
            Ok(()) => report.memory_files += 1,
            Err(e) => report.problems.push(format!("memory: {}", e)),
        }
    }
}

/// Merge `<dir>/<stem>.json` into `<dir>/<id>.json` and remove the source.
/// The target is written before the source is removed. A file that does not
/// parse as a JSON object is an error and both files are left alone.
fn merge_memory_file(dir: &Path, stem: &str, target: &Target) -> Result<(), String> {
    let source_path = dir.join(format!("{}.json", stem));
    let target_path = dir.join(format!("{}.json", target.id));
    let incoming = read_memory_object(&source_path)?;
    let existing = if target_path.exists() {
        read_memory_object(&target_path)?
    } else {
        Map::new()
    };
    let merged = merge_memory_entries(existing, incoming, stem);
    let pretty = serde_json::to_string_pretty(&Value::Object(merged))
        .map_err(|e| format!("Could not serialise {}: {}", target_path.display(), e))?;
    write_atomically(&target_path, &pretty)?;
    fs::remove_file(&source_path).map_err(|e| {
        format!(
            "Merged {} into {} but could not remove it: {}",
            source_path.display(),
            target_path.display(),
            e
        )
    })
}

fn read_memory_object(path: &Path) -> Result<Map<String, Value>, String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("Could not read {}: {}", path.display(), e))?;
    match serde_json::from_str::<Value>(&raw) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err(format!("{} is not a JSON object; left unchanged", path.display())),
        Err(e) => Err(format!("{} does not parse ({}); left unchanged", path.display(), e)),
    }
}

/// Merge memory entries from `incoming` (stored under the name `from`) into
/// `existing`. A key not yet present is copied. A key present with the same
/// `value` is a duplicate and dropped. A key present with a different value
/// keeps the existing entry and stores the incoming one under
/// `<key>@<from>`, then `<key>@<from>-2` and so on. Does no I/O.
fn merge_memory_entries(
    mut existing: Map<String, Value>,
    incoming: Map<String, Value>,
    from: &str,
) -> Map<String, Value> {
    let value_of = |entry: &Value| entry.get("value").cloned();
    let mut keys: Vec<&String> = incoming.keys().collect();
    keys.sort();
    for key in keys {
        let entry = &incoming[key];
        let mut candidate = key.clone();
        let mut attempt = 1;
        loop {
            match existing.get(&candidate) {
                None => {
                    existing.insert(candidate, entry.clone());
                    break;
                }
                Some(current) if value_of(current) == value_of(entry) => break,
                Some(_) => {
                    attempt += 1;
                    candidate = if attempt == 2 {
                        format!("{}@{}", key, from)
                    } else {
                        format!("{}@{}-{}", key, from, attempt - 1)
                    };
                }
            }
        }
    }
    existing
}

// ── SQLite stores ───────────────────────────────────────────────────────────

fn open_existing_db(path: &Path) -> Result<Option<Connection>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    let conn = Connection::open(path).map_err(|e| format!("Could not open {}: {}", path.display(), e))?;
    // The GUI and `mcp-serve` processes share these databases.
    conn.busy_timeout(Duration::from_secs(10))
        .map_err(|e| format!("Could not configure {}: {}", path.display(), e))?;
    Ok(Some(conn))
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool, String> {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
        params![table],
        |_| Ok(()),
    )
    .optional()
    .map(|found| found.is_some())
    .map_err(|e| format!("Could not inspect table {}: {}", table, e))
}

/// Distinct `project` values across `tables` (each must exist).
fn distinct_projects(conn: &Connection, tables: &[&str]) -> Result<Vec<String>, String> {
    let mut values: Vec<String> = Vec::new();
    for table in tables {
        let sql = format!("SELECT DISTINCT project FROM {}", table);
        let mut stmt = conn.prepare(&sql).map_err(|e| format!("Could not read {}: {}", table, e))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| format!("Could not read {}: {}", table, e))?;
        for row in rows {
            let value = row.map_err(|e| format!("Could not read {}: {}", table, e))?;
            if !values.contains(&value) {
                values.push(value);
            }
        }
    }
    values.sort();
    Ok(values)
}

/// Stored values in `tables` that map to a project, with that project.
fn pending_db_values(
    conn: &Connection,
    tables: &[&str],
    targets: &Targets,
    problems: &mut Vec<String>,
) -> Result<Vec<(String, Target)>, String> {
    Ok(distinct_projects(conn, tables)?
        .into_iter()
        .filter_map(|value| targets.map_reporting(&value, problems).map(|t| (value, t)))
        .collect())
}

fn migrate_features_db(path: &Path, targets: &Targets, backup: &mut Backup, report: &mut StoreMigrationReport) {
    let result = (|| -> Result<(), String> {
        let Some(mut conn) = open_existing_db(path)? else {
            return Ok(());
        };
        let tables: Vec<&str> = ["features", "feature_updates"]
            .into_iter()
            .filter(|t| table_exists(&conn, t).unwrap_or(false))
            .collect();
        if tables.is_empty() {
            return Ok(());
        }
        let pending = pending_db_values(&conn, &tables, targets, &mut report.problems)?;
        if pending.is_empty() {
            return Ok(());
        }
        backup
            .copy_db(&conn, "features.db")
            .map_err(|e| format!("not migrated because the backup failed: {}", e))?;
        let tx = conn.transaction().map_err(|e| format!("Could not start a transaction: {}", e))?;
        let mut changed = 0;
        for (value, target) in &pending {
            for table in &tables {
                changed += tx
                    .execute(
                        &format!("UPDATE {} SET project = ?1 WHERE project = ?2", table),
                        params![target.id, value],
                    )
                    .map_err(|e| format!("Could not re-key {} for '{}': {}", table, value, e))?;
            }
        }
        tx.commit().map_err(|e| format!("Could not commit: {}", e))?;
        report.feature_rows += changed;
        Ok(())
    })();
    if let Err(e) = result {
        report.problems.push(format!("features.db: {}", e));
    }
}

fn migrate_activity_db(path: &Path, targets: &Targets, backup: &mut Backup, report: &mut StoreMigrationReport) {
    let result = (|| -> Result<(), String> {
        let Some(mut conn) = open_existing_db(path)? else {
            return Ok(());
        };
        let row_tables: Vec<&str> = ["activity", "recommendations"]
            .into_iter()
            .filter(|t| table_exists(&conn, t).unwrap_or(false))
            .collect();
        let has_meta = table_exists(&conn, "recommendation_meta")?;
        let mut scanned = row_tables.clone();
        if has_meta {
            scanned.push("recommendation_meta");
        }
        if scanned.is_empty() {
            return Ok(());
        }
        let pending = pending_db_values(&conn, &scanned, targets, &mut report.problems)?;
        if pending.is_empty() {
            return Ok(());
        }
        backup
            .copy_db(&conn, "activity.db")
            .map_err(|e| format!("not migrated because the backup failed: {}", e))?;
        let tx = conn.transaction().map_err(|e| format!("Could not start a transaction: {}", e))?;
        let (mut activity, mut recommendations) = (0, 0);
        for (value, target) in &pending {
            for table in &row_tables {
                let changed = tx
                    .execute(
                        &format!("UPDATE {} SET project = ?1 WHERE project = ?2", table),
                        params![target.local_key, value],
                    )
                    .map_err(|e| format!("Could not re-key {} for '{}': {}", table, value, e))?;
                if *table == "activity" {
                    activity += changed;
                } else {
                    recommendations += changed;
                }
            }
            if has_meta {
                // `project` is the primary key: a row already under the key
                // (written after an earlier migration) keeps the later run.
                recommendations += tx
                    .execute(
                        "INSERT INTO recommendation_meta (project, last_ai_run)
                         SELECT ?1, last_ai_run FROM recommendation_meta WHERE project = ?2
                         ON CONFLICT(project) DO UPDATE
                           SET last_ai_run = MAX(last_ai_run, excluded.last_ai_run)",
                        params![target.local_key, value],
                    )
                    .map_err(|e| format!("Could not re-key recommendation_meta for '{}': {}", value, e))?;
                tx.execute("DELETE FROM recommendation_meta WHERE project = ?1", params![value])
                    .map_err(|e| format!("Could not re-key recommendation_meta for '{}': {}", value, e))?;
            }
        }
        tx.commit().map_err(|e| format!("Could not commit: {}", e))?;
        report.activity_rows += activity;
        report.recommendation_rows += recommendations;
        Ok(())
    })();
    if let Err(e) = result {
        report.problems.push(format!("activity.db: {}", e));
    }
}

// ── Groups ──────────────────────────────────────────────────────────────────

fn migrate_groups(dir: &Path, targets: &Targets, backup: &mut Backup, report: &mut StoreMigrationReport) {
    let stems = match json_stems(dir) {
        Ok(stems) => stems,
        Err(e) => {
            report.problems.push(format!("groups: {}", e));
            return;
        }
    };
    let mut rewrites: Vec<(PathBuf, Value)> = Vec::new();
    for stem in stems {
        let path = dir.join(format!("{}.json", stem));
        let parsed = fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|raw| serde_json::from_str::<Value>(&raw).map_err(|e| e.to_string()));
        let mut value = match parsed {
            Ok(value) => value,
            Err(e) => {
                report
                    .problems
                    .push(format!("groups: {} left unchanged: {}", path.display(), e));
                continue;
            }
        };
        let Some(members) = value.get_mut("projects").and_then(|p| p.as_array_mut()) else {
            continue;
        };
        let mut next: Vec<Value> = Vec::new();
        let mut changed = false;
        for member in members.iter() {
            let rewritten = match member.as_str() {
                Some(name) => match targets.map_reporting(name, &mut report.problems) {
                    Some(target) => {
                        changed = true;
                        Value::String(target.id)
                    }
                    None => member.clone(),
                },
                None => member.clone(),
            };
            if next.contains(&rewritten) {
                changed = true;
            } else {
                next.push(rewritten);
            }
        }
        if changed {
            *members = next;
            rewrites.push((path, value));
        }
    }
    if rewrites.is_empty() {
        return;
    }
    if let Err(e) = backup.copy_dir(dir, "groups") {
        report.problems.push(format!("groups: not migrated because the backup failed: {}", e));
        return;
    }
    for (path, value) in rewrites {
        let written = serde_json::to_string_pretty(&value)
            .map_err(|e| e.to_string())
            .and_then(|pretty| write_atomically(&path, &pretty));
        match written {
            Ok(()) => report.group_files += 1,
            Err(e) => report.problems.push(format!("groups: {}: {}", path.display(), e)),
        }
    }
}

// ── Dev servers ─────────────────────────────────────────────────────────────

fn migrate_dev_servers(dir: &Path, targets: &Targets, backup: &mut Backup, report: &mut StoreMigrationReport) {
    let stems = match json_stems(dir) {
        Ok(stems) => stems,
        Err(e) => {
            report.problems.push(format!("dev-servers: {}", e));
            return;
        }
    };
    let pending: Vec<(String, Target)> = stems
        .into_iter()
        .filter_map(|stem| {
            targets
                .map_reporting(&stem, &mut report.problems)
                .map(|t| (stem, t))
        })
        .collect();
    if pending.is_empty() {
        return;
    }
    if let Err(e) = backup.copy_dir(dir, "dev-servers") {
        report
            .problems
            .push(format!("dev-servers: not migrated because the backup failed: {}", e));
        return;
    }
    // A config file holds only server settings, never the project name, so
    // moving the file is the whole migration.
    for (stem, target) in pending {
        let source = dir.join(format!("{}.json", stem));
        let destination = dir.join(format!("{}.json", target.local_key));
        if destination.exists() {
            report.problems.push(format!(
                "dev-servers: {} was not moved because {} already exists; both were left unchanged",
                source.display(),
                destination.display()
            ));
            continue;
        }
        match fs::rename(&source, &destination) {
            Ok(()) => report.dev_server_files += 1,
            Err(e) => report.problems.push(format!(
                "dev-servers: could not move {} to {}: {}",
                source.display(),
                destination.display(),
                e
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ID: &str = "11111111-1111-4111-8111-111111111111";
    const KEY: &str = "aaaaaaaa-0000-4000-8000-000000000001";
    const KEY_2: &str = "aaaaaaaa-0000-4000-8000-000000000002";
    const OTHER_ID: &str = "22222222-2222-4222-8222-222222222222";
    const OTHER_KEY: &str = "bbbbbbbb-0000-4000-8000-000000000001";

    fn summary(name: &str, id: &str, local_key: &str) -> ProjectSummary {
        ProjectSummary {
            local_key: local_key.into(),
            id: id.into(),
            name: name.into(),
            directory: String::new(),
        }
    }

    /// `Site` and a worktree checkout `site-wt` share `ID`; `api` is another
    /// project; `unkeyed` has no keys.
    fn summaries() -> Vec<ProjectSummary> {
        vec![
            summary("Site", ID, KEY),
            summary("site-wt", ID, KEY_2),
            summary("api", OTHER_ID, OTHER_KEY),
            summary("unkeyed", "", ""),
        ]
    }

    fn now() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339("2026-09-28T01:02:03Z")
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    fn run(dir: &Path) -> StoreMigrationReport {
        migrate_project_stores_in(dir, summaries(), now())
    }

    fn write_json(path: &Path, value: &Value) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, serde_json::to_string_pretty(value).unwrap()).unwrap();
    }

    fn read_json(path: &Path) -> Value {
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    }

    fn entry(value: &str) -> Value {
        json!({"value": value, "timestamp": "2026-01-01T00:00:00Z", "source": null})
    }

    fn memory(dir: &Path, stem: &str) -> PathBuf {
        dir.join("memory").join(format!("{}.json", stem))
    }

    // ── memory ──────────────────────────────────────────────────────────

    #[test]
    fn memory_file_moves_to_the_id() {
        let tmp = tempfile::tempdir().unwrap();
        write_json(&memory(tmp.path(), "Site"), &json!({"a": entry("1")}));

        let report = run(tmp.path());
        assert_eq!(report.memory_files, 1);
        assert!(!memory(tmp.path(), "Site").exists());
        assert_eq!(read_json(&memory(tmp.path(), ID))["a"]["value"], "1");
        assert!(report.problems.is_empty(), "{:?}", report.problems);
    }

    #[test]
    fn memory_of_two_checkouts_merges_and_clashes_get_a_suffix() {
        let tmp = tempfile::tempdir().unwrap();
        write_json(
            &memory(tmp.path(), "Site"),
            &json!({"shared": entry("same"), "k": entry("from site"), "only-site": entry("s")}),
        );
        write_json(
            &memory(tmp.path(), "site-wt"),
            &json!({"shared": entry("same"), "k": entry("from wt"), "k@site-wt": entry("taken"), "only-wt": entry("w")}),
        );

        let report = run(tmp.path());
        assert_eq!(report.memory_files, 2);
        let merged = read_json(&memory(tmp.path(), ID));
        let merged = merged.as_object().unwrap();
        assert_eq!(merged["shared"]["value"], "same", "identical values dedupe");
        assert!(!merged.keys().any(|k| k.starts_with("shared@")));
        assert_eq!(merged["k"]["value"], "from site", "the existing entry is kept");
        assert_eq!(merged["only-site"]["value"], "s");
        assert_eq!(merged["only-wt"]["value"], "w");
        // Keys merge in sorted order: `k` takes `k@site-wt` first, so the
        // incoming `k@site-wt` clashes in turn and gets its own suffix.
        assert_eq!(merged["k@site-wt"]["value"], "from wt");
        assert_eq!(merged["k@site-wt@site-wt"]["value"], "taken");
        assert_eq!(merged.len(), 6);
    }

    #[test]
    fn memory_merge_suffix_counts_up_and_dedupes() {
        let existing = json!({"k": entry("a"), "k@old": entry("b")});
        let incoming = json!({"k": entry("c")});
        let merged = merge_memory_entries(
            existing.as_object().unwrap().clone(),
            incoming.as_object().unwrap().clone(),
            "old",
        );
        assert_eq!(merged["k@old-2"]["value"], "c");

        let again = merge_memory_entries(merged.clone(), incoming.as_object().unwrap().clone(), "old");
        assert_eq!(again, merged, "merging the same entries twice adds nothing");
    }

    #[test]
    fn unparseable_memory_is_reported_and_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("memory")).unwrap();
        fs::write(memory(tmp.path(), "Site"), "{ not json").unwrap();
        write_json(&memory(tmp.path(), "site-wt"), &json!({"a": entry("1")}));
        fs::write(memory(tmp.path(), ID), "[1, 2]").unwrap();

        let report = run(tmp.path());
        assert_eq!(report.memory_files, 0);
        assert_eq!(report.problems.len(), 2, "{:?}", report.problems);
        assert_eq!(fs::read_to_string(memory(tmp.path(), "Site")).unwrap(), "{ not json");
        assert!(memory(tmp.path(), "site-wt").exists(), "a source is kept when the target is bad");
        assert_eq!(fs::read_to_string(memory(tmp.path(), ID)).unwrap(), "[1, 2]");
    }

    #[test]
    fn a_second_run_changes_nothing_and_a_straggler_is_merged() {
        let tmp = tempfile::tempdir().unwrap();
        write_json(&memory(tmp.path(), "Site"), &json!({"a": entry("1")}));
        let first = run(tmp.path());
        assert!(first.backup_dir.is_some());

        let second = run(tmp.path());
        assert_eq!(second, StoreMigrationReport::default(), "idempotent, no second backup");

        // An older process writes under the name again.
        write_json(&memory(tmp.path(), "Site"), &json!({"b": entry("2")}));
        let third = run(tmp.path());
        assert_eq!(third.memory_files, 1);
        let merged = read_json(&memory(tmp.path(), ID));
        assert_eq!(merged["a"]["value"], "1");
        assert_eq!(merged["b"]["value"], "2");
    }

    #[test]
    fn orphans_and_unkeyed_projects_are_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        write_json(&memory(tmp.path(), "ghost"), &json!({"a": entry("1")}));
        write_json(&memory(tmp.path(), "unkeyed"), &json!({"a": entry("1")}));
        let report = run(tmp.path());
        assert_eq!(report, StoreMigrationReport::default());
        assert!(memory(tmp.path(), "ghost").exists() && memory(tmp.path(), "unkeyed").exists());
        assert!(!tmp.path().join("backups").exists(), "no backup when nothing migrates");
    }

    #[test]
    fn a_name_two_projects_share_is_reported_and_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        write_json(&memory(tmp.path(), "dup"), &json!({"a": entry("1")}));
        let report = migrate_project_stores_in(
            tmp.path(),
            vec![summary("dup", ID, KEY), summary("DUP", OTHER_ID, OTHER_KEY)],
            now(),
        );
        assert_eq!(report.memory_files, 0);
        assert_eq!(report.problems.len(), 1, "{:?}", report.problems);
        assert!(memory(tmp.path(), "dup").exists());
    }

    // ── SQLite stores ───────────────────────────────────────────────────

    fn features_db(dir: &Path) -> Connection {
        let conn = Connection::open(dir.join("features.db")).unwrap();
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE features (id TEXT PRIMARY KEY, project TEXT NOT NULL, title TEXT NOT NULL);
             CREATE TABLE feature_updates (id INTEGER PRIMARY KEY AUTOINCREMENT, feature_id TEXT NOT NULL, project TEXT NOT NULL, content TEXT NOT NULL);",
        )
        .unwrap();
        conn
    }

    fn projects_in(conn: &Connection, table: &str) -> Vec<String> {
        let mut stmt = conn
            .prepare(&format!("SELECT project FROM {} ORDER BY rowid", table))
            .unwrap();
        stmt.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect()
    }

    #[test]
    fn feature_rows_move_to_the_id() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = features_db(tmp.path());
        for (id, project) in [("f1", "Site"), ("f2", "site"), ("f3", "site-wt"), ("f4", "ghost"), ("f5", OTHER_ID)] {
            conn.execute("INSERT INTO features VALUES (?1, ?2, 't')", params![id, project]).unwrap();
        }
        conn.execute("INSERT INTO feature_updates (feature_id, project, content) VALUES ('f1', 'SITE', 'x')", [])
            .unwrap();
        drop(conn);

        let report = run(tmp.path());
        assert_eq!(report.feature_rows, 4);
        let conn = Connection::open(tmp.path().join("features.db")).unwrap();
        assert_eq!(projects_in(&conn, "features"), vec![ID, ID, ID, "ghost", OTHER_ID]);
        assert_eq!(projects_in(&conn, "feature_updates"), vec![ID]);
        assert!(tmp.path().join("backups/identity-20260928T010203Z/features.db").is_file());

        let again = run(tmp.path());
        assert_eq!(again.feature_rows, 0, "idempotent");
    }

    #[test]
    fn activity_and_recommendation_rows_move_to_the_local_key() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = Connection::open(tmp.path().join("activity.db")).unwrap();
        conn.execute_batch(
            "CREATE TABLE activity (id INTEGER PRIMARY KEY AUTOINCREMENT, project TEXT NOT NULL, event TEXT NOT NULL);
             CREATE TABLE recommendations (id INTEGER PRIMARY KEY AUTOINCREMENT, project TEXT NOT NULL, title TEXT NOT NULL);
             CREATE TABLE recommendation_meta (project TEXT PRIMARY KEY, last_ai_run TEXT NOT NULL);",
        )
        .unwrap();
        for project in ["Site", "site-wt", "ghost"] {
            conn.execute("INSERT INTO activity (project, event) VALUES (?1, 'e')", params![project]).unwrap();
            conn.execute("INSERT INTO recommendations (project, title) VALUES (?1, 't')", params![project])
                .unwrap();
        }
        conn.execute_batch(
            &format!(
                "INSERT INTO recommendation_meta VALUES ('Site', '2026-01-01T00:00:00Z');
                 INSERT INTO recommendation_meta VALUES ('{}', '2026-02-01T00:00:00Z');
                 INSERT INTO recommendation_meta VALUES ('site-wt', '2026-03-01T00:00:00Z');",
                KEY
            ),
        )
        .unwrap();
        drop(conn);

        let report = run(tmp.path());
        assert_eq!(report.activity_rows, 2);
        let conn = Connection::open(tmp.path().join("activity.db")).unwrap();
        assert_eq!(projects_in(&conn, "activity"), vec![KEY, KEY_2, "ghost"]);
        assert_eq!(projects_in(&conn, "recommendations"), vec![KEY, KEY_2, "ghost"]);
        let meta: Vec<(String, String)> = conn
            .prepare("SELECT project, last_ai_run FROM recommendation_meta ORDER BY project")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(
            meta,
            vec![
                (KEY.to_string(), "2026-02-01T00:00:00Z".to_string()),
                (KEY_2.to_string(), "2026-03-01T00:00:00Z".to_string()),
            ],
            "a key row already present keeps the later run"
        );
        assert!(run(tmp.path()).problems.is_empty());
        assert_eq!(run(tmp.path()).activity_rows, 0, "idempotent");
    }

    // ── groups ──────────────────────────────────────────────────────────

    #[test]
    fn group_members_become_ids() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("groups/team.json");
        write_json(
            &path,
            &json!({"name": "team", "projects": ["site", "site-wt", "ghost", "api", "unkeyed"], "extra": 1}),
        );
        let untouched = tmp.path().join("groups/other.json");
        write_json(&untouched, &json!({"name": "other", "projects": ["ghost"]}));

        let report = run(tmp.path());
        assert_eq!(report.group_files, 1);
        let value = read_json(&path);
        assert_eq!(value["projects"], json!([ID, "ghost", OTHER_ID, "unkeyed"]));
        assert_eq!(value["extra"], 1, "unknown fields survive");
        assert_eq!(read_json(&untouched)["projects"], json!(["ghost"]));
        assert_eq!(run(tmp.path()).group_files, 0, "idempotent");
    }

    // ── dev servers ─────────────────────────────────────────────────────

    #[test]
    fn dev_server_files_move_to_the_local_key() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("dev-servers");
        write_json(&dir.join("Site.json"), &json!([{"id": "s1"}]));
        write_json(&dir.join("api.json"), &json!([{"id": "a1"}]));
        write_json(&dir.join(format!("{}.json", OTHER_KEY)), &json!([{"id": "a2"}]));
        write_json(&dir.join("ghost.json"), &json!([]));

        let report = run(tmp.path());
        assert_eq!(report.dev_server_files, 1);
        assert_eq!(read_json(&dir.join(format!("{}.json", KEY)))[0]["id"], "s1");
        assert!(!dir.join("Site.json").exists());
        assert_eq!(report.problems.len(), 1, "{:?}", report.problems);
        assert!(report.problems[0].contains("already exists"));
        assert!(dir.join("api.json").exists(), "both files are left when the target exists");
        assert_eq!(read_json(&dir.join(format!("{}.json", OTHER_KEY)))[0]["id"], "a2");
        assert!(dir.join("ghost.json").exists());
    }

    // ── backups ─────────────────────────────────────────────────────────

    #[test]
    fn backup_holds_the_stores_that_changed() {
        let tmp = tempfile::tempdir().unwrap();
        write_json(&memory(tmp.path(), "Site"), &json!({"a": entry("1")}));
        write_json(&tmp.path().join("groups/team.json"), &json!({"name": "team", "projects": ["site"]}));
        let conn = features_db(tmp.path());
        conn.execute("INSERT INTO features VALUES ('f1', 'site', 't')", []).unwrap();
        drop(conn);

        let report = run(tmp.path());
        let backup = report.backup_dir.clone().expect("a backup was made");
        assert_eq!(backup, tmp.path().join("backups/identity-20260928T010203Z"));
        assert_eq!(read_json(&backup.join("memory/Site.json"))["a"]["value"], "1");
        assert_eq!(read_json(&backup.join("groups/team.json"))["projects"], json!(["site"]));
        let copy = Connection::open(backup.join("features.db")).unwrap();
        assert_eq!(projects_in(&copy, "features"), vec!["site"], "the copy holds the old keys");
        assert!(!backup.join("dev-servers").exists(), "unchanged stores are not copied");
    }

    #[test]
    fn a_second_backup_in_the_same_second_gets_a_suffix() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("backups/identity-20260928T010203Z")).unwrap();
        write_json(&memory(tmp.path(), "Site"), &json!({"a": entry("1")}));
        let report = run(tmp.path());
        assert_eq!(
            report.backup_dir,
            Some(tmp.path().join("backups/identity-20260928T010203Z-2"))
        );
    }

    #[test]
    fn a_failed_backup_blocks_the_migration() {
        let tmp = tempfile::tempdir().unwrap();
        write_json(&memory(tmp.path(), "Site"), &json!({"a": entry("1")}));
        write_json(&tmp.path().join("groups/team.json"), &json!({"name": "team", "projects": ["site"]}));
        write_json(&tmp.path().join("dev-servers/site.json"), &json!([]));
        let conn = features_db(tmp.path());
        conn.execute("INSERT INTO features VALUES ('f1', 'site', 't')", []).unwrap();
        drop(conn);
        // A file where the backups directory should be.
        fs::write(tmp.path().join("backups"), "").unwrap();

        let report = run(tmp.path());
        assert!(!report.changed_anything(), "{:?}", report);
        assert_eq!(report.problems.len(), 4, "{:?}", report.problems);
        assert!(report.problems.iter().all(|p| p.contains("backup failed")));
        assert!(memory(tmp.path(), "Site").exists());
        assert!(tmp.path().join("dev-servers/site.json").exists());
        assert_eq!(read_json(&tmp.path().join("groups/team.json"))["projects"], json!(["site"]));
        let conn = Connection::open(tmp.path().join("features.db")).unwrap();
        assert_eq!(projects_in(&conn, "features"), vec!["site"]);
    }
}
