use std::fs;
use std::path::PathBuf;

use super::types::ServerConfig;

/// Directory holding one JSON file per project, each containing that
/// project's list of configured dev servers. Self-contained under this
/// plugin's own module, mirroring `memory.rs`'s per-project storage.
fn dev_servers_dir() -> Result<PathBuf, String> {
    Ok(crate::core::get_automatic_dir()?.join("dev-servers"))
}

fn config_path(project: &str) -> Result<PathBuf, String> {
    if !crate::core::is_valid_name(project) {
        return Err("Invalid project name".into());
    }
    Ok(dev_servers_dir()?.join(format!("{}.json", project)))
}

/// Read all configured servers for a project. Returns an empty list if the
/// project has none configured yet.
pub fn list_configs(project: &str) -> Result<Vec<ServerConfig>, String> {
    let path = config_path(project)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| format!("Corrupt dev server config for '{}': {}", project, e))
}

fn write_configs(project: &str, configs: &[ServerConfig]) -> Result<(), String> {
    let path = config_path(project)?;
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let pretty = serde_json::to_string_pretty(configs).map_err(|e| e.to_string())?;
    fs::write(&path, pretty).map_err(|e| e.to_string())
}

/// Find a single server config by id.
pub fn find_config(project: &str, id: &str) -> Result<ServerConfig, String> {
    list_configs(project)?
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| format!("Dev server '{}' not found for project '{}'", id, project))
}

/// Create or update a server config. When `config.id` is empty, a new id is
/// generated and the config is appended; otherwise the existing entry with
/// a matching id is replaced.
pub fn save_config(project: &str, mut config: ServerConfig) -> Result<ServerConfig, String> {
    if config.name.trim().is_empty() {
        return Err("Server name cannot be empty".into());
    }
    if config.script.trim().is_empty() {
        return Err("Script cannot be empty".into());
    }

    let mut configs = list_configs(project)?;

    if config.id.is_empty() {
        config.id = uuid::Uuid::new_v4().to_string();
        config.created_at = chrono::Utc::now().to_rfc3339();
        configs.push(config.clone());
    } else if let Some(existing) = configs.iter_mut().find(|c| c.id == config.id) {
        config.created_at = existing.created_at.clone();
        *existing = config.clone();
    } else {
        return Err(format!("Dev server '{}' not found for project '{}'", config.id, project));
    }

    write_configs(project, &configs)?;
    Ok(config)
}

/// Remove a server config. Does not stop a running process — callers must
/// stop the server first if it is running.
pub fn delete_config(project: &str, id: &str) -> Result<(), String> {
    let mut configs = list_configs(project)?;
    let before = configs.len();
    configs.retain(|c| c.id != id);
    if configs.len() == before {
        return Err(format!("Dev server '{}' not found for project '{}'", id, project));
    }
    write_configs(project, &configs)
}

/// Names of every project that has at least one dev server configured.
/// Used to build the cross-project view in the global Tools section.
///
/// Orphaned files — dev-server configs whose project name is no longer in
/// `crate::core::list_projects()` because the project was renamed or
/// deleted before this plugin's registry was updated — are removed from
/// disk as a side effect. Without that guard the global "Servers" view
/// keeps showing rows the user cannot delete from the UI: the per-project
/// delete path lives inside the project editor, and an orphan has no
/// editor to open.
pub fn list_projects_with_configs() -> Result<Vec<String>, String> {
    let dir = dev_servers_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let live: std::collections::HashSet<String> = crate::core::list_projects()
        .unwrap_or_default()
        .into_iter()
        .collect();
    let mut names = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if live.contains(stem) {
            names.push(stem.to_string());
        } else {
            // Orphan: the project no longer exists. Best-effort delete —
            // an IO error here just means the row survives until next
            // refresh, not a broken UI.
            let _ = fs::remove_file(&path);
        }
    }
    names.sort();
    Ok(names)
}

/// Rename this project's dev-server registry file from `old` to `new`.
/// No-op when the source file does not exist (the common case — most
/// projects never configure a dev server). Called from the project
/// rename command so the global "Servers" view stays anchored to the
/// live project name.
pub fn rename_project(old: &str, new: &str) -> Result<(), String> {
    if old == new {
        return Ok(());
    }
    if !crate::core::is_valid_name(old) || !crate::core::is_valid_name(new) {
        return Err("Invalid project name".into());
    }
    let dir = dev_servers_dir()?;
    let old_path = dir.join(format!("{}.json", old));
    if !old_path.exists() {
        return Ok(());
    }
    let new_path = dir.join(format!("{}.json", new));
    if new_path.exists() {
        return Err(format!(
            "Dev server config for '{}' already exists",
            new
        ));
    }
    fs::rename(&old_path, &new_path).map_err(|e| e.to_string())
}

