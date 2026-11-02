use serde::{Deserialize, Serialize};
use std::fmt;

use crate::core::secret_store;

// ── Gateway config ────────────────────────────────────────────────────────────

/// Optional Cloudflare AI Gateway routing configuration.
///
/// When present, requests are sent to the Gateway URL instead of the provider's
/// direct endpoint. Supported for Anthropic and Workers AI agents.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct GatewayConfig {
    pub account_id: String,
    pub gateway_id: String,
    /// CF token for the `cf-aig-authorization` header, for this request only.
    ///
    /// In `settings.json` this is a token from an older version that has not
    /// reached the secret store yet. Startup moves it. It stays serialised
    /// until then, because a settings write must not lose it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cf_token: Option<String>,
    /// Whether the secret store holds a token for this gateway. Lets a gateway
    /// with no token, and the settings page, skip the keychain.
    #[serde(default, skip_serializing_if = "is_false")]
    pub cf_token_stored: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Written by hand so that no log line can print the token.
impl fmt::Debug for GatewayConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GatewayConfig")
            .field("account_id", &self.account_id)
            .field("gateway_id", &self.gateway_id)
            .field("cf_token", &self.cf_token.as_ref().map(|_| "<redacted>"))
            .field("cf_token_stored", &self.cf_token_stored)
            .finish()
    }
}

impl GatewayConfig {
    /// This config with its token ready for a request.
    ///
    /// Reads the secret store only when a token is recorded there. A token
    /// that cannot be read is an error: a request sent without it would look
    /// like a gateway fault.
    pub fn with_stored_token(mut self, agent_id: &str) -> Result<Self, String> {
        if self.cf_token.is_some() || !self.cf_token_stored {
            return Ok(self);
        }
        match load_gateway_token(agent_id)? {
            Some(token) => {
                self.cf_token = Some(token);
                Ok(self)
            }
            None => Err(format!(
                "The Cloudflare AI Gateway token for '{}' is missing from the secret store. \
                 Enter it again in Settings > Agents.",
                agent_id
            )),
        }
    }

    /// Validate both IDs and construct the Cloudflare AI Gateway base URL for
    /// the given provider path segment.
    ///
    /// Returns `Err` if either ID contains characters outside `[A-Za-z0-9_-]`,
    /// which prevents path injection via user-supplied config values.
    pub fn gateway_url(&self, provider_path: &str) -> Result<String, String> {
        Self::validate_id(&self.account_id, "account_id")?;
        Self::validate_id(&self.gateway_id, "gateway_id")?;
        Ok(format!(
            "https://gateway.ai.cloudflare.com/v1/{}/{}/{}",
            self.account_id, self.gateway_id, provider_path
        ))
    }

    fn validate_id(id: &str, field: &str) -> Result<(), String> {
        if id.is_empty() {
            return Err(format!("GatewayConfig: {field} must not be empty"));
        }
        if !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(format!(
                "GatewayConfig: {field} contains invalid characters (only A-Z, a-z, 0-9, -, _ allowed)"
            ));
        }
        Ok(())
    }
}

// ── Custom gateway route ──────────────────────────────────────────────────────

/// Which header carries the custom gateway token.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GatewayCredentialKind {
    /// `Authorization: Bearer <token>`.
    Bearer,
    /// `x-api-key: <token>`.
    ApiKey,
}

/// Where Anthropic requests go when the user has set a custom LLM gateway.
///
/// The token is the only credential this route holds, so a stored Anthropic
/// key cannot reach the gateway.
#[derive(Clone, PartialEq, Eq)]
pub struct CustomGatewayRoute {
    /// Validated base URL, with no trailing slash and no `/v1`.
    pub base_url: String,
    pub kind: GatewayCredentialKind,
    pub token: String,
}

impl std::fmt::Debug for CustomGatewayRoute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomGatewayRoute")
            .field("base_url", &self.base_url)
            .field("kind", &self.kind)
            .field("token", &"[redacted]")
            .finish()
    }
}

// ── Agent credential ──────────────────────────────────────────────────────────

/// Credential shape for a provider.
///
/// Stored as a JSON blob in one OS keychain entry keyed by `agent_id`. Simple
/// providers only need a single token; compound credentials (Workers AI) carry
/// the account ID alongside the bearer token.
#[derive(Debug, Clone)]
pub enum AgentCredential {
    /// A single bearer token — used by Anthropic, OpenAI, GitHub Models, Z.ai,
    /// and OpenCode Zen.
    SingleToken(String),
    /// A token plus a provider account identifier — used by Cloudflare Workers AI.
    TokenAndAccount { token: String, account_id: String },
}

impl AgentCredential {
    /// The bearer token regardless of credential shape.
    pub fn token(&self) -> &str {
        match self {
            AgentCredential::SingleToken(t) => t,
            AgentCredential::TokenAndAccount { token, .. } => token,
        }
    }
}

// ── Keychain helpers ──────────────────────────────────────────────────────────

/// Load the credential for `agent_id` from the OS keychain.
///
/// Returns `Ok(None)` when no credential is stored. A keychain that cannot be
/// read is an error: the credential may be there.
pub fn load_credential(agent_id: &str) -> Result<Option<AgentCredential>, String> {
    Ok(read_keychain_token(agent_id)?.map(AgentCredential::SingleToken))
}

/// Store a `SingleToken` credential for `agent_id` in the OS keychain.
pub fn store_token(agent_id: &str, token: &str) -> Result<(), String> {
    secret_store::store_secret(agent_id, token)
}

/// Return `true` if a non-empty credential is stored for `agent_id`.
///
/// A keychain that cannot be read is an error, not `false`.
pub fn has_credential(agent_id: &str) -> Result<bool, String> {
    Ok(read_keychain_token(agent_id)?.is_some())
}

