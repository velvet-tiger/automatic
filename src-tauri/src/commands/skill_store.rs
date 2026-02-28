use crate::core;

use super::projects::sync_projects_referencing_skill;

// ── Skills Store ─────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn search_remote_skills(query: String) -> Result<Vec<core::RemoteSkillResult>, String> {
    core::search_remote_skills(&query).await
}

#[tauri::command]
pub async fn fetch_remote_skill_content(source: String, name: String) -> Result<String, String> {
    core::fetch_remote_skill_content(&source, &name).await
}

/// Import a skill from skills.sh: save content + record its remote origin.
#[tauri::command]
pub async fn import_remote_skill(
    name: String,
    content: String,
    source: String,
    id: String,
) -> Result<(), String> {
    core::save_skill(&name, &content)?;
    core::record_skill_source(&name, &source, &id)?;
    sync_projects_referencing_skill(&name);
    Ok(())
}

/// Return all entries from ~/.automatic/skills.json as a JSON object.
#[tauri::command]
pub fn get_skill_sources() -> Result<String, String> {
    let registry = core::read_skill_sources()?;
    serde_json::to_string(&registry).map_err(|e| e.to_string())
}
