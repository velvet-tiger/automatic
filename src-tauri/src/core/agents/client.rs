use serde_json::Value;

use super::message::{ContentBlock, NeutralMessage, ToolDef};

// ── Turn response ─────────────────────────────────────────────────────────────

/// The provider's response for one turn in the agentic loop.
///
/// `stop_reason` uses Anthropic naming ("end_turn", "tool_use") — each client
/// normalises its provider's stop signal to one of these strings so that the
/// loop in `core::ai` does not need provider-specific branching.
#[derive(Debug, Clone)]
pub struct AgentTurnResponse {
    pub stop_reason: String,
    pub content_blocks: Vec<ContentBlock>,
}

// ── Client trait ──────────────────────────────────────────────────────────────

/// Abstraction over a single AI provider.
///
/// Each implementation handles credential storage, request serialisation, and
/// response parsing for its provider. The `core::ai` facade resolves credentials
/// once and passes the active client to all call sites.
///
/// Async fn in traits is stable since Rust 1.75.  The trait is not object-safe
/// out of the box with AFIT; callers that need dynamic dispatch should use
/// `Box<dyn AgentClientDyn>` via the provided blanket wrapper, or hold a
/// concrete type where possible.
pub trait AgentClient: Send + Sync {
    /// Send a list of text-only messages and return the assistant's text reply.
    ///
    /// Providers that require content blocks receive each message as a single
    /// `text` block. This method does not support tool use.
    fn chat<'a>(
        &'a self,
        messages: &'a [NeutralMessage],
        model: Option<&'a str>,
        system: Option<&'a str>,
        max_tokens: Option<u32>,
    ) -> impl std::future::Future<Output = Result<String, String>> + Send + 'a;

    /// Send messages with a JSON Schema and return the assistant's guaranteed-valid
    /// JSON reply. Not all providers support this natively; implementations that
    /// lack structured output fall back to plain chat with instruction injection.
    fn chat_structured<'a>(
        &'a self,
        messages: &'a [NeutralMessage],
        model: Option<&'a str>,
        system: Option<&'a str>,
        max_tokens: Option<u32>,
        schema: &'a Value,
    ) -> impl std::future::Future<Output = Result<String, String>> + Send + 'a;

    /// Execute one turn in an agentic tool-use loop.
    ///
    /// The caller (`core::ai`) maintains the message history, executes tool
    /// calls between turns, and calls this method again with the updated
    /// history. The client handles serialisation to and from the provider's
    /// wire format; it does not execute tools.
    ///
    /// `tool_defs` carries the provider-neutral tool definitions; the client
    /// converts them to its own wire format before sending.
    fn send_agentic_turn<'a>(
        &'a self,
        messages: &'a [NeutralMessage],
        tool_defs: &'a [ToolDef],
        model: Option<&'a str>,
        system: Option<&'a str>,
        max_tokens: Option<u32>,
    ) -> impl std::future::Future<Output = Result<AgentTurnResponse, String>> + Send + 'a;

    /// Return a list of model IDs available for this provider.
    ///
    /// Providers that do not expose a listing endpoint return a static curated
    /// list instead.
    fn list_models<'a>(
        &'a self,
    ) -> impl std::future::Future<Output = Result<Vec<String>, String>> + Send + 'a;
}

// ── Object-safe dynamic wrapper ───────────────────────────────────────────────

/// Object-safe version of `AgentClient` using boxed futures.
///
/// Use this when you need `Box<dyn AgentClientDyn>` for dynamic dispatch
/// (e.g. the return type of `active_client()`).  The blanket impl below
/// automatically covers any type that implements `AgentClient`.
pub trait AgentClientDyn: Send + Sync {
    fn chat_dyn<'a>(
        &'a self,
        messages: &'a [NeutralMessage],
        model: Option<&'a str>,
        system: Option<&'a str>,
        max_tokens: Option<u32>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + 'a>>;

    fn chat_structured_dyn<'a>(
        &'a self,
        messages: &'a [NeutralMessage],
        model: Option<&'a str>,
        system: Option<&'a str>,
        max_tokens: Option<u32>,
        schema: &'a Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + 'a>>;

    fn send_agentic_turn_dyn<'a>(
        &'a self,
        messages: &'a [NeutralMessage],
        tool_defs: &'a [ToolDef],
        model: Option<&'a str>,
        system: Option<&'a str>,
        max_tokens: Option<u32>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<AgentTurnResponse, String>> + Send + 'a>,
    >;

    fn list_models_dyn<'a>(
        &'a self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<String>, String>> + Send + 'a>>;
}

/// Blanket impl: any `AgentClient` automatically becomes an `AgentClientDyn`.
impl<T: AgentClient> AgentClientDyn for T {
    fn chat_dyn<'a>(
        &'a self,
        messages: &'a [NeutralMessage],
        model: Option<&'a str>,
        system: Option<&'a str>,
        max_tokens: Option<u32>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + 'a>>
    {
        Box::pin(self.chat(messages, model, system, max_tokens))
    }

    fn chat_structured_dyn<'a>(
        &'a self,
        messages: &'a [NeutralMessage],
        model: Option<&'a str>,
        system: Option<&'a str>,
        max_tokens: Option<u32>,
        schema: &'a Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + 'a>>
    {
        Box::pin(self.chat_structured(messages, model, system, max_tokens, schema))
    }

    fn send_agentic_turn_dyn<'a>(
        &'a self,
        messages: &'a [NeutralMessage],
        tool_defs: &'a [ToolDef],
        model: Option<&'a str>,
        system: Option<&'a str>,
        max_tokens: Option<u32>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<AgentTurnResponse, String>> + Send + 'a>,
    > {
        Box::pin(self.send_agentic_turn(messages, tool_defs, model, system, max_tokens))
    }

    fn list_models_dyn<'a>(
        &'a self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<String>, String>> + Send + 'a>>
    {
        Box::pin(self.list_models())
    }
}
