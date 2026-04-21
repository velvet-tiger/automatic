use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use crate::agent;
use crate::core::{self, Project, ProjectMode};

use super::autodetect::autodetect_inner;
use super::helpers::{
    build_selected_servers, clean_project_file, extract_agent_machine_name,
    load_mcp_server_configs, load_skill_contents, sync_custom_agents, sync_user_agents,
};

struct InstructionTarget {
    agent_id: String,
    filename: String,
}

/// Discover MCP server configurations from specific agents' existing on-disk
/// config files.  Used when new agents are added to an existing project so
/// that any servers they already have configured are preserved rather than
/// silently discarded when Automatic writes its own config.
///
/// Returns `(server_name, pretty-printed JSON string)` pairs.  The caller is
/// responsible for persisting them to the global registry and for merging the
/// names into `project.mcp_servers` before calling
/// [`sync_project_without_autodetect`].
///
/// The `automatic` server entries are filtered out automatically by
/// `discover_mcp_servers` — they are always injected at sync time.
pub fn discover_new_agent_mcp_configs(
    dir: &std::path::Path,
    agent_ids: &[String],
) -> Vec<(String, String)> {
    let mut discovered = Vec::new();
    for agent_id in agent_ids {
        if let Some(a) = agent::from_id(agent_id) {
            for (name, config) in a.discover_mcp_servers(dir) {
                if let Ok(config_str) = serde_json::to_string_pretty(&config) {
                    discovered.push((name, config_str));
                }
            }
        }
    }
    discovered
}

/// Sync a project's configuration to its directory for all selected agent tools.
/// Returns a list of files that were written.
pub fn sync_project(project: &Project) -> Result<Vec<String>, String> {
    if project.directory.is_empty() {
        return Err("Project has no directory configured".into());
    }

    let dir = PathBuf::from(&project.directory);
    if !dir.exists() {
        return Err(format!("Directory '{}' does not exist", project.directory));
    }

    let (mut updated_project, discovered_servers) = autodetect_inner(project)?;

    // Persist newly discovered MCP server configs into the global registry.
    // This only happens during an explicit sync, not during a read-only load.
    for (name, config_str) in discovered_servers {
        let _ = crate::core::save_mcp_server_config(&name, &config_str);
    }

    sync_project_without_autodetect(&mut updated_project)
}

