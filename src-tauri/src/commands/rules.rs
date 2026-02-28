use crate::core;

use super::projects::{prune_rule_from_projects, sync_projects_referencing_rule};

// ── Rules ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_rules() -> Result<Vec<core::RuleEntry>, String> {
    core::list_rules()
}

#[tauri::command]
pub fn read_rule(machine_name: &str) -> Result<String, String> {
    core::read_rule(machine_name)
}

#[tauri::command]
pub fn save_rule(machine_name: &str, name: &str, content: &str) -> Result<(), String> {
    core::save_rule(machine_name, name, content)?;
    // Re-sync all projects that reference this rule in their file_rules
    sync_projects_referencing_rule(machine_name);
    Ok(())
}

#[tauri::command]
pub fn delete_rule(machine_name: &str) -> Result<(), String> {
    core::delete_rule(machine_name)?;
    prune_rule_from_projects(machine_name);
    Ok(())
}
