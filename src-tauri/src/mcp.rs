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

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ReadClaudeMemoryParams {
    /// The project name as registered in Automatic
    pub project: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetProjectContextParams {
    /// The project name as registered in Automatic
    pub project: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetRelatedProjectsParams {
    /// The project name as registered in Automatic
    pub project: String,
}

// ── Feature Tool Parameter Types ─────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListFeaturesParams {
    /// The project name as registered in Automatic
    pub project: String,
    /// Optional state filter: backlog, todo, in_progress, review, complete, or cancelled
    pub state: Option<String>,
    /// When true, returns only archived features. Defaults to false (active features only).
    pub include_archived: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ArchiveFeatureParams {
    /// The project name as registered in Automatic
    pub project: String,
    /// The feature UUID to archive
    pub feature_id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct UnarchiveFeatureParams {
    /// The project name as registered in Automatic
    pub project: String,
    /// The feature UUID to unarchive
    pub feature_id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetFeatureParams {
    /// The project name as registered in Automatic
    pub project: String,
    /// The feature UUID
    pub feature_id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CreateFeatureParams {
    /// The project name as registered in Automatic
    pub project: String,
    /// Short title for the feature (required)
    pub title: String,
    /// Markdown description of the work to be done
    pub description: Option<String>,
    /// Priority: low, medium (default), or high
    pub priority: Option<String>,
    /// Agent id or name to assign this feature to
    pub assignee: Option<String>,
    /// List of searchable tags
    pub tags: Option<Vec<String>>,
    /// List of file paths in the project this feature relates to
    pub linked_files: Option<Vec<String>>,
    /// Effort estimate: xs, s, m, l, or xl
    pub effort: Option<String>,
    /// Identifier for the agent or tool creating this feature
    pub created_by: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct UpdateFeatureParams {
    /// The project name as registered in Automatic
    pub project: String,
    /// The feature UUID
    pub feature_id: String,
    /// New title (omit to leave unchanged)
    pub title: Option<String>,
    /// New markdown description (omit to leave unchanged)
    pub description: Option<String>,
    /// New priority: low, medium, or high (omit to leave unchanged)
    pub priority: Option<String>,
    /// New assignee (omit to leave unchanged, pass null to clear)
    pub assignee: Option<String>,
    /// New tags list (omit to leave unchanged)
    pub tags: Option<Vec<String>>,
    /// New linked files list (omit to leave unchanged)
    pub linked_files: Option<Vec<String>>,
    /// New effort: xs, s, m, l, or xl (omit to leave unchanged, pass null to clear)
    pub effort: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SetFeatureStateParams {
    /// The project name as registered in Automatic
    pub project: String,
    /// The feature UUID
    pub feature_id: String,
    /// New state: backlog, todo, in_progress, review, complete, or cancelled
    pub state: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DeleteFeatureParams {
    /// The project name as registered in Automatic
    pub project: String,
    /// The feature UUID to delete permanently
    pub feature_id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AddFeatureUpdateParams {
    /// The project name as registered in Automatic
    pub project: String,
    /// The feature UUID to add an update to
    pub feature_id: String,
    /// Markdown content of the progress update
    pub content: String,
    /// Agent id or name authoring this update
    pub author: Option<String>,
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
pub struct AutomaticMcpServer {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl AutomaticMcpServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    // ── Read-only tools ──────────────────────────────────────────────────

    #[tool(
        name = "automatic_get_credential",
        description = "Retrieve an API key for a given LLM provider stored in Automatic"
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
        description = "List all available skill names from the Automatic skill registry"
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
        description = "Read the content of a specific skill from the Automatic skill registry"
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
        description = "List all MCP server configurations registered in the Automatic server registry"
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

    #[tool(
        name = "automatic_get_related_projects",
        description = "Return all projects related to the given project via Project Groups, \
                       including each peer's name, description, directory, and relative path \
                       from this project's directory. Use this to discover sibling projects \
                       you can explore or reference."
    )]
    async fn get_related_projects(
        &self,
        params: Parameters<GetRelatedProjectsParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Err(e) = validate_project(&params.0.project) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }

        // Load the requesting project to get its directory for relative-path computation.
        let this_dir = match crate::core::read_project(&params.0.project) {
            Ok(raw) => serde_json::from_str::<crate::core::Project>(&raw)
                .map(|p| p.directory)
                .unwrap_or_default(),
            Err(_) => String::new(),
        };

        // Find every group this project belongs to.
        let groups = crate::core::groups_for_project(&params.0.project);

        if groups.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "This project does not belong to any groups and has no related projects.",
            )]));
        }

        // Collect unique peer project names across all groups, avoiding duplicates.
        let mut seen = std::collections::HashSet::new();
        let mut output = String::new();

        output.push_str("## Related Projects\n");
        output.push_str("The following projects are related to this one. They are provided for context — explore or reference them when relevant to the current task.\n\n");

        for group in &groups {
            output.push_str(&format!("### {}\n", group.name));
            if !group.description.trim().is_empty() {
                output.push_str(group.description.trim());
                output.push('\n');
            }

            let peers: Vec<&String> = group
                .projects
                .iter()
                .filter(|p| p.as_str() != params.0.project)
                .collect();

            if peers.is_empty() {
                output.push_str("No other projects in this group yet.\n");
            } else {
                for peer_name in peers {
                    if !seen.insert(peer_name.clone()) {
                        continue; // already included from another group
                    }
                    let peer_project = crate::core::read_project(peer_name)
                        .ok()
                        .and_then(|raw| serde_json::from_str::<crate::core::Project>(&raw).ok());

                    let (peer_desc, peer_dir) = peer_project
                        .map(|p| (p.description, p.directory))
                        .unwrap_or_default();

                    let rel_path = crate::core::compute_relative_path(&this_dir, &peer_dir);

                    let mut entry = format!("**{}**", peer_name);
                    if !peer_desc.trim().is_empty() {
                        entry.push_str(&format!(": {}", peer_desc.trim()));
                    }
                    if !rel_path.is_empty() {
                        entry.push_str(&format!("\nLocation: `{}`", rel_path));
                    }
                    if !peer_dir.is_empty() {
                        entry.push_str(&format!("\nAbsolute path: `{}`", peer_dir));
                    }
                    output.push_str(&entry);
                    output.push('\n');
                }
            }
            output.push('\n');
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        name = "automatic_get_project_context",
        description = "Read the project context for a registered project. Returns commands, entry points, \
                       architecture concepts, conventions, gotchas, and a documentation index merged from \
                       .automatic/context.json and .automatic/docs.json in the project directory. Returns \
                       an empty context (all sections present but empty) when the files do not exist yet."
    )]
    async fn get_project_context(
        &self,
        params: Parameters<GetProjectContextParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Err(e) = validate_project(&params.0.project) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }

        let project_json = match crate::core::read_project(&params.0.project) {
            Ok(j) => j,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read project '{}': {}",
                    params.0.project, e
                ))]));
            }
        };

        let project: crate::core::Project = match serde_json::from_str(&project_json) {
            Ok(p) => p,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to parse project data: {}",
                    e
                ))]));
            }
        };

        match crate::context::get_project_context(&project.directory) {
            Ok(ctx) => {
                // Build a brief plain-text summary first so the agent immediately
                // knows what sections are populated, followed by the full JSON.
                let mut summary = format!("# Project context for '{}'\n\n", params.0.project);

                let cmd_count = ctx.commands.len();
                let ep_count = ctx.entry_points.len();
                let concept_count = ctx.concepts.len();
                let conv_count = ctx.conventions.len();
                let gotcha_count = ctx.gotchas.len();
                let doc_count = ctx.docs.len();

                if cmd_count + ep_count + concept_count + conv_count + gotcha_count + doc_count == 0 {
                    summary.push_str("No context defined yet (.automatic/context.json is absent or empty).\n");
                } else {
                    summary.push_str(&format!(
                        "commands: {cmd_count}, entry_points: {ep_count}, concepts: {concept_count}, \
                         conventions: {conv_count}, gotchas: {gotcha_count}, docs: {doc_count}\n\n"
                    ));
                    summary.push_str("## Full context\n\n");
                    match serde_json::to_string_pretty(&ctx) {
                        Ok(json) => summary.push_str(&json),
                        Err(e) => summary.push_str(&format!("(serialisation error: {})", e)),
                    }
                }

                Ok(CallToolResult::success(vec![Content::text(summary)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to load project context for '{}': {}",
                params.0.project, e
            ))])),
        }
    }

    // ── Sessions tool ────────────────────────────────────────────────────

    #[tool(
        name = "automatic_list_sessions",
        description = "List active Claude Code sessions tracked by the Automatic hooks (session id, working directory, model, started_at)"
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

    // ── Claude auto-memory integration ────────────────────────────────────

    #[tool(
        name = "automatic_read_claude_memory",
        description = "Reads Claude Code's auto-memory files for a project (MEMORY.md index and any topic files). \
                       Claude Code stores learnings it discovers during sessions in ~/.claude/projects/<encoded-path>/memory/. \
                       Use this to inspect what Claude has learned, then call automatic_store_memory to promote \
                       important entries into Automatic's structured memory store."
    )]
    async fn read_claude_memory(
        &self,
        params: Parameters<ReadClaudeMemoryParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Err(e) = validate_project(&params.0.project) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }

        // Look up the project's directory
        let project_json = match crate::core::read_project(&params.0.project) {
            Ok(j) => j,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read project '{}': {}",
                    params.0.project, e
                ))]));
            }
        };

        let project: crate::core::Project = match serde_json::from_str(&project_json) {
            Ok(p) => p,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to parse project data: {}",
                    e
                ))]));
            }
        };

        match crate::memory::read_claude_memory(&project.directory) {
            Ok(content) => {
                let mut output = format!(
                    "# Claude Auto-Memory for '{}'\n\nDirectory: {}\n\n",
                    params.0.project, content.memory_dir
                );

                match &content.memory_md {
                    Some(md) => {
                        output.push_str("## MEMORY.md\n\n");
                        output.push_str(md);
                        output.push('\n');
                    }
                    None => {
                        output.push_str("MEMORY.md does not exist yet — Claude has not written any auto-memory for this project.\n");
                    }
                }

                if !content.topic_files.is_empty() {
                    output.push_str(&format!(
                        "\n## Topic files ({} found)\n\n",
                        content.topic_files.len()
                    ));
                    for file in &content.topic_files {
                        output.push_str(&format!("### {}\n\n{}\n\n", file.name, file.content));
                    }
                }

                Ok(CallToolResult::success(vec![Content::text(output)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to read Claude auto-memory: {}",
                e
            ))])),
        }
    }

    // ── Feature tools ─────────────────────────────────────────────────────

    #[tool(
        name = "automatic_list_features",
        description = "List all features for a project. By default returns only active (non-archived) features grouped by state with id, title, priority, effort, and assignee. Optionally filter by state: backlog, todo, in_progress, review, complete, or cancelled. Pass include_archived: true to list archived features instead of active ones."
    )]
    async fn list_features(
        &self,
        params: Parameters<ListFeaturesParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Err(e) = validate_project(&params.0.project) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        let include_archived = params.0.include_archived.unwrap_or(false);
        match crate::features::list_features(
            &params.0.project,
            params.0.state.as_deref(),
            include_archived,
        ) {
            Ok(features) => {
                let output = crate::features::format_features_markdown(
                    &features,
                    &params.0.project,
                    include_archived,
                );
                Ok(CallToolResult::success(vec![Content::text(output)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to list features: {}",
                e
            ))])),
        }
    }

    #[tool(
        name = "automatic_get_feature",
        description = "Get full detail for a specific feature by id, including description and all update history."
    )]
    async fn get_feature(
        &self,
        params: Parameters<GetFeatureParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Err(e) = validate_project(&params.0.project) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        match crate::features::get_feature_with_updates(&params.0.project, &params.0.feature_id) {
            Ok(fw) => {
                let output = crate::features::format_feature_detail_markdown(&fw);
                Ok(CallToolResult::success(vec![Content::text(output)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to get feature: {}",
                e
            ))])),
        }
    }

    #[tool(
        name = "automatic_create_feature",
        description = "Create a new feature in a project's backlog. Returns the created feature including its id, which you will need for subsequent calls."
    )]
    async fn create_feature(
        &self,
        params: Parameters<CreateFeatureParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Err(e) = validate_project(&params.0.project) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        let p = params.0;
        match crate::features::create_feature(
            &p.project,
            &p.title,
            p.description.as_deref().unwrap_or(""),
            p.priority.as_deref().unwrap_or("medium"),
            p.assignee.as_deref(),
            p.tags.as_deref().unwrap_or(&[]),
            p.linked_files.as_deref().unwrap_or(&[]),
            p.effort.as_deref(),
            p.created_by.as_deref(),
        ) {
            Ok(feature) => {
                let output = format!(
                    "Feature created successfully.\n\n**ID:** `{}`\n**Title:** {}\n**State:** backlog\n**Priority:** {}\n",
                    feature.id, feature.title, feature.priority
                );
                Ok(CallToolResult::success(vec![Content::text(output)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to create feature: {}",
                e
            ))])),
        }
    }

    #[tool(
        name = "automatic_update_feature",
        description = "Update a feature's metadata fields (title, description, priority, assignee, tags, linked_files, effort). Omit any field to leave it unchanged."
    )]
    async fn update_feature(
        &self,
        params: Parameters<UpdateFeatureParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Err(e) = validate_project(&params.0.project) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        let p = params.0;
        let patch = crate::features::FeaturePatch {
            title: p.title,
            description: p.description,
            state: None,
            priority: p.priority,
            // MCP passes Option<String>; None means unchanged, Some(v) sets it.
            // There's no way to clear via this tool — use update_feature for that.
            assignee: p.assignee.map(Some),
            tags: p.tags,
            linked_files: p.linked_files,
            effort: p.effort.map(Some),
            // Archiving is not exposed via this tool; use archive/unarchive tools instead.
            archived: None,
        };
        match crate::features::update_feature(&p.project, &p.feature_id, patch) {
            Ok(feature) => {
                let output = format!(
                    "Feature updated successfully.\n\n**ID:** `{}`\n**Title:** {}\n**State:** {}\n**Priority:** {}\n",
                    feature.id, feature.title, feature.state, feature.priority
                );
                Ok(CallToolResult::success(vec![Content::text(output)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to update feature: {}",
                e
            ))])),
        }
    }

    #[tool(
        name = "automatic_set_feature_state",
        description = "Change a feature's lifecycle state. Valid states: backlog, todo, in_progress, review, complete, cancelled. The feature is placed at the end of the target state column."
    )]
    async fn set_feature_state(
        &self,
        params: Parameters<SetFeatureStateParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Err(e) = validate_project(&params.0.project) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        match crate::features::set_feature_state(
            &params.0.project,
            &params.0.feature_id,
            &params.0.state,
        ) {
            Ok(feature) => {
                let output = format!(
                    "Feature state updated.\n\n**ID:** `{}`\n**Title:** {}\n**New state:** {}\n",
                    feature.id, feature.title, feature.state
                );
                Ok(CallToolResult::success(vec![Content::text(output)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to set feature state: {}",
                e
            ))])),
        }
    }

    #[tool(
        name = "automatic_delete_feature",
        description = "Permanently delete a feature and all its updates. This cannot be undone."
    )]
    async fn delete_feature(
        &self,
        params: Parameters<DeleteFeatureParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Err(e) = validate_project(&params.0.project) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        match crate::features::delete_feature(&params.0.project, &params.0.feature_id) {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Feature '{}' deleted from project '{}'.",
                params.0.feature_id, params.0.project
            ))])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to delete feature: {}",
                e
            ))])),
        }
    }

    #[tool(
        name = "automatic_archive_feature",
        description = "Archive a feature, hiding it from the Kanban board and default list views. The feature's state is preserved so it can be restored to its original column when unarchived."
    )]
    async fn archive_feature(
        &self,
        params: Parameters<ArchiveFeatureParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Err(e) = validate_project(&params.0.project) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        match crate::features::archive_feature(&params.0.project, &params.0.feature_id) {
            Ok(feature) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Feature '{}' archived. State '{}' is preserved for later restoration.",
                feature.title, feature.state
            ))])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to archive feature: {}",
                e
            ))])),
        }
    }

    #[tool(
        name = "automatic_unarchive_feature",
        description = "Unarchive a feature, restoring it to its preserved state in the Kanban board and default list views."
    )]
    async fn unarchive_feature(
        &self,
        params: Parameters<UnarchiveFeatureParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Err(e) = validate_project(&params.0.project) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        match crate::features::unarchive_feature(&params.0.project, &params.0.feature_id) {
            Ok(feature) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Feature '{}' unarchived and restored to state '{}'.",
                feature.title, feature.state
            ))])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to unarchive feature: {}",
                e
            ))])),
        }
    }

    #[tool(
        name = "automatic_add_feature_update",
        description = "Append a markdown progress update to a feature. Use this to log decisions, blockers, or progress notes. Updates are append-only and ordered newest-first."
    )]
    async fn add_feature_update(
        &self,
        params: Parameters<AddFeatureUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Err(e) = validate_project(&params.0.project) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        match crate::features::add_feature_update(
            &params.0.project,
            &params.0.feature_id,
            &params.0.content,
            params.0.author.as_deref(),
        ) {
            Ok(update) => {
                let output = format!(
                    "Update added to feature '{}'.\n\n**Update ID:** {}\n**Timestamp:** {}\n**Author:** {}\n",
                    params.0.feature_id,
                    update.id,
                    update.timestamp,
                    update.author.as_deref().unwrap_or("unknown")
                );
                Ok(CallToolResult::success(vec![Content::text(output)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to add feature update: {}",
                e
            ))])),
        }
    }
}

#[tool_handler]
impl ServerHandler for AutomaticMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Automatic is a desktop hub for AI coding agents. \
                 Use these tools to retrieve API keys, discover and search skills, list MCP \
                 server configs, inspect projects, track active sessions, and sync project \
                 configurations."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "automatic".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                title: Some("Automatic".into()),
                description: Some("Desktop hub for AI coding agents — skills, MCP configs, and project management".into()),
                icons: None,
                website_url: Some("https://github.com/anomalyco/automatic".into()),
            },
            ..Default::default()
        }
    }
}

// ── Entry Point ──────────────────────────────────────────────────────────────

pub async fn run_mcp_server() -> Result<(), Box<dyn std::error::Error>> {
    let server = AutomaticMcpServer::new();
    let service = server.serve(stdio()).await?;
    service.waiting().await?;

    Ok(())
}