/// Sync a project's configuration to its directory without re-running
/// dependency autodetection. Useful when reacting to registry changes
/// (e.g. deleting a skill/server) to avoid re-importing stale local files.
pub fn sync_project_without_autodetect(project: &mut Project) -> Result<Vec<String>, String> {
    if project.directory.is_empty() {
        return Err("Project has no directory configured".into());
    }

    let dir = PathBuf::from(&project.directory);
    if !dir.exists() {
        return Err(format!("Directory '{}' does not exist", project.directory));
    }

    // Ensure the project config is written to the project directory
    if let Ok(proj_str) = serde_json::to_string_pretty(project) {
        let _ = crate::core::save_project(&project.name, &proj_str);
    }

    // In Silent mode all synced files are written under .automatic/silent/
    // instead of the project root, leaving the project tree untouched.
    let effective_dir = match project.mode {
        ProjectMode::Silent => {
            let silent_dir = dir.join(".automatic").join("silent");
            fs::create_dir_all(&silent_dir).map_err(|e| {
                format!("Failed to create silent sync dir '{}': {}", silent_dir.display(), e)
            })?;
            silent_dir
        }
        ProjectMode::Normal => dir.clone(),
    };

    // Read MCP server configs from the Automatic registry and build the
    // selected server map (includes stripping internal fields and OAuth proxy
    // substitution).  Uses the shared helper so drift detection produces
    // identical output.
    let mcp_config = load_mcp_server_configs()?;
    let enabled_mcp_servers = project.enabled_mcp_servers();
    let selected_servers = build_selected_servers(&project.name, &enabled_mcp_servers, &mcp_config);

    // Read all skill contents from the global skill registry, then append
    // project-scoped custom skills (which live inline in the project JSON
    // rather than in ~/.automatic/skills/).
    let mut skill_contents = load_skill_contents(&project.skills);
    let custom_skills = project.custom_skills.as_deref().unwrap_or(&[]);
    for cs in custom_skills {
        skill_contents.push((cs.name.clone(), cs.content.clone()));
    }
    let custom_skill_names: Vec<String> = custom_skills.iter().map(|s| s.name.clone()).collect();
    let workspace_command_contents: Vec<(String, String)> = project
        .user_commands
        .iter()
        .filter_map(|name| {
            core::read_user_command(name)
                .ok()
                .map(|content| (name.clone(), content))
        })
        .collect();

    let mut written_files = Vec::new();
    let all_selected_skill_names: Vec<String> = project
        .skills
        .iter()
        .chain(custom_skill_names.iter())
        .cloned()
        .collect();

    let project_skills_dir = sync_project_skills_step(
        &effective_dir,
        project,
        &skill_contents,
        &all_selected_skill_names,
        &mut written_files,
    )?;
    let project_commands_dir = sync_project_commands_step(
        &effective_dir,
        project,
        &workspace_command_contents,
        &mut written_files,
    )?;
    let instruction_targets = sync_agent_configs_step(
        &effective_dir,
        project,
        &project_skills_dir,
        &project_commands_dir,
        &skill_contents,
        &all_selected_skill_names,
        &workspace_command_contents,
        &selected_servers,
        &mut written_files,
    )?;
    let effective_dir_str = effective_dir
        .to_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| project.directory.clone());
    let written_instruction_files = sync_instruction_files_step(
        &effective_dir,
        project,
        &instruction_targets,
        &effective_dir_str,
        &mut written_files,
    )?;
    record_instruction_state_step(project, &written_instruction_files, &effective_dir_str);

    Ok(written_files)
}

fn sync_project_skills_step(
    dir: &PathBuf,
    project: &Project,
    skill_contents: &[(String, String)],
    all_selected_skill_names: &[String],
    written_files: &mut Vec<String>,
) -> Result<PathBuf, String> {
    // Step 1: copy skills into the project's canonical .agents/skills/.
    let project_skills_dir = dir.join(".agents").join("skills");
    agent::copy_skills_to_project(
        &project_skills_dir,
        skill_contents,
        all_selected_skill_names,
        &project.local_skills,
        written_files,
    )?;

    Ok(project_skills_dir)
}

fn sync_project_commands_step(
    dir: &PathBuf,
    project: &Project,
    workspace_command_contents: &[(String, String)],
    written_files: &mut Vec<String>,
) -> Result<PathBuf, String> {
    let project_commands_dir = dir.join(".agents").join("commands");
    let custom_commands = project.custom_commands.as_deref().unwrap_or(&[]);
    written_files.extend(agent::copy_commands_to_project(
        &project_commands_dir,
        workspace_command_contents,
        custom_commands,
    )?);

    Ok(project_commands_dir)
}

