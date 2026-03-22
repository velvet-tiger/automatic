//! MCP Proxy — transparent stdio-to-HTTP bridge with keychain-backed auth.
//!
//! Launched via `automatic mcp-proxy <server-name>`.
//! Reads the server URL from `~/.automatic/mcp_servers/<name>.json`,
//! loads the OAuth bearer token from the system keychain, and relays
//! JSON-RPC messages between stdin/stdout and the remote HTTP server.
//! The token never touches any file on disk.

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use serde_json::Value;
use std::io::{self, BufRead, Write};
use std::sync::{Arc, Mutex};

// ── Constants ────────────────────────────────────────────────────────────────

const SESSION_HEADER: &str = "mcp-session-id";
const MCP_PROTOCOL_VERSION_HEADER: &str = "mcp-protocol-version";
const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

// ── Keychain helpers ─────────────────────────────────────────────────────────
//
// All entries use the same service name as the existing API-key storage in
// core::credentials (debug: "automatic_desktop_dev", release: "automatic_desktop").
// This avoids a macOS keyring issue where dynamic service names pass
// `set_password` but fail `get_password`. Entries are differentiated by the
// *user* field instead.

use crate::core::KEYCHAIN_SERVICE;

/// User field for an OAuth bearer token entry.
fn oauth_token_user(server_name: &str) -> String {
    format!("mcp_oauth_token_{}", server_name)
}

/// User field for the full OAuth credentials blob.
fn oauth_creds_user(server_name: &str) -> String {
    format!("mcp_oauth_creds_{}", server_name)
}

/// Store an OAuth bearer token in the system keychain.
pub fn store_oauth_token(server_name: &str, token: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &oauth_token_user(server_name))
        .map_err(|e| e.to_string())?;
    entry.set_password(token).map_err(|e| e.to_string())
}

/// Load an OAuth bearer token from the system keychain.
pub fn load_oauth_token(server_name: &str) -> Result<String, String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &oauth_token_user(server_name))
        .map_err(|e| e.to_string())?;
    entry.get_password().map_err(|e| e.to_string())
}

/// Delete an OAuth bearer token from the system keychain.
pub fn delete_oauth_token(server_name: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &oauth_token_user(server_name))
        .map_err(|e| e.to_string())?;
    entry.delete_credential().map_err(|e| e.to_string())
}

/// Check whether an OAuth token exists for a server.
pub fn has_oauth_token(server_name: &str) -> bool {
    load_oauth_token(server_name).is_ok()
}

/// Store OAuth credentials (client_id + token JSON) for refresh support.
pub fn store_oauth_credentials(server_name: &str, credentials_json: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &oauth_creds_user(server_name))
        .map_err(|e| e.to_string())?;
    entry
        .set_password(credentials_json)
        .map_err(|e| e.to_string())
}

/// Load stored OAuth credentials JSON.
pub fn load_oauth_credentials(server_name: &str) -> Result<String, String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &oauth_creds_user(server_name))
        .map_err(|e| e.to_string())?;
    entry.get_password().map_err(|e| e.to_string())
}

// ── Server config helpers ────────────────────────────────────────────────────

