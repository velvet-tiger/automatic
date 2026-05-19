use crate::core::ai_generate::{generate_library_asset, AssetKind};

/// Generate a new library asset (skill, command, rule, or sub-agent) from a
/// free-text description using the currently active agent.
///
/// `kind` must be one of `"skill"`, `"command"`, `"rule"`, `"subagent"`.
/// When revising a prior draft, supply both `previous_attempt` and `feedback`.
#[tauri::command]
pub async fn ai_generate_library_asset(
    kind: String,
    description: String,
    previous_attempt: Option<String>,
    feedback: Option<String>,
) -> Result<String, String> {
    let kind = AssetKind::parse(&kind)?;
    generate_library_asset(
        kind,
        &description,
        previous_attempt.as_deref(),
        feedback.as_deref(),
    )
    .await
}
