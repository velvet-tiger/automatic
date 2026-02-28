use std::fs;
use std::path::PathBuf;

use super::*;

// ── Projects ─────────────────────────────────────────────────────────────────
//
// Project configs are stored in the project directory at `.automatic/project.json`.
// A lightweight registry entry at `~/.automatic/projects/{name}.json` maps project
// names to their directories so we can enumerate them.  When a project has no
// directory set yet, the full config lives in the registry file as a fallback.

/// Returns the path to the full project config inside the project directory.
fn project_config_path(directory: &str) -> PathBuf {
    PathBuf::from(directory)
        .join(".automatic")
        .join("project.json")
}

pub fn list_projects() -> Result<Vec<String>, String> {
    let projects_dir = get_projects_dir()?;

    if !projects_dir.exists() {
        return Ok(Vec::new());
    }

    let mut projects = Vec::new();
    let entries = fs::read_dir(&projects_dir).map_err(|e| e.to_string())?;

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if is_valid_name(stem) {
                        projects.push(stem.to_string());
                    }
                }
            }
        }
    }

    Ok(projects)
}

pub fn read_project(name: &str) -> Result<String, String> {
    if !is_valid_name(name) {
        return Err("Invalid project name".into());
    }
    let projects_dir = get_projects_dir()?;
    let registry_path = projects_dir.join(format!("{}.json", name));

    if !registry_path.exists() {
        return Err(format!("Project '{}' not found", name));
    }

    let raw = fs::read_to_string(&registry_path).map_err(|e| e.to_string())?;
    let registry_project = match serde_json::from_str::<Project>(&raw) {
        Ok(p) => p,
        Err(_) => Project {
            name: name.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            ..Default::default()
        },
    };

    // If directory is set, try to read full config from the project directory
    if !registry_project.directory.is_empty() {
        let config_path = project_config_path(&registry_project.directory);
        if config_path.exists() {
            let project_raw = fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
            let full_project = match serde_json::from_str::<Project>(&project_raw) {
                Ok(p) => p,
                Err(_) => registry_project, // fall back to registry data
            };
            let formatted =
                serde_json::to_string_pretty(&full_project).map_err(|e| e.to_string())?;
            return Ok(formatted);
        }
    }

    // No project-directory config found — use registry data (legacy or no-directory case)
    let formatted = serde_json::to_string_pretty(&registry_project).map_err(|e| e.to_string())?;
    Ok(formatted)
}

pub fn save_project(name: &str, data: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid project name".into());
    }

    let project: Project =
        serde_json::from_str(data).map_err(|e| format!("Invalid project data: {}", e))?;
    let pretty = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;

    let projects_dir = get_projects_dir()?;
    if !projects_dir.exists() {
        fs::create_dir_all(&projects_dir).map_err(|e| e.to_string())?;
    }

    let registry_path = projects_dir.join(format!("{}.json", name));

    if !project.directory.is_empty() {
        // Write full config to project directory
        let automatic_dir = PathBuf::from(&project.directory).join(".automatic");
        if !automatic_dir.exists() {
            fs::create_dir_all(&automatic_dir).map_err(|e| e.to_string())?;
        }
        let config_path = automatic_dir.join("project.json");
        fs::write(&config_path, &pretty).map_err(|e| e.to_string())?;

        // Write lightweight registry entry
        let ref_data = serde_json::json!({
            "name": project.name,
            "directory": project.directory,
        });
        let ref_pretty = serde_json::to_string_pretty(&ref_data).map_err(|e| e.to_string())?;
        fs::write(&registry_path, &ref_pretty).map_err(|e| e.to_string())?;
    } else {
        // No directory yet — write full config to registry
        fs::write(&registry_path, &pretty).map_err(|e| e.to_string())?;
    }

    Ok(())
}

