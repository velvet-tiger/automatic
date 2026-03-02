use serde::{Deserialize, Serialize};

// ── Types ─────────────────────────────────────────────────────────────────────

/// A single message in a conversation.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AiMessage {
    pub role: String,
    pub content: String,
}

/// Request payload sent to the Anthropic Messages API.
#[derive(Debug, Serialize)]
struct AnthropicRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    messages: &'a [AiMessage],
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<&'a str>,
}

/// The text content block returned by Anthropic.
#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

/// Top-level response from the Anthropic Messages API.
#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<ContentBlock>,
}

/// Error body returned by Anthropic (best-effort parse).
#[derive(Debug, Deserialize)]
struct AnthropicError {
    error: AnthropicErrorDetail,
}

#[derive(Debug, Deserialize)]
struct AnthropicErrorDetail {
    message: String,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Resolve the API key to use for a request.
///
/// Priority order:
///   1. `VITE_ANTHROPIC_API_KEY` env var (dev mode only, compiled in via Vite).
///      In dev builds the frontend embeds the key from `.env`; here we read it
///      from the *process* environment so it works for backend-initiated calls.
///   2. Key stored in the OS keychain under provider `"anthropic"`.
///   3. The `api_key` argument (explicitly supplied by caller).
///
/// Returns an error if no key is found.
pub fn resolve_api_key(explicit_key: Option<&str>) -> Result<String, String> {
    // 1. Process environment (set in the shell before launching `tauri dev`).
    if let Ok(k) = std::env::var("ANTHROPIC_API_KEY") {
        if !k.is_empty() {
            return Ok(k);
        }
    }

    // 2. In debug builds, parse the .env file at the workspace root.
    //    Vite reads .env for the JS bundle but does NOT inject vars into the
    //    Rust process, so we do it ourselves here.
    #[cfg(debug_assertions)]
    {
        // CARGO_MANIFEST_DIR is src-tauri/; the .env lives one level up.
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let env_path = manifest_dir.parent().unwrap_or(manifest_dir).join(".env");
        if let Ok(contents) = std::fs::read_to_string(&env_path) {
            for line in contents.lines() {
                let line = line.trim();
                if line.starts_with('#') || line.is_empty() {
                    continue;
                }
                if let Some(rest) = line.strip_prefix("ANTHROPIC_API_KEY=") {
                    let val = rest.trim().trim_matches('"').trim_matches('\'');
                    if !val.is_empty() {
                        return Ok(val.to_string());
                    }
                }
            }
        }
    }

    // 3. OS keychain (saved via Settings or the playground key panel).
    if let Ok(k) = super::credentials::get_api_key("anthropic") {
        if !k.is_empty() {
            return Ok(k);
        }
    }

    // 4. Explicit argument passed by the caller.
    if let Some(k) = explicit_key {
        if !k.is_empty() {
            return Ok(k.to_string());
        }
    }

    Err("No Anthropic API key found. Set ANTHROPIC_API_KEY in .env or save one via Settings.".to_string())
}

/// Send a list of messages to the Anthropic Messages API and return the
/// assistant's text response.
///
/// - `api_key`: If `None`, `resolve_api_key` is called to find one.
/// - `model`: Defaults to `"claude-sonnet-4-5"` if `None`.
/// - `system`: Optional system prompt prepended to the conversation.
/// - `max_tokens`: Defaults to `4096`.
pub async fn chat(
    messages: Vec<AiMessage>,
    api_key: Option<String>,
    model: Option<String>,
    system: Option<String>,
    max_tokens: Option<u32>,
) -> Result<String, String> {
    let key = resolve_api_key(api_key.as_deref())?;
    let model_str = model.as_deref().unwrap_or("claude-sonnet-4-5");
    let tokens = max_tokens.unwrap_or(4096);

    let body = AnthropicRequest {
        model: model_str,
        max_tokens: tokens,
        messages: &messages,
        system: system.as_deref(),
    };

    let client = reqwest::Client::new();
    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    let status = response.status();
    let body_text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    if !status.is_success() {
        // Try to extract a human-readable error message from Anthropic's JSON error body.
        let msg = serde_json::from_str::<AnthropicError>(&body_text)
            .map(|e| e.error.message)
            .unwrap_or_else(|_| body_text.clone());
        return Err(format!("Anthropic API error {}: {}", status, msg));
    }

    let parsed: AnthropicResponse = serde_json::from_str(&body_text)
        .map_err(|e| format!("Failed to parse Anthropic response: {}", e))?;

    // Extract the first text content block.
    parsed
        .content
        .into_iter()
        .find(|b| b.kind == "text")
        .and_then(|b| b.text)
        .ok_or_else(|| "Anthropic returned no text content".to_string())
}
