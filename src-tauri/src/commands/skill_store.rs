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

/// Import a skill from skills.sh: save content + record its remote origin
/// along with install metadata (content SHA, publisher version, timestamp) so
/// the UI can later check whether the upstream has moved on.
#[tauri::command]
pub async fn import_remote_skill(
    name: String,
    content: String,
    source: String,
    id: String,
) -> Result<(), String> {
    core::save_skill(&name, &content)?;
    let installed_sha = Some(core::sha256_hex(&content));
    // skill.json is best-effort: many published skills do not include one,
    // and an outage here must not block the install.
    let installed_version = core::fetch_remote_skill_version(&source, &name).await;
    let installed_at = Some(core::now_iso8601());
    core::record_skill_source_with_meta(
        &name,
        &source,
        &id,
        "github",
        installed_sha,
        installed_version,
        installed_at,
    )?;
    sync_projects_referencing_skill(&name);
    // Mark getting-started flag; best-effort — never block the install.
    if let Err(e) = core::mark_skill_installed() {
        eprintln!("[automatic] Failed to mark skill_installed flag: {}", e);
    }
    Ok(())
}

/// Return all entries from ~/.automatic/skills.json as a JSON object.
#[tauri::command]
pub fn get_skill_sources() -> Result<String, String> {
    let registry = core::read_skill_sources()?;
    serde_json::to_string(&registry).map_err(|e| e.to_string())
}

/// Check whether a remote (GitHub-sourced) skill has a newer upstream version
/// than what is installed locally. Network-bound: the call may take a few
/// seconds and the frontend should surface a "checking…" state.
#[tauri::command]
pub async fn check_skill_update(name: String) -> Result<core::SkillUpdateStatus, String> {
    core::check_skill_update(&name).await
}
