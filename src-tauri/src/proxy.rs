//! MCP Proxy — transparent stdio-to-HTTP bridge with keychain-backed auth.
//!
//! Launched via `automatic mcp-proxy <server-name>`.
//! Reads the server URL from `~/.automatic/mcp_servers/<name>.json`,
//! loads the OAuth bearer token from the system keychain, and relays
//! JSON-RPC messages between stdin/stdout and the remote HTTP server.
//! The token never touches any file on disk.

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::sync::{Arc, Mutex, OnceLock};

// ── Constants ────────────────────────────────────────────────────────────────

const SESSION_HEADER: &str = "mcp-session-id";
const MCP_PROTOCOL_VERSION_HEADER: &str = "mcp-protocol-version";
const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

// ── Keychain helpers ─────────────────────────────────────────────────────────
//
// All entries go through `core::secret_store`, which keeps them under one
// keychain service name (debug: "automatic_desktop_dev", release:
// "automatic_desktop"). This avoids a macOS keyring issue where dynamic
// service names pass
// `set_password` but fail `get_password`. Entries are differentiated by the
// *user* field instead.

use crate::core::secret_store;

/// User field for an OAuth bearer token entry.
fn oauth_token_user(server_name: &str) -> String {
    format!("mcp_oauth_token_{}", server_name)
}

/// User field for the full OAuth credentials blob.
fn oauth_creds_user(server_name: &str) -> String {
    format!("mcp_oauth_creds_{}", server_name)
}

/// Remembers, per server, whether an OAuth token is present in the keychain.
///
/// Presence can only be established by reading the token, and sync walks every
/// server of every project on each run. On macOS each read is a separate access
/// check that can raise its own password dialog, so the answer is kept for the
/// life of the process and invalidated whenever this process stores or deletes
/// a token.
fn token_presence_cache() -> &'static Mutex<HashMap<String, bool>> {
    static CACHE: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn set_cached_token_presence(server_name: &str, present: bool) {
    if let Ok(mut cache) = token_presence_cache().lock() {
        cache.insert(server_name.to_string(), present);
    }
}

/// Store an OAuth bearer token in the system keychain.
pub fn store_oauth_token(server_name: &str, token: &str) -> Result<(), String> {
    secret_store::store_secret(&oauth_token_user(server_name), token)?;
    set_cached_token_presence(server_name, true);
    Ok(())
}

/// Load an OAuth bearer token from the system keychain.
pub fn load_oauth_token(server_name: &str) -> Result<String, String> {
    secret_store::load_secret(&oauth_token_user(server_name))
}

/// Delete an OAuth bearer token from the system keychain.
pub fn delete_oauth_token(server_name: &str) -> Result<(), String> {
    secret_store::delete_secret(&oauth_token_user(server_name))?;
    set_cached_token_presence(server_name, false);
    Ok(())
}

/// Check whether an OAuth token exists for a server.
///
/// The result is cached for the life of the process. A token added or removed
/// by another process is therefore not observed until restart.
pub fn has_oauth_token(server_name: &str) -> bool {
    if let Ok(cache) = token_presence_cache().lock() {
        if let Some(present) = cache.get(server_name) {
            return *present;
        }
    }

    let present = load_oauth_token(server_name).is_ok();
    set_cached_token_presence(server_name, present);
    present
}

/// Store OAuth credentials (client_id + token JSON) for refresh support.
pub fn store_oauth_credentials(server_name: &str, credentials_json: &str) -> Result<(), String> {
    secret_store::store_secret(&oauth_creds_user(server_name), credentials_json)
}

/// Load stored OAuth credentials JSON.
pub fn load_oauth_credentials(server_name: &str) -> Result<String, String> {
    secret_store::load_secret(&oauth_creds_user(server_name))
}

/// Delete the stored OAuth credentials, which hold the refresh token.
///
/// A missing entry is success. Authorising stores the credentials after the
/// token and carries on when that store fails, so a server can have a token
/// and no credentials. Revoking it must still work.
///
/// The call succeeds only when the store confirms that nothing is left. A
/// read that fails is an error, because the credentials may still be there.
/// A revoke is the user asking for the secrets to be gone, so it must not
/// report success on a guess.
pub fn delete_oauth_credentials(server_name: &str) -> Result<(), String> {
    match remove_secret(&oauth_creds_user(server_name), "OAuth credentials", server_name)? {
        Removal::Gone => Ok(()),
        Removal::Unconfirmed { delete_err, read_err } => Err(format!(
            "could not confirm the deletion of OAuth credentials for '{}': {} (delete failed with: {})",
            server_name, read_err, delete_err
        )),
    }
}

