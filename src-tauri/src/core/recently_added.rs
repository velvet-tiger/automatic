use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

use super::paths::get_automatic_dir;

const SEVEN_DAYS_SECS: i64 = 7 * 24 * 60 * 60;

#[derive(Debug, Serialize, Deserialize, Default)]
struct RecentlyAddedRegistry {
    #[serde(flatten)]
    items: HashMap<String, HashMap<String, i64>>,
}

fn registry_path() -> Result<std::path::PathBuf, String> {
    Ok(get_automatic_dir()?.join("recently_added.json"))
}

fn read_registry() -> RecentlyAddedRegistry {
    let path = match registry_path() {
        Ok(p) => p,
        Err(_) => return RecentlyAddedRegistry::default(),
    };
    let raw = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return RecentlyAddedRegistry::default(),
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn write_registry(registry: &RecentlyAddedRegistry) -> Result<(), String> {
    let path = registry_path()?;
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let json = serde_json::to_string_pretty(registry).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}

/// Record that an asset was newly added to the library.
///
/// `asset_type` is one of: "skills", "rules", "templates", "user_agents",
/// "commands", "mcp_servers", "project_templates".
///
/// Errors are swallowed — recording recently-added state must never fail
/// an otherwise-successful save operation.
pub fn record_recently_added(asset_type: &str, id: &str) {
    let mut registry = read_registry();
    let timestamp = Utc::now().timestamp();
    registry
        .items
        .entry(asset_type.to_string())
        .or_default()
        .insert(id.to_string(), timestamp);
    let _ = write_registry(&registry);
}

/// Remove an asset from the recently-added registry on deletion.
///
/// Errors are swallowed for the same reason as `record_recently_added`.
pub fn remove_recently_added(asset_type: &str, id: &str) {
    let mut registry = read_registry();
    if let Some(type_map) = registry.items.get_mut(asset_type) {
        type_map.remove(id);
    }
    let _ = write_registry(&registry);
}

/// Return the IDs of assets added within the last 7 days, sorted
/// most-recently-added first.
pub fn get_recently_added_ids(asset_type: &str) -> Vec<String> {
    let registry = read_registry();
    let cutoff = Utc::now().timestamp() - SEVEN_DAYS_SECS;

    match registry.items.get(asset_type) {
        Some(type_map) => {
            let mut entries: Vec<(String, i64)> = type_map
                .iter()
                .filter(|(_, &ts)| ts > cutoff)
                .map(|(id, &ts)| (id.clone(), ts))
                .collect();
            entries.sort_by(|a, b| b.1.cmp(&a.1));
            entries.into_iter().map(|(id, _)| id).collect()
        }
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::paths::with_test_home;
    use tempfile::TempDir;

    #[test]
    fn round_trip_record_and_retrieve() {
        let tmp = TempDir::new().unwrap();
        with_test_home(tmp.path().to_path_buf(), || {
            record_recently_added("skills", "my-skill");
            let ids = get_recently_added_ids("skills");
            assert_eq!(ids, vec!["my-skill"]);
        });
    }

    #[test]
    fn remove_clears_entry() {
        let tmp = TempDir::new().unwrap();
        with_test_home(tmp.path().to_path_buf(), || {
            record_recently_added("rules", "my-rule");
            remove_recently_added("rules", "my-rule");
            let ids = get_recently_added_ids("rules");
            assert!(ids.is_empty());
        });
    }

    #[test]
    fn unknown_asset_type_returns_empty() {
        let tmp = TempDir::new().unwrap();
        with_test_home(tmp.path().to_path_buf(), || {
            let ids = get_recently_added_ids("nonexistent");
            assert!(ids.is_empty());
        });
    }

    #[test]
    fn sorted_most_recent_first() {
        let tmp = TempDir::new().unwrap();
        with_test_home(tmp.path().to_path_buf(), || {
            // Manually write a registry with two entries at different timestamps
            let dir = get_automatic_dir().unwrap();
            fs::create_dir_all(&dir).unwrap();
            let json = serde_json::json!({
                "skills": {
                    "older-skill": Utc::now().timestamp() - 3600,
                    "newer-skill": Utc::now().timestamp() - 60,
                }
            });
            fs::write(dir.join("recently_added.json"), json.to_string()).unwrap();

            let ids = get_recently_added_ids("skills");
            assert_eq!(ids[0], "newer-skill");
            assert_eq!(ids[1], "older-skill");
        });
    }
}
