use rusqlite::Connection;
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, sync_individual_skills, Agent};

/// OpenCode agent — writes `opencode.json` and stores skills under
/// `<project>/.agents/skills/<name>/SKILL.md`.
pub struct OpenCode;

impl Agent for OpenCode {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "opencode"
    }

    fn label(&self) -> &'static str {
        "OpenCode"
    }

    fn config_description(&self) -> &'static str {
        "opencode.json"
    }

    fn project_file_name(&self) -> &'static str {
        "AGENTS.md"
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_in(&self, dir: &Path) -> bool {
        dir.join("opencode.json").exists()
            || dir.join(".opencode.json").exists()
            || dir.join(".agents").join("skills").exists()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".agents").join("skills")]
    }

    // ── Cleanup ─────────────────────────────────────────────────────────

    fn owned_config_paths(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join("opencode.json")]
    }

    // ── Config writing ──────────────────────────────────────────────────

    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String> {
        let mut oc_servers = Map::new();

        for (name, config) in servers {
            let config = config.clone();
            let transport = config
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("stdio");

            let mut server = Map::new();

            match transport {
                "http" | "sse" => {
                    server.insert("type".to_string(), json!("remote"));

                    if let Some(url) = config.get("url") {
                        server.insert("url".to_string(), url.clone());
                    }
                    if let Some(headers) = config.get("headers") {
                        server.insert("headers".to_string(), headers.clone());
                    }
                    if let Some(oauth) = config.get("oauth") {
                        server.insert("oauth".to_string(), oauth.clone());
                    }
                }
                _ => {
                    // stdio → OpenCode "local"
                    server.insert("type".to_string(), json!("local"));

                    // command as array: [command, ...args]
                    let mut cmd_array: Vec<Value> = Vec::new();
                    if let Some(command) = config.get("command").and_then(|v| v.as_str()) {
                        cmd_array.push(json!(command));
                    }
                    if let Some(args) = config.get("args").and_then(|v| v.as_array()) {
                        for arg in args {
                            cmd_array.push(arg.clone());
                        }
                    }
                    if !cmd_array.is_empty() {
                        server.insert("command".to_string(), Value::Array(cmd_array));
                    }

                    // "environment" instead of "env"
                    if let Some(env) = config.get("env").and_then(|v| v.as_object()) {
                        if !env.is_empty() {
                            server.insert("environment".to_string(), Value::Object(env.clone()));
                        }
                    }
                }
            }

            if let Some(enabled) = config.get("enabled") {
                if enabled.as_bool() == Some(false) {
                    server.insert("enabled".to_string(), json!(false));
                }
            }
            if let Some(timeout) = config.get("timeout") {
                server.insert("timeout".to_string(), timeout.clone());
            }

            oc_servers.insert(name.clone(), Value::Object(server));
        }

        let output = json!({ "$schema": "https://opencode.ai/config.json", "mcp": Value::Object(oc_servers) });
        let path = dir.join("opencode.json");
        let content =
            serde_json::to_string_pretty(&output).map_err(|e| format!("JSON error: {}", e))?;
        fs::write(&path, content).map_err(|e| format!("Failed to write opencode.json: {}", e))?;

        Ok(path.display().to_string())
    }

    fn sync_skills(
        &self,
        dir: &Path,
        skill_contents: &[(String, String)],
        selected_names: &[String],
        local_skill_names: &[String],
    ) -> Result<Vec<String>, String> {
        let mut written = Vec::new();
        let skills_dir = dir.join(".agents").join("skills");
        sync_individual_skills(
            &skills_dir,
            skill_contents,
            selected_names,
            local_skill_names,
            &mut written,
        )?;
        Ok(written)
    }

    // ── MCP capability ──────────────────────────────────────────────────

    fn mcp_note(&self) -> Option<&'static str> {
        Some("OpenCode requires a restart to pick up new MCP servers. After syncing, restart OpenCode for any newly added MCP servers to become available.")
    }

    // ── Discovery ───────────────────────────────────────────────────────

    fn discover_mcp_servers(&self, dir: &Path) -> Map<String, Value> {
        let mut result = Map::new();

        for filename in &["opencode.json", ".opencode.json"] {
            let path = dir.join(filename);
            if path.exists() {
                let found = discover_mcp_servers_from_json(&path, "mcp", normalise_import);
                result.extend(found);
            }
        }

        result
    }

    fn detect_global_install(&self) -> bool {
        super::cli_available("opencode")
            || super::home_dir()
                .map(|h| h.join(".config").join("opencode").exists())
                .unwrap_or(false)
    }

    fn discover_global_mcp_servers(&self) -> Map<String, Value> {
        let Some(home) = super::home_dir() else {
            return Map::new();
        };
        let mut result = Map::new();

        // ~/.opencode.json or ~/opencode.json — global OpenCode config in home dir
        for filename in &[".opencode.json", "opencode.json"] {
            let path = home.join(filename);
            if path.exists() {
                result.extend(discover_mcp_servers_from_json(
                    &path,
                    "mcp",
                    normalise_import,
                ));
            }
        }

        // ~/.config/opencode/config.json — XDG-style global config
        let xdg_path = home.join(".config").join("opencode").join("config.json");
        if xdg_path.exists() {
            result.extend(discover_mcp_servers_from_json(
                &xdg_path,
                "mcp",
                normalise_import,
            ));
        }

        result
    }
}

