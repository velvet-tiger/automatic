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
        AgentId::new("github-models"),
        AgentId::new("zai"),
        AgentId::new("opencode-zen"),
        AgentId::new("workers-ai"),
    ]
}

/// Default model ID for the given agent. Used when the caller does not supply
/// an explicit model and the facade needs a provider-appropriate value.
pub fn default_model(agent_id: &str) -> &'static str {
    match agent_id {
        "openai" => "gpt-5.4",
        "github-models" => "openai/gpt-4.1",
        "zai" => "glm-4.7",
        "opencode-zen" => "claude-sonnet-4-6",
        "workers-ai" => "@cf/meta/llama-3.1-8b-instruct",
        _ => "claude-sonnet-4-5",
    }
}

/// Curated list of OpenAI models shown in the model picker.
fn openai_static_models() -> Vec<String> {
    vec![
        "gpt-5.4".into(),
        "gpt-5.4-mini".into(),
        "gpt-5.4-nano".into(),
        "gpt-5.2".into(),
        "gpt-5.1".into(),
        "gpt-5".into(),
        "gpt-5-mini".into(),
        "gpt-4.1".into(),
        "gpt-4.1-mini".into(),
        "gpt-4o".into(),
        "gpt-4o-mini".into(),
        "o4-mini".into(),
        "o3".into(),
        "o3-mini".into(),
        "o1".into(),
    ]
}

/// Curated list of Cloudflare Workers AI models shown in the model picker.
///
/// Not every Workers AI model supports tool calling. The default
/// (`@cf/meta/llama-3.1-8b-instruct`) has been validated for the recommendations
/// tool-loop. Validate any new default before shipping.
fn workers_ai_static_models() -> Vec<String> {
    vec![
        "@cf/meta/llama-3.1-8b-instruct".into(),
        "@cf/meta/llama-3.3-70b-instruct-fp8-fast".into(),
        "@cf/google/gemma-3-12b-it".into(),
        "@cf/mistral/mistral-7b-instruct-v0.1".into(),
        "@cf/deepseek-ai/deepseek-r1-distill-qwen-32b".into(),
    ]
}

/// Curated list of Z.ai models shown in the model picker.
fn zai_static_models() -> Vec<String> {
    vec![
        "glm-4.7".into(),
        "glm-4-plus".into(),
        "glm-4-air".into(),
        "glm-4-flash".into(),
    ]
}

/// Curated model list for OpenCode Zen. Zen is an aggregator — it proxies
/// requests to Anthropic, OpenAI, Google, and others under a single key.
fn opencode_zen_static_models() -> Vec<String> {
    vec![
        "claude-sonnet-4-6".into(),
        "claude-opus-4-7".into(),
        "claude-haiku-4-5".into(),
        "gpt-5.5".into(),
        "gpt-5.4-mini".into(),
        "gemini-3.1-pro".into(),
        "gemini-3-flash".into(),
        "kimi-k2.6".into(),
        "qwen3.6-plus".into(),
    ]
}

/// Curated list of GitHub Models shown in the model picker.
fn github_models_static_models() -> Vec<String> {
    vec![
        "openai/gpt-4.1".into(),
        "openai/gpt-4o".into(),
        "openai/gpt-4o-mini".into(),
        "openai/o4-mini".into(),
        "openai/o3".into(),
        "meta/llama-3.3-70b-instruct".into(),
        "mistral-ai/mistral-large-2411".into(),
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
        "github-models" => Box::new(
            OpenAiCompatClient::new(
                api_key,
                "https://models.github.ai/inference",
                vec![("X-GitHub-Api-Version".to_string(), "2026-03-10".to_string())],
                gateway,
            )
            .with_static_models(github_models_static_models()),
        ),
        "zai" => Box::new(
            OpenAiCompatClient::new(api_key, "https://api.z.ai/api/paas/v4", vec![], gateway)
                .with_static_models(zai_static_models()),
        ),
        "opencode-zen" => Box::new(
            OpenAiCompatClient::new(api_key, "https://opencode.ai/zen/v1", vec![], gateway)
                .with_static_models(opencode_zen_static_models()),
        ),
        "workers-ai" => {
            // Credentials are stored as {"token":"...","account_id":"..."} by the frontend.
            let raw: String = api_key.into();
            let (token, account_id) = serde_json::from_str::<serde_json::Value>(&raw)
                .ok()
                .and_then(|v| {
                    let t = v.get("token")?.as_str()?.to_string();
                    let a = v.get("account_id")?.as_str()?.to_string();
                    Some((t, a))
                })
                .unwrap_or_else(|| (raw, String::new()));
            let base_url = format!(
                "https://api.cloudflare.com/client/v4/accounts/{}/ai/v1",
                account_id
            );
            Box::new(
                OpenAiCompatClient::new(token, base_url, vec![], gateway)
                    .with_static_models(workers_ai_static_models()),
            )
        }
        _ => Box::new(AnthropicClient::new(api_key, gateway)),
    }
}

/// Return the curated static model list for any agent without constructing a
/// client or requiring an API key. Used by the Settings > Agents model picker.
pub fn agent_static_models(agent_id: &str) -> Vec<String> {
    match agent_id {
        "openai" => openai_static_models(),
        "github-models" => github_models_static_models(),
        "zai" => zai_static_models(),
        "opencode-zen" => opencode_zen_static_models(),
        "workers-ai" => workers_ai_static_models(),
        _ => vec![
            "claude-opus-4-7".into(),
            "claude-sonnet-4-6".into(),
            "claude-sonnet-4-5".into(),
            "claude-haiku-4-5".into(),
        ],
    }
}
