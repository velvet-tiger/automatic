use std::fs;
use std::path::PathBuf;

use crate::agent;
use crate::core::AgentOptions;
use crate::core::Project;

use super::engine::sync_project_without_autodetect;
use super::helpers::cleanup_custom_agents;

/// Remove an agent from a project and clean up all files it wrote.
///
/// Steps:
/// 1. Compute the remaining agent list (project minus the removed agent).
/// 2. Call [`agent::cleanup_agent_from_project`] to delete / strip the
///    agent's config file and agent-specific skill directories.
/// 3. For the `claude` agent, also strip the managed rules block from
///    `CLAUDE.md`, remove any Automatic-managed `.claude/rules/*.md` files,
///    and attempt to remove the now-empty `.claude/` directory.
/// 4. Update `project.agents` and persist the new project config.
/// 5. If other agents remain, re-sync them so their own configs are still
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
    let mut removed = if let Some(agent_instance) = agent::from_id(agent_id) {
        agent::cleanup_agent_from_project(agent_instance, &dir, &remaining)
    } else {
        vec![]
    };

    // Clean up custom agents directory for this agent
    if let Some(agent_instance) = agent::from_id(agent_id) {
        if let Some(agents_dir) = agent_instance.agents_dir(&dir) {
            removed.extend(cleanup_custom_agents(
                &agents_dir,
                agent_instance.agents_file_ext(),
            ));
        }
    }

    // Claude-specific cleanup: strip managed rules from CLAUDE.md and remove
    // any Automatic-managed .claude/rules/*.md files, then prune .claude/ if
    // it is now empty.
    if agent_id == "claude" {
        let opts = project
            .agent_options
            .get("claude")
            .cloned()
            .unwrap_or_default();
        removed.extend(cleanup_claude_project_files(&dir, &opts));
    }

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

    let mut preview = if let Some(agent_instance) = agent::from_id(agent_id) {
        agent::cleanup_agent_preview(agent_instance, &dir, &remaining)
    } else {
        vec![]
    };

    // Include custom agents directory in the preview
    if let Some(agent_instance) = agent::from_id(agent_id) {
        if let Some(agents_dir) = agent_instance.agents_dir(&dir) {
            if agents_dir.exists() {
                if let Ok(entries) = fs::read_dir(&agents_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().is_some_and(|ext| ext == "md") {
                            preview.push(path.display().to_string());
                        }
                    }
                }
            }
        }
    }

    // Include Claude-specific files in the preview
    if agent_id == "claude" {
        let opts = project
            .agent_options
            .get("claude")
            .cloned()
            .unwrap_or_default();
        preview.extend(claude_cleanup_preview(&dir, &opts));
    }

    Ok(preview)
}

// ── Claude-specific cleanup helpers ─────────────────────────────────────────

/// Strip Automatic-managed content from Claude-specific project files.
///
/// Actions:
/// 1. Strip the `<!-- automatic:rules:start -->…<!-- automatic:rules:end -->`
///    block from `CLAUDE.md` if present.
/// 2. Delete every `<!-- managed by Automatic -->` file from `.claude/rules/`.
/// 3. Attempt to remove `.claude/rules/` if now empty, then `.claude/` if
///    now empty (both silently ignored when non-empty or absent).
///
/// Returns the paths of files deleted or modified.
fn cleanup_claude_project_files(dir: &PathBuf, opts: &AgentOptions) -> Vec<String> {
    let mut touched: Vec<String> = Vec::new();

    // 1. Strip managed rules block from CLAUDE.md.
    let claude_md = dir.join("CLAUDE.md");
    if claude_md.exists() {
        if let Ok(content) = fs::read_to_string(&claude_md) {
            let stripped = crate::core::strip_rules_section_pub(&content);
            if stripped != content {
                if fs::write(&claude_md, stripped).is_ok() {
                    touched.push(claude_md.display().to_string());
                }
            }
        }
    }

    // 2. Remove Automatic-managed .claude/rules/*.md files (both modes).
    // Even if opts.claude_rules_in_dot_claude is false now, the files may
    // have been written when the option was enabled — remove them anyway.
    let _ = opts; // suppress unused-variable warning; we always clean regardless
    let rules_dir = dir.join(".claude").join("rules");
    if rules_dir.exists() {
        // Re-use the sync function with an empty rule list: it removes all
        // managed files and writes nothing new.
        match crate::core::sync_rules_to_dot_claude_rules(&dir.display().to_string(), &[]) {
            Ok(removed) => touched.extend(removed),
            Err(e) => eprintln!("Failed to clean .claude/rules/ on agent removal: {}", e),
        }

        // Remove the .claude/rules/ directory itself if now empty.
        let _ = fs::remove_dir(&rules_dir); // silently ignored when non-empty
    }

    // 3. Attempt to remove .claude/ if it is now empty.
    let dot_claude = dir.join(".claude");
    if dot_claude.exists() {
        let _ = fs::remove_dir(&dot_claude); // silently ignored when non-empty
    }

    touched
}

/// Return the paths that [`cleanup_claude_project_files`] would touch —
/// used to populate the confirmation preview before the user commits.
fn claude_cleanup_preview(dir: &PathBuf, _opts: &AgentOptions) -> Vec<String> {
    let mut preview: Vec<String> = Vec::new();

    // CLAUDE.md if it contains a managed rules block.
    let claude_md = dir.join("CLAUDE.md");
    if claude_md.exists() {
        if let Ok(content) = fs::read_to_string(&claude_md) {
            if content.contains("<!-- automatic:rules:start -->") {
                preview.push(claude_md.display().to_string());
            }
        }
    }

    // Automatic-managed .claude/rules/*.md files.
    const MANAGED_HEADER: &str = "<!-- managed by Automatic — do not edit by hand -->";
    let rules_dir = dir.join(".claude").join("rules");
    if rules_dir.exists() {
        if let Ok(entries) = fs::read_dir(&rules_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("md") {
                    continue;
                }
                if let Ok(content) = fs::read_to_string(&path) {
                    if content.starts_with(MANAGED_HEADER) {
                        preview.push(path.display().to_string());
                    }
                }
            }
        }
    }

    preview
}
