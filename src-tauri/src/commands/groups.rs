use crate::core;

use super::projects::reconcile_group_profiles_for_projects;

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

/// Save a group and bring the contexts and profiles of its old and new
/// members in step. Members whose profiles changed are re-synced.
/// `projects` holds `local_key`s (ids are accepted too); they are stored as
/// project ids. Any other value is stored unchanged.
#[tauri::command]
pub fn save_group(name: &str, data: &str) -> Result<(), String> {
    let mut affected = core::group_member_idents(name);
    core::save_group_reconciling_contexts(name, data)?;
    affected.extend(core::group_member_idents(name));
    reconcile_group_profiles_for_projects(&affected);
    Ok(())
}

/// Delete a group and release the contexts and profiles it provided to its
/// members. Members that lose a profile are re-synced.
#[tauri::command]
pub fn delete_group(name: &str) -> Result<(), String> {
    let members = core::group_member_idents(name);
    core::delete_group_reconciling_contexts(name)?;
    reconcile_group_profiles_for_projects(&members);
    Ok(())
}

// ── Group profiles ───────────────────────────────────────────────────────────

/// Attach a profile to a group, then give it to every member project and
/// re-sync the ones that changed. Fails when the profile or the group does
/// not exist. Returns `false` when the group already listed the profile;
/// members are brought in step either way.
pub(crate) fn attach_group_profile(group_name: &str, profile_name: &str) -> Result<bool, String> {
    let changed = core::add_profile_to_group(group_name, profile_name)?;
    reconcile_group_profiles_for_projects(&core::group_member_idents(group_name));
    Ok(changed)
}

/// Detach a profile from a group, then take it from every member project no
/// other group provides it to, and re-sync the ones that changed. Fails when
/// the group does not exist. Returns `false` when the group did not list the
/// profile; members are brought in step either way.
pub(crate) fn detach_group_profile(group_name: &str, profile_name: &str) -> Result<bool, String> {
    let changed = core::remove_profile_from_group(group_name, profile_name)?;
    reconcile_group_profiles_for_projects(&core::group_member_idents(group_name));
    Ok(changed)
}

/// Attach a profile to a group. Every member project receives it and is
/// re-synced. Idempotent.
#[tauri::command]
pub fn attach_profile_to_group(group_name: &str, profile_name: &str) -> Result<(), String> {
    attach_group_profile(group_name, profile_name).map(|_| ())
}

/// Detach a profile from a group. Member projects lose it, unless another
/// of their groups provides it, and are re-synced. Idempotent.
#[tauri::command]
pub fn detach_profile_from_group(group_name: &str, profile_name: &str) -> Result<(), String> {
    detach_group_profile(group_name, profile_name).map(|_| ())
}

/// Names of the groups that list the profile, sorted.
#[tauri::command]
pub fn get_groups_referencing_profile(profile_name: &str) -> Result<Vec<String>, String> {
    core::groups_referencing_profile(profile_name)
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
