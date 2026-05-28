use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use crate::agent;
use crate::core::{Project, ProjectMode};

use super::helpers::{
    build_selected_servers, build_skill_contents, extract_agent_machine_name,
    load_mcp_server_configs,
};

// ── Problems types ────────────────────────────────────────────────────────────

/// The category of a project problem, for UI filtering and display.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectProblemKind {
    /// An MCP server defined in the project's local config also exists at the
    /// user (global) scope for the same agent.  Claude Code will use the
    /// user-scoped entry and ignore the project-local one.
    McpUserScopeConflict,
}

/// A single actionable problem in a project's configuration.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProjectProblem {
    /// Machine-readable category of the problem.
    pub kind: ProjectProblemKind,
    /// Human-readable title for the problem.
    pub title: String,
    /// Human-readable explanation of what is wrong and why it matters.
    pub description: String,
    /// Optional reference URL with more context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_url: Option<String>,
    /// The agent(s) affected (e.g. `"Claude Code"`).
    pub agents: Vec<String>,
    /// Specific resources involved (e.g. MCP server names).
    pub resources: Vec<String>,
}

/// Full problems report for a project.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectProblemsReport {
    /// `true` if there are any problems.
    pub has_problems: bool,
    /// The list of detected problems (may be empty).
    pub problems: Vec<ProjectProblem>,
}

/// Check for known configuration problems in a project.
///
/// Currently detects:
/// - MCP server names in the project's `.mcp.json` that also exist at the
///   Claude Code user scope (`~/.claude.json`).  Claude Code gives the
///   user-scoped entry priority and silently ignores the project-local one,
///   which can cause unexpected behaviour.
///
/// This is a read-only operation — nothing is written.
pub fn check_project_problems(project: &Project) -> Result<ProjectProblemsReport, String> {
    let mut problems: Vec<ProjectProblem> = Vec::new();

    if project.directory.is_empty() || project.agents.is_empty() {
        return Ok(ProjectProblemsReport {
            has_problems: false,
            problems,
        });
    }

    let dir = PathBuf::from(&project.directory);
    if !dir.exists() {
        return Ok(ProjectProblemsReport {
            has_problems: false,
            problems,
        });
    }

    // Check each agent that supports project-local MCP config.
    for agent_id in &project.agents {
        let Some(agent_instance) = agent::from_id(agent_id) else {
            continue;
        };

        // Read this agent's project-local MCP servers.
        let project_servers = agent_instance.discover_mcp_servers(&dir);
        if project_servers.is_empty() {
            continue;
        }

        // Read user-scoped (global) MCP servers for the same agent.
        let global_servers = agent_instance.discover_global_mcp_servers();
        if global_servers.is_empty() {
            continue;
        }

        // Find names that appear in both the project-local and user-scoped configs.
        let conflicts: Vec<String> = project_servers
            .keys()
            .filter(|name| global_servers.contains_key(*name))
            .cloned()
            .collect();

        if conflicts.is_empty() {
            continue;
        }

        let server_list = conflicts.join(", ");
        problems.push(ProjectProblem {
            kind: ProjectProblemKind::McpUserScopeConflict,
            title: format!(
                "MCP {} also configured at user scope",
                if conflicts.len() == 1 {
                    "server"
                } else {
                    "servers"
                }
            ),
            description: format!(
                "{agent_label} prioritises user-scoped MCP servers over project-local ones. \
                 The following {} defined in both .mcp.json and the user-scoped config \
                 (~/.claude.json) and will be overridden: {server_list}. \
                 Remove the user-scoped entry or rename the project-local entry to avoid \
                 unexpected behaviour.",
                if conflicts.len() == 1 {
                    "server is"
                } else {
                    "servers are"
                },
                agent_label = agent_instance.label(),
            ),
            reference_url: Some(
                "https://docs.anthropic.com/en/docs/claude-code/mcp#project-scope".to_string(),
            ),
            agents: vec![agent_instance.label().to_string()],
            resources: conflicts,
        });
    }

    let has_problems = !problems.is_empty();
    Ok(ProjectProblemsReport {
        has_problems,
        problems,
    })
}

// ── Drift types ───────────────────────────────────────────────────────────────

/// A single file that is out of sync, with a human-readable reason.
#[derive(Debug, Serialize, Deserialize)]
pub struct DriftedFile {
    /// Relative path from the project directory (e.g. `.mcp.json`).
    pub path: String,
    /// Short description of why it's drifted: "missing", "modified", "stale", "unreadable".
    pub reason: String,
    /// The content Automatic would generate. Present only for "modified" files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    /// The content currently on disk. Present only for "modified" files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
}

/// Per-agent drift report returned by [`check_project_drift`].
#[derive(Debug, Serialize, Deserialize)]
pub struct AgentDrift {
    pub agent_id: String,
    pub agent_label: String,
    pub files: Vec<DriftedFile>,
}

