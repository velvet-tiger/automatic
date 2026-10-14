use crate::core;

use super::projects::{sync_projects_referencing_mcp_server, sync_projects_referencing_skill};

// ── Agent Plugin packages ────────────────────────────────────────────────────

/// True when the folder has a `plugin.json` at its root, so the import
/// dialog should treat it as an Agent Plugin package.
#[tauri::command]
pub fn is_agent_package_dir(path: String) -> bool {
    core::is_agent_package_dir(&path)
}

/// Import an Agent Plugin package's skills and MCP servers into the library.
/// Only new names are added, so a re-sync only matters for projects that
/// already referenced a name before it existed in the library.
#[tauri::command]
pub fn import_agent_package(path: String) -> Result<core::AgentPackageImportResult, String> {
    let result = core::import_agent_package(&path)?;
    for name in &result.imported_skills {
        core::record_recently_added("skills", name);
        sync_projects_referencing_skill(name);
    }
    for name in &result.imported_mcp_servers {
        sync_projects_referencing_mcp_server(name);
    }
    Ok(result)
}

/// List the Agent Plugin packages installed in `~/.agents/plugins`.
#[tauri::command]
pub fn list_installed_agent_packages() -> Result<core::InstalledAgentPackages, String> {
    core::list_installed_agent_packages()
}
