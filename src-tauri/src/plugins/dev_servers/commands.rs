use std::path::{Path, PathBuf};

use super::types::{DevServerStatus, LogLine, NpmScriptEntry, PackageManager, ServerConfig};
use super::{detect, process, registry};

fn resolve_dir(project_dir: &str, subdirectory: Option<&str>) -> PathBuf {
    match subdirectory {
        Some(sub) if !sub.trim().is_empty() => Path::new(project_dir).join(sub),
        _ => Path::new(project_dir).to_path_buf(),
    }
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
    registry::list_configs(&project)
}

#[tauri::command]
pub fn save_dev_server_config(project: String, config: ServerConfig) -> Result<ServerConfig, String> {
    registry::save_config(&project, config)
}

#[tauri::command]
pub fn delete_dev_server_config(project: String, id: String) -> Result<(), String> {
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

#[tauri::command]
pub fn start_dev_server(project: String, id: String) -> Result<DevServerStatus, String> {
    let config = registry::find_config(&project, &id)?;
    let directory = project_directory(&project)?;
    process::start(&project, &directory, &config)
}

#[tauri::command]
pub fn stop_dev_server(id: String) -> Result<DevServerStatus, String> {
    process::stop(&id)
}

/// Statuses for one project's configured servers, or (when `project` is
/// omitted) across every project that has any server configured — used by
/// the global Tools "Servers" view.
#[tauri::command]
pub fn list_dev_server_statuses(project: Option<String>) -> Result<Vec<DevServerStatus>, String> {
    match project {
        Some(project) => {
            let configs = registry::list_configs(&project)?;
            Ok(process::list_statuses(&project, &configs))
        }
        None => process::list_all_statuses(),
    }
}

#[tauri::command]
pub fn get_dev_server_log(id: String) -> Result<Vec<LogLine>, String> {
    Ok(process::get_log(&id))
}
