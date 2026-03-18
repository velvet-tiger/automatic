use std::fs;
use std::path::PathBuf;

use super::*;

// ── Project Groups ────────────────────────────────────────────────────────────
//
// Group configs are stored as individual JSON files at:
//   ~/.automatic/groups/{name}.json
//
// Each file contains a full `ProjectGroup` value.  The group name is the
// file stem; it must pass `is_valid_name`.

fn group_path(groups_dir: &PathBuf, name: &str) -> PathBuf {
    groups_dir.join(format!("{}.json", name))
}

pub fn list_groups() -> Result<Vec<String>, String> {
    let groups_dir = get_groups_dir()?;

    if !groups_dir.exists() {
        return Ok(Vec::new());
    }

    let mut groups = Vec::new();
    let entries = fs::read_dir(&groups_dir).map_err(|e| e.to_string())?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if is_valid_name(stem) {
                    groups.push(stem.to_string());
                }
            }
        }
    }

    groups.sort();
    Ok(groups)
}

pub fn read_group(name: &str) -> Result<String, String> {
    if !is_valid_name(name) {
        return Err("Invalid group name".into());
    }
    let groups_dir = get_groups_dir()?;
    let path = group_path(&groups_dir, name);

    if !path.exists() {
        return Err(format!("Group '{}' not found", name));
    }

    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    // Round-trip through the struct to ensure forward-compatibility: unknown
    // fields are silently dropped and defaults are applied.
    let group = serde_json::from_str::<ProjectGroup>(&raw).unwrap_or_else(|_| ProjectGroup {
        name: name.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
        ..Default::default()
    });
    serde_json::to_string_pretty(&group).map_err(|e| e.to_string())
}

pub fn save_group(name: &str, data: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid group name".into());
    }

    let group: ProjectGroup =
        serde_json::from_str(data).map_err(|e| format!("Invalid group data: {}", e))?;
    let pretty = serde_json::to_string_pretty(&group).map_err(|e| e.to_string())?;

    let groups_dir = get_groups_dir()?;
    if !groups_dir.exists() {
        fs::create_dir_all(&groups_dir).map_err(|e| e.to_string())?;
    }

    fs::write(group_path(&groups_dir, name), &pretty).map_err(|e| e.to_string())
}

pub fn delete_group(name: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid group name".into());
    }
    let groups_dir = get_groups_dir()?;
    let path = group_path(&groups_dir, name);

    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Return all groups that contain the given project name.
pub fn groups_for_project(project_name: &str) -> Vec<ProjectGroup> {
    let names = match list_groups() {
        Ok(n) => n,
        Err(_) => return Vec::new(),
    };

    let mut result = Vec::new();
    for name in names {
        if let Ok(raw) = read_group(&name) {
            if let Ok(group) = serde_json::from_str::<ProjectGroup>(&raw) {
                if group.projects.iter().any(|p| p == project_name) {
                    result.push(group);
                }
            }
        }
    }
    result
}
