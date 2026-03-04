use crate::core;

// ── Author Resolution ─────────────────────────────────────────────────────────

/// Resolve a raw author descriptor JSON string into a fully-enriched
/// AuthorProfile for display.  Network calls (GitHub API) are made
/// transparently; errors produce safe fallbacks.
///
/// `descriptor` must be a JSON string matching the AuthorDescriptor shape:
///   `{ "type": "github", "repo": "owner/repo" }`
///   `{ "type": "provider", "name": "Acme", "url": "https://acme.com" }`
///   `{ "type": "local" }`
#[tauri::command]
pub async fn resolve_author(descriptor: String) -> Result<core::AuthorProfile, String> {
    core::resolve_author_json(&descriptor).await
}

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

// ── Directory Picker ──────────────────────────────────────────────────────────

/// Open a native folder-picker dialog and return the selected path.
///
/// On macOS we bypass `rfd`/`NSOpenPanel` entirely and use `osascript`
/// because `rfd 0.16` panics on Apple-Silicon Macs when `NSOpenPanel`
/// unexpectedly returns NULL (upstream issue: PolyMeilex/rfd#259).
///
/// Returns `Ok(Some(path))` when a folder is chosen, `Ok(None)` when the
/// user cancels, or `Err(message)` if the picker itself fails.
#[tauri::command]
pub async fn open_directory_dialog() -> Result<Option<String>, String> {
    #[cfg(target_os = "macos")]
    {
        // Use `osascript` to show a choose-folder dialog.  This is reliable
        // on all Apple-Silicon (M-series) Macs and avoids the rfd/NSOpenPanel
        // NULL-pointer panic.
        let output = std::process::Command::new("osascript")
            .args([
                "-e",
                "set result to choose folder with prompt \"Select project directory\"\n\
                 POSIX path of result",
            ])
            .output()
            .map_err(|e| format!("Failed to launch osascript: {e}"))?;

        if output.status.success() {
            let raw = String::from_utf8_lossy(&output.stdout);
            let path = raw.trim().trim_end_matches('/').to_string();
            if path.is_empty() {
                Ok(None)
            } else {
                Ok(Some(path))
            }
        } else {
            // Exit code 1 with "User canceled" in stderr means the user
            // dismissed the dialog — treat that as a normal cancellation.
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("User canceled") || stderr.contains("user canceled") {
                Ok(None)
            } else {
                Err(format!("osascript error: {}", stderr.trim()))
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        // On non-macOS platforms the rfd panic does not occur; use the
        // tauri-plugin-dialog blocking API from a background thread.
        Err("open_directory_dialog: not implemented on this platform".to_string())
    }
}
