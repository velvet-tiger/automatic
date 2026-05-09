use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use super::agents::{
    message::{neutral_to_anthropic, ContentBlock, NeutralContent, NeutralMessage, ToolDef},
    AgentClientDyn, GatewayConfig,
};

// ── Public types ──────────────────────────────────────────────────────────────

/// A single message in a conversation (text-only, used by the basic chat API).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AiMessage {
    pub role: String,
    pub content: String,
}

// ── Toggle + key resolution ───────────────────────────────────────────────────

/// Resolve the model to use for a request against `agent_id`.
///
/// Priority:
///   1. `explicit` — caller-supplied model (e.g. from AI Playground).
///   2. `Settings::agent_models[agent_id]` — user's saved preference.
///   3. `agents::default_model(agent_id)` — hardcoded provider default.
fn resolve_model<'a>(agent_id: &str, explicit: Option<&'a str>) -> String {
    if let Some(m) = explicit {
        return m.to_string();
    }
    super::settings::read_settings()
        .ok()
        .and_then(|s| s.agent_models.get(agent_id).cloned())
        .unwrap_or_else(|| super::agents::default_model(agent_id).to_string())
}

/// Resolve the currently configured active agent ID.
///
/// Reads `Settings::active_agent`; defaults to `"anthropic"` when unset so
/// that existing users with a stored Anthropic key are unaffected.
fn active_agent_id() -> String {
    super::settings::read_settings()
        .ok()
        .and_then(|s| s.active_agent)
        .unwrap_or_else(|| "anthropic".to_string())
}

/// Effective state of the master "Agent features" toggle.
///
/// * `Some(true)`  → enabled
/// * `Some(false)` → disabled
/// * `None`        → never set explicitly; derives from key presence so
///   upgrading users keep their previous behaviour
pub fn agent_features_enabled() -> bool {
    let explicit = super::settings::read_settings()
        .ok()
        .and_then(|s| s.agent_features_enabled);
    match explicit {
        Some(value) => value,
        None => super::agents::known_agents()
            .iter()
            .any(|id| super::credentials::has_api_key(id.as_str())),
    }
}

/// Resolve the API key to use for a request.
///
/// Priority order:
///   1. Master toggle: if `agent_features_enabled` is `Some(false)`, fail
///      regardless of stored keys.
///   2. Key stored in the OS keychain for the active agent.
///   3. The `api_key` argument (explicitly supplied by the caller).
///
/// Returns an error if features are disabled or no key is found.
pub fn resolve_api_key(explicit_key: Option<&str>) -> Result<String, String> {
    if let Ok(settings) = super::settings::read_settings() {
        if settings.agent_features_enabled == Some(false) {
            return Err(
                "Agent features are disabled. Enable them in Settings > Agents.".to_string(),
            );
        }
    }

    let agent_id = active_agent_id();
    if let Ok(k) = super::credentials::get_api_key(&agent_id) {
        if !k.is_empty() {
            return Ok(k);
        }
    }

    if let Some(k) = explicit_key {
        if !k.is_empty() {
            return Ok(k.to_string());
        }
    }

    Err("No API key found for the active agent. Add a key via Settings > Agents.".to_string())
}

// ── Active client factory ─────────────────────────────────────────────────────

/// Build the active agent client, applying any configured gateway routing.
fn active_client(api_key: &str) -> Box<dyn AgentClientDyn> {
    let agent_id = active_agent_id();
    let gateway = load_active_gateway(&agent_id);
    super::agents::active_client_with_key(&agent_id, api_key, gateway)
}

/// Read the AI Gateway config for the given agent from settings, if any.
fn load_active_gateway(agent_id: &str) -> Option<GatewayConfig> {
    super::settings::read_settings()
        .ok()
        .and_then(|s| s.agent_gateways.get(agent_id).cloned())
}

// ── Public AI operations ──────────────────────────────────────────────────────

/// Fetch the list of available model IDs from the active provider's API.
pub async fn list_models() -> Result<Vec<String>, String> {
    let key = resolve_api_key(None)?;
    active_client(&key).list_models_dyn().await
}

/// Send a list of messages to the active provider and return the assistant's
/// text response.
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
    let agent_id = active_agent_id();
    let model_str = resolve_model(&agent_id, model.as_deref());
    let neutral = ai_messages_to_neutral(&messages);
    active_client(&key)
        .chat_dyn(&neutral, Some(&model_str), system.as_deref(), max_tokens)
        .await
}

/// Send messages with a JSON Schema output constraint and return the
/// guaranteed-valid JSON reply.
///
/// Structured outputs cannot be combined with tool use — use this as a
/// second phase after `chat_with_tools_returning_history` has gathered facts.
pub async fn chat_structured(
    messages: Vec<AiMessage>,
    api_key: Option<String>,
    model: Option<String>,
    system: Option<String>,
    max_tokens: Option<u32>,
    schema: Value,
) -> Result<String, String> {
    let key = resolve_api_key(api_key.as_deref())?;
    let agent_id = active_agent_id();
    let model_str = resolve_model(&agent_id, model.as_deref());
    let neutral = ai_messages_to_neutral(&messages);
    active_client(&key)
        .chat_structured_dyn(&neutral, Some(&model_str), system.as_deref(), max_tokens, &schema)
        .await
}

// ── Tool-use (agentic loop) ───────────────────────────────────────────────────

/// Maximum agentic turns before we abort to prevent runaway loops.
// Some recommendation research prompts require additional Discover tool calls
// before the model can produce a final answer. A 10-turn cap is too tight and
// can terminate otherwise valid runs.
const MAX_TOOL_TURNS: usize = 20;

/// Allowed content-types for the `read_file` tool (binary-safe check).
const BINARY_EXTENSIONS: &[&str] = &[
    "zip", "tar", "gz", "exe", "dll", "so", "class", "jar", "war", "7z", "doc", "docx", "xls",
    "xlsx", "ppt", "pptx", "odt", "ods", "odp", "bin", "dat", "obj", "o", "a", "lib", "wasm",
    "pyc", "pyo", "png", "jpg", "jpeg", "gif", "bmp", "ico", "webp", "mp3", "mp4", "mov", "avi",
    "mkv", "pdf",
];

