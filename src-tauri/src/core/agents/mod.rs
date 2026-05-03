pub mod client;
pub mod clients;
pub mod credential;
pub mod message;

pub use client::{AgentClient, AgentClientDyn, AgentTurnResponse};
pub use credential::{AgentCredential, GatewayConfig};
pub use message::{ContentBlock, NeutralMessage, ToolDef};

use clients::anthropic::AnthropicClient;
use clients::openai_compat::OpenAiCompatClient;

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
/// credential auto-enable logic.
pub fn known_agents() -> Vec<AgentId> {
    vec![
        AgentId::new("anthropic"),
        AgentId::new("openai"),
    ]
}

/// Default model ID for the given agent. Used when the caller does not supply
/// an explicit model and the facade needs a provider-appropriate value.
pub fn default_model(agent_id: &str) -> &'static str {
    match agent_id {
        "openai" => "gpt-4o-mini",
        _ => "claude-sonnet-4-5",
    }
}

/// Curated list of OpenAI models shown in the model picker.
fn openai_static_models() -> Vec<String> {
    vec![
        "gpt-4o".into(),
        "gpt-4o-mini".into(),
        "gpt-4-turbo".into(),
        "gpt-3.5-turbo".into(),
        "o4-mini".into(),
        "o3".into(),
    ]
}

/// Construct the active agent's client using the supplied agent ID, API key,
/// and optional gateway config.
///
/// The active agent ID is determined by `Settings::active_agent`; when not set
/// it defaults to `"anthropic"`. Each known ID maps to the appropriate client
/// implementation. Unknown IDs fall back to Anthropic.
pub fn active_client_with_key(
    agent_id: &str,
    api_key: impl Into<String>,
    gateway: Option<GatewayConfig>,
) -> Box<dyn AgentClientDyn> {
    match agent_id {
        "openai" => Box::new(
            OpenAiCompatClient::new(api_key, "https://api.openai.com/v1", vec![], gateway)
                .with_static_models(openai_static_models()),
        ),
        _ => Box::new(AnthropicClient::new(api_key, gateway)),
    }
}
