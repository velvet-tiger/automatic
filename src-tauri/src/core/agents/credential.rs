use serde::{Deserialize, Serialize};

use crate::core::secret_store;

// ── Gateway config ────────────────────────────────────────────────────────────

/// Optional Cloudflare AI Gateway routing configuration.
///
/// When present, requests are sent to the Gateway URL instead of the provider's
/// direct endpoint. Supported for Anthropic and Workers AI agents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GatewayConfig {
    pub account_id: String,
    pub gateway_id: String,
    /// Optional CF token for the `cf-aig-authorization` header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cf_token: Option<String>,
}

impl GatewayConfig {
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

// ── Internal ──────────────────────────────────────────────────────────────────

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
}
