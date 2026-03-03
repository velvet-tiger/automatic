use crate::activity;

/// Return the N most-recent activity entries for a specific project.
/// `limit` defaults to 20 if 0 is passed.
#[tauri::command]
pub fn get_project_activity(project: &str, limit: usize) -> Result<String, String> {
    let n = if limit == 0 { 20 } else { limit };
    let entries = activity::get_project_activity(project, n)?;
    serde_json::to_string(&entries).map_err(|e| e.to_string())
}

/// Return a page of activity entries for a specific project.
/// `limit` defaults to 50 if 0 is passed.  `offset` is zero-based.
#[tauri::command]
pub fn get_project_activity_paged(
    project: &str,
    limit: usize,
    offset: usize,
) -> Result<String, String> {
    let n = if limit == 0 { 50 } else { limit };
    let entries = activity::get_project_activity_paged(project, n, offset)?;
    serde_json::to_string(&entries).map_err(|e| e.to_string())
}

/// Return the total count of activity entries for a specific project.
#[tauri::command]
pub fn get_project_activity_count(project: &str) -> Result<i64, String> {
    activity::get_project_activity_count(project)
}

/// Return the N most-recent activity entries across ALL projects.
/// `limit` defaults to 50 if 0 is passed.
#[tauri::command]
pub fn get_all_activity(limit: usize) -> Result<String, String> {
    let n = if limit == 0 { 50 } else { limit };
    let entries = activity::get_all_activity(n)?;
    serde_json::to_string(&entries).map_err(|e| e.to_string())
}
