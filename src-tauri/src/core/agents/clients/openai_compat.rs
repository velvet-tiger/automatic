use serde_json::Value;

use crate::core::agents::client::{AgentClient, AgentTurnResponse};
use crate::core::agents::credential::GatewayConfig;
use crate::core::agents::message::{
    neutral_messages_to_openai, openai_response_to_neutral_blocks, tool_def_to_openai,
    NeutralMessage, ToolDef,
};

// ── Client ────────────────────────────────────────────────────────────────────

/// OpenAI-compatible Chat Completions API client.
///
/// Used for providers that implement the `/v1/chat/completions` interface:
/// OpenAI, GitHub Models, Z.ai, OpenCode Zen, Cloudflare Workers AI.
///
/// Callers must supply `base_url` (no trailing slash) and any required
/// extra headers (e.g. `X-GitHub-Api-Version` for GitHub Models).
///
/// This client is not yet wired to the agent registry and will be connected
/// in subsequent PRs (PR 2–6). The implementation stubs here ensure the
/// module compiles and that the architecture is in place.
#[derive(Debug, Clone)]
pub struct OpenAiCompatClient {
    api_key: String,
    base_url: String,
    extra_headers: Vec<(String, String)>,
    gateway: Option<GatewayConfig>,
}

impl OpenAiCompatClient {
    pub fn new(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
        extra_headers: Vec<(String, String)>,
        gateway: Option<GatewayConfig>,
    ) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: base_url.into(),
            extra_headers,
            gateway,
        }
    }

    #[allow(dead_code)]
    fn effective_base_url(&self, provider_segment: &str) -> String {
        if let Some(gw) = &self.gateway {
            format!(
                "https://gateway.ai.cloudflare.com/v1/{}/{}/{}",
                gw.account_id, gw.gateway_id, provider_segment
            )
        } else {
            self.base_url.clone()
        }
    }

    fn client(&self) -> reqwest::Client {
        reqwest::Client::new()
    }

    fn auth_request(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let mut req = req
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json");

        for (name, value) in &self.extra_headers {
            req = req.header(name, value);
        }

        if let Some(gw) = &self.gateway {
            if let Some(cf_token) = &gw.cf_token {
                req = req.header("cf-aig-authorization", format!("Bearer {}", cf_token));
            }
        }

        req
    }

    async fn send_chat_request(&self, body: &Value) -> Result<(u16, String), String> {
        let url = format!("{}/chat/completions", self.base_url);
        let response = self
            .auth_request(self.client().post(&url))
            .json(body)
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        let status = response.status().as_u16();
        let text = response
            .text()
            .await
            .map_err(|e| format!("Failed to read response body: {}", e))?;
        Ok((status, text))
    }

    fn parse_error(body: &str, status: u16) -> String {
        // OpenAI error shape: { "error": { "message": "..." } }
        let detail = serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|v| {
                v.get("error")?
                    .get("message")?
                    .as_str()
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| body.to_string());
        format!("API error {}: {}", status, detail)
    }

    fn extract_text_from_response(body: &str) -> Result<String, String> {
        let parsed: serde_json::Value = serde_json::from_str(body)
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        parsed
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|msg| msg.get("content"))
            .and_then(|c| c.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "No text content in response".to_string())
    }
}

// ── AgentClient implementation ────────────────────────────────────────────────

impl AgentClient for OpenAiCompatClient {
    async fn chat(
        &self,
        messages: &[NeutralMessage],
        model: Option<&str>,
        system: Option<&str>,
        max_tokens: Option<u32>,
    ) -> Result<String, String> {
        let model_str = model.unwrap_or("gpt-4o-mini");
        let tokens = max_tokens.unwrap_or(4096);

        let mut wire_messages = neutral_messages_to_openai(messages);
        if let Some(sys) = system {
            wire_messages.insert(0, serde_json::json!({ "role": "system", "content": sys }));
        }

        let body = serde_json::json!({
            "model": model_str,
            "max_tokens": tokens,
            "messages": wire_messages,
        });

        let (status, body_text) = self.send_chat_request(&body).await?;
        if status < 200 || status >= 300 {
            return Err(Self::parse_error(&body_text, status));
        }

        Self::extract_text_from_response(&body_text)
    }

    async fn chat_structured(
        &self,
        messages: &[NeutralMessage],
        model: Option<&str>,
        system: Option<&str>,
        max_tokens: Option<u32>,
        schema: &Value,
    ) -> Result<String, String> {
        let model_str = model.unwrap_or("gpt-4o-mini");
        let tokens = max_tokens.unwrap_or(8192);

        let mut wire_messages = neutral_messages_to_openai(messages);
        if let Some(sys) = system {
            wire_messages.insert(0, serde_json::json!({ "role": "system", "content": sys }));
        }

        let body = serde_json::json!({
            "model": model_str,
            "max_tokens": tokens,
            "messages": wire_messages,
            "response_format": {
                "type": "json_schema",
                "json_schema": {
                    "name": "structured_output",
                    "strict": true,
                    "schema": schema
                }
            }
        });

        let (status, body_text) = self.send_chat_request(&body).await?;
        if status < 200 || status >= 300 {
            return Err(Self::parse_error(&body_text, status));
        }

        Self::extract_text_from_response(&body_text)
    }

    async fn send_agentic_turn(
        &self,
        messages: &[NeutralMessage],
        tool_defs: &[ToolDef],
        model: Option<&str>,
        system: Option<&str>,
        max_tokens: Option<u32>,
    ) -> Result<AgentTurnResponse, String> {
        let model_str = model.unwrap_or("gpt-4o-mini");
        let tokens = max_tokens.unwrap_or(4096);

        let mut wire_messages = neutral_messages_to_openai(messages);
        if let Some(sys) = system {
            wire_messages.insert(0, serde_json::json!({ "role": "system", "content": sys }));
        }

        let wire_tools: Vec<Value> = tool_defs.iter().map(tool_def_to_openai).collect();

        let body = serde_json::json!({
            "model": model_str,
            "max_tokens": tokens,
            "messages": wire_messages,
            "tools": wire_tools,
        });

        let (status, body_text) = self.send_chat_request(&body).await?;
        if status < 200 || status >= 300 {
            return Err(Self::parse_error(&body_text, status));
        }

        let parsed: serde_json::Value = serde_json::from_str(&body_text)
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        let choice = parsed
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .ok_or_else(|| "No choices in response".to_string())?;

        // OpenAI finish reasons: "stop", "tool_calls", "length", "content_filter"
        // Normalise to Anthropic naming for the loop: "end_turn" or "tool_use".
        let finish_reason = choice
            .get("finish_reason")
            .and_then(|v| v.as_str())
            .unwrap_or("stop");

        let stop_reason = match finish_reason {
            "tool_calls" => "tool_use".to_string(),
            _ => "end_turn".to_string(),
        };

        let message = choice.get("message").cloned().unwrap_or(serde_json::json!({}));
        let content_blocks = openai_response_to_neutral_blocks(&message);

        Ok(AgentTurnResponse {
            stop_reason,
            content_blocks,
        })
    }

    async fn list_models(&self) -> Result<Vec<String>, String> {
        // Not all OpenAI-compat providers expose a models endpoint.
        // Return an empty list; callers should use a curated static list
        // configured at registration time for providers that lack this API.
        Ok(vec![])
    }
}
