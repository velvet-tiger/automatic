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

// ── Test helpers (path-injectable versions of CRUD operations) ────────────────

#[cfg(test)]
mod test_helpers {
    use super::*;

    /// Save a project using an explicit projects dir (bypasses get_projects_dir).
    pub fn save_project_at(projects_dir: &PathBuf, name: &str, data: &str) -> Result<(), String> {
        if !is_valid_name(name) {
            return Err("Invalid project name".into());
        }
        let project: Project =
            serde_json::from_str(data).map_err(|e| format!("Invalid project data: {}", e))?;
        let pretty = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;

        if !projects_dir.exists() {
            fs::create_dir_all(projects_dir).map_err(|e| e.to_string())?;
        }

        let registry_path = projects_dir.join(format!("{}.json", name));

        if !project.directory.is_empty() {
            let automatic_dir = PathBuf::from(&project.directory).join(".automatic");
            if !automatic_dir.exists() {
                fs::create_dir_all(&automatic_dir).map_err(|e| e.to_string())?;
            }
            let config_path = automatic_dir.join("project.json");
            fs::write(&config_path, &pretty).map_err(|e| e.to_string())?;

            let ref_data = serde_json::json!({
                "name": project.name,
                "directory": project.directory,
            });
            let ref_pretty = serde_json::to_string_pretty(&ref_data).map_err(|e| e.to_string())?;
            fs::write(&registry_path, &ref_pretty).map_err(|e| e.to_string())?;
        } else {
            fs::write(&registry_path, &pretty).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Read a project using an explicit projects dir.
    pub fn read_project_at(projects_dir: &PathBuf, name: &str) -> Result<String, String> {
        if !is_valid_name(name) {
            return Err("Invalid project name".into());
        }
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

        if !registry_project.directory.is_empty() {
            let config_path = project_config_path(&registry_project.directory);
            if config_path.exists() {
                let project_raw = fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
                let full_project = match serde_json::from_str::<Project>(&project_raw) {
                    Ok(p) => p,
                    Err(_) => registry_project,
                };
                return serde_json::to_string_pretty(&full_project).map_err(|e| e.to_string());
            }
        }

        serde_json::to_string_pretty(&registry_project).map_err(|e| e.to_string())
    }

    /// List project names using an explicit projects dir.
    pub fn list_projects_at(projects_dir: &PathBuf) -> Result<Vec<String>, String> {
        if !projects_dir.exists() {
            return Ok(Vec::new());
        }
        let mut projects = Vec::new();
        let entries = fs::read_dir(projects_dir).map_err(|e| e.to_string())?;
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

    /// Delete a project using an explicit projects dir.
    pub fn delete_project_at(projects_dir: &PathBuf, name: &str) -> Result<(), String> {
        if !is_valid_name(name) {
            return Err("Invalid project name".into());
        }
        let registry_path = projects_dir.join(format!("{}.json", name));
        if registry_path.exists() {
            if let Ok(raw) = fs::read_to_string(&registry_path) {
                if let Ok(project) = serde_json::from_str::<Project>(&raw) {
                    if !project.directory.is_empty() {
                        let config_path = project_config_path(&project.directory);
                        if config_path.exists() {
                            let _ = fs::remove_file(&config_path);
                        }
                        let automatic_dir = PathBuf::from(&project.directory).join(".automatic");
                        if automatic_dir.exists() {
                            let _ = fs::remove_dir(&automatic_dir);
                        }
                    }
                }
            }
            fs::remove_file(&registry_path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::test_helpers::*;
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, PathBuf) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let projects_dir = tmp.path().join("projects");
        (tmp, projects_dir)
    }

    fn minimal_project(name: &str) -> String {
        serde_json::to_string(&Project {
            name: name.to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            ..Default::default()
        })
        .expect("serialize")
    }

    // ── list ─────────────────────────────────────────────────────────────────

    #[test]
    fn list_returns_empty_when_dir_missing() {
        let (_tmp, projects_dir) = setup();
        let names = list_projects_at(&projects_dir).expect("list");
        assert!(names.is_empty());
    }

    #[test]
    fn list_returns_project_names() {
        let (_tmp, projects_dir) = setup();
        save_project_at(&projects_dir, "alpha", &minimal_project("alpha")).expect("save");
        save_project_at(&projects_dir, "beta", &minimal_project("beta")).expect("save");

        let mut names = list_projects_at(&projects_dir).expect("list");
        names.sort();
        assert_eq!(names, vec!["alpha", "beta"]);
    }

    // ── save + read ──────────────────────────────────────────────────────────

    #[test]
    fn save_and_read_roundtrip_no_directory() {
        let (_tmp, projects_dir) = setup();
        let data = minimal_project("my-project");
        save_project_at(&projects_dir, "my-project", &data).expect("save");

        let raw = read_project_at(&projects_dir, "my-project").expect("read");
        let project: Project = serde_json::from_str(&raw).expect("parse");
        assert_eq!(project.name, "my-project");
    }

    #[test]
    fn save_with_directory_writes_config_to_project_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let projects_dir = tmp.path().join("projects");
        let project_dir = tmp.path().join("my-workspace");
        fs::create_dir_all(&project_dir).expect("create project dir");

        let project = Project {
            name: "with-dir".to_string(),
            directory: project_dir.to_str().unwrap().to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            ..Default::default()
        };
        let data = serde_json::to_string(&project).expect("serialize");
        save_project_at(&projects_dir, "with-dir", &data).expect("save");

        // Config should be in the project directory.
        let config_path = project_dir.join(".automatic").join("project.json");
        assert!(
            config_path.exists(),
            "project config missing in project dir"
        );

        // Registry entry should be a lightweight pointer.
        let registry_path = projects_dir.join("with-dir.json");
        let raw = fs::read_to_string(&registry_path).expect("read registry");
        let val: serde_json::Value = serde_json::from_str(&raw).expect("parse");
        assert!(val.get("name").is_some());
        assert!(val.get("directory").is_some());
    }

    #[test]
    fn read_falls_back_to_registry_when_no_project_dir_config() {
        let (_tmp, projects_dir) = setup();
        let data = minimal_project("fallback");
        save_project_at(&projects_dir, "fallback", &data).expect("save");

        let raw = read_project_at(&projects_dir, "fallback").expect("read");
        let project: Project = serde_json::from_str(&raw).expect("parse");
        assert_eq!(project.name, "fallback");
    }

    #[test]
    fn read_returns_error_for_missing_project() {
        let (_tmp, projects_dir) = setup();
        let result = read_project_at(&projects_dir, "does-not-exist");
        assert!(result.is_err());
    }

    // ── delete ───────────────────────────────────────────────────────────────

    #[test]
    fn delete_removes_registry_entry() {
        let (_tmp, projects_dir) = setup();
        save_project_at(&projects_dir, "doomed", &minimal_project("doomed")).expect("save");

        let path = projects_dir.join("doomed.json");
        assert!(path.exists());

        delete_project_at(&projects_dir, "doomed").expect("delete");
        assert!(!path.exists());
    }

    #[test]
    fn delete_is_idempotent_when_project_missing() {
        let (_tmp, projects_dir) = setup();
        // Deleting a non-existent project should not error.
        delete_project_at(&projects_dir, "ghost").expect("delete non-existent");
    }

    // ── invalid name handling ────────────────────────────────────────────────

    #[test]
    fn save_with_invalid_name_returns_error() {
        let (_tmp, projects_dir) = setup();
        let result = save_project_at(&projects_dir, "", &minimal_project(""));
        assert!(result.is_err());
    }

    #[test]
    fn save_with_path_traversal_name_returns_error() {
        let (_tmp, projects_dir) = setup();
        let result = save_project_at(&projects_dir, "../escape", &minimal_project("x"));
        assert!(result.is_err());
    }

    // ── overwrite ────────────────────────────────────────────────────────────

    #[test]
    fn save_overwrites_existing_project() {
        let (_tmp, projects_dir) = setup();
        let v1 = serde_json::to_string(&Project {
            name: "proj".to_string(),
            description: "v1".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            ..Default::default()
        })
        .expect("serialize v1");

        let v2 = serde_json::to_string(&Project {
            name: "proj".to_string(),
            description: "v2".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-02T00:00:00Z".to_string(),
            ..Default::default()
        })
        .expect("serialize v2");

        save_project_at(&projects_dir, "proj", &v1).expect("save v1");
        save_project_at(&projects_dir, "proj", &v2).expect("save v2");

        let raw = read_project_at(&projects_dir, "proj").expect("read");
        let project: Project = serde_json::from_str(&raw).expect("parse");
        assert_eq!(project.description, "v2");
    }
}