pub async fn chat_with_tools(
    messages: Vec<AiMessage>,
    api_key: Option<String>,
    model: Option<String>,
    system: Option<String>,
    max_tokens: Option<u32>,
    working_dir: String,
) -> Result<String, String> {
    let (text, _) = chat_with_tools_inner(
        messages,
        api_key,
        model,
        system,
        max_tokens,
        working_dir,
        None,
    )
    .await?;
    Ok(text)
}

/// Like `chat_with_tools` but also returns the full accumulated message history
/// (as `Value` items suitable for passing to `chat_structured` as context).
/// Text-only turns have `content` as a string; tool turns have `content` as
/// an array and are naturally skipped by callers that only read `.as_str()`.
pub async fn chat_with_tools_returning_history(
    messages: Vec<AiMessage>,
    api_key: Option<String>,
    model: Option<String>,
    system: Option<String>,
    max_tokens: Option<u32>,
    working_dir: String,
) -> Result<(String, Vec<Value>), String> {
    chat_with_tools_inner(
        messages,
        api_key,
        model,
        system,
        max_tokens,
        working_dir,
        None,
    )
    .await
}

async fn chat_with_tools_inner(
    messages: Vec<AiMessage>,
    api_key: Option<String>,
    model: Option<String>,
    system: Option<String>,
    max_tokens: Option<u32>,
    working_dir: String,
    max_turns_override: Option<usize>,
) -> Result<(String, Vec<Value>), String> {
    let work_path = validate_working_dir(&working_dir)?;
    let key = resolve_api_key(api_key.as_deref())?;
    let client = active_client(&key);

    let agent_id = active_agent_id();
    let model_str = resolve_model(&agent_id, model.as_deref());
    let tokens = max_tokens.unwrap_or(4096);
    let turn_limit = max_turns_override.unwrap_or(MAX_TOOL_TURNS);

    // Build initial neutral message history.
    let mut history: Vec<NeutralMessage> = ai_messages_to_neutral(&messages);

    let tool_defs = build_tool_defs();

    for _turn in 0..turn_limit {
        let turn_result = client
            .send_agentic_turn_dyn(
                &history,
                &tool_defs,
                Some(model_str.as_str()),
                system.as_deref(),
                Some(tokens),
            )
            .await?;

        // Append the assistant's response to the running history.
        history.push(NeutralMessage::assistant_blocks(
            turn_result.content_blocks.clone(),
        ));

        if turn_result.stop_reason != "tool_use" {
            // Extract the final text from the assistant's content blocks.
            let text = turn_result
                .content_blocks
                .iter()
                .find_map(|b| {
                    if let ContentBlock::Text { text } = b {
                        Some(text.clone())
                    } else {
                        None
                    }
                })
                .ok_or_else(|| "Agent returned no text content".to_string())?;

            // Serialise the neutral history to Values for backward-compat callers.
            let json_history: Vec<Value> =
                history.iter().map(neutral_to_anthropic).collect();

            return Ok((text, json_history));
        }

        // Execute each tool_use block and collect results.
        let mut tool_results: Vec<ContentBlock> = Vec::new();
        for block in &turn_result.content_blocks {
            if let ContentBlock::ToolUse { id, name, input } = block {
                let result = match name.as_str() {
                    "read_file" => execute_read_file(input, &work_path),
                    "list_skills" => execute_list_skills(),
                    "read_skill" => execute_read_skill(input),
                    "list_rules" => execute_list_rules(),
                    "read_rule" => execute_read_rule(input),
                    "list_templates" => execute_list_templates(),
                    "read_template" => execute_read_template(input),
                    "list_mcp_servers" => execute_list_mcp_servers(),
                    "read_mcp_server" => execute_read_mcp_server(input),
                    "search_discover_skills" => execute_search_discover_skills(input).await,
                    "search_discover_mcp" => execute_search_discover_mcp(input),
                    "search_collections" => execute_search_collections(input),
                    "search_discover_templates" => execute_search_discover_templates(input),
                    other => format!("Error: unknown tool '{}'.", other),
                };
                tool_results.push(ContentBlock::ToolResult {
                    tool_use_id: id.clone(),
                    content: result,
                });
            }
        }

        history.push(NeutralMessage::user_blocks(tool_results));
    }

    Err(format!(
        "Agentic loop exceeded {} turns without a final response.",
        turn_limit
    ))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn ai_messages_to_neutral(messages: &[AiMessage]) -> Vec<NeutralMessage> {
    messages
        .iter()
        .map(|m| NeutralMessage {
            role: m.role.clone(),
            content: NeutralContent::Text(m.content.clone()),
        })
        .collect()
}

// ── Tool definitions ──────────────────────────────────────────────────────────

/// Assemble the full set of provider-neutral tool definitions for the agentic loop.
fn build_tool_defs() -> Vec<ToolDef> {
    vec![
        read_file_tool_def(),
        list_skills_tool_def(),
        read_skill_tool_def(),
        list_rules_tool_def(),
        read_rule_tool_def(),
        list_templates_tool_def(),
        read_template_tool_def(),
        list_mcp_servers_tool_def(),
        read_mcp_server_tool_def(),
        search_discover_skills_tool_def(),
        search_discover_mcp_tool_def(),
        search_collections_tool_def(),
        search_discover_templates_tool_def(),
    ]
}

fn read_file_tool_def() -> ToolDef {
    ToolDef {
        name: "read_file".into(),
        description: "Read a file or list a directory from the local filesystem. \
            For files, returns the content with line numbers. \
            For directories, returns an entry list. \
            Only files within the project working directory are accessible."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute or relative path to the file or directory to read."
                },
                "offset": {
                    "type": "integer",
                    "description": "Line number to start reading from (1-indexed, optional)."
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of lines to return (default 200, max 500)."
                }
            },
            "required": ["path"]
        }),
    }
}

fn list_skills_tool_def() -> ToolDef {
    ToolDef {
        name: "list_skills".into(),
        description: "List all skill names available in Automatic's managed \
            skill library (~/.automatic/library/skills/) plus any skills discovered \
            in external scan locations such as ~/.agents/skills/ and ~/.claude/skills/."
            .into(),
        parameters: json!({ "type": "object", "properties": {}, "required": [] }),
    }
}

fn read_skill_tool_def() -> ToolDef {
    ToolDef {
        name: "read_skill".into(),
        description: "Read the content of a skill from the Automatic skill library by name.".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The skill name (directory name in the managed library at ~/.automatic/library/skills/)."
                }
            },
            "required": ["name"]
        }),
    }
}

