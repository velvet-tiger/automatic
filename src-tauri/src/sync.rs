use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use crate::agent;
use crate::core::Project;

// ── Drift types ───────────────────────────────────────────────────────────────

/// A single file that is out of sync, with a human-readable reason.
#[derive(Debug, Serialize, Deserialize)]
pub struct DriftedFile {
    /// Relative path from the project directory (e.g. `.mcp.json`).
    pub path: String,
    /// Short description of why it's drifted.
    pub reason: String,
}

/// Per-agent drift report returned by [`check_project_drift`].
#[derive(Debug, Serialize, Deserialize)]
pub struct AgentDrift {
    pub agent_id: String,
    pub agent_label: String,
    pub files: Vec<DriftedFile>,
}

/// Full drift report for a project.
#[derive(Debug, Serialize, Deserialize)]
pub struct DriftReport {
    /// `true` if any agent has drift.
    pub drifted: bool,
    /// One entry per agent that has at least one drifted file.
    pub agents: Vec<AgentDrift>,
}

// ── Public API ───────────────────────────────────────────────────────────────

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
/// The `automatic` / `nexus` server entries are filtered out automatically by
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

/// Check whether the on-disk agent configs match what Nexus would generate.
/// Returns a [`DriftReport`] describing which agents and files have drifted.
/// This is a read-only operation — nothing is written.
pub fn check_project_drift(project: &Project) -> Result<DriftReport, String> {
    if project.directory.is_empty() || project.agents.is_empty() {
        return Ok(DriftReport {
            drifted: false,
            agents: vec![],
        });
    }

    let dir = PathBuf::from(&project.directory);
    if !dir.exists() {
        return Ok(DriftReport {
            drifted: false,
            agents: vec![],
        });
    }

    // Build the MCP server map that sync would use
    let mcp_config = load_mcp_server_configs()?;
    let mut selected_servers = Map::new();
    let automatic_binary = find_automatic_binary();
    selected_servers.insert(
        "automatic".to_string(),
        json!({
            "command": automatic_binary,
            "args": ["mcp-serve"],
            "env": { "AUTOMATIC_PROJECT": project.name }
        }),
    );
    for server_name in &project.mcp_servers {
        if let Some(server_config) = mcp_config.get(server_name) {
            selected_servers.insert(server_name.clone(), server_config.clone());
        }
    }

    let skill_contents = load_skill_contents(&project.skills);

    let mut agent_drifts: Vec<AgentDrift> = Vec::new();

    for agent_id in &project.agents {
        if let Some(agent_instance) = agent::from_id(agent_id) {
            let mut files: Vec<DriftedFile> = Vec::new();

            collect_mcp_drift(agent_instance, &dir, &selected_servers, &mut files);
            collect_skills_drift(
                agent_instance,
                &dir,
                &skill_contents,
                &project.skills,
                &project.local_skills,
                &mut files,
            );

            if !files.is_empty() {
                agent_drifts.push(AgentDrift {
                    agent_id: agent_id.clone(),
                    agent_label: agent_instance.label().to_string(),
                    files,
                });
            }
        }
    }

    let drifted = !agent_drifts.is_empty();
    Ok(DriftReport {
        drifted,
        agents: agent_drifts,
    })
}