/// Remove this project's dev-server registry file. No-op if not present.
/// Called from the project delete command so no orphan is left behind.
pub fn remove_project(project: &str) -> Result<(), String> {
    if !crate::core::is_valid_name(project) {
        return Err("Invalid project name".into());
    }
    let dir = dev_servers_dir()?;
    let path = dir.join(format!("{}.json", project));
    if !path.exists() {
        return Ok(());
    }
    fs::remove_file(&path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::with_test_home;
    use tempfile::TempDir;

    fn tmp() -> TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn sample() -> ServerConfig {
        ServerConfig {
            id: String::new(),
            name: "web".into(),
            package_manager: super::super::types::PackageManager::Npm,
            script: "dev".into(),
            subdirectory: String::new(),
            port: Some(3000),
            created_at: String::new(),
        }
    }

    #[test]
    fn save_creates_new_config_with_generated_id() {
        let tmp = tmp();
        with_test_home(tmp.path().to_path_buf(), || {
            let saved = save_config("demo", sample()).unwrap();
            assert!(!saved.id.is_empty());
            assert_eq!(saved.name, "web");

            let listed = list_configs("demo").unwrap();
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0].id, saved.id);
        });
    }

    #[test]
    fn save_updates_existing_config_by_id() {
        let tmp = tmp();
        with_test_home(tmp.path().to_path_buf(), || {
            let saved = save_config("demo", sample()).unwrap();

            let mut updated = saved.clone();
            updated.script = "start".into();
            let resaved = save_config("demo", updated).unwrap();

            let listed = list_configs("demo").unwrap();
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0].script, "start");
            assert_eq!(listed[0].created_at, saved.created_at);
            assert_eq!(resaved.created_at, saved.created_at);
        });
    }

    #[test]
    fn delete_removes_config() {
        let tmp = tmp();
        with_test_home(tmp.path().to_path_buf(), || {
            let saved = save_config("demo", sample()).unwrap();
            delete_config("demo", &saved.id).unwrap();
            assert!(list_configs("demo").unwrap().is_empty());
        });
    }

    #[test]
    fn delete_missing_config_errors() {
        let tmp = tmp();
        with_test_home(tmp.path().to_path_buf(), || {
            let err = delete_config("demo", "missing").unwrap_err();
            assert!(err.contains("not found"));
        });
    }

    /// Register `name` as a project so `crate::core::list_projects` sees
    /// it. Empty-directory registry entries are the simplest possible
    /// project record and match how a wizard-in-progress project looks
    /// on disk before its directory is picked.
    fn register_project(name: &str) {
        let raw = format!("{{\"name\":\"{}\"}}", name);
        crate::core::save_project(name, &raw).expect("register project");
    }

    #[test]
    fn list_projects_with_configs_reflects_saved_projects() {
        let tmp = tmp();
        with_test_home(tmp.path().to_path_buf(), || {
            register_project("demo-a");
            register_project("demo-b");
            save_config("demo-a", sample()).unwrap();
            save_config("demo-b", sample()).unwrap();

            let names = list_projects_with_configs().unwrap();
            assert_eq!(names, vec!["demo-a".to_string(), "demo-b".to_string()]);
        });
    }

    #[test]
    fn list_projects_with_configs_prunes_orphaned_files() {
        // VEL-160: a project renamed or deleted before this plugin's
        // registry was cleaned up would leave a JSON file behind, and the
        // global Tools > Servers view would keep showing rows the user
        // could not delete without hand-editing config.
        let tmp = tmp();
        with_test_home(tmp.path().to_path_buf(), || {
            register_project("live-project");
            save_config("live-project", sample()).unwrap();
            save_config("ghost-project", sample()).unwrap();

            let names = list_projects_with_configs().unwrap();
            assert_eq!(names, vec!["live-project".to_string()]);

            // Orphaned file was pruned from disk, not just filtered.
            let dir = dev_servers_dir().unwrap();
            assert!(!dir.join("ghost-project.json").exists());
            assert!(dir.join("live-project.json").exists());
        });
    }

    #[test]
    fn rename_project_moves_config_file() {
        let tmp = tmp();
        with_test_home(tmp.path().to_path_buf(), || {
            save_config("old", sample()).unwrap();
            rename_project("old", "renamed").unwrap();

            let dir = dev_servers_dir().unwrap();
            assert!(!dir.join("old.json").exists());
            assert!(dir.join("renamed.json").exists());

            let listed = list_configs("renamed").unwrap();
            assert_eq!(listed.len(), 1);
        });
    }

    #[test]
    fn rename_project_is_noop_when_source_missing() {
        let tmp = tmp();
        with_test_home(tmp.path().to_path_buf(), || {
            // Most projects never touch this plugin. A rename must still
            // succeed for them or the whole rename_project command fails.
            rename_project("never-configured", "renamed").unwrap();
        });
    }

    #[test]
    fn rename_project_refuses_to_overwrite_existing_target() {
        let tmp = tmp();
        with_test_home(tmp.path().to_path_buf(), || {
            save_config("a", sample()).unwrap();
            save_config("b", sample()).unwrap();
            let err = rename_project("a", "b").unwrap_err();
            assert!(err.contains("already exists"), "unexpected error: {err}");
        });
    }

    #[test]
    fn remove_project_deletes_config_file() {
        let tmp = tmp();
        with_test_home(tmp.path().to_path_buf(), || {
            save_config("goner", sample()).unwrap();
            remove_project("goner").unwrap();

            let dir = dev_servers_dir().unwrap();
            assert!(!dir.join("goner.json").exists());
        });
    }

    #[test]
    fn remove_project_is_noop_when_file_missing() {
        let tmp = tmp();
        with_test_home(tmp.path().to_path_buf(), || {
            remove_project("never-configured").unwrap();
        });
    }
}
