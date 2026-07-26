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
    /// The skill name (directory name in Automatic's managed library at
    /// ~/.automatic/library/skills/, or in an external scan location such as
    /// ~/.agents/skills/ or ~/.claude/skills/)
    pub name: String,
    /// Optional project name. When provided, project-local skills are searched first
    /// before falling back to the managed library and external scan locations.
    pub project: Option<String>,
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

// ── Rule Tool Parameter Types ────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ReadRuleParams {
    /// The rule's machine name (lowercase letters, digits, and hyphens; must
    /// start with a letter; no consecutive or trailing hyphens).
    pub machine_name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CreateRuleParams {
    /// The rule's machine name (lowercase letters, digits, and hyphens; must
    /// start with a letter; no consecutive or trailing hyphens). Must not
    /// already exist — use `automatic_update_rule` to modify an existing rule.
    pub machine_name: String,
    /// Human-readable display name shown in the Automatic UI.
    pub name: String,
    /// Markdown content of the rule.
    pub content: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct UpdateRuleParams {
    /// The rule's machine name. Must already exist.
    pub machine_name: String,
    /// New display name. Omit to leave the current name unchanged.
    pub name: Option<String>,
    /// New markdown content. Omit to leave the current content unchanged.
    pub content: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DeleteRuleParams {
    /// The rule's machine name. Mandatory rules (e.g. `automatic-service`)
    /// and plugin-provided rules cannot be deleted and will return an error.
    pub machine_name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AttachRuleParams {
    /// The project name as registered in Automatic.
    pub project: String,
    /// The rule's machine name. Must already exist in the library.
    pub machine_name: String,
    /// Target instruction file key. Use a filename like `"CLAUDE.md"` or
    /// `"AGENTS.md"` to attach the rule to a single agent's instruction file.
    /// Use `"_project"` to inject the rule into every agent file. Omit when
    /// the project is in unified instruction mode — `"_unified"` is used
    /// automatically. In per-agent mode the field is required.
    pub file: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DetachRuleParams {
    /// The project name as registered in Automatic.
    pub project: String,
    /// The rule's machine name. Mandatory rules cannot be detached.
    pub machine_name: String,
    /// Target instruction file key. Same semantics as `automatic_attach_rule`:
    /// optional in unified mode, required in per-agent mode.
    pub file: Option<String>,
}

// ── Hook Tool Parameter Types ────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ReadHookParams {
    /// The hook's machine name (lowercase letters, digits, and hyphens; must
    /// start with a letter; no consecutive or trailing hyphens).
    pub machine_name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CreateHookParams {
    /// The hook's machine name. Must not already exist.
    pub machine_name: String,
    /// Human-readable display name shown in the Automatic UI.
    pub name: String,
    /// Target agent id (e.g. `"claude"`, `"codex"`).
    pub agent: String,
    /// Lifecycle event name. Accepted values depend on the target agent —
    /// e.g. Claude Code supports `"SessionStart"`, `"PreToolUse"`,
    /// `"PostToolUse"`, `"Stop"`, etc.; Codex CLI supports a smaller subset.
    pub event: String,
    /// Optional matcher (e.g. tool name regex for `PreToolUse`/`PostToolUse`).
    pub matcher: Option<String>,
    /// The handler that runs when the event fires. Must be one of the
    /// `HookHandler` variants serialised with a `kind` discriminator:
    /// `{ "kind": "command", "command": "echo hi" }` or
    /// `{ "kind": "script", "interpreter": "bash", "script": "..." }`.
    pub handler: serde_json::Value,
    /// Optional timeout in seconds.
    pub timeout_sec: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct UpdateHookParams {
    /// The hook's machine name. Must already exist.
    pub machine_name: String,
    /// New display name. Omit to leave unchanged.
    pub name: Option<String>,
    /// New target agent. Omit to leave unchanged.
    pub agent: Option<String>,
    /// New event. Omit to leave unchanged.
    pub event: Option<String>,
    /// New matcher. Omit to leave unchanged. Pass `null` to clear.
    pub matcher: Option<serde_json::Value>,
    /// New handler JSON (same shape as `automatic_create_hook`). Omit to
    /// leave unchanged.
    pub handler: Option<serde_json::Value>,
    /// New timeout. Omit to leave unchanged. Pass `null` to clear.
    pub timeout_sec: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DeleteHookParams {
    /// The hook's machine name. Plugin-provided hooks cannot be deleted.
    pub machine_name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AttachHookParams {
    /// The project name as registered in Automatic.
    pub project: String,
    /// The hook's machine name. Must already exist in the library.
    pub machine_name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DetachHookParams {
    /// The project name as registered in Automatic.
    pub project: String,
    /// The hook's machine name.
    pub machine_name: String,
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

/// Resolve the `file_rules` key for a rule attach/detach operation.
///
/// Returns the explicit `file` parameter when provided; otherwise falls back
/// to `"_unified"` in unified instruction mode, or errors with a list of the
/// project's existing keys in per-agent mode so the caller can pick one.
fn resolve_file_rules_key(
    project: &crate::core::Project,
    file: Option<&str>,
) -> Result<String, String> {
    if let Some(key) = file {
        return Ok(key.to_string());
    }
    if project.instruction_mode == "unified" {
        return Ok("_unified".to_string());
    }
    let existing: Vec<&String> = project.file_rules.keys().collect();
    if existing.is_empty() {
        Err(format!(
            "Project '{}' is in per-agent instruction mode and has no \
             file_rules entries yet. Specify the `file` parameter — pass a \
             filename like \"CLAUDE.md\" or \"AGENTS.md\", or \"_project\" \
             to inject the rule into every agent file.",
            project.name
        ))
    } else {
        let keys = existing
            .iter()
            .map(|k| format!("\"{}\"", k))
            .collect::<Vec<_>>()
            .join(", ");
        Err(format!(
            "Project '{}' is in per-agent instruction mode — specify the \
             `file` parameter. Existing keys for this project: {}.",
            project.name, keys
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
        description = "Retrieve an API key for a given LLM provider stored in Automatic. \
                       Only recognised provider IDs are accepted (e.g. anthropic, openai)."
    )]
    async fn get_credential(
        &self,
        params: Parameters<GetCredentialParams>,
    ) -> Result<CallToolResult, McpError> {
        let provider = &params.0.provider;
        let known = crate::core::agents::known_agents();
        if !known.iter().any(|id| id.as_str() == provider) {
            let valid: Vec<&str> = known.iter().map(|id| id.as_str()).collect();
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Unknown provider '{}'. Valid providers: {}",
                provider,
                valid.join(", ")
            ))]));
        }
        match crate::core::get_api_key(provider) {
            Ok(key) => Ok(CallToolResult::success(vec![Content::text(key)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to retrieve credential for '{}': {}",
                provider, e
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
        description = "Read the content of a specific skill. Pass `project` to also search \
                       project-local skills (skills that exist only within a project directory, \
                       not in the global registry). When `project` is given, local skills are \
                       checked first; the global registry is used as a fallback."
    )]
    async fn read_skill(
        &self,
        params: Parameters<ReadSkillParams>,
    ) -> Result<CallToolResult, McpError> {
        let name = &params.0.name;

        // When a project is specified, try project-scoped custom skills first.
        // Their content lives inline in the project JSON, so no disk read is needed.
        if let Some(ref project_name) = params.0.project {
            if let Ok(raw) = crate::core::read_project(project_name) {
                if let Ok(project) = serde_json::from_str::<crate::core::Project>(&raw) {
                    if let Some(custom) = project
                        .custom_skills
                        .as_ref()
                        .and_then(|skills| skills.iter().find(|s| s.name == *name))
                    {
                        return Ok(CallToolResult::success(vec![Content::text(
                            custom.content.clone(),
                        )]));
                    }
                }
            }
        }

        // Fall back to the global registry.
        match crate::core::read_skill(name) {
            Ok(content) => Ok(CallToolResult::success(vec![Content::text(content)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to read skill '{}': {}",
                name, e
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
                let json =
                    serde_json::to_string_pretty(&projects).unwrap_or_else(|_| "[]".to_string());
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
                       architecture concepts, conventions, gotchas, a documentation index merged from \
                       .automatic/context.json and .automatic/docs.json, and the list of rules currently \
                       attached to each instruction file. Returns an empty context (all sections present \
                       but empty) when the source files do not exist yet."
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

                if cmd_count + ep_count + concept_count + conv_count + gotcha_count + doc_count == 0
                {
                    summary.push_str(
                        "No context defined yet (.automatic/context.json is absent or empty).\n\n",
                    );
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
                    summary.push_str("\n\n");
                }

                // Attached rules — sourced from project.file_rules so agents
                // can discover which rules are active without an extra call.
                summary.push_str("## Attached rules\n\n");
                if project.file_rules.is_empty() {
                    summary.push_str("No rules attached to this project.\n");
                } else {
                    let mut keys: Vec<&String> = project.file_rules.keys().collect();
                    keys.sort();
                    for key in keys {
                        let rules = &project.file_rules[key];
                        if rules.is_empty() {
                            continue;
                        }
                        summary.push_str(&format!("- `{}`: {}\n", key, rules.join(", ")));
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

    // ── Rules tools ──────────────────────────────────────────────────────

    #[tool(
        name = "automatic_list_rules",
        description = "List every rule in the Automatic library. Returns an \
                       array of objects with `id` (machine name), `name` \
                       (display name), and optional `plugin_id` for \
                       plugin-provided rules that cannot be deleted."
    )]
    async fn list_rules(&self) -> Result<CallToolResult, McpError> {
        match crate::core::list_rules() {
            Ok(rules) => {
                let json =
                    serde_json::to_string_pretty(&rules).unwrap_or_else(|_| "[]".to_string());
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to list rules: {}",
                e
            ))])),
        }
    }

    #[tool(
        name = "automatic_read_rule",
        description = "Read a rule by machine name. Returns the full rule \
                       JSON (`name`, `content`, optional `plugin_id`)."
    )]
    async fn read_rule(
        &self,
        params: Parameters<ReadRuleParams>,
    ) -> Result<CallToolResult, McpError> {
        match crate::core::read_rule(&params.0.machine_name) {
            Ok(content) => Ok(CallToolResult::success(vec![Content::text(content)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to read rule '{}': {}",
                params.0.machine_name, e
            ))])),
        }
    }

    #[tool(
        name = "automatic_create_rule",
        description = "Create a new rule in the Automatic library. Fails if a \
                       rule with the same machine name already exists — use \
                       `automatic_update_rule` to modify an existing rule. \
                       Machine name must be lowercase letters, digits, and \
                       hyphens only, starting with a letter."
    )]
    async fn create_rule(
        &self,
        params: Parameters<CreateRuleParams>,
    ) -> Result<CallToolResult, McpError> {
        let machine_name = &params.0.machine_name;

        if crate::core::read_rule(machine_name).is_ok() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Rule '{}' already exists. Use automatic_update_rule to modify it.",
                machine_name
            ))]));
        }

        match crate::core::save_rule(machine_name, &params.0.name, &params.0.content) {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Created rule '{}'.",
                machine_name
            ))])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to create rule '{}': {}",
                machine_name, e
            ))])),
        }
    }

    #[tool(
        name = "automatic_update_rule",
        description = "Update an existing rule's display name and/or content. \
                       Fails if the rule does not exist or is provided by a \
                       plugin. Omit a field to leave it unchanged; at least \
                       one of `name` or `content` must be provided."
    )]
    async fn update_rule(
        &self,
        params: Parameters<UpdateRuleParams>,
    ) -> Result<CallToolResult, McpError> {
        let machine_name = &params.0.machine_name;

        if params.0.name.is_none() && params.0.content.is_none() {
            return Ok(CallToolResult::error(vec![Content::text(
                "Provide at least one of `name` or `content` to update.".to_string(),
            )]));
        }

        // Load the existing rule so unspecified fields are preserved and we
        // can refuse plugin-owned rules with a clear message.
        let existing_raw = match crate::core::read_rule(machine_name) {
            Ok(raw) => raw,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Cannot update rule '{}': {}",
                    machine_name, e
                ))]));
            }
        };
        let existing: crate::core::Rule = match serde_json::from_str(&existing_raw) {
            Ok(rule) => rule,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to parse existing rule '{}': {}",
                    machine_name, e
                ))]));
            }
        };

        if existing.plugin_id.is_some() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Cannot update rule '{}' — it is provided by a plugin.",
                machine_name
            ))]));
        }

        let new_name = params.0.name.as_deref().unwrap_or(&existing.name);
        let new_content = params.0.content.as_deref().unwrap_or(&existing.content);

        match crate::core::save_rule(machine_name, new_name, new_content) {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Updated rule '{}'.",
                machine_name
            ))])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to update rule '{}': {}",
                machine_name, e
            ))])),
        }
    }

    #[tool(
        name = "automatic_delete_rule",
        description = "Delete a rule from the Automatic library. Mandatory \
                       rules (e.g. `automatic-service`) and plugin-provided \
                       rules cannot be deleted. Note that this removes the \
                       rule from the library but does not detach it from any \
                       project — projects referencing the deleted rule will \
                       silently skip it on next sync."
    )]
    async fn delete_rule(
        &self,
        params: Parameters<DeleteRuleParams>,
    ) -> Result<CallToolResult, McpError> {
        let machine_name = &params.0.machine_name;
        match crate::core::delete_rule(machine_name) {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Deleted rule '{}'.",
                machine_name
            ))])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to delete rule '{}': {}",
                machine_name, e
            ))])),
        }
    }

    #[tool(
        name = "automatic_attach_rule",
        description = "Attach a rule to a project's instruction file so the \
                       rule's content is injected on next sync. Idempotent — \
                       attaching an already-attached rule reports success. \
                       Does not trigger a sync; call automatic_sync_project \
                       afterwards to write changes to disk."
    )]
    async fn attach_rule(
        &self,
        params: Parameters<AttachRuleParams>,
    ) -> Result<CallToolResult, McpError> {
        let project_name = &params.0.project;
        let machine_name = &params.0.machine_name;

        if let Err(e) = validate_project(project_name) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        if crate::core::read_rule(machine_name).is_err() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Rule '{}' does not exist in the library. Call \
                 automatic_list_rules to see available rules.",
                machine_name
            ))]));
        }

        let project_json = match crate::core::read_project(project_name) {
            Ok(j) => j,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read project '{}': {}",
                    project_name, e
                ))]));
            }
        };
        let mut project: crate::core::Project = match serde_json::from_str(&project_json) {
            Ok(p) => p,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to parse project '{}': {}",
                    project_name, e
                ))]));
            }
        };

        let key = match resolve_file_rules_key(&project, params.0.file.as_deref()) {
            Ok(k) => k,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };

        let entries = project.file_rules.entry(key.clone()).or_default();
        if entries.iter().any(|r| r == machine_name) {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "Rule '{}' is already attached to project '{}' under '{}'.",
                machine_name, project_name, key
            ))]));
        }
        entries.push(machine_name.to_string());

        let new_json = match serde_json::to_string(&project) {
            Ok(s) => s,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to serialise project '{}': {}",
                    project_name, e
                ))]));
            }
        };
        match crate::core::save_project(project_name, &new_json) {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Attached rule '{}' to project '{}' under '{}'. Call \
                 automatic_sync_project to write the change to disk.",
                machine_name, project_name, key
            ))])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to save project '{}': {}",
                project_name, e
            ))])),
        }
    }

    #[tool(
        name = "automatic_detach_rule",
        description = "Detach a rule from a project's instruction file. \
                       Mandatory rules (e.g. `automatic-service`) cannot be \
                       detached. Idempotent — detaching a rule that is not \
                       attached reports success. Does not trigger a sync."
    )]
    async fn detach_rule(
        &self,
        params: Parameters<DetachRuleParams>,
    ) -> Result<CallToolResult, McpError> {
        let project_name = &params.0.project;
        let machine_name = &params.0.machine_name;

        if let Err(e) = validate_project(project_name) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        if crate::core::is_mandatory_rule(machine_name) {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Cannot detach rule '{}' — it is required by Automatic and \
                 is re-added automatically on save.",
                machine_name
            ))]));
        }

        let project_json = match crate::core::read_project(project_name) {
            Ok(j) => j,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read project '{}': {}",
                    project_name, e
                ))]));
            }
        };
        let mut project: crate::core::Project = match serde_json::from_str(&project_json) {
            Ok(p) => p,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to parse project '{}': {}",
                    project_name, e
                ))]));
            }
        };

        let key = match resolve_file_rules_key(&project, params.0.file.as_deref()) {
            Ok(k) => k,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };

        let removed = match project.file_rules.get_mut(&key) {
            Some(entries) => {
                let before = entries.len();
                entries.retain(|r| r != machine_name);
                before != entries.len()
            }
            None => false,
        };

        if !removed {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "Rule '{}' was not attached to project '{}' under '{}'.",
                machine_name, project_name, key
            ))]));
        }

        let new_json = match serde_json::to_string(&project) {
            Ok(s) => s,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to serialise project '{}': {}",
                    project_name, e
                ))]));
            }
        };
        match crate::core::save_project(project_name, &new_json) {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Detached rule '{}' from project '{}' under '{}'. Call \
                 automatic_sync_project to write the change to disk.",
                machine_name, project_name, key
            ))])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to save project '{}': {}",
                project_name, e
            ))])),
        }
    }

    // ── Hooks tools ──────────────────────────────────────────────────────

    #[tool(
        name = "automatic_list_hooks",
        description = "List every hook in the Automatic library. Returns an \
                       array of objects with `id` (machine name), `name`, \
                       `agent`, `event`, and optional `plugin_id`."
    )]
    async fn list_hooks(&self) -> Result<CallToolResult, McpError> {
        match crate::core::list_hooks() {
            Ok(hooks) => {
                let json =
                    serde_json::to_string_pretty(&hooks).unwrap_or_else(|_| "[]".to_string());
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to list hooks: {}",
                e
            ))])),
        }
    }

    #[tool(
        name = "automatic_read_hook",
        description = "Read a hook by machine name. Returns the full hook \
                       JSON (`name`, `agent`, `event`, `matcher`, `handler`, \
                       `timeout_sec`, optional `plugin_id`)."
    )]
    async fn read_hook(
        &self,
        params: Parameters<ReadHookParams>,
    ) -> Result<CallToolResult, McpError> {
        match crate::core::read_hook(&params.0.machine_name) {
            Ok(content) => Ok(CallToolResult::success(vec![Content::text(content)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to read hook '{}': {}",
                params.0.machine_name, e
            ))])),
        }
    }

    #[tool(
        name = "automatic_create_hook",
        description = "Create a new hook in the Automatic library. Fails if a \
                       hook with the same machine name already exists — use \
                       `automatic_update_hook` to modify an existing hook. \
                       The handler payload follows the HookHandler shape: \
                       `{\"kind\":\"command\",\"command\":\"...\"}` or \
                       `{\"kind\":\"script\",\"interpreter\":\"bash\",\"script\":\"...\"}`."
    )]
    async fn create_hook(
        &self,
        params: Parameters<CreateHookParams>,
    ) -> Result<CallToolResult, McpError> {
        let machine_name = &params.0.machine_name;

        if crate::core::read_hook(machine_name).is_ok() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Hook '{}' already exists. Use automatic_update_hook to modify it.",
                machine_name
            ))]));
        }

        let handler: crate::core::HookHandler =
            match serde_json::from_value(params.0.handler.clone()) {
                Ok(h) => h,
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Invalid handler payload for hook '{}': {}",
                        machine_name, e
                    ))]))
                }
            };

        match crate::core::save_hook(
            machine_name,
            &params.0.name,
            &params.0.agent,
            &params.0.event,
            params.0.matcher.as_deref(),
            handler,
            params.0.timeout_sec,
        ) {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Created hook '{}'.",
                machine_name
            ))])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to create hook '{}': {}",
                machine_name, e
            ))])),
        }
    }

    #[tool(
        name = "automatic_update_hook",
        description = "Update an existing hook. Fails if the hook does not \
                       exist or is provided by a plugin. Pass only the fields \
                       you want to change."
    )]
    async fn update_hook(
        &self,
        params: Parameters<UpdateHookParams>,
    ) -> Result<CallToolResult, McpError> {
        let machine_name = &params.0.machine_name;

        let existing_raw = match crate::core::read_hook(machine_name) {
            Ok(raw) => raw,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Cannot update hook '{}': {}",
                    machine_name, e
                ))]));
            }
        };
        let existing: crate::core::Hook = match serde_json::from_str(&existing_raw) {
            Ok(h) => h,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to parse existing hook '{}': {}",
                    machine_name, e
                ))]));
            }
        };

        if existing.plugin_id.is_some() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Cannot update hook '{}' — it is provided by a plugin.",
                machine_name
            ))]));
        }

        let new_name = params.0.name.as_deref().unwrap_or(&existing.name);
        let new_agent = params.0.agent.as_deref().unwrap_or(&existing.agent);
        let new_event = params.0.event.as_deref().unwrap_or(&existing.event);

        // `matcher` and `timeout_sec` are JSON values so callers can pass
        // `null` to clear them. Omitted fields keep the existing value.
        let new_matcher: Option<String> = match params.0.matcher {
            Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::String(s)) => Some(s),
            Some(other) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Invalid matcher payload for hook '{}': expected string or null, got {}",
                    machine_name, other
                ))]));
            }
            None => existing.matcher.clone(),
        };

        let new_timeout: Option<u32> = match params.0.timeout_sec {
            Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::Number(n)) => match n.as_u64() {
                Some(v) if v <= u32::MAX as u64 => Some(v as u32),
                _ => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Invalid timeout_sec for hook '{}': must be a u32",
                        machine_name
                    ))]));
                }
            },
            Some(other) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Invalid timeout_sec for hook '{}': expected number or null, got {}",
                    machine_name, other
                ))]));
            }
            None => existing.timeout_sec,
        };

        let new_handler: crate::core::HookHandler = match params.0.handler {
            Some(val) => match serde_json::from_value(val) {
                Ok(h) => h,
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Invalid handler payload for hook '{}': {}",
                        machine_name, e
                    ))]));
                }
            },
            None => existing.handler.clone(),
        };

        match crate::core::save_hook(
            machine_name,
            new_name,
            new_agent,
            new_event,
            new_matcher.as_deref(),
            new_handler,
            new_timeout,
        ) {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Updated hook '{}'.",
                machine_name
            ))])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to update hook '{}': {}",
                machine_name, e
            ))])),
        }
    }

    #[tool(
        name = "automatic_delete_hook",
        description = "Delete a hook from the Automatic library. Plugin-provided \
                       hooks cannot be deleted. Note that this removes the \
                       hook from the library but does not detach it from any \
                       project — projects referencing the deleted hook will \
                       silently skip it on next sync."
    )]
    async fn delete_hook(
        &self,
        params: Parameters<DeleteHookParams>,
    ) -> Result<CallToolResult, McpError> {
        let machine_name = &params.0.machine_name;
        match crate::core::delete_hook(machine_name) {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Deleted hook '{}'.",
                machine_name
            ))])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to delete hook '{}': {}",
                machine_name, e
            ))])),
        }
    }

    #[tool(
        name = "automatic_attach_hook",
        description = "Attach a hook to a project. The hook's target agent is \
                       inferred from its library record. Idempotent. Does not \
                       trigger a sync — call automatic_sync_project to write \
                       the change to disk."
    )]
    async fn attach_hook(
        &self,
        params: Parameters<AttachHookParams>,
    ) -> Result<CallToolResult, McpError> {
        let project_name = &params.0.project;
        let machine_name = &params.0.machine_name;

        if let Err(e) = validate_project(project_name) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        if crate::core::read_hook(machine_name).is_err() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Hook '{}' does not exist in the library. Call \
                 automatic_list_hooks to see available hooks.",
                machine_name
            ))]));
        }

        let project_json = match crate::core::read_project(project_name) {
            Ok(j) => j,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read project '{}': {}",
                    project_name, e
                ))]));
            }
        };
        let mut project: crate::core::Project = match serde_json::from_str(&project_json) {
            Ok(p) => p,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to parse project '{}': {}",
                    project_name, e
                ))]));
            }
        };

        if project.hooks.iter().any(|h| h == machine_name) {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "Hook '{}' is already attached to project '{}'.",
                machine_name, project_name
            ))]));
        }
        project.hooks.push(machine_name.to_string());

        let new_json = match serde_json::to_string(&project) {
            Ok(s) => s,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to serialise project '{}': {}",
                    project_name, e
                ))]));
            }
        };
        match crate::core::save_project(project_name, &new_json) {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Attached hook '{}' to project '{}'. Call \
                 automatic_sync_project to write the change to disk.",
                machine_name, project_name
            ))])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to save project '{}': {}",
                project_name, e
            ))])),
        }
    }

    #[tool(
        name = "automatic_detach_hook",
        description = "Detach a hook from a project. Idempotent. Does not \
                       trigger a sync."
    )]
    async fn detach_hook(
        &self,
        params: Parameters<DetachHookParams>,
    ) -> Result<CallToolResult, McpError> {
        let project_name = &params.0.project;
        let machine_name = &params.0.machine_name;

        if let Err(e) = validate_project(project_name) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }

        let project_json = match crate::core::read_project(project_name) {
            Ok(j) => j,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read project '{}': {}",
                    project_name, e
                ))]));
            }
        };
        let mut project: crate::core::Project = match serde_json::from_str(&project_json) {
            Ok(p) => p,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to parse project '{}': {}",
                    project_name, e
                ))]));
            }
        };

        let before = project.hooks.len();
        project.hooks.retain(|h| h != machine_name);
        if project.hooks.len() == before {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "Hook '{}' was not attached to project '{}'.",
                machine_name, project_name
            ))]));
        }

        let new_json = match serde_json::to_string(&project) {
            Ok(s) => s,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to serialise project '{}': {}",
                    project_name, e
                ))]));
            }
        };
        match crate::core::save_project(project_name, &new_json) {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Detached hook '{}' from project '{}'. Call \
                 automatic_sync_project to write the change to disk.",
                machine_name, project_name
            ))])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to save project '{}': {}",
                project_name, e
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
                let json =
                    serde_json::to_string_pretty(&results).unwrap_or_else(|_| "[]".to_string());
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
        match crate::memory::clear_memories(
            &params.0.project,
            params.0.pattern.as_deref(),
            params.0.confirm,
        ) {
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

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AutomaticMcpServer {
    fn get_info(&self) -> ServerInfo {
        let server_info = Implementation::new("automatic", env!("CARGO_PKG_VERSION"))
            .with_title("Automatic")
            .with_description(
                "Desktop hub for AI coding agents — skills, MCP configs, and project management",
            )
            .with_website_url("https://github.com/anomalyco/automatic");

        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "Automatic is a desktop hub for AI coding agents. \
                 Use these tools to retrieve API keys, discover and search skills, list MCP \
                 server configs, inspect projects, track active sessions, and sync project \
                 configurations.",
            )
            .with_server_info(server_info)
    }
}

// ── Entry Point ──────────────────────────────────────────────────────────────

pub async fn run_mcp_server() -> Result<(), Box<dyn std::error::Error>> {
    let server = AutomaticMcpServer::new();
    let service = server.serve(stdio()).await?;
    service.waiting().await?;

    Ok(())
}