/// A conflict detected when an instruction file exists on disk with user content
/// that Automatic was not aware of (externally created or edited).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InstructionFileConflict {
    /// The instruction filename (e.g. `"AGENTS.md"`, `"CLAUDE.md"`).
    pub filename: String,
    /// Agent labels that use this file (e.g. `["Claude Code"]`).
    pub agent_labels: Vec<String>,
    /// The user-authored content currently on disk (stripped of Automatic managed sections).
    pub disk_content: String,
    /// The user-authored content Automatic has stored (empty string if never set through Automatic).
    pub automatic_content: String,
}

/// Full drift report for a project.
#[derive(Debug, Serialize, Deserialize)]
pub struct DriftReport {
    /// `true` if any agent has MCP/skill drift, or instruction files have conflicts.
    pub drifted: bool,
    /// One entry per agent that has at least one drifted file.
    pub agents: Vec<AgentDrift>,
    /// Instruction files that have content on disk which Automatic does not recognise.
    /// These require user action: keep existing or overwrite.
    #[serde(default)]
    pub instruction_conflicts: Vec<InstructionFileConflict>,
}

/// Check whether the on-disk agent configs match what Automatic would generate.
/// Returns a [`DriftReport`] describing which agents and files have drifted,
/// and any instruction files that have external content Automatic was not aware of.
/// This is a read-only operation — nothing is written.
pub fn check_project_drift(project: &Project) -> Result<DriftReport, String> {
    if project.directory.is_empty() || project.agents.is_empty() {
        return Ok(DriftReport {
            drifted: false,
            agents: vec![],
            instruction_conflicts: vec![],
        });
    }

    let dir = PathBuf::from(&project.directory);
    if !dir.exists() {
        return Ok(DriftReport {
            drifted: false,
            agents: vec![],
            instruction_conflicts: vec![],
        });
    }

    // In Silent mode all synced files live under .automatic/silent/ rather than
    // the project root.  Drift must be checked there — not against the project
    // root where Automatic has never written anything.
    let effective_dir = match project.mode {
        ProjectMode::Silent => dir.join(".automatic").join("silent"),
        ProjectMode::Normal => dir.clone(),
    };

    // Build the MCP server map using the same logic as the sync engine
    // (strips internal `_` fields, substitutes OAuth proxy configs).
    let mcp_config = load_mcp_server_configs()?;
    let enabled_mcp_servers = project.enabled_mcp_servers();
    let selected_servers = build_selected_servers(&project.name, &enabled_mcp_servers, &mcp_config);

    // Build skill contents with dedup: library-backed skills win over stale
    // custom_skills snapshots.
    let (skill_contents, custom_skill_names) = build_skill_contents(project);
    let all_selected_skill_names: Vec<String> = project
        .skills
        .iter()
        .chain(custom_skill_names.iter())
        .cloned()
        .collect();

    // Load workspace command contents from the global registry, mirroring how
    // the sync engine builds the same list in
    // `sync::engine::sync_project_without_autodetect`.
    let workspace_command_contents: Vec<(String, String)> = project
        .user_commands
        .iter()
        .filter_map(|name| {
            crate::core::read_user_command(name)
                .ok()
                .map(|content| (name.clone(), content))
        })
        .collect();
    let empty_custom_commands: Vec<crate::core::CustomCommand> = Vec::new();
    let custom_commands = project
        .custom_commands
        .as_deref()
        .unwrap_or(&empty_custom_commands);

    let mut agent_drifts: Vec<AgentDrift> = Vec::new();

    for agent_id in &project.agents {
        if let Some(agent_instance) = agent::from_id(agent_id) {
            let mut files: Vec<DriftedFile> = Vec::new();

            collect_mcp_drift(agent_instance, &effective_dir, &selected_servers, &mut files);
            collect_skills_drift(
                agent_instance,
                &effective_dir,
                &skill_contents,
                &all_selected_skill_names,
                &[],
                &mut files,
            );
            collect_agents_drift(
                agent_instance,
                &effective_dir,
                project.custom_agents.as_deref().unwrap_or(&[]),
                &project.user_agents,
                &mut files,
            );
            collect_commands_drift(
                agent_instance,
                &effective_dir,
                &workspace_command_contents,
                custom_commands,
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

    let instruction_conflicts = collect_instruction_file_conflicts(project, &effective_dir);

    let drifted = !agent_drifts.is_empty() || !instruction_conflicts.is_empty();
    Ok(DriftReport {
        drifted,
        agents: agent_drifts,
        instruction_conflicts,
    })
}

/// Public wrapper for use by the `commands` layer.
/// Automatically resolves the effective directory based on project mode.
pub fn collect_instruction_conflicts_pub(
    project: &Project,
    dir: &PathBuf,
) -> Vec<InstructionFileConflict> {
    let effective_dir = match project.mode {
        ProjectMode::Silent => dir.join(".automatic").join("silent"),
        ProjectMode::Normal => dir.clone(),
    };
    collect_instruction_file_conflicts(project, &effective_dir)
}

/// Detect instruction files that were modified outside Automatic.
///
/// Uses the stored snapshot of the user-authored content as the primary source
/// of truth. This avoids false conflicts when only Automatic-managed sections
/// (rules, groups, index blocks) changed on disk.
///
/// Also detects "orphaned" files: instruction files that exist on disk but
/// have no stored hash at all (e.g. the user created one manually before
/// Automatic ever synced the project).
///
/// In unified mode, additionally checks whether the instruction files are
/// inconsistent with each other (different user content), even if no
/// individual hash is stored yet.
fn collect_instruction_file_conflicts(
    project: &Project,
    dir: &PathBuf,
) -> Vec<InstructionFileConflict> {
    let mut conflicts: Vec<InstructionFileConflict> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // Collect all instruction filenames and their on-disk user content.
    let mut file_user_contents: Vec<(String, String)> = Vec::new();

    for agent_id in &project.agents {
        let agent_instance = match agent::from_id(agent_id) {
            Some(a) => a,
            None => continue,
        };

        if !agent_instance.capabilities().instructions {
            continue;
        }

        let filename = agent_instance.project_file_name().to_string();
        if seen.contains(&filename) {
            continue;
        }
        seen.insert(filename.clone());

        let file_path = dir.join(&filename);
        if !file_path.exists() {
            continue;
        }

        let raw_disk = match fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let disk_user_content = crate::core::strip_index_section(
            &crate::core::strip_groups_section(&crate::core::strip_rules_section_pub(
                &crate::core::strip_managed_section_pub(&raw_disk),
            )),
        );

        // Skip files with no user content.
        if disk_user_content.trim().is_empty() {
            continue;
        }

        let snapshot_content =
            crate::core::read_instruction_snapshot(project.directory.as_str(), &filename);
        let current_hash = crate::core::compute_content_hash(&raw_disk);
        let stored_hash = project.instruction_file_hashes.get(&filename);

        let is_externally_modified = match stored_hash {
            Some(stored) => match snapshot_content.as_ref() {
                Some(snapshot) => disk_user_content.trim() != snapshot.trim(),
                None => &current_hash != stored,
            },
            // No stored hash means Automatic may never have recorded writing this
            // file. If we do have a snapshot, compare against that; otherwise
            // any non-empty user content is treated as external.
            None => match snapshot_content.as_ref() {
                Some(snapshot) => disk_user_content.trim() != snapshot.trim(),
                None => true,
            },
        };

        if is_externally_modified {
            file_user_contents.push((filename, disk_user_content));
        }
    }

    // In unified mode, also check for inconsistency across files, even if
    // individual hashes match.  If files have different user content,
    // something was modified outside of Automatic's unified replication.
    if project.instruction_mode == "unified" && file_user_contents.is_empty() {
        let mut all_contents: Vec<(String, String)> = Vec::new();
        let mut seen2: HashSet<String> = HashSet::new();

        for agent_id in &project.agents {
            let agent_instance = match agent::from_id(agent_id) {
                Some(a) => a,
                None => continue,
            };
            if !agent_instance.capabilities().instructions {
                continue;
            }
            let filename = agent_instance.project_file_name().to_string();
            if seen2.contains(&filename) {
                continue;
            }
            seen2.insert(filename.clone());

            let file_path = dir.join(&filename);
            if !file_path.exists() {
                continue;
            }
            if let Ok(raw) = fs::read_to_string(&file_path) {
                let user_content = crate::core::strip_index_section(
                    &crate::core::strip_groups_section(&crate::core::strip_rules_section_pub(
                        &crate::core::strip_managed_section_pub(&raw),
                    )),
                );
                all_contents.push((filename, user_content));
            }
        }

        // If there are 2+ files with different user content, they're inconsistent.
        if all_contents.len() > 1 {
            let first_content = &all_contents[0].1;
            let inconsistent: Vec<_> = all_contents
                .iter()
                .filter(|(_, c)| c.trim() != first_content.trim())
                .collect();

            if !inconsistent.is_empty() {
                // Flag all files as conflicted so the user can choose.
                file_user_contents = all_contents;
            }
        }
    }

    // Build conflict entries for each externally-modified file.
    for (filename, disk_user_content) in &file_user_contents {
        let agent_labels: Vec<String> = project
            .agents
            .iter()
            .filter_map(|aid| {
                agent::from_id(aid).and_then(|a| {
                    if a.project_file_name() == *filename {
                        Some(a.label().to_string())
                    } else {
                        None
                    }
                })
            })
            .collect();

        // The "Automatic content" is what Automatic last wrote to this file
        // (user-content portion only), read from the snapshot it saves on
        // every write.  If no snapshot exists Automatic has never written the
        // file, so we leave it empty — the UI will show a plain preview.
        let automatic_content =
            crate::core::read_instruction_snapshot(project.directory.as_str(), filename)
                .unwrap_or_default();

        conflicts.push(InstructionFileConflict {
            filename: filename.clone(),
            agent_labels,
            disk_content: disk_user_content.clone(),
            automatic_content,
        });
    }

    conflicts
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
                expected: None,
                actual: None,
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
                    expected: None,
                    actual: None,
                });
                continue;
            }
        };
        if expected != actual {
            out.push(DriftedFile {
                path: filename,
                reason: "modified".into(),
                expected: Some(expected),
                actual: Some(actual),
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
                    // Local skills are user-managed — Automatic does not own them
                    // and must never flag them as drifted.
                    if local_skill_names.contains(&skill_name) {
                        continue;
                    }
                    let tmp_file = tmp_skill_path.join("SKILL.md");
                    let disk_file = skill_dir.join(&skill_name).join("SKILL.md");
                    let rel_path = format!("{}/{}/SKILL.md", relative.display(), skill_name);

                    // The bundled `automatic` skill documents the Automatic MCP
                    // service itself.  Its content is fully owned by the app and
                    // ships with each release — the user has no agency over it,
                    // so reporting drift would only generate noise that resolves
                    // on the next sync.  Auto-heal it silently and skip
                    // reporting.
                    let is_managed_skill = skill_name == crate::core::AUTOMATIC_SKILL_NAME;

                    if !disk_file.exists() {
                        if is_managed_skill {
                            if let Ok(content) = fs::read_to_string(&tmp_file) {
                                if let Some(parent) = disk_file.parent() {
                                    let _ = fs::create_dir_all(parent);
                                }
                                let _ = fs::write(&disk_file, content);
                            }
                            continue;
                        }
                        out.push(DriftedFile {
                            path: rel_path,
                            reason: "missing".into(),
                            expected: None,
                            actual: None,
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
                                expected: None,
                                actual: None,
                            });
                            continue;
                        }
                    };
                    if expected != actual {
                        if is_managed_skill {
                            let _ = fs::write(&disk_file, &expected);
                            continue;
                        }
                        out.push(DriftedFile {
                            path: rel_path,
                            reason: "modified".into(),
                            expected: Some(expected),
                            actual: Some(actual),
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
                            // Read the on-disk SKILL.md so the UI can preview
                            // what the user is adopting or removing.
                            let skill_md = disk_path.join("SKILL.md");
                            let actual = fs::read_to_string(&skill_md).ok();

                            out.push(DriftedFile {
                                path: format!("{}/{}", relative.display(), name),
                                reason: "stale".into(),
                                expected: None,
                                actual,
                            });
                        }
                    }
                }
            }
        }
    }
}