/// Convert an OpenCode MCP server config to Automatic's canonical format.
///
/// - `type: "local"` → `type: "stdio"`, command array → command + args
/// - `type: "remote"` → `type: "http"`
/// - `environment` → `env`
fn normalise_import(mut config: Value) -> Value {
    if let Some(obj) = config.as_object_mut() {
        if let Some(Value::String(t)) = obj.get("type") {
            if t == "local" {
                obj.insert("type".to_string(), json!("stdio"));
                if let Some(Value::Array(cmd_arr)) = obj.remove("command") {
                    if !cmd_arr.is_empty() {
                        obj.insert("command".to_string(), cmd_arr[0].clone());
                        if cmd_arr.len() > 1 {
                            obj.insert("args".to_string(), Value::Array(cmd_arr[1..].to_vec()));
                        }
                    }
                }
                if let Some(env) = obj.remove("environment") {
                    obj.insert("env".to_string(), env);
                }
            } else if t == "remote" {
                obj.insert("type".to_string(), json!("http"));
            }
        }
    }
    config
}

// ── Cache management ────────────────────────────────────────────────────────

/// Result returned by [`clear_opencode_cache`].
#[derive(Debug, serde::Serialize)]
pub struct ClearCacheResult {
    /// Number of sessions deleted from the database.
    pub sessions_deleted: usize,
    /// Number of orphaned storage files/directories removed.
    pub storage_entries_removed: usize,
    /// Bytes reclaimed (DB size before minus after VACUUM).
    pub bytes_reclaimed: u64,
}

