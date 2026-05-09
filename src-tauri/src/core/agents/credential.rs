use keyring::Entry;
use serde::{Deserialize, Serialize};

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
/// For the `"anthropic"` agent the lookup falls back to the legacy `"anthropic"`
/// keychain entry so existing users keep their stored key without re-entering it.
///
/// Returns `None` when no credential is stored.
pub fn load_credential(agent_id: &str) -> Option<AgentCredential> {
    // Attempt to read the canonical entry keyed by agent_id.
    if let Some(token) = read_keychain_token(agent_id) {
        return Some(AgentCredential::SingleToken(token));
    }
    None
}

/// Store a `SingleToken` credential for `agent_id` in the OS keychain.
pub fn store_token(agent_id: &str, token: &str) -> Result<(), String> {
    let entry =
        Entry::new(crate::core::KEYCHAIN_SERVICE, agent_id).map_err(|e| e.to_string())?;
    entry.set_password(token).map_err(|e| e.to_string())
}

/// Return `true` if a non-empty credential is stored for `agent_id`.
pub fn has_credential(agent_id: &str) -> bool {
    read_keychain_token(agent_id).is_some()
}

/// Remove the stored credential for `agent_id` from the OS keychain.
pub fn delete_credential(agent_id: &str) -> Result<(), String> {
    let entry =
        Entry::new(crate::core::KEYCHAIN_SERVICE, agent_id).map_err(|e| e.to_string())?;
    entry.delete_credential().map_err(|e| e.to_string())
}

// ── Internal ──────────────────────────────────────────────────────────────────

fn read_keychain_token(key: &str) -> Option<String> {
    let entry = Entry::new(crate::core::KEYCHAIN_SERVICE, key).ok()?;
    let pw = entry.get_password().ok()?;
    if pw.is_empty() { None } else { Some(pw) }
}
