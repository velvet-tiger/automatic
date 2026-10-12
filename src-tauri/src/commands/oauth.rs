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
    // Storing a token flips the rendered config from direct-entry to a
    // local `mcp-proxy` stub, so global assignments must re-emit too.
    crate::sync::global_mcp::reapply_agents_referencing(&server_name);
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

    // The check sends the token to `mcp_url`, which the caller supplies. The
    // token is only ever used with the saved URL, so no other URL gets it.
    if !crate::core::mcp_server_url_is(&server_name, &mcp_url)? {
        return Ok(McpOAuthTokenStatus {
            has_token: true,
            valid: false,
            revoked: false,
            message: Some(
                "Cannot verify token because this is not the URL saved for the MCP server. Save your changes, or reopen the server if it was changed elsewhere."
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

/// Remove the stored OAuth token and refresh credentials for a given MCP
/// server.
///
/// The credentials go first. The UI offers Revoke only while a token is
/// stored, so a failure must leave the token in place for a retry to be
/// possible. Returns an error when no token is stored, after removing any
/// credentials that were left behind.
#[tauri::command]
pub fn revoke_mcp_oauth_token(server_name: String) -> Result<(), String> {
    crate::proxy::delete_oauth_credentials(&server_name)?;
    crate::proxy::delete_oauth_token(&server_name)
}

/// Attempt to refresh an expired OAuth token using stored credentials.
#[tauri::command]
pub async fn refresh_mcp_oauth_token(server_name: String) -> Result<String, String> {
    crate::oauth::refresh_token(&server_name).await
}

#[cfg(test)]
mod tests {
    use super::*;

    // Each test uses its own server name. The test secret store and the
    // token presence cache are shared by the whole test process.

    #[test]
    fn the_token_check_does_not_send_the_token_to_a_url_that_is_not_saved() {
        let home = tempfile::tempdir().expect("tempdir");
        crate::core::with_test_home(home.path().to_path_buf(), || {
            let server = "status-saved-url-fixture";
            crate::core::save_mcp_server_config(
                server,
                r#"{"type":"http","url":"https://mcp.example.com/mcp"}"#,
            )
            .expect("save");
            crate::proxy::store_oauth_token(server, "access-token").unwrap();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime");

            // Nothing listens on this port. A request to it would come back
            // as an error, not as a status.
            let status = runtime
                .block_on(get_mcp_oauth_token_status(
                    server.to_string(),
                    "http://127.0.0.1:9/mcp".to_string(),
                ))
                .expect("status");

            assert!(status.has_token);
            assert!(!status.valid);
            assert!(!status.revoked);
            assert!(status.message.expect("message").contains("not the URL saved"));
        });
    }

    #[test]
    fn revoke_removes_token_and_refresh_credentials() {
        let server = "revoke-test-token-and-creds-fixture";
        crate::proxy::store_oauth_token(server, "access-token").unwrap();
        crate::proxy::store_oauth_credentials(server, r#"{"refresh_token":"refresh"}"#).unwrap();
        assert!(crate::proxy::has_oauth_token(server));

        revoke_mcp_oauth_token(server.to_string()).unwrap();

        assert!(crate::proxy::load_oauth_token(server).is_err());
        assert!(crate::proxy::load_oauth_credentials(server).is_err());
        assert!(!crate::proxy::has_oauth_token(server));
    }

    #[test]
    fn revoke_succeeds_without_refresh_credentials() {
        let server = "revoke-test-token-only-fixture";
        crate::proxy::store_oauth_token(server, "access-token").unwrap();

        revoke_mcp_oauth_token(server.to_string()).unwrap();

        assert!(crate::proxy::load_oauth_token(server).is_err());
        assert!(!crate::proxy::has_oauth_token(server));
    }

    #[test]
    fn revoke_without_token_still_removes_refresh_credentials() {
        let server = "revoke-test-creds-only-fixture";
        crate::proxy::store_oauth_credentials(server, r#"{"refresh_token":"refresh"}"#).unwrap();

        assert!(revoke_mcp_oauth_token(server.to_string()).is_err());

        assert!(crate::proxy::load_oauth_credentials(server).is_err());
    }
}
