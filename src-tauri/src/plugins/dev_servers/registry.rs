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
pub fn list_projects_with_configs() -> Result<Vec<String>, String> {
    let dir = dev_servers_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("json") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                names.push(stem.to_string());
            }
        }
    }
    names.sort();
    Ok(names)
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

    #[test]
    fn list_projects_with_configs_reflects_saved_projects() {
        let tmp = tmp();
        with_test_home(tmp.path().to_path_buf(), || {
            save_config("demo-a", sample()).unwrap();
            save_config("demo-b", sample()).unwrap();

            let names = list_projects_with_configs().unwrap();
            assert_eq!(names, vec!["demo-a".to_string(), "demo-b".to_string()]);
        });
    }
}
