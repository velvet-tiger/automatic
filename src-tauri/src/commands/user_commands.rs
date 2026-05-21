use crate::core;

use super::projects::sync_projects_referencing_user_command;

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
    core::save_user_command(&machine_name, &content)?;
    sync_projects_referencing_user_command(&machine_name);
    Ok(())
}

#[tauri::command]
pub fn rename_user_command(old_name: String, new_name: String) -> Result<(), String> {
    core::rename_user_command(&old_name, &new_name)?;
    // After rename the new name is what projects pick up on next sync, but
    // any project that already had the old name selected has now been
    // renamed (see core::rename_user_command), so re-sync those.
    sync_projects_referencing_user_command(&new_name);
    Ok(())
}

#[tauri::command]
pub fn delete_user_command(machine_name: String) -> Result<(), String> {
    core::delete_user_command(&machine_name)
}
