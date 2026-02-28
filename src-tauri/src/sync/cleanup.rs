use std::path::PathBuf;

use crate::agent;
use crate::core::Project;

use super::engine::sync_project_without_autodetect;

/// Remove an agent from a project and clean up all files it wrote.
///
/// Steps:
/// 1. Compute the remaining agent list (project minus the removed agent).
/// 2. Call [`agent::cleanup_agent_from_project`] to delete / strip the
///    agent's config file and agent-specific skill directories.
/// 3. Update `project.agents` and persist the new project config.
/// 4. If other agents remain, re-sync them so their own configs are still
///    accurate (e.g. no longer lists servers written for the removed agent).
///
/// Returns the list of paths that were removed or modified.
pub fn remove_agent_from_project(
    project: &mut Project,
    agent_id: &str,
) -> Result<Vec<String>, String> {
    if project.directory.is_empty() {
        return Err("Project has no directory configured".into());
    }

    let dir = PathBuf::from(&project.directory);
    if !dir.exists() {
        return Err(format!("Directory '{}' does not exist", project.directory));
    }

    // Compute the remaining agents before mutating the project
    let remaining: Vec<String> = project
        .agents
        .iter()
        .filter(|id| id.as_str() != agent_id)
        .cloned()
        .collect();

    // Clean up the agent's resources
    let removed = if let Some(agent_instance) = agent::from_id(agent_id) {
        agent::cleanup_agent_from_project(agent_instance, &dir, &remaining)
    } else {
        vec![]
    };

    // Update and persist the project
    project.agents = remaining;
    project.updated_at = chrono::Utc::now().to_rfc3339();
    let project_str =
        serde_json::to_string_pretty(&project).map_err(|e| format!("Serialise error: {}", e))?;
    crate::core::save_project(&project.name, &project_str)?;

    // Re-sync remaining agents so their configs are up to date
    if !project.agents.is_empty() {
        let _ = sync_project_without_autodetect(project);
    }

    Ok(removed)
}

/// Return the list of file/directory paths that *would* be removed if
/// [`remove_agent_from_project`] were called for the given agent.
///
/// This is a read-only operation used to populate the confirmation dialog
/// shown before the user commits to the removal.
pub fn get_agent_cleanup_preview(project: &Project, agent_id: &str) -> Result<Vec<String>, String> {
    if project.directory.is_empty() {
        return Ok(vec![]);
    }

    let dir = PathBuf::from(&project.directory);
    if !dir.exists() {
        return Ok(vec![]);
    }

    let remaining: Vec<String> = project
        .agents
        .iter()
        .filter(|id| id.as_str() != agent_id)
        .cloned()
        .collect();

    if let Some(agent_instance) = agent::from_id(agent_id) {
        Ok(agent::cleanup_agent_preview(
            agent_instance,
            &dir,
            &remaining,
        ))
    } else {
        Ok(vec![])
    }
}
