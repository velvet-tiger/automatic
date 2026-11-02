use crate::agent;
use crate::core;

// ── Settings ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn read_settings() -> Result<core::Settings, String> {
    core::read_settings()
}

#[tauri::command]
pub fn write_settings(settings: core::Settings) -> Result<(), String> {
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