fn list_rules_tool_def() -> ToolDef {
    ToolDef {
        name: "list_rules".into(),
        description: "List all rules available in the Automatic rules library. \
            Returns each rule's machine name (id) and display name."
            .into(),
        parameters: json!({ "type": "object", "properties": {}, "required": [] }),
    }
}

fn read_rule_tool_def() -> ToolDef {
    ToolDef {
        name: "read_rule".into(),
        description:
            "Read the content of a rule from the Automatic rules library by its machine name (id)."
                .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "The rule machine name (lowercase slug, e.g. \"automatic-general\")."
                }
            },
            "required": ["id"]
        }),
    }
}

fn list_templates_tool_def() -> ToolDef {
    ToolDef {
        name: "list_templates".into(),
        description:
            "List all instruction template names available in the Automatic templates library."
                .into(),
        parameters: json!({ "type": "object", "properties": {}, "required": [] }),
    }
}

fn read_template_tool_def() -> ToolDef {
    ToolDef {
        name: "read_template".into(),
        description: "Read the markdown content of an instruction template from the Automatic library by name.".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The template name (filename stem without .md extension)."
                }
            },
            "required": ["name"]
        }),
    }
}

fn list_mcp_servers_tool_def() -> ToolDef {
    ToolDef {
        name: "list_mcp_servers".into(),
        description:
            "List all MCP server configuration names registered in the Automatic library.".into(),
        parameters: json!({ "type": "object", "properties": {}, "required": [] }),
    }
}

fn read_mcp_server_tool_def() -> ToolDef {
    ToolDef {
        name: "read_mcp_server".into(),
        description: "Read the configuration for a single MCP server from the Automatic library by name. \
            Returns the JSON config (command, args, env, etc.)."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The MCP server config name (filename stem without .json extension)."
                }
            },
            "required": ["name"]
        }),
    }
}

fn search_discover_skills_tool_def() -> ToolDef {
    ToolDef {
        name: "search_discover_skills".into(),
        description: "Search the skills.sh community registry for skills matching a query. \
            Returns skill names, install counts, and source repos. \
            Use this to discover skills that are not yet installed locally."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query — skill name, topic, or keyword."
                }
            },
            "required": ["query"]
        }),
    }
}

fn search_discover_mcp_tool_def() -> ToolDef {
    ToolDef {
        name: "search_discover_mcp".into(),
        description: "Search the featured MCP server catalogue for servers matching a query. \
            Returns title, description, provider, classification, and install config. \
            Use this to discover MCP servers available to add to a project."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query — server name, provider, or keyword. Leave empty to list all."
                }
            },
            "required": ["query"]
        }),
    }
}

fn search_collections_tool_def() -> ToolDef {
    ToolDef {
        name: "search_collections".into(),
        description: "Search the bundled collections catalogue for curated sets of skills, MCP servers, and templates. \
            Returns matching collection names, descriptions, and their contents."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query — collection name, tag, or keyword. Leave empty to list all."
                }
            },
            "required": ["query"]
        }),
    }
}

fn search_discover_templates_tool_def() -> ToolDef {
    ToolDef {
        name: "search_discover_templates".into(),
        description: "Search the bundled project template Discover catalogue for templates matching a query. \
            Returns template names, descriptions, categories, required skills, and MCP servers."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query — template name, category, tag, or keyword. Leave empty to list all."
                }
            },
            "required": ["query"]
        }),
    }
}

// ── Library tool executors ────────────────────────────────────────────────────

fn execute_list_skills() -> String {
    match crate::core::list_skills() {
        Ok(skills) => {
            let names: Vec<String> = skills.into_iter().map(|s| s.name).collect();
            serde_json::to_string_pretty(&names).unwrap_or_else(|_| "[]".to_string())
        }
        Err(e) => format!("Error listing skills: {}", e),
    }
}

fn execute_read_skill(input: &Value) -> String {
    let name = match input.get("name").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return "Error: missing required parameter 'name'.".to_string(),
    };
    match crate::core::read_skill(name) {
        Ok(content) => content,
        Err(e) => format!("Error reading skill '{}': {}", name, e),
    }
}

fn execute_list_rules() -> String {
    match crate::core::list_rules() {
        Ok(rules) => serde_json::to_string_pretty(&rules).unwrap_or_else(|_| "[]".to_string()),
        Err(e) => format!("Error listing rules: {}", e),
    }
}

fn execute_read_rule(input: &Value) -> String {
    let id = match input.get("id").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return "Error: missing required parameter 'id'.".to_string(),
    };
    match crate::core::read_rule_content(id) {
        Ok(content) => content,
        Err(e) => format!("Error reading rule '{}': {}", id, e),
    }
}

fn execute_list_templates() -> String {
    match crate::core::list_templates() {
        Ok(templates) => {
            serde_json::to_string_pretty(&templates).unwrap_or_else(|_| "[]".to_string())
        }
        Err(e) => format!("Error listing templates: {}", e),
    }
}

fn execute_read_template(input: &Value) -> String {
    let name = match input.get("name").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return "Error: missing required parameter 'name'.".to_string(),
    };
    match crate::core::read_template(name) {
        Ok(content) => content,
        Err(e) => format!("Error reading template '{}': {}", name, e),
    }
}

fn execute_list_mcp_servers() -> String {
    match crate::core::list_mcp_server_configs() {
        Ok(names) => serde_json::to_string_pretty(&names).unwrap_or_else(|_| "[]".to_string()),
        Err(e) => format!("Error listing MCP servers: {}", e),
    }
}

fn execute_read_mcp_server(input: &Value) -> String {
    let name = match input.get("name").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return "Error: missing required parameter 'name'.".to_string(),
    };
    match crate::core::read_mcp_server_config(name) {
        Ok(content) => content,
        Err(e) => format!("Error reading MCP server config '{}': {}", name, e),
    }
}

// ── Discover tool executors ──────────────────────────────────────────────────

async fn execute_search_discover_skills(input: &Value) -> String {
    let query = input.get("query").and_then(|v| v.as_str()).unwrap_or("");
    match crate::core::search_remote_skills(query).await {
        Ok(results) => serde_json::to_string_pretty(&results).unwrap_or_else(|_| "[]".to_string()),
        Err(e) => format!("Error searching Discover skills: {}", e),
    }
}

fn execute_search_discover_mcp(input: &Value) -> String {
    let query = input.get("query").and_then(|v| v.as_str()).unwrap_or("");
    match crate::core::search_discover_mcp(query) {
        Ok(json) => json,
        Err(e) => format!("Error searching Discover MCP: {}", e),
    }
}