/// What became of a secret that was to be deleted.
enum Removal {
    /// Nothing is stored under the account now.
    Gone,
    /// The delete failed, and the store could not be read to see whether the
    /// secret is there.
    Unconfirmed { delete_err: String, read_err: String },
}

/// Delete the secret under `account`. `what` names it in an error message.
///
/// `delete_secret` reports a missing entry and a failed delete the same way,
/// so a failed delete is checked by reading the entry back. The delete is
/// tried first, so a secret that exists is removed without being read.
///
/// The only error is a store that answers and still holds the secret.
fn remove_secret(account: &str, what: &str, server_name: &str) -> Result<Removal, String> {
    let Err(delete_err) = secret_store::delete_secret(account) else {
        return Ok(Removal::Gone);
    };
    match secret_store::load_secret_if_present(account) {
        Ok(None) => Ok(Removal::Gone),
        Ok(Some(_)) => Err(format!(
            "failed to delete {} for '{}': {}",
            what, server_name, delete_err
        )),
        Err(read_err) => Ok(Removal::Unconfirmed { delete_err, read_err }),
    }
}

/// Delete the secret under `account` for a server whose name is changing
/// hands, and carry on when the store cannot be read.
///
/// A store that cannot be read cannot hand a token to the proxy either. On
/// Linux with no Secret Service every call fails, always. Treating that as an
/// error would stop every delete and rename of a remote server there. The
/// cost is a secret that outlives its server if the store is unreadable now
/// and readable later. A warning is logged, and creating a server with a URL
/// under the name makes the same attempt again.
fn forget_secret(account: &str, what: &str, server_name: &str) -> Result<(), String> {
    if let Removal::Unconfirmed { delete_err, read_err } = remove_secret(account, what, server_name)? {
        eprintln!(
            "warning: could not check whether {} for MCP server '{}' is still stored: {} (delete failed with: {})",
            what, server_name, read_err, delete_err
        );
    }
    Ok(())
}

/// Delete both OAuth secrets stored for a server: the refresh credentials and
/// the bearer token.
///
/// Call this whenever a server name stops referring to the server the secrets
/// were issued for. Both are keyed by name alone, so a server later created
/// under the same name would otherwise send the old token to its own URL.
///
/// A missing secret is success. So is a store that cannot be read, as
/// [`forget_secret`] explains. The error is a store that answers and still
/// holds a secret. The credentials go first, as in a revoke: a failure then
/// leaves the token in place, and the server still shows as signed in so the
/// user can try again. Unlike [`delete_oauth_token`], this does not fail when
/// no token is stored.
pub fn forget_oauth_secrets(server_name: &str) -> Result<(), String> {
    forget_secret(&oauth_creds_user(server_name), "the OAuth credentials", server_name)?;
    forget_secret(&oauth_token_user(server_name), "the OAuth token", server_name)?;
    set_cached_token_presence(server_name, false);
    Ok(())
}

/// Store the given OAuth secrets under `server_name`, and delete whichever of
/// the two is `None`.
///
/// Deleting the absent half matters. A leftover under this name would pair one
/// account's token with another account's refresh token.
fn replace_oauth_secrets(
    server_name: &str,
    token: Option<&str>,
    credentials: Option<&str>,
) -> Result<(), String> {
    match credentials {
        Some(credentials) => store_oauth_credentials(server_name, credentials)?,
        None => forget_secret(&oauth_creds_user(server_name), "the OAuth credentials", server_name)?,
    }
    match token {
        Some(token) => store_oauth_token(server_name, token),
        None => {
            forget_secret(&oauth_token_user(server_name), "the OAuth token", server_name)?;
            set_cached_token_presence(server_name, false);
            Ok(())
        }
    }
}

/// Read one of the secrets a rename has to move. `None` means the store could
/// not be read, and a warning has been logged.
fn read_secret_to_move(account: &str, old_name: &str, new_name: &str) -> Option<Option<String>> {
    match secret_store::load_secret_if_present(account) {
        Ok(secret) => Some(secret),
        Err(e) => {
            eprintln!(
                "warning: could not read the OAuth secrets of MCP server '{}', so none were moved to '{}': {}",
                old_name, new_name, e
            );
            None
        }
    }
}

