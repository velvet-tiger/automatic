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
//
// An OAuth secret is filed under the server's name and the URL it was issued
// for. Every function here takes both, so a secret is only ever found for
// that pair. The proxy reads the saved URL, asks for the token for that URL,
// and sends it there. A token issued for another URL is not found, whatever
// happened to the server's config in between. Deleting and moving entries
// when a server changes is tidying. Nothing depends on it for safety.

use crate::core::secret_store;
use sha2::{Digest, Sha256};

/// The store entries for one server at one URL.
struct OAuthEntries {
    /// The bearer token.
    token: String,
    /// The credentials blob, which holds the refresh token.
    credentials: String,
}

/// The entries for `server_name` at `url`.
///
/// The URL is hashed in the form [`crate::core::comparable_mcp_url`] gives
/// it, so the same URL written two ways has one entry, and the URL does not
/// show in the keychain's list. A server name cannot contain `/`, so these
/// names never collide with the entries of [`legacy_oauth_entries`].
///
/// An error means the URL is empty. A server with no URL has no OAuth secrets.
fn oauth_entries(server_name: &str, url: &str) -> Result<OAuthEntries, String> {
    let url = crate::core::comparable_mcp_url(url).ok_or_else(|| {
        format!(
            "MCP server '{}' has no URL, so it has no OAuth secrets",
            server_name
        )
    })?;
    let url_key = hex::encode(Sha256::digest(url.as_bytes()));
    Ok(OAuthEntries {
        token: format!("mcp_oauth_token_{}/{}", server_name, url_key),
        credentials: format!("mcp_oauth_creds_{}/{}", server_name, url_key),
    })
}

/// The entries older versions used, filed under the server's name alone.
///
/// Nothing is stored under these any more. [`find_oauth_token`] moves what it
/// finds to the entries for the server's URL.
fn legacy_oauth_entries(server_name: &str) -> OAuthEntries {
    OAuthEntries {
        token: format!("mcp_oauth_token_{}", server_name),
        credentials: format!("mcp_oauth_creds_{}", server_name),
    }
}

/// The store entry of a server's token at `url`, for a test that gives the
/// store a fault.
#[cfg(test)]
pub(crate) fn oauth_token_entry(server_name: &str, url: &str) -> String {
    oauth_entries(server_name, url).expect("a url").token
}

/// Test support: leave an HTTP server at `url` in the registry with a token
/// in the store, and return the token's store entry.
///
/// Both are written directly. A save or a sign-in would leave an answer in
/// the presence cache, and a test that gives the store a fault needs the next
/// check to read the store. Call it inside a test home.
#[cfg(test)]
pub(crate) fn leave_signed_in_server(server_name: &str, url: &str) -> String {
    let dir = crate::core::get_mcp_servers_dir().expect("registry dir");
    std::fs::create_dir_all(&dir).expect("create the registry");
    let config = serde_json::json!({ "type": "http", "url": url }).to_string();
    std::fs::write(dir.join(format!("{}.json", server_name)), config).expect("write the server");
    let entry = oauth_token_entry(server_name, url);
    secret_store::store_secret(&entry, "access-token").expect("store the token");
    entry
}

/// Remembers, per token entry, whether an OAuth token is present in the
/// keychain.
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

fn set_cached_token_presence(token_entry: &str, present: bool) {
    if let Ok(mut cache) = token_presence_cache().lock() {
        cache.insert(token_entry.to_string(), present);
    }
}

/// Store the OAuth bearer token issued to a server for `url`.
pub fn store_oauth_token(server_name: &str, url: &str, token: &str) -> Result<(), String> {
    let entries = oauth_entries(server_name, url)?;
    secret_store::store_secret(&entries.token, token)?;
    set_cached_token_presence(&entries.token, true);
    Ok(())
}

/// Load the OAuth bearer token issued to a server for `url`.
///
/// Returns an error when none is stored for that URL, as well as when the
/// store fails.
pub fn load_oauth_token(server_name: &str, url: &str) -> Result<String, String> {
    let entries = oauth_entries(server_name, url)?;
    find_oauth_token(server_name, url, &entries)?.ok_or_else(|| {
        format!(
            "no OAuth token is stored for MCP server '{}' at its URL",
            server_name
        )
    })
}

/// The token in `entries`, or the one an older version stored for the server.
///
/// `url` is the URL `entries` are for. The older token is only looked for
/// when that is the URL it was issued for, as [`adopt_legacy_secrets`]
/// explains.
fn find_oauth_token(
    server_name: &str,
    url: &str,
    entries: &OAuthEntries,
) -> Result<Option<String>, String> {
    if let Some(token) = secret_store::load_secret_if_present(&entries.token)? {
        return Ok(Some(token));
    }
    if !crate::core::legacy_oauth_url_is(server_name, url)? {
        return Ok(None);
    }
    adopt_legacy_secrets(server_name, entries)
}

