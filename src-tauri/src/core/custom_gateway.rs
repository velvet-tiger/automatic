//! The user's custom LLM gateway for Claude.
//!
//! One saved gateway drives two consumers: Automatic's Anthropic client and
//! Claude Code. The URL, credential kind and the record of what was written
//! live in `custom-gateway.json`, apart from general settings. Settings heal
//! when damaged and pass through the frontend. This record must do neither: if
//! it is lost, the token stays in Claude Code's file with nothing that knows
//! about it. The token lives in the secret store under a fixed account that is not a provider id, so the
//! credential commands and the `automatic_get_credential` MCP tool refuse it.
//! Claude Code only reads the token from its own settings file, so
//! [`claude_code_env`](super::claude_code_env) also writes it there.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use super::agents::{CustomGatewayRoute, GatewayCredentialKind};
use super::claude_code_env::{self, ClaudeCodeRecord, GatewayEnv, KnownValues};
use super::credentials::{any_known_api_key_stored, set_agent_features_enabled};
use super::secret_store;
use super::paths::get_automatic_dir;
use super::settings::write_atomically;
use super::store_reads::StoreRead;

/// The secret store account that holds the gateway token, bound to its URL.
pub(crate) const TOKEN_ACCOUNT: &str = "anthropic-custom-gateway";

/// The saved gateway. Holds no secret.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomGateway {
    pub base_url: String,
    pub credential_kind: GatewayCredentialKind,
    /// What Automatic last wrote to Claude Code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_code: Option<ClaudeCodeRecord>,
}

/// What the frontend may know about the gateway.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CustomGatewayStatus {
    pub configured: bool,
    pub base_url: Option<String>,
    pub credential_kind: Option<GatewayCredentialKind>,
    /// The record is gone but the keychain still holds a token, so Claude Code
    /// may still use the gateway. Claude requests are refused until this is
    /// cleared or saved again.
    pub orphaned_token: bool,
}

/// The outcome of a clear.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ClearedCustomGateway {
    /// Gateway variables still set in Claude Code's file after the clear.
    pub left_in_claude_code: Vec<String>,
}

/// How the gateway step of an erase went.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum GatewayErase {
    Cleared(ClearedCustomGateway),
    /// Claude Code's file may still hold the token, so nothing may be erased.
    /// The text is ready to show.
    FileNotCleared(String),
    /// Claude Code's file is done. The keychain kept the token.
    TokenNotDeleted(String),
}

/// The token as it sits in the secret store. The URL travels with the token,
/// so a save that stops half way cannot pair one gateway's token with another
/// gateway's URL.
#[derive(Serialize, Deserialize)]
struct BoundToken {
    base_url: String,
    token: String,
}

/// Save and clear each touch three stores, so they run one at a time.
fn change_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

// ── Validation ────────────────────────────────────────────────────────────────

/// Parse a gateway base URL into the form requests are built from, with no
/// trailing slash.
///
/// The token goes to this address in a header, so it must be HTTPS. Plain HTTP
/// is allowed only for a gateway on this machine. Credentials in the URL would
/// end up in logs and error messages, and a query or fragment could not
/// survive having `/v1/messages` added to it.
pub(crate) fn parse_gateway_url(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.chars().any(|c| c.is_whitespace() || c.is_control()) {
        // The URL parser removes tabs and newlines without saying so.
        return Err("The gateway URL cannot contain spaces or line breaks.".to_string());
    }
    let url = url::Url::parse(trimmed)
        .map_err(|_| "The gateway URL must be a full URL, such as https://gateway.example.com.")?;

    let host = url.host_str().unwrap_or_default();
    if host.is_empty() {
        return Err("The gateway URL must include a host name.".to_string());
    }
    match url.scheme() {
        "https" => {}
        "http" if matches!(host, "localhost" | "127.0.0.1" | "[::1]") => {}
        "http" => {
            return Err("The gateway URL must use https, except on localhost.".to_string());
        }
        _ => return Err("The gateway URL must start with https://.".to_string()),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(
            "The gateway URL cannot contain a user name or password. Put the credential in the token field."
                .to_string(),
        );
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("The gateway URL cannot contain a query or fragment.".to_string());
    }
    if url.path().trim_end_matches('/').to_ascii_lowercase().ends_with("/v1") {
        return Err(
            "Leave /v1 off the gateway URL. Automatic and Claude Code add it themselves."
                .to_string(),
        );
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}

/// Whether two validated URLs share a scheme, host and port. A token is only
/// kept for the same address. A new address might be run by someone else.
fn same_address(first: &str, second: &str) -> bool {
    match (url::Url::parse(first), url::Url::parse(second)) {
        (Ok(first), Ok(second)) => first.origin() == second.origin(),
        _ => false,
    }
}

/// Parse a token into the value sent in a request header.
fn parse_token(input: &str) -> Result<String, String> {
    let token = input.trim();
    if token.is_empty() {
        return Err("Enter the gateway token.".to_string());
    }
    if !token.chars().all(|c| c.is_ascii_graphic()) {
        return Err(
            "The gateway token cannot contain spaces, line breaks or non-ASCII characters."
                .to_string(),
        );
    }
    Ok(token.to_string())
}

// ── Reads ─────────────────────────────────────────────────────────────────────

/// The file that holds the saved gateway.
pub(crate) fn gateway_file_path() -> Result<PathBuf, String> {
    Ok(get_automatic_dir()?.join("custom-gateway.json"))
}

/// Whether a gateway is saved. Reads no secret, and not even the file: a
/// damaged file is reported when the gateway is used.
pub(crate) fn custom_gateway_saved() -> bool {
    gateway_file_path().is_ok_and(|path| path.exists())
}

/// The saved gateway. A file that cannot be parsed is an error, never `None`:
/// `None` would send Claude requests straight to Anthropic, and a clear would
/// leave the token in Claude Code's file.
pub(crate) fn read_gateway() -> Result<Option<CustomGateway>, String> {
    let path = gateway_file_path()?;
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Could not read {}: {}", path.display(), e)),
    };
    serde_json::from_str(&raw).map(Some).map_err(|e| {
        format!(
            "{} could not be parsed (line {}). Fix the file, then try again.",
            path.display(),
            e.line()
        )
    })
}