/// Collect command drift entries for one agent into `out`.
///
/// Writes the expected on-disk command files (both the project-local hub at
/// `.agents/commands/` and the per-agent dir) into a tempdir using the same
/// helpers the sync engine uses, then compares each tempdir file with its
/// real counterpart in the project directory. Missing, modified, unreadable
/// and stale managed files are all reported.
fn collect_commands_drift(
    agent_instance: &dyn agent::Agent,
    dir: &PathBuf,
    workspace_command_contents: &[(String, String)],
    custom_commands: &[crate::core::CustomCommand],
    out: &mut Vec<DriftedFile>,
) {
    let agent_commands_dir = match agent_instance.commands_dir(dir) {
        Some(d) => d,
        None => return,
    };

    let relative = match agent_commands_dir.strip_prefix(dir) {
        Ok(r) => r.to_path_buf(),
        Err(_) => return,
    };

    let tmp = match tempfile::tempdir() {
        Ok(t) => t,
        Err(_) => return,
    };

    // Mirror the engine's two-step write: first the canonical hub at
    // `<tmp>/.agents/commands/`, then the per-agent directory (which either
    // symlinks back to the hub for `.md` agents, or copies the converted
    // content for non-`.md` agents).
    let tmp_hub = tmp.path().join(".agents").join("commands");
    if agent::copy_commands_to_project(&tmp_hub, workspace_command_contents, custom_commands)
        .is_err()
    {
        return;
    }

    let tmp_agent_dir = tmp.path().join(&relative);
    let sync_result = if agent_instance.commands_file_ext() == "md" {
        agent::symlink_commands_from_project(
            &tmp_agent_dir,
            &tmp_hub,
            workspace_command_contents,
            custom_commands,
            agent_instance,
        )
    } else {
        agent::sync_commands_to_dir(
            &tmp_agent_dir,
            workspace_command_contents,
            custom_commands,
            agent_instance,
        )
    };
    if sync_result.is_err() {
        return;
    }

    // Compare expected (tempdir) with actual (project dir).
    let mut expected_names: HashSet<String> = HashSet::new();
    if let Ok(tmp_entries) = fs::read_dir(&tmp_agent_dir) {
        for entry in tmp_entries.flatten() {
            let tmp_path = entry.path();
            let file_name = match tmp_path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            expected_names.insert(file_name.clone());

            // `fs::read_to_string` follows symlinks, so symlinked entries
            // resolve to the hub file we just wrote.
            let expected = match fs::read_to_string(&tmp_path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let disk_path = agent_commands_dir.join(&file_name);
            let rel_path = format!("{}/{}", relative.display(), file_name);

            if !disk_path.exists() {
                out.push(DriftedFile {
                    path: rel_path,
                    reason: "missing".into(),
                    expected: Some(expected),
                    actual: None,
                });
                continue;
            }

            match fs::read_to_string(&disk_path) {
                Ok(actual) => {
                    if actual != expected {
                        out.push(DriftedFile {
                            path: rel_path,
                            reason: "modified".into(),
                            expected: Some(expected),
                            actual: Some(actual),
                        });
                    }
                }
                Err(_) => {
                    out.push(DriftedFile {
                        path: rel_path,
                        reason: "unreadable".into(),
                        expected: Some(expected),
                        actual: None,
                    });
                }
            }
        }
    }

    // Stale: managed files on disk that aren't in the expected set.
    if agent_commands_dir.exists() {
        if let Ok(disk_entries) = fs::read_dir(&agent_commands_dir) {
            for disk_entry in disk_entries.flatten() {
                let disk_path = disk_entry.path();
                let file_name = match disk_path.file_name().and_then(|n| n.to_str()) {
                    Some(n) => n.to_string(),
                    None => continue,
                };
                if expected_names.contains(&file_name) {
                    continue;
                }
                if !agent::is_managed_command_file(&disk_path) {
                    continue;
                }
                let actual = fs::read_to_string(&disk_path).ok();
                out.push(DriftedFile {
                    path: format!("{}/{}", relative.display(), file_name),
                    reason: "stale".into(),
                    expected: None,
                    actual,
                });
            }
        }
    }
}

/// Collect agent drift entries for one agent into `out`.
/// Handles both custom_agents (inline, project-scoped) and user_agents
/// (workspace-scoped from ~/.automatic/agents/).
fn collect_agents_drift(
    agent_instance: &dyn agent::Agent,
    dir: &PathBuf,
    custom_agents: &[crate::core::CustomAgent],
    user_agent_names: &[String],
    out: &mut Vec<DriftedFile>,
) {
    let agents_dir = match agent_instance.agents_dir(dir) {
        Some(d) => d,
        None => return,
    };

    let ext = agent_instance.agents_file_ext();

    // Build combined set of expected agent machine names
    let mut expected_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Add custom agent names
    for agent in custom_agents {
        if let Some(machine_name) = extract_agent_machine_name(&agent.content) {
            expected_names.insert(machine_name);
        } else {
            expected_names.insert(agent.name.to_lowercase().replace(' ', "-"));
        }
    }

    // Add user agent names (from workspace registry)
    for name in user_agent_names {
        if let Ok(content) = crate::core::read_subagent(name) {
            if let Ok(agent) = serde_json::from_str::<crate::core::Subagent>(&content) {
                if let Some(machine_name) = extract_agent_machine_name(&agent.content) {
                    expected_names.insert(machine_name);
                } else {
                    expected_names.insert(name.to_lowercase().replace(' ', "-"));
                }
            }
        }
    }

    // Check for missing/modified custom agent files
    for agent in custom_agents {
        let machine_name = extract_agent_machine_name(&agent.content)
            .unwrap_or_else(|| agent.name.to_lowercase().replace(' ', "-"));
        let converted_content = agent_instance.convert_agent_content(&agent.content, &machine_name);
        let agent_path = agents_dir.join(format!("{}.{}", machine_name, ext));

        if !agent_path.exists() {
            let relative = agent_path.strip_prefix(dir).unwrap_or(&agent_path);
            out.push(DriftedFile {
                path: relative.display().to_string(),
                reason: "missing".into(),
                expected: Some(converted_content),
                actual: None,
            });
        } else if let Ok(disk_content) = fs::read_to_string(&agent_path) {
            if disk_content != converted_content {
                let relative = agent_path.strip_prefix(dir).unwrap_or(&agent_path);
                out.push(DriftedFile {
                    path: relative.display().to_string(),
                    reason: "modified".into(),
                    expected: Some(converted_content),
                    actual: Some(disk_content),
                });
            }
        }
    }

    // Check for missing/modified user agent files
    for name in user_agent_names {
        if let Ok(content) = crate::core::read_subagent(name) {
            if let Ok(agent) = serde_json::from_str::<crate::core::Subagent>(&content) {
                let machine_name = extract_agent_machine_name(&agent.content)
                    .unwrap_or_else(|| name.to_lowercase().replace(' ', "-"));
                let converted_content =
                    agent_instance.convert_agent_content(&agent.content, &machine_name);
                let agent_path = agents_dir.join(format!("{}.{}", machine_name, ext));

                if !agent_path.exists() {
                    let relative = agent_path.strip_prefix(dir).unwrap_or(&agent_path);
                    out.push(DriftedFile {
                        path: relative.display().to_string(),
                        reason: "missing".into(),
                        expected: Some(converted_content),
                        actual: None,
                    });
                } else if let Ok(disk_content) = fs::read_to_string(&agent_path) {
                    if disk_content != converted_content {
                        let relative = agent_path.strip_prefix(dir).unwrap_or(&agent_path);
                        out.push(DriftedFile {
                            path: relative.display().to_string(),
                            reason: "modified".into(),
                            expected: Some(converted_content),
                            actual: Some(disk_content),
                        });
                    }
                }
            }
        }
    }

    // Check for stale agent files (in agents_dir but not in expected set)
    if agents_dir.exists() {
        if let Ok(entries) = fs::read_dir(&agents_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == ext) {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        if crate::core::is_valid_agent_machine_name(stem)
                            && !expected_names.contains(stem)
                        {
                            let relative = path.strip_prefix(dir).unwrap_or(&path);
                            let actual = fs::read_to_string(&path).ok();
                            out.push(DriftedFile {
                                path: relative.display().to_string(),
                                reason: "stale".into(),
                                expected: None,
                                actual,
                            });
                        }
                    }
                }
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, ClaudeCode};
    use serde_json::Map;
    use std::fs;
    use tempfile::tempdir;

    /// After `sync_skills` writes skills into a temp project dir, `collect_skills_drift`
    /// must report no drift for the same agent and the same skill list.
    ///
    /// Regression test for the "SKILL.md missing after sync" bug where drift
    /// detection reported files as missing immediately after a successful sync.
    #[test]
    fn no_drift_after_sync_skills() {
        let project_dir = tempdir().unwrap();
        let skill_contents: Vec<(String, String)> = vec![(
            "automatic".to_string(),
            "# Automatic skill content\n".to_string(),
        )];
        let selected_names = vec!["automatic".to_string()];
        let local_names: Vec<String> = vec![];

        // Sync skills to the project dir (same path as engine.rs Step 2 for Claude Code)
        ClaudeCode
            .sync_skills(
                project_dir.path(),
                &skill_contents,
                &selected_names,
                &local_names,
            )
            .expect("sync_skills should succeed");

        // Drift check: must report zero drifted files
        let mut files: Vec<DriftedFile> = Vec::new();
        collect_skills_drift(
            &ClaudeCode,
            &project_dir.path().to_path_buf(),
            &skill_contents,
            &selected_names,
            &local_names,
            &mut files,
        );

        assert!(
            files.is_empty(),
            "Expected no drift after sync, got: {:?}",
            files
                .iter()
                .map(|f| format!("{} ({})", f.path, f.reason))
                .collect::<Vec<_>>()
        );
    }

    /// After `write_mcp_config` writes the MCP config, `collect_mcp_drift` must
    /// report no drift for the same servers map.
    #[test]
    fn no_drift_after_write_mcp_config() {
        let project_dir = tempdir().unwrap();
        let mut servers = Map::new();
        servers.insert(
            "automatic".to_string(),
            serde_json::json!({
                "command": "/usr/local/bin/automatic",
                "args": ["mcp-serve"],
                "env": { "AUTOMATIC_PROJECT": "test-project" }
            }),
        );

        ClaudeCode
            .write_mcp_config(project_dir.path(), &servers)
            .expect("write_mcp_config should succeed");

        let mut files: Vec<DriftedFile> = Vec::new();
        collect_mcp_drift(
            &ClaudeCode,
            &project_dir.path().to_path_buf(),
            &servers,
            &mut files,
        );

        assert!(
            files.is_empty(),
            "Expected no MCP drift after write, got: {:?}",
            files
                .iter()
                .map(|f| format!("{} ({})", f.path, f.reason))
                .collect::<Vec<_>>()
        );
    }

    /// A changed binary path must produce a "modified" drift entry for `.mcp.json`.
    ///
    /// Regression test for the "MCP drift after binary path changes" bug where
    /// switching from a dev build to the release app caused `.mcp.json` to drift.
    #[test]
    fn mcp_drift_detected_when_binary_path_changes() {
        let project_dir = tempdir().unwrap();

        // Write .mcp.json with the OLD binary path
        let mut old_servers = Map::new();
        old_servers.insert(
            "automatic".to_string(),
            serde_json::json!({
                "command": "/old/path/to/automatic",
                "args": ["mcp-serve"],
                "env": { "AUTOMATIC_PROJECT": "test-project" }
            }),
        );
        ClaudeCode
            .write_mcp_config(project_dir.path(), &old_servers)
            .expect("write with old path");

        // Check drift against the NEW binary path (simulating a post-restart check)
        let mut new_servers = Map::new();
        new_servers.insert(
            "automatic".to_string(),
            serde_json::json!({
                "command": "/new/path/to/automatic",
                "args": ["mcp-serve"],
                "env": { "AUTOMATIC_PROJECT": "test-project" }
            }),
        );

        let mut files: Vec<DriftedFile> = Vec::new();
        collect_mcp_drift(
            &ClaudeCode,
            &project_dir.path().to_path_buf(),
            &new_servers,
            &mut files,
        );

        assert_eq!(files.len(), 1, "Expected exactly one drifted file");
        assert_eq!(files[0].reason, "modified");
        assert_eq!(files[0].path, ".mcp.json");
    }

    /// Drift check is a read-only operation — it must not write any files to disk.
    #[test]
    fn drift_check_is_read_only() {
        use crate::core::Project;

        let project_dir = tempdir().unwrap();
        let project = Project {
            name: "test".to_string(),
            directory: project_dir.path().display().to_string(),
            agents: vec!["claude".to_string()],
            skills: vec![],
            mcp_servers: vec![],
            ..Default::default()
        };

        let before: Vec<_> = fs::read_dir(project_dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .collect();

        let _ = check_project_drift(&project);

        let after: Vec<_> = fs::read_dir(project_dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .collect();

        assert_eq!(
            before.len(),
            after.len(),
            "Drift check must not write any files to disk"
        );
    }

    #[test]
    fn instruction_conflict_uses_snapshot_user_content_not_full_file_hash() {
        let project_dir = tempdir().unwrap();
        let project = Project {
            name: "test".to_string(),
            directory: project_dir.path().display().to_string(),
            agents: vec!["opencode".to_string()],
            ..Default::default()
        };

        let current_content = "# Instructions\n\nKeep this.\n\n<!-- automatic:rules:start -->\nRule v2\n<!-- automatic:rules:end -->\n";
        fs::write(project_dir.path().join("AGENTS.md"), current_content)
            .expect("write instruction file");
        crate::core::save_instruction_snapshot(
            project_dir.path().to_str().unwrap(),
            "AGENTS.md",
            "# Instructions\n\nKeep this.\n",
        )
        .expect("save snapshot");

        let mut project = project;
        project.instruction_file_hashes.insert(
            "AGENTS.md".to_string(),
            crate::core::compute_content_hash(
                "# Instructions\n\nKeep this.\n\n<!-- automatic:rules:start -->\nRule v1\n<!-- automatic:rules:end -->\n",
            ),
        );

        let conflicts =
            collect_instruction_conflicts_pub(&project, &project_dir.path().to_path_buf());
        assert!(
            conflicts.is_empty(),
            "matching snapshot content should not be treated as an instruction conflict"
        );
    }

    /// Custom skills (project-scoped) should produce no drift after sync,
    /// just like global skills.
    #[test]
    fn no_drift_after_sync_with_custom_skills() {
        let project_dir = tempdir().unwrap();

        // Mix of a global skill and a custom (project-scoped) skill
        let skill_contents: Vec<(String, String)> = vec![
            ("global-skill".to_string(), "# Global\n".to_string()),
            ("custom-skill".to_string(), "# Custom\n".to_string()),
        ];
        let selected_names = vec!["global-skill".to_string(), "custom-skill".to_string()];
        let local_names: Vec<String> = vec![];

        ClaudeCode
            .sync_skills(
                project_dir.path(),
                &skill_contents,
                &selected_names,
                &local_names,
            )
            .expect("sync_skills should succeed");

        let mut files: Vec<DriftedFile> = Vec::new();
        collect_skills_drift(
            &ClaudeCode,
            &project_dir.path().to_path_buf(),
            &skill_contents,
            &selected_names,
            &local_names,
            &mut files,
        );

        assert!(
            files.is_empty(),
            "Expected no drift after sync with custom skills, got: {:?}",
            files
                .iter()
                .map(|f| format!("{} ({})", f.path, f.reason))
                .collect::<Vec<_>>()
        );
    }

    /// Regression test for the "project keeps drifting after sync" bug:
    /// a skill that appears in both `project.skills` (library-backed) and
    /// `project.custom_skills` (stale snapshot) must not produce drift after
    /// the engine syncs the project.  Sync writes library content; if drift
    /// uses the custom_skills snapshot as its baseline they disagree forever.
    #[test]
    fn no_drift_when_skill_in_both_library_and_stale_custom_skills() {
        use crate::core::{save_skill, with_test_home, CustomSkill, Project};
        use crate::sync::engine::sync_project_without_autodetect;

        let home = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        with_test_home(home.path().to_path_buf(), || {
            // Library version is the "new" content the user has on disk.
            save_skill("boundary-audit", "# Library version (new)\n").expect("save_skill");

            // Project has the same name in both lists; custom_skills snapshot is
            // stale and would otherwise win in the second push, breaking drift.
            let mut project = Project {
                name: "regress".to_string(),
                directory: project_dir.path().display().to_string(),
                agents: vec!["claude".to_string()],
                skills: vec!["boundary-audit".to_string()],
                custom_skills: Some(vec![CustomSkill {
                    name: "boundary-audit".to_string(),
                    content: "# Stale custom snapshot (old)\n".to_string(),
                }]),
                ..Default::default()
            };

            sync_project_without_autodetect(&mut project).expect("sync");

            let report = check_project_drift(&project).expect("drift");
            assert!(
                !report.drifted,
                "Expected no drift, got: {:?}",
                report
                    .agents
                    .iter()
                    .flat_map(|a| a.files.iter())
                    .map(|f| format!("{} ({})", f.path, f.reason))
                    .collect::<Vec<_>>()
            );
        });
    }

    /// A custom skill that is not synced to disk should appear as "missing"
    /// in drift detection.
    #[test]
    fn drift_detected_for_missing_custom_skill() {
        let project_dir = tempdir().unwrap();

        // Create the skill dirs but don't write the custom skill
        let skill_dir = project_dir.path().join(".claude").join("skills");
        fs::create_dir_all(&skill_dir).unwrap();

        let skill_contents: Vec<(String, String)> =
            vec![("my-custom".to_string(), "# My Custom Skill\n".to_string())];
        let selected_names = vec!["my-custom".to_string()];
        let local_names: Vec<String> = vec![];

        let mut files: Vec<DriftedFile> = Vec::new();
        collect_skills_drift(
            &ClaudeCode,
            &project_dir.path().to_path_buf(),
            &skill_contents,
            &selected_names,
            &local_names,
            &mut files,
        );

        assert!(!files.is_empty(), "Expected drift for missing custom skill");
        assert!(
            files.iter().any(|f| f.reason == "missing"),
            "Expected a 'missing' drift entry"
        );
    }

    /// `collect_commands_drift` must report no drift when the on-disk
    /// commands match the workspace-command contents exactly.
    #[test]
    fn no_drift_after_sync_commands() {
        let project_dir = tempdir().unwrap();
        let dir = project_dir.path().to_path_buf();

        let workspace_commands: Vec<(String, String)> = vec![(
            "review".to_string(),
            "---\ndescription: Review the changes\n---\n\nLook hard.\n".to_string(),
        )];
        let custom_commands: Vec<crate::core::CustomCommand> = Vec::new();

        // Simulate the engine's two-step write so the on-disk state matches
        // what drift expects.
        let project_commands_dir = dir.join(".agents").join("commands");
        crate::agent::copy_commands_to_project(
            &project_commands_dir,
            &workspace_commands,
            &custom_commands,
        )
        .expect("write hub");
        let claude_commands_dir = ClaudeCode.commands_dir(&dir).expect("claude commands dir");
        crate::agent::symlink_commands_from_project(
            &claude_commands_dir,
            &project_commands_dir,
            &workspace_commands,
            &custom_commands,
            &ClaudeCode,
        )
        .expect("write claude commands");

        let mut files: Vec<DriftedFile> = Vec::new();
        collect_commands_drift(
            &ClaudeCode,
            &dir,
            &workspace_commands,
            &custom_commands,
            &mut files,
        );

        assert!(
            files.is_empty(),
            "Expected no drift after sync, got: {:?}",
            files
                .iter()
                .map(|f| format!("{} ({})", f.path, f.reason))
                .collect::<Vec<_>>()
        );
    }

    /// A library command edit that has not been propagated to a project
    /// must surface as a `modified` drift entry on the agent's commands dir.
    #[test]
    fn collect_commands_drift_detects_modified_file() {
        let project_dir = tempdir().unwrap();
        let dir = project_dir.path().to_path_buf();

        // First, write an "old" version of the command to disk.
        let old_workspace: Vec<(String, String)> = vec![(
            "review".to_string(),
            "---\ndescription: Old description\n---\n\nOld body.\n".to_string(),
        )];
        let custom: Vec<crate::core::CustomCommand> = Vec::new();
        let project_commands_dir = dir.join(".agents").join("commands");
        crate::agent::copy_commands_to_project(&project_commands_dir, &old_workspace, &custom)
            .expect("write hub old");
        let claude_commands_dir = ClaudeCode.commands_dir(&dir).expect("claude commands dir");
        crate::agent::symlink_commands_from_project(
            &claude_commands_dir,
            &project_commands_dir,
            &old_workspace,
            &custom,
            &ClaudeCode,
        )
        .expect("write claude old");

        // Now drift-check against the "new" library content (as if the
        // library was edited but the project wasn't re-synced).
        let new_workspace: Vec<(String, String)> = vec![(
            "review".to_string(),
            "---\ndescription: New description\n---\n\nNew body.\n".to_string(),
        )];

        let mut files: Vec<DriftedFile> = Vec::new();
        collect_commands_drift(&ClaudeCode, &dir, &new_workspace, &custom, &mut files);

        assert!(
            files.iter().any(|f| f.reason == "modified"),
            "Expected a 'modified' drift entry when library and disk diverge, got: {:?}",
            files
                .iter()
                .map(|f| format!("{} ({})", f.path, f.reason))
                .collect::<Vec<_>>()
        );
    }
}
