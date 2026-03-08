use crate::activity::{self, ActivityEvent};
use crate::context;
use crate::core;
use crate::sync;

// ── Projects ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_projects() -> Result<Vec<String>, String> {
    core::list_projects()
}

#[tauri::command]
pub fn read_project(name: &str) -> Result<String, String> {
    core::read_project(name)
}

#[tauri::command]
pub fn autodetect_project_dependencies(name: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let updated = sync::autodetect_project_dependencies(&project)?;
    serde_json::to_string_pretty(&updated).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_project(name: &str, data: &str) -> Result<(), String> {
    let incoming: core::Project =
        serde_json::from_str(data).map_err(|e| format!("Invalid project data: {}", e))?;

    // No directory configured yet -- just persist to the registry and return.
    // There is nothing to sync until the user has pointed us at a real directory.
    if incoming.directory.is_empty() {
        return core::save_project(name, data);
    }

    // Detect whether this is a brand-new project (no existing registry entry).
    let is_new = core::read_project(name).is_err();

    if is_new {
        // ── Case 1: Project is being added for the first time ─────────────
        //
        // Save the initial state so the project exists in the registry even if
        // the subsequent autodetect fails for any reason.  Then run full
        // autodetect to discover all agents, skills, and MCP servers that are
        // already present in the directory.  The enriched project config is
        // written back to disk by sync_project (via sync_project_without_autodetect).
        //
        // Nothing is deleted during this step -- autodetect only adds findings.
        core::save_project(name, data)?;

        // Log project creation.
        activity::log(name, ActivityEvent::ProjectCreated, "Project created", name);

        // Errors are intentionally swallowed: partial success (project saved
        // but no agent configs written because the directory has no AI tools)
        // is better than returning a hard error to the frontend.
        let written = sync::sync_project(&incoming);
        if let Ok(ref files) = written {
            if !files.is_empty() {
                let detail = format!(
                    "{} file{}",
                    files.len(),
                    if files.len() == 1 { "" } else { "s" }
                );
                activity::log(
                    name,
                    ActivityEvent::ProjectSynced,
                    "Synced agent configs",
                    &detail,
                );
            }
        }
    } else {
        // ── Case 2 / ongoing saves: Existing project update ───────────────
        //
        // Use sync_without_autodetect so the user's explicit agent/skill
        // removals are respected -- we never re-add an agent the user
        // intentionally removed just because its config files still exist.
        //
        // Exception: when the user *adds* a new agent, read its existing
        // config files (if any) to discover MCP servers it already has
        // configured, and merge those into the project so they are not
        // silently discarded when Automatic writes its own config.
        let existing_project = core::read_project(name)
            .ok()
            .and_then(|raw| serde_json::from_str::<core::Project>(&raw).ok());

        let mut enriched = incoming.clone();

        if let Some(ref existing) = existing_project {
            // ── Diff and log agent changes ───────────────────────────────
            for agent in incoming
                .agents
                .iter()
                .filter(|a| !existing.agents.contains(a))
            {
                activity::log(name, ActivityEvent::AgentAdded, "Agent added", agent);
            }
            for agent in existing
                .agents
                .iter()
                .filter(|a| !incoming.agents.contains(a))
            {
                activity::log(name, ActivityEvent::AgentRemoved, "Agent removed", agent);
            }

            // ── Diff and log skill changes ───────────────────────────────
            for skill in incoming
                .skills
                .iter()
                .filter(|s| !existing.skills.contains(s))
            {
                activity::log(name, ActivityEvent::SkillAdded, "Skill added", skill);
            }
            for skill in existing
                .skills
                .iter()
                .filter(|s| !incoming.skills.contains(s))
            {
                activity::log(name, ActivityEvent::SkillRemoved, "Skill removed", skill);
            }

            // ── Diff and log MCP server changes ──────────────────────────
            for server in incoming
                .mcp_servers
                .iter()
                .filter(|s| !existing.mcp_servers.contains(s))
            {
                activity::log(
                    name,
                    ActivityEvent::McpServerAdded,
                    "MCP server added",
                    server,
                );
            }
            for server in existing
                .mcp_servers
                .iter()
                .filter(|s| !incoming.mcp_servers.contains(s))
            {
                activity::log(
                    name,
                    ActivityEvent::McpServerRemoved,
                    "MCP server removed",
                    server,
                );
            }

            let new_agent_ids: Vec<String> = incoming
                .agents
                .iter()
                .filter(|a| !existing.agents.contains(a))
                .cloned()
                .collect();

            if !new_agent_ids.is_empty() {
                let dir = std::path::PathBuf::from(&incoming.directory);
                let discovered = sync::discover_new_agent_mcp_configs(&dir, &new_agent_ids);

                for (server_name, config_str) in discovered {
                    // Add the server name to the project's selection list.
                    if !enriched.mcp_servers.contains(&server_name) {
                        enriched.mcp_servers.push(server_name.clone());
                    }
                    // Persist the config to the global registry so that
                    // sync_project_without_autodetect can include it when
                    // building the mcpServers map written to disk.
                    let _ = core::save_mcp_server_config(&server_name, &config_str);
                }
            }
        }

        let enriched_data = serde_json::to_string_pretty(&enriched).map_err(|e| e.to_string())?;
        core::save_project(name, &enriched_data)?;

        if !enriched.agents.is_empty() {
            let written = sync::sync_project_without_autodetect(&mut enriched)?;
            if !written.is_empty() {
                let detail = format!(
                    "{} file{}",
                    written.len(),
                    if written.len() == 1 { "" } else { "s" }
                );
                activity::log(
                    name,
                    ActivityEvent::ProjectSynced,
                    "Synced agent configs",
                    &detail,
                );
            }
        }
    }

    // ── Fire-and-forget AI recommendations ───────────────────────────────────
    // Spawn on the Tauri async runtime so we never block the UI.
    // The throttle check inside ai_generate_project_recommendations_bg ensures
    // this runs at most once per 24 hours automatically; on a brand-new project
    // (is_new == true) we always run it regardless of the throttle.
    {
        let project_name = name.to_string();
        let is_new_project = is_new;
        tauri::async_runtime::spawn(async move {
            if let Err(e) = run_ai_recommendations_bg(&project_name, is_new_project).await {
                eprintln!("[automatic] AI recommendations skipped for '{}': {}", project_name, e);
            }
        });
    }

    Ok(())
}

/// Background helper: run AI recommendations for a project, respecting the
/// once-per-day throttle unless `force` is true (used for new projects).
async fn run_ai_recommendations_bg(project: &str, force: bool) -> Result<(), String> {
    // Verify a key exists before attempting the (potentially slow) AI call.
    crate::core::ai::resolve_api_key(None)?;

    // Honour the throttle for existing projects; always run for new ones.
    if !force && crate::recommendations::ai_recommendations_throttled(project)? {
        return Ok(());
    }

    // Delegate to the full command implementation (re-uses all the same logic).
    super::recommendations::ai_generate_project_recommendations(project, Some(force)).await?;

    Ok(())
}

#[tauri::command]
pub fn rename_project(old_name: &str, new_name: &str) -> Result<(), String> {
    core::rename_project(old_name, new_name)?;

    // Re-sync agent configs so AUTOMATIC_PROJECT reflects the new name.
    let raw = core::read_project(new_name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    if !project.directory.is_empty() && !project.agents.is_empty() {
        let written = sync::sync_project(&project);
        if let Ok(ref files) = written {
            if !files.is_empty() {
                let detail = format!(
                    "{} file{}",
                    files.len(),
                    if files.len() == 1 { "" } else { "s" }
                );
                activity::log(
                    new_name,
                    ActivityEvent::ProjectSynced,
                    "Synced agent configs",
                    &detail,
                );
            }
        }
        written?;
    }
    activity::log(
        new_name,
        ActivityEvent::ProjectUpdated,
        "Project renamed",
        &format!("{} → {}", old_name, new_name),
    );

    Ok(())
}

#[tauri::command]
pub fn delete_project(name: &str) -> Result<(), String> {
    core::delete_project(name)
}

// ── Project Context ───────────────────────────────────────────────────────────

/// Return the parsed `.automatic/context.json` for the given project as JSON.
/// Returns an empty `ProjectContext` (all empty maps) when the file does not
/// exist yet — callers can use this to show an empty-state UI.
#[tauri::command]
pub fn get_project_context(name: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let ctx = context::get_project_context(&project.directory)?;
    serde_json::to_string(&ctx).map_err(|e| e.to_string())
}

/// Return the raw text content of `.automatic/context.json` for editing.
/// Returns an empty string when the file does not exist yet.
#[tauri::command]
pub fn read_project_context_raw(name: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    if project.directory.is_empty() {
        return Err("Project has no directory configured".into());
    }
    let path = std::path::PathBuf::from(&project.directory)
        .join(".automatic")
        .join("context.json");
    if !path.exists() {
        return Ok(String::new());
    }
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

/// Write raw JSON text to `.automatic/context.json`, creating the `.automatic`
/// directory if it does not exist.  The content is validated as JSON before
/// being written so the file is never left in an unparseable state.
#[tauri::command]
pub fn save_project_context_raw(name: &str, content: &str) -> Result<(), String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    if project.directory.is_empty() {
        return Err("Project has no directory configured".into());
    }

    // Validate JSON before touching disk.
    let _: serde_json::Value =
        serde_json::from_str(content).map_err(|e| format!("Invalid JSON: {}", e))?;

    let dir = std::path::PathBuf::from(&project.directory).join(".automatic");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("context.json"), content).map_err(|e| e.to_string())
}

/// Use AI to analyse the project directory and generate a `.automatic/context.json`
/// scaffold.  Returns the generated JSON string so the frontend can preview it
/// before saving.  The caller is responsible for writing it to disk via
/// `save_project_context_raw`.
///
/// Approach: Rust builds a project snapshot (directory tree + key config/doc
/// file contents) and passes it directly in the prompt.  A single
/// `chat_structured` call then converts that snapshot into a guaranteed-valid
/// JSON object using the Anthropic structured outputs API.  No tool-use loop,
/// no turn counting, no markdown-fence guessing.
#[tauri::command]
pub async fn ai_generate_context(name: &str) -> Result<String, String> {
    use serde_json::json;

    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    if project.directory.is_empty() {
        return Err("Project has no directory configured".into());
    }

    // Build a structured snapshot of the project directory in Rust — this gives
    // the model a directory tree plus the full content of key config files and
    // the first 80 lines of the README, all in one go, with no tool calls needed.
    let snapshot = context::build_project_snapshot(&project.directory)?;

    // Structured outputs only allow `additionalProperties: false` — open string
    // maps are not supported.  We use arrays of keyed objects instead, then
    // convert the response back to the on-disk map format before returning.
    let context_schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["commands", "entry_points", "concepts", "conventions", "gotchas", "docs"],
        "properties": {
            "commands": {
                "type": "array",
                "description": "Shell commands extracted from the project (build, test, lint, dev, etc.).",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["name", "command"],
                    "properties": {
                        "name":    { "type": "string", "description": "Short label, e.g. 'build'." },
                        "command": { "type": "string", "description": "The shell command to run." }
                    }
                }
            },
            "entry_points": {
                "type": "array",
                "description": "Primary source entry-point files.",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["label", "path"],
                    "properties": {
                        "label": { "type": "string", "description": "Short label, e.g. 'app', 'cli'." },
                        "path":  { "type": "string", "description": "Relative path to the file." }
                    }
                }
            },
            "concepts": {
                "type": "array",
                "description": "Key architecture concepts (3-8) with the files that implement them.",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["name", "summary", "files"],
                    "properties": {
                        "name":    { "type": "string", "description": "Concept name." },
                        "summary": { "type": "string", "description": "One or two sentence description." },
                        "files":   { "type": "array", "items": { "type": "string" }, "description": "Relative file paths." }
                    }
                }
            },
            "conventions": {
                "type": "array",
                "description": "Coding or project conventions observed in the codebase.",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["name", "description"],
                    "properties": {
                        "name":        { "type": "string" },
                        "description": { "type": "string" }
                    }
                }
            },
            "gotchas": {
                "type": "array",
                "description": "Known pitfalls, build quirks, or unusual dependencies.",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["name", "description"],
                    "properties": {
                        "name":        { "type": "string" },
                        "description": { "type": "string" }
                    }
                }
            },
            "docs": {
                "type": "array",
                "description": "Significant documentation files.",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["label", "path", "summary"],
                    "properties": {
                        "label":   { "type": "string" },
                        "path":    { "type": "string", "description": "Relative path." },
                        "summary": { "type": "string", "description": "One sentence description." }
                    }
                }
            }
        }
    });

    let system = "You are a senior software engineer producing structured documentation \
        for AI coding agents. You will be given a project snapshot (directory tree, \
        config files, and README). Analyse it and populate all fields. \
        Rules: \
        - commands: extract from package.json scripts, Makefile targets, Cargo.toml [dev], etc. \
        - entry_points: identify main source files (src/main.rs, src/index.ts, app.py, etc.). \
        - concepts: 3-8 key architecture concepts with the files that implement them. \
        - conventions: naming, file structure, or code style patterns you can observe. \
        - gotchas: build quirks, unusual dependencies, or known pitfalls. \
        - docs: significant documentation files visible in the tree. \
        Use empty arrays [] for sections where the snapshot provides no clear evidence. \
        File paths must be relative to the project root. \
        Keep all summaries to 1-2 sentences.";

    let user_msg = format!(
        "Project name: \"{}\"\n\nProject snapshot:\n\n{}",
        name, snapshot
    );

    let generated = crate::core::ai::chat_structured(
        vec![crate::core::ai::AiMessage {
            role: "user".into(),
            content: user_msg,
        }],
        None,
        None,
        Some(system.to_string()),
        Some(4096),
        context_schema,
    )
    .await?;

    // Convert the array-of-objects response into the on-disk map format that
    // context.rs / the frontend expect.
    let ai_val: serde_json::Value = serde_json::from_str(&generated)
        .map_err(|e| format!("Unexpected JSON parse failure: {}", e))?;

    let output = convert_arrays_to_context_maps(&ai_val)?;
    serde_json::to_string_pretty(&output).map_err(|e| e.to_string())
}

