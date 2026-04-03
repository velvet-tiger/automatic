// ── OAuth Commands ────────────────────────────────────────────────────────────

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use serde::Serialize;

const MCP_PROTOCOL_VERSION_HEADER: &str = "mcp-protocol-version";
const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct McpOAuthTokenStatus {
    pub has_token: bool,
    pub valid: bool,
    pub revoked: bool,
    pub message: Option<String>,
}

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

/// Validate whether the stored OAuth token is still accepted by the remote MCP server.
#[tauri::command]
pub async fn get_mcp_oauth_token_status(
    server_name: String,
    mcp_url: String,
) -> Result<McpOAuthTokenStatus, String> {
    let token = match crate::proxy::load_oauth_token(&server_name) {
        Ok(token) => token,
        Err(_) => {
            return Ok(McpOAuthTokenStatus {
                has_token: false,
                valid: false,
                revoked: false,
                message: None,
            });
        }
    };

    if mcp_url.trim().is_empty() {
        return Ok(McpOAuthTokenStatus {
            has_token: true,
            valid: false,
            revoked: false,
            message: Some(
                "Cannot verify token because this MCP server has no URL configured yet."
                    .to_string(),
            ),
        });
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {}", e))?;

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/json, text/event-stream"),
    );
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", token))
            .map_err(|e| format!("invalid stored token: {}", e))?,
    );
    headers.insert(
        HeaderName::from_static(MCP_PROTOCOL_VERSION_HEADER),
        HeaderValue::from_static(MCP_PROTOCOL_VERSION),
    );

    let probe = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "automatic-auth-check",
        "method": "initialize",
        "params": {
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {
                "name": "Automatic",
                "version": env!("CARGO_PKG_VERSION"),
            }
        }
    });

    let response = client
        .post(&mcp_url)
        .headers(headers)
        .json(&probe)
        .send()
        .await
        .map_err(|e| format!("token validation request failed: {}", e))?;

    let status = response.status();
    if status.is_success() {
        return Ok(McpOAuthTokenStatus {
            has_token: true,
            valid: true,
            revoked: false,
            message: None,
        });
    }

    let body = response.text().await.unwrap_or_default();
    let body_lower = body.to_lowercase();
    let revoked = status.as_u16() == 401
        || status.as_u16() == 403
        || body_lower.contains("invalid_token")
        || body_lower.contains("token revoked")
        || body_lower.contains("revoked token")
        || body_lower.contains("token has been revoked")
        || body_lower.contains("invalid_grant");

    Ok(McpOAuthTokenStatus {
        has_token: true,
        valid: false,
        revoked,
        message: Some(if revoked {
            "Stored token is no longer accepted by the MCP server. Re-authenticate to restore access.".to_string()
        } else if body.trim().is_empty() {
            format!(
                "Automatic could not verify this token right now (HTTP {}).",
                status.as_u16()
            )
        } else {
            format!(
                "Automatic could not verify this token right now (HTTP {}): {}",
                status.as_u16(),
                body
            )
        }),
    })
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
