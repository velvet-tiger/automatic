use crate::core;

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
    core::save_user_command(&machine_name, &content)
}

#[tauri::command]
pub fn delete_user_command(machine_name: String) -> Result<(), String> {
    core::delete_user_command(&machine_name)
}
