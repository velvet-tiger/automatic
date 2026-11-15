//! Read-only check of whether a project's rules are on disk the way sync
//! writes them.
//!
//! The Rules page shows this per project.  It must follow the same routing as
//! `sync_instruction_target_file` and the Cursor `.mdc` step in the engine:
//! the same resolved rule list (mandatory and gitignore rules included), the
//! same owner agent per instruction file, and the same target for each mode.
//! A check that drifts from the writer reports projects as stale for ever,
//! because a sync cannot change what the check expects.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::engine::resolve_rules_for_target;
use super::helpers::escaped_silent_dir;
use crate::agent;
use crate::core::{self, Project, ProjectMode};

/// One instruction file sync writes, and the agent whose rule routing
/// applies to it.  As in the engine, the first agent listed that uses a file
/// owns it.
struct RuleTarget {
    agent_id: String,
    filename: String,
}

fn rule_targets(project: &Project) -> Vec<RuleTarget> {
    let mut seen = HashSet::new();
    project
        .agents
        .iter()
        .filter_map(|id| agent::from_id(id).map(|a| (id, a)))
        .filter(|(_, a)| seen.insert(a.project_file_name().to_string()))
        .map(|(id, a)| RuleTarget {
            agent_id: id.clone(),
            filename: a.project_file_name().to_string(),
        })
        .collect()
}

fn uses_unified_rules(project: &Project) -> bool {
    project.instruction_mode == "unified"
}

/// Every library rule that sync places in this project's instruction files,
/// including the mandatory rules and the gitignore rule.  Empty when the
/// project has no known agents, since sync then writes no rules.
pub(crate) fn project_rule_names(project: &Project) -> Vec<String> {
    let unified = uses_unified_rules(project);
    let mut names: Vec<String> = Vec::new();
    for target in rule_targets(project) {
        for rule in resolve_rules_for_target(project, &target.filename, unified) {
            if !names.contains(&rule) {
                names.push(rule);
            }
        }
    }
    names
}

/// Returns `true` when every rule sync would write for `project` is already
/// on disk with the content sync would write.  Nothing is written.
///
/// A project whose folder is missing is never current: sync cannot run.
/// Custom rules are compared with their stored content.  While a custom rule
/// conflict is open, sync keeps the on-disk text instead, so the project
/// reads as not current until the conflict is resolved.
pub(crate) fn project_rules_current(project: &Project) -> bool {
    if project.directory.is_empty() {
        return false;
    }
    let dir = PathBuf::from(&project.directory);
    if !dir.is_dir() {
        return false;
    }
    let effective_dir = match project.mode {
        ProjectMode::Silent => dir.join(".automatic").join("silent"),
        ProjectMode::Normal => dir.clone(),
    };
    if escaped_silent_dir(&dir, &effective_dir).is_some() {
        return false;
    }

    let unified = uses_unified_rules(project);
    let custom_contents: Vec<String> = project
        .custom_rules
        .iter()
        .filter(|r| !r.content.trim().is_empty())
        .map(|r| r.content.clone())
        .collect();

    let targets_current = rule_targets(project).iter().all(|target| {
        let rules = resolve_rules_for_target(project, &target.filename, unified);
        target_rules_current(project, &effective_dir, target, &rules, &custom_contents)
    });
    targets_current && shared_file_cursor_mdc_current(project, &effective_dir, unified)
}

/// Where sync routes the library rules of one instruction file.
enum RulesDir {
    DotClaude,
    CursorMdc,
}

fn target_rules_current(
    project: &Project,
    effective_dir: &Path,
    target: &RuleTarget,
    rules: &[String],
    custom_contents: &[String],
) -> bool {
    let path = effective_dir.join(&target.filename);
    let rules_dir = match target.agent_id.as_str() {
        "claude" if core::project_uses_dot_claude_rules(project, &target.filename) => {
            Some(RulesDir::DotClaude)
        }
        "cursor" if core::project_uses_cursor_mdc_rules(project, &target.filename) => {
            Some(RulesDir::CursorMdc)
        }
        _ => None,
    };
    let effective_str = effective_dir.to_string_lossy();

    match rules_dir {
        Some(kind) => {
            // The agent writes its rules directory only when there are rules,
            // and keeps custom rules inline in the instruction file.
            let dir_current = rules.is_empty()
                || match kind {
                    RulesDir::DotClaude => core::is_dot_claude_rules_current(&effective_str, rules),
                    RulesDir::CursorMdc => core::is_cursor_mdc_rules_current(&effective_str, rules),
                }
                .unwrap_or(false);
            dir_current
                && core::is_file_rules_current_with_custom(effective_dir, &path, &[], custom_contents)
                    .unwrap_or(false)
        }
        None if project.instructions_index_mode => core::is_index_rules_current(
            &project.directory,
            effective_dir,
            &path,
            rules,
            &project.custom_rules,
        )
        .unwrap_or(false),
        None => core::is_file_rules_current_with_custom(effective_dir, &path, rules, custom_contents)
            .unwrap_or(false),
    }
}