/// Move the secrets an older version stored under the server's name alone to
/// `entries`, and return the token. `None` when there is no such token.
///
/// The old entries do not say which URL they were issued for. The caller has
/// checked that `entries` are for the URL the server had when this version
/// first ran, which [`crate::core::legacy_oauth_url_is`] records. A server
/// whose URL has changed since, or that was created since, adopts nothing.
///
/// The new entries are written first, so a second process asking at the same
/// moment finds the token in one place or the other. The old token is then
/// deleted, and the adoption is undone unless the store confirms that delete.
/// An old token is so adopted at most once.
///
/// The old credentials move with the token and are never adopted alone.
/// Without that, credentials left behind could later refresh into a token
/// for another URL.
fn adopt_legacy_secrets(
    server_name: &str,
    entries: &OAuthEntries,
) -> Result<Option<String>, String> {
    let legacy = legacy_oauth_entries(server_name);
    let Some(token) = secret_store::load_secret_if_present(&legacy.token)? else {
        return Ok(None);
    };
    let credentials = match secret_store::load_secret_if_present(&legacy.credentials) {
        Ok(credentials) => credentials,
        Err(e) => {
            eprintln!(
                "warning: could not read the stored OAuth credentials of MCP server '{}', so its token will not refresh: {}",
                server_name, e
            );
            None
        }
    };

    if let Some(credentials) = &credentials {
        secret_store::store_secret(&entries.credentials, credentials)?;
    }
    secret_store::store_secret(&entries.token, &token)?;
    if let Some(problem) = remove_secret(&legacy.token).problem() {
        eprintln!(
            "warning: the OAuth token of MCP server '{}' is in an old store entry that could not be deleted, so it was not used: {}",
            server_name, problem
        );
        discard_secret(&entries.credentials, "the OAuth credentials", server_name);
        discard_secret(&entries.token, "the OAuth token", server_name);
        return Ok(None);
    }
    discard_secret(&legacy.credentials, "the old OAuth credentials", server_name);
    Ok(Some(token))
}

/// Delete the OAuth bearer token issued to a server for `url`.
///
/// Returns an error when none is stored, matching the store. A revoke relies
/// on that.
pub fn delete_oauth_token(server_name: &str, url: &str) -> Result<(), String> {
    let entries = oauth_entries(server_name, url)?;
    secret_store::delete_secret(&entries.token)?;
    set_cached_token_presence(&entries.token, false);
    Ok(())
}

/// The OAuth bearer token issued to a server for `url`, or `None` when none
/// is stored for that URL.
///
/// A store that cannot be read is an error, never `None`. A server with no
/// URL has no token.
pub fn load_oauth_token_if_present(server_name: &str, url: &str) -> Result<Option<String>, String> {
    let Ok(entries) = oauth_entries(server_name, url) else {
        return Ok(None);
    };
    find_oauth_token(server_name, url, &entries)
}

/// Check whether an OAuth token is stored for a server at `url`.
///
/// The result is cached for the life of the process. A token added or removed
/// by another process is therefore not observed until restart.
///
/// A store that cannot be read is an error. Sync chooses between the proxy
/// stub and the server's own URL on this answer, so "could not read" must not
/// pass as "no token". The error is not cached, and the next call reads the
/// store again.
pub fn has_oauth_token(server_name: &str, url: &str) -> Result<bool, String> {
    let Ok(entries) = oauth_entries(server_name, url) else {
        return Ok(false);
    };
    if let Ok(cache) = token_presence_cache().lock() {
        if let Some(present) = cache.get(&entries.token) {
            return Ok(*present);
        }
    }

    let present = find_oauth_token(server_name, url, &entries)
        .map_err(|e| {
            format!(
                "could not check for an OAuth token for MCP server '{}': {}",
                server_name, e
            )
        })?
        .is_some();
    set_cached_token_presence(&entries.token, present);
    Ok(present)
}

/// Store OAuth credentials (client_id + token JSON) for refresh support.
pub fn store_oauth_credentials(
    server_name: &str,
    url: &str,
    credentials_json: &str,
) -> Result<(), String> {
    secret_store::store_secret(&oauth_entries(server_name, url)?.credentials, credentials_json)
}