/// Convert the array-of-objects schema returned by the structured outputs API
/// into the `{ "key": value }` map format used by `context.json` on disk.
///
/// Input shape (from AI):
/// ```json
/// {
///   "commands":    [{ "name": "build", "command": "cargo build" }, ...],
///   "entry_points":[{ "label": "app",  "path": "src/main.rs" }, ...],
///   "concepts":    [{ "name": "MCP",   "summary": "...", "files": [...] }, ...],
///   "conventions": [{ "name": "naming","description": "..." }, ...],
///   "gotchas":     [{ "name": "lock",  "description": "..." }, ...],
///   "docs":        [{ "label": "README","path": "README.md", "summary": "..." }, ...]
/// }
/// ```
///
/// Output shape (context.json format):
/// ```json
/// {
///   "commands":    { "build": "cargo build" },
///   "entry_points":{ "app": "src/main.rs" },
///   "concepts":    { "MCP": { "summary": "...", "files": [...] } },
///   "conventions": { "naming": "..." },
///   "gotchas":     { "lock": "..." },
///   "docs":        { "README": { "path": "README.md", "summary": "..." } }
/// }
/// ```
fn convert_arrays_to_context_maps(ai: &serde_json::Value) -> Result<serde_json::Value, String> {
    use serde_json::{Map, Value};

    fn arr<'a>(v: &'a Value, field: &str) -> Result<&'a Vec<Value>, String> {
        v.get(field)
            .and_then(|x| x.as_array())
            .ok_or_else(|| format!("missing or non-array field '{}'", field))
    }

    fn str_field<'a>(item: &'a Value, field: &str) -> Result<&'a str, String> {
        item.get(field)
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("item missing string field '{}'", field))
    }

    let mut out = Map::new();

    // commands: [{ name, command }] → { name: command }
    let mut commands = Map::new();
    for item in arr(ai, "commands")? {
        let name = str_field(item, "name")?.to_string();
        let cmd  = str_field(item, "command")?.to_string();
        commands.insert(name, Value::String(cmd));
    }
    out.insert("commands".into(), Value::Object(commands));

    // entry_points: [{ label, path }] → { label: path }
    let mut eps = Map::new();
    for item in arr(ai, "entry_points")? {
        let label = str_field(item, "label")?.to_string();
        let path  = str_field(item, "path")?.to_string();
        eps.insert(label, Value::String(path));
    }
    out.insert("entry_points".into(), Value::Object(eps));

    // concepts: [{ name, summary, files }] → { name: { summary, files } }
    let mut concepts = Map::new();
    for item in arr(ai, "concepts")? {
        let name    = str_field(item, "name")?.to_string();
        let summary = str_field(item, "summary")?.to_string();
        let files   = item.get("files").cloned().unwrap_or(Value::Array(vec![]));
        let mut concept = Map::new();
        concept.insert("summary".into(), Value::String(summary));
        concept.insert("files".into(), files);
        concepts.insert(name, Value::Object(concept));
    }
    out.insert("concepts".into(), Value::Object(concepts));

    // conventions: [{ name, description }] → { name: description }
    let mut conventions = Map::new();
    for item in arr(ai, "conventions")? {
        let name = str_field(item, "name")?.to_string();
        let desc = str_field(item, "description")?.to_string();
        conventions.insert(name, Value::String(desc));
    }
    out.insert("conventions".into(), Value::Object(conventions));

    // gotchas: [{ name, description }] → { name: description }
    let mut gotchas = Map::new();
    for item in arr(ai, "gotchas")? {
        let name = str_field(item, "name")?.to_string();
        let desc = str_field(item, "description")?.to_string();
        gotchas.insert(name, Value::String(desc));
    }
    out.insert("gotchas".into(), Value::Object(gotchas));

    // docs: [{ label, path, summary }] → { label: { path, summary } }
    let mut docs = Map::new();
    for item in arr(ai, "docs")? {
        let label   = str_field(item, "label")?.to_string();
        let path    = str_field(item, "path")?.to_string();
        let summary = str_field(item, "summary")?.to_string();
        let mut doc = Map::new();
        doc.insert("path".into(), Value::String(path));
        doc.insert("summary".into(), Value::String(summary));
        docs.insert(label, Value::Object(doc));
    }
    out.insert("docs".into(), Value::Object(docs));

    Ok(Value::Object(out))
}