/// Cursor's `.cursor/rules/` option, when Cursor shares `AGENTS.md` with
/// another agent: the rules stay inline for the other agent, and sync also
/// writes the `.mdc` files.  `true` when that case does not apply.
fn shared_file_cursor_mdc_current(project: &Project, effective_dir: &Path, unified: bool) -> bool {
    if !project.agents.iter().any(|a| a == "cursor") {
        return true;
    }
    let Some(cursor) = agent::from_id("cursor") else {
        return true;
    };
    let option_on = project
        .agent_options
        .get("cursor")
        .cloned()
        .unwrap_or_default()
        .cursor_rules_in_dot_cursor;
    if !option_on || core::project_uses_cursor_mdc_rules(project, cursor.project_file_name()) {
        return true;
    }
    let rules = resolve_rules_for_target(project, cursor.project_file_name(), unified);
    core::is_cursor_mdc_rules_current(&effective_dir.to_string_lossy(), &rules).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{CustomRule, Project};

    const RULE: &str = "team-style";

    fn project_in(dir: &Path) -> Project {
        let mut file_rules = std::collections::HashMap::new();
        file_rules.insert("_project".to_string(), vec![RULE.to_string()]);
        Project {
            name: "rule-status".into(),
            directory: dir.display().to_string(),
            agents: vec![
                "claude".into(),
                "cursor".into(),
                "codex".into(),
                "opencode".into(),
            ],
            instruction_mode: "unified".into(),
            file_rules: file_rules.into_iter().collect(),
            manage_gitignore: true,
            custom_rules: vec![CustomRule {
                name: "House note".into(),
                content: "Keep it tidy.".into(),
            }],
            ..Default::default()
        }
    }

    /// Sync the project, then expect it current; change the rule in the
    /// library and expect it stale; sync again and expect it current.
    fn assert_round_trip(label: &str, project: &Project) {
        crate::core::save_rule(RULE, "Team style", "Use short names.").expect("save rule");
        crate::sync::sync_project(project).expect("sync");
        assert!(project_rules_current(project), "{label}: current after sync");

        crate::core::save_rule(RULE, "Team style", "Use long names.").expect("edit rule");
        assert!(!project_rules_current(project), "{label}: stale after a rule edit");

        crate::sync::sync_project(project).expect("resync");
        assert!(project_rules_current(project), "{label}: current after resync");
    }

    fn with_project(label: &str, configure: impl Fn(&mut Project)) {
        let home = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        crate::core::with_test_home(home.path().to_path_buf(), || {
            let mut project = project_in(dir.path());
            configure(&mut project);
            let data = serde_json::to_string_pretty(&project).expect("serialise");
            crate::core::save_project(&project.name, &data).expect("save project");
            assert_round_trip(label, &project);
        });
    }

    #[test]
    fn inline_rules_round_trip() {
        with_project("inline", |_| {});
    }

    #[test]
    fn dot_claude_rules_with_gitignore_rule_round_trip() {
        // The reported case: `.claude/rules/` holds the implicit
        // automatic-gitignore rule, which the old check did not expect.
        with_project("dot-claude", |p| {
            p.agent_options.entry("claude".into()).or_default().claude_rules_in_dot_claude = true;
        });
    }

    #[test]
    fn cursor_mdc_rules_shared_file_round_trip() {
        with_project("cursor-shared", |p| {
            p.agent_options.entry("cursor".into()).or_default().cursor_rules_in_dot_cursor = true;
        });
    }

    #[test]
    fn cursor_mdc_rules_sole_owner_round_trip() {
        with_project("cursor-only", |p| {
            p.agents = vec!["claude".into(), "cursor".into()];
            p.agent_options.entry("cursor".into()).or_default().cursor_rules_in_dot_cursor = true;
        });
    }

    #[test]
    fn index_mode_round_trip() {
        with_project("index", |p| p.instructions_index_mode = true);
    }

    #[test]
    fn per_agent_mode_round_trip() {
        with_project("per-agent", |p| p.instruction_mode = "per-agent".into());
    }

    #[test]
    fn silent_mode_round_trip() {
        with_project("silent", |p| p.mode = ProjectMode::Silent);
    }

    #[test]
    fn rule_names_include_implicit_rules() {
        let dir = tempfile::tempdir().unwrap();
        let names = project_rule_names(&project_in(dir.path()));
        assert!(names.iter().any(|n| n == RULE));
        assert!(names.iter().any(|n| n == core::MANDATORY_RULE));
        assert!(names.iter().any(|n| n == core::GITIGNORE_RULE));
    }

    #[test]
    fn missing_folder_is_not_current() {
        let dir = tempfile::tempdir().unwrap();
        let project = project_in(&dir.path().join("gone"));
        assert!(!project_rules_current(&project));
    }
}
