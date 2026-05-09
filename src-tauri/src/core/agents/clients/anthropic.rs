use serde::Deserialize;
use serde_json::{json, Value};

use crate::core::agents::client::{AgentClient, AgentTurnResponse};
use crate::core::agents::credential::GatewayConfig;
use crate::core::agents::message::{
    anthropic_blocks_to_neutral, neutral_to_anthropic, tool_def_to_anthropic, NeutralMessage,
    ToolDef,
};

// ── Internal wire types ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AnthropicTextBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicSimpleResponse {
    content: Vec<AnthropicTextBlock>,
}

#[derive(Debug, Deserialize)]
struct AnthropicTurnResponse {
    stop_reason: Option<String>,
    content: Value,
}

#[derive(Debug, Deserialize)]
struct AnthropicError {
    error: AnthropicErrorDetail,
}

#[derive(Debug, Deserialize)]
struct AnthropicErrorDetail {
    message: String,
}

#[derive(Debug, Deserialize)]
struct ModelInfo {
    id: String,
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelInfo>,
}

// ── Client ────────────────────────────────────────────────────────────────────

/// Anthropic Messages API client.
///
/// Handles authentication, optional AI Gateway routing, and serialisation
/// between the provider-neutral message types and the Anthropic wire format.
#[derive(Debug, Clone)]
pub struct AnthropicClient {
    api_key: String,
    gateway: Option<GatewayConfig>,
}

impl AnthropicClient {
    pub fn new(api_key: impl Into<String>, gateway: Option<GatewayConfig>) -> Self {
        Self {
            api_key: api_key.into(),
            gateway,
        }
    }

    fn base_url(&self) -> Result<String, String> {
        if let Some(gw) = &self.gateway {
            gw.gateway_url("anthropic/v1")
        } else {
            Ok("https://api.anthropic.com/v1".to_string())
        }
    }

    fn client(&self) -> reqwest::Client {
        reqwest::Client::new()
    }

    fn auth_headers(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let req = req
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json");

        if let Some(gw) = &self.gateway {
            if let Some(cf_token) = &gw.cf_token {
                return req.header("cf-aig-authorization", format!("Bearer {}", cf_token));
            }
        }
        req
    }

    async fn send_messages_request(&self, body: &Value) -> Result<(u16, String), String> {
        let url = format!("{}/messages", self.base_url()?);
        let response = self
            .auth_headers(self.client().post(&url))
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
        serde_json::from_str::<AnthropicError>(body)
            .map(|e| e.error.message)
            .unwrap_or_else(|_| body.to_string());
        // Return a composed message so callers get full context.
        let detail = serde_json::from_str::<AnthropicError>(body)
            .map(|e| e.error.message)
            .unwrap_or_else(|_| body.to_string());
        format!("Anthropic API error {}: {}", status, detail)
    }
}

// ── AgentClient implementation ────────────────────────────────────────────────

impl AgentClient for AnthropicClient {
    async fn chat(
        &self,
        messages: &[NeutralMessage],
        model: Option<&str>,
        system: Option<&str>,
        max_tokens: Option<u32>,
    ) -> Result<String, String> {
        let model_str = model.unwrap_or("claude-sonnet-4-5");
        let tokens = max_tokens.unwrap_or(4096);

        let wire_messages: Vec<Value> = messages.iter().map(neutral_to_anthropic).collect();

        let mut body = json!({
            "model": model_str,
            "max_tokens": tokens,
            "messages": wire_messages,
        });
        if let Some(sys) = system {
            body["system"] = json!(sys);
        }

        let (status, body_text) = self.send_messages_request(&body).await?;
        if !(200..300).contains(&status) {
            return Err(Self::parse_error(&body_text, status));
        }

        let parsed: AnthropicSimpleResponse = serde_json::from_str(&body_text)
            .map_err(|e| format!("Failed to parse Anthropic response: {}", e))?;

        parsed
            .content
            .into_iter()
            .find(|b| b.kind == "text")
            .and_then(|b| b.text)
            .ok_or_else(|| "Anthropic returned no text content".to_string())
    }

    async fn chat_structured(
        &self,
        messages: &[NeutralMessage],
        model: Option<&str>,
        system: Option<&str>,
        max_tokens: Option<u32>,
        schema: &Value,
    ) -> Result<String, String> {
        let model_str = model.unwrap_or("claude-sonnet-4-5");
        let tokens = max_tokens.unwrap_or(8192);

        let wire_messages: Vec<Value> = messages.iter().map(neutral_to_anthropic).collect();

        let mut body = json!({
            "model": model_str,
            "max_tokens": tokens,
            "messages": wire_messages,
            "output_config": {
                "format": {
                    "type": "json_schema",
                    "schema": schema
                }
            }
        });
        if let Some(sys) = system {
            body["system"] = json!(sys);
        }

        let (status, body_text) = self.send_messages_request(&body).await?;
        if !(200..300).contains(&status) {
            return Err(Self::parse_error(&body_text, status));
        }

        let parsed: AnthropicSimpleResponse = serde_json::from_str(&body_text)
            .map_err(|e| format!("Failed to parse Anthropic response: {}", e))?;

        parsed
            .content
            .into_iter()
            .find(|b| b.kind == "text")
            .and_then(|b| b.text)
            .ok_or_else(|| "Anthropic returned no text content".to_string())
    }

    async fn send_agentic_turn(
        &self,
        messages: &[NeutralMessage],
        tool_defs: &[ToolDef],
        model: Option<&str>,
        system: Option<&str>,
        max_tokens: Option<u32>,
    ) -> Result<AgentTurnResponse, String> {
        let model_str = model.unwrap_or("claude-sonnet-4-5");
        let tokens = max_tokens.unwrap_or(4096);

        let wire_messages: Vec<Value> = messages.iter().map(neutral_to_anthropic).collect();
        let wire_tools: Vec<Value> = tool_defs.iter().map(tool_def_to_anthropic).collect();

        let mut body = json!({
            "model": model_str,
            "max_tokens": tokens,
            "tools": wire_tools,
            "messages": wire_messages,
        });
        if let Some(sys) = system {
            body["system"] = json!(sys);
        }

        let (status, body_text) = self.send_messages_request(&body).await?;
        if !(200..300).contains(&status) {
            return Err(Self::parse_error(&body_text, status));
        }

        let parsed: AnthropicTurnResponse = serde_json::from_str(&body_text)
            .map_err(|e| format!("Failed to parse Anthropic response: {}", e))?;

        // Anthropic uses "end_turn" for completion and "tool_use" when it wants
        // to invoke a tool. We surface these strings unchanged; the loop in
        // core::ai normalises them.
        let stop_reason = parsed.stop_reason.unwrap_or_else(|| "end_turn".to_string());
        let content_blocks = anthropic_blocks_to_neutral(&parsed.content);

        Ok(AgentTurnResponse {
            stop_reason,
            content_blocks,
        })
    }

    async fn list_models(&self) -> Result<Vec<String>, String> {
        let url = format!("{}/models", self.base_url()?);
        let response = self
            .auth_headers(self.client().get(&url))
            .query(&[("limit", "100")])
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        let status = response.status().as_u16();
        let body_text = response
            .text()
            .await
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        if !(200..300).contains(&status) {
            return Err(Self::parse_error(&body_text, status));
        }

        let parsed: ModelsResponse = serde_json::from_str(&body_text)
            .map_err(|e| format!("Failed to parse models response: {}", e))?;

        Ok(parsed.data.into_iter().map(|m| m.id).collect())
    }
}
