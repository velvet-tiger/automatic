pub mod client;
pub mod clients;
pub mod credential;
pub mod message;

pub use client::{AgentClient, AgentClientDyn, AgentTurnResponse};
pub use credential::{AgentCredential, GatewayConfig};
pub use message::{ContentBlock, NeutralMessage, ToolDef};

use clients::anthropic::AnthropicClient;

// ── AgentId ───────────────────────────────────────────────────────────────────

/// Opaque identifier for a model provider ("anthropic", "openai", etc.).
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct AgentId(pub String);

impl AgentId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for AgentId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ── Registry ──────────────────────────────────────────────────────────────────

/// All provider IDs that Automatic recognises for the in-app AI toggle and
/// credential auto-enable logic.  Subsequent PRs add entries here.
pub fn known_agents() -> Vec<AgentId> {
    vec![AgentId::new("anthropic")]
}

/// Construct the active agent's client using the supplied API key and optional
/// gateway config.
///
/// The active agent is determined by `Settings::active_agent`; when not set it
/// defaults to `"anthropic"`.  For PR 1 only Anthropic is implemented; this
/// function always returns an `AnthropicClient`.
pub fn active_client_with_key(
    api_key: impl Into<String>,
    gateway: Option<GatewayConfig>,
) -> Box<dyn AgentClientDyn> {
    // TODO(PR 2+): inspect active_agent setting and instantiate the correct client.
    Box::new(AnthropicClient::new(api_key, gateway))
}