fn sync_agent_configs_step(
    dir: &PathBuf,
    project: &Project,
    project_skills_dir: &PathBuf,
    project_commands_dir: &PathBuf,
    skill_contents: &[(String, String)],
    all_selected_skill_names: &[String],
    workspace_command_contents: &[(String, String)],
    selected_servers: &serde_json::Map<String, serde_json::Value>,
    written_files: &mut Vec<String>,
) -> Result<Vec<InstructionTarget>, String> {
    let project_groups = crate::core::groups_for_project(&project.name);
    let mut cleaned_project_files = HashSet::new();
    let mut instruction_targets = Vec::new();

    for agent_id in &project.agents {
        match agent::from_id(agent_id) {
            Some(agent_instance) => {
                for skill_dir in agent_instance.skill_dirs(dir) {
                    if skill_dir == *project_skills_dir {
                        continue;
                    }
                    agent::symlink_skills_from_project(
                        &skill_dir,
                        project_skills_dir,
                        skill_contents,
                        all_selected_skill_names,
                        &project.local_skills,
                        written_files,
                    )?;
                }

                let path = agent_instance.write_mcp_config(dir, selected_servers)?;
                if !path.is_empty() {
                    written_files.push(path);
                }

                if let Some(agents_dir) = agent_instance.agents_dir(dir) {
                    let custom_agents = project.custom_agents.as_deref().unwrap_or(&[]);
                    let agent_files =
                        sync_custom_agents(&agents_dir, custom_agents, agent_instance)?;
                    written_files.extend(agent_files);

                    let custom_agent_names: Vec<String> = custom_agents
                        .iter()
                        .map(|a| {
                            extract_agent_machine_name(&a.content)
                                .unwrap_or_else(|| a.name.to_lowercase().replace(' ', "-"))
                        })
                        .collect();

                    let user_agent_files = sync_user_agents(
                        &agents_dir,
                        &project.user_agents,
                        &custom_agent_names,
                        agent_instance,
                    )?;
                    written_files.extend(user_agent_files);
                }

                if let Some(commands_dir) = agent_instance.commands_dir(dir) {
                    let custom_commands = project.custom_commands.as_deref().unwrap_or(&[]);
                    let command_files = if agent_instance.commands_file_ext() == "md" {
                        agent::symlink_commands_from_project(
                            &commands_dir,
                            project_commands_dir,
                            workspace_command_contents,
                            custom_commands,
                            agent_instance,
                        )?
                    } else {
                        agent::sync_commands_to_dir(
                            &commands_dir,
                            workspace_command_contents,
                            custom_commands,
                            agent_instance,
                        )?
                    };
                    written_files.extend(command_files);
                }

                let pf = agent_instance.project_file_name();
                if !cleaned_project_files.contains(pf) {
                    cleaned_project_files.insert(pf.to_string());
                    instruction_targets.push(InstructionTarget {
                        agent_id: agent_id.clone(),
                        filename: pf.to_string(),
                    });

                    if let Ok(path) = clean_project_file(dir, pf) {
                        if let Some(p) = path {
                            written_files.push(p);
                        }
                    }

                    if let Ok(true) = crate::core::inject_groups_into_project_file(
                        dir.to_str().unwrap_or(&project.directory),
                        pf,
                        &project.name,
                        &project_groups,
                    ) {
                        let groups_path = dir.join(pf).display().to_string();
                        if !written_files.contains(&groups_path) {
                            written_files.push(groups_path);
                        }
                    }
                }
            }
            None => {
                eprintln!("Unknown agent '{}', skipping", agent_id);
            }
        }
    }

    Ok(instruction_targets)
}

fn sync_instruction_files_step(
    dir: &PathBuf,
    project: &Project,
    instruction_targets: &[InstructionTarget],
    write_dir: &str,
    written_files: &mut Vec<String>,
) -> Result<Vec<String>, String> {
    if !project.instructions_index_mode {
        let _ = crate::core::sync_rules_to_automatic_instructions(&project.directory, &[], &[]);
    }

    let mut written_instruction_files = Vec::new();

    if project.instruction_mode == "unified" {
        let shared_user_content =
            resolve_unified_instruction_content(dir, project, instruction_targets);
        if let Some(shared_user_content) = shared_user_content {
            for target in instruction_targets {
                sync_instruction_target_file(
                    dir,
                    project,
                    &target.agent_id,
                    &target.filename,
                    &shared_user_content,
                    true,
                    write_dir,
                    written_files,
                )?;
                written_instruction_files.push(target.filename.clone());
            }
        }
    } else {
        for target in instruction_targets {
            let user_content = crate::core::read_project_file(write_dir, &target.filename)
                .unwrap_or_default();
            sync_instruction_target_file(
                dir,
                project,
                &target.agent_id,
                &target.filename,
                &user_content,
                false,
                write_dir,
                written_files,
            )?;
            written_instruction_files.push(target.filename.clone());
        }
    }

    Ok(written_instruction_files)
}