pub fn rename_project(old_name: &str, new_name: &str) -> Result<(), String> {
    if !is_valid_name(old_name) {
        return Err("Invalid current project name".into());
    }
    if !is_valid_name(new_name) {
        return Err("Invalid new project name".into());
    }
    if old_name == new_name {
        return Ok(());
    }

    let projects_dir = get_projects_dir()?;
    let old_registry = projects_dir.join(format!("{}.json", old_name));
    let new_registry = projects_dir.join(format!("{}.json", new_name));

    if !old_registry.exists() {
        return Err(format!("Project '{}' not found", old_name));
    }
    // Only block if the target file exists and is a genuinely different project
    // (not just a case change on a case-insensitive filesystem like macOS APFS).
    if new_registry.exists() {
        // Compare canonical paths: on a case-insensitive FS, a case-only rename
        // will resolve both paths to the same inode.
        let old_canon = old_registry.canonicalize().map_err(|e| e.to_string())?;
        let new_canon = new_registry.canonicalize().map_err(|e| e.to_string())?;
        if old_canon != new_canon {
            return Err(format!("A project named '{}' already exists", new_name));
        }
    }

    // Read the full project (via read_project which resolves directory-based configs)
    let raw = read_project(old_name)?;
    let mut project: Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    // Update the name field
    project.name = new_name.to_string();
    project.updated_at = chrono::Utc::now().to_rfc3339();

    let pretty = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;

    // Write the in-directory config with the updated name
    if !project.directory.is_empty() {
        let config_path = project_config_path(&project.directory);
        if config_path.exists()
            || PathBuf::from(&project.directory)
                .join(".automatic")
                .exists()
        {
            let automatic_dir = PathBuf::from(&project.directory).join(".automatic");
            if !automatic_dir.exists() {
                fs::create_dir_all(&automatic_dir).map_err(|e| e.to_string())?;
            }
            fs::write(&config_path, &pretty).map_err(|e| e.to_string())?;
        }

        // Write new registry entry (lightweight pointer)
        let ref_data = serde_json::json!({
            "name": project.name,
            "directory": project.directory,
        });
        let ref_pretty = serde_json::to_string_pretty(&ref_data).map_err(|e| e.to_string())?;
        fs::write(&new_registry, &ref_pretty).map_err(|e| e.to_string())?;
    } else {
        // No directory — write full config to new registry entry
        fs::write(&new_registry, &pretty).map_err(|e| e.to_string())?;
    }

    // On a case-insensitive filesystem (macOS APFS/HFS+), a case-only rename
    // means old_registry and new_registry point to the same inode.  In that
    // case fs::write already updated the content above, so we just need to
    // rename the file to get the new casing on disk.  A plain remove would
    // delete the only copy.
    let same_file = old_registry.canonicalize().ok() == new_registry.canonicalize().ok();
    if same_file {
        // fs::rename handles case-only renames correctly on APFS/HFS+
        fs::rename(&old_registry, &new_registry).map_err(|e| e.to_string())?;
    } else {
        fs::remove_file(&old_registry).map_err(|e| e.to_string())?;
    }

    Ok(())
}

pub fn delete_project(name: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid project name".into());
    }
    let projects_dir = get_projects_dir()?;
    let registry_path = projects_dir.join(format!("{}.json", name));

    // Try to read the project to clean up the project-directory config
    if registry_path.exists() {
        if let Ok(raw) = fs::read_to_string(&registry_path) {
            if let Ok(project) = serde_json::from_str::<Project>(&raw) {
                if !project.directory.is_empty() {
                    let config_path = project_config_path(&project.directory);
                    if config_path.exists() {
                        let _ = fs::remove_file(&config_path);
                    }
                    // Remove .automatic dir if it's now empty
                    let automatic_dir = PathBuf::from(&project.directory).join(".automatic");
                    if automatic_dir.exists() {
                        let _ = fs::remove_dir(&automatic_dir); // only succeeds if empty
                    }
                }
            }
        }

        fs::remove_file(&registry_path).map_err(|e| e.to_string())?;
    }

    Ok(())
}
