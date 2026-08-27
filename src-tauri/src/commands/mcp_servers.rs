use crate::core;

use super::projects::{
    prune_mcp_server_from_projects, rename_mcp_server_in_projects,
    rename_mcp_server_in_templates, sync_projects_referencing_mcp_server,
};

// ── MCP Servers ──────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_mcp_servers() -> Result<String, String> {
    core::list_mcp_servers()
}

#[tauri::command]
pub fn list_mcp_server_configs() -> Result<Vec<String>, String> {
    core::list_mcp_server_configs()
}

#[tauri::command]
pub fn read_mcp_server_config(name: &str) -> Result<String, String> {
    core::read_mcp_server_config(name)
}

#[tauri::command]
pub fn save_mcp_server_config(name: &str, data: &str) -> Result<(), String> {
    core::save_mcp_server_config(name, data)?;
    sync_projects_referencing_mcp_server(name);
    // Global assignments render the same registry entry per agent, so an
    // edit here should propagate to every agent that has this server
    // assigned globally too.  Best-effort — individual failures are logged
    // in the orchestrator, not propagated (mirrors the projects path).
    crate::sync::global_mcp::reapply_agents_referencing(name);
    Ok(())
}

#[tauri::command]
pub fn rename_mcp_server_config(old_name: &str, new_name: &str) -> Result<(), String> {
    if old_name == new_name {
        return Ok(());
    }
    if core::is_builtin_mcp_server(old_name) {
        return Err(format!("Cannot rename built-in MCP server '{}'", old_name));
    }
    core::rename_mcp_server_config(old_name, new_name)?;

    // Rewrite every project and template that referenced the old name so no
    // dangling references remain. Projects are re-synced from within the
    // project helper so on-disk agent config reflects the new name.
    rename_mcp_server_in_projects(old_name, new_name);
    rename_mcp_server_in_templates(old_name, new_name);

    // Rewrite global-MCP selections and re-apply for every agent that had
    // this server assigned globally, then reapply for the new name in case
    // any downstream renderer needs the freshly-named entry propagated.
    crate::sync::global_mcp::rename_server_in_global(old_name, new_name);
    crate::sync::global_mcp::reapply_agents_referencing(new_name);

    Ok(())
}

#[tauri::command]
pub fn delete_mcp_server_config(name: &str) -> Result<(), String> {
    if core::is_builtin_mcp_server(name) {
        return Err(format!("Cannot delete built-in MCP server '{}'", name));
    }
    core::delete_mcp_server_config(name)?;
    prune_mcp_server_from_projects(name);
    // Drop the server from every agent's global selection and rewrite the
    // affected files so the entry disappears from any place Automatic
    // manages it.
    crate::sync::global_mcp::prune_server_from_global(name);
    Ok(())
}

#[tauri::command]
pub async fn check_mcp_server_status(name: String) -> Result<core::McpServerAvailability, String> {
    core::check_mcp_server_status(&name).await
}

/// Check whether a raw stdio command string resolves to an executable —
/// used by the MCP server editor to warn the user while they type that
/// their configured command (e.g. `npx`, `uvx`) isn't installed on the
/// system.  Trimmed and empty-checked here so the frontend can pass the
/// input value straight through.
#[tauri::command]
pub fn check_mcp_command_available(command: String) -> core::McpServerAvailability {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return core::McpServerAvailability {
            available: false,
            message: None,
        };
    }
    core::check_mcp_command_available(trimmed)
}

// ── MCP Discover ─────────────────────────────────────────────────────────────

/// Return all MCP server Discover catalogue entries matching `query` as a JSON array.
/// When `query` is blank, all entries are returned.
#[tauri::command]
pub fn search_discover_mcp(query: &str) -> Result<String, String> {
    core::search_discover_mcp(query)
}

/// Return all collections matching `query` as a JSON array.
/// When `query` is blank, all entries are returned.
#[tauri::command]
pub fn search_collections(query: &str) -> Result<String, String> {
    core::search_collections(query)
}