// ── Project Sync ─────────────────────────────────────────────────────────────

#[tauri::command]
pub fn sync_project(name: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let written = sync::sync_project(&project)?;
    if !written.is_empty() {
        let detail = format!(
            "{} file{}",
            written.len(),
            if written.len() == 1 { "" } else { "s" }
        );
        activity::log(
            name,
            ActivityEvent::ProjectSynced,
            "Synced agent configs",
            &detail,
        );
    }
    serde_json::to_string_pretty(&written).map_err(|e| e.to_string())
}

/// Return the list of file/directory paths that would be removed if the given
/// agent were removed from the project.  Read-only -- used to populate the
/// confirmation dialog before the user commits to the removal.
#[tauri::command]
pub fn get_agent_cleanup_preview(name: &str, agent_id: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let preview = sync::get_agent_cleanup_preview(&project, agent_id)?;
    serde_json::to_string(&preview).map_err(|e| e.to_string())
}

/// Remove an agent from a project and delete all files it wrote.
/// The project config is persisted and remaining agents are re-synced.
/// Returns a JSON array of paths that were removed or modified.
#[tauri::command]
pub fn remove_agent_from_project(name: &str, agent_id: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let mut project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let removed = sync::remove_agent_from_project(&mut project, agent_id)?;
    activity::log(name, ActivityEvent::AgentRemoved, "Agent removed", agent_id);
    serde_json::to_string(&removed).map_err(|e| e.to_string())
}

