use crate::core;
use crate::sync;

use super::projects::sync_projects_referencing_skill;

// ── Skills ───────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_skills() -> Result<Vec<core::SkillEntry>, String> {
    core::list_skills()
}

#[tauri::command]
pub fn list_skill_directories() -> Result<Vec<core::SkillSourceDir>, String> {
    Ok(core::get_all_skill_sources())
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
    if core::is_builtin_skill(name) {
        return Err(format!("Cannot delete built-in skill '{}'", name));
    }
    if let Some(pid) = core::plugin_id_for_skill(name) {
        return Err(format!(
            "Cannot delete skill '{}' — it is provided by plugin '{}'",
            name, pid
        ));
    }
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
    let mut updated = sync::import_local_skill(&project, skill_name)?;
    super::projects::sync_project_if_configured(name, &mut updated);
    serde_json::to_string_pretty(&updated).map_err(|e| e.to_string())
}

/// Copy all local skills to every agent's skill directory in the project.
/// Returns the list of files written.
#[tauri::command]
pub fn sync_local_skills(name: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let written = sync::sync_local_skills_across_agents(&project)?;
    serde_json::to_string_pretty(&written).map_err(|e| e.to_string())
}

/// Reinstall all bundled default skills, overwriting existing on-disk copies.
/// Useful for recovering after accidental edits or upgrading bundled skill content.
#[tauri::command]
pub fn reinstall_default_skills() -> Result<(), String> {
    core::install_default_skills_inner(true)
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

// ── Skill Import ─────────────────────────────────────────────────────────────

/// Import a skill from a local file path or directory.
/// Accepts:
/// - Path to a SKILL.md file
/// - Path to a directory containing skill.json
/// - Path to a directory to scan for SKILL.md files (up to 3 levels deep)
///
/// Returns the list of imported skills as JSON.
#[tauri::command]
pub fn import_skill_from_local_path(path: String) -> Result<String, String> {
    let imported = core::import_skill_from_local_path(&path)?;
    serde_json::to_string_pretty(&imported).map_err(|e| e.to_string())
}

/// Import a skill from a GitHub repository URL.
/// Accepts URLs in formats:
/// - https://github.com/owner/repo
/// - github.com/owner/repo
/// - owner/repo
///
/// Returns the imported skill info as JSON.
#[tauri::command]
pub async fn import_skill_from_repository(
    repo_url: String,
    skill_name: Option<String>,
) -> Result<String, String> {
    let imported = core::import_skill_from_repository(
        &repo_url,
        skill_name.as_deref(),
    )
    .await?;
    serde_json::to_string_pretty(&imported).map_err(|e| e.to_string())
}

/// Import a skill from a Claude .skill package (zip file).
/// Accepts a path to a .skill file and extracts it to the skills directory.
///
/// Returns the list of imported skills as JSON.
#[tauri::command]
pub fn import_skill_from_package(path: String) -> Result<String, String> {
    let imported = core::import_skill_from_package(&path)?;
    serde_json::to_string_pretty(&imported).map_err(|e| e.to_string())
}
