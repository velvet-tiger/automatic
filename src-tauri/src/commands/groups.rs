use crate::core;

// ── Project Groups ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_groups() -> Result<Vec<String>, String> {
    core::list_groups()
}

#[tauri::command]
pub fn read_group(name: &str) -> Result<String, String> {
    core::read_group(name)
}

/// Save a group and bring the contexts of its old and new members in step.
#[tauri::command]
pub fn save_group(name: &str, data: &str) -> Result<(), String> {
    core::save_group_reconciling_contexts(name, data).map(|_| ())
}

/// Delete a group and release the contexts it provided to its members.
#[tauri::command]
pub fn delete_group(name: &str) -> Result<(), String> {
    core::delete_group_reconciling_contexts(name).map(|_| ())
}

/// Return the names of all groups that contain the given project.
#[tauri::command]
pub fn groups_for_project(project_name: &str) -> Result<Vec<String>, String> {
    let groups = core::groups_for_project(project_name);
    Ok(groups.into_iter().map(|g| g.name).collect())
}
