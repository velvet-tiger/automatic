use crate::core;

use super::projects::{sync_projects_referencing_mcp_server, sync_projects_referencing_skill};

// ── Agent Plugin packages ────────────────────────────────────────────────────
//
// Each command reads files, so each runs on a blocking thread and never on
// the main thread.

/// True when the folder has a `plugin.json` at its root, so the import
/// dialog should treat it as an Agent Plugin package.
#[tauri::command]
pub async fn is_agent_package_dir(path: String) -> Result<bool, String> {
    tokio::task::spawn_blocking(move || core::is_agent_package_dir(&path))
        .await
        .map_err(|e| format!("is_agent_package_dir task join error: {e}"))
}

/// Read a package and plan its import without changing anything, for the
/// confirm step. The plan lists the projects each new item reaches.
#[tauri::command]
pub async fn preview_agent_package(path: String) -> Result<core::AgentPackagePreview, String> {
    tokio::task::spawn_blocking(move || core::preview_agent_package(&path))
        .await
        .map_err(|e| format!("preview_agent_package task join error: {e}"))?
}

/// Import an Agent Plugin package's skills and MCP servers into the library.
/// `expected_digest` is the digest the preview showed; the import refuses a
/// package that changed since. Projects that already referenced one of the
/// new names are synced, as for any other library save; the preview lists
/// them so the user agrees to that before importing.
#[tauri::command]
pub async fn import_agent_package(
    path: String,
    expected_digest: String,
) -> Result<core::AgentPackageImportResult, String> {
    tokio::task::spawn_blocking(move || {
        let result = core::import_agent_package(&path, &expected_digest)?;
        for name in &result.imported_skills {
            core::record_recently_added("skills", name);
            sync_projects_referencing_skill(name);
        }
        for name in &result.imported_mcp_servers {
            sync_projects_referencing_mcp_server(name);
        }
        Ok(result)
    })
    .await
    .map_err(|e| format!("import_agent_package task join error: {e}"))?
}

/// List the Agent Plugin packages installed in `~/.agents/plugins`.
#[tauri::command]
pub async fn list_installed_agent_packages() -> Result<core::InstalledAgentPackages, String> {
    tokio::task::spawn_blocking(core::list_installed_agent_packages)
        .await
        .map_err(|e| format!("list_installed_agent_packages task join error: {e}"))?
}
