use crate::core;

// ── Newsletter ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn subscribe_newsletter(email: String) -> Result<(), String> {
    core::subscribe_newsletter(&email).await
}

// ── Editor Detection & Open ───────────────────────────────────────────────────

#[tauri::command]
pub fn check_installed_editors() -> Vec<core::EditorInfo> {
    core::check_installed_editors()
}

#[tauri::command]
pub fn open_in_editor(editor_id: &str, path: &str) -> Result<(), String> {
    core::open_in_editor(editor_id, path)
}

#[tauri::command]
pub fn get_editor_icon(editor_id: &str) -> Result<String, String> {
    core::get_editor_icon(editor_id)
}

// ── Analytics ────────────────────────────────────────────────────────────────

/// Track an event via Amplitude's HTTP API v2.
/// Fire-and-forget from the frontend -- errors are logged but not surfaced.
#[tauri::command]
pub async fn track_event(
    user_id: String,
    event: String,
    properties: Option<serde_json::Value>,
    enabled: bool,
) -> Result<(), String> {
    core::track_event(&user_id, &event, properties, enabled).await
}

// ── Plugins / Sessions ───────────────────────────────────────────────────────

#[tauri::command]
pub fn install_plugin_marketplace() -> Result<String, String> {
    core::install_plugin_marketplace()
}

#[tauri::command]
pub fn get_sessions() -> Result<String, String> {
    core::list_sessions()
}

// ── App Updates ───────────────────────────────────────────────────────────────

/// Restart the application to apply a freshly-installed update.
#[tauri::command]
pub fn restart_app(app: tauri::AppHandle) {
    app.restart();
}
