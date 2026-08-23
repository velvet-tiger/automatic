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

/// End-to-end refresh triggered by the Settings UI's "check now" button
/// or by the background scheduler. Combines check_for_update →
/// download_and_verify → apply.
///
/// Returns the applied version on success, or `None` when there was no
/// newer release. Errors describe transport, signature, hash, or write
/// failures.
#[tauri::command]
pub async fn apply_library_update() -> Result<Option<String>, String> {
    let Some(release) = core::library_refresh::check_for_update().await? else {
        return Ok(None);
    };
    let verified = core::library_refresh::download_and_verify(&release).await?;
    let version = verified.version.clone();
    // apply is synchronous fs work; spawn_blocking keeps it off the
    // async executor when called from a Tauri command.
    tokio::task::spawn_blocking(move || core::library_refresh::apply(&verified))
        .await
        .map_err(|e| format!("apply task panicked: {}", e))??;
    Ok(Some(version))
}