/// Clear stale data from the OpenCode local database.
///
/// ## What gets deleted
///
/// **From the database** (cascade removes messages, parts, todos automatically):
/// - Sessions with `time_archived IS NOT NULL` (explicitly archived by the user)
/// - Sub-agent sessions (`parent_id IS NOT NULL`) whose parent is missing from
///   the DB or is itself archived — opencode never archives these automatically
///
/// **From the filesystem** (storage files that outlive their DB rows):
/// - `storage/session/<project-id>/ses_<id>.json` for deleted sessions
/// - `storage/session_diff/ses_<id>.json` for any session no longer in the DB
/// - `storage/message/<ses_id>/` directories for any session no longer in the DB
/// - `storage/part/<msg_id>/` directories for any message no longer in the DB
///
/// Non-archived top-level sessions are never touched regardless of age.
pub fn clear_opencode_cache() -> Result<ClearCacheResult, String> {
    let data_dir = opencode_data_dir().ok_or_else(|| {
        "Cannot locate OpenCode data directory — home directory unknown".to_string()
    })?;

    let db_path = data_dir.join("opencode.db");
    if !db_path.exists() {
        return Err(format!(
            "OpenCode database not found at {}",
            db_path.display()
        ));
    }

    // ── Open the database ────────────────────────────────────────────────
    let conn = Connection::open(&db_path)
        .map_err(|e| format!("Failed to open OpenCode database: {}", e))?;

    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|e| format!("Failed to enable foreign keys: {}", e))?;

    // ── Determine which sessions to delete ───────────────────────────────
    //
    // Rule 1: explicitly archived sessions.
    // Rule 2: sub-agent sessions whose parent is absent from the DB or archived.
    //   We do NOT delete non-archived top-level sessions regardless of age.
    let ids_to_delete: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT id FROM session
                 WHERE time_archived IS NOT NULL
                 UNION
                 SELECT s.id FROM session s
                 WHERE s.parent_id IS NOT NULL
                   AND (
                     -- parent no longer exists
                     NOT EXISTS (SELECT 1 FROM session p WHERE p.id = s.parent_id)
                     OR
                     -- parent is archived
                     EXISTS (SELECT 1 FROM session p WHERE p.id = s.parent_id AND p.time_archived IS NOT NULL)
                   )",
            )
            .map_err(|e| format!("Failed to prepare session query: {}", e))?;
        let rows: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| format!("Failed to query sessions: {}", e))?
            .filter_map(|r| r.ok())
            .collect();
        rows
    };

    let sessions_deleted = ids_to_delete.len();

    // ── Snapshot surviving message IDs before deleting ───────────────────
    // Used afterwards to identify orphaned part/ directories.
    let surviving_message_ids: std::collections::HashSet<String> = {
        let ids_to_delete_set: std::collections::HashSet<&str> =
            ids_to_delete.iter().map(|s| s.as_str()).collect();
        let mut stmt = conn
            .prepare("SELECT id FROM message")
            .map_err(|e| format!("Failed to query messages: {}", e))?;
        let rows: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| format!("Failed to iterate messages: {}", e))?
            .filter_map(|r| r.ok())
            .collect();
        // Keep only messages whose session is NOT being deleted.
        // We need session_id per message for this filter.
        drop(stmt);
        let mut stmt2 = conn
            .prepare("SELECT id, session_id FROM message")
            .map_err(|e| format!("Failed to query message session_ids: {}", e))?;
        let pairs: Vec<(String, String)> = stmt2
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| format!("Failed to iterate message pairs: {}", e))?
            .filter_map(|r| r.ok())
            .collect();
        let _ = rows; // drop the earlier collect
        pairs
            .into_iter()
            .filter(|(_, session_id)| !ids_to_delete_set.contains(session_id.as_str()))
            .map(|(msg_id, _)| msg_id)
            .collect()
    };

    // ── Snapshot surviving session IDs (after deletion) ──────────────────
    // Used to identify orphaned session_diff/ and message/ entries.
    let surviving_session_ids: std::collections::HashSet<String> = {
        let ids_to_delete_set: std::collections::HashSet<&str> =
            ids_to_delete.iter().map(|s| s.as_str()).collect();
        let mut stmt = conn
            .prepare("SELECT id FROM session")
            .map_err(|e| format!("Failed to query all sessions: {}", e))?;
        let all: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| format!("Failed to iterate sessions: {}", e))?
            .filter_map(|r| r.ok())
            .collect();
        all.into_iter()
            .filter(|id| !ids_to_delete_set.contains(id.as_str()))
            .collect()
    };

    // ── Delete from DB (cascade removes messages, parts, todos) ──────────
    let size_before = db_path.metadata().map(|m| m.len()).unwrap_or(0);

    conn.execute(
        "DELETE FROM session
         WHERE time_archived IS NOT NULL
            OR (
              parent_id IS NOT NULL
              AND (
                NOT EXISTS (SELECT 1 FROM session p WHERE p.id = parent_id)
                OR EXISTS (SELECT 1 FROM session p WHERE p.id = parent_id AND p.time_archived IS NOT NULL)
              )
            )",
        [],
    )
    .map_err(|e| format!("Failed to delete sessions: {}", e))?;

    // ── Scrub orphaned messages and parts (defensive, catches any rows ────
    // that CASCADE missed due to prior incomplete cleanups or pragma state).
    conn.execute(
        "DELETE FROM message WHERE NOT EXISTS (SELECT 1 FROM session s WHERE s.id = session_id)",
        [],
    )
    .map_err(|e| format!("Failed to delete orphaned messages: {}", e))?;

    conn.execute(
        "DELETE FROM part WHERE NOT EXISTS (SELECT 1 FROM session s WHERE s.id = session_id)
                             OR NOT EXISTS (SELECT 1 FROM message m WHERE m.id = message_id)",
        [],
    )
    .map_err(|e| format!("Failed to delete orphaned parts: {}", e))?;

    conn.execute_batch("VACUUM;")
        .map_err(|e| format!("VACUUM failed: {}", e))?;

    drop(conn);

    let size_after = db_path.metadata().map(|m| m.len()).unwrap_or(0);
    let bytes_reclaimed = size_before.saturating_sub(size_after);

    // ── Clean up orphaned storage files ──────────────────────────────────
    let storage_dir = data_dir.join("storage");
    let mut storage_entries_removed = 0usize;

    // storage/session/<project-id>/ses_<id>.json — keyed by session ID
    storage_entries_removed += remove_storage_entries_by_id(
        &storage_dir.join("session"),
        EntryKind::FileInSubdir,
        &surviving_session_ids,
    );

    // storage/session_diff/ses_<id>.json — flat directory, keyed by session ID
    storage_entries_removed += remove_storage_entries_by_id(
        &storage_dir.join("session_diff"),
        EntryKind::FlatFile,
        &surviving_session_ids,
    );

    // storage/message/<ses_id>/ — flat dirs named by session ID
    storage_entries_removed += remove_storage_entries_by_id(
        &storage_dir.join("message"),
        EntryKind::FlatDir,
        &surviving_session_ids,
    );

    // storage/part/<msg_id>/ — flat dirs named by message ID
    storage_entries_removed += remove_storage_entries_by_id(
        &storage_dir.join("part"),
        EntryKind::FlatDir,
        &surviving_message_ids,
    );

    Ok(ClearCacheResult {
        sessions_deleted,
        storage_entries_removed,
        bytes_reclaimed,
    })
}

