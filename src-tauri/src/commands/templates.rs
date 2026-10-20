use crate::core;

// ── Templates ────────────────────────────────────────────────────────────────
//
// Project starter templates that bundle skills, MCP servers, sub-agents,
// instructions, and project files. Stored as JSON at
// `~/.automatic/library/templates/`.

#[tauri::command]
pub fn get_templates() -> Result<Vec<String>, String> {
    core::list_templates()
}

#[tauri::command]
pub fn read_template(name: &str) -> Result<String, String> {
    core::read_template(name)
}

#[tauri::command]
pub fn save_template(name: &str, data: &str) -> Result<(), String> {
    // The user now owns this version, so no source may replace or delete it.
    core::remote_sources::release_ownership_on_user_save("template", name)?;
    core::save_template(name, data)
}

#[tauri::command]
pub fn delete_template(name: &str) -> Result<(), String> {
    core::delete_template(name)
}

#[tauri::command]
pub fn rename_template(old_name: &str, new_name: &str) -> Result<(), String> {
    core::rename_template(old_name, new_name)
}

// ── Discover Templates (bundled) ─────────────────────────────────────────────

#[tauri::command]
pub fn list_bundled_templates() -> Result<String, String> {
    core::list_bundled_templates()
}

#[tauri::command]
pub fn read_bundled_template(name: &str) -> Result<String, String> {
    core::read_bundled_template(name)
}

#[tauri::command]
pub async fn import_bundled_template(name: String) -> Result<(), String> {
    core::import_bundled_template(&name).await?;
    Ok(())
}

#[tauri::command]
pub fn search_bundled_templates(query: &str) -> Result<String, String> {
    core::search_bundled_templates(query)
}

/// Check which skills / MCP servers a bundled template requires are missing
/// locally. Bundled skills are flagged as installable without a network call.
#[tauri::command]
pub fn check_template_dependencies(name: String) -> Result<String, String> {
    core::check_template_dependencies(&name)
}

/// Merge one or more project templates into an existing project.
/// Syncs the project to disk immediately so there is no drift.
/// Returns the updated project and any pending unified instruction entries.
#[tauri::command]
pub fn apply_templates_to_project(
    project_name: &str,
    template_names: Vec<String>,
) -> Result<String, String> {
    super::projects::ensure_writable(project_name)?;
    let project_name = &crate::core::canonical_project_ident(project_name)?;
    let mut result = core::apply_templates_to_project(project_name, &template_names)?;
    super::projects::sync_project_if_configured(project_name, &mut result.project);
    serde_json::to_string(&result).map_err(|e| e.to_string())
}
