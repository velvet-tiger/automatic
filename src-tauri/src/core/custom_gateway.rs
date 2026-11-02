//! The user's custom LLM gateway for Claude.
//!
//! One saved gateway drives two consumers: Automatic's Anthropic client and
//! Claude Code. The URL and credential kind live in settings. The token lives
//! in the secret store under a fixed account that is not a provider id, so the
//! credential commands and the `automatic_get_credential` MCP tool refuse it.
//! Claude Code only reads the token from its own settings file, so
//! [`claude_code_env`](super::claude_code_env) also writes it there.

use serde::{Deserialize, Serialize};
use std::sync::{Mutex, MutexGuard};

use super::agents::{CustomGatewayRoute, GatewayCredentialKind};
use super::claude_code_env::{self, ClaudeCodeRecord, GatewayEnv};
use super::credentials::{any_known_api_key_stored, set_agent_features_enabled};
use super::secret_store;
use super::settings::{read_settings, write_settings};
use super::store_reads::StoreRead;

/// The secret store account that holds the gateway token.
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
}

/// The outcome of a clear.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ClearedCustomGateway {
    /// Claude Code variables left in place because they were changed outside
    /// Automatic.
    pub left_in_claude_code: Vec<String>,
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
    Ok(url.as_str().trim_end_matches('/').to_string())
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

/// The gateway's public details. Reads no secret.
pub fn custom_gateway_status() -> Result<CustomGatewayStatus, String> {
    let gateway = read_settings()?.custom_gateway;
    Ok(CustomGatewayStatus {
        configured: gateway.is_some(),
        base_url: gateway.as_ref().map(|g| g.base_url.clone()),
        credential_kind: gateway.as_ref().map(|g| g.credential_kind),
    })
}

/// The stored token, with a message the user can act on when it is missing.
fn stored_token() -> Result<String, String> {
    secret_store::load_secret_if_present(TOKEN_ACCOUNT)
        .map_err(|e| format!("Could not read the gateway token: {}", e))?
        .filter(|token| !token.is_empty())
        .ok_or_else(|| {
            "The custom gateway token is missing. Enter it again in Settings > Agents.".to_string()
        })
}

/// The route for a saved gateway. Settings can be edited by hand, so the URL
/// is checked again here before a token is sent to it.
pub(crate) fn gateway_route(gateway: &CustomGateway) -> Result<CustomGatewayRoute, String> {
    Ok(CustomGatewayRoute {
        base_url: parse_gateway_url(&gateway.base_url)?,
        kind: gateway.credential_kind,
        token: stored_token()?,
    })
}

// ── Save ──────────────────────────────────────────────────────────────────────

/// Save the gateway for Automatic and Claude Code. A blank `token` keeps the
/// one already stored.
///
/// The steps run in an order that keeps a failure safe to retry: validate,
/// check Claude Code's file for conflicts, store the token, write that file,
/// then write Automatic's settings.
pub fn save_custom_gateway(
    base_url: &str,
    credential_kind: GatewayCredentialKind,
    token: Option<&str>,
) -> Result<(), String> {
    let _changing = change_lock();

    let base_url = parse_gateway_url(base_url)?;
    let previous = read_settings()?.custom_gateway;
    let new_token = match token.map(str::trim).filter(|t| !t.is_empty()) {
        Some(token) => Some(parse_token(token)?),
        None if previous.is_none() => return Err("Enter the gateway token.".to_string()),
        None => None,
    };
    let token = match &new_token {
        Some(token) => token.clone(),
        None => stored_token()?,
    };

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

    if new_token.is_some() {
        secret_store::store_secret(TOKEN_ACCOUNT, &token)
            .map_err(|e| format!("Could not store the gateway token: {}", e))?;
    }

    let written = claude_code_env::write_file(&path, &desired, record).map_err(|e| {
        if previous.is_none() {
            // The form still shows no gateway, so nothing could clear this.
            let _ = secret_store::delete_secret(TOKEN_ACCOUNT);
            return e;
        }
        if new_token.is_some() {
            return format!(
                "The new token is stored and Automatic now uses it, but Claude Code was not \
                 updated. Save again to finish. {}",
                e
            );
        }
        e
    })?;

    let mut settings = read_settings()?;
    settings.custom_gateway = Some(CustomGateway {
        base_url,
        credential_kind,
        claude_code: Some(written),
    });
    if first_credential {
        settings.agent_features_enabled = Some(true);
    }
    write_settings(&settings).map_err(|e| {
        format!(
            "Claude Code now uses this gateway, but Automatic could not record it. \
             Save again with the same token to finish. {}",
            e
        )
    })
}