/// Describes the layout of a storage subdirectory.
enum EntryKind {
    /// Flat file: `<dir>/<id>.json` — delete the file itself.
    FlatFile,
    /// Flat directory: `<dir>/<id>/` — delete the whole directory.
    FlatDir,
    /// File inside a project subdirectory: `<dir>/<subdir>/<id>.json`.
    /// Empty subdirs are pruned after.
    FileInSubdir,
}

/// Walk a storage directory and remove any entry whose ID (filename stem or
/// directory name) is NOT in `keep`.  Returns the number of entries removed.
fn remove_storage_entries_by_id(
    dir: &Path,
    kind: EntryKind,
    keep: &std::collections::HashSet<String>,
) -> usize {
    if !dir.exists() {
        return 0;
    }
    let mut removed = 0usize;

    match kind {
        EntryKind::FlatFile => {
            for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                if !keep.contains(&stem) {
                    if fs::remove_file(&path).is_ok() {
                        removed += 1;
                    }
                }
            }
        }

        EntryKind::FlatDir => {
            for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                if !keep.contains(&name) {
                    if fs::remove_dir_all(&path).is_ok() {
                        removed += 1;
                    }
                }
            }
        }

        EntryKind::FileInSubdir => {
            for subdir_entry in fs::read_dir(dir).into_iter().flatten().flatten() {
                let subdir = subdir_entry.path();
                if !subdir.is_dir() {
                    continue;
                }
                for file_entry in fs::read_dir(&subdir).into_iter().flatten().flatten() {
                    let path = file_entry.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("json") {
                        continue;
                    }
                    let stem = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_string();
                    if !keep.contains(&stem) {
                        if fs::remove_file(&path).is_ok() {
                            removed += 1;
                        }
                    }
                }
                // Prune empty project subdirectory.
                let is_empty = fs::read_dir(&subdir)
                    .map(|mut d| d.next().is_none())
                    .unwrap_or(false);
                if is_empty {
                    let _ = fs::remove_dir(&subdir);
                }
            }
        }
    }

    removed
}