fn write_gateway(gateway: &CustomGateway) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(gateway).map_err(|e| e.to_string())?;
    write_atomically(&gateway_file_path()?, &raw)
}

fn remove_gateway_file() -> Result<(), String> {
    match fs::remove_file(gateway_file_path()?) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// The gateway's public details. Reads no secret.
pub fn custom_gateway_status() -> Result<CustomGatewayStatus, String> {
    let gateway = read_gateway()?;
    Ok(CustomGatewayStatus {
        configured: gateway.is_some(),
        base_url: gateway.as_ref().map(|g| g.base_url.clone()),
        credential_kind: gateway.as_ref().map(|g| g.credential_kind),
        orphaned_token: gateway.is_none() && token_entry_present()?,
    })
}

/// Whether the keychain holds anything under the gateway account. This is a
/// keychain read, so it is for real requests and user-opened pages only, never
/// a background or polling path.
fn token_entry_present() -> Result<bool, String> {
    secret_store::load_secret_if_present(TOKEN_ACCOUNT)
        .map(|value| value.is_some())
        .map_err(|e| format!("Could not read the gateway token: {}", e))
}

/// Refuse a request when the gateway record is gone but its token is not.
/// Without the record the request would go straight to Anthropic while Claude
/// Code keeps using the gateway, so the user would not know which one is in use.
pub(crate) fn refuse_if_token_is_orphaned() -> Result<(), String> {
    let present = token_entry_present().map_err(|e| {
        format!(
            "Automatic could not check for a custom gateway, so this request was not sent. {}",
            e
        )
    })?;
    if present {
        return Err(
            "Automatic has lost its custom gateway record, but the gateway token is still \
             stored, so this request was not sent. Save the gateway again or clear it in \
             Settings > Agents."
                .to_string(),
        );
    }
    Ok(())
}

fn store_bound_token(base_url: &str, token: &str) -> Result<(), String> {
    let bound = BoundToken {
        base_url: base_url.to_string(),
        token: token.to_string(),
    };
    let value = serde_json::to_string(&bound)
        .map_err(|e| format!("Could not prepare the gateway token: {}", e))?;
    secret_store::store_secret(TOKEN_ACCOUNT, &value)
        .map_err(|e| format!("Could not store the gateway token: {}", e))
}

/// What the secret store holds, if it can be read and parsed.
fn stored_binding() -> Result<Option<BoundToken>, String> {
    let Some(value) = secret_store::load_secret_if_present(TOKEN_ACCOUNT)
        .map_err(|e| format!("Could not read the gateway token: {}", e))?
    else {
        return Ok(None);
    };
    // The parse error is dropped because serde can quote what it read.
    serde_json::from_str(&value)
        .map(Some)
        .map_err(|_| enter_token_again("The custom gateway token could not be read."))
}

fn enter_token_again(problem: &str) -> String {
    format!("{} Enter it again in Settings > Agents.", problem)
}

/// The token saved for a URL that `accepts`, with a message the user can act
/// on when there is none.
fn bound_token(accepts: impl Fn(&str) -> bool) -> Result<String, String> {
    let bound = stored_binding()?
        .ok_or_else(|| enter_token_again("The custom gateway token is missing."))?;
    if !accepts(&bound.base_url) {
        return Err(enter_token_again(
            "The custom gateway token was saved for a different URL.",
        ));
    }
    Ok(bound.token)
}

/// The token saved for exactly `base_url`. A token saved for another URL is
/// never sent to this one.
fn stored_token(base_url: &str) -> Result<String, String> {
    bound_token(|saved| saved == base_url)
}

/// The route for a saved gateway. The file can be edited by hand, so the URL
/// is checked again here before a token is sent to it.
pub(crate) fn gateway_route(gateway: &CustomGateway) -> Result<CustomGatewayRoute, String> {
    let base_url = parse_gateway_url(&gateway.base_url)?;
    let token = stored_token(&base_url)?;
    Ok(CustomGatewayRoute {
        base_url,
        kind: gateway.credential_kind,
        token,
    })
}

// ── Save ──────────────────────────────────────────────────────────────────────

/// The token for a save. A blank one keeps the stored token, but only for the
/// same address.
fn token_for_save(
    typed: Option<&str>,
    base_url: &str,
    previous: Option<&CustomGateway>,
) -> Result<String, String> {
    if let Some(token) = typed.map(str::trim).filter(|t| !t.is_empty()) {
        return parse_token(token);
    }
    let Some(previous) = previous else {
        return Err("Enter the gateway token.".to_string());
    };
    if !same_address(&previous.base_url, base_url) {
        return Err(
            "Enter the token again. A saved token is only reused for the same address.".to_string(),
        );
    }
    // The token may already be bound to a new path at this address, when an
    // earlier save stopped after rebinding it.
    bound_token(|saved| same_address(saved, base_url))
}

/// Save the gateway for Automatic and Claude Code. A blank `token` keeps the
/// one already stored, as long as the address stays the same.
///
/// The steps run in an order that keeps a failure safe to retry: validate,
/// check Claude Code's file for conflicts, store the token, write that file,
/// then write the gateway file. A failed first save takes its changes back.
///
/// A typed token is stored before Claude Code's file is written. A kept token
/// moving to a new path is rebound after, so a failed write leaves it usable.
pub fn save_custom_gateway(
    base_url: &str,
    credential_kind: GatewayCredentialKind,
    token: Option<&str>,
) -> Result<(), String> {
    let _changing = change_lock();

    let base_url = parse_gateway_url(base_url)?;
    let previous = read_gateway()?;
    let typed_token = token.is_some_and(|t| !t.trim().is_empty());
    let token = token_for_save(token, &base_url, previous.as_ref())?;

    let path = claude_code_env::settings_path()?;
    let desired = GatewayEnv {
        base_url: &base_url,
        kind: credential_kind,
        token: &token,
    };
    let record = previous.as_ref().and_then(|g| g.claude_code.as_ref());
    claude_code_env::check_file(&path, &desired, record)?;

    let first_credential =
        previous.is_none() && any_known_api_key_stored(StoreRead::Now) == Ok(false);

    // Read before the store below, so this says what an earlier save left.
    let held = stored_binding().ok().flatten();
    let known = KnownValues {
        // A save can write the file and then fail to rebind a kept token, so
        // the same token also vouches for the URL it was being saved with.
        base_url: held
            .as_ref()
            .is_some_and(|bound| bound.base_url == base_url || bound.token == token),
        token: held.as_ref().is_some_and(|bound| bound.token == token),
    };

    if typed_token {
        store_bound_token(&base_url, &token)?;
    }

    let written = claude_code_env::write_file(&path, &desired, record, known).map_err(|e| {
        match &previous {
            None => {
                // The form still shows no gateway, so nothing could clear this.
                match delete_stored_token() {
                    Ok(()) => e,
                    Err(undo) => format!(
                        "{} The gateway token could not be removed from the keychain either: {}",
                        e, undo
                    ),
                }
            }
            Some(_) if !typed_token => format!(
                "Claude Code was not updated, so the saved gateway has not changed. \
                 Save again to finish. {}",
                e
            ),
            Some(previous) if previous.base_url == base_url => format!(
                "Automatic has the new token, but Claude Code still has the old one. \
                 Save again to finish. {}",
                e
            ),
            Some(_) => format!(
                "Claude Code was not updated, and Automatic cannot use the gateway until \
                 this save finishes. Save again with the same token. {}",
                e
            ),
        }
    })?;

    if !typed_token && held.as_ref().is_none_or(|bound| bound.base_url != base_url) {
        store_bound_token(&base_url, &token).map_err(|e| {
            format!(
                "Claude Code now uses this gateway, but Automatic could not update its own \
                 copy. Save again to finish. {}",
                e
            )
        })?;
    }

    let gateway = CustomGateway {
        base_url,
        credential_kind,
        claude_code: Some(written),
    };
    record_gateway(&gateway, first_credential).map_err(|e| match &previous {
        None => undo_first_save(&path, &gateway, &e),
        Some(_) => format!(
            "Claude Code now uses this gateway, but Automatic could not record it. \
             Save again with the same token to finish. {}",
            e
        ),
    })
}

fn record_gateway(gateway: &CustomGateway, first_credential: bool) -> Result<(), String> {
    write_gateway(gateway)?;
    if first_credential {
        set_agent_features_enabled(true);
    }
    Ok(())
}

/// Take back a first save that Automatic could not record. Without a saved
/// gateway the user would have no way to clear what is in Claude Code's file.
fn undo_first_save(path: &Path, gateway: &CustomGateway, error: &str) -> String {
    let file_undone = match &gateway.claude_code {
        Some(record) => claude_code_env::clear_file(path, record).map(|_| ()),
        None => Ok(()),
    };
    let token_undone = delete_stored_token();
    match file_undone.and(token_undone) {
        Ok(()) => format!(
            "Automatic could not record the gateway, so the change was taken back. {}",
            error
        ),
        Err(undo_error) => format!(
            "Automatic could not record the gateway, and could not take the change back. \
             Check {} for the gateway values. {} {}",
            path.display(),
            error,
            undo_error
        ),
    }
}

// ── Clear ─────────────────────────────────────────────────────────────────────

/// Remove the gateway from Claude Code, the secret store and the gateway file,
/// in that order. A failure leaves the file in place, so a second clear can
/// finish the job.
pub fn clear_custom_gateway() -> Result<ClearedCustomGateway, String> {
    let _changing = change_lock();

    let left_in_claude_code = match read_gateway()? {
        Some(gateway) => clear_claude_code(&gateway)?,
        None if token_entry_present()? => clear_orphaned_values()?,
        None => {
            return Ok(ClearedCustomGateway {
                left_in_claude_code: Vec::new(),
            })
        }
    };

    delete_stored_token().map_err(|e| {
        format!(
            "Claude Code's settings were updated, but the gateway token could not be removed \
             from the keychain. Clear again to finish. {}",
            e
        )
    })?;

    remove_gateway_file().map_err(|e| {
        format!(
            "The gateway was removed from Claude Code and the keychain, but Automatic could \
             not remove its own record. Clear again to finish. {}",
            e
        )
    })?;

    // The same rule as removing the last API key.
    if any_known_api_key_stored(StoreRead::Now) == Ok(false) {
        set_agent_features_enabled(false);
    }
    Ok(ClearedCustomGateway { left_in_claude_code })
}

fn clear_claude_code(gateway: &CustomGateway) -> Result<Vec<String>, String> {
    let path = claude_code_env::settings_path()?;
    match &gateway.claude_code {
        Some(record) => claude_code_env::clear_file(&path, record),
        // With no record nothing can be called Automatic's, so this only reports.
        None => claude_code_env::clear_matching(&path, None, None),
    }
}

/// Clear Claude Code's file when the record is gone. The keychain still says
/// what was saved, so exact matches are removed and anything else is reported.
/// A keychain value that cannot be parsed matches nothing.
fn clear_orphaned_values() -> Result<Vec<String>, String> {
    let bound = stored_binding().ok().flatten();
    claude_code_env::clear_matching(
        &claude_code_env::settings_path()?,
        bound.as_ref().map(|b| b.base_url.as_str()),
        bound.as_ref().map(|b| b.token.as_str()),
    )
}

/// Clear the gateway before an erase. The token is always deleted, whatever
/// happens to Claude Code's file, and whether or not the record can be read.
pub(crate) fn clear_gateway_for_erase() -> GatewayErase {
    let _changing = change_lock();

    let gateway = read_gateway();
    let file = match &gateway {
        Ok(Some(gateway)) => clear_claude_code(gateway),
        // A keychain that cannot be read is reported by the delete below.
        Ok(None) if token_entry_present().unwrap_or(false) => clear_orphaned_values(),
        _ => Ok(Vec::new()),
    };
    let token = delete_stored_token();
    let token_note = match &token {
        Ok(()) => "The gateway token was deleted from the keychain.".to_string(),
        Err(e) => format!("The gateway token could not be deleted from the keychain: {}", e),
    };

    let claude_settings = claude_code_env::settings_path()
        .map_or_else(|_| "Claude Code's settings".to_string(), |p| p.display().to_string());
    if let Err(e) = gateway {
        return GatewayErase::FileNotCleared(format!(
            "App data was not erased, because Automatic could not read its record of what it \
             wrote to Claude Code. The gateway token may remain in plain text in {}. {} {}",
            claude_settings, e, token_note
        ));
    }
    match (file, token) {
        (Err(e), _) => GatewayErase::FileNotCleared(format!(
            "App data was not erased, because the custom gateway could not be cleared from \
             {}. Fix that file and try again. {} {}",
            claude_settings, e, token_note
        )),
        (Ok(_), Err(e)) => GatewayErase::TokenNotDeleted(e),
        (Ok(left_in_claude_code), Ok(())) => {
            GatewayErase::Cleared(ClearedCustomGateway { left_in_claude_code })
        }
    }
}

/// Delete the token. The store reports a missing secret as an error, and a
/// retried clear may find it already gone.
fn delete_stored_token() -> Result<(), String> {
    match secret_store::load_secret_if_present(TOKEN_ACCOUNT)? {
        Some(_) => secret_store::delete_secret(TOKEN_ACCOUNT),
        None => Ok(()),
    }
}

// ── Test support ──────────────────────────────────────────────────────────────

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    /// The test secret store is shared by the whole process and the token
    /// account is fixed, so tests that use it run one at a time.
    ///
    /// Each test starts with the account empty, whatever the last one left.
    pub(crate) fn gateway_store_lock() -> MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        let guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        delete_stored_token().expect("empty the token account");
        guard
    }

    /// Lose the record as a deleted file would, leaving the keychain as it is.
    pub(crate) fn lose_gateway_record() {
        remove_gateway_file().expect("remove the record");
    }

    /// Save a gateway in the calling test's home.
    pub(crate) fn save_test_gateway(token: &str) {
        save_custom_gateway(
            "https://gateway.example.com",
            GatewayCredentialKind::Bearer,
            Some(token),
        )
        .expect("save gateway");
    }

    /// Change the saved URL as a hand edit would, without touching the token.
    pub(crate) fn set_saved_gateway_url(url: &str) {
        let mut gateway = read_gateway().expect("read").expect("gateway");
        gateway.base_url = url.to_string();
        write_gateway(&gateway).expect("write");
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{
        gateway_store_lock, lose_gateway_record, save_test_gateway, set_saved_gateway_url,
    };
    use crate::core::settings::{read_settings, write_settings};
    use super::*;
    use crate::core::secret_store::{set_test_fault, TestFault};
    use crate::core::temp_test_home;
    use std::fs;

    const BEARER: GatewayCredentialKind = GatewayCredentialKind::Bearer;
    const API_KEY: GatewayCredentialKind = GatewayCredentialKind::ApiKey;
    const URL: &str = "https://gateway.example.com";

    fn claude_settings_text() -> String {
        let path = claude_code_env::settings_path().expect("path");
        fs::read_to_string(path).expect("read Claude Code settings")
    }

    fn write_claude_settings(text: &str) {
        let path = claude_code_env::settings_path().expect("path");
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, text).expect("write");
    }

    fn remove_token() {
        let _ = secret_store::delete_secret(TOKEN_ACCOUNT);
    }

    // ── URL validation ───────────────────────────────────────────────────────

    #[test]
    fn https_urls_are_accepted_and_lose_their_trailing_slash() {
        for (input, expected) in [
            ("https://gateway.example.com", "https://gateway.example.com"),
            ("https://gateway.example.com/", "https://gateway.example.com"),
            ("  https://gateway.example.com/llm/  ", "https://gateway.example.com/llm"),
            ("https://gateway.example.com:8443", "https://gateway.example.com:8443"),
        ] {
            assert_eq!(parse_gateway_url(input).as_deref(), Ok(expected));
        }
    }

    #[test]
    fn plain_http_is_accepted_only_on_this_machine() {
        for url in ["http://localhost:4000", "http://127.0.0.1:4000", "http://[::1]:4000"] {
            assert_eq!(parse_gateway_url(url).as_deref(), Ok(url));
        }
        for url in [
            "http://gateway.example.com",
            "http://localhost.example.com",
            "http://127.0.0.1.example.com",
            "http://192.168.1.10",
        ] {
            let err = parse_gateway_url(url).expect_err(url);
            assert!(err.contains("https"), "{err}");
        }
    }

    #[test]
    fn urls_that_could_leak_or_break_are_refused() {
        for url in [
            "https://user:secret@gateway.example.com",
            "https://user@gateway.example.com",
            "https://gateway.example.com?key=secret",
            "https://gateway.example.com?",
            "https://gateway.example.com#part",
            "https://gateway.example.com/a b",
            "https://gateway\n.example.com",
            "gateway.example.com",
            "ftp://gateway.example.com",
            "file:///etc/hosts",
            "mailto:me@example.com",
            "not a url",
            "",
        ] {
            let err = parse_gateway_url(url).expect_err(url);
            assert!(!err.contains("secret"), "{err}");
        }
    }

    // ── Token validation ─────────────────────────────────────────────────────

    #[test]
    fn a_url_ending_in_v1_is_refused_because_v1_gets_added() {
        for url in [
            "https://gateway.example.com/v1",
            "https://gateway.example.com/v1/",
            "https://gateway.example.com/llm/V1",
        ] {
            let err = parse_gateway_url(url).expect_err(url);
            assert!(err.contains("/v1"), "{err}");
        }
        assert!(parse_gateway_url("https://gateway.example.com/v1beta").is_ok());
        assert_eq!(parse_gateway_url("https://v1").as_deref(), Ok("https://v1"));
        assert!(parse_gateway_url("https://gateway.v1/").is_ok());
    }

    #[test]
    fn tokens_are_trimmed_and_must_fit_in_a_header() {
        assert_eq!(parse_token("  token-1\n").as_deref(), Ok("token-1"));
        for token in ["", "   ", "two\nlines", "two\r\nlines", "with space", "tökén"] {
            assert!(parse_token(token).is_err(), "{token:?}");
        }
    }

    // ── Save ─────────────────────────────────────────────────────────────────

    #[test]
    fn a_save_reaches_every_store_and_keeps_the_token_out_of_settings() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();

        save_custom_gateway(URL, BEARER, Some("plain-token")).expect("save");

        assert_eq!(stored_token(URL).as_deref(), Ok("plain-token"));
        assert!(claude_settings_text().contains("plain-token"));
        let saved = fs::read_to_string(gateway_file_path().expect("path")).expect("read record");
        assert!(saved.contains(URL), "{saved}");
        assert!(!saved.contains("plain-token"), "{saved}");
        remove_token();
    }

    #[test]
    fn the_status_reports_the_gateway_without_the_token() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        assert_eq!(
            custom_gateway_status(),
            Ok(CustomGatewayStatus {
                configured: false,
                base_url: None,
                credential_kind: None,
                orphaned_token: false,
            })
        );

        save_test_gateway("plain-token");

        let status = custom_gateway_status().expect("status");
        assert!(status.configured);
        assert_eq!(status.base_url.as_deref(), Some(URL));
        assert_eq!(status.credential_kind, Some(BEARER));
        assert!(!serde_json::to_string(&status).expect("json").contains("plain-token"));
        remove_token();
    }

    #[test]
    fn a_first_save_needs_a_token_and_a_later_one_can_keep_it() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();

        assert!(save_custom_gateway(URL, BEARER, Some("  ")).is_err());
        assert!(save_custom_gateway(URL, BEARER, None).is_err());

        save_test_gateway("plain-token");
        let moved = "https://gateway.example.com/team";
        save_custom_gateway(moved, API_KEY, Some("")).expect("keep the token");

        assert_eq!(stored_token(moved).as_deref(), Ok("plain-token"));
        let text = claude_settings_text();
        assert!(text.contains(moved), "{text}");
        assert!(text.contains(r#""ANTHROPIC_API_KEY": "plain-token""#), "{text}");
        assert!(!text.contains("ANTHROPIC_AUTH_TOKEN"), "{text}");
        remove_token();
    }

    #[test]
    fn a_conflict_stops_the_save_before_anything_is_stored() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        let theirs = r#"{"env":{"ANTHROPIC_API_KEY":"theirs"}}"#;
        write_claude_settings(theirs);

        let err = save_custom_gateway(URL, BEARER, Some("plain-token")).expect_err("conflict");

        assert!(err.contains("ANTHROPIC_API_KEY"), "{err}");
        assert_eq!(secret_store::load_secret_if_present(TOKEN_ACCOUNT), Ok(None));
        assert_eq!(claude_settings_text(), theirs);
        assert!(!custom_gateway_status().expect("status").configured);
    }

    #[cfg(unix)]
    fn read_only(folder: &std::path::Path, read_only: bool) {
        use std::os::unix::fs::PermissionsExt;
        let mode = if read_only { 0o555 } else { 0o755 };
        fs::set_permissions(folder, fs::Permissions::from_mode(mode)).expect("chmod");
    }

    #[cfg(unix)]
    #[test]
    fn a_first_save_that_cannot_write_claude_code_stores_no_token() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        let folder = claude_code_env::settings_path().expect("path");
        let folder = folder.parent().expect("parent");
        fs::create_dir_all(folder).expect("mkdir");
        read_only(folder, true);

        let result = save_custom_gateway(URL, BEARER, Some("plain-token"));

        read_only(folder, false);
        assert!(result.is_err());
        assert_eq!(secret_store::load_secret_if_present(TOKEN_ACCOUNT), Ok(None));
        assert!(!custom_gateway_status().expect("status").configured);
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_first_save_reports_a_token_it_could_not_remove() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        let folder = claude_code_env::settings_path().expect("path");
        let folder = folder.parent().expect("parent");
        fs::create_dir_all(folder).expect("mkdir");
        read_only(folder, true);
        set_test_fault(TOKEN_ACCOUNT, Some(TestFault::DeleteRefused));

        let err = save_custom_gateway(URL, BEARER, Some("plain-token")).expect_err("no write");

        set_test_fault(TOKEN_ACCOUNT, None);
        read_only(folder, false);
        assert!(err.contains("could not be removed from the keychain"), "{err}");
        remove_token();
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_path_change_with_a_kept_token_can_be_retried_blank() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("plain-token");
        let path = claude_code_env::settings_path().expect("path");
        let folder = path.parent().expect("parent");
        read_only(folder, true);
        let moved = "https://gateway.example.com/team";

        let err = save_custom_gateway(moved, BEARER, None).expect_err("no write");

        read_only(folder, false);
        assert!(err.contains("has not changed"), "{err}");
        // The saved gateway still works.
        let saved = read_gateway().expect("read").expect("gateway");
        assert_eq!(gateway_route(&saved).expect("route").base_url, URL);

        save_custom_gateway(moved, BEARER, None).expect("retry");
        assert_eq!(stored_token(moved).as_deref(), Ok("plain-token"));
        remove_token();
    }

    #[cfg(unix)]
    #[test]
    fn a_kept_token_retry_works_after_the_record_failed_to_write() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("plain-token");
        let folder = crate::core::get_automatic_dir().expect("dir");
        read_only(&folder, true);
        let moved = "https://gateway.example.com/team";

        assert!(save_custom_gateway(moved, BEARER, None).is_err());

        read_only(&folder, false);
        save_custom_gateway(moved, BEARER, None).expect("retry");
        assert!(custom_gateway_status().expect("status").base_url.as_deref() == Some(moved));
        remove_token();
    }

    #[cfg(unix)]
    #[test]
    fn a_rotation_that_cannot_write_claude_code_says_what_happened() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("first-token");
        let path = claude_code_env::settings_path().expect("path");
        let folder = path.parent().expect("parent");
        read_only(folder, true);

        let err = save_custom_gateway(URL, BEARER, Some("second-token")).expect_err("no write");

        read_only(folder, false);
        assert!(err.contains("still has the old one"), "{err}");
        assert!(!err.contains("second-token"), "{err}");
        assert!(claude_settings_text().contains("first-token"));

        // Retrying finishes the job.
        save_custom_gateway(URL, BEARER, Some("second-token")).expect("retry");
        assert!(claude_settings_text().contains("second-token"));
        remove_token();
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_move_to_a_new_url_never_pairs_the_new_token_with_the_old_url() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("token-for-a");
        let path = claude_code_env::settings_path().expect("path");
        let folder = path.parent().expect("parent");
        read_only(folder, true);

        let err = save_custom_gateway("https://b.example.com", BEARER, Some("token-for-b"))
            .expect_err("no write");

        read_only(folder, false);
        assert!(err.contains("cannot use the gateway"), "{err}");
        let saved = read_gateway().expect("read").expect("gateway");
        assert_eq!(saved.base_url, URL);
        let refused = gateway_route(&saved).expect_err("no route");
        assert!(refused.contains("different URL"), "{refused}");
        remove_token();
    }

    #[cfg(unix)]
    #[test]
    fn a_first_save_automatic_cannot_record_is_taken_back() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        let theirs = r#"{"model":"opus"}"#;
        write_claude_settings(theirs);
        let settings_dir = crate::core::get_automatic_dir().expect("dir");
        fs::create_dir_all(&settings_dir).expect("mkdir");
        read_only(&settings_dir, true);

        let err = save_custom_gateway(URL, BEARER, Some("plain-token")).expect_err("no record");

        read_only(&settings_dir, false);
        assert!(err.contains("taken back"), "{err}");
        let text = claude_settings_text();
        assert!(!text.contains("ANTHROPIC"), "{text}");
        assert!(text.contains("opus"), "{text}");
        assert_eq!(secret_store::load_secret_if_present(TOKEN_ACCOUNT), Ok(None));
    }

    #[cfg(unix)]
    #[test]
    fn a_later_save_automatic_cannot_record_keeps_claude_code_and_says_so() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("first-token");
        let settings_dir = crate::core::get_automatic_dir().expect("dir");
        read_only(&settings_dir, true);

        let err = save_custom_gateway(URL, BEARER, Some("second-token")).expect_err("no record");

        read_only(&settings_dir, false);
        assert!(err.contains("could not record"), "{err}");
        assert!(claude_settings_text().contains("second-token"));
        save_custom_gateway(URL, BEARER, Some("second-token")).expect("retry");
        remove_token();
    }

    #[test]
    fn a_hand_edited_url_is_checked_before_the_token_is_used() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("plain-token");

        for url in ["http://gateway.example.com", "https://evil.example.com"] {
            set_saved_gateway_url(url);
            let saved = read_gateway().expect("read").expect("gateway");
            assert!(gateway_route(&saved).is_err(), "{url}");
        }
        remove_token();
    }

    #[test]
    fn a_new_address_needs_a_token_and_a_new_path_does_not() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("plain-token");

        for url in [
            "https://other.example.com",
            "https://gateway.example.com:8443",
            "http://localhost:4000",
        ] {
            let err = save_custom_gateway(url, BEARER, Some("")).expect_err(url);
            assert!(err.contains("Enter the token again"), "{err}");
        }
        assert!(save_custom_gateway("https://gateway.example.com/team", BEARER, None).is_ok());
        remove_token();
    }

    #[test]
    fn a_failed_token_store_leaves_claude_code_alone() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        set_test_fault(TOKEN_ACCOUNT, Some(TestFault::Unreadable));

        let result = save_custom_gateway(URL, BEARER, Some("plain-token"));

        set_test_fault(TOKEN_ACCOUNT, None);
        assert!(result.is_err());
        assert!(!claude_code_env::settings_path().expect("path").exists());
        assert!(!custom_gateway_status().expect("status").configured);
    }

    #[test]
    fn a_retry_after_the_record_was_lost_is_not_taken_for_the_users_values() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("first-token");
        save_test_gateway("plain-token");

        // Claude Code and the keychain hold the values. Automatic's record does not.
        let mut gateway = read_gateway().expect("read").expect("gateway");
        gateway.claude_code = None;
        write_gateway(&gateway).expect("write");

        save_test_gateway("plain-token");

        let cleared = clear_custom_gateway().expect("clear");
        assert!(cleared.left_in_claude_code.is_empty(), "{cleared:?}");
        assert!(!claude_settings_text().contains("plain-token"));
    }

    #[test]
    fn a_value_the_user_typed_into_the_file_is_still_adopted() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        write_claude_settings(r#"{"env":{"ANTHROPIC_AUTH_TOKEN":"plain-token"}}"#);

        save_test_gateway("plain-token");

        let cleared = clear_custom_gateway().expect("clear");
        assert_eq!(cleared.left_in_claude_code, ["ANTHROPIC_AUTH_TOKEN"]);
        assert!(claude_settings_text().contains("plain-token"));
    }

    #[test]
    fn a_first_gateway_turns_agent_features_on_as_a_first_key_does() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        let mut settings = read_settings().expect("read");
        settings.agent_features_enabled = Some(false);
        write_settings(&settings).expect("write");

        save_test_gateway("plain-token");

        let settings = read_settings().expect("read");
        assert_eq!(settings.agent_features_enabled, Some(true));
        remove_token();
    }

    // ── Clear ────────────────────────────────────────────────────────────────

    #[test]
    fn a_clear_removes_the_gateway_everywhere() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("plain-token");

        let cleared = clear_custom_gateway().expect("clear");

        assert!(cleared.left_in_claude_code.is_empty());
        assert_eq!(secret_store::load_secret_if_present(TOKEN_ACCOUNT), Ok(None));
        assert!(!custom_gateway_status().expect("status").configured);
        assert!(!claude_settings_text().contains("ANTHROPIC"));
    }

    #[test]
    fn a_clear_reports_a_value_the_user_changed() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("plain-token");
        write_claude_settings(
            &claude_settings_text().replace("plain-token", "their-replacement"),
        );

        let cleared = clear_custom_gateway().expect("clear");

        assert_eq!(cleared.left_in_claude_code, ["ANTHROPIC_AUTH_TOKEN"]);
        assert!(claude_settings_text().contains("their-replacement"));
    }

    #[test]
    fn a_keychain_that_refuses_the_delete_leaves_a_clear_to_retry() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("plain-token");

        set_test_fault(TOKEN_ACCOUNT, Some(TestFault::DeleteRefused));
        let first = clear_custom_gateway();
        set_test_fault(TOKEN_ACCOUNT, None);

        let err = first.expect_err("the delete failed");
        assert!(err.contains("Clear again"), "{err}");
        assert!(custom_gateway_status().expect("status").configured);

        assert!(clear_custom_gateway().is_ok());
        assert_eq!(secret_store::load_secret_if_present(TOKEN_ACCOUNT), Ok(None));
    }

    // ── Erase ────────────────────────────────────────────────────────────────

    #[test]
    fn an_erase_stops_when_claude_code_cannot_be_cleared_but_deletes_the_token() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("plain-token");
        let good = claude_settings_text();
        write_claude_settings("{not json");

        let err = crate::core::erase_app_data().expect_err("stopped");

        assert!(err.contains(".claude/settings.json"), "{err}");
        assert!(err.contains("not erased"), "{err}");
        assert!(err.contains("token was deleted"), "{err}");
        assert!(custom_gateway_status().expect("status").configured, "app data was kept");
        assert_eq!(secret_store::load_secret_if_present(TOKEN_ACCOUNT), Ok(None));

        // The token is already gone, and the retry must not mind.
        write_claude_settings(&good);
        assert!(clear_custom_gateway().is_ok());
        assert!(!claude_settings_text().contains("ANTHROPIC"));
    }

    #[test]
    fn an_erase_says_when_the_gateway_record_is_damaged() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("plain-token");
        let file = gateway_file_path().expect("path");
        fs::write(&file, "{not json").expect("damage the file");

        let err = crate::core::erase_app_data().expect_err("stopped");

        assert!(err.contains("custom-gateway.json"), "{err}");
        assert!(err.contains("not erased"), "{err}");
        assert!(err.contains("token was deleted"), "{err}");
        assert!(err.contains("may remain"), "{err}");
        assert!(err.contains(".claude/settings.json"), "{err}");
        assert!(!err.contains("remove that file"), "{err}");
        assert!(file.exists());
    }

    #[test]
    fn an_erase_goes_ahead_on_a_damaged_settings_file() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("plain-token");
        let file = crate::core::get_automatic_dir().expect("dir").join("settings.json");
        fs::write(&file, "{not json").expect("damage the file");

        crate::core::erase_app_data().expect("erase");

        assert!(!gateway_file_path().expect("path").exists());
        assert!(!claude_settings_text().contains("ANTHROPIC"));
    }

    #[test]
    fn an_erase_reports_what_stayed_in_claude_code() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("plain-token");
        write_claude_settings(&claude_settings_text().replace("plain-token", "theirs"));

        let cleared = crate::core::erase_app_data().expect("erase");

        assert_eq!(cleared.left_in_claude_code, ["ANTHROPIC_AUTH_TOKEN"]);
        assert_eq!(secret_store::load_secret_if_present(TOKEN_ACCOUNT), Ok(None));
        assert!(!custom_gateway_status().expect("status").configured);
    }

    #[test]
    fn an_erase_with_a_keychain_that_refuses_says_so_after_erasing() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("plain-token");

        set_test_fault(TOKEN_ACCOUNT, Some(TestFault::DeleteRefused));
        let erased = crate::core::erase_app_data();
        set_test_fault(TOKEN_ACCOUNT, None);

        let err = erased.expect_err("the token stayed");
        assert!(err.contains("was erased"), "{err}");
        assert!(!custom_gateway_status().expect("status").configured);
        remove_token();
    }

    // ── A lost record ────────────────────────────────────────────────────────

    #[test]
    fn a_lost_record_with_a_stored_token_shows_in_the_status() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("plain-token");
        lose_gateway_record();

        let status = custom_gateway_status().expect("status");

        assert!(!status.configured);
        assert!(status.orphaned_token);
        assert!(refuse_if_token_is_orphaned().is_err());
        remove_token();
    }

    #[test]
    fn a_keychain_that_cannot_be_read_fails_the_orphan_check_closed() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();

        set_test_fault(TOKEN_ACCOUNT, Some(TestFault::Unreadable));
        let checked = refuse_if_token_is_orphaned();
        set_test_fault(TOKEN_ACCOUNT, None);

        assert!(checked.expect_err("closed").contains("was not sent"));
    }

    #[test]
    fn a_clear_after_a_lost_record_removes_exact_matches_and_the_token() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("plain-token");
        lose_gateway_record();

        let cleared = clear_custom_gateway().expect("clear");

        assert!(cleared.left_in_claude_code.is_empty(), "{cleared:?}");
        assert!(!claude_settings_text().contains("ANTHROPIC"));
        assert_eq!(secret_store::load_secret_if_present(TOKEN_ACCOUNT), Ok(None));
        assert!(!custom_gateway_status().expect("status").orphaned_token);
    }

    #[test]
    fn a_clear_after_a_lost_record_leaves_and_reports_a_value_that_differs() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("plain-token");
        lose_gateway_record();
        write_claude_settings(&claude_settings_text().replace("plain-token", "theirs"));

        let cleared = clear_custom_gateway().expect("clear");

        assert_eq!(cleared.left_in_claude_code, ["ANTHROPIC_AUTH_TOKEN"]);
        assert!(claude_settings_text().contains("theirs"));
        assert!(!claude_settings_text().contains("ANTHROPIC_BASE_URL"));
    }

    #[test]
    fn a_clear_that_cannot_read_the_keychain_is_an_error() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();

        set_test_fault(TOKEN_ACCOUNT, Some(TestFault::Unreadable));
        let cleared = clear_custom_gateway();
        set_test_fault(TOKEN_ACCOUNT, None);

        assert!(cleared.is_err());
    }

    #[test]
    fn a_stored_value_that_cannot_be_parsed_can_still_be_cleared() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        secret_store::store_secret(TOKEN_ACCOUNT, "not a binding").expect("store");
        write_claude_settings(r#"{"env":{"ANTHROPIC_AUTH_TOKEN":"theirs"}}"#);

        let cleared = clear_custom_gateway().expect("clear");

        assert_eq!(cleared.left_in_claude_code, ["ANTHROPIC_AUTH_TOKEN"]);
        assert_eq!(secret_store::load_secret_if_present(TOKEN_ACCOUNT), Ok(None));
    }

    #[test]
    fn an_erase_after_a_lost_record_cleans_up_too() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("plain-token");
        lose_gateway_record();

        crate::core::erase_app_data().expect("erase");

        assert!(!claude_settings_text().contains("plain-token"));
        assert_eq!(secret_store::load_secret_if_present(TOKEN_ACCOUNT), Ok(None));
    }

    #[test]
    fn a_clear_of_a_record_with_no_write_history_reports_what_is_set() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("plain-token");
        let mut gateway = read_gateway().expect("read").expect("gateway");
        gateway.claude_code = None;
        write_gateway(&gateway).expect("write");

        let cleared = clear_custom_gateway().expect("clear");

        assert_eq!(
            cleared.left_in_claude_code,
            ["ANTHROPIC_BASE_URL", "ANTHROPIC_AUTH_TOKEN"]
        );
    }

    #[test]
    fn a_failed_rebind_retry_does_not_adopt_automatics_url() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("plain-token");
        let moved = "https://gateway.example.com/team";

        set_test_fault(TOKEN_ACCOUNT, Some(TestFault::StoreRefused));
        let first = save_custom_gateway(moved, BEARER, None);
        set_test_fault(TOKEN_ACCOUNT, None);
        assert!(first.is_err());
        assert!(claude_settings_text().contains(moved));

        save_custom_gateway(moved, BEARER, None).expect("retry");

        let cleared = clear_custom_gateway().expect("clear");
        assert!(cleared.left_in_claude_code.is_empty(), "{cleared:?}");
    }

    #[test]
    fn an_erase_with_a_damaged_settings_file_and_no_gateway_works() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        let dir = crate::core::get_automatic_dir().expect("dir");
        fs::create_dir_all(&dir).expect("mkdir");
        fs::write(dir.join("settings.json"), "{not json").expect("damage the file");

        crate::core::erase_app_data().expect("erase");

        assert!(serde_json::from_str::<serde_json::Value>(
            &fs::read_to_string(dir.join("settings.json")).expect("read")
        )
        .is_ok());
    }

    #[test]
    fn a_clear_with_nothing_saved_does_nothing() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();

        assert_eq!(clear_custom_gateway().map(|c| c.left_in_claude_code), Ok(Vec::new()));
        assert!(!claude_code_env::settings_path().expect("path").exists());
    }

    #[test]
    fn a_malformed_claude_code_file_stops_a_clear_so_it_can_be_retried() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("plain-token");
        let good = claude_settings_text();
        write_claude_settings("{not json");

        assert!(clear_custom_gateway().is_err());
        assert!(custom_gateway_status().expect("status").configured);

        write_claude_settings(&good);
        assert!(clear_custom_gateway().is_ok());
    }

    // ── Who can change or read it ────────────────────────────────────────────

    #[test]
    fn a_damaged_gateway_file_stops_a_save_and_a_clear_and_changes_nothing() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("plain-token");
        let file = gateway_file_path().expect("path");
        fs::write(&file, "{not json").expect("damage the file");
        let claude_code = claude_settings_text();

        assert!(clear_custom_gateway().is_err());
        assert!(save_custom_gateway(URL, BEARER, Some("other")).is_err());
        assert!(custom_gateway_status().is_err());

        assert_eq!(fs::read_to_string(&file).expect("read"), "{not json");
        assert_eq!(claude_settings_text(), claude_code);
        assert_eq!(stored_token(URL).as_deref(), Ok("plain-token"));
        remove_token();
    }

    #[test]
    fn a_damaged_settings_file_leaves_the_gateway_saved_and_clearable() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("plain-token");
        let file = crate::core::get_automatic_dir().expect("dir").join("settings.json");
        fs::write(&file, "{not json").expect("damage the file");

        crate::core::reset_settings().expect("reset");

        assert!(custom_gateway_status().expect("status").configured);
        assert!(clear_custom_gateway().is_ok());
    }

    #[test]
    fn the_record_is_not_in_general_settings() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("plain-token");

        let settings = serde_json::to_string(&read_settings().expect("read")).expect("json");

        assert!(!settings.contains("token_fingerprint"), "{settings}");
        assert!(!settings.contains("custom_gateway"), "{settings}");
        remove_token();
    }

    #[test]
    fn resetting_settings_keeps_the_gateway_record() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("plain-token");

        crate::core::reset_settings().expect("reset");

        assert!(custom_gateway_status().expect("status").configured);
        remove_token();
    }

    #[test]
    fn the_token_account_is_refused_by_the_credential_checks() {
        assert!(crate::core::agents::check_credential_provider(TOKEN_ACCOUNT).is_err());
    }
}