/// Check whether the on-disk agent configs have drifted from what Automatic would
/// generate.  Returns a JSON-serialised [`sync::DriftReport`] describing which
/// agents and files are out of sync.  This is a read-only operation.
#[tauri::command]
pub fn check_project_drift(name: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let report = sync::check_project_drift(&project)?;
    serde_json::to_string(&report).map_err(|e| e.to_string())
}

// ── Cross-cutting helpers ────────────────────────────────────────────────────
//
// These are used by skills, rules, mcp_servers, and skill_store modules when
// a registry item is saved or deleted and projects referencing it need updating.

pub(crate) fn with_each_project_mut<F>(mut f: F)
where
    F: FnMut(&str, &mut core::Project),
{
    let project_names = match core::list_projects() {
        Ok(names) => names,
        Err(e) => {
            eprintln!("Failed to list projects for config updates: {}", e);
            return;
        }
    };

    for project_name in project_names {
        let raw = match core::read_project(&project_name) {
            Ok(raw) => raw,
            Err(e) => {
                eprintln!("Failed to read project '{}': {}", project_name, e);
                continue;
            }
        };

        let mut project: core::Project = match serde_json::from_str(&raw) {
            Ok(project) => project,
            Err(e) => {
                eprintln!("Failed to parse project '{}': {}", project_name, e);
                continue;
            }
        };

        f(&project_name, &mut project);
    }
}

