use std::path::{Path, PathBuf};

use super::types::{DevServerStatus, LogLine, NpmScriptEntry, PackageManager, ServerConfig};
use super::{detect, process, registry};
use crate::node_runtime::{self, NodeSelection};

fn resolve_dir(project_dir: &str, subdirectory: Option<&str>) -> PathBuf {
    match subdirectory {
        Some(sub) if !sub.trim().is_empty() => Path::new(project_dir).join(sub),
        _ => Path::new(project_dir).to_path_buf(),
    }
}

/// Fill each status's `local_key` from the registry, scanned once per call.
/// Statuses for a project that is no longer registered keep `None`.
fn with_local_keys(mut statuses: Vec<DevServerStatus>) -> Result<Vec<DevServerStatus>, String> {
    let keys = crate::core::project_local_keys_by_name()?;
    for status in &mut statuses {
        status.local_key = keys.get(&status.project).cloned();
    }
    Ok(statuses)
}

fn with_local_key(status: DevServerStatus) -> Result<DevServerStatus, String> {
    with_local_keys(vec![status])?
        .pop()
        .ok_or_else(|| "Dev server status vanished while its key was looked up".to_string())
}

fn project_directory(project: &str) -> Result<String, String> {
    let raw = crate::core::read_project(project)?;
    let parsed: crate::core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Corrupt project '{}': {}", project, e))?;
    Ok(parsed.directory)
}

// ── Config CRUD ──────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_dev_server_configs(project: String) -> Result<Vec<ServerConfig>, String> {
    let project = crate::core::project_store_name(&project)?;
    registry::list_configs(&project)
}

#[tauri::command]
pub fn save_dev_server_config(project: String, config: ServerConfig) -> Result<ServerConfig, String> {
    let project = crate::core::project_store_name(&project)?;
    registry::save_config(&project, config)
}

#[tauri::command]
pub fn delete_dev_server_config(project: String, id: String) -> Result<(), String> {
    let project = crate::core::project_store_name(&project)?;
    process::forget(&id)?;
    registry::delete_config(&project, &id)
}

// ── Detection ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn detect_dev_server_package_manager(
    project_dir: String,
    subdirectory: Option<String>,
) -> Result<Option<PackageManager>, String> {
    let dir = resolve_dir(&project_dir, subdirectory.as_deref());
    Ok(detect::detect_package_manager(&dir))
}

#[tauri::command]
pub fn list_dev_server_scripts(
    project_dir: String,
    subdirectory: Option<String>,
) -> Result<Vec<NpmScriptEntry>, String> {
    let dir = resolve_dir(&project_dir, subdirectory.as_deref());
    detect::list_npm_scripts(&dir)
}

// ── Process control ────────────────────────────────────────────────────────

/// `process::start` watches the new server's output for a few seconds and
/// blocks while it does. Tauri runs plain commands on the main thread, so
/// the work is moved to a blocking task to keep the window responsive.
#[tauri::command]
pub async fn start_dev_server(project: String, id: String) -> Result<DevServerStatus, String> {
    let project = crate::core::project_store_name(&project)?;
    tokio::task::spawn_blocking(move || {
        let config = registry::find_config(&project, &id)?;
        let directory = project_directory(&project)?;
        let node = select_node(&directory, &config)?;
        with_local_key(process::start(&project, &directory, &config, &node)?)
    })
    .await
    .map_err(|e| format!("start_dev_server task join error: {e}"))?
}

/// The Node to run a server with. Only consults nvm when the user has turned
/// on Settings → App → "Use nvm for Dev Servers".
fn select_node(project_dir: &str, config: &ServerConfig) -> Result<NodeSelection, String> {
    let settings = crate::core::read_settings()?;
    if !settings.nvm_enabled || project_dir.trim().is_empty() {
        return Ok(NodeSelection::NotRequested);
    }
    let nvm_dir = node_runtime::default_nvm_dir()
        .ok_or("Could not find the home directory to locate nvm. Set NVM_DIR before starting Automatic.")?;
    let working_dir = resolve_dir(project_dir, Some(&config.subdirectory));
    node_runtime::select_node(&working_dir, Path::new(project_dir), &nvm_dir)
}

#[tauri::command]
pub fn stop_dev_server(id: String) -> Result<DevServerStatus, String> {
    with_local_key(process::stop(&id)?)
}

/// Statuses for one project's configured servers, or (when `project` is
/// omitted) across every project that has any server configured — used by
/// the global Tools "Servers" view.
#[tauri::command]
pub fn list_dev_server_statuses(project: Option<String>) -> Result<Vec<DevServerStatus>, String> {
    match project {
        Some(project) => {
            let project = crate::core::project_store_name(&project)?;
            let configs = registry::list_configs(&project)?;
            with_local_keys(process::list_statuses(&project, &configs))
        }
        None => with_local_keys(process::list_all_statuses()?),
    }
}

#[tauri::command]
pub fn get_dev_server_log(id: String) -> Result<Vec<LogLine>, String> {
    Ok(process::get_log(&id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::dev_servers::types::PackageManager;

    fn sample() -> ServerConfig {
        ServerConfig {
            id: String::new(),
            name: "web".into(),
            package_manager: PackageManager::Npm,
            script: "dev".into(),
            subdirectory: String::new(),
            port: Some(3000),
            created_at: String::new(),
        }
    }

    /// A private Automatic home with one registered project, `site`.
    /// Passes the project's `local_key`.
    fn with_site(test: impl FnOnce(&str)) {
        let home = tempfile::tempdir().expect("temp home");
        crate::core::with_test_home(home.path().to_path_buf(), || {
            let mut project = crate::core::Project {
                name: "site".into(),
                ..Default::default()
            };
            crate::core::fill_missing_project_keys(&mut project);
            crate::core::save_project("site", &serde_json::to_string(&project).unwrap())
                .expect("save project");
            test(&project.local_key);
        });
    }

    #[test]
    fn config_saved_by_key_lands_under_the_name() {
        with_site(|key| {
            save_dev_server_config(key.to_string(), sample()).expect("save by key");
            assert_eq!(registry::list_configs("site").unwrap().len(), 1);
            assert!(registry::list_configs(key).unwrap().is_empty());
            assert_eq!(list_dev_server_configs(key.to_string()).unwrap().len(), 1);
        });
    }

    #[test]
    fn statuses_carry_the_key_and_orphans_carry_none() {
        with_site(|key| {
            registry::save_config("site", sample()).unwrap();
            let by_key = list_dev_server_statuses(Some(key.to_string())).expect("by key");
            assert_eq!(by_key.len(), 1);
            assert_eq!(by_key[0].project, "site");
            assert_eq!(by_key[0].local_key.as_deref(), Some(key));

            registry::save_config("ghost", sample()).unwrap();
            let ghost = list_dev_server_statuses(Some("ghost".into())).expect("orphan");
            assert_eq!(ghost.len(), 1);
            assert_eq!(ghost[0].project, "ghost", "an unknown project passes through");
            assert_eq!(ghost[0].local_key, None);
        });
    }
}