fn resolve_unified_instruction_content(
    dir: &PathBuf,
    project: &Project,
    instruction_targets: &[InstructionTarget],
) -> Option<String> {
    let mut file_contents: Vec<(String, String)> = Vec::new();
    for target in instruction_targets {
        let path = dir.join(&target.filename);
        if path.exists() {
            if let Ok(raw) = fs::read_to_string(&path) {
                let user_content = crate::core::strip_index_section(
                    &crate::core::strip_groups_section(&crate::core::strip_rules_section_pub(
                        &crate::core::strip_managed_section_pub(&raw),
                    )),
                );
                file_contents.push((target.filename.clone(), user_content));
            }
        }
    }

    let any_externally_modified = file_contents.iter().any(|(filename, _)| {
        if let Some(stored_hash) = project.instruction_file_hashes.get(filename) {
            let on_disk_path = dir.join(filename);
            if let Ok(raw) = fs::read_to_string(&on_disk_path) {
                let current_hash = crate::core::compute_content_hash(&raw);
                return &current_hash != stored_hash;
            }
        }
        false
    });

    let all_consistent = if file_contents.len() > 1 {
        let first_content = &file_contents[0].1;
        file_contents
            .iter()
            .all(|(_, c)| c.trim() == first_content.trim())
    } else {
        true
    };

    if any_externally_modified || !all_consistent {
        eprintln!(
            "[automatic] Unified replication skipped: instruction file(s) were modified externally. \
             Drift detection will surface the conflict."
        );
        return None;
    }

    let source_file = instruction_targets
        .iter()
        .find(|target| dir.join(&target.filename).exists())
        .map(|target| target.filename.clone());

    if let Some(source) = source_file {
        let raw = fs::read_to_string(dir.join(&source)).unwrap_or_default();
        Some(crate::core::strip_index_section(
            &crate::core::strip_groups_section(&crate::core::strip_rules_section_pub(
                &crate::core::strip_managed_section_pub(&raw),
            )),
        ))
    } else {
        Some(String::new())
    }
}

fn sync_instruction_target_file(
    dir: &PathBuf,
    project: &Project,
    agent_id: &str,
    filename: &str,
    user_content: &str,
    use_unified_rules: bool,
    write_dir: &str,
    written_files: &mut Vec<String>,
) -> Result<(), String> {
    let Some(agent_instance) = agent::from_id(agent_id) else {
        return Err(format!("Unknown agent '{}'", agent_id));
    };

    let user_rules: Vec<String> = project
        .file_rules
        .get("_project")
        .filter(|v| !v.is_empty())
        .or_else(|| {
            if use_unified_rules {
                project.file_rules.get("_unified")
            } else {
                project.file_rules.get(filename)
            }
        })
        .cloned()
        .unwrap_or_default();
    let has_commands = !project.user_commands.is_empty()
        || project
            .custom_commands
            .as_ref()
            .is_some_and(|commands| !commands.is_empty());
    let rules = crate::core::ensure_automatic_rules(&user_rules, has_commands);
    let custom_contents: Vec<String> = project
        .custom_rules
        .iter()
        .filter(|r| !r.content.trim().is_empty())
        .map(|r| r.content.clone())
        .collect();
    let custom_rule_structs = project.custom_rules.clone();
    let file_path = dir.join(filename).display().to_string();
    let project_groups = crate::core::groups_for_project(&project.name);
    let uses_dot_claude_rules = crate::core::project_uses_dot_claude_rules(project, filename);

    crate::core::save_project_file(write_dir, filename, user_content)?;
    let _ = crate::core::inject_groups_into_project_file(
        write_dir,
        filename,
        &project.name,
        &project_groups,
    );

    let mut custom_rules_handled = false;
    if uses_dot_claude_rules {
        // When write_dir differs from project.directory (Silent mode), redirect
        // the .claude/rules/ writes to write_dir by using a temporary project.
        let tmp_project;
        let project_for_rules: &Project = if write_dir != project.directory {
            tmp_project = crate::core::Project {
                directory: write_dir.to_string(),
                ..project.clone()
            };
            &tmp_project
        } else {
            project
        };
        if let Some(touched) =
            agent_instance.sync_instruction_rules(project_for_rules, filename, &rules, &custom_contents)?
        {
            custom_rules_handled = true;
            for path in touched {
                if !written_files.contains(&path) {
                    written_files.push(path);
                }
            }
        }
    }

    if project.instructions_index_mode && !uses_dot_claude_rules {
        // .automatic/instructions/ lives inside .automatic/ — never redirect it to
        // the Silent write root.  Always write to the real project directory.
        match crate::core::sync_rules_to_automatic_instructions(
            &project.directory,
            &rules,
            &custom_rule_structs,
        ) {
            Ok(touched) => written_files.extend(touched),
            Err(e) => eprintln!("Failed to sync rules to .automatic/instructions/: {}", e),
        }
        if let Ok(true) = crate::core::inject_index_into_project_file(
            write_dir,
            filename,
            &rules,
            &custom_rule_structs,
        ) {
            if !written_files.contains(&file_path) {
                written_files.push(file_path);
            }
        }
    } else if !custom_rules_handled {
        if let Ok(true) = crate::core::inject_rules_into_project_file_with_custom(
            write_dir,
            filename,
            &rules,
            &custom_contents,
        ) {
            if !written_files.contains(&file_path) {
                written_files.push(file_path);
            }
        }
    }

    Ok(())
}