// ── Clear ─────────────────────────────────────────────────────────────────────

/// Remove the gateway from Claude Code, the secret store and settings, in that
/// order. A failure leaves the settings entry in place, so a second clear can
/// finish the job.
pub fn clear_custom_gateway() -> Result<ClearedCustomGateway, String> {
    let _changing = change_lock();

    let Some(gateway) = read_settings()?.custom_gateway else {
        return Ok(ClearedCustomGateway {
            left_in_claude_code: Vec::new(),
        });
    };

    let left_in_claude_code = match &gateway.claude_code {
        Some(record) => claude_code_env::clear_file(&claude_code_env::settings_path()?, record)?,
        None => Vec::new(),
    };

    delete_stored_token().map_err(|e| {
        format!(
            "Claude Code's settings were updated, but the gateway token could not be removed \
             from the keychain. Clear again to finish. {}",
            e
        )
    })?;

    let mut settings = read_settings()?;
    settings.custom_gateway = None;
    write_settings(&settings).map_err(|e| {
        format!(
            "The gateway was removed from Claude Code and the keychain, but Automatic could \
             not update its settings. Clear again to finish. {}",
            e
        )
    })?;

    // The same rule as removing the last API key.
    if any_known_api_key_stored(StoreRead::Now) == Ok(false) {
        set_agent_features_enabled(false);
    }
    Ok(ClearedCustomGateway { left_in_claude_code })
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
    pub(crate) fn gateway_store_lock() -> MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
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
}

#[cfg(test)]
mod tests {
    use super::test_support::{gateway_store_lock, save_test_gateway};
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

        assert_eq!(secret_store::load_secret(TOKEN_ACCOUNT).as_deref(), Ok("plain-token"));
        assert!(claude_settings_text().contains("plain-token"));
        let settings_file = crate::core::get_automatic_dir().expect("dir").join("settings.json");
        let saved = fs::read_to_string(settings_file).expect("read settings");
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
                credential_kind: None
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
        save_custom_gateway("https://other.example.com", API_KEY, Some(""))
            .expect("keep the token");

        assert_eq!(secret_store::load_secret(TOKEN_ACCOUNT).as_deref(), Ok("plain-token"));
        let text = claude_settings_text();
        assert!(text.contains("https://other.example.com"), "{text}");
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
    fn a_retry_after_automatics_settings_failed_to_write_succeeds() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("plain-token");

        // Claude Code holds the values, and Automatic lost its record of them.
        let mut settings = read_settings().expect("read");
        settings.custom_gateway = None;
        write_settings(&settings).expect("write");

        save_test_gateway("plain-token");

        assert!(custom_gateway_status().expect("status").configured);
        remove_token();
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
    fn the_frontend_settings_write_cannot_change_the_gateway() {
        let _store = gateway_store_lock();
        let _home = temp_test_home();
        save_test_gateway("plain-token");
        let saved = read_settings().expect("read").custom_gateway;

        let mut stale = read_settings().expect("read");
        stale.custom_gateway = None;
        crate::core::write_settings_from_frontend(stale).expect("write");

        assert_eq!(read_settings().expect("read").custom_gateway, saved);
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
