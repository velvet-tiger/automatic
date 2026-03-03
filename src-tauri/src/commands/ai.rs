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

/// Send a chat message with the `read_file` tool available.
///
/// Runs an agentic loop: the model may call `read_file` one or more times
/// before producing a final text response.  All file access is sandboxed to
/// `working_dir`; paths that escape it are rejected.
#[tauri::command]
pub async fn ai_chat_with_tools(
    messages: Vec<AiMessage>,
    api_key: Option<String>,
    model: Option<String>,
    system: Option<String>,
    max_tokens: Option<u32>,
    working_dir: String,
) -> Result<String, String> {
    ai::chat_with_tools(messages, api_key, model, system, max_tokens, working_dir).await
}

