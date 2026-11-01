use crate::core;

use super::project_sweeps::{
    hold_project_writes, queue_sweep, run_sweep_and_wait, SweepProgress,
};
use super::projects::{
    reconcile_projects_referencing_profile, sync_projects_referencing_user_command,
};

#[tauri::command]
pub fn get_user_commands() -> Result<Vec<core::UserCommandEntry>, String> {
    core::list_user_commands()
}

#[tauri::command]
pub fn read_user_command(machine_name: String) -> Result<String, String> {
    core::read_user_command(&machine_name)
}

#[tauri::command]
pub fn save_user_command(machine_name: String, content: String) -> Result<(), String> {
    core::user_writes::save_user_command(&machine_name, &content)?;
    queue_sweep(
        Some(format!("command:{machine_name}")),
        format!("command {machine_name}"),
        move |progress| sync_projects_referencing_user_command(&machine_name, progress),
    );
    Ok(())
}

#[tauri::command(async)]
pub fn rename_user_command(old_name: String, new_name: String) -> Result<(), String> {
    run_sweep_and_wait(format!("renamed command {new_name}"), move |progress| {
        // The rename rewrites each project that selected the command.
        let writes = hold_project_writes();
        core::rename_user_command(&old_name, &new_name)?;
        drop(writes);
        for profile in core::rename_asset_in_profiles(
            core::ProfileResourceKind::UserCommand,
            &old_name,
            &new_name,
        ) {
            reconcile_projects_referencing_profile(&profile, progress);
        }
        // After rename the new name is what projects pick up on next sync, but
        // any project that already had the old name selected has now been
        // renamed (see core::rename_user_command), so re-sync those.
        sync_projects_referencing_user_command(&new_name, progress);
        Ok(())
    })
}

#[tauri::command(async)]
pub fn delete_user_command(machine_name: String) -> Result<(), String> {
    run_sweep_and_wait(format!("deleted command {machine_name}"), move |progress| {
        core::user_writes::delete_user_command(&machine_name)?;
        prune_user_command_references(&machine_name, progress);
        Ok(())
    })
}

/// Take a deleted command out of every profile, and bring the projects that
/// use those profiles back in step. Run after the command is deleted.
pub(crate) fn prune_user_command_references(machine_name: &str, progress: &SweepProgress) {
    for profile in
        core::prune_asset_from_profiles(core::ProfileResourceKind::UserCommand, machine_name)
    {
        reconcile_projects_referencing_profile(&profile, progress);
    }
}
