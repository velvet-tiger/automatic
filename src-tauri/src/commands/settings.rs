use crate::agent;
use crate::core;

// ── Settings ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn read_settings() -> Result<core::Settings, String> {
    let mut settings = core::read_settings()?;
    // The record is only for the backend. `write_settings` keeps the copy on
    // disk, so leaving it out here loses nothing.
    if let Some(gateway) = &mut settings.custom_gateway {
        gateway.claude_code = None;
    }
    Ok(settings)
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

#[cfg(test)]
mod custom_gateway_tests {
    use super::*;
    use crate::core::custom_gateway::test_support::{gateway_store_lock, save_test_gateway};

    #[test]
    fn the_frontend_never_sees_the_record_and_cannot_lose_it() {
        let _store = gateway_store_lock();
        let _home = core::temp_test_home();
        save_test_gateway("gateway-token");

        let seen = read_settings().expect("read");
        assert!(seen.custom_gateway.as_ref().expect("gateway").claude_code.is_none());

        write_settings(seen).expect("write");

        let kept = core::read_settings().expect("read").custom_gateway.expect("gateway");
        assert!(kept.claude_code.is_some());
        core::clear_custom_gateway().expect("clean up");
    }
}
