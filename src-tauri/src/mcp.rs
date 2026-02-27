// Re-export schemars so the JsonSchema derive macro can find it
use rmcp::schemars;

use rmcp::{
    handler::server::tool::ToolRouter, handler::server::wrapper::Parameters, model::*, tool,
    tool_handler, tool_router, transport::stdio, ErrorData as McpError, ServerHandler, ServiceExt,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ── Tool Parameter Types ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetCredentialParams {
    /// The provider name (e.g. "anthropic", "openai", "gemini")
    pub provider: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ReadSkillParams {
    /// The skill name (directory name under ~/.agents/skills/ or ~/.claude/skills/)
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ReadProjectParams {
    /// The project name as registered in Automatic
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SearchSkillsParams {
    /// Search query (skill name, topic, or keyword)
    pub query: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SyncProjectParams {
    /// The project name to sync configs for
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct StoreMemoryParams {
    /// The project name
    pub project: String,
    /// The memory key (identifier)
    pub key: String,
    /// The memory value to store
    pub value: String,
    /// Optional: identifier for the agent/tool storing this memory
    pub source: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetMemoryParams {
    /// The project name
    pub project: String,
    /// The memory key to retrieve
    pub key: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListMemoriesParams {
    /// The project name
    pub project: String,
    /// Optional: filter keys by this substring (case-insensitive)
    pub pattern: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SearchMemoriesParams {
    /// The project name
    pub project: String,
    /// Search query to match against keys and values
    pub query: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DeleteMemoryParams {
    /// The project name
    pub project: String,
    /// The memory key to delete
    pub key: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ClearMemoriesParams {
    /// The project name
    pub project: String,
    /// Optional: only delete memories with keys matching this pattern (case-insensitive)
    pub pattern: Option<String>,
    /// Must be set to true to confirm deletion
    pub confirm: bool,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Verify that `project` is a registered project name.
/// Returns `Ok(())` on success, or an `Err` with a helpful message listing
/// the valid project names so the agent can self-correct immediately.
fn validate_project(project: &str) -> Result<(), String> {
    let known = crate::core::list_projects().unwrap_or_default();
    if known.iter().any(|p| p == project) {
        Ok(())
    } else {
        let list = if known.is_empty() {
            "no projects registered yet".to_string()
        } else {
            known.join(", ")
        };
        Err(format!(
            "Unknown project '{}'. Valid project names are: {}. \
             Call automatic_list_projects to confirm the correct name before retrying.",
            project, list
        ))
    }
}

// ── MCP Server Handler ──────────────────────────────────────────────────────

#[derive(Clone)]
pub struct NexusMcpServer {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl NexusMcpServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    // ── Read-only tools ──────────────────────────────────────────────────

    #[tool(
        name = "automatic_get_credential",
        description = "Retrieve an API key for a given LLM provider stored in Nexus"
    )]
    async fn get_credential(
        &self,
        params: Parameters<GetCredentialParams>,
    ) -> Result<CallToolResult, McpError> {
        match crate::core::get_api_key(&params.0.provider) {
            Ok(key) => Ok(CallToolResult::success(vec![Content::text(key)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to retrieve credential for '{}': {}",
                params.0.provider, e
            ))])),
        }
    }

    #[tool(
        name = "automatic_list_skills",
        description = "List all available skill names from the Nexus skill registry"
    )]
    async fn list_skills(&self) -> Result<CallToolResult, McpError> {
        match crate::core::list_skills() {
            Ok(skills) => {
                let json =
                    serde_json::to_string_pretty(&skills).unwrap_or_else(|_| "[]".to_string());
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to list skills: {}",
                e
            ))])),
        }
    }

    #[tool(
        name = "automatic_read_skill",
        description = "Read the content of a specific skill from the Nexus skill registry"
    )]
    async fn read_skill(
        &self,
        params: Parameters<ReadSkillParams>,
    ) -> Result<CallToolResult, McpError> {
        match crate::core::read_skill(&params.0.name) {
            Ok(content) => Ok(CallToolResult::success(vec![Content::text(content)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to read skill '{}': {}",
                params.0.name, e
            ))])),
        }
    }

    #[tool(
        name = "automatic_list_mcp_servers",
        description = "List all MCP server configurations registered in the Nexus server registry"
    )]
    async fn list_mcp_servers(&self) -> Result<CallToolResult, McpError> {
        match crate::core::list_mcp_server_configs() {
            Ok(names) => {
                // Build a full config object with all server details
                let mut servers = serde_json::Map::new();
                for name in &names {
                    if let Ok(raw) = crate::core::read_mcp_server_config(name) {
                        if let Ok(config) = serde_json::from_str::<serde_json::Value>(&raw) {
                            servers.insert(name.clone(), config);
                        }
                    }
                }
                let result = serde_json::json!({ "mcpServers": servers });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string()),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to list MCP servers: {}",
                e
            ))])),
        }
    }

    // ── Project tools ────────────────────────────────────────────────────

    #[tool(
        name = "automatic_list_projects",
        description = "List all project names registered in Automatic"
    )]
    async fn list_projects(&self) -> Result<CallToolResult, McpError> {
        match crate::core::list_projects() {
            Ok(projects) => {
                let json = serde_json::to_string_pretty(&projects)
                    .unwrap_or_else(|_| "[]".to_string());
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to list projects: {}",
                e
            ))])),
        }
    }

    #[tool(
        name = "automatic_read_project",
        description = "Read the full configuration for a project (skills, MCP servers, agents, directory, description)"
    )]
    async fn read_project(
        &self,
        params: Parameters<ReadProjectParams>,
    ) -> Result<CallToolResult, McpError> {
        match crate::core::read_project(&params.0.name) {
            Ok(content) => Ok(CallToolResult::success(vec![Content::text(content)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to read project '{}': {}",
                params.0.name, e
            ))])),
        }
    }

    // ── Sessions tool ────────────────────────────────────────────────────

    #[tool(
        name = "automatic_list_sessions",
        description = "List active Claude Code sessions tracked by the Nexus hooks (session id, working directory, model, started_at)"
    )]
    async fn list_sessions(&self) -> Result<CallToolResult, McpError> {
        match crate::core::list_sessions() {
            Ok(json) => Ok(CallToolResult::success(vec![Content::text(json)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to list sessions: {}",
                e
            ))])),
        }
    }

    // ── Skills Store tool ────────────────────────────────────────────────

    #[tool(
        name = "automatic_search_skills",
        description = "Search the skills.sh registry for community skills matching a query. Returns skill names, install counts, and source repos."
    )]
    async fn search_skills(
        &self,
        params: Parameters<SearchSkillsParams>,
    ) -> Result<CallToolResult, McpError> {
        match crate::core::search_remote_skills(&params.0.query).await {
            Ok(results) => {
                let json = serde_json::to_string_pretty(&results)
                    .unwrap_or_else(|_| "[]".to_string());
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to search skills: {}",
                e
            ))])),
        }
    }

    // ── Config sync tool ─────────────────────────────────────────────────

    #[tool(
        name = "automatic_sync_project",
        description = "Sync a project's MCP server configs to its directory for all configured agent tools. The project must have a directory path and at least one agent tool configured."
    )]
    async fn sync_project(
        &self,
        params: Parameters<SyncProjectParams>,
    ) -> Result<CallToolResult, McpError> {
        let raw = match crate::core::read_project(&params.0.name) {
            Ok(r) => r,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read project '{}': {}",
                    params.0.name, e
                ))]));
            }
        };

        let project: crate::core::Project = match serde_json::from_str(&raw) {
            Ok(p) => p,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Invalid project data: {}",
                    e
                ))]));
            }
        };

        match crate::sync::sync_project(&project) {
            Ok(files) => {
                let response = serde_json::json!({
                    "synced_files": files,
                    "agents": project.agents,
                    "directory": project.directory,
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&response)
                        .unwrap_or_else(|_| format!("Synced {} files", files.len())),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Sync failed: {}",
                e
            ))])),
        }
    }

    // ── Memory tools ─────────────────────────────────────────────────────

    #[tool(
        name = "automatic_store_memory",
        description = "Stores a memory entry (key-value pair) for a project. AI agents can use this to persist learned information, preferences, or context over time."
    )]
    async fn store_memory(
        &self,
        params: Parameters<StoreMemoryParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Err(e) = validate_project(&params.0.project) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        match crate::memory::store_memory(
            &params.0.project,
            &params.0.key,
            &params.0.value,
            params.0.source.as_deref(),
        ) {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to store memory: {}",
                e
            ))])),
        }
    }

    #[tool(
        name = "automatic_get_memory",
        description = "Retrieves a specific memory entry by key for a project."
    )]
    async fn get_memory(
        &self,
        params: Parameters<GetMemoryParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Err(e) = validate_project(&params.0.project) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        match crate::memory::get_memory(&params.0.project, &params.0.key) {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to get memory: {}",
                e
            ))])),
        }
    }

    #[tool(
        name = "automatic_list_memories",
        description = "Lists all stored memories for a project, optionally filtered by a key pattern."
    )]
    async fn list_memories(
        &self,
        params: Parameters<ListMemoriesParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Err(e) = validate_project(&params.0.project) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        match crate::memory::list_memories(&params.0.project, params.0.pattern.as_deref()) {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to list memories: {}",
                e
            ))])),
        }
    }

    #[tool(
        name = "automatic_search_memories",
        description = "Searches memory keys and values for a query string (case-insensitive substring match)."
    )]
    async fn search_memories(
        &self,
        params: Parameters<SearchMemoriesParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Err(e) = validate_project(&params.0.project) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        match crate::memory::search_memories(&params.0.project, &params.0.query) {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to search memories: {}",
                e
            ))])),
        }
    }

    #[tool(
        name = "automatic_delete_memory",
        description = "Deletes a specific memory entry by key for a project."
    )]
    async fn delete_memory(
        &self,
        params: Parameters<DeleteMemoryParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Err(e) = validate_project(&params.0.project) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        match crate::memory::delete_memory(&params.0.project, &params.0.key) {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to delete memory: {}",
                e
            ))])),
        }
    }

    #[tool(
        name = "automatic_clear_memories",
        description = "Clears all memories for a project, optionally filtered by pattern. Use with caution!"
    )]
    async fn clear_memories(
        &self,
        params: Parameters<ClearMemoriesParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Err(e) = validate_project(&params.0.project) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        match crate::memory::clear_memories(&params.0.project, params.0.pattern.as_deref(), params.0.confirm) {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to clear memories: {}",
                e
            ))])),
        }
    }
}

#[tool_handler]
impl ServerHandler for NexusMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Nexus is a skill registry and MCP config hub. \
                 Use these tools to retrieve API keys, discover and search skills, list MCP \
                 server configs, inspect projects, track active sessions, and sync project \
                 configurations."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "automatic".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                title: None,
                description: None,
                icons: None,
                website_url: None,
            },
            ..Default::default()
        }
    }
}

// ── Entry Point ──────────────────────────────────────────────────────────────

pub async fn run_mcp_server() -> Result<(), Box<dyn std::error::Error>> {
    let server = NexusMcpServer::new();
    let service = server.serve(stdio()).await?;
    service.waiting().await?;

    Ok(())
}
