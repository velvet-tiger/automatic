use crate::core;

// ── App plugin commands ───────────────────────────────────────────────────────

/// Returns all bundled plugins with their current enabled/disabled state.
#[tauri::command]
pub fn list_app_plugins() -> Result<Vec<core::PluginEntry>, String> {
    core::list_app_plugins()
}

/// Enable or disable a plugin by its stable id.
#[tauri::command]
pub fn set_app_plugin_enabled(id: String, enabled: bool) -> Result<(), String> {
    core::set_app_plugin_enabled(&id, enabled)
}

/// Returns true if the given plugin id is currently enabled.
#[tauri::command]
pub fn is_app_plugin_enabled(id: String) -> Result<bool, String> {
    core::is_app_plugin_enabled(&id)
}
