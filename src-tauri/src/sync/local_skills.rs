use std::fs;
use std::path::PathBuf;

use crate::agent;
use crate::core::Project;

use super::helpers::add_unique;

/// Read a local skill's content from whichever agent directory contains it.
pub fn read_local_skill(project: &Project, skill_name: &str) -> Result<String, String> {
    let dir = PathBuf::from(&project.directory);

    for agent_id in &project.agents {
        if let Some(a) = agent::from_id(agent_id) {
            for skill_dir in a.skill_dirs(&dir) {
                let skill_file = skill_dir.join(skill_name).join("SKILL.md");
                if skill_file.exists() {
                    return fs::read_to_string(&skill_file).map_err(|e| e.to_string());
                }
            }
        }
    }

    Err(format!(
        "Local skill '{}' not found in any agent directory",
        skill_name
    ))
}

/// Write new content to a local skill's SKILL.md in every agent directory
/// where it already exists (or in the first available agent's skill dir if
/// none exists yet).  Returns the list of files written.
pub fn save_local_skill(
    project: &Project,
    skill_name: &str,
    content: &str,
) -> Result<Vec<String>, String> {
    if project.directory.is_empty() {
        return Err("Project has no directory configured".into());
    }
    let dir = PathBuf::from(&project.directory);
    if !dir.exists() {
        return Err(format!("Directory '{}' does not exist", project.directory));
    }

    let mut written: Vec<String> = Vec::new();

    // Write into every agent directory that already has a copy of this skill,
    // so all copies stay in sync.
    let mut found_any = false;
    for agent_id in &project.agents {
        if let Some(a) = agent::from_id(agent_id) {
            for skill_dir in a.skill_dirs(&dir) {
                let target_dir = skill_dir.join(skill_name);
                let target_file = target_dir.join("SKILL.md");
                if target_file.exists() {
                    found_any = true;
                    fs::write(&target_file, content)
                        .map_err(|e| format!("Failed to write skill: {}", e))?;
                    written.push(target_file.display().to_string());
                }
            }
        }
    }

    // If no existing copy was found, create one in the first available agent
    // skill directory so the skill materialises on disk.
    if !found_any {
        'outer: for agent_id in &project.agents {
            if let Some(a) = agent::from_id(agent_id) {
                for skill_dir in a.skill_dirs(&dir) {
                    let target_dir = skill_dir.join(skill_name);
                    fs::create_dir_all(&target_dir)
                        .map_err(|e| format!("Failed to create dir: {}", e))?;
                    let target_file = target_dir.join("SKILL.md");
                    fs::write(&target_file, content)
                        .map_err(|e| format!("Failed to write skill: {}", e))?;
                    written.push(target_file.display().to_string());
                    break 'outer;
                }
            }
        }
    }

    if written.is_empty() {
        return Err("No agent skill directories found for this project".into());
    }

    Ok(written)
}

/// Copy a local skill into the global registry and promote it to a normal
/// (global) project skill.  Returns the updated project.
pub fn import_local_skill(project: &Project, skill_name: &str) -> Result<Project, String> {
    let content = read_local_skill(project, skill_name)?;
    crate::core::save_skill(skill_name, &content)?;

    let mut updated = project.clone();
    updated.local_skills.retain(|s| s != skill_name);
    add_unique(&mut updated.skills, skill_name);

    let proj_str =
        serde_json::to_string_pretty(&updated).map_err(|e| format!("JSON error: {}", e))?;
    crate::core::save_project(&updated.name, &proj_str)?;

    Ok(updated)
}

/// Copy every local skill to all agent skill directories so that each agent
/// in the project has a copy.  Returns the list of files written.
pub fn sync_local_skills_across_agents(project: &Project) -> Result<Vec<String>, String> {
    if project.directory.is_empty() {
        return Err("Project has no directory configured".into());
    }

    let dir = PathBuf::from(&project.directory);
    if !dir.exists() {
        return Err(format!("Directory '{}' does not exist", project.directory));
    }

    // Collect content for each local skill (first copy found wins)
    let mut local_contents: Vec<(String, String)> = Vec::new();
    for name in &project.local_skills {
        if let Ok(content) = read_local_skill(project, name) {
            local_contents.push((name.clone(), content));
        }
    }

    if local_contents.is_empty() {
        return Ok(Vec::new());
    }

    // Write each local skill to every agent's skill directory
    let mut written = Vec::new();
    for agent_id in &project.agents {
        if let Some(a) = agent::from_id(agent_id) {
            for skill_dir in a.skill_dirs(&dir) {
                for (name, content) in &local_contents {
                    let target_dir = skill_dir.join(name);
                    fs::create_dir_all(&target_dir)
                        .map_err(|e| format!("Failed to create dir: {}", e))?;
                    let target_file = target_dir.join("SKILL.md");
                    fs::write(&target_file, content)
                        .map_err(|e| format!("Failed to write skill: {}", e))?;
                    written.push(target_file.display().to_string());
                }
            }
        }
    }

    Ok(written)
}