/// Move both OAuth secrets from `old_name` to `new_name`, for a server that is
/// being renamed.
///
/// Anything already stored under `new_name` is replaced or deleted, because
/// the caller has checked that no server has that name. A server with no
/// secrets is success, and leaves none under either name.
///
/// A store that cannot be read is success too, and nothing is moved. The
/// reasons are the ones [`forget_secret`] gives. The renamed server is then
/// signed out.
///
/// On an error the secrets are still under `old_name`, and none are left
/// under `new_name`.
pub fn move_oauth_secrets(old_name: &str, new_name: &str) -> Result<(), String> {
    let Some(token) = read_secret_to_move(&oauth_token_user(old_name), old_name, new_name) else {
        return Ok(());
    };
    let Some(credentials) = read_secret_to_move(&oauth_creds_user(old_name), old_name, new_name)
    else {
        return Ok(());
    };

    if let Err(store_err) =
        replace_oauth_secrets(new_name, token.as_deref(), credentials.as_deref())
    {
        // One of the two may have been stored. Half a pair is not worth
        // keeping: it could sit next to the other half of another account.
        let cleanup = match forget_oauth_secrets(new_name) {
            Ok(()) => String::new(),
            Err(e) => format!(" (a copy may be left under '{}': {})", new_name, e),
        };
        return Err(format!(
            "could not store the OAuth secrets of '{}' under '{}': {}{}",
            old_name, new_name, store_err, cleanup
        ));
    }

    if token.is_none() && credentials.is_none() {
        set_cached_token_presence(old_name, false);
        return Ok(());
    }

    let Err(forget_err) = forget_oauth_secrets(old_name) else {
        return Ok(());
    };
    // The old name still holds a secret, so the rename must not go ahead.
    // Put back whatever was deleted and remove the copies.
    let restored = replace_oauth_secrets(old_name, token.as_deref(), credentials.as_deref())
        .and_then(|()| forget_oauth_secrets(new_name));
    Err(match restored {
        Ok(()) => forget_err,
        Err(restore_err) => format!(
            "{} (the OAuth secrets could not be put back as they were: {})",
            forget_err, restore_err
        ),
    })
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

// ── Request helper ───────────────────────────────────────────────────────────

/// Send a single JSON-RPC request to the remote MCP server and return the response.
async fn send_request(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    session_id: &Arc<Mutex<Option<String>>>,
    body: &str,
) -> Result<reqwest::Response, Box<dyn std::error::Error>> {
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

    if let Some(ref sid) = *session_id.lock().unwrap() {
        if let Ok(val) = HeaderValue::from_str(sid) {
            headers.insert(HeaderName::from_static(SESSION_HEADER), val);
        }
    }

    Ok(client
        .post(url)
        .headers(headers)
        .body(body.to_string())
        .send()
        .await?)
}

/// Write a successful response (JSON or SSE) to a writer.
fn write_response<W: io::Write>(
    response_body: &str,
    content_type: &str,
    out: &mut W,
) -> Result<(), Box<dyn std::error::Error>> {
    if content_type.contains("text/event-stream") {
        for event_data in parse_sse_events(response_body) {
            if !event_data.trim().is_empty() {
                writeln!(out, "{}", event_data)?;
            }
        }
    } else if !response_body.trim().is_empty() {
        writeln!(out, "{}", response_body.trim())?;
    }
    out.flush()?;
    Ok(())
}

/// Check whether an HTTP status code indicates an expired/revoked token
/// that should trigger a refresh attempt.
fn is_token_expired(status_code: u16) -> bool {
    status_code == 401 || status_code == 403
}

// ── Proxy entry point ────────────────────────────────────────────────────────

/// Run the MCP proxy for `server_name`.
///
/// Reads JSON-RPC from stdin, relays to the remote server over HTTP with the
/// stored bearer token, and writes responses to stdout.  Runs until stdin is
/// closed or the remote connection fails.
///
/// When the remote server returns 401/403 (expired token), the proxy
/// automatically attempts a token refresh and retries the request once.
pub async fn run_proxy(server_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let url = read_server_url(server_name)?;
    let mut token = load_oauth_token(server_name).map_err(|e| {
        format!(
            "No OAuth token found for '{}'. Authenticate first in the Automatic app. ({})",
            server_name, e
        )
    })?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    let session_id: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

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

        let mut response = send_request(&client, &url, &token, &session_id, trimmed).await?;

        // On 401/403, attempt a silent token refresh and retry once.
        if is_token_expired(response.status().as_u16()) {
            if let Ok(new_token) = crate::oauth::refresh_token(server_name).await {
                token = new_token;
                response = send_request(&client, &url, &token, &session_id, trimmed).await?;
            }
        }

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
            let body = response.text().await.unwrap_or_default();
            let error_msg = format!("HTTP {} from remote: {}", status.as_u16(), body);
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

        let body = response.text().await?;
        let mut out = stdout.lock();
        write_response(&body, &content_type, &mut out)?;
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
    fn oauth_token_presence_follows_store_and_delete() {
        let server = "proxy-test-presence-fixture";
        assert!(!has_oauth_token(server));

        store_oauth_token(server, "token").unwrap();
        assert!(has_oauth_token(server));
        assert_eq!(load_oauth_token(server).unwrap(), "token");

        delete_oauth_token(server).unwrap();
        assert!(!has_oauth_token(server));
    }

    // Each test below uses server names no other test uses. The test secret
    // store and the token presence cache are shared by the whole test process.

    #[test]
    fn forget_removes_the_token_and_the_credentials() {
        let server = "forget-test-both-fixture";
        store_oauth_token(server, "token").unwrap();
        store_oauth_credentials(server, r#"{"refresh_token":"refresh"}"#).unwrap();
        assert!(has_oauth_token(server));

        forget_oauth_secrets(server).unwrap();

        assert_eq!(secret_store::load_secret_if_present(&oauth_token_user(server)), Ok(None));
        assert_eq!(secret_store::load_secret_if_present(&oauth_creds_user(server)), Ok(None));
        assert!(!has_oauth_token(server));
    }

    #[test]
    fn forget_succeeds_when_nothing_is_stored() {
        let server = "forget-test-empty-fixture";

        forget_oauth_secrets(server).unwrap();

        assert!(!has_oauth_token(server));
    }

    #[test]
    fn forget_removes_credentials_left_without_a_token() {
        let server = "forget-test-creds-only-fixture";
        store_oauth_credentials(server, r#"{"refresh_token":"refresh"}"#).unwrap();

        forget_oauth_secrets(server).unwrap();

        assert_eq!(secret_store::load_secret_if_present(&oauth_creds_user(server)), Ok(None));
    }

    #[test]
    fn move_carries_both_secrets_to_the_new_name() {
        let (old, new) = ("move-test-both-old-fixture", "move-test-both-new-fixture");
        store_oauth_token(old, "token").unwrap();
        store_oauth_credentials(old, r#"{"refresh_token":"refresh"}"#).unwrap();
        // Prime the cache with the answers a sync would have recorded.
        assert!(has_oauth_token(old));
        assert!(!has_oauth_token(new));

        move_oauth_secrets(old, new).unwrap();

        assert_eq!(load_oauth_token(new).unwrap(), "token");
        assert_eq!(load_oauth_credentials(new).unwrap(), r#"{"refresh_token":"refresh"}"#);
        assert_eq!(secret_store::load_secret_if_present(&oauth_token_user(old)), Ok(None));
        assert_eq!(secret_store::load_secret_if_present(&oauth_creds_user(old)), Ok(None));
        assert!(has_oauth_token(new));
        assert!(!has_oauth_token(old));
    }

    #[test]
    fn move_with_no_secrets_leaves_none_under_either_name() {
        let (old, new) = ("move-test-empty-old-fixture", "move-test-empty-new-fixture");
        // Secrets left under the new name by a server that no longer exists.
        store_oauth_token(new, "stale-token").unwrap();
        store_oauth_credentials(new, r#"{"refresh_token":"stale"}"#).unwrap();
        assert!(has_oauth_token(new));

        move_oauth_secrets(old, new).unwrap();

        assert_eq!(secret_store::load_secret_if_present(&oauth_token_user(new)), Ok(None));
        assert_eq!(secret_store::load_secret_if_present(&oauth_creds_user(new)), Ok(None));
        assert!(!has_oauth_token(new));
        assert!(!has_oauth_token(old));
    }

    #[test]
    fn move_of_a_token_alone_deletes_credentials_left_under_the_new_name() {
        let (old, new) = ("move-test-token-old-fixture", "move-test-token-new-fixture");
        store_oauth_token(old, "token").unwrap();
        store_oauth_credentials(new, r#"{"refresh_token":"stale"}"#).unwrap();

        move_oauth_secrets(old, new).unwrap();

        assert_eq!(load_oauth_token(new).unwrap(), "token");
        assert_eq!(secret_store::load_secret_if_present(&oauth_creds_user(new)), Ok(None));
        assert_eq!(secret_store::load_secret_if_present(&oauth_token_user(old)), Ok(None));
    }

    // ── A secret store that fails ────────────────────────────────────────────

    use crate::core::secret_store::{set_test_fault, TestFault};

    #[test]
    fn a_revoke_does_not_report_success_when_the_store_cannot_be_read() {
        let server = "fault-test-revoke-fixture";
        set_test_fault(&oauth_creds_user(server), Some(TestFault::Unreadable));

        let err = delete_oauth_credentials(server).expect_err("must not guess");

        assert!(err.contains("could not confirm"), "unexpected error: {err}");
    }

    #[test]
    fn forget_succeeds_when_the_store_cannot_be_read() {
        let server = "fault-test-forget-unreadable-fixture";
        set_test_fault(&oauth_creds_user(server), Some(TestFault::Unreadable));
        set_test_fault(&oauth_token_user(server), Some(TestFault::Unreadable));

        forget_oauth_secrets(server).unwrap();

        assert!(!has_oauth_token(server));
    }

    #[test]
    fn forget_fails_when_the_store_still_holds_the_token() {
        let server = "fault-test-forget-refused-fixture";
        store_oauth_token(server, "token").unwrap();
        set_test_fault(&oauth_token_user(server), Some(TestFault::DeleteRefused));

        let err = forget_oauth_secrets(server).expect_err("the token is still stored");

        assert!(err.contains("failed to delete the OAuth token"), "unexpected error: {err}");
        assert_eq!(load_oauth_token(server).unwrap(), "token");
        assert!(has_oauth_token(server));
    }

    #[test]
    fn move_succeeds_and_moves_nothing_when_the_store_cannot_be_read() {
        let (old, new) = ("fault-test-move-unreadable-old-fixture", "fault-test-move-unreadable-new-fixture");
        set_test_fault(&oauth_token_user(old), Some(TestFault::Unreadable));

        move_oauth_secrets(old, new).unwrap();

        assert_eq!(secret_store::load_secret_if_present(&oauth_token_user(new)), Ok(None));
        assert_eq!(secret_store::load_secret_if_present(&oauth_creds_user(new)), Ok(None));
    }

    #[test]
    fn a_move_that_cannot_clear_the_old_name_puts_the_secrets_back() {
        let (old, new) = ("fault-test-move-refused-old-fixture", "fault-test-move-refused-new-fixture");
        store_oauth_token(old, "token").unwrap();
        store_oauth_credentials(old, r#"{"refresh_token":"refresh"}"#).unwrap();
        set_test_fault(&oauth_token_user(old), Some(TestFault::DeleteRefused));

        move_oauth_secrets(old, new).expect_err("the old name still holds the token");

        assert_eq!(load_oauth_token(old).unwrap(), "token");
        assert_eq!(load_oauth_credentials(old).unwrap(), r#"{"refresh_token":"refresh"}"#);
        assert_eq!(secret_store::load_secret_if_present(&oauth_token_user(new)), Ok(None));
        assert_eq!(secret_store::load_secret_if_present(&oauth_creds_user(new)), Ok(None));
        assert!(has_oauth_token(old));
        assert!(!has_oauth_token(new));
    }

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

    #[test]
    fn test_is_token_expired_401() {
        assert!(is_token_expired(401));
    }

    #[test]
    fn test_is_token_expired_403() {
        assert!(is_token_expired(403));
    }

    #[test]
    fn test_is_token_expired_200_not_expired() {
        assert!(!is_token_expired(200));
    }

    #[test]
    fn test_is_token_expired_500_not_expired() {
        assert!(!is_token_expired(500));
    }

    #[test]
    fn test_write_response_json() {
        let mut buf = Vec::new();
        write_response(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}",
            "application/json",
            &mut buf,
        )
        .unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert_eq!(
            output.trim(),
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}"
        );
    }

    #[test]
    fn test_write_response_json_trims_whitespace() {
        let mut buf = Vec::new();
        write_response("  {\"a\":1}  \n", "application/json", &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert_eq!(output.trim(), "{\"a\":1}");
    }

    #[test]
    fn test_write_response_empty_body_writes_nothing() {
        let mut buf = Vec::new();
        write_response("   ", "application/json", &mut buf).unwrap();
        assert!(buf.is_empty());
    }

    #[test]
    fn test_write_response_sse() {
        let mut buf = Vec::new();
        let sse_body = "data: {\"a\":1}\n\ndata: {\"b\":2}\n\n";
        write_response(sse_body, "text/event-stream", &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "{\"a\":1}");
        assert_eq!(lines[1], "{\"b\":2}");
    }

    #[test]
    fn test_write_response_sse_skips_empty_events() {
        let mut buf = Vec::new();
        let sse_body = "data: {\"a\":1}\n\ndata:   \n\ndata: {\"b\":2}\n\n";
        write_response(sse_body, "text/event-stream", &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 2);
    }
}