/// Load the stored OAuth credentials JSON for a server at `url`.
pub fn load_oauth_credentials(server_name: &str, url: &str) -> Result<String, String> {
    secret_store::load_secret(&oauth_entries(server_name, url)?.credentials)
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
pub fn delete_oauth_credentials(server_name: &str, url: &str) -> Result<(), String> {
    let entries = oauth_entries(server_name, url)?;
    match remove_secret(&entries.credentials).problem() {
        None => Ok(()),
        Some(problem) => Err(format!(
            "could not delete the OAuth credentials for '{}': {}",
            server_name, problem
        )),
    }
}

/// What became of a secret that was to be deleted.
pub(crate) enum Removal {
    /// Nothing is stored under the entry now.
    Gone,
    /// The store answered, and the secret is still there.
    StillStored { delete_err: String },
    /// The delete failed, and the store could not be read to see whether the
    /// secret is there.
    Unconfirmed { delete_err: String, read_err: String },
}

impl Removal {
    /// Why the secret may still be stored, or `None` when it is gone.
    pub(crate) fn problem(&self) -> Option<String> {
        match self {
            Removal::Gone => None,
            Removal::StillStored { delete_err } => {
                Some(format!("the store still holds it ({})", delete_err))
            }
            Removal::Unconfirmed { delete_err, read_err } => Some(format!(
                "the store could not be read to check ({}; delete failed with: {})",
                read_err, delete_err
            )),
        }
    }
}

/// Delete the secret under `entry`.
///
/// `delete_secret` reports a missing entry and a failed delete the same way,
/// so a failed delete is checked by reading the entry back. The delete is
/// tried first, so a secret that exists is removed without being read.
pub(crate) fn remove_secret(entry: &str) -> Removal {
    let Err(delete_err) = secret_store::delete_secret(entry) else {
        return Removal::Gone;
    };
    match secret_store::load_secret_if_present(entry) {
        Ok(None) => Removal::Gone,
        Ok(Some(_)) => Removal::StillStored { delete_err },
        Err(read_err) => Removal::Unconfirmed { delete_err, read_err },
    }
}

/// Delete a secret that nothing should use any more. A failure is logged and
/// not returned.
///
/// This is tidying. A secret left behind is still filed under its server name
/// and URL, so it can only ever be found for that pair. Failing the caller
/// would stop a delete, rename or save wherever the store cannot be reached,
/// which on Linux with no Secret Service is always.
fn discard_secret(entry: &str, what: &str, server_name: &str) {
    if let Some(problem) = remove_secret(entry).problem() {
        eprintln!(
            "warning: could not delete {} of MCP server '{}': {}",
            what, server_name, problem
        );
    }
}

/// Delete the OAuth secrets a server holds for `url`, and any that an older
/// version stored under its name alone. Failures are logged, as
/// [`discard_secret`] explains.
///
/// Call this when a server is deleted or stops using `url`. The old entries
/// go too. They were issued for a URL the server no longer has, so nothing
/// will use them.
pub fn forget_oauth_secrets(server_name: &str, url: &str) {
    if let Ok(entries) = oauth_entries(server_name, url) {
        discard_secret(&entries.credentials, "the OAuth credentials", server_name);
        discard_secret(&entries.token, "the OAuth token", server_name);
        set_cached_token_presence(&entries.token, false);
    }
    forget_legacy_oauth_secrets(server_name);
}

/// Delete the OAuth secrets an older version stored under a server's name
/// alone. Failures are logged.
pub fn forget_legacy_oauth_secrets(server_name: &str) {
    let legacy = legacy_oauth_entries(server_name);
    discard_secret(&legacy.credentials, "the old OAuth credentials", server_name);
    discard_secret(&legacy.token, "the old OAuth token", server_name);
}

/// Delete the OAuth secrets an older version stored under a server's name
/// alone, and fail unless the store confirms they are gone.
///
/// A revoke uses this. An old token left behind would be adopted again the
/// next time the token is asked for, and the server would be signed back in.
pub fn delete_legacy_oauth_secrets(server_name: &str) -> Result<(), String> {
    let legacy = legacy_oauth_entries(server_name);
    for (entry, what) in [(&legacy.credentials, "credentials"), (&legacy.token, "token")] {
        if let Some(problem) = remove_secret(entry).problem() {
            return Err(format!(
                "could not delete the old OAuth {} for '{}': {}",
                what, server_name, problem
            ));
        }
    }
    Ok(())
}

/// Move the OAuth secrets a server holds for `url` from `old_name` to
/// `new_name`, for a server that is being renamed.
///
/// Anything already stored for `new_name` at that URL is replaced or deleted,
/// so the renamed server never pairs its token with credentials left by
/// another. A failure is logged and the renamed server is signed out. The
/// rename itself does not depend on this.
pub fn move_oauth_secrets(old_name: &str, new_name: &str, url: &str) {
    let (Ok(old), Ok(new)) = (oauth_entries(old_name, url), oauth_entries(new_name, url)) else {
        return;
    };
    forget_legacy_oauth_secrets(new_name);

    let secrets = find_oauth_token(old_name, url, &old).and_then(|token| {
        let credentials = secret_store::load_secret_if_present(&old.credentials)?;
        Ok((token, credentials))
    });
    let (token, credentials) = match secrets {
        Ok(secrets) => secrets,
        Err(e) => {
            eprintln!(
                "warning: could not read the OAuth secrets of MCP server '{}', so '{}' is signed out: {}",
                old_name, new_name, e
            );
            discard_secret(&new.credentials, "a leftover OAuth secret", new_name);
            discard_secret(&new.token, "a leftover OAuth secret", new_name);
            set_cached_token_presence(&new.token, false);
            return;
        }
    };

    let stored = [(&new.credentials, &credentials), (&new.token, &token)]
        .into_iter()
        .try_for_each(|(entry, secret)| match secret {
            Some(secret) => secret_store::store_secret(entry, secret),
            None => {
                discard_secret(entry, "a leftover OAuth secret", new_name);
                Ok(())
            }
        });
    if let Err(e) = stored {
        eprintln!(
            "warning: could not store the OAuth secrets of MCP server '{}' under '{}', so it is signed out: {}",
            old_name, new_name, e
        );
        // Half a pair is not worth keeping.
        discard_secret(&new.credentials, "the OAuth credentials", new_name);
        discard_secret(&new.token, "the OAuth token", new_name);
        set_cached_token_presence(&new.token, false);
        return;
    }
    set_cached_token_presence(&new.token, token.is_some());

    forget_oauth_secrets(old_name, url);
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
    let mut token = load_oauth_token(server_name, &url).map_err(|e| {
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
            if let Ok(new_token) = crate::oauth::refresh_token(server_name, &url).await {
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

    // Each test below uses server names no other test uses. The test secret
    // store and the token presence cache are shared by the whole test process.

    use crate::core::secret_store::{set_test_fault, TestFault};
    use crate::core::{save_mcp_server_config, with_test_home};

    const URL: &str = "https://mcp.example.com/mcp";
    const OTHER_URL: &str = "https://other.example/mcp";
    const CREDENTIALS: &str = r#"{"refresh_token":"refresh"}"#;

    fn entries(server: &str, url: &str) -> OAuthEntries {
        oauth_entries(server, url).expect("a url")
    }

    fn stored(entry: &str) -> Option<String> {
        secret_store::load_secret_if_present(entry).expect("read the test store")
    }

    /// Save an HTTP server at `url` in a home of its own, then run `test`.
    fn with_saved_server(server: &str, url: &str, test: impl FnOnce()) {
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let config = serde_json::json!({ "type": "http", "url": url }).to_string();
            save_mcp_server_config(server, &config).expect("save");
            test();
        });
    }

    /// Leave an HTTP server at `url` in a home of its own as an older version
    /// would have, then run `test`. The file is written directly. A save
    /// through this version would first record that the server did not
    /// exist before.
    fn with_server_from_an_older_version(server: &str, url: &str, test: impl FnOnce()) {
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = crate::core::get_mcp_servers_dir().expect("dir");
            std::fs::create_dir_all(&dir).expect("create the registry");
            let config = serde_json::json!({ "type": "http", "url": url }).to_string();
            std::fs::write(dir.join(format!("{}.json", server)), config).expect("write");
            test();
        });
    }

    #[test]
    fn oauth_token_presence_follows_store_and_delete() {
        let server = "proxy-test-presence-fixture";
        assert_eq!(has_oauth_token(server, URL), Ok(false));

        store_oauth_token(server, URL, "token").unwrap();
        assert_eq!(has_oauth_token(server, URL), Ok(true));
        assert_eq!(load_oauth_token(server, URL).unwrap(), "token");

        delete_oauth_token(server, URL).unwrap();
        assert_eq!(has_oauth_token(server, URL), Ok(false));
    }

    #[test]
    fn a_token_that_cannot_be_read_is_an_error_and_is_not_remembered() {
        let server = "presence-unreadable-fixture";
        // Stored straight into the store, so the presence cache has no answer
        // and the check has to read.
        let entry = oauth_token_entry(server, URL);
        secret_store::store_secret(&entry, "token").unwrap();

        set_test_fault(&entry, Some(TestFault::Unreadable));
        let present = has_oauth_token(server, URL);
        let token = load_oauth_token_if_present(server, URL);
        set_test_fault(&entry, None);

        let err = present.expect_err("the store cannot be read");
        assert!(err.contains(server), "the error names the server: {err}");
        assert!(token.is_err(), "a failed read is not a missing token");

        // A cached "no token" would last for the life of the process.
        assert_eq!(has_oauth_token(server, URL), Ok(true));
        assert_eq!(load_oauth_token_if_present(server, URL), Ok(Some("token".to_string())));
    }

    #[test]
    fn a_token_that_is_not_stored_loads_as_none() {
        let server = "presence-absent-fixture";
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            assert_eq!(load_oauth_token_if_present(server, URL), Ok(None));
            assert_eq!(load_oauth_token_if_present(server, ""), Ok(None));
        });
    }

    // ── A secret is found only for its server name and URL ───────────────────

    #[test]
    fn a_token_is_found_only_for_the_url_it_was_stored_for() {
        let server = "filing-test-url-fixture";
        store_oauth_token(server, URL, "token").unwrap();
        store_oauth_credentials(server, URL, CREDENTIALS).unwrap();

        assert_eq!(has_oauth_token(server, URL), Ok(true));
        assert_eq!(has_oauth_token(server, OTHER_URL), Ok(false));
        assert!(load_oauth_token(server, OTHER_URL).is_err());
        assert!(load_oauth_credentials(server, OTHER_URL).is_err());
        // Another path on the same host is another URL.
        assert_eq!(has_oauth_token(server, "https://mcp.example.com/sse"), Ok(false));
    }

    #[test]
    fn a_token_is_found_only_under_the_name_it_was_stored_for() {
        store_oauth_token("filing-test-name-fixture", URL, "token").unwrap();

        assert_eq!(has_oauth_token("filing-test-other-name-fixture", URL), Ok(false));
        assert!(load_oauth_token("filing-test-other-name-fixture", URL).is_err());
    }

    #[test]
    fn a_url_written_two_ways_has_one_entry() {
        let server = "filing-test-forms-fixture";
        store_oauth_token(server, URL, "token").unwrap();

        assert_eq!(
            load_oauth_token(server, " HTTPS://MCP.Example.com:443/mcp ").unwrap(),
            "token"
        );
    }

    #[test]
    fn a_server_with_no_url_has_no_secrets() {
        let server = "filing-test-no-url-fixture";

        assert!(store_oauth_token(server, "", "token").is_err());
        assert!(store_oauth_credentials(server, " ", CREDENTIALS).is_err());
        assert_eq!(has_oauth_token(server, ""), Ok(false));
        assert!(load_oauth_token(server, "").is_err());
    }

    #[test]
    fn an_entry_is_named_for_the_server_and_a_hash_of_the_url() {
        let entries = entries("amplitude-eu", URL);

        let hash = entries
            .token
            .strip_prefix("mcp_oauth_token_amplitude-eu/")
            .expect("token entry prefix");
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(entries.credentials, format!("mcp_oauth_creds_amplitude-eu/{}", hash));
        assert!(!entries.token.contains("mcp.example.com"));

        let legacy = legacy_oauth_entries("amplitude-eu");
        assert_eq!(legacy.token, "mcp_oauth_token_amplitude-eu");
        assert_eq!(legacy.credentials, "mcp_oauth_creds_amplitude-eu");
    }

    // ── Tidying ──────────────────────────────────────────────────────────────

    #[test]
    fn forget_removes_the_secrets_for_one_url_and_leaves_the_others() {
        let server = "forget-test-both-fixture";
        store_oauth_token(server, URL, "token").unwrap();
        store_oauth_credentials(server, URL, CREDENTIALS).unwrap();
        store_oauth_token(server, OTHER_URL, "other-token").unwrap();
        assert_eq!(has_oauth_token(server, URL), Ok(true));

        forget_oauth_secrets(server, URL);

        assert_eq!(stored(&entries(server, URL).token), None);
        assert_eq!(stored(&entries(server, URL).credentials), None);
        assert_eq!(has_oauth_token(server, URL), Ok(false));
        assert_eq!(load_oauth_token(server, OTHER_URL).unwrap(), "other-token");
    }

    #[test]
    fn forget_removes_secrets_an_older_version_stored() {
        let server = "forget-test-legacy-fixture";
        let legacy = legacy_oauth_entries(server);
        secret_store::store_secret(&legacy.token, "old-token").unwrap();
        secret_store::store_secret(&legacy.credentials, CREDENTIALS).unwrap();

        forget_oauth_secrets(server, URL);

        assert_eq!(stored(&legacy.token), None);
        assert_eq!(stored(&legacy.credentials), None);
    }

    #[test]
    fn forget_does_nothing_when_nothing_is_stored() {
        let server = "forget-test-empty-fixture";

        forget_oauth_secrets(server, URL);
        forget_legacy_oauth_secrets(server);

        assert_eq!(has_oauth_token(server, URL), Ok(false));
    }

    #[test]
    fn forget_carries_on_when_the_store_refuses_and_the_token_stays_with_its_url() {
        let server = "fault-test-forget-refused-fixture";
        store_oauth_token(server, URL, "token").unwrap();
        set_test_fault(&entries(server, URL).token, Some(TestFault::DeleteRefused));

        forget_oauth_secrets(server, URL);

        assert_eq!(stored(&entries(server, URL).token), Some("token".to_string()));
        assert!(load_oauth_token(server, OTHER_URL).is_err());
    }

    #[test]
    fn forget_carries_on_when_the_store_cannot_be_read() {
        let server = "fault-test-forget-unreadable-fixture";
        set_test_fault(&entries(server, URL).token, Some(TestFault::Unreadable));
        set_test_fault(&entries(server, URL).credentials, Some(TestFault::Unreadable));

        forget_oauth_secrets(server, URL);

        assert_eq!(has_oauth_token(server, URL), Ok(false));
    }

    #[test]
    fn a_revoke_does_not_report_success_when_the_store_cannot_be_read() {
        let server = "fault-test-revoke-fixture";
        set_test_fault(&entries(server, URL).credentials, Some(TestFault::Unreadable));

        let err = delete_oauth_credentials(server, URL).expect_err("must not guess");

        assert!(err.contains("could not be read"), "unexpected error: {err}");
    }

    #[test]
    fn a_revoke_fails_when_the_store_still_holds_the_credentials() {
        let server = "fault-test-revoke-refused-fixture";
        store_oauth_credentials(server, URL, CREDENTIALS).unwrap();
        set_test_fault(&entries(server, URL).credentials, Some(TestFault::DeleteRefused));

        let err = delete_oauth_credentials(server, URL).expect_err("still stored");

        assert!(err.contains("still holds"), "unexpected error: {err}");
    }

    // ── Moving on a rename ───────────────────────────────────────────────────

    #[test]
    fn move_carries_both_secrets_to_the_new_name() {
        let (old, new) = ("move-test-both-old-fixture", "move-test-both-new-fixture");
        store_oauth_token(old, URL, "token").unwrap();
        store_oauth_credentials(old, URL, CREDENTIALS).unwrap();
        // Prime the cache with the answers a sync would have recorded.
        assert_eq!(has_oauth_token(old, URL), Ok(true));
        assert_eq!(has_oauth_token(new, URL), Ok(false));

        move_oauth_secrets(old, new, URL);

        assert_eq!(load_oauth_token(new, URL).unwrap(), "token");
        assert_eq!(load_oauth_credentials(new, URL).unwrap(), CREDENTIALS);
        assert_eq!(stored(&entries(old, URL).token), None);
        assert_eq!(stored(&entries(old, URL).credentials), None);
        assert_eq!(has_oauth_token(new, URL), Ok(true));
        assert_eq!(has_oauth_token(old, URL), Ok(false));
    }

    #[test]
    fn move_with_no_secrets_leaves_none_under_either_name() {
        let (old, new) = ("move-test-empty-old-fixture", "move-test-empty-new-fixture");
        // Secrets left under the new name by a server that no longer exists.
        store_oauth_token(new, URL, "stale-token").unwrap();
        store_oauth_credentials(new, URL, r#"{"refresh_token":"stale"}"#).unwrap();
        secret_store::store_secret(&legacy_oauth_entries(new).token, "older-token").unwrap();
        assert_eq!(has_oauth_token(new, URL), Ok(true));

        move_oauth_secrets(old, new, URL);

        assert_eq!(stored(&entries(new, URL).token), None);
        assert_eq!(stored(&entries(new, URL).credentials), None);
        assert_eq!(stored(&legacy_oauth_entries(new).token), None);
        assert_eq!(has_oauth_token(new, URL), Ok(false));
        assert_eq!(has_oauth_token(old, URL), Ok(false));
    }

    #[test]
    fn move_of_a_token_alone_deletes_credentials_left_under_the_new_name() {
        let (old, new) = ("move-test-token-old-fixture", "move-test-token-new-fixture");
        store_oauth_token(old, URL, "token").unwrap();
        store_oauth_credentials(new, URL, r#"{"refresh_token":"stale"}"#).unwrap();

        move_oauth_secrets(old, new, URL);

        assert_eq!(load_oauth_token(new, URL).unwrap(), "token");
        assert_eq!(stored(&entries(new, URL).credentials), None);
        assert_eq!(stored(&entries(old, URL).token), None);
    }

    #[test]
    fn a_move_that_cannot_read_the_old_secrets_leaves_the_new_name_signed_out() {
        let (old, new) = ("fault-test-move-unreadable-old-fixture", "fault-test-move-unreadable-new-fixture");
        // Left under the new name by a server that no longer exists.
        store_oauth_token(new, URL, "stale-token").unwrap();
        store_oauth_credentials(new, URL, r#"{"refresh_token":"stale"}"#).unwrap();
        set_test_fault(&entries(old, URL).token, Some(TestFault::Unreadable));

        move_oauth_secrets(old, new, URL);

        assert_eq!(stored(&entries(new, URL).token), None);
        assert_eq!(stored(&entries(new, URL).credentials), None);
        assert_eq!(has_oauth_token(new, URL), Ok(false));
    }

    #[test]
    fn a_move_that_cannot_store_both_secrets_leaves_none_under_the_new_name() {
        let (old, new) = ("fault-test-move-store-old-fixture", "fault-test-move-store-new-fixture");
        store_oauth_token(old, URL, "token").unwrap();
        store_oauth_credentials(old, URL, CREDENTIALS).unwrap();
        // The credentials are stored first, then the token store fails.
        set_test_fault(&entries(new, URL).token, Some(TestFault::Unreadable));

        move_oauth_secrets(old, new, URL);

        assert_eq!(stored(&entries(new, URL).credentials), None);
        assert_eq!(load_oauth_token(old, URL).unwrap(), "token");
        assert_eq!(load_oauth_credentials(old, URL).unwrap(), CREDENTIALS);
    }

    // ── Secrets an older version stored under the name alone ─────────────────

    fn seed_legacy_secrets(server: &str) -> OAuthEntries {
        let legacy = legacy_oauth_entries(server);
        secret_store::store_secret(&legacy.token, "old-token").unwrap();
        secret_store::store_secret(&legacy.credentials, CREDENTIALS).unwrap();
        legacy
    }

    #[test]
    fn an_older_token_is_adopted_for_the_url_the_server_had() {
        let server = "legacy-test-adopt-fixture";
        with_server_from_an_older_version(server, URL, || {
            let legacy = seed_legacy_secrets(server);

            assert_eq!(has_oauth_token(server, URL), Ok(true));

            assert_eq!(load_oauth_token(server, URL).unwrap(), "old-token");
            assert_eq!(stored(&entries(server, URL).token), Some("old-token".to_string()));
            assert_eq!(load_oauth_credentials(server, URL).unwrap(), CREDENTIALS);
            assert_eq!(stored(&legacy.token), None);
            assert_eq!(stored(&legacy.credentials), None);
        });
    }

    #[test]
    fn an_older_token_is_not_adopted_for_another_url() {
        let server = "legacy-test-other-url-fixture";
        with_server_from_an_older_version(server, URL, || {
            let legacy = seed_legacy_secrets(server);

            assert_eq!(has_oauth_token(server, OTHER_URL), Ok(false));
            assert!(load_oauth_token(server, OTHER_URL).is_err());

            assert_eq!(stored(&legacy.token), Some("old-token".to_string()));
            assert_eq!(stored(&entries(server, OTHER_URL).token), None);
        });
    }

    #[test]
    fn an_older_token_does_not_follow_its_server_to_a_new_url() {
        let server = "legacy-test-url-change-fixture";
        with_server_from_an_older_version(server, URL, || {
            let legacy = seed_legacy_secrets(server);
            // The save cannot delete the old token, so it is still there.
            set_test_fault(&legacy.token, Some(TestFault::DeleteRefused));

            let config = serde_json::json!({ "type": "http", "url": OTHER_URL }).to_string();
            save_mcp_server_config(server, &config).expect("save the new url");

            assert_eq!(stored(&legacy.token), Some("old-token".to_string()));
            assert!(load_oauth_token(server, OTHER_URL).is_err());
            assert_eq!(stored(&entries(server, OTHER_URL).token), None);
        });
    }

    #[test]
    fn an_older_token_does_not_follow_a_save_under_another_case_of_the_name() {
        let server = "legacy-test-case-fixture";
        with_server_from_an_older_version(server, URL, || {
            let legacy = seed_legacy_secrets(server);
            // Where the filesystem ignores case, this writes the server's own
            // file, and tidies under a name its old token is not filed as.
            let config = serde_json::json!({ "type": "http", "url": OTHER_URL }).to_string();
            save_mcp_server_config(&server.to_uppercase(), &config).expect("save");

            assert!(load_oauth_token(server, OTHER_URL).is_err());
            assert!(load_oauth_token(&server.to_uppercase(), OTHER_URL).is_err());
            assert_eq!(stored(&legacy.token), Some("old-token".to_string()));
        });
    }

    #[test]
    fn a_server_created_by_this_version_does_not_adopt_an_older_token() {
        let server = "legacy-test-new-server-fixture";
        with_saved_server(server, URL, || {
            // Left under the name by a server deleted long ago.
            let legacy = legacy_oauth_entries(server);
            secret_store::store_secret(&legacy.token, "old-token").unwrap();

            assert!(load_oauth_token(server, URL).is_err());

            assert_eq!(stored(&legacy.token), Some("old-token".to_string()));
        });
    }

    #[test]
    fn an_older_token_is_not_adopted_for_a_server_that_is_not_stored() {
        let server = "legacy-test-unsaved-fixture";
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let legacy = seed_legacy_secrets(server);

            assert!(load_oauth_token(server, URL).is_err());

            assert_eq!(stored(&legacy.token), Some("old-token".to_string()));
        });
    }

    #[test]
    fn an_older_token_that_cannot_be_deleted_is_not_adopted() {
        let server = "legacy-test-refused-fixture";
        with_server_from_an_older_version(server, URL, || {
            let legacy = seed_legacy_secrets(server);
            set_test_fault(&legacy.token, Some(TestFault::DeleteRefused));

            assert_eq!(has_oauth_token(server, URL), Ok(false));
            assert!(load_oauth_token(server, URL).is_err());

            assert_eq!(stored(&entries(server, URL).token), None);
        });
    }

    #[test]
    fn an_adoption_that_cannot_delete_the_old_token_leaves_nothing_behind() {
        let server = "legacy-test-undo-fixture";
        with_server_from_an_older_version(server, URL, || {
            let legacy = seed_legacy_secrets(server);
            set_test_fault(&legacy.token, Some(TestFault::DeleteRefused));

            assert!(load_oauth_token(server, URL).is_err());

            assert_eq!(stored(&entries(server, URL).token), None);
            assert_eq!(stored(&entries(server, URL).credentials), None);
            assert_eq!(stored(&legacy.token), Some("old-token".to_string()));
        });
    }

    #[test]
    fn the_strict_delete_of_older_secrets_fails_while_one_is_still_stored() {
        let server = "legacy-test-strict-fixture";
        let legacy = seed_legacy_secrets(server);
        set_test_fault(&legacy.token, Some(TestFault::DeleteRefused));

        let err = delete_legacy_oauth_secrets(server).expect_err("still stored");

        assert!(err.contains("old OAuth token"), "unexpected error: {err}");
        set_test_fault(&legacy.token, None);
        delete_legacy_oauth_secrets(server).expect("now it can be deleted");
        assert_eq!(stored(&legacy.token), None);
        delete_legacy_oauth_secrets(server).expect("nothing stored is success");
    }

    // ── The proxy asks for the token filed under the saved URL ───────────────

    #[test]
    fn the_proxy_does_not_start_for_a_server_whose_token_is_filed_under_another_url() {
        let server = "run-proxy-other-url-fixture";
        with_saved_server(server, URL, || {
            store_oauth_token(server, OTHER_URL, "token").unwrap();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime");

            let err = runtime
                .block_on(run_proxy(server))
                .expect_err("no token for the saved url")
                .to_string();

            assert!(err.contains("No OAuth token found"), "unexpected error: {err}");
        });
    }

    #[test]
    fn older_credentials_are_not_adopted_without_a_token() {
        let server = "legacy-test-creds-only-fixture";
        with_server_from_an_older_version(server, URL, || {
            let legacy = legacy_oauth_entries(server);
            secret_store::store_secret(&legacy.credentials, CREDENTIALS).unwrap();

            assert_eq!(has_oauth_token(server, URL), Ok(false));
            assert!(load_oauth_credentials(server, URL).is_err());
            assert_eq!(stored(&entries(server, URL).credentials), None);
        });
    }

    #[test]
    fn a_rename_carries_an_older_token_to_the_new_name() {
        let (old, new) = ("legacy-test-move-old-fixture", "legacy-test-move-new-fixture");
        with_server_from_an_older_version(old, URL, || {
            let legacy = seed_legacy_secrets(old);

            move_oauth_secrets(old, new, URL);

            assert_eq!(load_oauth_token(new, URL).unwrap(), "old-token");
            assert_eq!(load_oauth_credentials(new, URL).unwrap(), CREDENTIALS);
            assert_eq!(stored(&legacy.token), None);
            assert_eq!(stored(&entries(old, URL).token), None);
        });
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
