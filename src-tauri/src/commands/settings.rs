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
