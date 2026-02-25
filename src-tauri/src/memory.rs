use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
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
}

/// Memory database type: a simple key-value store.
pub type MemoryDb = HashMap<String, MemoryEntry>;

pub fn get_memory_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".automatic/memory"))
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

/// Generates an ISO 8601 timestamp for the current time.
pub fn current_timestamp() -> String {
    chrono::Utc::now().to_rfc3339()
}

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
        },
    );

    write_memory_db(project_name, &db)?;

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
            k.to_lowercase().contains(&query_lower)
                || v.value.to_lowercase().contains(&query_lower)
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

    Ok(format!(
        "Memory deleted: key='{}' for project '{}'",
        key, project_name
    ))
}

pub fn clear_memories(project_name: &str, pattern: Option<&str>, confirm: bool) -> Result<String, String> {
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
