use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;

/// A single memory entry with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// The stored value.
    pub value: String,
    /// ISO 8601 timestamp when this entry was created or last updated.
    pub timestamp: String,
    /// Optional source identifier (e.g., which agent or tool stored this).
    pub source: Option<String>,
    /// Clerk user ID of the user whose agent stored this entry.
    /// Populated by the frontend/MCP caller for future team/cloud sync.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

/// Memory database type: a simple key-value store.
pub type MemoryDb = HashMap<String, MemoryEntry>;

pub fn get_memory_dir() -> Result<PathBuf, String> {
    Ok(crate::core::get_automatic_dir()?.join("memory"))
}

fn get_project_memory_path(project_name: &str) -> Result<PathBuf, String> {
    if !crate::core::is_valid_name(project_name) {
        return Err("Invalid project name".into());
    }
    let dir = get_memory_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    Ok(dir.join(format!("{}.json", project_name)))
}

pub fn read_memory_db(project_name: &str) -> Result<MemoryDb, String> {
    let path = get_project_memory_path(project_name)?;
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let db: MemoryDb = serde_json::from_str(&raw).unwrap_or_default();
    Ok(db)
}

pub fn write_memory_db(project_name: &str, db: &MemoryDb) -> Result<(), String> {
    let path = get_project_memory_path(project_name)?;
    let raw = serde_json::to_string_pretty(db).map_err(|e| e.to_string())?;
    fs::write(&path, raw).map_err(|e| e.to_string())
}

// ── Path-injectable helpers used by tests ────────────────────────────────────

