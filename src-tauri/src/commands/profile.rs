use crate::core;

// ── User Profile ─────────────────────────────────────────────────────────────

#[tauri::command]
pub fn read_profile() -> Result<Option<core::UserProfile>, String> {
    core::read_profile()
}

#[tauri::command]
pub fn save_profile(profile: core::UserProfile) -> Result<(), String> {
    core::save_profile(&profile)
}
