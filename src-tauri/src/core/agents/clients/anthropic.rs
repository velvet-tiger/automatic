use serde::Deserialize;
use serde_json::{json, Value};

use crate::core::agents::client::{AgentClient, AgentTurnResponse};
use crate::core::agents::credential::{CustomGatewayRoute, GatewayConfig, GatewayCredentialKind};
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

/// Where requests go, with the credential that belongs to that destination.
///
/// The Anthropic key lives only in `Direct`, so `Custom` has no way to send it.
#[derive(Debug, Clone)]
enum Route {
    Direct {
        api_key: String,
        gateway: Option<GatewayConfig>,
    },
    Custom(CustomGatewayRoute),
}

/// Anthropic Messages API client.
///
/// Handles authentication, optional AI Gateway routing, and serialisation
/// between the provider-neutral message types and the Anthropic wire format.
#[derive(Debug, Clone)]
pub struct AnthropicClient {
    route: Route,
}

impl AnthropicClient {
    pub fn new(api_key: impl Into<String>, gateway: Option<GatewayConfig>) -> Self {
        Self {
            route: Route::Direct {
                api_key: api_key.into(),
                gateway,
            },
        }
    }

    /// A client whose requests all go to the user's custom gateway.
    pub fn through_custom_gateway(route: CustomGatewayRoute) -> Self {
        Self {
            route: Route::Custom(route),
        }
    }

    fn base_url(&self) -> Result<String, String> {
        match &self.route {
            Route::Direct {
                gateway: Some(gw), ..
            } => gw.gateway_url("anthropic/v1"),
            Route::Direct { gateway: None, .. } => Ok("https://api.anthropic.com/v1".to_string()),
            Route::Custom(route) => Ok(format!("{}/v1", route.base_url)),
        }
    }

    fn client(&self) -> Result<reqwest::Client, String> {
        let builder = reqwest::Client::builder();
        // reqwest keeps `x-api-key` on a redirect to another host, so a gateway
        // could otherwise pass the token to any host it names.
        let builder = match self.route {
            Route::Custom(_) => builder.redirect(reqwest::redirect::Policy::none()),
            Route::Direct { .. } => builder,
        };
        builder
            .build()
            .map_err(|e| format!("Could not create the HTTP client: {}", e))
    }

    fn auth_headers(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let req = req
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json");

        match &self.route {
            Route::Direct { api_key, gateway } => {
                let req = req.header("x-api-key", api_key);
                match gateway.as_ref().and_then(|gw| gw.cf_token.as_ref()) {
                    Some(cf_token) => {
                        req.header("cf-aig-authorization", format!("Bearer {}", cf_token))
                    }
                    None => req,
                }
            }
            Route::Custom(route) => match route.kind {
                GatewayCredentialKind::Bearer => {
                    req.header("authorization", format!("Bearer {}", route.token))
                }
                GatewayCredentialKind::ApiKey => req.header("x-api-key", &route.token),
            },
        }
    }

    async fn send_messages_request(&self, body: &Value) -> Result<(u16, String), String> {
        let url = format!("{}/messages", self.base_url()?);
        let response = self
            .auth_headers(self.client()?.post(&url))
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
            .auth_headers(self.client()?.get(&url))
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

#[cfg(test)]
mod tests {
    use super::*;

    const STORED_KEY: &str = "sk-ant-stored-key";
    const TOKEN: &str = "gateway-token";

    fn cloudflare() -> GatewayConfig {
        GatewayConfig {
            account_id: "account".to_string(),
            gateway_id: "gateway".to_string(),
            cf_token: Some("cf-token".to_string()),
        }
    }

    fn custom(kind: GatewayCredentialKind) -> AnthropicClient {
        AnthropicClient::through_custom_gateway(CustomGatewayRoute {
            base_url: "https://gateway.example.com/llm".to_string(),
            kind,
            token: TOKEN.to_string(),
        })
    }

    /// The request this client would send, without sending it.
    fn built_request(client: &AnthropicClient) -> reqwest::Request {
        let url = format!("{}/messages", client.base_url().expect("base url"));
        client
            .auth_headers(client.client().expect("client").post(&url))
            .build()
            .expect("request")
    }

    fn header<'a>(request: &'a reqwest::Request, name: &str) -> Option<&'a str> {
        request.headers().get(name).map(|v| v.to_str().expect("text"))
    }

    fn all_header_values(request: &reqwest::Request) -> String {
        request
            .headers()
            .values()
            .map(|v| v.to_str().expect("text"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn a_direct_client_sends_the_key_to_anthropic() {
        let request = built_request(&AnthropicClient::new(STORED_KEY, None));

        assert_eq!(request.url().as_str(), "https://api.anthropic.com/v1/messages");
        assert_eq!(header(&request, "x-api-key"), Some(STORED_KEY));
        assert_eq!(header(&request, "authorization"), None);
    }

    #[test]
    fn cloudflare_routing_still_adds_its_header() {
        let request = built_request(&AnthropicClient::new(STORED_KEY, Some(cloudflare())));

        assert_eq!(
            request.url().as_str(),
            "https://gateway.ai.cloudflare.com/v1/account/gateway/anthropic/v1/messages"
        );
        assert_eq!(header(&request, "cf-aig-authorization"), Some("Bearer cf-token"));
    }

    #[test]
    fn a_bearer_gateway_gets_the_token_in_the_authorization_header() {
        let request = built_request(&custom(GatewayCredentialKind::Bearer));

        assert_eq!(request.url().as_str(), "https://gateway.example.com/llm/v1/messages");
        assert_eq!(header(&request, "authorization"), Some("Bearer gateway-token"));
        assert_eq!(header(&request, "x-api-key"), None);
        assert_eq!(header(&request, "anthropic-version"), Some("2023-06-01"));
    }

    #[test]
    fn an_api_key_gateway_gets_the_token_in_x_api_key() {
        let request = built_request(&custom(GatewayCredentialKind::ApiKey));

        assert_eq!(header(&request, "x-api-key"), Some(TOKEN));
        assert_eq!(header(&request, "authorization"), None);
    }

    #[test]
    fn a_gateway_request_carries_no_anthropic_key_or_cloudflare_header() {
        for kind in [GatewayCredentialKind::Bearer, GatewayCredentialKind::ApiKey] {
            let request = built_request(&custom(kind));

            assert!(!all_header_values(&request).contains(STORED_KEY));
            assert_eq!(header(&request, "cf-aig-authorization"), None);
        }
    }

    #[test]
    fn the_debug_form_does_not_show_the_gateway_token() {
        let shown = format!("{:?}", custom(GatewayCredentialKind::Bearer));

        assert!(!shown.contains(TOKEN), "{shown}");
    }
}
