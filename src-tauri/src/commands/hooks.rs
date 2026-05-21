use crate::core;

use super::projects::{
    prune_hook_from_projects, sync_project_if_configured, sync_projects_referencing_hook,
};

// ── Hooks ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_hooks() -> Result<Vec<core::HookEntry>, String> {
    core::list_hooks()
}

#[tauri::command]
pub fn read_hook(machine_name: &str) -> Result<String, String> {
    core::read_hook(machine_name)
}

/// Create or update a hook in the library.
///
/// `handler` is the serialised `HookHandler` JSON value (the same shape the
/// frontend hands to the editor). It is parsed server-side so we keep the
/// schema enforcement in one place.
#[tauri::command]
pub fn save_hook(
    machine_name: &str,
    name: &str,
    agent: &str,
    event: &str,
    matcher: Option<String>,
    handler: serde_json::Value,
    timeout_sec: Option<u32>,
) -> Result<(), String> {
    let handler: core::HookHandler = serde_json::from_value(handler)
        .map_err(|e| format!("Invalid hook handler payload: {}", e))?;

    core::save_hook(
        machine_name,
        name,
        agent,
        event,
        matcher.as_deref(),
        handler,
        timeout_sec,
    )?;

    // Library propagation invariant: every save resyncs all projects that
    // reference this hook so their on-disk agent config matches the library.
    sync_projects_referencing_hook(machine_name);
    Ok(())
}

#[tauri::command]
pub fn delete_hook(machine_name: &str) -> Result<(), String> {
    core::delete_hook(machine_name)?;
    prune_hook_from_projects(machine_name);
    Ok(())
}

#[tauri::command]
pub fn attach_hook_to_project(project_name: &str, hook_name: &str) -> Result<(), String> {
    let raw = core::read_project(project_name)?;
    let mut project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    if core::read_hook(hook_name).is_err() {
        return Err(format!("Hook '{}' not found in the library", hook_name));
    }

    if !project.hooks.iter().any(|h| h == hook_name) {
        project.hooks.push(hook_name.to_string());
        project.updated_at = chrono::Utc::now().to_rfc3339();
        let json = serde_json::to_string_pretty(&project)
            .map_err(|e| format!("Failed to serialise project: {}", e))?;
        core::save_project(project_name, &json)?;
    }

    sync_project_if_configured(project_name, &mut project);
    Ok(())
}

#[tauri::command]
pub fn detach_hook_from_project(project_name: &str, hook_name: &str) -> Result<(), String> {
    let raw = core::read_project(project_name)?;
    let mut project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    let before = project.hooks.len();
    project.hooks.retain(|h| h != hook_name);
    if project.hooks.len() == before {
        return Ok(()); // already detached, nothing to do
    }

    project.updated_at = chrono::Utc::now().to_rfc3339();
    let json = serde_json::to_string_pretty(&project)
        .map_err(|e| format!("Failed to serialise project: {}", e))?;
    core::save_project(project_name, &json)?;
    sync_project_if_configured(project_name, &mut project);
    Ok(())
}

/// Return all projects that have `hook_name` attached. Used by the Hooks
/// page to show "this hook is used in N projects".
#[derive(serde::Serialize)]
pub struct HookProjectStatus {
    pub name: String,
}

#[tauri::command]
pub fn get_projects_referencing_hook(hook_name: &str) -> Result<Vec<HookProjectStatus>, String> {
    let mut referencing = Vec::new();
    super::projects::with_each_project_mut(|project_name, project| {
        if project.hooks.iter().any(|h| h == hook_name) {
            referencing.push(HookProjectStatus {
                name: project_name.to_string(),
            });
        }
    });
    Ok(referencing)
}