pub(crate) fn sync_project_if_configured(project_name: &str, project: &mut core::Project) {
    if project.directory.is_empty() || project.agents.is_empty() {
        return;
    }

    if let Err(e) = sync::sync_project_without_autodetect(project) {
        eprintln!(
            "Failed to sync project '{}' after registry update: {}",
            project_name, e
        );
    }
}

pub(crate) fn sync_projects_referencing_skill(skill_name: &str) {
    with_each_project_mut(|project_name, project| {
        if project.skills.iter().any(|skill| skill == skill_name) {
            sync_project_if_configured(project_name, project);
        }
    });
}

pub(crate) fn sync_projects_referencing_mcp_server(server_name: &str) {
    with_each_project_mut(|project_name, project| {
        if project
            .mcp_servers
            .iter()
            .any(|server| server == server_name)
        {
            sync_project_if_configured(project_name, project);
        }
    });
}

pub(crate) fn prune_skill_from_projects(skill_name: &str) {
    with_each_project_mut(|project_name, project| {
        let before = project.skills.len();
        project.skills.retain(|skill| skill != skill_name);

        if project.skills.len() != before {
            project.updated_at = chrono::Utc::now().to_rfc3339();
            match serde_json::to_string_pretty(project).map_err(|e| e.to_string()) {
                Ok(data) => {
                    if let Err(e) = core::save_project(project_name, &data) {
                        eprintln!("Failed to update project '{}': {}", project_name, e);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to serialize project '{}': {}", project_name, e);
                }
            }
            sync_project_if_configured(project_name, project);
        }
    });
}

pub(crate) fn prune_mcp_server_from_projects(server_name: &str) {
    with_each_project_mut(|project_name, project| {
        let before = project.mcp_servers.len();
        project.mcp_servers.retain(|server| server != server_name);

        if project.mcp_servers.len() != before {
            project.updated_at = chrono::Utc::now().to_rfc3339();
            match serde_json::to_string_pretty(project).map_err(|e| e.to_string()) {
                Ok(data) => {
                    if let Err(e) = core::save_project(project_name, &data) {
                        eprintln!("Failed to update project '{}': {}", project_name, e);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to serialize project '{}': {}", project_name, e);
                }
            }
            sync_project_if_configured(project_name, project);
        }
    });
}

pub(crate) fn prune_rule_from_projects(rule_name: &str) {
    with_each_project_mut(|project_name, project| {
        let mut changed = false;
        for rules in project.file_rules.values_mut() {
            let before = rules.len();
            rules.retain(|r| r != rule_name);
            if rules.len() != before {
                changed = true;
            }
        }
        // Remove empty entries
        project.file_rules.retain(|_, rules| !rules.is_empty());

        if changed {
            project.updated_at = chrono::Utc::now().to_rfc3339();
            match serde_json::to_string_pretty(project).map_err(|e| e.to_string()) {
                Ok(data) => {
                    if let Err(e) = core::save_project(project_name, &data) {
                        eprintln!("Failed to update project '{}': {}", project_name, e);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to serialize project '{}': {}", project_name, e);
                }
            }
            // Re-inject rules for affected files, skipping any file whose rules
            // are managed via .claude/rules/ (inline injection must not happen there).
            for (filename, rules) in &project.file_rules {
                if core::project_uses_dot_claude_rules(project, filename) {
                    continue;
                }
                let _ = core::inject_rules_into_project_file(&project.directory, filename, rules);
            }
            sync_project_if_configured(project_name, project);
        }
    });
}
