use crate::recommendations::{
    AddRecommendationParams, ListRecommendationsFilter, Recommendation, RecommendationCounts,
    RecommendationPriority, RecommendationStatus,
};
use serde::{Deserialize, Serialize};

// ── Recommendations ───────────────────────────────────────────────────────────

/// Add a new recommendation for a project.
///
/// Returns the `id` of the newly created row.
#[tauri::command]
pub fn add_recommendation(params: AddRecommendationParams) -> Result<i64, String> {
    crate::recommendations::add_recommendation(params)
}

/// Fetch a single recommendation by its numeric id.
#[tauri::command]
pub fn get_recommendation(id: i64) -> Result<Recommendation, String> {
    crate::recommendations::get_recommendation(id)
}

/// List recommendations for a project with optional filters.
///
/// `status` – one of `"pending"`, `"dismissed"`, `"actioned"`, or omit for all.
/// `kind`   – e.g. `"skill"`, `"mcp_server"`, `"agent"`, `"rule"`, or omit for all.
/// `limit`  – max rows to return (default 100).
#[tauri::command]
pub fn list_recommendations(
    project: &str,
    status: Option<String>,
    kind: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<Recommendation>, String> {
    let filter = ListRecommendationsFilter {
        status: status.as_deref().map(RecommendationStatus::from_str),
        kind,
        source: None,
        limit,
    };
    crate::recommendations::list_recommendations(project, filter)
}

/// Dismiss a recommendation (sets status → "dismissed").
#[tauri::command]
pub fn dismiss_recommendation(id: i64) -> Result<(), String> {
    crate::recommendations::dismiss_recommendation(id)
}

/// Mark a recommendation as actioned (sets status → "actioned").
#[tauri::command]
pub fn action_recommendation(id: i64) -> Result<(), String> {
    crate::recommendations::action_recommendation(id)
}

/// Hard-delete a single recommendation by id.
#[tauri::command]
pub fn delete_recommendation(id: i64) -> Result<(), String> {
    crate::recommendations::delete_recommendation(id)
}

/// Delete all recommendations for a project.
///
/// Pass `status` to restrict deletion to that lifecycle state
/// (`"pending"`, `"dismissed"`, or `"actioned"`).  Omit to delete all.
///
/// Returns the number of rows deleted.
#[tauri::command]
pub fn clear_recommendations(project: &str, status: Option<String>) -> Result<usize, String> {
    let s = status.as_deref().map(RecommendationStatus::from_str);
    crate::recommendations::clear_recommendations(project, s)
}

/// Return pending / dismissed / actioned counts for a project.
#[tauri::command]
pub fn count_recommendations(project: &str) -> Result<RecommendationCounts, String> {
    crate::recommendations::count_recommendations(project)
}

/// List pending recommendations for a project filtered by source.
///
/// Used by the frontend to restore persisted AI suggestion results (e.g.
/// `"automatic-ai-skills"`, `"automatic-ai-mcp"`) when switching between
/// projects, without re-running the AI.
#[tauri::command]
pub fn list_recommendations_by_source(
    project: &str,
    source: &str,
) -> Result<Vec<Recommendation>, String> {
    crate::recommendations::list_recommendations(
        project,
        crate::recommendations::ListRecommendationsFilter {
            status: Some(RecommendationStatus::Pending),
            kind: None,
            source: Some(source.to_string()),
            limit: None,
        },
    )
}

/// Return all pending recommendations across every project, ordered by
/// priority (high first) then creation time.  Used by the dashboard.
/// `limit` defaults to 50 when omitted.
#[tauri::command]
pub fn list_all_pending_recommendations(
    limit: Option<usize>,
) -> Result<Vec<Recommendation>, String> {
    crate::recommendations::list_all_pending_recommendations(limit.unwrap_or(50))
}

/// Evaluate a project's configuration and upsert system-generated
/// recommendations for common issues:
///
/// 1. No rules are attached to any project instruction file (`file_rules` is
///    empty across all file keys).
/// 2. The project has agents configured but no instruction file exists on disk.
///
/// For each satisfied condition the corresponding pending recommendation is
/// cleared automatically.  Already-dismissed recommendations are never
/// re-created (users can opt out permanently by dismissing).
///
/// Returns the list of current pending recommendations after evaluation.
#[tauri::command]
pub fn evaluate_project_recommendations(project: &str) -> Result<Vec<Recommendation>, String> {
    use crate::agent;
    use std::path::Path;

    // Load the project config.
    let raw = crate::core::read_project(project)?;
    let proj: crate::core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    // Clear any stale mcp_server recommendations created by an older version of
    // this evaluator (the Automatic MCP server is always injected by the sync
    // engine, so this check was incorrect and has been removed).
    crate::recommendations::clear_system_recommendations_by_kind(project, "mcp_server")?;

    // ── Check 1: Rules attached to instruction files ──────────────────────────
    // A project benefits from having at least one rule attached to its
    // instruction files so agents receive consistent behavioural guidelines.
    // A project has rules if any file_rules value is non-empty.
    // This includes the "_project" key (project-level rules set from the Rules
    // tab), the "_unified" key (legacy), and any per-file keys.
    let has_any_rules =
        !proj.file_rules.is_empty() && proj.file_rules.values().any(|v| !v.is_empty());
    if has_any_rules {
        crate::recommendations::clear_system_recommendations_by_kind(project, "rule")?;
    } else {
        let already_dismissed = crate::recommendations::list_recommendations(
            project,
            crate::recommendations::ListRecommendationsFilter {
                status: Some(RecommendationStatus::Dismissed),
                kind: Some("rule".to_string()),
                source: None,
                limit: None,
            },
        )?
        .into_iter()
        .any(|r| r.source == "automatic-system");

        if !already_dismissed
            && !crate::recommendations::has_pending_system_recommendation(project, "rule")?
        {
            crate::recommendations::add_recommendation(AddRecommendationParams {
                project: project.to_string(),
                kind: "rule".to_string(),
                title: "Attach rules to your project".to_string(),
                body: "No rules are attached to this project. \
                       Rules inject shared guidelines (coding standards, checklists, \
                       and Automatic service instructions) into your agent files. \
                       Open the Rules tab and attach the \"automatic-service\" rule \
                       to get started."
                    .to_string(),
                priority: RecommendationPriority::Normal,
                source: "automatic-system".to_string(),
                metadata: String::new(),
            })?;
        }
    }

    // ── Check 2: Instruction file exists on disk ──────────────────────────────
    // Only evaluated when the project has a directory and at least one agent
    // configured (otherwise there is nothing to check).
    if !proj.directory.is_empty() && !proj.agents.is_empty() {
        let project_dir = Path::new(&proj.directory);
        let any_file_exists = proj.agents.iter().any(|agent_id| {
            if let Some(a) = agent::from_id(agent_id) {
                project_dir.join(a.project_file_name()).exists()
            } else {
                false
            }
        });

        if any_file_exists {
            crate::recommendations::clear_system_recommendations_by_kind(project, "project_file")?;
        } else {
            let already_dismissed = crate::recommendations::list_recommendations(
                project,
            crate::recommendations::ListRecommendationsFilter {
                status: Some(RecommendationStatus::Dismissed),
                kind: Some("project_file".to_string()),
                source: None,
                limit: None,
            },
            )?
            .into_iter()
            .any(|r| r.source == "automatic-system");

            if !already_dismissed
                && !crate::recommendations::has_pending_system_recommendation(
                    project,
                    "project_file",
                )?
            {
                crate::recommendations::add_recommendation(AddRecommendationParams {
                    project: project.to_string(),
                    kind: "project_file".to_string(),
                    title: "Create an instructions file".to_string(),
                    body: "No instruction file was found in your project directory. \
                           Create one via the Project File tab so agents receive \
                           project-specific context and rules on every session."
                        .to_string(),
                    priority: RecommendationPriority::Normal,
                    source: "automatic-system".to_string(),
                    metadata: String::new(),
                })?;
            }
        }
    }

    // Return all current pending recommendations for this project.
    crate::recommendations::list_recommendations(
        project,
        crate::recommendations::ListRecommendationsFilter {
            status: Some(RecommendationStatus::Pending),
            kind: None,
            source: None,
            limit: None,
        },
    )
}

// ── AI-led recommendations ─────────────────────────────────────────────────────

/// Structured output shape returned by the AI for each recommendation.
#[derive(Debug, Deserialize, Serialize)]
struct AiRecommendation {
    kind: String,
    title: String,
    body: String,
    priority: String,
}

/// Structured output wrapper from the AI.
#[derive(Debug, Deserialize, Serialize)]
struct AiRecommendationsOutput {
    recommendations: Vec<AiRecommendation>,
}

/// Result returned to the frontend after an AI recommendations run.
#[derive(Debug, Serialize, Deserialize)]
pub struct AiRecommendationsResult {
    /// Pending recommendations for this project after the run.
    pub recommendations: Vec<Recommendation>,
    /// ISO 8601 UTC timestamp of this run (stored in DB).
    pub last_run_at: String,
}

/// Return the ISO 8601 timestamp of the last AI recommendation run for a
/// project, or `None` if it has never been run.  Non-throwing — UI uses this
/// to decide whether to show "last updated" metadata.
#[tauri::command]
pub fn get_ai_recommendations_timestamp(project: &str) -> Result<Option<String>, String> {
    crate::recommendations::get_ai_recommendations_timestamp(project)
}

/// Use the AI to analyse a project's current configuration (skills, MCP servers,
/// agents, rules, context, and instruction files) and generate tailored
/// recommendations for additional skills, MCP servers, or templates that might
/// be helpful.
///
/// Throttle: by default this will not run more than once per 24 hours per
/// project.  Pass `force = true` to bypass the throttle (used when the user
/// clicks "Update recommendations" manually).
///
/// Returns all current pending recommendations for the project after the run,
/// plus the timestamp of this run.
#[tauri::command]
pub async fn ai_generate_project_recommendations(
    project: &str,
    force: Option<bool>,
) -> Result<AiRecommendationsResult, String> {
    let force = force.unwrap_or(false);

    // Throttle check — skip if run within 24 h and not forced.
    if !force && crate::recommendations::ai_recommendations_throttled(project)? {
        let recs = crate::recommendations::list_recommendations(
            project,
            crate::recommendations::ListRecommendationsFilter {
                status: Some(RecommendationStatus::Pending),
                kind: None,
                source: None,
                limit: None,
            },
        )?;
        let last_run_at = crate::recommendations::get_ai_recommendations_timestamp(project)?
            .unwrap_or_default();
        return Ok(AiRecommendationsResult {
            recommendations: recs,
            last_run_at,
        });
    }

    // Verify an AI key is available before doing any work.
    crate::core::ai::resolve_api_key(None)?;

    // ── Build project state snapshot ─────────────────────────────────────────

    let raw = crate::core::read_project(project)?;
    let proj: crate::core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    // Gather names/summaries of installed skills.
    let installed_skills: Vec<String> = proj.skills.clone();

    // Gather configured MCP servers.
    let installed_mcp: Vec<String> = proj.mcp_servers.clone();

    // Gather configured agents.
    let agents: Vec<String> = proj.agents.clone();

    // Gather rule names from file_rules.
    let rules: Vec<String> = proj
        .file_rules
        .values()
        .flat_map(|v| v.iter().cloned())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    // Read project description and context summary.
    let description = proj.description.clone();
    let context_summary = if !proj.directory.is_empty() {
        match crate::context::get_project_context(&proj.directory) {
            Ok(ctx) => {
                // Pull key commands and concepts to inform the AI.
                let cmds: Vec<String> = ctx.commands.keys().cloned().collect();
                let concepts: Vec<String> = ctx.concepts.keys().cloned().collect();
                let conventions: Vec<String> = ctx.conventions.keys().cloned().collect();
                format!(
                    "Commands: {}\nConcepts: {}\nConventions: {}",
                    cmds.join(", "),
                    concepts.join(", "),
                    conventions.join(", ")
                )
            }
            Err(_) => String::new(),
        }
    } else {
        String::new()
    };

    // Read the first instruction file content (up to 2000 chars) for richer context.
    let instruction_content = if !proj.directory.is_empty() && !proj.agents.is_empty() {
        let dir = std::path::Path::new(&proj.directory);
        proj.agents
            .iter()
            .find_map(|agent_id| {
                crate::agent::from_id(agent_id).and_then(|a| {
                    let p = dir.join(a.project_file_name());
                    std::fs::read_to_string(&p).ok().map(|s| {
                        if s.len() > 2000 {
                            s[..2000].to_string()
                        } else {
                            s
                        }
                    })
                })
            })
            .unwrap_or_default()
    } else {
        String::new()
    };

    // ── Build the user prompt ────────────────────────────────────────────────

    let user_msg = format!(
        r#"Project: "{name}"
Description: {description}

Configured agents: {agents}
Installed skills: {skills}
MCP servers: {mcp}
Attached rules: {rules}
Context summary: {context}
Instruction file excerpt:
{instructions}

Using the tools available to you, explore the skills library, MCP server catalogue, 
and collections to identify what would genuinely benefit this project.

Recommend 3-7 specific skills, MCP servers, or instruction templates that are 
NOT already installed/configured. For each recommendation explain concisely why 
it would help this specific project. Be specific and practical — focus on what 
will have the highest impact given the project's evident tech stack and workflow."#,
        name = project,
        description = if description.is_empty() { "(none provided)".to_string() } else { description },
        agents = if agents.is_empty() { "(none)".to_string() } else { agents.join(", ") },
        skills = if installed_skills.is_empty() { "(none)".to_string() } else { installed_skills.join(", ") },
        mcp = if installed_mcp.is_empty() { "(none)".to_string() } else { installed_mcp.join(", ") },
        rules = if rules.is_empty() { "(none)".to_string() } else { rules.join(", ") },
        context = if context_summary.is_empty() { "(not generated)".to_string() } else { context_summary },
        instructions = if instruction_content.is_empty() { "(no instruction file found)".to_string() } else { instruction_content },
    );

    let system = "You are an expert AI-development advisor embedded in Automatic, \
        a tool that manages skills, MCP servers, and agent configurations. \
        Your job is to analyse a project's current setup and recommend the most \
        valuable additions from the available catalogue. \
        \
        Use the list_skills, list_mcp_servers, search_skills_marketplace, \
        search_mcp_marketplace, search_collections, and search_templates_marketplace \
        tools to research what is available before making recommendations. \
        Only recommend things that are genuinely relevant and not already configured. \
        \
        After your research, respond with ONLY a JSON object matching this schema — \
        no markdown fences, no extra text: \
        { \"recommendations\": [ \
          { \"kind\": \"skill\" | \"mcp_server\" | \"template\" | \"collection\", \
            \"title\": \"short headline (max 60 chars)\", \
            \"body\": \"1-2 sentence explanation of why this is useful for this project\", \
            \"priority\": \"low\" | \"normal\" | \"high\" \
          } \
        ] }";

    // Call the AI with tool access (uses the existing chat_with_tools_inner path
    // with a short working_dir that won't grant file access — we pass a temp dir).
    // We need a valid directory for the tool sandbox but we do not grant any file
    // reads (the AI has no reason to read files for this task).
    let working_dir = if !proj.directory.is_empty() {
        proj.directory.clone()
    } else {
        // Fall back to the Automatic data dir which always exists.
        crate::core::get_automatic_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
            .to_string_lossy()
            .to_string()
    };

    let ai_response = crate::core::ai::chat_with_tools(
        vec![crate::core::ai::AiMessage {
            role: "user".into(),
            content: user_msg,
        }],
        None,
        None,
        Some(system.to_string()),
        Some(8192),
        working_dir,
    )
    .await?;

    // ── Parse the structured JSON response ───────────────────────────────────

    // Strip markdown fences if the model wrapped the output despite instructions.
    let clean = ai_response
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let parsed: AiRecommendationsOutput = serde_json::from_str(clean)
        .map_err(|e| format!("Failed to parse AI recommendations JSON: {} — raw: {}", e, clean))?;

    // ── Persist the AI recommendations ──────────────────────────────────────
    // Clear previous AI-generated pending recommendations before inserting fresh ones
    // so we don't accumulate stale suggestions across runs.
    {
        use rusqlite::{params, Connection};
        let db_path = crate::core::get_automatic_dir()
            .map_err(|e| format!("Cannot locate data dir: {}", e))?
            .join("activity.db");
        let conn = Connection::open(&db_path)
            .map_err(|e| format!("Failed to open DB: {}", e))?;
        conn.execute(
            "DELETE FROM recommendations WHERE project = ?1 AND source = 'automatic-ai' AND status = 'pending'",
            params![project],
        )
        .map_err(|e| format!("Failed to clear old AI recommendations: {}", e))?;
    }

    for rec in &parsed.recommendations {
        let priority = RecommendationPriority::from_str(&rec.priority);
        crate::recommendations::add_recommendation(AddRecommendationParams {
            project: project.to_string(),
            kind: rec.kind.clone(),
            title: rec.title.clone(),
            body: rec.body.clone(),
            priority,
            source: "automatic-ai".to_string(),
            metadata: String::new(),
        })?;
    }

    // Record the timestamp of this run.
    crate::recommendations::set_ai_recommendations_timestamp(project)?;
    let last_run_at = crate::recommendations::get_ai_recommendations_timestamp(project)?
        .unwrap_or_default();

    // Also run the rule-based evaluator so structural checks are always current.
    evaluate_project_recommendations(project)?;

    // Return all pending recommendations.
    let recs = crate::recommendations::list_recommendations(
        project,
        crate::recommendations::ListRecommendationsFilter {
            status: Some(RecommendationStatus::Pending),
            kind: None,
            source: None,
            limit: None,
        },
    )?;

    Ok(AiRecommendationsResult {
        recommendations: recs,
        last_run_at,
    })
}

// ── Targeted AI suggestion helpers ────────────────────────────────────────────

/// Shared project-state snapshot builder used by the targeted suggestion
/// commands.  Returns a human-readable text block suitable for inclusion in
/// an LLM prompt.
fn build_project_state(proj: &crate::core::Project) -> String {
    let agents = if proj.agents.is_empty() {
        "(none)".to_string()
    } else {
        proj.agents.join(", ")
    };
    let skills = if proj.skills.is_empty() {
        "(none)".to_string()
    } else {
        proj.skills.join(", ")
    };
    let mcp = if proj.mcp_servers.is_empty() {
        "(none)".to_string()
    } else {
        proj.mcp_servers.join(", ")
    };
    let rules: Vec<String> = proj
        .file_rules
        .values()
        .flat_map(|v| v.iter().cloned())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let rules_str = if rules.is_empty() {
        "(none)".to_string()
    } else {
        rules.join(", ")
    };

    let context_summary = if !proj.directory.is_empty() {
        match crate::context::get_project_context(&proj.directory) {
            Ok(ctx) => {
                let cmds: Vec<String> = ctx.commands.keys().cloned().collect();
                let concepts: Vec<String> = ctx.concepts.keys().cloned().collect();
                let langs: Vec<String> = ctx.conventions.keys().cloned().collect();
                format!(
                    "Commands: {}\nConcepts: {}\nConventions/languages: {}",
                    cmds.join(", "),
                    concepts.join(", "),
                    langs.join(", ")
                )
            }
            Err(_) => String::new(),
        }
    } else {
        String::new()
    };

    let instruction_excerpt = if !proj.directory.is_empty() && !proj.agents.is_empty() {
        let dir = std::path::Path::new(&proj.directory);
        proj.agents
            .iter()
            .find_map(|agent_id| {
                crate::agent::from_id(agent_id).and_then(|a| {
                    let p = dir.join(a.project_file_name());
                    std::fs::read_to_string(&p).ok().map(|s| {
                        if s.len() > 1500 { s[..1500].to_string() } else { s }
                    })
                })
            })
            .unwrap_or_default()
    } else {
        String::new()
    };

    format!(
        "Project: \"{name}\"\nDescription: {desc}\nAgents: {agents}\nSkills: {skills}\nMCP servers: {mcp}\nRules: {rules}\nContext: {ctx}\nInstruction file excerpt:\n{instr}",
        name = proj.name,
        desc = if proj.description.is_empty() { "(none)".to_string() } else { proj.description.clone() },
        agents = agents,
        skills = skills,
        mcp = mcp,
        rules = rules_str,
        ctx = if context_summary.is_empty() { "(not generated)".to_string() } else { context_summary },
        instr = if instruction_excerpt.is_empty() { "(no instruction file found)".to_string() } else { instruction_excerpt },
    )
}

/// Phase 1: Run the AI with tools to gather facts (what is available in the
/// marketplace).  Returns the raw text summary produced by the agentic loop.
async fn run_research_phase(
    prompt: &str,
    system: &str,
    working_dir: &str,
) -> Result<String, String> {
    crate::core::ai::chat_with_tools(
        vec![crate::core::ai::AiMessage {
            role: "user".into(),
            content: prompt.to_string(),
        }],
        None,
        None,
        Some(system.to_string()),
        Some(8192),
        working_dir.to_string(),
    )
    .await
}

/// Phase 2: Given a research summary, use structured outputs (JSON schema) to
/// produce a guaranteed-valid JSON suggestions list.
///
/// This avoids the "model wraps output in markdown fences" failure mode that
/// occurs when `chat_with_tools` is asked to produce structured output.
async fn run_structured_phase(
    research_summary: &str,
    final_prompt: &str,
    system: &str,
    schema: serde_json::Value,
) -> Result<String, String> {
    // Combine the research output and the final formatting instruction into a
    // single user turn so the model sees both.
    let content = format!(
        "Research findings:\n{research}\n\n{instruction}",
        research = research_summary,
        instruction = final_prompt,
    );
    crate::core::ai::chat_structured(
        vec![crate::core::ai::AiMessage {
            role: "user".into(),
            content,
        }],
        None,
        None,
        Some(system.to_string()),
        Some(4096),
        schema,
    )
    .await
}

/// Parse the JSON array `[{ "title", "body", "priority", "metadata"? }]` the AI
/// returns for targeted suggestions and persist them, replacing any previous
/// pending suggestions of the same source for this project.
///
/// `metadata` is an optional JSON object (e.g. `{"id":"owner/repo/skill","source":"owner/repo",
/// "name":"skill","installs":123}`) stored verbatim as a string on the recommendation
/// so the frontend can deep-link to the exact marketplace item.
fn persist_targeted_suggestions(
    project: &str,
    kind: &str,
    source: &str,
    raw_response: &str,
) -> Result<Vec<crate::recommendations::Recommendation>, String> {
    use serde::Deserialize;
    use serde_json::Value;

    #[derive(Deserialize)]
    struct Item {
        title: String,
        body: String,
        #[serde(default)]
        priority: String,
        /// Optional structured data (skill id, source, installs, …).
        /// Serialised back to a string for storage.
        #[serde(default)]
        metadata: Option<Value>,
    }

    #[derive(Deserialize)]
    struct Wrapper {
        suggestions: Vec<Item>,
    }

    let clean = raw_response
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let parsed: Wrapper = serde_json::from_str(clean)
        .map_err(|e| format!("Failed to parse AI suggestions: {} — raw: {}", e, clean))?;

    // Clear previous pending suggestions of this source before inserting fresh ones.
    {
        use rusqlite::{params, Connection};
        let db_path = crate::core::get_automatic_dir()
            .map_err(|e| format!("Cannot locate data dir: {}", e))?
            .join("activity.db");
        let conn = Connection::open(&db_path)
            .map_err(|e| format!("Failed to open DB: {}", e))?;
        conn.execute(
            "DELETE FROM recommendations WHERE project = ?1 AND source = ?2 AND status = 'pending'",
            params![project, source],
        )
        .map_err(|e| format!("Failed to clear old suggestions: {}", e))?;
    }

    for item in &parsed.suggestions {
        let priority = RecommendationPriority::from_str(&item.priority);
        // Serialise the optional metadata Value back to a compact JSON string,
        // or use an empty string when absent.
        let metadata_str = match &item.metadata {
            Some(v) => serde_json::to_string(v).unwrap_or_default(),
            None => String::new(),
        };
        crate::recommendations::add_recommendation(AddRecommendationParams {
            project: project.to_string(),
            kind: kind.to_string(),
            title: item.title.clone(),
            body: item.body.clone(),
            priority,
            source: source.to_string(),
            metadata: metadata_str,
        })?;
    }

    // Return all current pending recommendations for this project so the
    // frontend can refresh its full list in one round-trip.
    crate::recommendations::list_recommendations(
        project,
        crate::recommendations::ListRecommendationsFilter {
            status: Some(RecommendationStatus::Pending),
            kind: None,
            source: None,
            limit: None,
        },
    )
}

// ── Public targeted commands ───────────────────────────────────────────────────

/// Ask the AI to suggest specific **skills** for this project.
///
/// Two-phase approach:
/// 1. Tool-use phase — explores the skill library and skills.sh marketplace.
/// 2. Structured output phase — produces a guaranteed-valid JSON list via JSON schema.
///
/// Results are stored as `"automatic-ai-skills"` recommendations.
/// Returns all current pending recommendations for the project after the run.
#[tauri::command]
pub async fn ai_suggest_skills(project: &str) -> Result<Vec<Recommendation>, String> {
    crate::core::ai::resolve_api_key(None)?;

    let raw = crate::core::read_project(project)?;
    let proj: crate::core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    let state = build_project_state(&proj);

    let working_dir = if !proj.directory.is_empty() {
        proj.directory.clone()
    } else {
        crate::core::get_automatic_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
            .to_string_lossy()
            .to_string()
    };

    // ── Phase 1: Research ────────────────────────────────────────────────────
    // Ask the model to use tools to discover what skills are available.
    let research_prompt = format!(
        r#"{state}

Use the list_skills and search_skills_marketplace tools to research what skills are
available — both locally installed and in the community registry (skills.sh).

For each relevant skill you find via search_skills_marketplace, record its exact
registry fields: id (e.g. "owner/repo/skill-name"), name, source (e.g. "owner/repo"),
and installs count.

Identify 3–5 skills that are NOT already in the "Skills" list above and would
genuinely benefit this project given its tech stack, workflow, and agent configuration.

Summarise your findings as plain text — list the candidate skills with their id,
source, installs count, and a brief reason why each fits this project."#,
        state = state,
    );

    let research_system = "You are an expert AI-development advisor. \
        Use the list_skills and search_skills_marketplace tools to explore what skills \
        are available for the given project. Record each candidate skill's exact registry \
        fields (id, name, source, installs). Summarise your findings as plain text.";

    let research = run_research_phase(&research_prompt, research_system, &working_dir).await?;

    // ── Phase 2: Structured output ───────────────────────────────────────────
    // Feed the research summary back and demand JSON via schema enforcement.
    let schema = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["suggestions"],
        "properties": {
            "suggestions": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["title", "body", "priority"],
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "Exact skill name (e.g. 'laravel-specialist')"
                        },
                        "body": {
                            "type": "string",
                            "description": "1–2 sentence explanation of why this skill benefits the project"
                        },
                        "priority": {
                            "type": "string",
                            "enum": ["low", "normal", "high"]
                        },
                        "metadata": {
                            "type": "object",
                            "additionalProperties": false,
                            "required": ["id", "name", "source", "installs"],
                            "properties": {
                                "id":       { "type": "string", "description": "Full skill ID, e.g. 'owner/repo/skill-name'" },
                                "name":     { "type": "string", "description": "Bare skill name" },
                                "source":   { "type": "string", "description": "Repository slug, e.g. 'owner/repo'" },
                                "installs": { "type": "integer", "description": "Weekly install count from the registry" }
                            }
                        }
                    }
                }
            }
        }
    });

    let format_instruction = "Based on the research findings above, produce the final \
        skill suggestions for this project. Include the metadata object for each skill \
        using the exact id, name, source, and installs values found during research. \
        Only include skills that are not already installed on the project.";

    let format_system = "You are an expert AI-development advisor. \
        Convert the provided research summary into a structured list of skill \
        suggestions. Use the exact registry fields (id, name, source, installs) \
        captured during research. Output must conform exactly to the provided JSON schema.";

    let structured_response =
        run_structured_phase(&research, format_instruction, format_system, schema).await?;

    persist_targeted_suggestions(project, "skill", "automatic-ai-skills", &structured_response)
}

