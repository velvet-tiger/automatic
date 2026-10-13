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

/// Check whether a stored OAuth token exists for a given MCP server at its
/// saved URL.
#[tauri::command]
pub fn has_mcp_oauth_token(server_name: String) -> bool {
    match crate::core::saved_mcp_server_url(&server_name) {
        Ok(Some(url)) => crate::proxy::has_oauth_token(&server_name, &url),
        Ok(None) => false,
        Err(e) => {
            eprintln!(
                "warning: could not read the URL of MCP server '{}': {}",
                server_name, e
            );
            false
        }
    }
}

/// Validate whether the stored OAuth token is still accepted by the remote MCP server.
///
/// The token used is the one filed under `mcp_url`, so it is only ever sent
/// to the URL it was issued for. A URL with no token filed under it reports
/// `has_token: false` and sends nothing.
#[tauri::command]
pub async fn get_mcp_oauth_token_status(
    server_name: String,
    mcp_url: String,
) -> Result<McpOAuthTokenStatus, String> {
    let token = match crate::proxy::load_oauth_token(&server_name, &mcp_url) {
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
/// The secrets are the ones filed under the server's saved URL, and any an
/// older version stored under its name alone. The token for the saved URL
/// goes last. The UI offers Revoke only while a token is stored, so a
/// failure must leave that token in place for a retry to be possible.
/// Returns an error when no token is stored, after removing any credentials
/// that were left behind.
#[tauri::command]
pub fn revoke_mcp_oauth_token(server_name: String) -> Result<(), String> {
    let url = saved_url(&server_name)?;
    crate::proxy::delete_legacy_oauth_secrets(&server_name)?;
    crate::proxy::delete_oauth_credentials(&server_name, &url)?;
    crate::proxy::delete_oauth_token(&server_name, &url)
}

/// The saved URL of a server, which its OAuth secrets are filed under.
fn saved_url(server_name: &str) -> Result<String, String> {
    crate::core::saved_mcp_server_url(server_name)?.ok_or_else(|| {
        format!(
            "MCP server '{}' has no saved URL, so it has no OAuth token",
            server_name
        )
    })
}

/// Attempt to refresh an expired OAuth token using stored credentials.
#[tauri::command]
pub async fn refresh_mcp_oauth_token(server_name: String) -> Result<String, String> {
    let url = saved_url(&server_name)?;
    crate::oauth::refresh_token(&server_name, &url).await
}

#[cfg(test)]
mod tests {
    use super::*;

    // Each test uses its own server name. The test secret store and the
    // token presence cache are shared by the whole test process.

    const URL: &str = "https://mcp.example.com/mcp";
    const OTHER_URL: &str = "https://other.example/mcp";

    /// Save an HTTP server at [`URL`] in a home of its own, then run `test`.
    fn with_saved_server(server: &str, test: impl FnOnce()) {
        let home = tempfile::tempdir().expect("tempdir");
        crate::core::with_test_home(home.path().to_path_buf(), || {
            save_server(server, URL);
            test();
        });
    }

    fn save_server(server: &str, url: &str) {
        let config = serde_json::json!({ "type": "http", "url": url }).to_string();
        crate::core::save_mcp_server_config(server, &config).expect("save");
    }

    #[test]
    fn a_server_has_a_token_only_at_the_url_it_signed_in_at() {
        let server = "command-has-token-fixture";
        with_saved_server(server, || {
            assert!(!has_mcp_oauth_token(server.to_string()));

            crate::proxy::store_oauth_token(server, URL, "access-token").unwrap();
            assert!(has_mcp_oauth_token(server.to_string()));

            // The registry file is changed directly, so nothing tidies the
            // token away. It is still stored, and still not found.
            let dir = crate::core::get_mcp_servers_dir().expect("dir");
            let config = serde_json::json!({ "type": "http", "url": OTHER_URL }).to_string();
            std::fs::write(dir.join(format!("{}.json", server)), config).expect("write");

            assert!(!has_mcp_oauth_token(server.to_string()));
            assert!(crate::proxy::load_oauth_token(server, URL).is_ok());
        });
    }

    #[test]
    fn a_server_that_is_not_saved_has_no_token() {
        let home = tempfile::tempdir().expect("tempdir");
        crate::core::with_test_home(home.path().to_path_buf(), || {
            assert!(!has_mcp_oauth_token("command-unsaved-fixture".to_string()));
        });
    }

    #[test]
    fn the_token_check_finds_no_token_for_a_url_the_server_did_not_sign_in_at() {
        let server = "status-saved-url-fixture";
        with_saved_server(server, || {
            crate::proxy::store_oauth_token(server, URL, "access-token").unwrap();
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

            assert!(!status.has_token);
            assert!(!status.valid);
        });
    }

    #[test]
    fn revoke_removes_token_and_refresh_credentials() {
        let server = "revoke-test-token-and-creds-fixture";
        with_saved_server(server, || {
            crate::proxy::store_oauth_token(server, URL, "access-token").unwrap();
            crate::proxy::store_oauth_credentials(server, URL, r#"{"refresh_token":"refresh"}"#)
                .unwrap();
            assert!(crate::proxy::has_oauth_token(server, URL));

            revoke_mcp_oauth_token(server.to_string()).unwrap();

            assert!(crate::proxy::load_oauth_token(server, URL).is_err());
            assert!(crate::proxy::load_oauth_credentials(server, URL).is_err());
            assert!(!crate::proxy::has_oauth_token(server, URL));
        });
    }

    #[test]
    fn revoke_succeeds_without_refresh_credentials() {
        let server = "revoke-test-token-only-fixture";
        with_saved_server(server, || {
            crate::proxy::store_oauth_token(server, URL, "access-token").unwrap();

            revoke_mcp_oauth_token(server.to_string()).unwrap();

            assert!(crate::proxy::load_oauth_token(server, URL).is_err());
            assert!(!crate::proxy::has_oauth_token(server, URL));
        });
    }

    #[test]
    fn revoke_without_token_still_removes_refresh_credentials() {
        let server = "revoke-test-creds-only-fixture";
        with_saved_server(server, || {
            crate::proxy::store_oauth_credentials(server, URL, r#"{"refresh_token":"refresh"}"#)
                .unwrap();

            assert!(revoke_mcp_oauth_token(server.to_string()).is_err());

            assert!(crate::proxy::load_oauth_credentials(server, URL).is_err());
        });
    }

    #[test]
    fn revoke_fails_while_a_token_from_an_older_version_is_still_stored() {
        use crate::core::secret_store::{self, set_test_fault, TestFault};

        let server = "revoke-test-legacy-fixture";
        with_saved_server(server, || {
            crate::proxy::store_oauth_token(server, URL, "access-token").unwrap();
            let legacy_token = format!("mcp_oauth_token_{}", server);
            secret_store::store_secret(&legacy_token, "old-token").unwrap();
            set_test_fault(&legacy_token, Some(TestFault::DeleteRefused));

            let err = revoke_mcp_oauth_token(server.to_string()).expect_err("still stored");

            assert!(err.contains("old OAuth token"), "unexpected error: {err}");
            // The token for the saved URL is left, so Revoke is still offered.
            assert!(crate::proxy::load_oauth_token(server, URL).is_ok());
        });
    }

    #[test]
    fn revoke_fails_for_a_server_with_no_saved_url() {
        let home = tempfile::tempdir().expect("tempdir");
        crate::core::with_test_home(home.path().to_path_buf(), || {
            let err = revoke_mcp_oauth_token("revoke-test-unsaved-fixture".to_string())
                .expect_err("nothing to revoke");
            assert!(err.contains("no saved URL"), "unexpected error: {err}");
        });
    }
}
