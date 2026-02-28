use crate::core;

use super::projects::{prune_mcp_server_from_projects, sync_projects_referencing_mcp_server};

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
    Ok(())
}

#[tauri::command]
pub fn delete_mcp_server_config(name: &str) -> Result<(), String> {
    core::delete_mcp_server_config(name)?;
    prune_mcp_server_from_projects(name);
    Ok(())
}
