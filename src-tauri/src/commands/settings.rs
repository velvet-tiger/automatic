use crate::agent;
use crate::core;

// ── Settings ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn read_settings() -> Result<core::Settings, String> {
    core::read_settings()
}

#[tauri::command]
pub fn write_settings(settings: core::Settings) -> Result<(), String> {
    core::write_settings(&settings)
}

#[tauri::command]
pub fn reset_settings() -> Result<(), String> {
    core::reset_settings()
}

#[tauri::command]
pub fn reinstall_defaults() -> Result<(), String> {
    core::reinstall_defaults()
}

#[tauri::command]
pub fn erase_app_data() -> Result<(), String> {
    core::erase_app_data()
}

#[tauri::command]
pub fn dismiss_welcome() -> Result<(), String> {
    core::dismiss_welcome()
}

#[tauri::command]
pub fn clear_opencode_cache() -> Result<agent::ClearCacheResult, String> {
    agent::clear_opencode_cache()
}

#[tauri::command]
pub fn clean_opencode_snapshots() -> Result<agent::CleanSnapshotsResult, String> {
    agent::clean_opencode_snapshots()
}
