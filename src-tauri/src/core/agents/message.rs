use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ── Provider-neutral message types ────────────────────────────────────────────

/// Provider-neutral representation of one conversation message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeutralMessage {
    pub role: String,
    pub content: NeutralContent,
}

/// Content of a neutral message — either a plain text string or a list of
/// structured content blocks (tool calls, tool results, or mixed text+tool).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NeutralContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

/// A single content block within a structured message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// A plain text segment.
    Text { text: String },
    /// The model wants to invoke a tool.
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    /// The result of a previously requested tool invocation.
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

/// Provider-neutral tool definition. Each `AgentClient` converts this to the
/// wire format its provider expects (Anthropic uses `input_schema`; OpenAI uses
/// `function.parameters`).
#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    /// JSON Schema object describing the tool's input parameters.
    pub parameters: Value,
}

// ── Constructors ──────────────────────────────────────────────────────────────

impl NeutralMessage {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: NeutralContent::Text(text.into()),
        }
    }

    pub fn assistant_text(text: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: NeutralContent::Text(text.into()),
        }
    }

    pub fn user_blocks(blocks: Vec<ContentBlock>) -> Self {
        Self {
            role: "user".to_string(),
            content: NeutralContent::Blocks(blocks),
        }
    }

    pub fn assistant_blocks(blocks: Vec<ContentBlock>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: NeutralContent::Blocks(blocks),
        }
    }

    /// Return the plain text if this is a text-only message, or `None` if it
    /// contains structured content blocks.
    pub fn as_text(&self) -> Option<&str> {
        match &self.content {
            NeutralContent::Text(t) => Some(t),
            NeutralContent::Blocks(_) => None,
        }
    }
}

// ── Anthropic ↔ neutral conversions ──────────────────────────────────────────

/// Convert a neutral message to the Anthropic Messages API wire format.
///
/// Anthropic represents content as either a plain string or an array of typed
/// content blocks. Tool use and tool result blocks use Anthropic field names.
pub fn neutral_to_anthropic(msg: &NeutralMessage) -> Value {
    match &msg.content {
        NeutralContent::Text(t) => json!({ "role": msg.role, "content": t }),
        NeutralContent::Blocks(blocks) => {
            let wire_blocks: Vec<Value> = blocks.iter().map(block_to_anthropic).collect();
            json!({ "role": msg.role, "content": wire_blocks })
        }
    }
}

fn block_to_anthropic(block: &ContentBlock) -> Value {
    match block {
        ContentBlock::Text { text } => json!({ "type": "text", "text": text }),
        ContentBlock::ToolUse { id, name, input } => json!({
            "type": "tool_use",
            "id": id,
            "name": name,
            "input": input,
        }),
        ContentBlock::ToolResult {
            tool_use_id,
            content,
        } => json!({
            "type": "tool_result",
            "tool_use_id": tool_use_id,
            "content": content,
        }),
    }
}

/// Parse Anthropic wire-format content blocks into neutral `ContentBlock`s.
///
/// Blocks with unrecognised types are silently dropped; text content that
/// arrives as a plain string is wrapped into a single `Text` block.
pub fn anthropic_blocks_to_neutral(raw: &Value) -> Vec<ContentBlock> {
    match raw {
        Value::String(s) => vec![ContentBlock::Text { text: s.clone() }],
        Value::Array(arr) => arr.iter().filter_map(anthropic_block_to_neutral).collect(),
        _ => vec![],
    }
}

fn anthropic_block_to_neutral(block: &Value) -> Option<ContentBlock> {
    let kind = block.get("type")?.as_str()?;
    match kind {
        "text" => Some(ContentBlock::Text {
            text: block.get("text")?.as_str()?.to_string(),
        }),
        "tool_use" => Some(ContentBlock::ToolUse {
            id: block.get("id")?.as_str()?.to_string(),
            name: block.get("name")?.as_str()?.to_string(),
            input: block.get("input").cloned().unwrap_or(json!({})),
        }),
        "tool_result" => {
            let content = block
                .get("content")
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string();
            Some(ContentBlock::ToolResult {
                tool_use_id: block.get("tool_use_id")?.as_str()?.to_string(),
                content,
            })
        }
        _ => None,
    }
}

// ── OpenAI ↔ neutral conversions ─────────────────────────────────────────────

