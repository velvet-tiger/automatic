use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

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
#[derive(Clone)]
enum Route {
    Direct {
        api_key: String,
        gateway: Option<GatewayConfig>,
    },
    Custom(CustomGatewayRoute),
}

impl std::fmt::Debug for Route {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Route::Direct { gateway, .. } => f
                .debug_struct("Direct")
                .field("api_key", &"[redacted]")
                .field("cloudflare_gateway", &gateway.is_some())
                .finish(),
            Route::Custom(route) => f.debug_tuple("Custom").field(route).finish(),
        }
    }
}

/// The longest wait for a gateway. A non-streaming reply can take minutes, but
/// a gateway that stops answering must not hang the feature for ever.
const CUSTOM_GATEWAY_TIMEOUT: Duration = Duration::from_secs(600);

/// How much of a gateway's error body is shown. A gateway is not Anthropic, so
/// its body can be anything, such as a whole HTML page.
const MAX_GATEWAY_ERROR_CHARS: usize = 500;

/// The largest reply read from a gateway. Anthropic is trusted not to send
/// gigabytes. A gateway is whatever the user pointed at.
const MAX_GATEWAY_BODY_BYTES: usize = 32 * 1024 * 1024;

/// Read a response body, stopping at `limit` bytes.
async fn read_capped(mut response: reqwest::Response, limit: usize) -> Result<String, String> {
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?
    {
        if body.len() + chunk.len() > limit {
            return Err(format!(
                "The custom gateway's reply is larger than the {} byte limit.",
                limit
            ));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&body).into_owned())
}

/// Whether a gateway URL names this machine.
fn is_loopback(base_url: &str) -> bool {
    url::Url::parse(base_url).is_ok_and(|url| {
        matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
    })
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
        let builder = match &self.route {
            Route::Direct { .. } => builder,
            Route::Custom(route) => {
                // reqwest keeps `x-api-key` on a redirect to another host, so
                // a gateway could otherwise pass the token to any host it names.
                let builder = builder
                    .redirect(reqwest::redirect::Policy::none())
                    .timeout(CUSTOM_GATEWAY_TIMEOUT);
                // A system proxy must not see a token meant for this machine.
                if is_loopback(&route.base_url) {
                    builder.no_proxy()
                } else {
                    builder
                }
            }
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
        Ok((status, self.read_body(response).await?))
    }

    async fn read_body(&self, response: reqwest::Response) -> Result<String, String> {
        match self.route {
            Route::Custom(_) => read_capped(response, MAX_GATEWAY_BODY_BYTES).await,
            Route::Direct { .. } => response
                .text()
                .await
                .map_err(|e| format!("Failed to read response body: {}", e)),
        }
    }

    /// The error for a response outside the 2xx range.
    fn response_error(&self, body: &str, status: u16) -> String {
        if !matches!(self.route, Route::Custom(_)) {
            return Self::parse_error(body, status);
        }
        if (300..400).contains(&status) {
            return format!(
                "The custom gateway answered with a redirect (status {}), which Automatic does \
                 not follow. Set the gateway's final URL in Settings > Agents.",
                status
            );
        }
        let detail = serde_json::from_str::<AnthropicError>(body)
            .map(|e| e.error.message)
            .unwrap_or_else(|_| body.to_string());
        let shown: String = detail.chars().take(MAX_GATEWAY_ERROR_CHARS).collect();
        let cut = if shown.len() < detail.len() { "..." } else { "" };
        format!("Custom gateway error {}: {}{}", status, shown, cut)
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
            return Err(self.response_error(&body_text, status));
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
            return Err(self.response_error(&body_text, status));
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
            return Err(self.response_error(&body_text, status));
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
        let body_text = self.read_body(response).await?;

        if !(200..300).contains(&status) {
            return Err(self.response_error(&body_text, status));
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
            cf_token_stored: false,
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
    fn the_debug_form_shows_no_credential() {
        let gateway = format!("{:?}", custom(GatewayCredentialKind::Bearer));
        assert!(!gateway.contains(TOKEN), "{gateway}");

        let direct = format!("{:?}", AnthropicClient::new(STORED_KEY, Some(cloudflare())));
        assert!(!direct.contains(STORED_KEY), "{direct}");
        assert!(!direct.contains("cf-token"), "{direct}");
    }

    #[test]
    fn only_this_machine_counts_as_loopback() {
        for url in ["http://localhost:4000", "http://127.0.0.1:4000", "http://[::1]:4000"] {
            assert!(is_loopback(url), "{url}");
        }
        assert!(!is_loopback("https://gateway.example.com"));
    }

    #[test]
    fn a_long_gateway_error_body_is_cut() {
        let client = custom(GatewayCredentialKind::Bearer);
        let body = "x".repeat(MAX_GATEWAY_ERROR_CHARS * 2);

        let error = client.response_error(&body, 500);

        assert!(error.starts_with("Custom gateway error 500"), "{error}");
        assert!(error.len() < MAX_GATEWAY_ERROR_CHARS + 100, "{error}");
        assert!(error.ends_with("..."), "{error}");
    }

    /// Serve one canned response on a local port. Returns the address and the
    /// channel that receives the request text.
    fn one_shot_server(response: String) -> (String, std::sync::mpsc::Receiver<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = format!("http://{}", listener.local_addr().expect("address"));
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buffer = [0u8; 4096];
            let read = stream.read(&mut buffer).expect("read");
            let _ = sender.send(String::from_utf8_lossy(&buffer[..read]).to_string());
            stream.write_all(response.as_bytes()).expect("write");
        });
        (address, receiver)
    }

    #[test]
    fn a_reply_over_the_limit_is_refused() {
        let body = "x".repeat(100);
        let (address, _requests) = one_shot_server(format!(
            "HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n{body}"
        ));

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let (over, reply) = runtime.block_on(async {
            let response = reqwest::get(&address).await.expect("response");
            let (again, _requests) = one_shot_server(format!(
                "HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n{body}"
            ));
            let fits = reqwest::get(&again).await.expect("response");
            (read_capped(response, 99).await, read_capped(fits, 100).await)
        });

        assert!(over.expect_err("too big").contains("limit"));
        assert_eq!(reply.expect("fits").len(), 100);
    }

    #[test]
    fn a_redirect_is_reported_and_the_token_does_not_follow_it() {
        let (second, second_requests) = one_shot_server(String::new());
        let (first, first_requests) = one_shot_server(format!(
            "HTTP/1.1 302 Found\r\nLocation: {second}/v1/models\r\nContent-Length: 0\r\n\r\n"
        ));
        let client = AnthropicClient::through_custom_gateway(CustomGatewayRoute {
            base_url: first,
            kind: GatewayCredentialKind::ApiKey,
            token: TOKEN.to_string(),
        });

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let error = runtime.block_on(client.list_models()).expect_err("a redirect");

        assert!(error.contains("redirect"), "{error}");
        assert!(error.contains("302"), "{error}");
        let seen = first_requests.recv().expect("the gateway got the request");
        assert!(seen.to_lowercase().contains("x-api-key: gateway-token"), "{seen}");
        assert!(second_requests.try_recv().is_err(), "the redirect was followed");
    }
}
