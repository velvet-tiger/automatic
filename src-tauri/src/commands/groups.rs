use crate::core;

// ── Project Groups ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_groups() -> Result<Vec<String>, String> {
    core::list_groups()
}

/// Read a group. `projects` lists each member project as the `local_key` of
/// every checkout of it (stage 6). A stored member no registered project
/// claims (a legacy name, or a deleted project's id) is returned as stored,
/// so the UI can still show and remove it. Display names come from
/// `get_project_summaries`, not from the group.
#[tauri::command]
pub fn read_group(name: &str) -> Result<String, String> {
    core::read_group(name)
}

/// Save a group and bring the contexts of its old and new members in step.
/// `projects` holds `local_key`s (ids are accepted too); they are stored as
/// project ids. Any other value is stored unchanged.
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
/// `project_name` is an identifier: a `local_key`, an `id`, or a name only
/// one project has (a shared name is an ambiguity error). An unknown one
/// matches a member stored under it.
#[tauri::command]
pub fn groups_for_project(project_name: &str) -> Result<Vec<String>, String> {
    let keys = crate::core::project_store_keys(project_name)?;
    let groups = core::groups_for_project(&keys.local_key);
    Ok(groups.into_iter().map(|g| g.name).collect())
}