/// Convert a neutral message to the OpenAI Chat Completions API wire format.
///
/// OpenAI represents tool calls under `tool_calls` on `role:assistant` messages
/// and expects a separate `role:tool` message for each tool result.
pub fn neutral_to_openai(msg: &NeutralMessage) -> Vec<Value> {
    match &msg.content {
        NeutralContent::Text(t) => vec![json!({ "role": msg.role, "content": t })],
        NeutralContent::Blocks(blocks) => {
            let mut output: Vec<Value> = Vec::new();

            let tool_uses: Vec<&ContentBlock> = blocks
                .iter()
                .filter(|b| matches!(b, ContentBlock::ToolUse { .. }))
                .collect();

            let tool_results: Vec<&ContentBlock> = blocks
                .iter()
                .filter(|b| matches!(b, ContentBlock::ToolResult { .. }))
                .collect();

            let text_content: String = blocks
                .iter()
                .filter_map(|b| {
                    if let ContentBlock::Text { text } = b {
                        Some(text.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");

            if !tool_uses.is_empty() {
                // Assistant message with tool_calls array.
                let calls: Vec<Value> = tool_uses
                    .iter()
                    .filter_map(|b| {
                        if let ContentBlock::ToolUse { id, name, input } = b {
                            Some(json!({
                                "id": id,
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "arguments": input.to_string(),
                                }
                            }))
                        } else {
                            None
                        }
                    })
                    .collect();

                let content = if text_content.is_empty() {
                    Value::Null
                } else {
                    json!(text_content)
                };

                output.push(json!({
                    "role": "assistant",
                    "content": content,
                    "tool_calls": calls,
                }));
            } else if !text_content.is_empty() {
                output.push(json!({ "role": msg.role, "content": text_content }));
            }

            // Tool results become individual `role:tool` messages.
            for block in &tool_results {
                if let ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                } = block
                {
                    output.push(json!({
                        "role": "tool",
                        "tool_call_id": tool_use_id,
                        "content": content,
                    }));
                }
            }

            output
        }
    }
}

/// Convert a slice of neutral messages to a flat OpenAI message array,
/// expanding any neutral message that maps to multiple OpenAI messages.
pub fn neutral_messages_to_openai(messages: &[NeutralMessage]) -> Vec<Value> {
    messages.iter().flat_map(neutral_to_openai).collect()
}

/// Convert an OpenAI assistant response (with optional `tool_calls`) into
/// neutral content blocks.
pub fn openai_response_to_neutral_blocks(response: &Value) -> Vec<ContentBlock> {
    let mut blocks: Vec<ContentBlock> = Vec::new();

    // Plain text content.
    if let Some(text) = response.get("content").and_then(|c| c.as_str()) {
        if !text.is_empty() {
            blocks.push(ContentBlock::Text {
                text: text.to_string(),
            });
        }
    }

    // Tool calls array.
    if let Some(calls) = response.get("tool_calls").and_then(|c| c.as_array()) {
        for call in calls {
            let id = call
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let func = call.get("function");
            let name = func
                .and_then(|f| f.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let args_raw = func
                .and_then(|f| f.get("arguments"))
                .and_then(|v| v.as_str())
                .unwrap_or("{}");
            let input: Value = serde_json::from_str(args_raw).unwrap_or(json!({}));
            blocks.push(ContentBlock::ToolUse { id, name, input });
        }
    }

    blocks
}

// ── ToolDef formatting ────────────────────────────────────────────────────────

/// Format a `ToolDef` for the Anthropic Messages API.
pub fn tool_def_to_anthropic(t: &ToolDef) -> Value {
    json!({
        "name": t.name,
        "description": t.description,
        "input_schema": t.parameters,
    })
}

/// Format a `ToolDef` for the OpenAI Chat Completions API.
pub fn tool_def_to_openai(t: &ToolDef) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": t.name,
            "description": t.description,
            "parameters": t.parameters,
        }
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── Anthropic round-trips ─────────────────────────────────────────────────

    #[test]
    fn text_message_round_trips_through_anthropic() {
        let msg = NeutralMessage::user("hello");
        let wire = neutral_to_anthropic(&msg);
        assert_eq!(wire["role"], "user");
        assert_eq!(wire["content"], "hello");
    }

    #[test]
    fn tool_use_block_serialises_to_anthropic() {
        let blocks = vec![ContentBlock::ToolUse {
            id: "tu_1".into(),
            name: "read_file".into(),
            input: json!({ "path": "/tmp/foo.txt" }),
        }];
        let msg = NeutralMessage::assistant_blocks(blocks);
        let wire = neutral_to_anthropic(&msg);
        let content = wire["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "tool_use");
        assert_eq!(content[0]["id"], "tu_1");
        assert_eq!(content[0]["name"], "read_file");
    }

    #[test]
    fn tool_result_block_serialises_to_anthropic() {
        let blocks = vec![ContentBlock::ToolResult {
            tool_use_id: "tu_1".into(),
            content: "file content here".into(),
        }];
        let msg = NeutralMessage::user_blocks(blocks);
        let wire = neutral_to_anthropic(&msg);
        let content = wire["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "tool_result");
        assert_eq!(content[0]["tool_use_id"], "tu_1");
        assert_eq!(content[0]["content"], "file content here");
    }

    #[test]
    fn anthropic_text_block_parses_to_neutral() {
        let raw = json!([{ "type": "text", "text": "Hello there" }]);
        let blocks = anthropic_blocks_to_neutral(&raw);
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            blocks[0],
            ContentBlock::Text {
                text: "Hello there".into()
            }
        );
    }

    #[test]
    fn anthropic_tool_use_block_parses_to_neutral() {
        let raw = json!([{
            "type": "tool_use",
            "id": "tu_42",
            "name": "list_skills",
            "input": {}
        }]);
        let blocks = anthropic_blocks_to_neutral(&raw);
        assert_eq!(blocks.len(), 1);
        assert!(matches!(
            &blocks[0],
            ContentBlock::ToolUse { id, name, .. }
            if id == "tu_42" && name == "list_skills"
        ));
    }

    #[test]
    fn anthropic_string_content_parses_to_text_block() {
        let raw = json!("just a string");
        let blocks = anthropic_blocks_to_neutral(&raw);
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            blocks[0],
            ContentBlock::Text {
                text: "just a string".into()
            }
        );
    }

    #[test]
    fn tool_use_round_trip_anthropic() {
        let original = vec![
            ContentBlock::ToolUse {
                id: "tu_1".into(),
                name: "read_file".into(),
                input: json!({ "path": "src/main.rs" }),
            },
            ContentBlock::Text {
                text: "I'll read that file.".into(),
            },
        ];
        let msg = NeutralMessage::assistant_blocks(original.clone());
        let wire = neutral_to_anthropic(&msg);
        let parsed = anthropic_blocks_to_neutral(&wire["content"]);
        assert_eq!(parsed, original);
    }

    // ── OpenAI round-trips ────────────────────────────────────────────────────

    #[test]
    fn text_message_converts_to_openai() {
        let msg = NeutralMessage::user("hi");
        let wire = neutral_to_openai(&msg);
        assert_eq!(wire.len(), 1);
        assert_eq!(wire[0]["role"], "user");
        assert_eq!(wire[0]["content"], "hi");
    }

    #[test]
    fn tool_use_blocks_convert_to_openai_tool_calls() {
        let blocks = vec![ContentBlock::ToolUse {
            id: "call_abc".into(),
            name: "read_file".into(),
            input: json!({ "path": "foo.txt" }),
        }];
        let msg = NeutralMessage::assistant_blocks(blocks);
        let wire = neutral_to_openai(&msg);
        assert_eq!(wire.len(), 1);
        let tool_calls = wire[0]["tool_calls"].as_array().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0]["id"], "call_abc");
        assert_eq!(tool_calls[0]["function"]["name"], "read_file");
    }

    #[test]
    fn tool_result_blocks_become_separate_tool_messages() {
        let blocks = vec![
            ContentBlock::ToolResult {
                tool_use_id: "call_1".into(),
                content: "result A".into(),
            },
            ContentBlock::ToolResult {
                tool_use_id: "call_2".into(),
                content: "result B".into(),
            },
        ];
        let msg = NeutralMessage::user_blocks(blocks);
        let wire = neutral_to_openai(&msg);
        assert_eq!(wire.len(), 2);
        assert_eq!(wire[0]["role"], "tool");
        assert_eq!(wire[0]["tool_call_id"], "call_1");
        assert_eq!(wire[1]["role"], "tool");
        assert_eq!(wire[1]["tool_call_id"], "call_2");
    }

    #[test]
    fn openai_response_with_tool_calls_parses_to_neutral() {
        let response = json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": "call_xyz",
                "type": "function",
                "function": {
                    "name": "list_skills",
                    "arguments": "{}"
                }
            }]
        });
        let blocks = openai_response_to_neutral_blocks(&response);
        assert_eq!(blocks.len(), 1);
        assert!(matches!(&blocks[0], ContentBlock::ToolUse { name, .. } if name == "list_skills"));
    }

    #[test]
    fn openai_text_response_parses_to_neutral() {
        let response = json!({ "role": "assistant", "content": "Done." });
        let blocks = openai_response_to_neutral_blocks(&response);
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            blocks[0],
            ContentBlock::Text {
                text: "Done.".into()
            }
        );
    }

    // ── ToolDef formatting ────────────────────────────────────────────────────

    #[test]
    fn tool_def_formats_for_anthropic() {
        let t = ToolDef {
            name: "read_file".into(),
            description: "Read a file.".into(),
            parameters: json!({ "type": "object", "properties": {}, "required": [] }),
        };
        let wire = tool_def_to_anthropic(&t);
        assert_eq!(wire["name"], "read_file");
        assert!(wire.get("input_schema").is_some());
        assert!(wire.get("parameters").is_none());
    }

    #[test]
    fn tool_def_formats_for_openai() {
        let t = ToolDef {
            name: "list_skills".into(),
            description: "List skills.".into(),
            parameters: json!({ "type": "object", "properties": {}, "required": [] }),
        };
        let wire = tool_def_to_openai(&t);
        assert_eq!(wire["type"], "function");
        assert_eq!(wire["function"]["name"], "list_skills");
        assert!(wire["function"].get("parameters").is_some());
        assert!(wire["function"].get("input_schema").is_none());
    }
}
