use crate::recommendations::{
    AddRecommendationParams, ListRecommendationsFilter, Recommendation, RecommendationCounts,
    RecommendationStatus,
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
