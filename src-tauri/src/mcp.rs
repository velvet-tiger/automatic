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
    /// The project whose project-local skills are searched first, before the
    /// managed library and external scan locations. Omit it to use the
    /// current project, the one this agent is working in. Accepts a
    /// local_key, an id or a name.
    pub project: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ReadProjectParams {
    /// The project to read. Omit it to use the current project, the one this
    /// agent is working in. Accepts a local_key, an id or a name.
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct RegisterProjectParams {
    /// The name for the new project. Another project may already use it;
    /// the new project gets its own local_key.
    pub name: String,
    /// Absolute path to the project's working directory. The directory must
    /// already exist on disk.
    pub directory: String,
    /// Optional short description of the project.
    pub description: Option<String>,
    /// Optional list of agent tool ids to configure for the project (e.g.
    /// "claude", "cursor", "codex"). When provided, agent configuration
    /// files are synced into the directory immediately.
    pub agents: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SearchSkillsParams {
    /// Search query (skill name, topic, or keyword)
    pub query: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SyncProjectParams {
    /// The project to sync configs for. Omit it to use the current project,
    /// the one this agent is working in. Accepts a local_key, an id or a
    /// name.
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct StoreMemoryParams {
    /// The project to act on. Omit it to use the current project, the one
    /// this agent is working in. Accepts a local_key, an id or a name.
    pub project: Option<String>,
    /// The memory key (identifier)
    pub key: String,
    /// The memory value to store
    pub value: String,
    /// Optional: identifier for the agent/tool storing this memory
    pub source: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetMemoryParams {
    /// The project to act on. Omit it to use the current project, the one
    /// this agent is working in. Accepts a local_key, an id or a name.
    pub project: Option<String>,
    /// The memory key to retrieve
    pub key: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListMemoriesParams {
    /// The project to act on. Omit it to use the current project, the one
    /// this agent is working in. Accepts a local_key, an id or a name.
    pub project: Option<String>,
    /// Optional: filter keys by this substring (case-insensitive)
    pub pattern: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SearchMemoriesParams {
    /// The project to act on. Omit it to use the current project, the one
    /// this agent is working in. Accepts a local_key, an id or a name.
    pub project: Option<String>,
    /// Search query to match against keys and values
    pub query: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DeleteMemoryParams {
    /// The project to act on. Omit it to use the current project, the one
    /// this agent is working in. Accepts a local_key, an id or a name.
    pub project: Option<String>,
    /// The memory key to delete
    pub key: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ClearMemoriesParams {
    /// The project to act on. Omit it to use the current project, the one
    /// this agent is working in. Accepts a local_key, an id or a name.
    pub project: Option<String>,
    /// Optional: only delete memories with keys matching this pattern (case-insensitive)
    pub pattern: Option<String>,
    /// Must be set to true to confirm deletion
    pub confirm: bool,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ReadClaudeMemoryParams {
    /// The project to act on. Omit it to use the current project, the one
    /// this agent is working in. Accepts a local_key, an id or a name.
    pub project: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetRelatedProjectsParams {
    /// The project to act on. Omit it to use the current project, the one
    /// this agent is working in. Accepts a local_key, an id or a name.
    pub project: Option<String>,
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
    /// The project to act on. Omit it to use the current project, the one
    /// this agent is working in. Accepts a local_key, an id or a name.
    pub project: Option<String>,
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
    /// The project to act on. Omit it to use the current project, the one
    /// this agent is working in. Accepts a local_key, an id or a name.
    pub project: Option<String>,
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
    /// The project to act on. Omit it to use the current project, the one
    /// this agent is working in. Accepts a local_key, an id or a name.
    pub project: Option<String>,
    /// The hook's machine name. Must already exist in the library.
    pub machine_name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DetachHookParams {
    /// The project to act on. Omit it to use the current project, the one
    /// this agent is working in. Accepts a local_key, an id or a name.
    pub project: Option<String>,
    /// The hook's machine name.
    pub machine_name: String,
}

// ── Profile Tool Parameter Types ─────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ReadProfileParams {
    /// The profile name (lowercase letters, digits, and hyphens).
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AttachProfileParams {
    /// The project to act on. Omit it to use the current project, the one
    /// this agent is working in. Accepts a local_key, an id or a name.
    pub project: Option<String>,
    /// The profile name. Must already exist in the library.
    pub profile: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DetachProfileParams {
    /// The project to act on. Omit it to use the current project, the one
    /// this agent is working in. Accepts a local_key, an id or a name.
    pub project: Option<String>,
    /// The profile name.
    pub profile: String,
}

// ── Context Tool Parameter Types ─────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListContextsParams {
    /// The project whose attached contexts to list, each with the group
    /// that provides it (if any). Omit it to use the current project, the
    /// one this agent is working in. Accepts a local_key, an id or a name.
    #[serde(default)]
    pub project: Option<String>,
    /// List every context in the library instead of one project's. Ignores
    /// `project`.
    #[serde(default)]
    pub all: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ReadContextParams {
    /// The context slug.
    pub context: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListContextEntriesParams {
    /// The context slug.
    pub context: String,
    /// The source id, as returned by automatic_read_context.
    pub source: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ReadContextEntryParams {
    /// The context slug.
    pub context: String,
    /// The source id, as returned by automatic_read_context.
    pub source: String,
    /// The entry path, as returned by automatic_list_context_entries.
    pub path: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AttachContextParams {
    /// The context slug. Must already exist in the library.
    pub context: String,
    /// Project to attach to: a local_key, an id or a name. Give `project`
    /// or `group`, not both. Give neither to use the current project, the
    /// one this agent is working in.
    #[serde(default)]
    pub project: Option<String>,
    /// Project group name to attach to. Every member project receives the
    /// context. Give `project` or `group`, not both.
    #[serde(default)]
    pub group: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CreateContextParams {
    /// A short name, e.g. "Coding standards". The id is derived from it.
    pub name: String,
    /// What the context holds and when an agent should read it.
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct WriteContextPageParams {
    /// The context slug.
    pub context: String,
    /// Page title. The page path is derived from it and `folder`. Writing
    /// the same title in the same folder replaces that page. Give either
    /// `title` or `path`.
    #[serde(default)]
    pub title: Option<String>,
    /// Folder for a titled page, e.g. "Guides" or "Guides/Troubleshooting".
    /// Omit for the top level.
    #[serde(default)]
    pub folder: Option<String>,
    /// Exact path of an existing page to replace, as listed by
    /// automatic_list_context_entries (e.g. "guides/setup.md").
    #[serde(default)]
    pub path: Option<String>,
    /// The page body in Markdown.
    pub content: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct MoveContextPageParams {
    /// The context slug.
    pub context: String,
    /// Current page path, as listed by automatic_list_context_entries.
    pub path: String,
    /// Destination folder. Omit to stay in the current folder; "" for the top level.
    #[serde(default)]
    pub folder: Option<String>,
    /// New title. Omit to keep the current name.
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DeleteContextPageParams {
    /// The context slug.
    pub context: String,
    /// Page path, as listed by automatic_list_context_entries.
    pub path: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AddContextFolderParams {
    /// The context slug.
    pub context: String,
    /// Absolute path to a folder or file on this machine.
    pub path: String,
    /// A short name for the linked material, e.g. "Design notes".
    pub name: String,
    /// When an agent should look here.
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AddContextWebPageParams {
    /// The context slug.
    pub context: String,
    /// An http or https address on the public internet.
    pub url: String,
    /// A short name for the page, e.g. "Public API reference".
    pub name: String,
    /// When an agent should look here.
    #[serde(default)]
    pub description: String,
    /// Seconds a downloaded copy is reused before the next read downloads
    /// the page again. `0` downloads on every read. Defaults to 3600.
    #[serde(default)]
    pub ttl_secs: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct RemoveContextSourceParams {
    /// The context slug.
    pub context: String,
    /// The source id, as returned by automatic_read_context.
    pub source: String,
}

// ── Feature Tool Parameter Types ─────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListFeaturesParams {
    /// The project to act on. Omit it to use the current project, the one
    /// this agent is working in. Accepts a local_key, an id or a name.
    pub project: Option<String>,
    /// Optional state filter: backlog, todo, in_progress, review, complete, or cancelled
    pub state: Option<String>,
    /// When true, returns only archived features. Defaults to false (active features only).
    pub include_archived: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ArchiveFeatureParams {
    /// The project to act on. Omit it to use the current project, the one
    /// this agent is working in. Accepts a local_key, an id or a name.
    pub project: Option<String>,
    /// The feature UUID to archive
    pub feature_id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct UnarchiveFeatureParams {
    /// The project to act on. Omit it to use the current project, the one
    /// this agent is working in. Accepts a local_key, an id or a name.
    pub project: Option<String>,
    /// The feature UUID to unarchive
    pub feature_id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetFeatureParams {
    /// The project to act on. Omit it to use the current project, the one
    /// this agent is working in. Accepts a local_key, an id or a name.
    pub project: Option<String>,
    /// The feature UUID
    pub feature_id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CreateFeatureParams {
    /// The project to act on. Omit it to use the current project, the one
    /// this agent is working in. Accepts a local_key, an id or a name.
    pub project: Option<String>,
    /// Short title for the feature (required)
    pub title: String,
    /// Markdown description of the work to be done
    pub description: Option<String>,
    /// Initial state: backlog (default), todo, in_progress, review, complete, or cancelled
    pub state: Option<String>,
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
    /// The project to act on. Omit it to use the current project, the one
    /// this agent is working in. Accepts a local_key, an id or a name.
    pub project: Option<String>,
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
    /// The project to act on. Omit it to use the current project, the one
    /// this agent is working in. Accepts a local_key, an id or a name.
    pub project: Option<String>,
    /// The feature UUID
    pub feature_id: String,
    /// New state: backlog, todo, in_progress, review, complete, or cancelled
    pub state: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DeleteFeatureParams {
    /// The project to act on. Omit it to use the current project, the one
    /// this agent is working in. Accepts a local_key, an id or a name.
    pub project: Option<String>,
    /// The feature UUID to delete permanently
    pub feature_id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AddFeatureUpdateParams {
    /// The project to act on. Omit it to use the current project, the one
    /// this agent is working in. Accepts a local_key, an id or a name.
    pub project: Option<String>,
    /// The feature UUID to add an update to
    pub feature_id: String,
    /// Markdown content of the progress update
    pub content: String,
    /// Agent id or name authoring this update
    pub author: Option<String>,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Read and parse one checkout's project, with error text ready for a tool
/// result.
fn load_project(checkout: &crate::core::ProjectSummary) -> Result<crate::core::Project, String> {
    let raw = crate::core::read_project(crate::core::checkout_ident(checkout))
        .map_err(|e| format!("Failed to read project '{}': {}", checkout.name, e))?;
    serde_json::from_str(&raw)
        .map_err(|e| format!("Failed to parse project '{}': {}", checkout.name, e))
}

/// Serialise and save one checkout's project, with error text ready for a
/// tool result.
fn persist_project(
    checkout: &crate::core::ProjectSummary,
    project: &crate::core::Project,
) -> Result<(), String> {
    let json = serde_json::to_string(project)
        .map_err(|e| format!("Failed to serialise project '{}': {}", checkout.name, e))?;
    crate::core::save_project(crate::core::checkout_ident(checkout), &json)
        .map_err(|e| format!("Failed to save project '{}': {}", checkout.name, e))
}

/// One `automatic_list_projects` row.
#[derive(Debug, Serialize, PartialEq, Eq)]
struct ProjectListRow {
    name: String,
    local_key: String,
    id: String,
    directory: String,
    current: bool,
}

fn project_list_rows(
    summaries: &[crate::core::ProjectSummary],
    current: &crate::core::CurrentProject,
) -> Vec<ProjectListRow> {
    summaries
        .iter()
        .map(|s| ProjectListRow {
            name: s.name.clone(),
            local_key: s.local_key.clone(),
            id: s.id.clone(),
            directory: s.directory.clone(),
            current: current.includes(s),
        })
        .collect()
}

fn tool_json<T: Serialize>(value: &T) -> CallToolResult {
    match serde_json::to_string_pretty(value) {
        Ok(json) => CallToolResult::success(vec![Content::text(json)]),
        Err(e) => CallToolResult::error(vec![Content::text(format!(
            "Failed to serialise result: {}",
            e
        ))]),
    }
}

fn tool_error(message: String) -> CallToolResult {
    CallToolResult::error(vec![Content::text(message)])
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

/// Register a new project in Automatic. Shared body of the
/// `automatic_register_project` MCP tool, kept as a free function so tests
/// can exercise it without an MCP runtime.
///
/// Mirrors the GUI's Add Project flow: validate name, directory, and agent
/// ids; refuse directories that already belong to a registered project or
/// hold an unregistered on-disk config; persist via `core::save_project`;
/// log a `ProjectCreated` activity; and, when agents were provided, run a
/// best-effort sync so agent config files appear in the directory at once.
fn register_project_impl(params: &RegisterProjectParams) -> Result<String, String> {
    let name = params.name.trim();
    let directory = params.directory.trim();

    if !crate::core::is_valid_name(name) {
        return Err(
            "Invalid project name — it must be non-empty and must not contain '/' or '\\'."
                .to_string(),
        );
    }

    if directory.is_empty() {
        return Err("A project directory is required.".to_string());
    }
    let dir_path = std::path::PathBuf::from(directory);
    if !dir_path.is_absolute() {
        return Err(format!(
            "The directory must be an absolute path (got '{}').",
            directory
        ));
    }
    if !dir_path.is_dir() {
        return Err(format!("Directory '{}' does not exist.", directory));
    }

    // Validate agent ids before touching any state so a typo cannot leave a
    // half-registered project behind. Empty entries are skipped; duplicates
    // are collapsed.
    let mut agents: Vec<String> = Vec::new();
    for id in params.agents.iter().flatten() {
        let id = id.trim();
        if id.is_empty() {
            continue;
        }
        if crate::agent::from_id(id).is_none() {
            let valid: Vec<String> = crate::agent::all()
                .iter()
                .map(|a| a.id().to_string())
                .collect();
            return Err(format!(
                "Unknown agent '{}'. Valid agent ids are: {}.",
                id,
                valid.join(", ")
            ));
        }
        if !agents.iter().any(|a| a == id) {
            agents.push(id.to_string());
        }
    }

    // Refuse directories that already belong to a registered project or hold
    // an unregistered on-disk config — both need a human decision in the UI
    // (open the existing project, or import/discard the orphan).
    match crate::core::inspect_project_directory(directory)? {
        crate::core::DirectoryStatus::Available => {}
        crate::core::DirectoryStatus::RegisteredHere { name: existing, .. } => {
            return Err(format!(
                "This directory is already registered as project '{}'. Open that project \
                 instead of creating it again.",
                existing
            ));
        }
        crate::core::DirectoryStatus::OrphanConfig { name: orphan } => {
            return Err(format!(
                "Directory '{}' contains an Automatic config (.automatic.json) that is \
                 not registered (project name '{}'). Ask the user to import it from the \
                 Automatic app, or remove it, before registering a new project here.",
                directory, orphan
            ));
        }
    }

    // Belt-and-suspenders with inspect_project_directory: validates the name
    // and refuses a directory another project owns. Another project may
    // already use the name (stage 6).
    crate::core::assert_can_create_project(name, directory)?;

    let now = chrono::Utc::now().to_rfc3339();
    let mut project = crate::core::Project {
        name: name.to_string(),
        description: params
            .description
            .as_deref()
            .unwrap_or("")
            .trim()
            .to_string(),
        directory: directory.to_string(),
        agents: agents.clone(),
        created_at: now.clone(),
        updated_at: now,
        ..Default::default()
    };
    crate::core::prepare_new_project_keys(&mut project)?;
    let data = serde_json::to_string(&project)
        .map_err(|e| format!("Failed to serialise project: {}", e))?;
    // By the fresh `local_key`: the name may belong to another project too.
    let ident = crate::core::project_ident(&project).to_string();
    crate::core::save_project(&ident, &data)?;

    crate::activity::log(
        &ident,
        crate::activity::ActivityEvent::ProjectCreated,
        "Project created",
        name,
    );

    // Sync agent configs when agents were requested. Registration has already
    // succeeded at this point, so a sync failure is reported as a warning
    // instead of failing the whole call (mirrors the GUI's new-project flow,
    // where partial success beats a hard error).
    let mut report = format!(
        "Registered project '{}'.\nDirectory: {}\nlocal_key: {}\n",
        name, directory, ident
    );
    if !agents.is_empty() {
        report.push_str(&format!("Agents: {}\n", agents.join(", ")));
        match crate::sync::sync_project(&project) {
            Ok(files) => {
                report.push_str(&format!(
                    "Synced {} agent config file{} to the directory.\n",
                    files.len(),
                    if files.len() == 1 { "" } else { "s" }
                ));
            }
            Err(e) => {
                report.push_str(&format!(
                    "Warning: the initial agent-config sync failed: {}. The project is \
                     registered; call automatic_sync_project to retry.\n",
                    e
                ));
            }
        }
    } else {
        report.push_str(
            "No agent tools configured yet — add agents in the Automatic app, then call \
             automatic_sync_project to write their config files.\n",
        );
    }
    report.push_str("Call automatic_read_project to inspect the saved configuration.");

    Ok(report)
}

/// One peer in `automatic_get_related_projects`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RelatedPeer {
    name: String,
    directory: String,
    /// Identifier to read the peer by: its `local_key`, or the stored value
    /// for a member no registered project claims.
    ident: String,
}

/// The peers a group's stored `members` name, leaving out `this` project
/// (every checkout of its `id`). A member `id` expands to every checkout of
/// that project, in the order `read_group` shows them. A legacy name only
/// one project has resolves to it. Any other member is listed as stored
/// with no folder, so nothing disappears.
fn related_peers(
    index: &crate::core::ProjectKeyIndex,
    members: &[String],
    this: &crate::core::ProjectSummary,
) -> Vec<RelatedPeer> {
    let is_this = |s: &crate::core::ProjectSummary| {
        (!this.id.is_empty() && s.id == this.id)
            || (!this.local_key.is_empty() && s.local_key == this.local_key)
    };
    let mut peers = Vec::new();
    for member in members {
        let keys = index.member_keys(member);
        for key in keys {
            match index.summaries().iter().find(|s| crate::core::checkout_ident(s) == key)
                .or_else(|| index.resolve(&key))
            {
                Some(summary) if is_this(summary) => {}
                Some(summary) => peers.push(RelatedPeer {
                    name: summary.name.clone(),
                    directory: summary.directory.clone(),
                    ident: crate::core::checkout_ident(summary).to_string(),
                }),
                None => peers.push(RelatedPeer {
                    name: key.clone(),
                    directory: String::new(),
                    ident: key,
                }),
            }
        }
    }
    peers
}

// ── MCP Server Handler ──────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AutomaticMcpServer {
    tool_router: ToolRouter<Self>,
    /// The project this `mcp-serve` process works for, resolved once at
    /// startup. Tools use it when the `project` argument is omitted.
    current_project: crate::core::CurrentProject,
}

/// How a tool's `project` argument becomes the project it acts on. The
/// registry is read on every call, so a project registered, renamed or
/// removed since startup is seen.
impl AutomaticMcpServer {
    /// Resolve `explicit`, or the current project when it is omitted.
    fn project_target(&self, explicit: Option<&str>) -> Result<crate::core::ProjectTarget, String> {
        let summaries = crate::core::get_project_summaries()
            .map_err(|e| format!("Failed to read the project registry: {}", e))?;
        crate::core::resolve_project_target(explicit, &self.current_project, &summaries)
    }

    /// For tools that read or write one checkout's configuration or folder.
    /// A project id with several checkouts is an error listing them.
    fn project_checkout(&self, explicit: Option<&str>) -> Result<crate::core::ProjectSummary, String> {
        self.project_target(explicit)?.into_checkout()
    }

    /// For tools that reach a per-project store keyed by `id` (memory).
    fn project_store_keys(
        &self,
        explicit: Option<&str>,
    ) -> Result<crate::core::ProjectStoreKeys, String> {
        Ok(self.project_target(explicit)?.store_keys())
    }

    /// As [`Self::project_store_keys`], and the project must have feature
    /// tracking on. For a project with several checkouts the setting is
    /// read from the first, as `project_store_keys` does for an `id`.
    fn feature_store_keys(
        &self,
        explicit: Option<&str>,
    ) -> Result<crate::core::ProjectStoreKeys, String> {
        let target = self.project_target(explicit)?;
        crate::plugins::build::require_feature_tracking(crate::core::checkout_ident(
            target.representative(),
        ))?;
        Ok(target.store_keys())
    }

    /// Resolve the `project` / `group` pair of a context attach or detach
    /// call, with a label for the result message. Neither given means the
    /// current project.
    fn context_target(
        &self,
        params: &AttachContextParams,
    ) -> Result<(crate::core::ContextTarget, String), String> {
        match (&params.project, &params.group) {
            (Some(_), Some(_)) => Err("Give `project` or `group`, not both.".to_string()),
            (None, Some(group)) => {
                if !crate::core::list_groups()?.iter().any(|g| g == group) {
                    return Err(format!(
                        "Unknown group '{}'. Call automatic_get_related_projects or ask the \
                         user for the group name.",
                        group
                    ));
                }
                Ok((
                    crate::core::ContextTarget::Group(group.clone()),
                    format!("group '{}'", group),
                ))
            }
            (project, None) => {
                let checkout = self.project_checkout(project.as_deref())?;
                Ok((
                    crate::core::ContextTarget::Project(
                        crate::core::checkout_ident(&checkout).to_string(),
                    ),
                    format!("project '{}'", checkout.name),
                ))
            }
        }
    }
}

#[tool_router]
impl AutomaticMcpServer {
    /// A server with no current project. Every tool that acts on a project
    /// then needs its `project` argument.
    pub fn new() -> Self {
        Self::with_current_project(crate::core::CurrentProject::None)
    }

    pub fn with_current_project(current_project: crate::core::CurrentProject) -> Self {
        Self {
            tool_router: Self::tool_router(),
            current_project,
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
        description = "Read the content of a specific skill. Project-local skills (skills \
                       that exist only within a project directory, not in the global registry) \
                       of the current project, or of `project` when given, are checked first; \
                       the global registry is used as a fallback."
    )]
    async fn read_skill(
        &self,
        params: Parameters<ReadSkillParams>,
    ) -> Result<CallToolResult, McpError> {
        let name = &params.0.name;

        // Try the project's own custom skills first: the explicit `project`,
        // else the current project. Their content lives inline in the
        // project JSON, so no disk read is needed. An explicit project that
        // does not resolve is an error; no project at all means the global
        // registry only.
        let target = match (&params.0.project, &self.current_project) {
            (None, crate::core::CurrentProject::None) => None,
            (explicit, _) => match self.project_target(explicit.as_deref()) {
                Ok(target) => Some(target),
                Err(e) => return Ok(tool_error(e)),
            },
        };
        if let Some(target) = target {
            let project = match load_project(target.representative()) {
                Ok(project) => project,
                Err(e) => return Ok(tool_error(e)),
            };
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
        description = "List every project registered in Automatic. Each entry has `name`, \
                       `local_key` (one checkout on this machine), `id` (the project, shared \
                       by every checkout of it), `directory`, and `current`. `current` is true \
                       for the project this agent is working in, which tools use when \
                       `project` is omitted. When that is a project with several checkouts \
                       and none contains the working directory, every checkout is marked. \
                       Any of `local_key`, `id` or `name` can be passed as `project`."
    )]
    async fn list_projects(&self) -> Result<CallToolResult, McpError> {
        match crate::core::get_project_summaries() {
            Ok(summaries) => Ok(tool_json(&project_list_rows(&summaries, &self.current_project))),
            Err(e) => Ok(tool_error(format!("Failed to list projects: {}", e))),
        }
    }

    #[tool(
        name = "automatic_read_project",
        description = "Read the full configuration for a project (skills, MCP servers, agents, \
                       directory, description). Omit `name` to read the current project. \
                       `profiles` lists the attached profiles and \
                       `profile_contributions` records which entries each profile provides, \
                       including entries the project had before the profile was attached; \
                       those entries are owned by the profile and are re-attached on the \
                       next save if removed directly. `contexts` lists attached context \
                       slugs and `group_context_contributions` records which of them each \
                       project group provides."
    )]
    async fn read_project(
        &self,
        params: Parameters<ReadProjectParams>,
    ) -> Result<CallToolResult, McpError> {
        let checkout = match self.project_checkout(params.0.name.as_deref()) {
            Ok(checkout) => checkout,
            Err(e) => return Ok(tool_error(e)),
        };
        match crate::core::read_project(crate::core::checkout_ident(&checkout)) {
            Ok(content) => Ok(CallToolResult::success(vec![Content::text(content)])),
            Err(e) => Ok(tool_error(format!(
                "Failed to read project '{}': {}",
                checkout.name, e
            ))),
        }
    }

    #[tool(
        name = "automatic_register_project",
        description = "Register a new project in Automatic. Requires a project name and \
                       an absolute path to an existing directory on disk. Optionally provide a \
                       description and a list of agent tool ids (e.g. claude, cursor, codex) — \
                       when agents are given, their configuration files are synced into the \
                       directory immediately. Another project may share the name. Fails when \
                       the directory is already registered to another project, or the directory \
                       holds an unregistered Automatic config."
    )]
    async fn register_project(
        &self,
        params: Parameters<RegisterProjectParams>,
    ) -> Result<CallToolResult, McpError> {
        match register_project_impl(&params.0) {
            Ok(report) => Ok(CallToolResult::success(vec![Content::text(report)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to register project: {}",
                e
            ))])),
        }
    }

    #[tool(
        name = "automatic_get_related_projects",
        description = "Return all projects related to the current project, or to `project` \
                       when given, via Project Groups, \
                       including each peer's name, description, directory, and relative path \
                       from this project's directory. Use this to discover sibling projects \
                       you can explore or reference."
    )]
    async fn get_related_projects(
        &self,
        params: Parameters<GetRelatedProjectsParams>,
    ) -> Result<CallToolResult, McpError> {
        // Groups hold project ids, so a project with several checkouts is
        // enough. Relative paths are computed from its first checkout.
        let target = match self.project_target(params.0.project.as_deref()) {
            Ok(target) => target,
            Err(e) => return Ok(tool_error(e)),
        };
        let this = target.representative().clone();
        let this_dir = this.directory.as_str();

        // Find every group this project belongs to. Members are stored as
        // project ids.
        let groups = crate::core::groups_for_project(crate::core::checkout_ident(&this));

        if groups.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "This project does not belong to any groups and has no related projects.",
            )]));
        }
        let index = match crate::core::ProjectKeyIndex::load() {
            Ok(index) => index,
            Err(e) => return Ok(tool_error(format!("Failed to read the project registry: {}", e))),
        };

        // Collect unique peer checkouts across all groups, avoiding duplicates.
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

            let peers = related_peers(&index, &group.projects, &this);
            if peers.is_empty() {
                output.push_str("No other projects in this group yet.\n");
            } else {
                for peer in peers {
                    if !seen.insert(peer.ident.clone()) {
                        continue; // already included from another group
                    }
                    let peer_desc = crate::core::read_project(&peer.ident)
                        .ok()
                        .and_then(|raw| serde_json::from_str::<crate::core::Project>(&raw).ok())
                        .map(|p| p.description)
                        .unwrap_or_default();
                    let rel_path = crate::core::compute_relative_path(this_dir, &peer.directory);

                    // Every peer lists its folder, so peers that share a name
                    // stay distinguishable.
                    let mut entry = format!("**{}**", peer.name);
                    if !peer_desc.trim().is_empty() {
                        entry.push_str(&format!(": {}", peer_desc.trim()));
                    }
                    if !rel_path.is_empty() {
                        entry.push_str(&format!("\nLocation: `{}`", rel_path));
                    }
                    if !peer.directory.is_empty() {
                        entry.push_str(&format!("\nAbsolute path: `{}`", peer.directory));
                    }
                    output.push_str(&entry);
                    output.push('\n');
                }
            }
            output.push('\n');
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
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
        let machine_name = &params.0.machine_name;

        let checkout = match self.project_checkout(params.0.project.as_deref()) {
            Ok(checkout) => checkout,
            Err(e) => return Ok(tool_error(e)),
        };
        let project_ident = crate::core::checkout_ident(&checkout);
        let project_name = &checkout.name;
        if crate::core::read_rule(machine_name).is_err() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Rule '{}' does not exist in the library. Call \
                 automatic_list_rules to see available rules.",
                machine_name
            ))]));
        }

        let project_json = match crate::core::read_project(project_ident) {
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
        match crate::core::save_project(project_ident, &new_json) {
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
                       attached reports success. Does not trigger a sync. \
                       A rule provided by an attached profile is re-attached \
                       on the project's next save; detach the profile with \
                       automatic_detach_profile instead."
    )]
    async fn detach_rule(
        &self,
        params: Parameters<DetachRuleParams>,
    ) -> Result<CallToolResult, McpError> {
        let machine_name = &params.0.machine_name;

        let checkout = match self.project_checkout(params.0.project.as_deref()) {
            Ok(checkout) => checkout,
            Err(e) => return Ok(tool_error(e)),
        };
        let project_ident = crate::core::checkout_ident(&checkout);
        let project_name = &checkout.name;
        if crate::core::is_mandatory_rule(machine_name) {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Cannot detach rule '{}' — it is required by Automatic and \
                 is re-added automatically on save.",
                machine_name
            ))]));
        }

        let project_json = match crate::core::read_project(project_ident) {
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
        match crate::core::save_project(project_ident, &new_json) {
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
        let machine_name = &params.0.machine_name;

        let checkout = match self.project_checkout(params.0.project.as_deref()) {
            Ok(checkout) => checkout,
            Err(e) => return Ok(tool_error(e)),
        };
        let project_ident = crate::core::checkout_ident(&checkout);
        let project_name = &checkout.name;
        if crate::core::read_hook(machine_name).is_err() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Hook '{}' does not exist in the library. Call \
                 automatic_list_hooks to see available hooks.",
                machine_name
            ))]));
        }

        let project_json = match crate::core::read_project(project_ident) {
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
        match crate::core::save_project(project_ident, &new_json) {
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
                       trigger a sync. A hook provided by an attached profile \
                       is re-attached on the project's next save; detach the \
                       profile with automatic_detach_profile instead."
    )]
    async fn detach_hook(
        &self,
        params: Parameters<DetachHookParams>,
    ) -> Result<CallToolResult, McpError> {
        let machine_name = &params.0.machine_name;

        let checkout = match self.project_checkout(params.0.project.as_deref()) {
            Ok(checkout) => checkout,
            Err(e) => return Ok(tool_error(e)),
        };
        let project_ident = crate::core::checkout_ident(&checkout);
        let project_name = &checkout.name;

        let project_json = match crate::core::read_project(project_ident) {
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
        match crate::core::save_project(project_ident, &new_json) {
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

    // ── Profile tools ────────────────────────────────────────────────────

    #[tool(
        name = "automatic_list_profiles",
        description = "List every profile in the Automatic library. A profile \
                       is a live bundle of library references (skills, MCP \
                       servers, providers, agents, sub-agents, commands, hooks, \
                       rules) that keeps every attached project in step. \
                       Returns an array of objects with `name` and \
                       `description`; call automatic_read_profile for contents."
    )]
    async fn list_profiles(&self) -> Result<CallToolResult, McpError> {
        let names = match crate::core::list_project_profiles() {
            Ok(names) => names,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to list profiles: {}",
                    e
                ))]));
            }
        };
        let entries: Vec<serde_json::Value> = names
            .iter()
            .filter_map(|name| crate::core::read_project_profile_parsed(name).ok())
            .map(|profile| {
                serde_json::json!({
                    "name": profile.name,
                    "description": profile.description,
                })
            })
            .collect();
        let json = serde_json::to_string_pretty(&entries).unwrap_or_else(|_| "[]".to_string());
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        name = "automatic_read_profile",
        description = "Read a profile by name. Returns the full profile JSON \
                       (`name`, `description`, `skills`, `mcp_servers`, \
                       `providers`, `agents`, `user_agents`, `user_commands`, \
                       `hooks`, `rules`)."
    )]
    async fn read_profile(
        &self,
        params: Parameters<ReadProfileParams>,
    ) -> Result<CallToolResult, McpError> {
        match crate::core::read_project_profile(&params.0.name) {
            Ok(content) => Ok(CallToolResult::success(vec![Content::text(content)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to read profile '{}': {}",
                params.0.name, e
            ))])),
        }
    }

    #[tool(
        name = "automatic_attach_profile",
        description = "Attach a profile to a project. Every skill, MCP server, \
                       provider, agent, sub-agent, command, hook and rule the \
                       profile lists is recorded as the profile's contribution, \
                       so later edits to the profile keep the project in step. \
                       Missing entries are added; entries the project already \
                       had are adopted by the profile. Idempotent. Does not \
                       trigger a sync — call automatic_sync_project to write \
                       the change to disk."
    )]
    async fn attach_profile(
        &self,
        params: Parameters<AttachProfileParams>,
    ) -> Result<CallToolResult, McpError> {
        let profile_name = &params.0.profile;

        let checkout = match self.project_checkout(params.0.project.as_deref()) {
            Ok(checkout) => checkout,
            Err(e) => return Ok(tool_error(e)),
        };
        let project_name = &checkout.name;
        if crate::core::read_project_profile_parsed(profile_name).is_err() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Profile '{}' does not exist in the library. Call \
                 automatic_list_profiles to see available profiles.",
                profile_name
            ))]));
        }

        let mut project = match load_project(&checkout) {
            Ok(p) => p,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };

        if project.profiles.iter().any(|p| p == profile_name) {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "Profile '{}' is already attached to project '{}'.",
                profile_name, project_name
            ))]));
        }
        project.profiles.push(profile_name.to_string());
        crate::core::reconcile_project_profiles(&mut project);

        match persist_project(&checkout, &project) {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Attached profile '{}' to project '{}'. Call \
                 automatic_sync_project to write the change to disk.",
                profile_name, project_name
            ))])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    #[tool(
        name = "automatic_detach_profile",
        description = "Detach a profile from a project. Removes every entry \
                       the profile provides, including entries the project had \
                       before it was attached; entries no attached profile \
                       lists stay. Idempotent. Does not trigger a sync — call \
                       automatic_sync_project to write the change to disk."
    )]
    async fn detach_profile(
        &self,
        params: Parameters<DetachProfileParams>,
    ) -> Result<CallToolResult, McpError> {
        let profile_name = &params.0.profile;

        let checkout = match self.project_checkout(params.0.project.as_deref()) {
            Ok(checkout) => checkout,
            Err(e) => return Ok(tool_error(e)),
        };
        let project_name = &checkout.name;

        let mut project = match load_project(&checkout) {
            Ok(p) => p,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };

        let before = project.profiles.len();
        project.profiles.retain(|p| p != profile_name);
        let recorded = project.profile_contributions.contains_key(profile_name);
        if project.profiles.len() == before && !recorded {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "Profile '{}' was not attached to project '{}'.",
                profile_name, project_name
            ))]));
        }
        crate::core::reconcile_project_profiles(&mut project);

        match persist_project(&checkout, &project) {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Detached profile '{}' from project '{}'. Call \
                 automatic_sync_project to write the change to disk.",
                profile_name, project_name
            ))])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    // ── Context tools ────────────────────────────────────────────────────

    #[tool(
        name = "automatic_list_contexts",
        description = "List contexts: named collections of reference material \
                       (documentation pages, local files, URLs, cloud sources) \
                       that agents read on demand. Returns `slug`, \
                       `display_name`, `description` and `location` (`local` or \
                       `cloud`). Lists the contexts attached to the current \
                       project, or to `project` when given; each then carries \
                       `group` when a project group provides it. Pass \
                       `all: true`, or run with no current project, to list \
                       every context in the library. Call \
                       automatic_read_context next."
    )]
    async fn list_contexts(
        &self,
        params: Parameters<ListContextsParams>,
    ) -> Result<CallToolResult, McpError> {
        // The whole library when asked for, or when nothing names a project
        // (no argument and no current project), which is what this tool
        // listed before it had a current project.
        let whole_library = params.0.all.unwrap_or(false)
            || (params.0.project.is_none()
                && self.current_project == crate::core::CurrentProject::None);
        let slugs = if whole_library {
            match crate::core::list_contexts() {
                Ok(slugs) => slugs
                    .into_iter()
                    .map(|slug| crate::core::ProjectContextEntry { slug, group: None })
                    .collect(),
                Err(e) => return Ok(tool_error(format!("Failed to list contexts: {}", e))),
            }
        } else {
            let checkout = match self.project_checkout(params.0.project.as_deref()) {
                Ok(checkout) => checkout,
                Err(e) => return Ok(tool_error(e)),
            };
            match load_project(&checkout) {
                Ok(p) => crate::core::project_context_entries(&p),
                Err(e) => return Ok(tool_error(e)),
            }
        };
        let entries: Vec<serde_json::Value> = slugs
            .into_iter()
            .map(|entry| match crate::core::read_context(&entry.slug) {
                Ok(c) => serde_json::json!({
                    "slug": c.slug,
                    "display_name": c.display_name,
                    "description": c.description,
                    "location": match c.body {
                        crate::core::ContextBody::Local { .. } => "local",
                        crate::core::ContextBody::Cloud { .. } => "cloud",
                    },
                    "group": entry.group,
                }),
                // An attached slug whose file is gone is reported, not hidden.
                Err(e) => serde_json::json!({
                    "slug": entry.slug,
                    "group": entry.group,
                    "error": e,
                }),
            })
            .collect();
        Ok(tool_json(&entries))
    }

    #[tool(
        name = "automatic_read_context",
        description = "Read a context by slug. Returns its description and \
                       `sources`: each has an `id`, a `kind` (`documentation`, \
                       `local`, `url`, `cloud`, or a webapp kind for cloud \
                       contexts), and a display name and description. Call \
                       automatic_list_context_entries with a source id next. \
                       Cloud contexts need the user to be signed in."
    )]
    async fn read_context(
        &self,
        params: Parameters<ReadContextParams>,
    ) -> Result<CallToolResult, McpError> {
        let context = match crate::core::read_context(&params.0.context) {
            Ok(c) => c,
            Err(e) => return Ok(tool_error(e)),
        };
        let sources = match crate::core::list_context_sources(&context).await {
            Ok(s) => s,
            Err(e) => {
                return Ok(tool_error(format!(
                    "Failed to list sources of context '{}': {}",
                    context.slug, e
                )))
            }
        };
        Ok(tool_json(&serde_json::json!({
            "slug": context.slug,
            "display_name": context.display_name,
            "description": context.description,
            "sources": sources,
        })))
    }

    #[tool(
        name = "automatic_list_context_entries",
        description = "List the readable entries of one source in a context. \
                       Returns `entries` (each with `path`, and `title` or \
                       `size` when known) and `truncated` when the source holds \
                       more than the listing limit. A URL source has one entry, \
                       `content`. Call automatic_read_context_entry with a path."
    )]
    async fn list_context_entries(
        &self,
        params: Parameters<ListContextEntriesParams>,
    ) -> Result<CallToolResult, McpError> {
        let context = match crate::core::read_context(&params.0.context) {
            Ok(c) => c,
            Err(e) => return Ok(tool_error(e)),
        };
        match crate::core::list_source_entries(&context, &params.0.source).await {
            Ok(listing) => Ok(tool_json(&listing)),
            Err(e) => Ok(tool_error(format!(
                "Failed to list source '{}' of context '{}': {}",
                params.0.source, context.slug, e
            ))),
        }
    }

    #[tool(
        name = "automatic_read_context_entry",
        description = "Read the text of one entry in a context source. Local \
                       files are confined to the source's folder, and every \
                       entry is capped at 512,000 bytes of UTF-8 text. URL \
                       sources are served from cache while fresh; a failed \
                       fetch is an error rather than stale content."
    )]
    async fn read_context_entry(
        &self,
        params: Parameters<ReadContextEntryParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = &params.0;
        let context = match crate::core::read_context(&p.context) {
            Ok(c) => c,
            Err(e) => return Ok(tool_error(e)),
        };
        match crate::core::read_source_entry(&context, &p.source, &p.path).await {
            Ok(text) => Ok(CallToolResult::success(vec![Content::text(text)])),
            Err(e) => Ok(tool_error(format!(
                "Failed to read '{}' from source '{}' of context '{}': {}",
                p.path, p.source, context.slug, e
            ))),
        }
    }

    #[tool(
        name = "automatic_attach_context",
        description = "Attach a context to a project or to a project group. \
                       Give `project`, `group`, or neither for the current \
                       project. A group's \
                       contexts are added to every member project and recorded \
                       as provided by that group. Idempotent. Contexts are read \
                       through MCP only, so no sync is needed."
    )]
    async fn attach_context(
        &self,
        params: Parameters<AttachContextParams>,
    ) -> Result<CallToolResult, McpError> {
        let (target, label) = match self.context_target(&params.0) {
            Ok(t) => t,
            Err(e) => return Ok(tool_error(e)),
        };
        let slug = &params.0.context;
        match crate::core::attach_context(&target, slug) {
            Ok(true) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Attached context '{}' to {}.",
                slug, label
            ))])),
            Ok(false) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Context '{}' was already attached to {}.",
                slug, label
            ))])),
            Err(e) => Ok(tool_error(format!(
                "Failed to attach context '{}': {}",
                slug, e
            ))),
        }
    }

    #[tool(
        name = "automatic_detach_context",
        description = "Detach a context from a project or from a project group. \
                       Give `project`, `group`, or neither for the current \
                       project. A context a \
                       group provides cannot be detached from a member project; \
                       detach it from the group instead. Idempotent."
    )]
    async fn detach_context(
        &self,
        params: Parameters<AttachContextParams>,
    ) -> Result<CallToolResult, McpError> {
        let (target, label) = match self.context_target(&params.0) {
            Ok(t) => t,
            Err(e) => return Ok(tool_error(e)),
        };
        let slug = &params.0.context;
        match crate::core::detach_context(&target, slug) {
            Ok(true) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Detached context '{}' from {}.",
                slug, label
            ))])),
            Ok(false) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Context '{}' was not attached to {}.",
                slug, label
            ))])),
            Err(e) => Ok(tool_error(format!(
                "Failed to detach context '{}': {}",
                slug, e
            ))),
        }
    }

    #[tool(
        name = "automatic_create_context",
        description = "Create a local context with a name and description, \
                       ready for pages. Only when the user asks. Returns the \
                       new context's slug. Link folders and web pages with \
                       automatic_add_context_folder and \
                       automatic_add_context_web_page; cloud sources can only \
                       be added by the user in the Automatic app."
    )]
    async fn create_context(
        &self,
        params: Parameters<CreateContextParams>,
    ) -> Result<CallToolResult, McpError> {
        match crate::core::create_local_context(&params.0.name, &params.0.description) {
            Ok(context) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Created context '{}' with slug '{}'. Add pages with \
                 automatic_write_context_page, and attach it with \
                 automatic_attach_context.",
                context.display_name, context.slug
            ))])),
            Err(e) => Ok(tool_error(format!("Failed to create context: {}", e))),
        }
    }

    #[tool(
        name = "automatic_write_context_page",
        description = "Create or replace a Markdown page in a local context. \
                       Give `title` (and optionally `folder`, e.g. \
                       \"Guides\") for a page addressed by name; the same \
                       title in the same folder replaces that page. Or give \
                       `path` to replace an existing page exactly. Write \
                       pages only when the user asks, or to record something \
                       durable the user has agreed, such as a decision. \
                       Returns the page path."
    )]
    async fn write_context_page(
        &self,
        params: Parameters<WriteContextPageParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = &params.0;
        let target = match (&p.path, &p.title) {
            (Some(path), None) => crate::core::PageTarget::Path(path),
            (None, Some(title)) => crate::core::PageTarget::Title {
                folder: p.folder.as_deref(),
                title,
            },
            _ => return Ok(tool_error("Give exactly one of `title` or `path`.".to_string())),
        };
        match crate::core::write_context_page(&p.context, target, &p.content) {
            Ok(path) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Saved page '{}' in context '{}'.",
                path, p.context
            ))])),
            Err(e) => Ok(tool_error(format!(
                "Failed to write page in context '{}': {}",
                p.context, e
            ))),
        }
    }

    #[tool(
        name = "automatic_move_context_page",
        description = "Move a page to another folder or rename it. Refuses to \
                       overwrite an existing page. Folders left empty are \
                       removed. Returns the new path."
    )]
    async fn move_context_page(
        &self,
        params: Parameters<MoveContextPageParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = &params.0;
        match crate::core::move_context_page(&p.context, &p.path, p.folder.as_deref(), p.title.as_deref()) {
            Ok(to) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Moved '{}' to '{}' in context '{}'.",
                p.path, to, p.context
            ))])),
            Err(e) => Ok(tool_error(format!("Failed to move page '{}': {}", p.path, e))),
        }
    }

    #[tool(
        name = "automatic_delete_context_page",
        description = "Delete a page from a local context. This cannot be \
                       undone; only do it when the user asks."
    )]
    async fn delete_context_page(
        &self,
        params: Parameters<DeleteContextPageParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = &params.0;
        match crate::core::delete_context_page(&p.context, &p.path) {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Deleted page '{}' from context '{}'.",
                p.path, p.context
            ))])),
            Err(e) => Ok(tool_error(format!("Failed to delete page '{}': {}", p.path, e))),
        }
    }

    #[tool(
        name = "automatic_add_context_folder",
        description = "Link a local folder or file into a local context. Agents \
                       read its Markdown and text files where they are; \
                       nothing is copied. Only when the user asks. The user's \
                       settings decide which folders are allowed (by default, \
                       only folders inside registered projects); hidden, \
                       credential and system folders are always refused. The \
                       user sees it marked as added by an agent. Returns the \
                       source id."
    )]
    async fn add_context_folder(
        &self,
        params: Parameters<AddContextFolderParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = &params.0;
        match crate::core::link_folder_for_agent(&p.context, &p.path, &p.name, &p.description) {
            Ok(id) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Linked '{}' into context '{}' as source '{}'.",
                p.path, p.context, id
            ))])),
            Err(e) => Ok(tool_error(format!(
                "Failed to link '{}' into context '{}': {}",
                p.path, p.context, e
            ))),
        }
    }

    #[tool(
        name = "automatic_add_context_web_page",
        description = "Link a web page into a local context. It is downloaded \
                       when an agent reads it. Only when the user asks. Pages \
                       an agent adds may only reach public internet \
                       addresses until the user keeps them in the Automatic \
                       app. Returns the source id."
    )]
    async fn add_context_web_page(
        &self,
        params: Parameters<AddContextWebPageParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = &params.0;
        match crate::core::link_web_page_for_agent(&p.context, &p.url, &p.name, &p.description, p.ttl_secs) {
            Ok(id) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Linked '{}' into context '{}' as source '{}'.",
                p.url.trim(),
                p.context,
                id
            ))])),
            Err(e) => Ok(tool_error(format!(
                "Failed to link '{}' into context '{}': {}",
                p.url, p.context, e
            ))),
        }
    }

    #[tool(
        name = "automatic_remove_context_source",
        description = "Remove a linked folder, file, web page or cloud source \
                       from a local context. The material itself is not \
                       touched. The context's pages cannot be removed this \
                       way. Only when the user asks."
    )]
    async fn remove_context_source(
        &self,
        params: Parameters<RemoveContextSourceParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = &params.0;
        match crate::core::remove_linked_source(&p.context, &p.source) {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Removed source '{}' from context '{}'.",
                p.source, p.context
            ))])),
            Err(e) => Ok(tool_error(format!(
                "Failed to remove source '{}' from context '{}': {}",
                p.source, p.context, e
            ))),
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
        description = "Sync a project's MCP server configs to its directory for all configured agent tools. Omit `name` to sync the current project. The project must have a directory path. A project with no agent tools configured syncs nothing; agents are never added by a sync."
    )]
    async fn sync_project(
        &self,
        params: Parameters<SyncProjectParams>,
    ) -> Result<CallToolResult, McpError> {
        let checkout = match self.project_checkout(params.0.name.as_deref()) {
            Ok(checkout) => checkout,
            Err(e) => return Ok(tool_error(e)),
        };
        let project = match load_project(&checkout) {
            Ok(project) => project,
            Err(e) => return Ok(tool_error(e)),
        };

        match crate::sync::sync_project(&project) {
            Ok(files) => {
                let mut response = serde_json::json!({
                    "synced_files": files,
                    "agents": project.agents,
                    "directory": project.directory,
                });
                if project.agents.is_empty() {
                    response["message"] = serde_json::Value::String(
                        "No agents configured, nothing synced. Add an agent tool to the project first."
                            .to_string(),
                    );
                }
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
        // Memory is keyed by project id, so every checkout of a project
        // shares it and an id with several checkouts is enough.
        let keys = match self.project_store_keys(params.0.project.as_deref()) {
            Ok(keys) => keys,
            Err(e) => return Ok(tool_error(e)),
        };
        match crate::memory::store_memory(
            &keys,
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
        // Memory is keyed by project id, so every checkout of a project
        // shares it and an id with several checkouts is enough.
        let keys = match self.project_store_keys(params.0.project.as_deref()) {
            Ok(keys) => keys,
            Err(e) => return Ok(tool_error(e)),
        };
        match crate::memory::get_memory(&keys, &params.0.key) {
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
        // Memory is keyed by project id, so every checkout of a project
        // shares it and an id with several checkouts is enough.
        let keys = match self.project_store_keys(params.0.project.as_deref()) {
            Ok(keys) => keys,
            Err(e) => return Ok(tool_error(e)),
        };
        match crate::memory::list_memories(&keys, params.0.pattern.as_deref()) {
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
        // Memory is keyed by project id, so every checkout of a project
        // shares it and an id with several checkouts is enough.
        let keys = match self.project_store_keys(params.0.project.as_deref()) {
            Ok(keys) => keys,
            Err(e) => return Ok(tool_error(e)),
        };
        match crate::memory::search_memories(&keys, &params.0.query) {
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
        // Memory is keyed by project id, so every checkout of a project
        // shares it and an id with several checkouts is enough.
        let keys = match self.project_store_keys(params.0.project.as_deref()) {
            Ok(keys) => keys,
            Err(e) => return Ok(tool_error(e)),
        };
        match crate::memory::delete_memory(&keys, &params.0.key) {
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
        // Memory is keyed by project id, so every checkout of a project
        // shares it and an id with several checkouts is enough.
        let keys = match self.project_store_keys(params.0.project.as_deref()) {
            Ok(keys) => keys,
            Err(e) => return Ok(tool_error(e)),
        };
        match crate::memory::clear_memories(
            &keys,
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
        // Claude's auto-memory lives under one folder, so this needs one
        // checkout.
        let checkout = match self.project_checkout(params.0.project.as_deref()) {
            Ok(checkout) => checkout,
            Err(e) => return Ok(tool_error(e)),
        };
        let project = match load_project(&checkout) {
            Ok(project) => project,
            Err(e) => return Ok(tool_error(e)),
        };

        match crate::memory::read_claude_memory(&project.directory) {
            Ok(content) => {
                let mut output = format!(
                    "# Claude Auto-Memory for '{}'\n\nDirectory: {}\n\n",
                    checkout.name, content.memory_dir
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
        // Features are keyed by project id, so every checkout of a project
        // shares them and an id with several checkouts is enough.
        let keys = match self.feature_store_keys(params.0.project.as_deref()) {
            Ok(keys) => keys,
            Err(e) => return Ok(tool_error(e)),
        };
        let include_archived = params.0.include_archived.unwrap_or(false);
        match crate::features::list_features(
            &keys,
            params.0.state.as_deref(),
            include_archived,
        ) {
            Ok(features) => {
                let output = crate::features::format_features_markdown(
                    &features,
                    &keys.name,
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
        // Features are keyed by project id, so every checkout of a project
        // shares them and an id with several checkouts is enough.
        let keys = match self.feature_store_keys(params.0.project.as_deref()) {
            Ok(keys) => keys,
            Err(e) => return Ok(tool_error(e)),
        };
        match crate::features::get_feature_with_updates(&keys, &params.0.feature_id) {
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
        description = "Create a new feature in a project. Defaults to backlog; pass state to create in another column. Returns the created feature including its id, which you will need for subsequent calls."
    )]
    async fn create_feature(
        &self,
        params: Parameters<CreateFeatureParams>,
    ) -> Result<CallToolResult, McpError> {
        // Features are keyed by project id, so every checkout of a project
        // shares them and an id with several checkouts is enough.
        let keys = match self.feature_store_keys(params.0.project.as_deref()) {
            Ok(keys) => keys,
            Err(e) => return Ok(tool_error(e)),
        };
        let p = params.0;
        match crate::features::create_feature(
            &keys,
            &p.title,
            p.description.as_deref().unwrap_or(""),
            p.priority.as_deref().unwrap_or("medium"),
            p.assignee.as_deref(),
            p.tags.as_deref().unwrap_or(&[]),
            p.linked_files.as_deref().unwrap_or(&[]),
            p.effort.as_deref(),
            p.created_by.as_deref(),
            p.state.as_deref(),
        ) {
            Ok(feature) => {
                let output = format!(
                    "Feature created successfully.\n\n**ID:** `{}`\n**Title:** {}\n**State:** {}\n**Priority:** {}\n",
                    feature.id, feature.title, feature.state, feature.priority
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
        // Features are keyed by project id, so every checkout of a project
        // shares them and an id with several checkouts is enough.
        let keys = match self.feature_store_keys(params.0.project.as_deref()) {
            Ok(keys) => keys,
            Err(e) => return Ok(tool_error(e)),
        };
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
        match crate::features::update_feature(&keys, &p.feature_id, patch) {
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
        // Features are keyed by project id, so every checkout of a project
        // shares them and an id with several checkouts is enough.
        let keys = match self.feature_store_keys(params.0.project.as_deref()) {
            Ok(keys) => keys,
            Err(e) => return Ok(tool_error(e)),
        };
        match crate::features::set_feature_state(
            &keys,
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
        // Features are keyed by project id, so every checkout of a project
        // shares them and an id with several checkouts is enough.
        let keys = match self.feature_store_keys(params.0.project.as_deref()) {
            Ok(keys) => keys,
            Err(e) => return Ok(tool_error(e)),
        };
        match crate::features::delete_feature(&keys, &params.0.feature_id) {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Feature '{}' deleted from project '{}'.",
                params.0.feature_id, keys.name
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
        // Features are keyed by project id, so every checkout of a project
        // shares them and an id with several checkouts is enough.
        let keys = match self.feature_store_keys(params.0.project.as_deref()) {
            Ok(keys) => keys,
            Err(e) => return Ok(tool_error(e)),
        };
        match crate::features::archive_feature(&keys, &params.0.feature_id) {
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
        // Features are keyed by project id, so every checkout of a project
        // shares them and an id with several checkouts is enough.
        let keys = match self.feature_store_keys(params.0.project.as_deref()) {
            Ok(keys) => keys,
            Err(e) => return Ok(tool_error(e)),
        };
        match crate::features::unarchive_feature(&keys, &params.0.feature_id) {
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
        // Features are keyed by project id, so every checkout of a project
        // shares them and an id with several checkouts is enough.
        let keys = match self.feature_store_keys(params.0.project.as_deref()) {
            Ok(keys) => keys,
            Err(e) => return Ok(tool_error(e)),
        };
        match crate::features::add_feature_update(
            &keys,
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
                 server configs, inspect projects, read the contexts attached to a project, \
                 track active sessions, and sync project configurations. \
                 Automatic knows which project this agent is working in. Omit the `project` \
                 argument (`name` on automatic_read_project and automatic_sync_project) to act \
                 on that project. Pass it only to act on a different project; it accepts a \
                 local_key, an id or a name from automatic_list_projects.",
            )
            .with_server_info(server_info)
    }
}

// ── Entry Point ──────────────────────────────────────────────────────────────

pub async fn run_mcp_server() -> Result<(), Box<dyn std::error::Error>> {
    // Resolved once. Agents start one `mcp-serve` per session, and the
    // working directory and `AUTOMATIC_PROJECT` do not change during it.
    let server = AutomaticMcpServer::with_current_project(crate::core::detect_current_project());
    let service = server.serve(stdio()).await?;
    service.waiting().await?;

    Ok(())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::with_test_home;

    fn params(name: &str, directory: &str, agents: Option<Vec<&str>>) -> RegisterProjectParams {
        RegisterProjectParams {
            name: name.to_string(),
            directory: directory.to_string(),
            description: None,
            agents: agents.map(|a| a.into_iter().map(String::from).collect()),
        }
    }

    #[test]
    fn project_argument_accepts_names_after_registry_migration() {
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let project = crate::core::Project {
                name: "legacy".into(),
                ..Default::default()
            };
            crate::core::save_project("legacy", &serde_json::to_string(&project).unwrap())
                .expect("save");
            crate::core::ensure_project_keys().expect("migrate");
            let migrated = crate::core::read_project("legacy").expect("read");
            let local_key = serde_json::from_str::<crate::core::Project>(&migrated)
                .expect("parse")
                .local_key;

            let server = AutomaticMcpServer::new();
            let by_name = server.project_checkout(Some("legacy")).expect("the name is still valid");
            assert_eq!(by_name.name, "legacy");
            let by_key = server.project_checkout(Some(&local_key)).expect("a local_key is accepted");
            assert_eq!(by_key.name, "legacy", "a key resolves to the registered project");
        });
    }

    #[test]
    fn project_argument_resolves_to_the_registry_name_or_lists_names() {
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let project = crate::core::Project {
                name: "Website".into(),
                local_key: "5b1f0c7e-0000-4000-8000-000000000001".into(),
                ..Default::default()
            };
            crate::core::save_project("Website", &serde_json::to_string(&project).unwrap())
                .expect("save");

            let server = AutomaticMcpServer::new();
            assert_eq!(server.project_checkout(Some("website")).unwrap().name, "Website");
            assert_eq!(
                server
                    .project_checkout(Some("5b1f0c7e-0000-4000-8000-000000000001"))
                    .unwrap()
                    .name,
                "Website"
            );
            let err = server.project_checkout(Some("nope")).unwrap_err();
            assert!(err.contains("Valid project names are: Website"), "{err}");
        });
    }

    #[test]
    fn memory_tool_given_a_name_or_key_stores_under_the_id() {
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let key = "5b1f0c7e-0000-4000-8000-000000000002";
            let id = "5b1f0c7e-0000-4000-8000-0000000000aa";
            let project = crate::core::Project {
                name: "site".into(),
                id: id.into(),
                local_key: key.into(),
                ..Default::default()
            };
            crate::core::save_project("site", &serde_json::to_string(&project).unwrap())
                .expect("save");

            let server = AutomaticMcpServer::new();
            for (ident, memory_key) in [(key, "by-key"), ("site", "by-name")] {
                let result = tauri::async_runtime::block_on(server.store_memory(Parameters(
                    StoreMemoryParams {
                        project: Some(ident.into()),
                        key: memory_key.into(),
                        value: "v".into(),
                        source: None,
                    },
                )))
                .expect("tool call");
                assert_ne!(result.is_error, Some(true), "{:?}", result.content);
            }
            let stored = crate::memory::get_all_memories(id).unwrap();
            assert!(stored.contains_key("by-key") && stored.contains_key("by-name"));
            assert!(crate::memory::get_all_memories("site").unwrap().is_empty());
            assert!(crate::memory::get_all_memories(key).unwrap().is_empty());

            let read = tauri::async_runtime::block_on(server.get_memory(Parameters(GetMemoryParams {
                project: Some("site".into()),
                key: "by-key".into(),
            })))
            .expect("tool call");
            assert_ne!(read.is_error, Some(true), "{:?}", read.content);
        });
    }

    #[test]
    fn feature_tool_creates_under_the_id_and_names_the_project() {
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let id = "5b1f0c7e-0000-4000-8000-0000000000bb";
            let project = crate::core::Project {
                name: "site".into(),
                id: id.into(),
                local_key: "5b1f0c7e-0000-4000-8000-000000000003".into(),
                tools: vec!["build".into()],
                ..Default::default()
            };
            crate::core::save_project("site", &serde_json::to_string(&project).unwrap())
                .expect("save");
            let keys = crate::core::project_store_keys("site").unwrap();
            assert_eq!(keys.id, id);

            let created = crate::features::create_feature(
                &keys, "t", "", "medium", None, &[], &[], None, None, None,
            )
            .expect("create");
            assert_eq!(created.project, "site", "rows are named, not keyed");

            // Feature tools need the Build plugin on; write its state
            // directly to skip the plugin resource sync.
            let dir = crate::core::get_automatic_dir().unwrap();
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("app_plugins.json"), r#"{"plugins":{"build":true}}"#).unwrap();

            let server = AutomaticMcpServer::new();
            let listed = tauri::async_runtime::block_on(server.list_features(Parameters(
                ListFeaturesParams {
                    project: Some("site".into()),
                    state: None,
                    include_archived: None,
                },
            )))
            .expect("tool call");
            assert_ne!(listed.is_error, Some(true), "{:?}", listed.content);
            let text = format!("{:?}", listed.content);
            assert!(text.contains(&created.id), "{text}");
            assert!(!text.contains(id), "the id stays internal: {text}");
            assert!(
                crate::features::list_features(&crate::core::ProjectStoreKeys::unregistered("site"), None, false)
                    .unwrap()
                    .is_empty(),
                "nothing is stored under the name"
            );
        });
    }

    #[test]
    fn related_projects_are_listed_by_name() {
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            for (name, id, key) in [
                ("site", "5b1f0c7e-0000-4000-8000-0000000000c1", "5b1f0c7e-0000-4000-8000-0000000000d1"),
                ("api", "5b1f0c7e-0000-4000-8000-0000000000c2", "5b1f0c7e-0000-4000-8000-0000000000d2"),
            ] {
                let project = crate::core::Project {
                    name: name.into(),
                    id: id.into(),
                    local_key: key.into(),
                    ..Default::default()
                };
                crate::core::save_project(name, &serde_json::to_string(&project).unwrap())
                    .expect("save");
            }
            let group = serde_json::json!({"name": "team", "projects": ["site", "api"]});
            crate::core::save_group("team", &group.to_string()).expect("save group");

            let server = AutomaticMcpServer::new();
            let result = tauri::async_runtime::block_on(server.get_related_projects(Parameters(
                GetRelatedProjectsParams { project: Some("site".into()) },
            )))
            .expect("tool call");
            let text = format!("{:?}", result.content);
            assert!(text.contains("**api**"), "{text}");
            assert!(!text.contains("0000000000c2"), "no id leaks: {text}");
        });
    }

    // ── current project ─────────────────────────────────────────────────

    const API_ID: &str = "5b1f0c7e-0000-4000-8000-0000000001a1";
    const API_KEY: &str = "5b1f0c7e-0000-4000-8000-0000000001b1";
    const SITE_ID: &str = "5b1f0c7e-0000-4000-8000-0000000001a2";
    const SITE_KEY: &str = "5b1f0c7e-0000-4000-8000-0000000001b2";
    const SITE_WT_KEY: &str = "5b1f0c7e-0000-4000-8000-0000000001b3";

    fn save_checkout(name: &str, id: &str, local_key: &str) {
        let project = crate::core::Project {
            name: name.into(),
            id: id.into(),
            local_key: local_key.into(),
            ..Default::default()
        };
        crate::core::save_project(name, &serde_json::to_string(&project).unwrap()).expect("save");
    }

    /// `api` has one checkout. `site` and `site-wt` are two checkouts of one
    /// project (a folder and its worktree).
    fn save_registry() -> Vec<crate::core::ProjectSummary> {
        save_checkout("api", API_ID, API_KEY);
        save_checkout("site", SITE_ID, SITE_KEY);
        save_checkout("site-wt", SITE_ID, SITE_WT_KEY);
        crate::core::get_project_summaries().expect("summaries")
    }

    /// The current project for `AUTOMATIC_PROJECT=env` run outside every
    /// registered folder.
    fn current_for(env: &str, summaries: &[crate::core::ProjectSummary]) -> crate::core::CurrentProject {
        crate::core::resolve_current_project(
            Some(env),
            Some(std::path::Path::new("/nowhere")),
            summaries,
            |dir: &str| std::path::PathBuf::from(dir),
        )
    }

    fn result_text(result: &CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn store(server: &AutomaticMcpServer, project: Option<&str>, key: &str) -> CallToolResult {
        tauri::async_runtime::block_on(server.store_memory(Parameters(StoreMemoryParams {
            project: project.map(String::from),
            key: key.into(),
            value: "v".into(),
            source: None,
        })))
        .expect("tool call")
    }

    fn read(server: &AutomaticMcpServer, name: Option<&str>) -> CallToolResult {
        tauri::async_runtime::block_on(
            server.read_project(Parameters(ReadProjectParams { name: name.map(String::from) })),
        )
        .expect("tool call")
    }

    #[test]
    fn an_omitted_project_uses_the_current_project() {
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let summaries = save_registry();
            let server = AutomaticMcpServer::with_current_project(current_for(API_ID, &summaries));

            let stored = store(&server, None, "k");
            assert_ne!(stored.is_error, Some(true), "{}", result_text(&stored));
            assert!(crate::memory::get_all_memories(API_ID).unwrap().contains_key("k"));

            let project = read(&server, None);
            assert_ne!(project.is_error, Some(true), "{}", result_text(&project));
            assert!(result_text(&project).contains(API_KEY), "{}", result_text(&project));
        });
    }

    #[test]
    fn an_explicit_key_id_or_name_overrides_the_current_project() {
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let summaries = save_registry();
            let server = AutomaticMcpServer::with_current_project(current_for(API_ID, &summaries));

            for (ident, key) in [(SITE_WT_KEY, "by-key"), (SITE_ID, "by-id"), ("Site", "by-name")] {
                let stored = store(&server, Some(ident), key);
                assert_ne!(stored.is_error, Some(true), "{ident}: {}", result_text(&stored));
            }
            let site = crate::memory::get_all_memories(SITE_ID).unwrap();
            assert!(["by-key", "by-id", "by-name"].iter().all(|k| site.contains_key(*k)), "{site:?}");
            assert!(crate::memory::get_all_memories(API_ID).unwrap().is_empty());

            let project = read(&server, Some(SITE_WT_KEY));
            assert!(result_text(&project).contains("site-wt"), "{}", result_text(&project));
        });
    }

    #[test]
    fn memory_accepts_an_id_shared_by_several_checkouts() {
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let summaries = save_registry();
            let current = current_for(SITE_ID, &summaries);
            assert!(
                matches!(current, crate::core::CurrentProject::Project { .. }),
                "outside both checkouts only the id is known: {current:?}"
            );
            let server = AutomaticMcpServer::with_current_project(current);

            let stored = store(&server, None, "shared");
            assert_ne!(stored.is_error, Some(true), "{}", result_text(&stored));
            assert!(crate::memory::get_all_memories(SITE_ID).unwrap().contains_key("shared"));
        });
    }

    #[test]
    fn a_checkout_tool_on_an_ambiguous_id_lists_each_checkout() {
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let summaries = save_registry();
            let server =
                AutomaticMcpServer::with_current_project(current_for(SITE_ID, &summaries));

            let result = read(&server, None);
            assert_eq!(result.is_error, Some(true));
            let text = result_text(&result);
            assert!(text.contains(&format!("site — no folder ({})", SITE_KEY)), "{text}");
            assert!(text.contains(&format!("site-wt — no folder ({})", SITE_WT_KEY)), "{text}");

            let explicit_id = AutomaticMcpServer::new();
            let result = read(&explicit_id, Some(SITE_ID));
            assert_eq!(result.is_error, Some(true), "an explicit shared id is ambiguous too");
        });
    }

    #[test]
    fn no_current_project_and_no_argument_is_a_helpful_error() {
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            save_registry();
            let server = AutomaticMcpServer::new();
            for result in [store(&server, None, "k"), read(&server, None)] {
                assert_eq!(result.is_error, Some(true));
                let text = result_text(&result);
                assert!(text.contains("Pass `project`"), "{text}");
                assert!(text.contains("automatic_list_projects"), "{text}");
            }
        });
    }

    #[test]
    fn list_projects_returns_keys_and_marks_the_current_checkout() {
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let summaries = save_registry();
            let server =
                AutomaticMcpServer::with_current_project(current_for(SITE_WT_KEY, &summaries));

            let result = tauri::async_runtime::block_on(server.list_projects()).expect("tool call");
            let rows: Vec<serde_json::Value> =
                serde_json::from_str(&result_text(&result)).expect("a JSON array");
            assert_eq!(
                rows,
                vec![
                    serde_json::json!({"name": "api", "local_key": API_KEY, "id": API_ID, "directory": "", "current": false}),
                    serde_json::json!({"name": "site", "local_key": SITE_KEY, "id": SITE_ID, "directory": "", "current": false}),
                    serde_json::json!({"name": "site-wt", "local_key": SITE_WT_KEY, "id": SITE_ID, "directory": "", "current": true}),
                ]
            );
        });
    }

    #[test]
    fn register_creates_registry_entry_and_project_config() {
        let home = tempfile::tempdir().expect("tempdir");
        let project_dir = home.path().join("workspace");
        std::fs::create_dir_all(&project_dir).expect("mkdir");

        with_test_home(home.path().to_path_buf(), || {
            let dir = project_dir.to_str().unwrap().to_string();
            let report =
                register_project_impl(&params("fresh", &dir, None)).expect("register succeeds");
            assert!(
                report.contains("Registered project 'fresh'"),
                "unexpected report: {report}"
            );

            let names = crate::core::list_projects().expect("list");
            assert!(
                names.iter().any(|n| n == "fresh"),
                "project missing from registry: {names:?}"
            );

            for file in [
                project_dir.join(".automatic.json"),
                project_dir.join(".automatic").join("project.json"),
            ] {
                assert!(file.exists(), "project file missing at {}", file.display());
            }

            let raw = crate::core::read_project("fresh").expect("read back");
            let project: crate::core::Project =
                serde_json::from_str(&raw).expect("parse roundtrip");
            assert_eq!(project.name, "fresh");
            assert_eq!(project.directory, dir);
            assert!(uuid::Uuid::parse_str(&project.id).is_ok(), "register mints an id");
            assert!(
                uuid::Uuid::parse_str(&project.local_key).is_ok(),
                "register mints a local_key"
            );
        });
    }

    #[test]
    fn related_peers_show_every_checkout_with_its_folder() {
        const ID_A: &str = "11111111-1111-4111-8111-111111111111";
        const ID_B: &str = "22222222-2222-4222-8222-222222222222";
        const ID_C: &str = "33333333-3333-4333-8333-333333333333";
        let summary = |name: &str, id: &str, key: &str, dir: &str| crate::core::ProjectSummary {
            local_key: key.into(),
            id: id.into(),
            name: name.into(),
            directory: dir.into(),
        };
        let this = summary("api", ID_A, "aaaaaaaa-0000-4000-8000-000000000001", "/w/api");
        let index = crate::core::ProjectKeyIndex::new(vec![
            this.clone(),
            summary("website", ID_B, "bbbbbbbb-0000-4000-8000-000000000001", "/w/consultmed/website"),
            summary("website", ID_C, "cccccccc-0000-4000-8000-000000000001", "/w/_active/website"),
        ]);
        let members: Vec<String> =
            [ID_A, ID_B, ID_C, "gone"].iter().map(|s| s.to_string()).collect();
        let peers = related_peers(&index, &members, &this);
        let shown: Vec<(&str, &str)> =
            peers.iter().map(|p| (p.name.as_str(), p.directory.as_str())).collect();
        assert_eq!(
            shown,
            vec![
                ("website", "/w/consultmed/website"),
                ("website", "/w/_active/website"),
                ("gone", ""),
            ],
            "this project is left out; same-named peers keep their folders; unknown members stay"
        );
    }

    #[test]
    fn register_allows_a_name_another_project_uses() {
        let home = tempfile::tempdir().expect("tempdir");
        let ws1 = home.path().join("ws1");
        let ws2 = home.path().join("ws2");
        std::fs::create_dir_all(&ws1).expect("mkdir");
        std::fs::create_dir_all(&ws2).expect("mkdir");

        with_test_home(home.path().to_path_buf(), || {
            register_project_impl(&params("alpha", ws1.to_str().unwrap(), None))
                .expect("first registration");
            let report = register_project_impl(&params("alpha", ws2.to_str().unwrap(), None))
                .expect("a shared name registers");
            assert!(report.contains("local_key: "), "{report}");

            let summaries = crate::core::get_project_summaries().expect("summaries");
            assert_eq!(summaries.len(), 2);
            assert!(summaries.iter().all(|s| s.name == "alpha"));
            assert_ne!(summaries[0].id, summaries[1].id);
            let dirs: Vec<&str> = summaries.iter().map(|s| s.directory.as_str()).collect();
            assert!(dirs.contains(&ws1.to_str().unwrap()) && dirs.contains(&ws2.to_str().unwrap()));

            let err = crate::core::resolve_project_target(Some("alpha"), &crate::core::CurrentProject::None, &summaries)
                .expect_err("a shared name is ambiguous");
            assert!(err.contains(&summaries[0].local_key) && err.contains(&summaries[1].local_key));
        });
    }

    #[test]
    fn register_rejects_directory_already_registered() {
        let home = tempfile::tempdir().expect("tempdir");
        let ws = home.path().join("ws");
        std::fs::create_dir_all(&ws).expect("mkdir");

        with_test_home(home.path().to_path_buf(), || {
            register_project_impl(&params("alpha", ws.to_str().unwrap(), None))
                .expect("first registration");

            let err = register_project_impl(&params("beta", ws.to_str().unwrap(), None))
                .expect_err("directory claimed by another project must be rejected");
            assert!(
                err.contains("already registered as project 'alpha'"),
                "unexpected error: {err}"
            );
        });
    }

    #[test]
    fn register_rejects_missing_directory() {
        let home = tempfile::tempdir().expect("tempdir");
        let missing = home.path().join("does-not-exist");

        with_test_home(home.path().to_path_buf(), || {
            let err = register_project_impl(&params("ghost", missing.to_str().unwrap(), None))
                .expect_err("missing directory must be rejected");
            assert!(
                err.contains("does not exist"),
                "unexpected error: {err}"
            );

            let names = crate::core::list_projects().expect("list");
            assert!(
                !names.iter().any(|n| n == "ghost"),
                "nothing should have been registered: {names:?}"
            );
        });
    }

    #[test]
    fn register_rejects_relative_directory() {
        let home = tempfile::tempdir().expect("tempdir");

        with_test_home(home.path().to_path_buf(), || {
            let err = register_project_impl(&params("rel", "some/relative/path", None))
                .expect_err("relative directory must be rejected");
            assert!(
                err.contains("absolute path"),
                "unexpected error: {err}"
            );
        });
    }

    #[test]
    fn register_rejects_invalid_name() {
        let home = tempfile::tempdir().expect("tempdir");
        let ws = home.path().join("ws");
        std::fs::create_dir_all(&ws).expect("mkdir");

        with_test_home(home.path().to_path_buf(), || {
            let err = register_project_impl(&params("../escape", ws.to_str().unwrap(), None))
                .expect_err("path traversal name must be rejected");
            assert!(
                err.contains("Invalid project name"),
                "unexpected error: {err}"
            );
        });
    }

    #[test]
    fn register_rejects_unknown_agent_without_registering() {
        let home = tempfile::tempdir().expect("tempdir");
        let ws = home.path().join("ws");
        std::fs::create_dir_all(&ws).expect("mkdir");

        with_test_home(home.path().to_path_buf(), || {
            let err = register_project_impl(&params(
                "bad-agent",
                ws.to_str().unwrap(),
                Some(vec!["claude", "not-an-agent"]),
            ))
            .expect_err("unknown agent id must be rejected");
            assert!(
                err.contains("Unknown agent 'not-an-agent'"),
                "unexpected error: {err}"
            );
            assert!(
                err.contains("claude"),
                "error should list valid agent ids: {err}"
            );

            let names = crate::core::list_projects().expect("list");
            assert!(
                !names.iter().any(|n| n == "bad-agent"),
                "nothing should have been registered: {names:?}"
            );
        });
    }

    #[test]
    fn register_refuses_orphan_config_and_leaves_it_untouched() {
        let home = tempfile::tempdir().expect("tempdir");
        let ws = home.path().join("orphan-ws");
        let automatic_dir = ws.join(".automatic");
        std::fs::create_dir_all(&automatic_dir).expect("mkdir");
        let orphan_raw = r#"{"name":"orphan","directory":"x"}"#;
        std::fs::write(automatic_dir.join("project.json"), orphan_raw).expect("write orphan");

        with_test_home(home.path().to_path_buf(), || {
            let err = register_project_impl(&params("fresh", ws.to_str().unwrap(), None))
                .expect_err("orphan on-disk config must be refused");
            assert!(
                err.contains("not registered"),
                "unexpected error: {err}"
            );

            // The orphan config must be left exactly as it was.
            let on_disk =
                std::fs::read_to_string(automatic_dir.join("project.json")).expect("reread");
            assert_eq!(on_disk, orphan_raw);

            let names = crate::core::list_projects().expect("list");
            assert!(
                !names.iter().any(|n| n == "fresh"),
                "nothing should have been registered: {names:?}"
            );
        });
    }

    #[test]
    fn register_with_agents_syncs_agent_configs() {
        let home = tempfile::tempdir().expect("tempdir");
        let ws = home.path().join("ws");
        std::fs::create_dir_all(&ws).expect("mkdir");

        with_test_home(home.path().to_path_buf(), || {
            let report = register_project_impl(&params(
                "with-agents",
                ws.to_str().unwrap(),
                Some(vec!["claude"]),
            ))
            .expect("register with agents");
            assert!(
                report.contains("Agents: claude"),
                "unexpected report: {report}"
            );
            assert!(
                !report.contains("Warning"),
                "sync should not have failed: {report}"
            );

            // Claude Code owns .mcp.json and CLAUDE.md in the project root.
            let mcp = ws.join(".mcp.json");
            assert!(mcp.exists(), ".mcp.json missing at {}", mcp.display());
            let mcp_content = std::fs::read_to_string(&mcp).expect("read .mcp.json");
            assert!(
                mcp_content.contains("automatic"),
                "the automatic server should be injected into .mcp.json: {mcp_content}"
            );
            assert!(ws.join("CLAUDE.md").exists(), "CLAUDE.md missing");

            let raw = crate::core::read_project("with-agents").expect("read back");
            let project: crate::core::Project =
                serde_json::from_str(&raw).expect("parse roundtrip");
            assert!(
                project.agents.iter().any(|a| a == "claude"),
                "agent missing from saved project: {:?}",
                project.agents
            );
        });
    }
}