/// Read the URL for a named MCP server from the Automatic registry.
fn read_server_url(server_name: &str) -> Result<String, String> {
    let raw = crate::core::read_mcp_server_config(server_name)?;
    let config: Value =
        serde_json::from_str(&raw).map_err(|e| format!("invalid server config JSON: {}", e))?;
    config
        .get("url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("server config '{}' has no 'url' field", server_name))
}

// ── Proxy entry point ────────────────────────────────────────────────────────

/// Run the MCP proxy for `server_name`.
///
/// Reads JSON-RPC from stdin, relays to the remote server over HTTP with the
/// stored bearer token, and writes responses to stdout.  Runs until stdin is
/// closed or the remote connection fails.
pub async fn run_proxy(server_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let url = read_server_url(server_name)?;
    let token = load_oauth_token(server_name).map_err(|e| {
        format!(
            "No OAuth token found for '{}'. Authenticate first in the Automatic app. ({})",
            server_name, e
        )
    })?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    // Shared session ID — set after the first server response.
    let session_id: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    // Read JSON-RPC messages from stdin (one per line) and relay.
    let stdin = io::stdin();
    let stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Validate it's JSON before sending.
        let _: Value =
            serde_json::from_str(trimmed).map_err(|e| format!("invalid JSON on stdin: {}", e))?;

        // Build request headers.
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/json, text/event-stream"),
        );
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token))
                .map_err(|e| format!("invalid token: {}", e))?,
        );
        headers.insert(
            HeaderName::from_static(MCP_PROTOCOL_VERSION_HEADER),
            HeaderValue::from_static(MCP_PROTOCOL_VERSION),
        );

        // Include session ID if we have one.
        if let Some(ref sid) = *session_id.lock().unwrap() {
            if let Ok(val) = HeaderValue::from_str(sid) {
                headers.insert(HeaderName::from_static(SESSION_HEADER), val);
            }
        }

        let response = client
            .post(&url)
            .headers(headers)
            .body(trimmed.to_string())
            .send()
            .await?;

        // Capture session ID from response headers.
        if let Some(sid) = response.headers().get(SESSION_HEADER) {
            if let Ok(s) = sid.to_str() {
                *session_id.lock().unwrap() = Some(s.to_string());
            }
        }

        let status = response.status();
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        if !status.is_success() {
            // Return a JSON-RPC error for non-2xx responses.
            let body = response.text().await.unwrap_or_default();
            let error_msg = format!("HTTP {} from remote: {}", status.as_u16(), body);
            // Try to extract the request id from the original message for a proper error response.
            let id = serde_json::from_str::<Value>(trimmed)
                .ok()
                .and_then(|v| v.get("id").cloned())
                .unwrap_or(Value::Null);
            let error_response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -(status.as_u16() as i64),
                    "message": error_msg,
                }
            });
            let mut out = stdout.lock();
            writeln!(out, "{}", error_response)?;
            out.flush()?;
            continue;
        }

        if content_type.contains("text/event-stream") {
            // SSE response — read events and emit each data payload as a line.
            let body = response.text().await?;
            for event_data in parse_sse_events(&body) {
                if !event_data.trim().is_empty() {
                    let mut out = stdout.lock();
                    writeln!(out, "{}", event_data)?;
                    out.flush()?;
                }
            }
        } else {
            // Plain JSON response.
            let body = response.text().await?;
            if !body.trim().is_empty() {
                let mut out = stdout.lock();
                writeln!(out, "{}", body.trim())?;
                out.flush()?;
            }
        }
    }

    Ok(())
}

// ── SSE parsing ──────────────────────────────────────────────────────────────

/// Parse a raw SSE text body into individual `data:` payloads.
fn parse_sse_events(body: &str) -> Vec<String> {
    let mut events = Vec::new();
    let mut current_data = String::new();

    for line in body.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            if !current_data.is_empty() {
                current_data.push('\n');
            }
            current_data.push_str(data);
        } else if line.strip_prefix("data:").is_some() {
            // `data:` with no space — the rest of the line is the value
            let data = line.strip_prefix("data:").unwrap();
            if !current_data.is_empty() {
                current_data.push('\n');
            }
            current_data.push_str(data);
        } else if line.is_empty() {
            // Empty line = event boundary.
            if !current_data.is_empty() {
                events.push(current_data.clone());
                current_data.clear();
            }
        }
        // Ignore `event:`, `id:`, `retry:` lines — we only care about data.
    }

    // Flush any trailing data without a terminating blank line.
    if !current_data.is_empty() {
        events.push(current_data);
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sse_events_basic() {
        let body = "data: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n\n";
        let events = parse_sse_events(body);
        assert_eq!(events.len(), 1);
        assert!(events[0].contains("jsonrpc"));
    }

    #[test]
    fn test_parse_sse_events_multiple() {
        let body = "data: {\"a\":1}\n\ndata: {\"b\":2}\n\n";
        let events = parse_sse_events(body);
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_parse_sse_events_multiline_data() {
        let body = "data: line1\ndata: line2\n\n";
        let events = parse_sse_events(body);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], "line1\nline2");
    }

    #[test]
    fn test_parse_sse_events_with_event_and_id() {
        let body = "event: message\nid: 42\ndata: {\"hello\":true}\n\n";
        let events = parse_sse_events(body);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], "{\"hello\":true}");
    }

    #[test]
    fn test_keychain_user_names() {
        assert_eq!(
            oauth_token_user("amplitude-eu"),
            "mcp_oauth_token_amplitude-eu"
        );
        assert_eq!(
            oauth_creds_user("amplitude-eu"),
            "mcp_oauth_creds_amplitude-eu"
        );
    }
}
