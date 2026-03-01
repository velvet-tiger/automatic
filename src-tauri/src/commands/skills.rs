use crate::core;
use crate::sync;

use super::projects::sync_projects_referencing_skill;

// ── Skills ───────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_skills() -> Result<Vec<core::SkillEntry>, String> {
    core::list_skills()
}

#[tauri::command]
pub fn read_skill(name: &str) -> Result<String, String> {
    core::read_skill(name)
}

#[tauri::command]
pub fn save_skill(name: &str, content: &str) -> Result<(), String> {
    core::save_skill(name, content)?;
    sync_projects_referencing_skill(name);
    Ok(())
}

#[tauri::command]
pub fn delete_skill(name: &str) -> Result<(), String> {
    core::delete_skill(name)?;
    super::projects::prune_skill_from_projects(name);
    Ok(())
}

/// Sync a single skill across both global directories (~/.agents/skills/ and
/// ~/.claude/skills/).
#[tauri::command]
pub fn sync_skill(name: &str) -> Result<(), String> {
    core::sync_skill(name)
}

/// Sync all skills across both global directories.  Returns the list of
/// skill names that were synced.
#[tauri::command]
pub fn sync_all_skills() -> Result<Vec<String>, String> {
    core::sync_all_skills()
}

#[tauri::command]
pub fn get_skill_resources(name: &str) -> Result<core::SkillResources, String> {
    core::list_skill_resources(name)
}

// ── Local Skills ─────────────────────────────────────────────────────────

/// Import a local skill into the global registry and promote it to a normal
/// project skill.  Returns the updated project JSON.
#[tauri::command]
pub fn import_local_skill(name: &str, skill_name: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let updated = sync::import_local_skill(&project, skill_name)?;
    super::projects::sync_project_if_configured(name, &updated);
    serde_json::to_string_pretty(&updated).map_err(|e| e.to_string())
}

/// Copy all local skills to every agent's skill directory in the project.
/// Returns the list of files written.
#[tauri::command]
pub async fn sync_local_skills(name: String) -> Result<String, String> {
    let handle = std::thread::Builder::new()
        .name("sync_local_skills_thread".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            let raw = core::read_project(&name)?;
            let project: core::Project =
                serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
            let written = sync::sync_local_skills_across_agents(&project)?;
            serde_json::to_string_pretty(&written).map_err(|e| e.to_string())
        })
        .map_err(|e| e.to_string())?;

    handle.join().unwrap_or_else(|_| Err("sync_local_skills thread panicked".to_string()))
}

/// Read the SKILL.md content of a local skill from the project directory.
/// Returns the raw file content as a string.
#[tauri::command]
pub fn read_local_skill(name: &str, skill_name: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    sync::read_local_skill(&project, skill_name)
}

/// Save new content for a local skill, writing it to all agent directories
/// where the skill already exists (or creating it if absent).
/// Returns the list of files written as JSON.
#[tauri::command]
pub fn save_local_skill(name: &str, skill_name: &str, content: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let written = sync::save_local_skill(&project, skill_name, content)?;
    serde_json::to_string_pretty(&written).map_err(|e| e.to_string())
}
