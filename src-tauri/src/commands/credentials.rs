use crate::core;

// ── API Keys ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn save_api_key(provider: &str, key: &str) -> Result<(), String> {
    core::save_api_key(provider, key)
}

#[tauri::command]
pub fn get_api_key(provider: &str) -> Result<String, String> {
    core::get_api_key(provider)
}

#[tauri::command]
pub fn has_api_key(provider: &str) -> bool {
    core::has_api_key(provider)
}

#[tauri::command]
pub fn delete_api_key(provider: &str) -> Result<(), String> {
    core::delete_api_key(provider)
}

/// Effective state of the master "Agent features" toggle. Used by the frontend
/// to gate AI buttons / inputs purely on whether the user has agent features
/// turned on, independent of whether an API key is currently stored.
#[tauri::command]
pub fn agent_features_enabled() -> bool {
    core::ai::agent_features_enabled()
}
