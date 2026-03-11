use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use crate::agent;
use crate::core::Project;

use super::helpers::{build_selected_servers, load_mcp_server_configs, load_skill_contents};

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

    // Build the MCP server map using the same logic as the sync engine
    // (strips internal `_` fields, substitutes OAuth proxy configs).
    let mcp_config = load_mcp_server_configs()?;
    let selected_servers = build_selected_servers(&project.name, &project.mcp_servers, &mcp_config);

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

    let instruction_conflicts = collect_instruction_file_conflicts(project, &dir);

    let drifted = !agent_drifts.is_empty() || !instruction_conflicts.is_empty();
    Ok(DriftReport {
        drifted,
        agents: agent_drifts,
        instruction_conflicts,
    })
}

/// Public wrapper for use by the `commands` layer.
pub fn collect_instruction_conflicts_pub(
    project: &Project,
    dir: &PathBuf,
) -> Vec<InstructionFileConflict> {
    collect_instruction_file_conflicts(project, dir)
}

/// Detect instruction files that were modified outside Automatic.
///
/// Compares the current on-disk hash of each instruction file against the
/// hash Automatic recorded the last time it wrote the file (stored in
/// `project.instruction_file_hashes`).  A mismatch means the file was
/// edited externally.
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

        let disk_user_content = crate::core::strip_rules_section_pub(
            &crate::core::strip_managed_section_pub(&raw_disk),
        );

        // Skip files with no user content.
        if disk_user_content.trim().is_empty() {
            continue;
        }

        let current_hash = crate::core::compute_content_hash(&raw_disk);
        let stored_hash = project.instruction_file_hashes.get(&filename);

        let is_externally_modified = match stored_hash {
            Some(stored) => &current_hash != stored,
            // No stored hash means Automatic has never recorded writing this
            // file.  If the file has user content, it was created externally.
            None => true,
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
                let user_content = crate::core::strip_rules_section_pub(
                    &crate::core::strip_managed_section_pub(&raw),
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

                    if !disk_file.exists() {
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