/// Ask the AI to suggest specific **MCP servers** for this project.
///
/// Two-phase approach:
/// 1. Tool-use phase — explores the local MCP registry and featured catalogue.
/// 2. Structured output phase — produces a guaranteed-valid JSON list via JSON schema.
///
/// Results are stored as `"automatic-ai-mcp"` recommendations.
/// Returns all current pending recommendations for the project after the run.
#[tauri::command]
pub async fn ai_suggest_mcp_servers(project: &str) -> Result<Vec<Recommendation>, String> {
    crate::core::ai::resolve_api_key(None)?;

    let raw = crate::core::read_project(project)?;
    let proj: crate::core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    let state = build_project_state(&proj);

    let working_dir = if !proj.directory.is_empty() {
        proj.directory.clone()
    } else {
        crate::core::get_automatic_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
            .to_string_lossy()
            .to_string()
    };

    // ── Phase 1: Research ────────────────────────────────────────────────────
    let research_prompt = format!(
        r#"{state}

Use the list_mcp_servers and search_mcp_marketplace tools to research what MCP
servers are available — both locally configured and in the featured catalogue.

Identify 3–5 servers that are NOT already in the "MCP servers" list above and
would genuinely benefit this project given its tech stack, workflow, and data sources.

Summarise your findings as plain text — list each candidate server name and a brief
reason why it fits this project."#,
        state = state,
    );

    let research_system = "You are an expert AI-development advisor. \
        Use the list_mcp_servers and search_mcp_marketplace tools to explore what MCP \
        servers are available for the given project. Summarise your findings as plain text.";

    let research = run_research_phase(&research_prompt, research_system, &working_dir).await?;

    // ── Phase 2: Structured output ───────────────────────────────────────────
    let schema = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["suggestions"],
        "properties": {
            "suggestions": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["title", "body", "priority"],
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "Exact MCP server name (slug)"
                        },
                        "body": {
                            "type": "string",
                            "description": "1–2 sentence explanation of why this server benefits the project"
                        },
                        "priority": {
                            "type": "string",
                            "enum": ["low", "normal", "high"]
                        }
                    }
                }
            }
        }
    });

    let format_instruction = "Based on the research findings above, produce the final \
        MCP server suggestions for this project. Only include servers that are not \
        already configured on the project.";

    let format_system = "You are an expert AI-development advisor. \
        Convert the provided research summary into a structured list of MCP server \
        suggestions. Output must conform exactly to the provided JSON schema.";

    let structured_response =
        run_structured_phase(&research, format_instruction, format_system, schema).await?;

    persist_targeted_suggestions(project, "mcp_server", "automatic-ai-mcp", &structured_response)
}
