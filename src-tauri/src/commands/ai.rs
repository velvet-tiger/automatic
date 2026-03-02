use crate::core::ai::{self, AiMessage};

/// Send a chat message (or multi-turn conversation) to the Anthropic API.
///
/// The `api_key` parameter is optional; if omitted the backend resolves the
/// key from the environment or OS keychain automatically.
#[tauri::command]
pub async fn ai_chat(
    messages: Vec<AiMessage>,
    api_key: Option<String>,
    model: Option<String>,
    system: Option<String>,
    max_tokens: Option<u32>,
) -> Result<String, String> {
    ai::chat(messages, api_key, model, system, max_tokens).await
}


