use serde_json::{json, Map, Value};
use std::fs;
use std::path::PathBuf;

use crate::agent;
use crate::core::Project;

// ── Public API ───────────────────────────────────────────────────────────────

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

    let updated_project = autodetect_project_dependencies(project)?;

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

    // Read MCP server configs from the Nexus registry
    let mcp_config = load_mcp_server_configs()?;

    // Build the set of MCP servers this project uses (+ always include Nexus)
    let mut selected_servers = Map::new();

    // Always include Nexus MCP server
    let nexus_binary = find_nexus_binary();
    selected_servers.insert(
        "nexus".to_string(),
        json!({
            "command": nexus_binary,
            "args": ["mcp-serve"],
            "env": {
                "NEXUS_PROJECT": project.name
            }
        }),
    );

    // Add project-selected MCP servers from the Nexus registry
    for server_name in &project.mcp_servers {
        if let Some(server_config) = mcp_config.get(server_name) {
            selected_servers.insert(server_name.clone(), server_config.clone());
        }
    }

    // Read all skill contents from the global skill registry
    let skill_contents = load_skill_contents(&project.skills);

    let mut written_files = Vec::new();

    // Resolve each agent string id to a trait object and delegate
    for agent_id in &project.agents {
        match agent::from_id(agent_id) {
            Some(a) => {
                let skill_files = a.sync_skills(&dir, &skill_contents, &project.skills)?;
                written_files.extend(skill_files);

                let path = a.write_mcp_config(&dir, &selected_servers)?;
                written_files.push(path);
            }
            None => {
                eprintln!("Unknown agent '{}', skipping", agent_id);
            }
        }
    }

    Ok(written_files)
}

/// Discover dependencies already present in a project's directory and persist
/// any new findings into the project + global registries.
pub fn autodetect_project_dependencies(project: &Project) -> Result<Project, String> {
    if project.directory.is_empty() {
        return Ok(project.clone());
    }

    let dir = PathBuf::from(&project.directory);
    if !dir.exists() {
        return Ok(project.clone());
    }

    let mut updated_project = project.clone();
    let mut modified = false;

    // Detect which agents are present by asking each agent to check
    for a in agent::all() {
        if a.detect_in(&dir) {
            modified |= add_unique(&mut updated_project.agents, a.id());
        }
    }

    // Discover skills from all known skill directories
    // (includes agent-specific dirs + the generic `skills/` dir)
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
                            if let Ok(content) = fs::read_to_string(&skill_file) {
                                let _ = crate::core::save_skill(name, &content);
                                if !updated_project.skills.contains(&name.to_string()) {
                                    updated_project.skills.push(name.to_string());
                                    modified = true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Discover MCP servers by asking each agent to scan its config files
    for a in agent::all() {
        let servers = a.discover_mcp_servers(&dir);
        for (name, config) in servers {
            if let Ok(config_str) = serde_json::to_string_pretty(&config) {
                let _ = crate::core::save_mcp_server_config(&name, &config_str);
                if !updated_project.mcp_servers.contains(&name) {
                    updated_project.mcp_servers.push(name.clone());
                    modified = true;
                }
            }
        }
    }

    if modified {
        if let Ok(proj_str) = serde_json::to_string(&updated_project) {
            let _ = crate::core::save_project(&updated_project.name, &proj_str);
        }
    }

    Ok(updated_project)
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Load MCP server configs from the Nexus registry (~/.nexus/mcp_servers/).
fn load_mcp_server_configs() -> Result<Map<String, Value>, String> {
    let names = crate::core::list_mcp_server_configs()?;
    let mut servers = Map::new();

    for name in names {
        match crate::core::read_mcp_server_config(&name) {
            Ok(raw) => {
                if let Ok(config) = serde_json::from_str::<Value>(&raw) {
                    servers.insert(name, config);
                }
            }
            Err(_) => continue,
        }
    }

    Ok(servers)
}

/// Read all skill contents from the global registry for the given names.
fn load_skill_contents(skill_names: &[String]) -> Vec<(String, String)> {
    let mut contents = Vec::new();
    for name in skill_names {
        match crate::core::read_skill(name) {
            Ok(content) if !content.is_empty() => {
                contents.push((name.clone(), content));
            }
            _ => {}
        }
    }
    contents
}

/// Find the Nexus binary path.
fn find_nexus_binary() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "nexus".to_string())
}

fn add_unique(items: &mut Vec<String>, value: &str) -> bool {
    if items.iter().any(|v| v == value) {
        false
    } else {
        items.push(value.to_string());
        true
    }
}
