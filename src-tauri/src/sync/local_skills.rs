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

    // Fallback: if no agents are configured (or none resolved), write to the
    // canonical hub directory so the skill is not lost.
    if written.is_empty() {
        let hub_dir = dir.join(".agents").join("skills").join(skill_name);
        fs::create_dir_all(&hub_dir)
            .map_err(|e| format!("Failed to create hub skill dir: {}", e))?;
        let hub_file = hub_dir.join("SKILL.md");
        fs::write(&hub_file, content)
            .map_err(|e| format!("Failed to write skill to hub: {}", e))?;
        written.push(hub_file.display().to_string());
    }

    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Project;
    use std::fs;
    use tempfile::TempDir;

    fn tmp() -> TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    /// Write a SKILL.md at `<project_dir>/<skill_subdir>/<skill_name>/SKILL.md`.
    fn make_local_skill(
        project_dir: &std::path::Path,
        skill_subdir: &str,
        skill_name: &str,
        content: &str,
    ) {
        let skill_dir = project_dir.join(skill_subdir).join(skill_name);
        fs::create_dir_all(&skill_dir).expect("create skill dir");
        fs::write(skill_dir.join("SKILL.md"), content).expect("write SKILL.md");
    }

    fn project_with_dir(
        dir: &std::path::Path,
        agents: Vec<String>,
        local_skills: Vec<String>,
    ) -> Project {
        Project {
            name: "test-project".into(),
            directory: dir.to_str().unwrap().into(),
            agents,
            local_skills,
            ..Default::default()
        }
    }

    #[test]
    fn read_local_skill_finds_skill_in_claude_agent_dir() {
        let tmp = tmp();
        let project_dir = tmp.path().join("project");
        fs::create_dir_all(&project_dir).expect("create project dir");

        // ClaudeCode stores skills at <project>/.claude/skills/<name>/SKILL.md
        make_local_skill(&project_dir, ".claude/skills", "my-skill", "# My Local Skill");

        let project =
            project_with_dir(&project_dir, vec!["claude".into()], vec!["my-skill".into()]);
        let content = read_local_skill(&project, "my-skill").expect("read");
        assert_eq!(content, "# My Local Skill");
    }

    #[test]
    fn read_local_skill_returns_error_when_skill_file_missing() {
        let tmp = tmp();
        let project_dir = tmp.path().join("project");
        fs::create_dir_all(&project_dir).expect("create project dir");

        let project =
            project_with_dir(&project_dir, vec!["claude".into()], vec!["missing".into()]);
        let result = read_local_skill(&project, "missing");
        assert!(result.is_err(), "expected error for missing skill");
    }

    #[test]
    fn read_local_skill_returns_error_when_no_agents_configured() {
        let tmp = tmp();
        let project_dir = tmp.path().join("project");
        fs::create_dir_all(&project_dir).expect("create project dir");
        make_local_skill(&project_dir, ".claude/skills", "my-skill", "# content");

        // No agents — nothing to search
        let project = project_with_dir(&project_dir, vec![], vec!["my-skill".into()]);
        let result = read_local_skill(&project, "my-skill");
        assert!(result.is_err());
    }

    #[test]
    fn save_local_skill_updates_existing_skill_in_agent_dir() {
        let tmp = tmp();
        let project_dir = tmp.path().join("project");
        make_local_skill(&project_dir, ".claude/skills", "my-skill", "# Old");

        let project =
            project_with_dir(&project_dir, vec!["claude".into()], vec!["my-skill".into()]);
        let written = save_local_skill(&project, "my-skill", "# New").expect("save");
        assert!(!written.is_empty());

        let read_back = read_local_skill(&project, "my-skill").expect("read");
        assert_eq!(read_back, "# New");
    }

    #[test]
    fn save_local_skill_creates_new_skill_in_first_agent_dir() {
        let tmp = tmp();
        let project_dir = tmp.path().join("project");
        fs::create_dir_all(&project_dir).expect("create project dir");

        let project = project_with_dir(&project_dir, vec!["claude".into()], vec![]);
        let written = save_local_skill(&project, "brand-new", "# Brand New").expect("save");
        assert!(!written.is_empty());

        let written_path = std::path::PathBuf::from(&written[0]);
        let on_disk = fs::read_to_string(&written_path).expect("read written file");
        assert_eq!(on_disk, "# Brand New");
    }
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
