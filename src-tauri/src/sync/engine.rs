use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use crate::agent;
use crate::core::{self, Project};

use super::autodetect::autodetect_inner;
use super::helpers::{
    build_selected_servers, clean_project_file, clean_project_file_rules_section,
    extract_agent_machine_name, load_mcp_server_configs, load_skill_contents, sync_custom_agents,
    sync_user_agents,
};

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

    // ── Step 1: Copy skills into the project's canonical .agents/skills/ ──
    //
    // This is the project-local hub.  Full directories are copied from the
    // global registry (~/.agents/skills/) so companion files are included.
    // Custom skills (project-scoped) are included alongside global skills.
    let project_skills_dir = dir.join(".agents").join("skills");
    let all_selected_skill_names: Vec<String> = project
        .skills
        .iter()
        .chain(custom_skill_names.iter())
        .cloned()
        .collect();
    agent::copy_skills_to_project(
        &project_skills_dir,
        &skill_contents,
        &all_selected_skill_names,
        &project.local_skills,
        &mut written_files,
    )?;

    // Look up all groups this project belongs to once, before the per-agent loop.
    let project_groups = crate::core::groups_for_project(&project.name);

    // ── Step 2: Per-agent config (MCP, symlinks, project-file cleanup) ────
    let mut cleaned_project_files = HashSet::new();
    for agent_id in &project.agents {
        match agent::from_id(agent_id) {
            Some(agent_instance) => {
                // Symlink agent-specific skill directories to the project hub.
                // Agents whose skill dir IS .agents/skills/ are skipped — they
                // already have the skills from Step 1.
                for skill_dir in agent_instance.skill_dirs(&dir) {
                    if skill_dir == project_skills_dir {
                        continue;
                    }
                    agent::symlink_skills_from_project(
                        &skill_dir,
                        &project_skills_dir,
                        &skill_contents,
                        &all_selected_skill_names,
                        &project.local_skills,
                        &mut written_files,
                    )?;
                }

                let path = agent_instance.write_mcp_config(&dir, &selected_servers)?;
                // write_mcp_config returns "" for agents (like Warp) that
                // cannot have their MCP config managed by Automatic.
                if !path.is_empty() {
                    written_files.push(path);
                }

                // Sync custom agents to this provider's agents directory
                if let Some(agents_dir) = agent_instance.agents_dir(&dir) {
                    let custom_agents = project.custom_agents.as_deref().unwrap_or(&[]);
                    let agent_files =
                        sync_custom_agents(&agents_dir, custom_agents, agent_instance)?;
                    written_files.extend(agent_files);

                    // Collect custom agent machine names for stale file check
                    let custom_agent_names: Vec<String> = custom_agents
                        .iter()
                        .map(|a| {
                            extract_agent_machine_name(&a.content)
                                .unwrap_or_else(|| a.name.to_lowercase().replace(' ', "-"))
                        })
                        .collect();

                    // Sync workspace user_agents (from ~/.automatic/agents/)
                    let user_agent_files = sync_user_agents(
                        &agents_dir,
                        &project.user_agents,
                        &custom_agent_names,
                        agent_instance,
                    )?;
                    written_files.extend(user_agent_files);
                }

                if let Some(commands_dir) = agent_instance.commands_dir(&dir) {
                    let custom_commands = project.custom_commands.as_deref().unwrap_or(&[]);
                    let command_files = agent::sync_commands_to_dir(
                        &commands_dir,
                        &workspace_command_contents,
                        custom_commands,
                        agent_instance,
                    )?;
                    written_files.extend(command_files);
                }

                // Strip legacy managed sections from project files (once per filename)
                let pf = agent_instance.project_file_name();
                if !cleaned_project_files.contains(pf) {
                    cleaned_project_files.insert(pf.to_string());
                    if let Ok(path) = clean_project_file(&dir, pf) {
                        if let Some(p) = path {
                            written_files.push(p);
                        }
                    }

                    // Inject (or update) the project-group context block.
                    // This sits between the user content and the rules section
                    // so agents can discover related projects.
                    if let Ok(true) = crate::core::inject_groups_into_project_file(
                        &project.directory,
                        pf,
                        &project.name,
                        &project_groups,
                    ) {
                        let groups_path = dir.join(pf).display().to_string();
                        if !written_files.contains(&groups_path) {
                            written_files.push(groups_path);
                        }
                    }

                    // Resolve the rules assigned to this project file.
                    // Priority order:
                    //   1. "_project" — project-level rules set from the Rules tab (applies to all files)
                    //   2. "_unified" — legacy unified-mode key
                    //   3. Per-file key (e.g. "CLAUDE.md") — legacy per-agent mode
                    let rules: Option<&Vec<String>> = project
                        .file_rules
                        .get("_project")
                        .filter(|v| !v.is_empty())
                        .or_else(|| {
                            if project.instruction_mode == "unified" {
                                project.file_rules.get("_unified")
                            } else {
                                project.file_rules.get(pf)
                            }
                        });

                    // Resolve per-agent options for this agent (use defaults if absent).
                    let opts = project
                        .agent_options
                        .get(agent_id)
                        .cloned()
                        .unwrap_or_default();

                    // Collect custom rule content strings for this project.
                    let custom_contents: Vec<String> = project
                        .custom_rules
                        .iter()
                        .filter(|r| !r.content.trim().is_empty())
                        .map(|r| r.content.clone())
                        .collect();

                    let has_any_rules = rules.map(|r| !r.is_empty()).unwrap_or(false)
                        || !custom_contents.is_empty();

                    if let Some(rules) = rules {
                        if !rules.is_empty() || !custom_contents.is_empty() {
                            // Claude Code supports writing rules as individual files under
                            // `.claude/rules/` — the format recommended by the Claude Code
                            // documentation.  Use that path when the option is enabled.
                            // Note: custom (inline) rules are always injected inline regardless
                            // of this option — they don't have a machine name to use as a filename.
                            if agent_id == "claude" && opts.claude_rules_in_dot_claude {
                                // Write global rules as .claude/rules/<name>.md files.
                                match crate::core::sync_rules_to_dot_claude_rules(
                                    &project.directory,
                                    rules,
                                ) {
                                    Ok(touched) => written_files.extend(touched),
                                    Err(e) => {
                                        eprintln!("Failed to sync rules to .claude/rules/: {}", e)
                                    }
                                }
                                // Custom rules are still injected inline even in dot-claude mode.
                                if !custom_contents.is_empty() {
                                    if let Ok(true) =
                                        crate::core::inject_rules_into_project_file_with_custom(
                                            &project.directory,
                                            pf,
                                            &[],
                                            &custom_contents,
                                        )
                                    {
                                        let rule_path = dir.join(pf).display().to_string();
                                        if !written_files.contains(&rule_path) {
                                            written_files.push(rule_path);
                                        }
                                    }
                                } else {
                                    // No custom rules — strip any legacy inline rules block from CLAUDE.md.
                                    if let Ok(path) = clean_project_file_rules_section(&dir, pf) {
                                        if let Some(p) = path {
                                            written_files.push(p);
                                        }
                                    }
                                }
                            } else {
                                // Default: inject all rules inline into the project file.
                                if let Ok(true) =
                                    crate::core::inject_rules_into_project_file_with_custom(
                                        &project.directory,
                                        pf,
                                        rules,
                                        &custom_contents,
                                    )
                                {
                                    let rule_path = dir.join(pf).display().to_string();
                                    if !written_files.contains(&rule_path) {
                                        written_files.push(rule_path);
                                    }
                                }
                            }
                        } else if agent_id == "claude" && opts.claude_rules_in_dot_claude {
                            // No rules configured — still clean up any stale managed files.
                            match crate::core::sync_rules_to_dot_claude_rules(
                                &project.directory,
                                rules,
                            ) {
                                Ok(touched) => written_files.extend(touched),
                                Err(e) => eprintln!("Failed to clean .claude/rules/: {}", e),
                            }
                        }
                    } else if has_any_rules {
                        // No global rules key but there are custom rules — inject inline.
                        if let Ok(true) = crate::core::inject_rules_into_project_file_with_custom(
                            &project.directory,
                            pf,
                            &[],
                            &custom_contents,
                        ) {
                            let rule_path = dir.join(pf).display().to_string();
                            if !written_files.contains(&rule_path) {
                                written_files.push(rule_path);
                            }
                        }
                    } else if agent_id == "claude" && opts.claude_rules_in_dot_claude {
                        // No rules key at all — clean up any stale managed files.
                        match crate::core::sync_rules_to_dot_claude_rules(&project.directory, &[]) {
                            Ok(touched) => written_files.extend(touched),
                            Err(e) => eprintln!("Failed to clean .claude/rules/: {}", e),
                        }
                    }
                }
            }
            None => {
                eprintln!("Unknown agent '{}', skipping", agent_id);
            }
        }
    }

    // ── Step 3: Unified mode — replicate content across all agent files ───
    //
    // Before blindly overwriting, check whether any file was modified
    // externally by comparing its current on-disk hash against the hash
    // Automatic recorded the last time it wrote the file.  If a file was
    // externally modified, skip Step 3 entirely so drift detection can
    // surface the conflict for the user to resolve.
    if project.instruction_mode == "unified" && cleaned_project_files.len() > 1 {
        // Collect user content from each existing file.
        let mut file_contents: Vec<(String, String)> = Vec::new();
        for f in &cleaned_project_files {
            let path = dir.join(f);
            if path.exists() {
                if let Ok(raw) = fs::read_to_string(&path) {
                    let user_content = crate::core::strip_rules_section_pub(
                        &crate::core::strip_managed_section_pub(&raw),
                    );
                    file_contents.push((f.clone(), user_content));
                }
            }
        }

        // Check if any existing file was externally modified (hash mismatch).
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

        // Also check if existing files have inconsistent user content — this
        // means one was edited externally even if we have no stored hash yet
        // (first sync after adding this feature).
        let all_consistent = if file_contents.len() > 1 {
            let first_content = &file_contents[0].1;
            file_contents
                .iter()
                .all(|(_, c)| c.trim() == first_content.trim())
        } else {
            true
        };

        if any_externally_modified || !all_consistent {
            // An instruction file was modified outside Automatic.
            // Do NOT overwrite — leave the files as-is so drift detection
            // can surface the conflict and the user can choose what to keep.
            eprintln!(
                "[automatic] Unified replication skipped: instruction file(s) were modified externally. \
                 Drift detection will surface the conflict."
            );
        } else {
            // All files are consistent (or only one exists).  Safe to replicate.
            let source_file = cleaned_project_files
                .iter()
                .find(|f| dir.join(f).exists())
                .cloned();

            if let Some(source) = source_file {
                let raw = fs::read_to_string(dir.join(&source)).unwrap_or_default();
                let user_content = crate::core::strip_rules_section_pub(
                    &crate::core::strip_managed_section_pub(&raw),
                );

                let rules = project
                    .file_rules
                    .get("_unified")
                    .cloned()
                    .unwrap_or_default();

                let custom_contents: Vec<String> = project
                    .custom_rules
                    .iter()
                    .filter(|r| !r.content.trim().is_empty())
                    .map(|r| r.content.clone())
                    .collect();

                for target in &cleaned_project_files {
                    if *target == source {
                        continue;
                    }
                    if let Ok(()) = crate::core::save_project_file_with_rules_and_custom(
                        &project.directory,
                        target,
                        &user_content,
                        &rules,
                        &custom_contents,
                    ) {
                        let p = dir.join(target).display().to_string();
                        if !written_files.contains(&p) {
                            written_files.push(p);
                        }
                    }
                }
            }
        }
    }

    // ── Step 4: Record instruction file hashes and snapshots ────────────
    //
    // After all writes are complete, snapshot the current on-disk content of
    // every instruction file so drift detection can compare against it later.
    let project_name = project.name.clone();
    crate::core::record_instruction_hashes(&project_name, project);

    // Save a user-content snapshot for every instruction file that was
    // touched during this sync so the conflict diff has something to compare
    // against.  We read from disk (user section only) at this point because
    // the individual write paths (rules injection, unified replication) don't
    // all go through save_project_file_for_project.
    let mut snap_seen: HashSet<String> = HashSet::new();
    for agent_id in &project.agents {
        if let Some(a) = agent::from_id(agent_id) {
            if !a.capabilities().instructions {
                continue;
            }
            let filename = a.project_file_name().to_string();
            if snap_seen.contains(&filename) {
                continue;
            }
            snap_seen.insert(filename.clone());

            if let Ok(user_content) = crate::core::read_project_file(&project.directory, &filename)
            {
                let _ = crate::core::save_instruction_snapshot(
                    &project.directory,
                    &filename,
                    &user_content,
                );
            }
        }
    }

    Ok(written_files)
}
