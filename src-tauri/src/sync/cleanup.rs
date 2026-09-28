use std::path::PathBuf;

use crate::agent::{self, RemovalEntry, RemovalMode};
use crate::core::Project;

use super::engine::sync_project_without_autodetect;

/// The project directory, when it is set and exists.
fn project_dir(project: &Project) -> Option<PathBuf> {
    if project.directory.is_empty() {
        return None;
    }
    let dir = PathBuf::from(&project.directory);
    dir.exists().then_some(dir)
}

fn remaining_agents(project: &Project, agent_id: &str) -> Vec<String> {
    project
        .agents
        .iter()
        .filter(|id| id.as_str() != agent_id)
        .cloned()
        .collect()
}

/// Remove an agent from a project.
///
/// - [`RemovalMode::Keep`] takes the agent off `project.agents` and saves the
///   project.  No file on disk changes, and the remaining agents are not
///   re-synced, because a re-sync would rewrite shared files.
/// - [`RemovalMode::Remove`] also deletes the agent's config from the project
///   directory, as planned by [`agent::plan_agent_removal`], and then
///   re-syncs the remaining agents so shared files stay correct for them.
///
/// Returns what removal did, in the same shape as
/// [`get_agent_cleanup_preview`].  When some files could not be deleted the
/// agent is still taken off the list, and the error names those files.
pub fn remove_agent_from_project(
    project: &mut Project,
    agent_id: &str,
    mode: RemovalMode,
) -> Result<Vec<RemovalEntry>, String> {
    let remaining = remaining_agents(project, agent_id);

    let outcome = match mode {
        RemovalMode::Keep => Ok(vec![]),
        RemovalMode::Remove => {
            let dir = project_dir(project).ok_or_else(|| {
                format!(
                    "Project directory '{}' is not set or does not exist",
                    project.directory
                )
            })?;
            match agent::from_id(agent_id) {
                Some(agent_instance) => {
                    let plan = agent::plan_agent_removal(agent_instance, &dir, &remaining)?;
                    agent::apply_removal_plan(agent_instance, &dir, &plan)
                }
                None => Ok(vec![]),
            }
        }
    };

    project.agents = remaining;
    project.updated_at = chrono::Utc::now().to_rfc3339();
    let project_str =
        serde_json::to_string_pretty(&project).map_err(|e| format!("Serialise error: {}", e))?;
    crate::core::save_project(crate::core::project_ident(project), &project_str)?;

    if mode == RemovalMode::Remove && !project.agents.is_empty() {
        if let Err(e) = sync_project_without_autodetect(project) {
            eprintln!(
                "[automatic] Re-sync after removing '{}' from '{}' failed: {}",
                agent_id, project.name, e
            );
        }
    }

    outcome
}

/// What [`remove_agent_from_project`] would do in `mode`.  Read-only; feeds
/// the confirmation dialog.  Keep changes no files, so its preview is empty,
/// as is the preview for a project without a directory on disk.
pub fn get_agent_cleanup_preview(
    project: &Project,
    agent_id: &str,
    mode: RemovalMode,
) -> Result<Vec<RemovalEntry>, String> {
    if mode == RemovalMode::Keep {
        return Ok(vec![]);
    }
    let Some(dir) = project_dir(project) else {
        return Ok(vec![]);
    };
    let Some(agent_instance) = agent::from_id(agent_id) else {
        return Ok(vec![]);
    };
    let remaining = remaining_agents(project, agent_id);
    Ok(agent::plan_agent_removal(agent_instance, &dir, &remaining)?.entries())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn with_temp_home<T>(test: impl FnOnce() -> T) -> T {
        let home = tempdir().expect("temp home");
        crate::core::with_test_home(home.path().to_path_buf(), test)
    }

    fn project_in(dir: &std::path::Path, agents: &[&str]) -> Project {
        let project = Project {
            name: "removal-test".to_string(),
            directory: dir.display().to_string(),
            agents: agents.iter().map(|a| a.to_string()).collect(),
            ..Default::default()
        };
        let json = serde_json::to_string_pretty(&project).expect("project json");
        crate::core::save_project(&project.name, &json).expect("save project");
        project
    }

    /// Every file under `root`, with its bytes, so a test can prove nothing
    /// changed.  Skips the project's own config and state files: saving the
    /// project writes Automatic's record of the agent list there, which is the
    /// point.
    fn snapshot(root: &std::path::Path) -> Vec<(PathBuf, Vec<u8>)> {
        let records = [
            root.join(".automatic.json"),
            root.join(".automatic").join("project.json"),
        ];
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir).expect("read dir").flatten() {
                let path = entry.path();
                if records.contains(&path) {
                    continue;
                }
                if path.is_dir() {
                    stack.push(path);
                } else {
                    out.push((path.clone(), fs::read(&path).expect("read file")));
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn keep_takes_the_agent_off_the_list_and_touches_no_file() {
        with_temp_home(|| {
            let project_dir = tempdir().unwrap();
            let root = project_dir.path();
            fs::create_dir_all(root.join(".claude/skills/demo")).unwrap();
            fs::write(root.join(".claude/skills/demo/SKILL.md"), "# Demo").unwrap();
            fs::write(root.join("CLAUDE.md"), "# Instructions").unwrap();
            fs::write(root.join(".mcp.json"), "{}").unwrap();
            fs::write(root.join("AGENTS.md"), "# Shared").unwrap();
            let mut project = project_in(root, &["claude", "codex"]);
            let before = snapshot(root);

            assert!(
                get_agent_cleanup_preview(&project, "claude", RemovalMode::Keep)
                    .unwrap()
                    .is_empty()
            );
            let result =
                remove_agent_from_project(&mut project, "claude", RemovalMode::Keep).unwrap();

            assert!(result.is_empty());
            assert_eq!(project.agents, vec!["codex".to_string()]);
            assert_eq!(snapshot(root), before, "Keep must not change any file");
            let saved: Project =
                serde_json::from_str(&crate::core::read_project("removal-test").unwrap()).unwrap();
            assert_eq!(saved.agents, vec!["codex".to_string()]);
        });
    }

    #[test]
    fn remove_deletes_what_the_preview_lists() {
        with_temp_home(|| {
            let project_dir = tempdir().unwrap();
            let root = project_dir.path();
            fs::create_dir_all(root.join(".claude/notes")).unwrap();
            fs::write(root.join(".claude/notes/mine.md"), "user file").unwrap();
            fs::write(root.join("CLAUDE.md"), "# Instructions").unwrap();
            fs::write(root.join(".mcp.json"), "{}").unwrap();
            let mut project = project_in(root, &["claude"]);

            let preview =
                get_agent_cleanup_preview(&project, "claude", RemovalMode::Remove).unwrap();
            let result =
                remove_agent_from_project(&mut project, "claude", RemovalMode::Remove).unwrap();

            assert_eq!(result, preview);
            assert!(!root.join(".claude").exists());
            assert!(!root.join("CLAUDE.md").exists());
            assert!(!root.join(".mcp.json").exists());
            assert!(project.agents.is_empty());
        });
    }

    #[test]
    fn remove_without_directory_is_an_error_but_keep_is_not() {
        with_temp_home(|| {
            let mut project = Project {
                name: "no-dir".to_string(),
                agents: vec!["claude".to_string()],
                ..Default::default()
            };
            assert!(
                remove_agent_from_project(&mut project.clone(), "claude", RemovalMode::Remove)
                    .is_err()
            );
            assert!(remove_agent_from_project(&mut project, "claude", RemovalMode::Keep).is_ok());
            assert!(project.agents.is_empty());
        });
    }
}
