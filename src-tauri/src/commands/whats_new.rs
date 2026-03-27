use crate::core;

#[tauri::command]
pub fn get_whats_new() -> Result<core::WhatsNewResponse, String> {
    core::get_whats_new()
}

#[tauri::command]
pub fn mark_whats_new_seen() -> Result<(), String> {
    core::mark_whats_new_seen()
}
