//! Tauri commands for the content library surface: current versions and
//! upstream update polling. Download/verify/apply commands arrive in
//! Phase 3b alongside the signing pipeline.

use serde::{Deserialize, Serialize};

use crate::core;

/// What the Settings UI needs to render "Content library: vX.Y.Z" and the
/// "check for updates" affordance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryStatus {
    /// Semver bundled with the current binary. Always present.
    pub bundled_version: String,
    /// Semver recorded in `settings.json` as last installed. `None` on a
    /// fresh install between the first boot and the version-persist step.
    pub installed_version: Option<String>,
}

#[tauri::command]
pub fn get_library_version() -> Result<LibraryStatus, String> {
    let installed = core::read_settings().ok().and_then(|s| s.library_version);
    Ok(LibraryStatus {
        bundled_version: core::bundled_library::version().to_string(),
        installed_version: installed,
    })
}

#[tauri::command]
pub async fn check_library_updates() -> Result<Option<core::library_refresh::LibraryRelease>, String>
{
    core::library_refresh::check_for_update().await
}
