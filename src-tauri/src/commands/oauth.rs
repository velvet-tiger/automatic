// ── OAuth Commands ────────────────────────────────────────────────────────────

/// Trigger the full OAuth 2.1 flow for a remote MCP server.
/// Opens the user's browser for authorization and stores the resulting token
/// in the system keychain.  After the token is stored, re-syncs all projects
/// that reference this server so the proxy config is written immediately.
#[tauri::command]
pub async fn authorize_mcp_server(server_name: String, mcp_url: String) -> Result<String, String> {
    let token = crate::oauth::authorize_server(&server_name, &mcp_url).await?;
    crate::commands::projects::sync_projects_referencing_mcp_server(&server_name);
    Ok(token)
}

/// Check whether a stored OAuth token exists for a given MCP server.
#[tauri::command]
pub fn has_mcp_oauth_token(server_name: String) -> bool {
    crate::proxy::has_oauth_token(&server_name)
}

/// Remove the stored OAuth token for a given MCP server.
#[tauri::command]
pub fn revoke_mcp_oauth_token(server_name: String) -> Result<(), String> {
    crate::proxy::delete_oauth_token(&server_name)
}

/// Attempt to refresh an expired OAuth token using stored credentials.
#[tauri::command]
pub async fn refresh_mcp_oauth_token(server_name: String) -> Result<String, String> {
    crate::oauth::refresh_token(&server_name).await
}
