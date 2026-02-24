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
                name: "nexus".into(),
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
