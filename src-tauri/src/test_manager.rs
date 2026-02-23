use tauri::Manager;
pub fn test_fn(app: &tauri::AppHandle) {
    let is_active = app.webview_windows().values().any(|window| {
        window.is_visible().unwrap_or(false) || window.is_focused().unwrap_or(false)
    });
}
