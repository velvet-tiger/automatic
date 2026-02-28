use serde_json::{json, Map};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use crate::agent;
use crate::core::Project;

use super::autodetect::autodetect_inner;
use super::helpers::{
    clean_project_file, find_automatic_binary, load_mcp_server_configs, load_skill_contents,
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

    let (updated_project, discovered_servers) = autodetect_inner(project)?;

    // Persist newly discovered MCP server configs into the global registry.
    // This only happens during an explicit sync, not during a read-only load.
    for (name, config_str) in discovered_servers {
        let _ = crate::core::save_mcp_server_config(&name, &config_str);
    }

    sync_project_without_autodetect(&updated_project)
}

/// Sync a project's configuration to its directory without re-running
/// dependency autodetection. Useful when reacting to registry changes
/// (e.g. deleting a skill/server) to avoid re-importing stale local files.
pub fn sync_project_without_autodetect(project: &Project) -> Result<Vec<String>, String> {
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

    // Read MCP server configs from the Automatic registry
    let mcp_config = load_mcp_server_configs()?;

    // Build the set of MCP servers this project uses (+ always include Automatic)
    let mut selected_servers = Map::new();

    // Always include Automatic MCP server
    let automatic_binary = find_automatic_binary();
    selected_servers.insert(
        "automatic".to_string(),
        json!({
            "command": automatic_binary,
            "args": ["mcp-serve"],
            "env": {
                "AUTOMATIC_PROJECT": project.name
            }
        }),
    );

    // Add project-selected MCP servers from the Automatic registry
    for server_name in &project.mcp_servers {
        if let Some(server_config) = mcp_config.get(server_name) {
            selected_servers.insert(server_name.clone(), server_config.clone());
        }
    }

    // Read all skill contents from the global skill registry
    let skill_contents = load_skill_contents(&project.skills);

    let mut written_files = Vec::new();

    // ── Step 1: Copy skills into the project's canonical .agents/skills/ ──
    //
    // This is the project-local hub.  Full directories are copied from the
    // global registry (~/.agents/skills/) so companion files are included.
    let project_skills_dir = dir.join(".agents").join("skills");
    agent::copy_skills_to_project(
        &project_skills_dir,
        &skill_contents,
        &project.skills,
        &project.local_skills,
        &mut written_files,
    )?;

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
                        &project.skills,
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

                // Strip legacy managed sections from project files (once per filename)
                let pf = agent_instance.project_file_name();
                if !cleaned_project_files.contains(pf) {
                    cleaned_project_files.insert(pf.to_string());
                    if let Ok(path) = clean_project_file(&dir, pf) {
                        if let Some(p) = path {
                            written_files.push(p);
                        }
                    }
                    // Re-inject rules for this project file if configured.
                    // In unified mode, rules are stored under the "_unified" key.
                    let rules = if project.instruction_mode == "unified" {
                        project.file_rules.get("_unified")
                    } else {
                        project.file_rules.get(pf)
                    };
                    if let Some(rules) = rules {
                        if !rules.is_empty() {
                            if let Ok(true) = crate::core::inject_rules_into_project_file(
                                &project.directory,
                                pf,
                                rules,
                            ) {
                                let rule_path = dir.join(pf).display().to_string();
                                if !written_files.contains(&rule_path) {
                                    written_files.push(rule_path);
                                }
                            }
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
    if project.instruction_mode == "unified" && cleaned_project_files.len() > 1 {
        // Find the first existing file to use as the source of truth
        let source_file = cleaned_project_files
            .iter()
            .find(|f| dir.join(f).exists())
            .cloned();

        if let Some(source) = source_file {
            let raw = fs::read_to_string(dir.join(&source)).unwrap_or_default();
            let user_content =
                crate::core::strip_rules_section_pub(&crate::core::strip_managed_section_pub(&raw));

            let rules = project
                .file_rules
                .get("_unified")
                .cloned()
                .unwrap_or_default();

            for target in &cleaned_project_files {
                if *target == source {
                    continue;
                }
                if let Ok(()) = crate::core::save_project_file_with_rules(
                    &project.directory,
                    target,
                    &user_content,
                    &rules,
                ) {
                    let p = dir.join(target).display().to_string();
                    if !written_files.contains(&p) {
                        written_files.push(p);
                    }
                }
            }
        }
    }

    Ok(written_files)
}