/// Result returned by [`clean_opencode_snapshots`].
#[derive(Debug, serde::Serialize)]
pub struct CleanSnapshotsResult {
    /// Number of snapshot git repos that `git gc` was run on.
    pub repos_gced: usize,
    /// Number of orphaned snapshot directories removed (project no longer in DB).
    pub orphans_removed: usize,
    /// Number of stale `tmp_pack_*` files deleted.
    pub tmp_pack_files_removed: usize,
    /// Bytes freed (snapshot dir size before minus after).
    pub bytes_freed: u64,
}

/// Clean up the OpenCode snapshot directory.
///
/// The snapshot feature maintains a bare git repo per project under
/// `~/.local/share/opencode/snapshot/<project-id>/`.  Over time these can grow
/// very large due to unreferenced objects and uncompressed loose packs.
///
/// This function:
/// 1. Opens the OpenCode database to get the set of known project IDs.
/// 2. Removes `tmp_pack_*` orphan files left by crashed opencode processes.
/// 3. Runs `git gc --prune=now --quiet` on each live snapshot repo to compress
///    and discard unreachable objects immediately.
/// 4. Deletes entire snapshot directories whose project ID is no longer in the
///    database (projects that have been removed from OpenCode).
pub fn clean_opencode_snapshots() -> Result<CleanSnapshotsResult, String> {
    let data_dir = opencode_data_dir().ok_or_else(|| {
        "Cannot locate OpenCode data directory — home directory unknown".to_string()
    })?;

    let snapshot_dir = data_dir.join("snapshot");
    if !snapshot_dir.exists() {
        return Ok(CleanSnapshotsResult {
            repos_gced: 0,
            orphans_removed: 0,
            tmp_pack_files_removed: 0,
            bytes_freed: 0,
        });
    }

    // ── Measure size before ───────────────────────────────────────────────
    let size_before = dir_size_bytes(&snapshot_dir);

    // ── Load known project IDs from the database ──────────────────────────
    let known_project_ids = load_snapshot_project_ids(&data_dir);

    // ── Walk snapshot subdirectories ──────────────────────────────────────
    let entries = fs::read_dir(&snapshot_dir)
        .map_err(|e| format!("Cannot read snapshot directory: {}", e))?;

    let mut repos_gced = 0usize;
    let mut orphans_removed = 0usize;
    let mut tmp_pack_files_removed = 0usize;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let dir_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        // ── Remove tmp_pack_* files anywhere under this repo ──────────────
        tmp_pack_files_removed += remove_tmp_pack_files(&path);

        // ── Orphan check: remove dirs whose project ID is gone from DB ────
        // The "global" directory is always kept — it is not a DB project.
        if dir_name != "global" {
            if let Some(ref known) = known_project_ids {
                if !known.contains(&dir_name) {
                    if fs::remove_dir_all(&path).is_ok() {
                        orphans_removed += 1;
                    }
                    continue; // nothing more to do for this dir
                }
            }
        }

        // ── Run git gc on live snapshot repos ─────────────────────────────
        // Only attempt gc on directories that look like bare git repos.
        if path.join("objects").exists() && path.join("HEAD").exists() {
            let status = std::process::Command::new("git")
                .args(["gc", "--prune=now", "--quiet"])
                .current_dir(&path)
                .status();
            if status.map(|s| s.success()).unwrap_or(false) {
                repos_gced += 1;
            }
        }
    }

    let size_after = dir_size_bytes(&snapshot_dir);
    let bytes_freed = size_before.saturating_sub(size_after);

    Ok(CleanSnapshotsResult {
        repos_gced,
        orphans_removed,
        tmp_pack_files_removed,
        bytes_freed,
    })
}

/// Return the set of project IDs from the OpenCode database, or `None` if the
/// database cannot be opened (in which case orphan removal is skipped to avoid
/// accidentally deleting live snapshot data).
fn load_snapshot_project_ids(data_dir: &Path) -> Option<std::collections::HashSet<String>> {
    let db_path = data_dir.join("opencode.db");
    if !db_path.exists() {
        return None;
    }
    let conn = Connection::open(&db_path).ok()?;
    let mut stmt = conn.prepare("SELECT id FROM project").ok()?;
    let ids: std::collections::HashSet<String> = stmt
        .query_map([], |row| row.get(0))
        .ok()?
        .filter_map(|r| r.ok())
        .collect();
    Some(ids)
}

