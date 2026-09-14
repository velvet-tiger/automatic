use crate::core;

use super::projects::{
    detach_profile_from_projects, reconcile_projects_referencing_profile,
    rename_profile_in_projects, sync_project_if_configured, with_each_project_mut,
};

// ── Project Profiles ─────────────────────────────────────────────────────────
//
// Thin wrappers over `core::project_profiles`. Every write that can change
// what an attached project should contain fans out through the sweeps in
// `commands::projects`, so projects never drift from their profiles.

#[tauri::command]
pub fn get_project_profiles() -> Result<Vec<String>, String> {
    core::list_project_profiles()
}

#[tauri::command]
pub fn read_project_profile(name: &str) -> Result<String, String> {
    core::read_project_profile(name)
}

/// Save a profile, then bring every attached project back in step and
/// re-sync the ones that changed.
#[tauri::command]
pub fn save_project_profile(name: &str, data: &str) -> Result<(), String> {
    core::save_project_profile(name, data)?;
    reconcile_projects_referencing_profile(name);
    Ok(())
}

/// Detach the profile from every project first so each one loses only what
/// the profile added, then remove the file.
#[tauri::command]
pub fn delete_project_profile(name: &str) -> Result<(), String> {
    detach_profile_from_projects(name);
    core::delete_project_profile(name)
}

#[tauri::command]
pub fn rename_project_profile(old_name: &str, new_name: &str) -> Result<(), String> {
    core::rename_project_profile(old_name, new_name)?;
    rename_profile_in_projects(old_name, new_name);
    Ok(())
}

/// Every project that has the profile attached.
#[tauri::command]
pub fn get_projects_referencing_profile(
    profile_name: &str,
) -> Result<Vec<core::ProjectRef>, String> {
    let mut referencing = Vec::new();
    with_each_project_mut(|project_name, project| {
        if project.profiles.iter().any(|p| p == profile_name) {
            referencing.push(core::ProjectRef {
                name: project_name.to_string(),
                directory: project.directory.clone(),
            });
        }
    });
    referencing.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(referencing)
}

#[tauri::command]
pub fn attach_profile_to_project(project_name: &str, profile_name: &str) -> Result<(), String> {
    core::read_project_profile_parsed(profile_name)?;

    let raw = core::read_project(project_name)?;
    let mut project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    if !project.profiles.iter().any(|p| p == profile_name) {
        project.profiles.push(profile_name.to_string());
    }
    core::reconcile_project_profiles(&mut project);
    project.updated_at = chrono::Utc::now().to_rfc3339();
    let json = serde_json::to_string_pretty(&project)
        .map_err(|e| format!("Failed to serialise project: {}", e))?;
    core::save_project(project_name, &json)?;

    sync_project_if_configured(project_name, &mut project);
    Ok(())
}

#[tauri::command]
pub fn detach_profile_from_project(project_name: &str, profile_name: &str) -> Result<(), String> {
    let raw = core::read_project(project_name)?;
    let mut project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    let before = project.profiles.len();
    project.profiles.retain(|p| p != profile_name);
    let recorded = project.profile_contributions.contains_key(profile_name);
    if project.profiles.len() == before && !recorded {
        return Ok(()); // already detached, nothing to do
    }

    core::reconcile_project_profiles(&mut project);
    project.updated_at = chrono::Utc::now().to_rfc3339();
    let json = serde_json::to_string_pretty(&project)
        .map_err(|e| format!("Failed to serialise project: {}", e))?;
    core::save_project(project_name, &json)?;
    sync_project_if_configured(project_name, &mut project);
    Ok(())
}
