use crate::agent;
use crate::core;

// ── Settings ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn read_settings() -> Result<core::Settings, String> {
    core::read_settings()
}

/// Write the settings the Settings page edits. Where the writable folders go,
/// and the user's agreement to user settings, change only through
/// `set_writable_folders_target` and `set_writable_folders_user_consent`, so
/// a stale or partial object cannot grant or drop that agreement. A damaged
/// file reads as defaults, which withhold the agreement, and is healed.
#[tauri::command]
pub fn write_settings(mut settings: core::Settings) -> Result<(), String> {
    let stored = core::read_settings()?;
    settings.writable_folders_target = stored.writable_folders_target;
    settings.writable_folders_user_consent = stored.writable_folders_user_consent;
    core::write_settings(&settings)
}

#[tauri::command]
pub fn reset_settings() -> Result<(), String> {
    core::reset_settings()
}

#[tauri::command(async)]
pub fn reinstall_defaults() -> Result<(), String> {
    let label = "reinstalled defaults".to_string();
    crate::commands::project_sweeps::run_sweep_and_wait(label, |progress| {
        core::reinstall_defaults()?;
        // Bundled skills/rules/instructions/subagents were just force-overwritten
        // in the library. Propagate them to every configured project so the update
        // never surfaces as drift.
        crate::commands::projects::resync_all_projects(progress);
        Ok(())
    })
}

#[tauri::command]
pub fn erase_app_data() -> Result<core::ClearedCustomGateway, String> {
    core::erase_app_data()
}

#[tauri::command]
pub fn dismiss_welcome() -> Result<(), String> {
    core::dismiss_welcome()
}

#[tauri::command]
pub fn clear_opencode_cache() -> Result<agent::ClearCacheResult, String> {
    agent::clear_opencode_cache()
}

#[tauri::command]
pub fn clean_opencode_snapshots() -> Result<agent::CleanSnapshotsResult, String> {
    agent::clean_opencode_snapshots()
}