/// Recursively delete `tmp_pack_*` files under `dir` and return the count.
fn remove_tmp_pack_files(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let mut count = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            count += remove_tmp_pack_files(&path);
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with("tmp_pack_"))
            .unwrap_or(false)
        {
            if fs::remove_file(&path).is_ok() {
                count += 1;
            }
        }
    }
    count
}

/// Approximate total byte size of all files under `dir` via recursive walk.
fn dir_size_bytes(dir: &Path) -> u64 {
    fn walk(path: &Path) -> u64 {
        let Ok(entries) = fs::read_dir(path) else {
            return 0;
        };
        entries
            .flatten()
            .map(|e| {
                let p = e.path();
                if p.is_dir() {
                    walk(&p)
                } else {
                    p.metadata().map(|m| m.len()).unwrap_or(0)
                }
            })
            .sum()
    }
    walk(dir)
}

/// Resolve the OpenCode data directory.
///
/// - macOS / Linux: `~/.local/share/opencode`
/// - Falls back to `None` when the home directory cannot be determined.
pub(crate) fn opencode_data_dir() -> Option<PathBuf> {
    super::home_dir().map(|h| h.join(".local").join("share").join("opencode"))
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    fn stdio_servers() -> Map<String, Value> {
        let mut s = Map::new();
        s.insert(
            "automatic".to_string(),
            json!({"type":"stdio","command":"/usr/local/bin/automatic","args":["mcp-serve"]}),
        );
        s.insert(
            "github".to_string(),
            json!({"type":"stdio","command":"npx","args":["-y","@modelcontextprotocol/server-github"],"env":{"GITHUB_TOKEN":"ghp_test123"}}),
        );
        s
    }

    fn http_servers() -> Map<String, Value> {
        let mut s = Map::new();
        s.insert(
            "remote-api".to_string(),
            json!({"type":"http","url":"https://api.example.com/mcp","headers":{"Authorization":"Bearer tok_abc123"},"oauth":{"clientId":"client_123","scope":"read"}}),
        );
        s
    }

    #[test]
    fn test_detect() {
        let dir = tempdir().unwrap();
        assert!(!OpenCode.detect_in(dir.path()));

        fs::write(dir.path().join(".opencode.json"), "{}").unwrap();
        assert!(OpenCode.detect_in(dir.path()));
    }

    #[test]
    fn test_write_stdio() {
        let dir = tempdir().unwrap();
        OpenCode
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join("opencode.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(
            parsed["$schema"].as_str().unwrap(),
            "https://opencode.ai/config.json"
        );
        assert_eq!(
            parsed["mcp"]["automatic"]["type"].as_str().unwrap(),
            "local"
        );
        let cmd = parsed["mcp"]["automatic"]["command"].as_array().unwrap();
        assert_eq!(cmd[0].as_str().unwrap(), "/usr/local/bin/automatic");
        assert_eq!(cmd[1].as_str().unwrap(), "mcp-serve");

        let env = parsed["mcp"]["github"]["environment"].as_object().unwrap();
        assert_eq!(env["GITHUB_TOKEN"].as_str().unwrap(), "ghp_test123");
    }

    #[test]
    fn test_write_http() {
        let dir = tempdir().unwrap();
        OpenCode
            .write_mcp_config(dir.path(), &http_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join("opencode.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(
            parsed["$schema"].as_str().unwrap(),
            "https://opencode.ai/config.json"
        );
        assert_eq!(
            parsed["mcp"]["remote-api"]["type"].as_str().unwrap(),
            "remote"
        );
        assert_eq!(
            parsed["mcp"]["remote-api"]["url"].as_str().unwrap(),
            "https://api.example.com/mcp"
        );
        assert_eq!(
            parsed["mcp"]["remote-api"]["oauth"]["clientId"]
                .as_str()
                .unwrap(),
            "client_123"
        );
    }
}
