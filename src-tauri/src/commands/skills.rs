use crate::core;

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
pub fn get_skill_scan_state(name: &str) -> Result<Option<core::AssetSecurityScanRecord>, String> {
    core::get_skill_scan_state(name)
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

/// Import a skill from an external scan directory (e.g. `~/.agents/skills/`
/// from an independent `npx skills add` install) into Automatic's managed
/// library. No-op if already in the library.
#[tauri::command]
pub fn sync_skill(name: &str) -> Result<(), String> {
    core::sync_skill(name)
}

/// Import every skill visible in external scan directories that isn't yet
/// in the managed library. Returns the names imported.
#[tauri::command]
pub fn sync_all_skills() -> Result<Vec<String>, String> {
    core::sync_all_skills()
}

#[tauri::command]
pub fn get_skill_resources(name: &str) -> Result<core::SkillResources, String> {
    core::list_skill_resources(name)
}

/// Reinstall all bundled default skills, overwriting existing on-disk copies.
/// Useful for recovering after accidental edits or upgrading bundled skill content.
#[tauri::command]
pub fn reinstall_default_skills() -> Result<(), String> {
    core::install_default_skills_inner(true)
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
    for skill in &imported {
        core::record_recently_added("skills", &skill.name);
    }
    serde_json::to_string_pretty(&imported).map_err(|e| e.to_string())
}

/// Import skills from a GitHub repository URL.
/// Accepts URLs in formats:
/// - https://github.com/owner/repo
/// - github.com/owner/repo
/// - owner/repo
///
/// Returns the list of imported skills as JSON.
#[tauri::command]
pub async fn import_skill_from_repository(
    repo_url: String,
    skill_name: Option<String>,
) -> Result<String, String> {
    let imported = core::import_skill_from_repository(&repo_url, skill_name.as_deref()).await?;
    for skill in &imported {
        core::record_recently_added("skills", &skill.name);
    }
    serde_json::to_string_pretty(&imported).map_err(|e| e.to_string())
}

/// Import a skill from a Claude .skill package (zip file).
/// Accepts a path to a .skill file and extracts it to the skills directory.
///
/// Returns the list of imported skills as JSON.
#[tauri::command]
pub fn import_skill_from_package(path: String) -> Result<String, String> {
    let imported = core::import_skill_from_package(&path)?;
    for skill in &imported {
        core::record_recently_added("skills", &skill.name);
    }
    serde_json::to_string_pretty(&imported).map_err(|e| e.to_string())
}

// ── Skill Collections ─────────────────────────────────────────────────────

/// Return all skill collections with their member skill names.
#[tauri::command]
pub fn get_skill_collections() -> Result<Vec<core::SkillCollection>, String> {
    core::list_skill_collections()
}

/// Assign a skill to a collection.
#[tauri::command]
pub fn set_skill_collection(skill_name: String, collection: String) -> Result<(), String> {
    core::set_skill_collection(&skill_name, &collection)
}

/// Remove a skill from its collection.
#[tauri::command]
pub fn remove_skill_collection(skill_name: String) -> Result<(), String> {
    core::remove_skill_collection(&skill_name)
}