/// Collect MCP config drift entries for one agent into `out`.
fn collect_mcp_drift(
    agent_instance: &dyn agent::Agent,
    dir: &PathBuf,
    servers: &Map<String, Value>,
    out: &mut Vec<DriftedFile>,
) {
    // Write the expected config to a temp dir, then compare file-by-file.
    // Each agent has its own format logic so we delegate rather than replicating it.
    let tmp = match tempfile::tempdir() {
        Ok(t) => t,
        Err(_) => return,
    };

    if agent_instance
        .write_mcp_config(tmp.path(), servers)
        .is_err()
    {
        return;
    }

    let tmp_entries: Vec<_> = match fs::read_dir(tmp.path()) {
        Ok(e) => e.flatten().collect(),
        Err(_) => return,
    };

    for entry in &tmp_entries {
        let tmp_path = entry.path();
        if !tmp_path.is_file() {
            continue;
        }
        let filename = match tmp_path.file_name().and_then(|n| n.to_str()) {
            Some(f) => f.to_string(),
            None => continue,
        };
        let disk_path = dir.join(&filename);

        if !disk_path.exists() {
            out.push(DriftedFile {
                path: filename,
                reason: "missing".into(),
            });
            continue;
        }

        let expected = match fs::read_to_string(&tmp_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let actual = match fs::read_to_string(&disk_path) {
            Ok(c) => c,
            Err(_) => {
                out.push(DriftedFile {
                    path: filename,
                    reason: "unreadable".into(),
                });
                continue;
            }
        };
        if expected != actual {
            out.push(DriftedFile {
                path: filename,
                reason: "modified".into(),
            });
        }
    }
}

/// Collect skill drift entries for one agent into `out`.
fn collect_skills_drift(
    agent_instance: &dyn agent::Agent,
    dir: &PathBuf,
    skill_contents: &[(String, String)],
    selected_names: &[String],
    local_skill_names: &[String],
    out: &mut Vec<DriftedFile>,
) {
    let tmp = match tempfile::tempdir() {
        Ok(t) => t,
        Err(_) => return,
    };

    if agent_instance
        .sync_skills(
            tmp.path(),
            skill_contents,
            selected_names,
            local_skill_names,
        )
        .is_err()
    {
        return;
    }

    for skill_dir in agent_instance.skill_dirs(dir) {
        let relative = match skill_dir.strip_prefix(dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let tmp_skill_dir = tmp.path().join(relative);

        // Check each skill that *should* be present
        if tmp_skill_dir.exists() {
            if let Ok(entries) = fs::read_dir(&tmp_skill_dir) {
                for entry in entries.flatten() {
                    let tmp_skill_path = entry.path();
                    if !tmp_skill_path.is_dir() {
                        continue;
                    }
                    let skill_name = match tmp_skill_path.file_name().and_then(|n| n.to_str()) {
                        Some(n) => n.to_string(),
                        None => continue,
                    };
                    let tmp_file = tmp_skill_path.join("SKILL.md");
                    let disk_file = skill_dir.join(&skill_name).join("SKILL.md");
                    let rel_path = format!("{}/{}/SKILL.md", relative.display(), skill_name);

                    if !disk_file.exists() {
                        out.push(DriftedFile {
                            path: rel_path,
                            reason: "missing".into(),
                        });
                        continue;
                    }

                    let expected = match fs::read_to_string(&tmp_file) {
                        Ok(c) => c,
                        Err(_) => continue,
                    };
                    let actual = match fs::read_to_string(&disk_file) {
                        Ok(c) => c,
                        Err(_) => {
                            out.push(DriftedFile {
                                path: rel_path,
                                reason: "unreadable".into(),
                            });
                            continue;
                        }
                    };
                    if expected != actual {
                        out.push(DriftedFile {
                            path: rel_path,
                            reason: "modified".into(),
                        });
                    }
                }
            }
        }

        // Check for stale skill dirs that should have been removed
        if skill_dir.exists() {
            let selected: HashSet<&str> = selected_names.iter().map(|s| s.as_str()).collect();
            let preserved: HashSet<&str> = local_skill_names.iter().map(|s| s.as_str()).collect();

            if let Ok(disk_entries) = fs::read_dir(&skill_dir) {
                for disk_entry in disk_entries.flatten() {
                    let disk_path = disk_entry.path();
                    if !disk_path.is_dir() {
                        continue;
                    }
                    if let Some(name) = disk_path.file_name().and_then(|n| n.to_str()) {
                        if crate::core::is_valid_name(name)
                            && !selected.contains(name)
                            && !preserved.contains(name)
                        {
                            out.push(DriftedFile {
                                path: format!("{}/{}", relative.display(), name),
                                reason: "stale".into(),
                            });
                        }
                    }
                }
            }
        }
    }
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

/// Inner autodetection that returns both the enriched project and the
/// discovered MCP server configs (name → pretty-printed JSON string) so that
/// `sync_project` can persist them without a second filesystem scan.
fn autodetect_inner(project: &Project) -> Result<(Project, Vec<(String, String)>), String> {
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

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Load MCP server configs from the Nexus registry (~/.automatic/mcp_servers/).
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

/// Find the Automatic binary path.
fn find_automatic_binary() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "automatic".to_string())
}

/// Strip any legacy `<!-- automatic:skills:start -->…<!-- automatic:skills:end -->`
/// managed section from a project file.  Returns the path if the file was
/// modified, or None if no cleanup was needed.
fn clean_project_file(dir: &PathBuf, filename: &str) -> Result<Option<String>, String> {
    let path = dir.join(filename);
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;

    let start_marker = "<!-- automatic:skills:start -->";
    let end_marker = "<!-- automatic:skills:end -->";

    if let (Some(start), Some(end)) = (content.find(start_marker), content.find(end_marker)) {
        let before = &content[..start];
        let after = &content[end + end_marker.len()..];
        let cleaned = format!(
            "{}{}",
            before.trim_end(),
            if after.trim().is_empty() {
                "\n".to_string()
            } else {
                format!("\n\n{}", after.trim_start())
            }
        );
        fs::write(&path, cleaned).map_err(|e| e.to_string())?;
        Ok(Some(path.display().to_string()))
    } else {
        Ok(None)
    }
}

// ── Local-skill operations ───────────────────────────────────────────────────

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

    if written.is_empty() {
        return Err("No agent skill directories found for this project".into());
    }

    Ok(written)
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

fn add_unique(items: &mut Vec<String>, value: &str) -> bool {
    if items.iter().any(|v| v == value) {
        false
    } else {
        items.push(value.to_string());
        true
    }
}
