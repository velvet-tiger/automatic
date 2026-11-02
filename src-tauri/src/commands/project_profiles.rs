use crate::core;

use super::project_sweeps::{
    ensure_not_being_deleted, hold_library_writes, queue_sweep, run_delete_and_wait,
    run_sweep_and_wait,
};
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
#[tauri::command(async)]
pub fn save_project_profile(name: &str, data: &str) -> Result<(), String> {
    ensure_not_being_deleted(&format!("profile:{name}"), name)?;
    let writes = hold_library_writes();
    core::save_project_profile(name, data)?;
    drop(writes);
    let profile = name.to_string();
    queue_sweep(
        Some(format!("profile:{profile}")),
        format!("profile {profile}"),
        move |progress| reconcile_projects_referencing_profile(&profile, progress),
    );
    Ok(())
}

/// Detach the profile from every project first so each one loses every entry
/// the profile provides, then remove the file.
#[tauri::command(async)]
pub fn delete_project_profile(name: &str) -> Result<(), String> {
    let profile = name.to_string();
    run_delete_and_wait(
        format!("profile:{profile}"),
        format!("deleted profile {profile}"),
        move |progress| {
            detach_profile_from_projects(&profile, progress);
            core::delete_project_profile(&profile)
        },
    )
}

#[tauri::command(async)]
pub fn rename_project_profile(old_name: &str, new_name: &str) -> Result<(), String> {
    let (old_name, new_name) = (old_name.to_string(), new_name.to_string());
    run_sweep_and_wait(format!("renamed profile {new_name}"), move |progress| {
        core::rename_project_profile(&old_name, &new_name)?;
        rename_profile_in_projects(&old_name, &new_name, progress);
        Ok(())
    })
}

/// Every project that has the profile attached.
#[tauri::command(async)]
pub fn get_projects_referencing_profile(
    profile_name: &str,
) -> Result<Vec<core::ProjectRef>, String> {
    let mut referencing = Vec::new();
    with_each_project_mut(|_, project| {
        if project.profiles.iter().any(|p| p == profile_name) {
            referencing.push(core::ProjectRef {
                name: project.name.clone(),
                directory: project.directory.clone(),
                local_key: core::local_key_of(project),
            });
        }
    });
    referencing.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(referencing)
}

#[tauri::command(async)]
pub fn attach_profile_to_project(project_name: &str, profile_name: &str) -> Result<(), String> {
    let _writes = hold_library_writes();
    let project_name = &crate::core::canonical_project_ident(project_name)?;
    core::read_project_profile_parsed(profile_name)?;

    let raw = core::read_project(project_name)?;
    let mut project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    core::reconcile_group_profiles(&mut project, &core::groups_listing(project_name)?);
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

#[tauri::command(async)]
pub fn detach_profile_from_project(project_name: &str, profile_name: &str) -> Result<(), String> {
    let _writes = hold_library_writes();
    let project_name = &crate::core::canonical_project_ident(project_name)?;
    let raw = core::read_project(project_name)?;
    let mut project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    // Reconcile against the groups on disk first, so a stale record cannot
    // refuse a detach the group no longer stands behind.
    let groups_changed =
        core::reconcile_group_profiles(&mut project, &core::groups_listing(project_name)?);
    if let Some(group) = core::group_providing_profile(&project, profile_name) {
        return Err(format!(
            "Profile '{}' is provided by group '{}'. Detach it from the group instead.",
            profile_name, group
        ));
    }

    if !core::release_profile(&mut project, profile_name) && !groups_changed {
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