fn record_instruction_state_step(
    project: &mut Project,
    written_instruction_files: &[String],
    effective_dir: &str,
) {
    let project_name = project.name.clone();

    if effective_dir != project.directory {
        // Silent mode: compute hashes directly from the files in effective_dir and
        // store them on the real project (whose directory field must not change).
        // We do NOT use record_instruction_hashes_for_filenames here because that
        // function persists the project to disk — passing a clone with a different
        // directory would overwrite project.directory with the silent path.
        let hash_dir = std::path::PathBuf::from(effective_dir);
        let mut seen: HashSet<String> = HashSet::new();
        for filename in written_instruction_files {
            if !seen.insert(filename.clone()) {
                continue;
            }
            let path = hash_dir.join(filename);
            if path.is_file() {
                if let Ok(content) = fs::read_to_string(&path) {
                    project.instruction_file_hashes.insert(
                        filename.clone(),
                        crate::core::compute_content_hash(&content),
                    );
                }
            } else {
                project.instruction_file_hashes.remove(filename);
            }
        }
        // Persist the real project (directory unchanged) with the updated hashes.
        if let Ok(data) = serde_json::to_string_pretty(project) {
            let _ = crate::core::save_project(&project_name, &data);
        }
    } else {
        crate::core::record_instruction_hashes_for_filenames(
            &project_name,
            project,
            written_instruction_files,
        );
    }

    let mut snap_seen: HashSet<String> = HashSet::new();
    for filename in written_instruction_files {
        if !snap_seen.insert(filename.clone()) {
            continue;
        }

        if let Ok(user_content) = crate::core::read_project_file(effective_dir, filename) {
            let _ =
                crate::core::save_instruction_snapshot(&project.directory, filename, &user_content);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{CustomRule, read_project_file, save_project_file_for_project};
    use tempfile::TempDir;

    const USER_INSTRUCTIONS: &str = "# Instructions\n\nFollow the project conventions.";
    const CUSTOM_RULE_NAME: &str = "Test Rule";
    const CUSTOM_RULE_CONTENT: &str = "Always explain why the change exists.";
    const INSTRUCTION_FILE: &str = "AGENTS.md";
    const CUSTOM_RULE_FILE: &str = ".automatic/instructions/custom-test-rule.md";

    #[derive(Clone, Copy)]
    enum Action {
        Save,
        Sync,
        ToggleSplitOn,
        ToggleSplitOff,
    }

    fn tmp() -> TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn make_project(dir: &str, split_rules: bool, rules_set: bool) -> Project {
        let mut project = Project {
            name: "test-project".to_string(),
            directory: dir.to_string(),
            agents: vec!["opencode".to_string()],
            instruction_mode: "per-agent".to_string(),
            instructions_index_mode: split_rules,
            ..Default::default()
        };

        if rules_set {
            project.custom_rules = vec![CustomRule {
                name: CUSTOM_RULE_NAME.to_string(),
                content: CUSTOM_RULE_CONTENT.to_string(),
            }];
        }

        project
    }

    fn make_cline_project(dir: &str) -> Project {
        Project {
            name: "test-project".to_string(),
            directory: dir.to_string(),
            agents: vec!["cline".to_string()],
            instruction_mode: "per-agent".to_string(),
            ..Default::default()
        }
    }

    fn apply_action(
        action: Action,
        project: &mut Project,
        instructions_set: bool,
    ) -> Result<(), String> {
        match action {
            Action::Save => save_project_file_for_project(
                project,
                INSTRUCTION_FILE,
                if instructions_set {
                    USER_INSTRUCTIONS
                } else {
                    ""
                },
            ),
            Action::Sync => {
                if instructions_set {
                    crate::core::save_project_file(
                        &project.directory,
                        INSTRUCTION_FILE,
                        USER_INSTRUCTIONS,
                    )?;
                }
                sync_project_without_autodetect(project).map(|_| ())
            }
            Action::ToggleSplitOn => {
                save_project_file_for_project(
                    project,
                    INSTRUCTION_FILE,
                    if instructions_set {
                        USER_INSTRUCTIONS
                    } else {
                        ""
                    },
                )?;
                project.instructions_index_mode = true;
                sync_project_without_autodetect(project).map(|_| ())
            }
            Action::ToggleSplitOff => {
                save_project_file_for_project(
                    project,
                    INSTRUCTION_FILE,
                    if instructions_set {
                        USER_INSTRUCTIONS
                    } else {
                        ""
                    },
                )?;
                project.instructions_index_mode = false;
                sync_project_without_autodetect(project).map(|_| ())
            }
        }
    }

    fn assert_instruction_state(project: &Project, instructions_set: bool, rules_set: bool) {
        let file_path = PathBuf::from(&project.directory).join(INSTRUCTION_FILE);
        let on_disk = fs::read_to_string(&file_path).expect("read instruction file");
        let read_back =
            read_project_file(&project.directory, INSTRUCTION_FILE).expect("read user content");
        let custom_rule_path = PathBuf::from(&project.directory).join(CUSTOM_RULE_FILE);

        if instructions_set {
            assert!(on_disk.contains(USER_INSTRUCTIONS));
            assert_eq!(read_back.trim(), USER_INSTRUCTIONS.trim());
        } else {
            assert!(!on_disk.contains(USER_INSTRUCTIONS));
            assert_eq!(read_back.trim(), "");
        }

        if project.instructions_index_mode {
            assert!(
                on_disk.contains("<!-- automatic:index:start -->"),
                "expected index mode in {:?}",
                on_disk
            );
            assert!(
                !on_disk.contains(CUSTOM_RULE_CONTENT),
                "custom rule content should move out of the main instruction file in split mode: {:?}",
                on_disk
            );
            if rules_set {
                let custom_rule =
                    fs::read_to_string(&custom_rule_path).expect("read split custom rule");
                assert!(custom_rule.contains(CUSTOM_RULE_CONTENT));
            } else {
                assert!(!custom_rule_path.exists());
            }
        } else {
            assert!(!on_disk.contains("<!-- automatic:index:start -->"));
            if rules_set {
                assert!(
                    on_disk.contains(CUSTOM_RULE_CONTENT),
                    "custom rule content should stay inline when split mode is off: {:?}",
                    on_disk
                );
            } else {
                assert!(!on_disk.contains(CUSTOM_RULE_CONTENT));
            }
            assert!(!custom_rule_path.exists());
        }
    }

    #[test]
    fn save_covers_instruction_rule_and_split_combinations() {
        for instructions_set in [false, true] {
            for rules_set in [false, true] {
                for split_rules in [false, true] {
                    let dir = tmp();
                    let mut project =
                        make_project(dir.path().to_str().unwrap(), split_rules, rules_set);

                    apply_action(Action::Save, &mut project, instructions_set).expect("save");
                    assert_instruction_state(&project, instructions_set, rules_set);
                }
            }
        }
    }

    #[test]
    fn sync_covers_instruction_rule_and_split_combinations() {
        for instructions_set in [false, true] {
            for rules_set in [false, true] {
                for split_rules in [false, true] {
                    let dir = tmp();
                    let mut project =
                        make_project(dir.path().to_str().unwrap(), split_rules, rules_set);

                    apply_action(Action::Sync, &mut project, instructions_set).expect("sync");
                    assert_instruction_state(&project, instructions_set, rules_set);
                }
            }
        }
    }

    #[test]
    fn toggle_split_on_covers_instruction_and_rule_combinations() {
        for instructions_set in [false, true] {
            for rules_set in [false, true] {
                let dir = tmp();
                let mut project = make_project(dir.path().to_str().unwrap(), false, rules_set);

                apply_action(Action::ToggleSplitOn, &mut project, instructions_set)
                    .expect("toggle split on");
                assert_instruction_state(&project, instructions_set, rules_set);
            }
        }
    }

    #[test]
    fn toggle_split_off_covers_instruction_and_rule_combinations() {
        for instructions_set in [false, true] {
            for rules_set in [false, true] {
                let dir = tmp();
                let mut project = make_project(dir.path().to_str().unwrap(), true, rules_set);

                apply_action(Action::ToggleSplitOff, &mut project, instructions_set)
                    .expect("toggle split off");
                assert_instruction_state(&project, instructions_set, rules_set);
            }
        }
    }

    #[test]
    fn sync_migrates_legacy_clinerules_file_to_directory() {
        let dir = tmp();
        let legacy_content = "# Legacy Cline Instructions\n\nKeep this during upgrade.";
        fs::write(dir.path().join(".clinerules"), legacy_content).expect("write legacy file");
        let mut project = make_cline_project(dir.path().to_str().unwrap());

        sync_project_without_autodetect(&mut project).expect("sync");

        let clinerules_dir = dir.path().join(".clinerules");
        assert!(
            clinerules_dir.is_dir(),
            "legacy .clinerules should become a directory"
        );

        let migrated_path = clinerules_dir.join("automatic.md");
        let on_disk = fs::read_to_string(&migrated_path).expect("read migrated file");
        assert!(
            on_disk.contains(legacy_content),
            "sync should preserve legacy user instructions during migration"
        );

        let user_content =
            read_project_file(dir.path().to_str().unwrap(), ".clinerules/automatic.md")
                .expect("read migrated project file");
        assert_eq!(user_content.trim(), legacy_content.trim());
    }

    #[test]
    fn sync_keeps_conflicted_unified_instruction_files_unmanaged() {
        let dir = tmp();
        fs::write(dir.path().join("AGENTS.md"), "# Agents\n\nCodex-specific")
            .expect("write AGENTS");
        fs::write(dir.path().join("CLAUDE.md"), "# Claude\n\nClaude-specific")
            .expect("write CLAUDE");

        let mut project = Project {
            name: "test-project".to_string(),
            directory: dir.path().to_str().unwrap().to_string(),
            agents: vec!["claude".to_string(), "opencode".to_string()],
            instruction_mode: "unified".to_string(),
            ..Default::default()
        };

        sync_project_without_autodetect(&mut project).expect("sync");

        assert!(
            project.instruction_file_hashes.is_empty(),
            "conflicted files should not be marked as Automatic-managed"
        );

        let drift = crate::sync::check_project_drift(&project).expect("drift");
        assert_eq!(
            drift.instruction_conflicts.len(),
            2,
            "both conflicting unified files should still require resolution"
        );
    }
}