#[cfg(test)]
/// Read the memory DB from an explicit base directory (used in tests).
fn read_db_at(base: &Path, project_name: &str) -> Result<MemoryDb, String> {
    let path = base.join(format!("{}.json", project_name));
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

#[cfg(test)]
/// Write the memory DB to an explicit base directory (used in tests).
fn write_db_at(base: &Path, project_name: &str, db: &MemoryDb) -> Result<(), String> {
    let path = base.join(format!("{}.json", project_name));
    let raw = serde_json::to_string_pretty(db).map_err(|e| e.to_string())?;
    fs::write(&path, raw).map_err(|e| e.to_string())
}

#[cfg(test)]
/// Store a memory entry into an explicit base directory (used in tests).
fn store_at(
    base: &Path,
    project_name: &str,
    key: &str,
    value: &str,
    source: Option<&str>,
) -> Result<String, String> {
    let mut db = read_db_at(base, project_name)?;
    db.insert(
        key.to_string(),
        MemoryEntry {
            value: value.to_string(),
            timestamp: current_timestamp(),
            source: source.map(|s| s.to_string()),
            created_by: None,
        },
    );
    write_db_at(base, project_name, &db)?;
    Ok(format!(
        "Memory stored: key='{}' for project '{}'",
        key, project_name
    ))
}

#[cfg(test)]
/// Delete a memory entry from an explicit base directory (used in tests).
fn delete_at(base: &Path, project_name: &str, key: &str) -> Result<String, String> {
    let mut db = read_db_at(base, project_name)?;
    if db.remove(key).is_none() {
        return Err(format!("Memory key '{}' not found", key));
    }
    write_db_at(base, project_name, &db)?;
    Ok(format!(
        "Memory deleted: key='{}' for project '{}'",
        key, project_name
    ))
}

#[cfg(test)]
/// Clear memory entries from an explicit base directory (used in tests).
fn clear_at(
    base: &Path,
    project_name: &str,
    pattern: Option<&str>,
    confirm: bool,
) -> Result<usize, String> {
    if !confirm {
        return Err("Deletion not confirmed.".to_string());
    }
    let mut db = read_db_at(base, project_name)?;
    let deleted_count = if let Some(pat) = pattern {
        let pat_lower = pat.to_lowercase();
        let keys: Vec<String> = db
            .keys()
            .filter(|k| k.to_lowercase().contains(&pat_lower))
            .cloned()
            .collect();
        let n = keys.len();
        for k in keys {
            db.remove(&k);
        }
        n
    } else {
        let n = db.len();
        db.clear();
        n
    };
    write_db_at(base, project_name, &db)?;
    Ok(deleted_count)
}

/// Generates an ISO 8601 timestamp for the current time.
pub fn current_timestamp() -> String {
    chrono::Utc::now().to_rfc3339()
}

// ============================================================================
// Raw API for Frontend UI
// ============================================================================

pub fn get_all_memories(project_name: &str) -> Result<MemoryDb, String> {
    read_memory_db(project_name)
}

// ============================================================================
// Formatted API for MCP (Agents)
// ============================================================================

pub fn store_memory(
    project_name: &str,
    key: &str,
    value: &str,
    source: Option<&str>,
) -> Result<String, String> {
    let mut db = read_memory_db(project_name)?;

    db.insert(
        key.to_string(),
        MemoryEntry {
            value: value.to_string(),
            timestamp: current_timestamp(),
            source: source.map(|s| s.to_string()),
            created_by: None,
        },
    );

    write_memory_db(project_name, &db)?;

    crate::activity::log(
        project_name,
        crate::activity::ActivityEvent::MemoryStored,
        &format!("Memory stored: {}", key),
        key,
    );

    Ok(format!(
        "Memory stored: key='{}' for project '{}'",
        key, project_name
    ))
}

pub fn get_memory(project_name: &str, key: &str) -> Result<String, String> {
    let db = read_memory_db(project_name)?;

    if let Some(entry) = db.get(key) {
        let mut output = format!("# Memory: {}\n\n", key);
        output.push_str(&format!("**Value:** {}\n", entry.value));
        output.push_str(&format!("**Timestamp:** {}\n", entry.timestamp));
        if let Some(src) = &entry.source {
            output.push_str(&format!("**Source:** {}\n", src));
        }
        Ok(output)
    } else {
        Err(format!("Memory key '{}' not found", key))
    }
}

pub fn list_memories(project_name: &str, pattern: Option<&str>) -> Result<String, String> {
    let db = read_memory_db(project_name)?;

    if db.is_empty() {
        return Ok(format!("No memories stored for project '{}'", project_name));
    }

    let mut keys: Vec<&String> = db.keys().collect();
    keys.sort();

    let filtered_keys: Vec<&String> = if let Some(pat) = pattern {
        let pat_lower = pat.to_lowercase();
        keys.into_iter()
            .filter(|k| k.to_lowercase().contains(&pat_lower))
            .collect()
    } else {
        keys
    };

    if filtered_keys.is_empty() {
        return Ok(format!(
            "No memories matching pattern '{}' for project '{}'",
            pattern.unwrap_or(""),
            project_name
        ));
    }

    let mut output = format!("# Memories for '{}'\n\n", project_name);
    if let Some(pat) = pattern {
        output.push_str(&format!("Filtered by: {}\n\n", pat));
    }

    for key in filtered_keys {
        if let Some(entry) = db.get(key) {
            output.push_str(&format!("- **{}**\n", key));
            output.push_str(&format!("  Timestamp: {}\n", entry.timestamp));
            if let Some(src) = &entry.source {
                output.push_str(&format!("  Source: {}\n", src));
            }
            let preview = if entry.value.len() > 100 {
                format!("{}...", &entry.value[..100])
            } else {
                entry.value.clone()
            };
            output.push_str(&format!("  Preview: {}\n", preview));
        }
    }

    Ok(output)
}

pub fn search_memories(project_name: &str, query: &str) -> Result<String, String> {
    let db = read_memory_db(project_name)?;

    if db.is_empty() {
        return Ok(format!("No memories stored for project '{}'", project_name));
    }

    let query_lower = query.to_lowercase();
    let mut matches: Vec<(&String, &MemoryEntry)> = db
        .iter()
        .filter(|(k, v)| {
            k.to_lowercase().contains(&query_lower) || v.value.to_lowercase().contains(&query_lower)
        })
        .collect();

    if matches.is_empty() {
        return Ok(format!(
            "No memories matching query '{}' for project '{}'",
            query, project_name
        ));
    }

    matches.sort_by_key(|(k, _)| *k);

    let mut output = format!("# Search results for '{}' in '{}'\n\n", query, project_name);
    output.push_str(&format!("Found {} match(es)\n\n", matches.len()));

    for (key, entry) in matches {
        output.push_str(&format!("## {}\n", key));
        output.push_str(&format!("**Value:** {}\n", entry.value));
        output.push_str(&format!("**Timestamp:** {}\n", entry.timestamp));
        if let Some(src) = &entry.source {
            output.push_str(&format!("**Source:** {}\n", src));
        }
        output.push('\n');
    }

    Ok(output)
}

pub fn delete_memory(project_name: &str, key: &str) -> Result<String, String> {
    let mut db = read_memory_db(project_name)?;

    if db.remove(key).is_none() {
        return Err(format!("Memory key '{}' not found", key));
    }

    write_memory_db(project_name, &db)?;

    crate::activity::log(
        project_name,
        crate::activity::ActivityEvent::MemoryDeleted,
        &format!("Memory deleted: {}", key),
        key,
    );

    Ok(format!(
        "Memory deleted: key='{}' for project '{}'",
        key, project_name
    ))
}

pub fn clear_memories(
    project_name: &str,
    pattern: Option<&str>,
    confirm: bool,
) -> Result<String, String> {
    if !confirm {
        return Err("Deletion not confirmed. Set 'confirm' to true to proceed.".to_string());
    }

    let mut db = read_memory_db(project_name)?;
    let deleted_count;

    if let Some(pat) = pattern {
        let pat_lower = pat.to_lowercase();
        let keys_to_delete: Vec<String> = db
            .keys()
            .filter(|k| k.to_lowercase().contains(&pat_lower))
            .cloned()
            .collect();

        deleted_count = keys_to_delete.len();
        for key in keys_to_delete {
            db.remove(&key);
        }
    } else {
        deleted_count = db.len();
        db.clear();
    }

    write_memory_db(project_name, &db)?;

    let detail = if let Some(pat) = pattern {
        format!("{} entries matching '{}'", deleted_count, pat)
    } else {
        format!("{} entries", deleted_count)
    };

    crate::activity::log(
        project_name,
        crate::activity::ActivityEvent::MemoryCleared,
        "Memory cleared",
        &detail,
    );

    if let Some(pat) = pattern {
        Ok(format!(
            "Cleared {} memor{} matching pattern '{}' for project '{}'",
            deleted_count,
            if deleted_count == 1 { "y" } else { "ies" },
            pat,
            project_name
        ))
    } else {
        Ok(format!(
            "Cleared all {} memor{} for project '{}'",
            deleted_count,
            if deleted_count == 1 { "y" } else { "ies" },
            project_name
        ))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // ── store / get ──────────────────────────────────────────────────────────

    #[test]
    fn store_creates_entry() {
        let dir = tempdir().unwrap();
        store_at(dir.path(), "proj", "k1", "hello", None).unwrap();

        let db = read_db_at(dir.path(), "proj").unwrap();
        let entry = db.get("k1").expect("key must exist");
        assert_eq!(entry.value, "hello");
        assert!(entry.source.is_none());
        assert!(!entry.timestamp.is_empty());
    }

    #[test]
    fn store_records_source() {
        let dir = tempdir().unwrap();
        store_at(dir.path(), "proj", "k1", "v", Some("agent-x")).unwrap();

        let db = read_db_at(dir.path(), "proj").unwrap();
        assert_eq!(db["k1"].source.as_deref(), Some("agent-x"));
    }

    #[test]
    fn store_overwrites_existing_key() {
        let dir = tempdir().unwrap();
        store_at(dir.path(), "proj", "k1", "first", None).unwrap();
        store_at(dir.path(), "proj", "k1", "second", None).unwrap();

        let db = read_db_at(dir.path(), "proj").unwrap();
        assert_eq!(db["k1"].value, "second");
        assert_eq!(db.len(), 1);
    }

    #[test]
    fn read_empty_project_returns_empty_map() {
        let dir = tempdir().unwrap();
        let db = read_db_at(dir.path(), "nonexistent").unwrap();
        assert!(db.is_empty());
    }

    #[test]
    fn store_multiple_keys_all_persisted() {
        let dir = tempdir().unwrap();
        store_at(dir.path(), "proj", "a", "1", None).unwrap();
        store_at(dir.path(), "proj", "b", "2", None).unwrap();
        store_at(dir.path(), "proj", "c", "3", None).unwrap();

        let db = read_db_at(dir.path(), "proj").unwrap();
        assert_eq!(db.len(), 3);
        assert_eq!(db["a"].value, "1");
        assert_eq!(db["b"].value, "2");
        assert_eq!(db["c"].value, "3");
    }

    // ── delete ───────────────────────────────────────────────────────────────

    #[test]
    fn delete_removes_key() {
        let dir = tempdir().unwrap();
        store_at(dir.path(), "proj", "k1", "v", None).unwrap();
        store_at(dir.path(), "proj", "k2", "v2", None).unwrap();

        delete_at(dir.path(), "proj", "k1").unwrap();

        let db = read_db_at(dir.path(), "proj").unwrap();
        assert!(!db.contains_key("k1"), "k1 should be gone");
        assert!(db.contains_key("k2"), "k2 should remain");
    }

    #[test]
    fn delete_missing_key_returns_err() {
        let dir = tempdir().unwrap();
        let err = delete_at(dir.path(), "proj", "ghost").unwrap_err();
        assert!(err.contains("not found"), "unexpected error: {}", err);
    }

    // ── clear ────────────────────────────────────────────────────────────────

    #[test]
    fn clear_all_empties_db() {
        let dir = tempdir().unwrap();
        store_at(dir.path(), "proj", "a", "1", None).unwrap();
        store_at(dir.path(), "proj", "b", "2", None).unwrap();

        let n = clear_at(dir.path(), "proj", None, true).unwrap();
        assert_eq!(n, 2);

        let db = read_db_at(dir.path(), "proj").unwrap();
        assert!(db.is_empty());
    }

    #[test]
    fn clear_with_pattern_removes_only_matching() {
        let dir = tempdir().unwrap();
        store_at(dir.path(), "proj", "foo/bar", "1", None).unwrap();
        store_at(dir.path(), "proj", "foo/baz", "2", None).unwrap();
        store_at(dir.path(), "proj", "other", "3", None).unwrap();

        let n = clear_at(dir.path(), "proj", Some("foo"), true).unwrap();
        assert_eq!(n, 2);

        let db = read_db_at(dir.path(), "proj").unwrap();
        assert!(!db.contains_key("foo/bar"));
        assert!(!db.contains_key("foo/baz"));
        assert!(db.contains_key("other"), "non-matching key must survive");
    }

    #[test]
    fn clear_pattern_case_insensitive() {
        let dir = tempdir().unwrap();
        store_at(dir.path(), "proj", "PREFIX/thing", "v", None).unwrap();

        let n = clear_at(dir.path(), "proj", Some("prefix"), true).unwrap();
        assert_eq!(n, 1);
        assert!(read_db_at(dir.path(), "proj").unwrap().is_empty());
    }

    #[test]
    fn clear_without_confirm_returns_err() {
        let dir = tempdir().unwrap();
        store_at(dir.path(), "proj", "k", "v", None).unwrap();
        let err = clear_at(dir.path(), "proj", None, false).unwrap_err();
        assert!(err.contains("not confirmed"), "unexpected error: {}", err);

        // DB must be untouched.
        assert_eq!(read_db_at(dir.path(), "proj").unwrap().len(), 1);
    }

    #[test]
    fn clear_empty_project_succeeds_with_zero_count() {
        let dir = tempdir().unwrap();
        let n = clear_at(dir.path(), "proj", None, true).unwrap();
        assert_eq!(n, 0);
    }

    // ── persistence ──────────────────────────────────────────────────────────

    #[test]
    fn data_survives_read_write_roundtrip() {
        let dir = tempdir().unwrap();
        let mut db: MemoryDb = HashMap::new();
        db.insert(
            "conventions/naming".to_string(),
            MemoryEntry {
                value: "use snake_case".to_string(),
                timestamp: current_timestamp(),
                source: Some("claude-code".to_string()),
                created_by: None,
            },
        );
        write_db_at(dir.path(), "proj", &db).unwrap();

        let loaded = read_db_at(dir.path(), "proj").unwrap();
        assert_eq!(loaded["conventions/naming"].value, "use snake_case");
        assert_eq!(
            loaded["conventions/naming"].source.as_deref(),
            Some("claude-code")
        );
    }

    // ── project isolation ────────────────────────────────────────────────────

    #[test]
    fn different_projects_are_isolated() {
        let dir = tempdir().unwrap();
        store_at(dir.path(), "project-a", "shared-key", "alpha", None).unwrap();
        store_at(dir.path(), "project-b", "shared-key", "beta", None).unwrap();

        let a = read_db_at(dir.path(), "project-a").unwrap();
        let b = read_db_at(dir.path(), "project-b").unwrap();

        assert_eq!(a["shared-key"].value, "alpha");
        assert_eq!(b["shared-key"].value, "beta");
    }
}