fn execute_search_collections(input: &Value) -> String {
    let query = input.get("query").and_then(|v| v.as_str()).unwrap_or("");
    match crate::core::search_collections(query) {
        Ok(json) => json,
        Err(e) => format!("Error searching collections: {}", e),
    }
}

fn execute_search_discover_templates(input: &Value) -> String {
    let query = input.get("query").and_then(|v| v.as_str()).unwrap_or("");
    match crate::core::search_bundled_templates(query) {
        Ok(json) => json,
        Err(e) => format!("Error searching Discover templates: {}", e),
    }
}

// ── File-system security ──────────────────────────────────────────────────────

/// Filenames (exact, case-insensitive match against the final path component)
/// that are never readable regardless of directory.
const SECRET_FILENAMES: &[&str] = &[
    ".env",
    "credentials",
    "config",
    ".netrc",
    "token",
    "tokens",
];

/// Filename *prefixes* (case-insensitive) — any filename that starts with one
/// of these is blocked regardless of suffix.
const SECRET_FILENAME_PREFIXES: &[&str] = &["id_rsa", "id_dsa", "id_ecdsa", "id_ed25519"];

/// Parent directory names (case-insensitive, matched against *any* ancestor
/// component of the resolved path) — every file inside these directories is blocked.
const SECRET_PARENT_DIRS: &[&str] = &[".ssh", ".aws", ".claude", ".gnupg"];

/// File extensions that are always blocked.
const SECRET_EXTENSIONS: &[&str] = &["pem", "key", "p12", "pfx", "cer", "crt", "der", "ppk"];

/// Resolve and validate a file path requested by the AI.
///
/// Rules (applied in order):
///   1. The path must canonicalise successfully — if it does not exist on disk,
///      return not-found.
///   2. The resolved path must be strictly inside `working_dir`.
///   3. The filename must not match the secret filename or extension denylists.
pub(crate) fn resolve_tool_path(path: &str, working_dir: &Path) -> Result<PathBuf, String> {
    let p = Path::new(path);
    let joined = if p.is_absolute() {
        p.to_path_buf()
    } else {
        working_dir.join(p)
    };

    let canonical = joined
        .canonicalize()
        .map_err(|_| format!("Path does not exist or is not accessible: '{}'", path))?;

    let canonical_dir = working_dir
        .canonicalize()
        .map_err(|_| "Working directory is not accessible.".to_string())?;

    if !canonical.starts_with(&canonical_dir) {
        return Err(format!(
            "Access denied: '{}' is outside the working directory.",
            path
        ));
    }

    for component in canonical.components() {
        if let std::path::Component::Normal(os_name) = component {
            if let Some(part) = os_name.to_str() {
                let lower = part.to_lowercase();
                if SECRET_PARENT_DIRS.contains(&lower.as_str()) {
                    return Err(format!(
                        "Access denied: '{}' is inside a protected directory.",
                        path
                    ));
                }
            }
        }
    }

    if let Some(name) = canonical.file_name().and_then(|n| n.to_str()) {
        let lower = name.to_lowercase();

        if SECRET_FILENAMES.contains(&lower.as_str()) {
            return Err(format!(
                "Access denied: '{}' is a protected filename.",
                name
            ));
        }

        if lower.starts_with(".env") {
            return Err(format!(
                "Access denied: '{}' matches the .env file pattern.",
                name
            ));
        }

        for prefix in SECRET_FILENAME_PREFIXES {
            if lower.starts_with(prefix) {
                return Err(format!(
                    "Access denied: '{}' matches a protected key filename pattern.",
                    name
                ));
            }
        }
    }

    if let Some(ext) = canonical.extension().and_then(|e| e.to_str()) {
        if SECRET_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
            return Err(format!("Access denied: '.{}' files are protected.", ext));
        }
    }

    Ok(canonical)
}

/// Execute the `read_file` tool call on behalf of the AI.
pub(crate) fn execute_read_file(input: &Value, working_dir: &Path) -> String {
    let path_str = match input.get("path").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return "Error: missing required parameter 'path'.".to_string(),
    };

    let resolved = match resolve_tool_path(path_str, working_dir) {
        Ok(p) => p,
        Err(e) => return format!("Error: {}", e),
    };

    if let Some(ext) = resolved.extension().and_then(|e| e.to_str()) {
        if BINARY_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
            return format!("Error: Cannot read binary file '{}'.", path_str);
        }
    }

    match resolved.metadata() {
        Err(_) => return format!("Error: Path does not exist: '{}'.", path_str),
        Ok(meta) if meta.is_dir() => {
            match std::fs::read_dir(&resolved) {
                Err(e) => return format!("Error reading directory: {}", e),
                Ok(entries) => {
                    let mut names: Vec<String> = entries
                        .filter_map(|e| e.ok())
                        .map(|e| {
                            let name = e.file_name().to_string_lossy().to_string();
                            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                                name + "/"
                            } else {
                                name
                            }
                        })
                        .collect();
                    names.sort();
                    return format!(
                        "<path>{}</path>\n<type>directory</type>\n<entries>\n{}\n</entries>",
                        resolved.display(),
                        names.join("\n")
                    );
                }
            }
        }
        Ok(_) => {}
    }

    let raw = match std::fs::read_to_string(&resolved) {
        Ok(s) => s,
        Err(e) => return format!("Error reading file '{}': {}", path_str, e),
    };

    let offset = input
        .get("offset")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .unwrap_or(1)
        .max(1);

    let limit = input
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|n| (n as usize).min(500))
        .unwrap_or(200);

    let lines: Vec<&str> = raw.lines().collect();
    let total = lines.len();
    let start = (offset - 1).min(total);
    let end = (start + limit).min(total);
    let slice = &lines[start..end];

    let numbered: Vec<String> = slice
        .iter()
        .enumerate()
        .map(|(i, l)| format!("{}: {}", start + i + 1, l))
        .collect();

    let mut output = format!(
        "<path>{}</path>\n<type>file</type>\n<content>\n{}",
        resolved.display(),
        numbered.join("\n")
    );

    if end < total {
        output += &format!(
            "\n\n(Showing lines {}-{} of {}. Use offset={} to continue.)",
            offset,
            end,
            total,
            end + 1
        );
    } else {
        output += &format!("\n\n(End of file - total {} lines)", total);
    }
    output += "\n</content>";

    output
}

