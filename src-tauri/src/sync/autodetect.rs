use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use crate::agent;
use crate::core::Project;

use super::helpers::add_unique;

/// Discover dependencies already present in a project's directory and persist
/// any new findings into the project + global registries.
/// Pure read-only autodetection. Scans the project directory and returns an
/// enriched [`Project`] with any newly discovered agents, skills, and MCP
/// server names. Does not write anything to disk — callers that need to
/// persist discoveries (e.g. `sync_project`) must do so themselves.
pub fn autodetect_project_dependencies(project: &Project) -> Result<Project, String> {
    let (updated, _) = autodetect_inner(project)?;
    Ok(updated)
}

/// Inner autodetection that returns both the enriched project and the
/// discovered MCP server configs (name -> pretty-printed JSON string) so that
/// `sync_project` can persist them without a second filesystem scan.
pub(super) fn autodetect_inner(
    project: &Project,
) -> Result<(Project, Vec<(String, String)>), String> {
    if project.directory.is_empty() {
        return Ok((project.clone(), vec![]));
    }

    let dir = PathBuf::from(&project.directory);
    if !dir.exists() {
        return Ok((project.clone(), vec![]));
    }

    let mut updated_project = project.clone();
    let mut discovered_servers: Vec<(String, String)> = Vec::new();

    // Detect which agents are present by asking each agent to check
    for a in agent::all() {
        if a.detect_in(&dir) {
            add_unique(&mut updated_project.agents, a.id());
        }
    }

    // Discover skills from all known skill directories
    // (includes agent-specific dirs + the generic `skills/` dir)
    let global_skill_names: HashSet<String> = crate::core::list_skill_names()
        .unwrap_or_default()
        .into_iter()
        .collect();

    let mut skill_dirs: Vec<PathBuf> = Vec::new();
    for a in agent::all() {
        skill_dirs.extend(a.skill_dirs(&dir));
    }
    skill_dirs.push(dir.join("skills")); // generic fallback

    for skill_base_dir in &skill_dirs {
        if !skill_base_dir.exists() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(skill_base_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        let skill_file = path.join("SKILL.md");
                        if skill_file.exists() && crate::core::is_valid_name(name) {
                            if global_skill_names.contains(name) {
                                // Skill exists in the global registry — track
                                // it as a normal (global) project skill.
                                add_unique(&mut updated_project.skills, name);
                            } else if !updated_project.skills.contains(&name.to_string()) {
                                // Skill only exists locally in this project —
                                // track it separately without importing.
                                add_unique(&mut updated_project.local_skills, name);
                            }
                        }
                    }
                }
            }
        }
    }

    // Discover MCP servers by asking each agent to scan its config files.
    // Configs are collected here and returned to the caller — we do not write
    // to the global MCP registry from this read-only function.
    for a in agent::all() {
        let servers = a.discover_mcp_servers(&dir);
        for (name, config) in servers {
            if let Ok(config_str) = serde_json::to_string_pretty(&config) {
                if !updated_project.mcp_servers.contains(&name) {
                    updated_project.mcp_servers.push(name.clone());
                }
                discovered_servers.push((name, config_str));
            }
        }
    }

    Ok((updated_project, discovered_servers))
}
