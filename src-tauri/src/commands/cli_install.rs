//! Tauri command wrappers around `core::cli_install`.
//!
//! These three commands are invoked from the Settings → Command Line page.
//! The wrappers add nothing beyond the `#[tauri::command]` attribute so the
//! frontend can call them via `invoke`. All real logic lives in `core/`.

use crate::core::cli_install::{self, CliInstallStatus};

#[tauri::command]
pub fn cli_install_status() -> Result<CliInstallStatus, String> {
    cli_install::status()
}

#[tauri::command]
pub fn cli_install_install() -> Result<String, String> {
    cli_install::install()
}

#[tauri::command]
pub fn cli_install_uninstall() -> Result<String, String> {
    cli_install::uninstall()
}