/// Remove the stored credential for `agent_id` from the OS keychain.
pub fn delete_credential(agent_id: &str) -> Result<(), String> {
    secret_store::delete_secret(agent_id)
}

/// Store the Cloudflare AI Gateway token for `agent_id`.
pub fn store_gateway_token(agent_id: &str, token: &str) -> Result<(), String> {
    secret_store::store_secret(&gateway_token_entry(agent_id), token)
}

/// The Cloudflare AI Gateway token for `agent_id`, or `None` when none is
/// stored. A keychain that cannot be read is an error.
pub fn load_gateway_token(agent_id: &str) -> Result<Option<String>, String> {
    read_keychain_token(&gateway_token_entry(agent_id))
}

/// Remove the Cloudflare AI Gateway token for `agent_id`. Removing a token
/// that is not there succeeds.
pub fn delete_gateway_token(agent_id: &str) -> Result<(), String> {
    let entry = gateway_token_entry(agent_id);
    if secret_store::load_secret_if_present(&entry)?.is_none() {
        return Ok(());
    }
    secret_store::delete_secret(&entry)
}

// ── Internal ──────────────────────────────────────────────────────────────────

/// The prefix keeps these entries apart from API keys, which are filed under
/// the bare agent id.
fn gateway_token_entry(agent_id: &str) -> String {
    format!("cf_gateway_token_{}", agent_id)
}

/// The token stored under `key`. An empty token counts as none.
fn read_keychain_token(key: &str) -> Result<Option<String>, String> {
    let token = secret_store::load_secret_if_present(key)
        .map_err(|e| format!("could not read the credential for '{}': {}", key, e))?;
    Ok(token.filter(|token| !token.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::secret_store::{set_test_fault, TestFault};

    // The test store is shared by the whole test process, so each test uses
    // an agent id no other test uses.

    #[test]
    fn a_stored_token_is_the_credential() {
        let agent = "agent-credential-test-stored";
        assert!(matches!(load_credential(agent), Ok(None)));
        assert_eq!(has_credential(agent), Ok(false));

        store_token(agent, "token").expect("store token");

        let credential = load_credential(agent).expect("read").expect("a credential");
        assert_eq!(credential.token(), "token");
        assert_eq!(has_credential(agent), Ok(true));
    }

    #[test]
    fn an_empty_token_is_no_credential() {
        let agent = "agent-credential-test-empty";
        store_token(agent, "").expect("store token");

        assert!(matches!(load_credential(agent), Ok(None)));
        assert_eq!(has_credential(agent), Ok(false));
    }

    #[test]
    fn a_keychain_that_cannot_be_read_is_an_error_not_a_missing_credential() {
        let agent = "agent-credential-test-unreadable";
        store_token(agent, "token").expect("store token");

        set_test_fault(agent, Some(TestFault::Unreadable));
        let loaded = load_credential(agent);
        let present = has_credential(agent);
        set_test_fault(agent, None);

        let err = loaded.expect_err("the keychain cannot be read");
        assert!(err.contains(agent), "unexpected error: {err}");
        assert!(present.is_err());
        assert_eq!(has_credential(agent), Ok(true));
    }

    fn gateway(cf_token: Option<&str>, cf_token_stored: bool) -> GatewayConfig {
        GatewayConfig {
            account_id: "account".to_string(),
            gateway_id: "gateway".to_string(),
            cf_token: cf_token.map(str::to_string),
            cf_token_stored,
        }
    }

    #[test]
    fn debug_output_never_shows_the_token() {
        let printed = format!("{:?}", gateway(Some("cf-secret-value"), false));
        assert!(!printed.contains("cf-secret-value"), "{printed}");
        assert!(printed.contains("account"), "{printed}");
    }

    #[test]
    fn a_gateway_token_is_kept_apart_from_the_api_key() {
        let agent = "gateway-token-test-apart";
        store_token(agent, "api-key").expect("store key");
        store_gateway_token(agent, "cf-token").expect("store token");

        assert_eq!(load_gateway_token(agent), Ok(Some("cf-token".to_string())));
        assert_eq!(load_credential(agent).unwrap().unwrap().token(), "api-key");

        delete_gateway_token(agent).expect("delete");
        delete_gateway_token(agent).expect("nothing left to delete");
        assert_eq!(load_gateway_token(agent), Ok(None));
    }

    #[test]
    fn a_stored_token_is_loaded_for_the_request() {
        let agent = "gateway-token-test-load";
        store_gateway_token(agent, "cf-token").expect("store");

        let ready = gateway(None, true).with_stored_token(agent).expect("loaded");

        assert_eq!(ready.cf_token.as_deref(), Some("cf-token"));
    }

    #[test]
    fn a_gateway_without_a_token_never_reads_the_store() {
        let agent = "gateway-token-test-unused";
        set_test_fault(&gateway_token_entry(agent), Some(TestFault::Unreadable));
        let ready = gateway(None, false).with_stored_token(agent);
        let old = gateway(Some("plaintext"), true).with_stored_token(agent);
        set_test_fault(&gateway_token_entry(agent), None);

        assert_eq!(ready.expect("no read").cf_token, None);
        assert_eq!(old.expect("no read").cf_token.as_deref(), Some("plaintext"));
    }

    #[test]
    fn an_unreadable_or_missing_token_stops_the_request() {
        let agent = "gateway-token-test-unreadable";
        let missing = gateway(None, true).with_stored_token(agent);
        assert!(missing.expect_err("missing").contains("Enter it again"));

        set_test_fault(&gateway_token_entry(agent), Some(TestFault::Unreadable));
        let unreadable = gateway(None, true).with_stored_token(agent);
        set_test_fault(&gateway_token_entry(agent), None);
        assert!(unreadable.is_err());
    }
}
