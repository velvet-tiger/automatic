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
    if let Err(e) = core::mark_template_imported() {
        eprintln!("[automatic] Failed to mark template_imported flag: {}", e);
    }
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
    let mut result = core::apply_templates_to_project(project_name, &template_names)?;
    super::projects::sync_project_if_configured(project_name, &mut result.project);
    serde_json::to_string(&result).map_err(|e| e.to_string())
}