// ── Working-directory validation ──────────────────────────────────────────────

/// Paths that are never acceptable as `working_dir`.
const DANGEROUS_WORKING_DIRS: &[&str] = &[
    "/",
    "/etc",
    "/etc/",
    "/usr",
    "/usr/",
    "/var",
    "/var/",
    "/root",
    "/root/",
    "/home",
    "/home/",
    "/private",
    "/private/",
    "C:\\",
    "C:/",
];

/// Validate a `working_dir` string before using it as a sandbox boundary.
pub(crate) fn validate_working_dir(working_dir: &str) -> Result<PathBuf, String> {
    let trimmed = working_dir.trim();
    if trimmed.is_empty() {
        return Err(
            "working_dir must be set to a specific project directory before using file tools."
                .to_string(),
        );
    }

    let work_path = PathBuf::from(trimmed).canonicalize().map_err(|_| {
        format!(
            "working_dir '{}' does not exist or is not accessible.",
            trimmed
        )
    })?;

    for &dangerous in DANGEROUS_WORKING_DIRS {
        let raw = if dangerous == "/" || dangerous == "C:\\" || dangerous == "C:/" {
            PathBuf::from(dangerous)
        } else {
            PathBuf::from(dangerous.trim_end_matches(['/', '\\']))
        };
        let canonical_dangerous = raw.canonicalize().unwrap_or(raw);

        if work_path == canonical_dangerous {
            return Err(format!(
                "working_dir '{}' is too broad. Set it to a specific project directory.",
                trimmed
            ));
        }
    }

    if let Some(home) = dirs::home_dir() {
        let canonical_home = home.canonicalize().unwrap_or(home);
        if work_path == canonical_home {
            return Err(
                "working_dir must not be the home directory. Set it to a specific project directory."
                    .to_string(),
            );
        }
    }

    Ok(work_path)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;
    use tempfile::TempDir;

    fn setup_project() -> (TempDir, PathBuf) {
        let dir = TempDir::new().expect("tempdir");
        let file = dir.path().join("hello.txt");
        fs::write(&file, "line one\nline two\nline three\n").expect("write");
        (dir, file)
    }

    // ── validate_working_dir ──────────────────────────────────────────────────

    #[test]
    fn working_dir_empty_string_is_rejected() {
        assert!(validate_working_dir("").is_err());
        assert!(validate_working_dir("   ").is_err());
    }

    #[test]
    fn working_dir_nonexistent_path_is_rejected() {
        let result = validate_working_dir("/this/path/does/not/exist/ever");
        assert!(result.is_err(), "non-existent path should be rejected");
    }

    #[test]
    fn working_dir_root_is_rejected() {
        let result = validate_working_dir("/");
        assert!(result.is_err(), "/ must be rejected");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("too broad"),
            "expected 'too broad', got: {}",
            msg
        );
    }

    #[test]
    fn working_dir_etc_is_rejected() {
        if !std::path::Path::new("/etc").exists() {
            return;
        }
        let result = validate_working_dir("/etc");
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("too broad"),
            "expected 'too broad', got: {}",
            msg
        );
    }

    #[test]
    fn working_dir_valid_project_directory_is_accepted() {
        let dir = TempDir::new().expect("tempdir");
        let result = validate_working_dir(dir.path().to_str().unwrap());
        assert!(
            result.is_ok(),
            "valid project dir should be accepted: {:?}",
            result
        );
    }

    #[test]
    fn working_dir_home_directory_is_rejected() {
        if let Some(home) = dirs::home_dir() {
            if home.exists() {
                let result = validate_working_dir(home.to_str().unwrap());
                assert!(result.is_err(), "home directory must be rejected");
                let msg = result.unwrap_err();
                assert!(
                    msg.contains("home directory"),
                    "expected 'home directory' in error, got: {}",
                    msg
                );
            }
        }
    }

    // ── resolve_tool_path ─────────────────────────────────────────────────────

    #[test]
    fn resolve_allows_file_inside_working_dir() {
        let (dir, file) = setup_project();
        let result = resolve_tool_path(file.to_str().unwrap(), dir.path());
        assert!(
            result.is_ok(),
            "file inside working dir should resolve: {:?}",
            result
        );
    }

    #[test]
    fn resolve_allows_relative_path_inside_working_dir() {
        let (dir, _) = setup_project();
        let result = resolve_tool_path("hello.txt", dir.path());
        assert!(
            result.is_ok(),
            "relative path inside working dir should resolve: {:?}",
            result
        );
    }

    #[test]
    fn resolve_rejects_path_outside_working_dir() {
        let (dir, _) = setup_project();
        let result = resolve_tool_path("/tmp", dir.path());
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("outside the working directory"),
            "expected boundary error, got: {}",
            msg
        );
    }

    #[test]
    fn resolve_rejects_dotdot_escape() {
        let (dir, _) = setup_project();
        let result = resolve_tool_path("../", dir.path());
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("outside the working directory") || msg.contains("does not exist"),
            "expected boundary or not-found error, got: {}",
            msg
        );
    }

    #[test]
    fn resolve_rejects_nonexistent_path() {
        let (dir, _) = setup_project();
        let result = resolve_tool_path("ghost_file_that_never_exists.txt", dir.path());
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("does not exist"),
            "expected not-found error, got: {}",
            msg
        );
    }

    #[test]
    fn resolve_rejects_env_file() {
        let dir = TempDir::new().expect("tempdir");
        let env_file = dir.path().join(".env");
        fs::write(&env_file, "SECRET=hunter2").expect("write");
        let result = resolve_tool_path(env_file.to_str().unwrap(), dir.path());
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("protected") || msg.contains(".env"),
            "expected .env to be blocked, got: {}",
            msg
        );
    }

    #[test]
    fn resolve_rejects_dotenv_variants() {
        let dir = TempDir::new().expect("tempdir");
        for name in &[
            ".env.local",
            ".env.production",
            ".env.staging",
            ".env.override",
        ] {
            let f = dir.path().join(name);
            fs::write(&f, "SECRET=x").expect("write");
            let result = resolve_tool_path(f.to_str().unwrap(), dir.path());
            assert!(
                result.is_err(),
                "{} should be blocked but was allowed",
                name
            );
        }
    }

    #[test]
    fn resolve_rejects_ssh_private_keys() {
        let dir = TempDir::new().expect("tempdir");
        for name in &["id_rsa", "id_ed25519", "id_ecdsa", "id_dsa"] {
            let f = dir.path().join(name);
            fs::write(&f, "-----BEGIN RSA PRIVATE KEY-----").expect("write");
            let result = resolve_tool_path(f.to_str().unwrap(), dir.path());
            assert!(
                result.is_err(),
                "{} should be blocked but was allowed",
                name
            );
        }
    }

    #[test]
    fn resolve_rejects_credentials_file() {
        let dir = TempDir::new().expect("tempdir");
        let f = dir.path().join("credentials");
        fs::write(&f, "[default]\naws_secret_access_key = AKIA...").expect("write");
        let result = resolve_tool_path(f.to_str().unwrap(), dir.path());
        assert!(result.is_err(), "credentials should be blocked");
    }

    #[test]
    fn resolve_rejects_pem_extension() {
        let dir = TempDir::new().expect("tempdir");
        let f = dir.path().join("server.pem");
        fs::write(&f, "-----BEGIN CERTIFICATE-----").expect("write");
        let result = resolve_tool_path(f.to_str().unwrap(), dir.path());
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("protected"),
            "expected .pem to be blocked, got: {}",
            msg
        );
    }

    #[test]
    fn resolve_rejects_key_extension() {
        let dir = TempDir::new().expect("tempdir");
        let f = dir.path().join("private.key");
        fs::write(&f, "-----BEGIN PRIVATE KEY-----").expect("write");
        let result = resolve_tool_path(f.to_str().unwrap(), dir.path());
        assert!(result.is_err(), ".key files should be blocked");
    }

    #[test]
    fn resolve_rejects_p12_pfx_extensions() {
        let dir = TempDir::new().expect("tempdir");
        for name in &["cert.p12", "cert.pfx"] {
            let f = dir.path().join(name);
            fs::write(&f, "binary-cert-data").expect("write");
            let result = resolve_tool_path(f.to_str().unwrap(), dir.path());
            assert!(result.is_err(), "{} should be blocked", name);
        }
    }

    #[test]
    fn resolve_rejects_symlink_pointing_outside_working_dir() {
        let project = TempDir::new().expect("project tempdir");
        let outside = TempDir::new().expect("outside tempdir");
        let secret = outside.path().join("secret.txt");
        fs::write(&secret, "top secret").expect("write");
        let link = project.path().join("link.txt");
        symlink(&secret, &link).expect("symlink");
        let result = resolve_tool_path(link.to_str().unwrap(), project.path());
        assert!(
            result.is_err(),
            "symlink pointing outside working dir should be rejected"
        );
        let msg = result.unwrap_err();
        assert!(
            msg.contains("outside the working directory"),
            "expected boundary error, got: {}",
            msg
        );
    }

    #[test]
    fn resolve_allows_symlink_inside_working_dir() {
        let dir = TempDir::new().expect("tempdir");
        let target = dir.path().join("real.txt");
        fs::write(&target, "content").expect("write");
        let link = dir.path().join("link.txt");
        symlink(&target, &link).expect("symlink");
        let result = resolve_tool_path(link.to_str().unwrap(), dir.path());
        assert!(
            result.is_ok(),
            "symlink within working dir should be allowed: {:?}",
            result
        );
    }

    // ── execute_read_file ─────────────────────────────────────────────────────

    #[test]
    fn execute_reads_file_content_with_line_numbers() {
        let (dir, file) = setup_project();
        let input = json!({ "path": file.to_str().unwrap() });
        let output = execute_read_file(&input, dir.path());
        assert!(output.contains("<type>file</type>"), "should be file type");
        assert!(output.contains("1: line one"), "should have line 1");
        assert!(output.contains("2: line two"), "should have line 2");
        assert!(output.contains("3: line three"), "should have line 3");
    }

    #[test]
    fn execute_reads_file_with_offset() {
        let (dir, file) = setup_project();
        let input = json!({ "path": file.to_str().unwrap(), "offset": 2 });
        let output = execute_read_file(&input, dir.path());
        assert!(!output.contains("1: line one"), "line 1 should be skipped");
        assert!(output.contains("2: line two"), "should start at line 2");
    }

    #[test]
    fn execute_reads_file_with_limit() {
        let (dir, file) = setup_project();
        let input = json!({ "path": file.to_str().unwrap(), "limit": 1 });
        let output = execute_read_file(&input, dir.path());
        assert!(output.contains("1: line one"), "should have line 1");
        assert!(
            !output.contains("2: line two"),
            "line 2 should be cut by limit"
        );
        assert!(
            output.contains("Showing lines"),
            "should show truncation notice"
        );
    }

    #[test]
    fn execute_limit_is_capped_at_500() {
        let dir = TempDir::new().expect("tempdir");
        let content: String = (1..=600).map(|i| format!("line {}\n", i)).collect();
        let file = dir.path().join("big.txt");
        fs::write(&file, &content).expect("write");
        let input = json!({ "path": file.to_str().unwrap(), "limit": 9999 });
        let output = execute_read_file(&input, dir.path());
        assert!(
            !output.contains("501: line 501"),
            "limit should be capped at 500"
        );
        assert!(
            output.contains("500: line 500"),
            "line 500 should be present"
        );
    }

    #[test]
    fn execute_lists_directory_entries() {
        let dir = TempDir::new().expect("tempdir");
        fs::write(dir.path().join("alpha.txt"), "").expect("write");
        fs::write(dir.path().join("beta.txt"), "").expect("write");
        fs::create_dir(dir.path().join("subdir")).expect("mkdir");
        let input = json!({ "path": dir.path().to_str().unwrap() });
        let output = execute_read_file(&input, dir.path());
        assert!(output.contains("<type>directory</type>"), "should be directory type");
        assert!(output.contains("alpha.txt"), "should list alpha.txt");
        assert!(output.contains("beta.txt"), "should list beta.txt");
        assert!(output.contains("subdir/"), "subdirectory should have trailing slash");
    }

    #[test]
    fn execute_rejects_binary_extension() {
        let dir = TempDir::new().expect("tempdir");
        let f = dir.path().join("archive.zip");
        fs::write(&f, b"\x50\x4b\x03\x04").expect("write");
        let input = json!({ "path": f.to_str().unwrap() });
        let output = execute_read_file(&input, dir.path());
        assert!(
            output.starts_with("Error:"),
            "binary file should be rejected, got: {}",
            output
        );
        assert!(output.contains("binary"), "should mention binary, got: {}", output);
    }

    #[test]
    fn execute_returns_error_for_missing_path_param() {
        let dir = TempDir::new().expect("tempdir");
        let input = json!({ "offset": 1 });
        let output = execute_read_file(&input, dir.path());
        assert!(
            output.starts_with("Error:"),
            "missing path param should error, got: {}",
            output
        );
        assert!(output.contains("path"), "should mention missing 'path', got: {}", output);
    }

    #[test]
    fn execute_returns_error_for_nonexistent_file() {
        let dir = TempDir::new().expect("tempdir");
        let input = json!({ "path": dir.path().join("ghost.txt").to_str().unwrap() });
        let output = execute_read_file(&input, dir.path());
        assert!(
            output.starts_with("Error:"),
            "nonexistent file should error, got: {}",
            output
        );
    }

    #[test]
    fn execute_returns_error_for_secret_file() {
        let dir = TempDir::new().expect("tempdir");
        let env = dir.path().join(".env");
        fs::write(&env, "API_KEY=secret").expect("write");
        let input = json!({ "path": env.to_str().unwrap() });
        let output = execute_read_file(&input, dir.path());
        assert!(
            output.starts_with("Error:"),
            ".env should be blocked in execute, got: {}",
            output
        );
    }

    #[test]
    fn execute_returns_error_for_file_outside_working_dir() {
        let project = TempDir::new().expect("project tempdir");
        let other = TempDir::new().expect("other tempdir");
        let outside_file = other.path().join("secret.txt");
        fs::write(&outside_file, "sensitive").expect("write");
        let input = json!({ "path": outside_file.to_str().unwrap() });
        let output = execute_read_file(&input, project.path());
        assert!(
            output.starts_with("Error:"),
            "file outside working dir should error, got: {}",
            output
        );
        assert!(
            output.contains("outside") || output.contains("Access denied"),
            "expected boundary message, got: {}",
            output
        );
    }

    #[test]
    fn execute_end_of_file_notice_when_all_lines_shown() {
        let (dir, file) = setup_project();
        let input = json!({ "path": file.to_str().unwrap() });
        let output = execute_read_file(&input, dir.path());
        assert!(
            output.contains("End of file"),
            "should show end-of-file notice, got: {}",
            output
        );
    }

    // ── Deny-list coverage tests ──────────────────────────────────────────────

    #[test]
    fn denylist_env_exact() {
        let dir = TempDir::new().expect("tempdir");
        let f = dir.path().join(".env");
        fs::write(&f, "SECRET=x").unwrap();
        assert!(resolve_tool_path(f.to_str().unwrap(), dir.path()).is_err(), ".env must be blocked");
    }

    #[test]
    fn denylist_env_local() {
        let dir = TempDir::new().expect("tempdir");
        let f = dir.path().join(".env.local");
        fs::write(&f, "SECRET=x").unwrap();
        assert!(resolve_tool_path(f.to_str().unwrap(), dir.path()).is_err(), ".env.local must be blocked");
    }

    #[test]
    fn denylist_env_production() {
        let dir = TempDir::new().expect("tempdir");
        let f = dir.path().join(".env.production");
        fs::write(&f, "SECRET=x").unwrap();
        assert!(resolve_tool_path(f.to_str().unwrap(), dir.path()).is_err(), ".env.production must be blocked");
    }

    #[test]
    fn denylist_env_nested_in_subdir() {
        let dir = TempDir::new().expect("tempdir");
        let sub = dir.path().join("config");
        fs::create_dir(&sub).unwrap();
        let f = sub.join(".env");
        fs::write(&f, "SECRET=x").unwrap();
        assert!(resolve_tool_path(f.to_str().unwrap(), dir.path()).is_err(), "nested .env must be blocked");
    }

    #[test]
    fn denylist_env_variant_nested_in_subdir() {
        let dir = TempDir::new().expect("tempdir");
        let sub = dir.path().join("packages").join("app");
        fs::create_dir_all(&sub).unwrap();
        let f = sub.join(".env.staging");
        fs::write(&f, "SECRET=x").unwrap();
        assert!(resolve_tool_path(f.to_str().unwrap(), dir.path()).is_err(), "nested .env.staging must be blocked");
    }

    #[test]
    fn denylist_ssh_id_rsa_bare() {
        let dir = TempDir::new().expect("tempdir");
        let f = dir.path().join("id_rsa");
        fs::write(&f, "KEY").unwrap();
        assert!(resolve_tool_path(f.to_str().unwrap(), dir.path()).is_err(), "id_rsa must be blocked");
    }

    #[test]
    fn denylist_ssh_id_rsa_pub() {
        let dir = TempDir::new().expect("tempdir");
        let f = dir.path().join("id_rsa.pub");
        fs::write(&f, "ssh-rsa AAAA...").unwrap();
        assert!(resolve_tool_path(f.to_str().unwrap(), dir.path()).is_err(), "id_rsa.pub must be blocked");
    }

    #[test]
    fn denylist_ssh_id_rsa_with_suffix() {
        let dir = TempDir::new().expect("tempdir");
        let f = dir.path().join("id_rsa_backup");
        fs::write(&f, "KEY").unwrap();
        assert!(resolve_tool_path(f.to_str().unwrap(), dir.path()).is_err(), "id_rsa_backup must be blocked");
    }

    #[test]
    fn denylist_ssh_id_ed25519_bare() {
        let dir = TempDir::new().expect("tempdir");
        let f = dir.path().join("id_ed25519");
        fs::write(&f, "KEY").unwrap();
        assert!(resolve_tool_path(f.to_str().unwrap(), dir.path()).is_err(), "id_ed25519 must be blocked");
    }

    #[test]
    fn denylist_ssh_id_ed25519_pub() {
        let dir = TempDir::new().expect("tempdir");
        let f = dir.path().join("id_ed25519.pub");
        fs::write(&f, "ssh-ed25519 AAAA...").unwrap();
        assert!(resolve_tool_path(f.to_str().unwrap(), dir.path()).is_err(), "id_ed25519.pub must be blocked");
    }

    #[test]
    fn denylist_ssh_id_ecdsa_bare() {
        let dir = TempDir::new().expect("tempdir");
        let f = dir.path().join("id_ecdsa");
        fs::write(&f, "KEY").unwrap();
        assert!(resolve_tool_path(f.to_str().unwrap(), dir.path()).is_err(), "id_ecdsa must be blocked");
    }

    #[test]
    fn denylist_ssh_id_dsa_bare() {
        let dir = TempDir::new().expect("tempdir");
        let f = dir.path().join("id_dsa");
        fs::write(&f, "KEY").unwrap();
        assert!(resolve_tool_path(f.to_str().unwrap(), dir.path()).is_err(), "id_dsa must be blocked");
    }

    #[test]
    fn denylist_ssh_id_keys_nested_in_subdir() {
        let dir = TempDir::new().expect("tempdir");
        let sub = dir.path().join(".ssh");
        fs::create_dir(&sub).unwrap();
        let f = sub.join("id_rsa");
        fs::write(&f, "KEY").unwrap();
        assert!(resolve_tool_path(f.to_str().unwrap(), dir.path()).is_err(), "nested id_rsa must be blocked");
    }

    #[test]
    fn denylist_ssh_dir_known_hosts() {
        let dir = TempDir::new().expect("tempdir");
        let ssh = dir.path().join(".ssh");
        fs::create_dir(&ssh).unwrap();
        let f = ssh.join("known_hosts");
        fs::write(&f, "github.com ssh-rsa AAAA").unwrap();
        assert!(resolve_tool_path(f.to_str().unwrap(), dir.path()).is_err(), ".ssh/known_hosts must be blocked");
    }

    #[test]
    fn denylist_ssh_dir_authorized_keys() {
        let dir = TempDir::new().expect("tempdir");
        let ssh = dir.path().join(".ssh");
        fs::create_dir(&ssh).unwrap();
        let f = ssh.join("authorized_keys");
        fs::write(&f, "ssh-rsa AAAA").unwrap();
        assert!(resolve_tool_path(f.to_str().unwrap(), dir.path()).is_err(), ".ssh/authorized_keys must be blocked");
    }

    #[test]
    fn denylist_ssh_dir_config() {
        let dir = TempDir::new().expect("tempdir");
        let ssh = dir.path().join(".ssh");
        fs::create_dir(&ssh).unwrap();
        let f = ssh.join("config");
        fs::write(&f, "Host *").unwrap();
        assert!(resolve_tool_path(f.to_str().unwrap(), dir.path()).is_err(), ".ssh/config must be blocked");
    }

    #[test]
    fn denylist_aws_credentials() {
        let dir = TempDir::new().expect("tempdir");
        let aws = dir.path().join(".aws");
        fs::create_dir(&aws).unwrap();
        let f = aws.join("credentials");
        fs::write(&f, "[default]\naws_access_key_id = AKIA...").unwrap();
        assert!(resolve_tool_path(f.to_str().unwrap(), dir.path()).is_err(), ".aws/credentials must be blocked");
    }

    #[test]
    fn denylist_aws_config() {
        let dir = TempDir::new().expect("tempdir");
        let aws = dir.path().join(".aws");
        fs::create_dir(&aws).unwrap();
        let f = aws.join("config");
        fs::write(&f, "[default]\nregion = us-east-1").unwrap();
        assert!(resolve_tool_path(f.to_str().unwrap(), dir.path()).is_err(), ".aws/config must be blocked");
    }

    #[test]
    fn denylist_claude_settings_json() {
        let dir = TempDir::new().expect("tempdir");
        let claude = dir.path().join(".claude");
        fs::create_dir(&claude).unwrap();
        let f = claude.join("settings.json");
        fs::write(&f, r#"{"apiKey":"sk-..."}"#).unwrap();
        assert!(resolve_tool_path(f.to_str().unwrap(), dir.path()).is_err(), ".claude/settings.json must be blocked");
    }

    #[test]
    fn denylist_claude_any_file_in_claude_dir() {
        let dir = TempDir::new().expect("tempdir");
        let claude = dir.path().join(".claude");
        fs::create_dir(&claude).unwrap();
        let f = claude.join("custom_instructions.md");
        fs::write(&f, "some instructions").unwrap();
        assert!(resolve_tool_path(f.to_str().unwrap(), dir.path()).is_err(), ".claude/* must be blocked");
    }

    #[test]
    fn denylist_gnupg_private_key() {
        let dir = TempDir::new().expect("tempdir");
        let gnupg = dir.path().join(".gnupg");
        fs::create_dir(&gnupg).unwrap();
        let f = gnupg.join("secring.gpg");
        fs::write(&f, "GPG secret ring").unwrap();
        assert!(resolve_tool_path(f.to_str().unwrap(), dir.path()).is_err(), ".gnupg/* must be blocked");
    }

    #[test]
    fn denylist_env_like_name_does_not_block_normal_files() {
        let dir = TempDir::new().expect("tempdir");
        let f = dir.path().join("envelope.txt");
        fs::write(&f, "not a secret").unwrap();
        assert!(resolve_tool_path(f.to_str().unwrap(), dir.path()).is_ok(), "envelope.txt must NOT be blocked");
    }

    #[test]
    fn denylist_id_not_blocked_unless_ssh_prefix() {
        let dir = TempDir::new().expect("tempdir");
        let f = dir.path().join("identity.txt");
        fs::write(&f, "user info").unwrap();
        assert!(resolve_tool_path(f.to_str().unwrap(), dir.path()).is_ok(), "identity.txt must NOT be blocked");
    }

    #[test]
    fn denylist_config_in_non_sensitive_dir_is_allowed() {
        let dir = TempDir::new().expect("tempdir");
        let f = dir.path().join("config");
        fs::write(&f, "some config").unwrap();
        let result = resolve_tool_path(f.to_str().unwrap(), dir.path());
        assert!(result.is_err(), "bare 'config' filename is blocked (intentional trade-off)");
    }

    #[test]
    fn denylist_config_json_is_allowed() {
        let dir = TempDir::new().expect("tempdir");
        let f = dir.path().join("config.json");
        fs::write(&f, r#"{"port":3000}"#).unwrap();
        let result = resolve_tool_path(f.to_str().unwrap(), dir.path());
        assert!(result.is_ok(), "config.json in project root must be readable, got: {:?}", result);
    }

    #[test]
    fn denylist_readme_is_allowed() {
        let dir = TempDir::new().expect("tempdir");
        let f = dir.path().join("README.md");
        fs::write(&f, "# Project").unwrap();
        assert!(resolve_tool_path(f.to_str().unwrap(), dir.path()).is_ok(), "README.md must be readable");
    }
}
