use crate::core;

// ── Instruction Templates ────────────────────────────────────────────────────

#[tauri::command]
pub fn get_templates() -> Result<Vec<String>, String> {
    core::list_templates()
}

#[tauri::command]
pub fn read_template(name: &str) -> Result<String, String> {
    core::read_template(name)
}

#[tauri::command]
pub fn save_template(name: &str, content: &str) -> Result<(), String> {
    core::save_template(name, content)
}

#[tauri::command]
pub fn delete_template(name: &str) -> Result<(), String> {
    core::delete_template(name)
}

// ── Project Templates ─────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_project_templates() -> Result<Vec<String>, String> {
    core::list_project_templates()
}

#[tauri::command]
pub fn read_project_template(name: &str) -> Result<String, String> {
    core::read_project_template(name)
}

#[tauri::command]
pub fn save_project_template(name: &str, data: &str) -> Result<(), String> {
    core::save_project_template(name, data)
}

#[tauri::command]
pub fn delete_project_template(name: &str) -> Result<(), String> {
    core::delete_project_template(name)
}

#[tauri::command]
pub fn rename_project_template(old_name: &str, new_name: &str) -> Result<(), String> {
    core::rename_project_template(old_name, new_name)
}

// ── Template Marketplace (bundled) ────────────────────────────────────────────

#[tauri::command]
pub fn list_bundled_project_templates() -> Result<String, String> {
    core::list_bundled_project_templates()
}

#[tauri::command]
pub fn read_bundled_project_template(name: &str) -> Result<String, String> {
    core::read_bundled_project_template(name)
}

#[tauri::command]
pub fn import_bundled_project_template(name: &str) -> Result<(), String> {
    core::import_bundled_project_template(name)
}

#[tauri::command]
pub fn search_bundled_project_templates(query: &str) -> Result<String, String> {
    core::search_bundled_project_templates(query)
}

/// Check which skills / MCP servers a bundled template requires are missing
/// locally.  Bundled skills are flagged as installable without a network call.
#[tauri::command]
pub fn check_template_dependencies(name: String) -> Result<String, String> {
    core::check_template_dependencies(&name)
}
