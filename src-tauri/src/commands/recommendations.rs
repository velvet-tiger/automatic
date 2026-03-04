use crate::recommendations::{
    AddRecommendationParams, ListRecommendationsFilter, Recommendation, RecommendationCounts,
    RecommendationPriority, RecommendationStatus,
};

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
                title: "Attach rules to your instruction file".to_string(),
                body: "No rules are attached to this project's instruction files. \
                       Rules inject shared guidelines (coding standards, checklists, \
                       Automatic service instructions) into your agent files. \
                       Open the Project File tab and attach the \"automatic-service\" rule \
                       to get started."
                    .to_string(),
                priority: RecommendationPriority::Normal,
                source: "automatic-system".to_string(),
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
            limit: None,
        },
    )
}
