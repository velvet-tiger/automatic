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

/// Resources locked by plugins for a given set of project tools.
#[derive(serde::Serialize)]
pub struct PluginLockedResources {
    pub skills: Vec<String>,
    pub rules: Vec<String>,
}

/// Given a list of tool names present on a project, return the skill and rule
/// names that are provided by enabled plugins and should not be removable.
#[tauri::command]
pub fn get_plugin_locked_resources(tools: Vec<String>) -> Result<PluginLockedResources, String> {
    let (skills, rules) = core::get_plugin_locked_resources(&tools);
    Ok(PluginLockedResources { skills, rules })
}
