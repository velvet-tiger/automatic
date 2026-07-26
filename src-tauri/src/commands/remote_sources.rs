use crate::core::remote_sources;

// ── Tauri Commands for Remote Sources ───────────────────────────────────────

/// Fetch a remote source manifest without installing.
/// Returns the parsed manifest as JSON for the frontend confirmation UI.
///
/// `dir` specifies a subdirectory within the repo where `automatic.json` lives
/// (monorepo support). When omitted, the repo root is used.
#[tauri::command]
pub fn fetch_remote_source(
    repo: String,
    git_ref: Option<String>,
    dir: Option<String>,
) -> Result<String, String> {
    let pin = git_ref.map(|r| remote_sources::PinningConfig {
        strategy: "tag".to_string(),
        git_ref: r,
    });

    let manifest = remote_sources::fetch_source_manifest(&repo, pin, dir.as_deref())?;
    serde_json::to_string(&manifest).map_err(|e| format!("Failed to serialize manifest: {}", e))
}

/// Install resources from a previously fetched source.
/// `selected` is an optional JSON-encoded SelectedResources.
/// `dir` specifies the subdirectory within the repo (must match what was passed to fetch).
#[tauri::command]
pub fn install_remote_source(
    repo: String,
    selected: Option<String>,
    dir: Option<String>,
) -> Result<String, String> {
    let base_dir = remote_sources::resolve_base_dir(&repo, dir.as_deref())?;
    let manifest = remote_sources::parse_manifest(&base_dir)?;

    let selected_resources = match selected {
        Some(json) => {
            let sel: remote_sources::SelectedResources = serde_json::from_str(&json)
                .map_err(|e| format!("Failed to parse selected resources: {}", e))?;
            Some(sel)
        }
        None => None,
    };

    let result =
        remote_sources::install_source(&repo, &manifest, selected_resources, dir.as_deref())?;
    serde_json::to_string(&result).map_err(|e| format!("Failed to serialize install result: {}", e))
}

/// Update a previously installed source (pull latest + re-install).
#[tauri::command]
pub fn update_remote_source(repo: String) -> Result<String, String> {
    let result = remote_sources::update_source(&repo)?;
    serde_json::to_string(&result).map_err(|e| format!("Failed to serialize update result: {}", e))
}

/// Remove a source and all resources it provided.
#[tauri::command]
pub fn remove_remote_source(repo: String) -> Result<String, String> {
    let removed = remote_sources::remove_source(&repo)?;
    serde_json::to_string(&removed).map_err(|e| format!("Failed to serialize result: {}", e))
}

/// List all registered remote sources.
#[tauri::command]
pub fn list_remote_sources() -> Result<String, String> {
    let sources = remote_sources::list_sources()?;
    serde_json::to_string(&sources).map_err(|e| format!("Failed to serialize sources: {}", e))
}

/// Check for conflicts before installing a source.
/// `dir` specifies the subdirectory within the repo (must match what was passed to fetch).
#[tauri::command]
pub fn check_source_conflicts(repo: String, dir: Option<String>) -> Result<String, String> {
    let base_dir = remote_sources::resolve_base_dir(&repo, dir.as_deref())?;
    let manifest = remote_sources::parse_manifest(&base_dir)?;
    let conflicts = remote_sources::check_conflicts(&manifest)?;
    serde_json::to_string(&conflicts).map_err(|e| format!("Failed to serialize conflicts: {}", e))
}

/// Handle an automatic:// deep link URI.
/// Returns the parsed parameters for the frontend to initiate the install flow.
#[tauri::command]
pub fn handle_install_uri(uri: String) -> Result<String, String> {
    let params = remote_sources::parse_install_uri(&uri)?;
    serde_json::to_string(&params).map_err(|e| format!("Failed to serialize: {}", e))
}
