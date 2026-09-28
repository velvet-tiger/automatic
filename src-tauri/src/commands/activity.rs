use crate::activity::{self, ActivityEntry};

/// Fill each entry's `local_key` from the registry, scanned once per call.
/// Entries for a project that is no longer registered keep `None`.
fn with_local_keys(mut entries: Vec<ActivityEntry>) -> Result<Vec<ActivityEntry>, String> {
    let keys = crate::core::project_local_keys_by_name()?;
    for entry in &mut entries {
        entry.local_key = keys.get(&entry.project).cloned();
    }
    Ok(entries)
}

/// Return the N most-recent activity entries for a specific project.
/// `limit` defaults to 20 if 0 is passed.
#[tauri::command]
pub fn get_project_activity(project: &str, limit: usize) -> Result<String, String> {
    let project = &crate::core::project_store_name(project)?;
    let n = if limit == 0 { 20 } else { limit };
    let entries = activity::get_project_activity(project, n)?;
    serde_json::to_string(&with_local_keys(entries)?).map_err(|e| e.to_string())
}

/// Return a page of activity entries for a specific project.
/// `limit` defaults to 50 if 0 is passed.  `offset` is zero-based.
#[tauri::command]
pub fn get_project_activity_paged(
    project: &str,
    limit: usize,
    offset: usize,
) -> Result<String, String> {
    let project = &crate::core::project_store_name(project)?;
    let n = if limit == 0 { 50 } else { limit };
    let entries = activity::get_project_activity_paged(project, n, offset)?;
    serde_json::to_string(&with_local_keys(entries)?).map_err(|e| e.to_string())
}

/// Return the total count of activity entries for a specific project.
#[tauri::command]
pub fn get_project_activity_count(project: &str) -> Result<i64, String> {
    let project = &crate::core::project_store_name(project)?;
    activity::get_project_activity_count(project)
}

/// Return the N most-recent activity entries across ALL projects.
/// `limit` defaults to 50 if 0 is passed.
#[tauri::command]
pub fn get_all_activity(limit: usize) -> Result<String, String> {
    let n = if limit == 0 { 50 } else { limit };
    let entries = activity::get_all_activity(n)?;
    serde_json::to_string(&with_local_keys(entries)?).map_err(|e| e.to_string())
}
