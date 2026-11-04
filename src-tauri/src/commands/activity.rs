use crate::activity::{self, ActivityEntry};

// Activity rows are stored under the project's `local_key`. Each command
// resolves its `project` identifier with `project_store_keys` (an
// unregistered identifier passes through unchanged) and shows every row by
// the project's name.

/// Show each entry by project name and fill its `local_key`, reading the
/// registry once per call. A row stored under a key shows its project's
/// name; a legacy row stored under a name keeps it; a row no project claims
/// is unchanged with no key.
fn for_display(mut entries: Vec<ActivityEntry>) -> Result<Vec<ActivityEntry>, String> {
    let index = crate::core::ProjectKeyIndex::load()?;
    for entry in &mut entries {
        let (name, local_key) = index.display_checkout_row(&entry.project);
        entry.project = name;
        entry.local_key = local_key;
    }
    Ok(entries)
}

/// Return the N most-recent activity entries for a specific project.
/// `limit` defaults to 20 if 0 is passed.
#[tauri::command]
pub fn get_project_activity(project: &str, limit: usize) -> Result<String, String> {
    let project = &crate::core::project_store_local_key(project)?;
    let n = if limit == 0 { 20 } else { limit };
    let entries = activity::get_project_activity(project, n)?;
    serde_json::to_string(&for_display(entries)?).map_err(|e| e.to_string())
}

/// Return a page of activity entries for a specific project.
/// `limit` defaults to 50 if 0 is passed.  `offset` is zero-based.
#[tauri::command]
pub fn get_project_activity_paged(
    project: &str,
    limit: usize,
    offset: usize,
) -> Result<String, String> {
    let project = &crate::core::project_store_local_key(project)?;
    let n = if limit == 0 { 50 } else { limit };
    let entries = activity::get_project_activity_paged(project, n, offset)?;
    serde_json::to_string(&for_display(entries)?).map_err(|e| e.to_string())
}

/// Return the total count of activity entries for a specific project.
#[tauri::command]
pub fn get_project_activity_count(project: &str) -> Result<i64, String> {
    let project = &crate::core::project_store_local_key(project)?;
    activity::get_project_activity_count(project)
}

/// Return the N most-recent activity entries across ALL projects.
/// `limit` defaults to 50 if 0 is passed.
#[tauri::command]
pub fn get_all_activity(limit: usize) -> Result<String, String> {
    let n = if limit == 0 { 50 } else { limit };
    let entries = activity::get_all_activity(n)?;
    serde_json::to_string(&for_display(entries)?).map_err(|e| e.to_string())
}

/// Recent commits and working-tree state for a project's folder, for the
/// Summary. `None` when the folder is not in a git repository. The folder
/// comes from the project registry, never from the caller, so this cannot be
/// pointed at an arbitrary path.
#[tauri::command]
pub async fn get_project_git_activity(
    project: String,
) -> Result<Option<crate::core::git_activity::GitActivity>, String> {
    tokio::task::spawn_blocking(move || {
        let ident = crate::core::canonical_project_ident(&project)?;
        let raw = crate::core::read_project(&ident)?;
        let project: crate::core::Project =
            serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
        let dir = crate::core::git_activity::project_git_dir(&project)?;
        crate::core::git_activity::read_git_activity(&dir, &project)
    })
    .await
    .map_err(|e| format!("Git activity task failed: {}", e))?
}
