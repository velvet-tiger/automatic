use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use crate::agent;
use crate::core::{Project, ProjectMode};

use super::helpers::{
    build_selected_servers, build_skill_contents, collect_custom_asset_conflicts,
    extract_agent_machine_name, load_mcp_server_configs, CustomAssetConflict, CustomAssetKind,
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
        //
        // Entries Automatic manages at global scope are deliberately excluded —
        // they render byte-identical to the project entry (both come from the
        // same registry config), so Claude Code's "user scope wins" precedence
        // is behaviorally a no-op.  Warning on Automatic-managed conflicts would
        // mean every server the user assigned via Providers > agent > MCP
        // showed up in every project's problems list.  Foreign global entries
        // (a `claude mcp add --scope user` the user ran themselves) still
        // conflict and are still surfaced.
        let managed_global = crate::sync::global_mcp::managed_entries_for(agent_id);
        let conflicts: Vec<String> = project_servers
            .keys()
            .filter(|name| global_servers.contains_key(*name))
            .filter(|name| !managed_global.iter().any(|m| m == *name))
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
    /// `true` if any agent has MCP/skill drift, or instruction/custom assets have conflicts.
    pub drifted: bool,
    /// One entry per agent that has at least one drifted file.
    pub agents: Vec<AgentDrift>,
    /// Instruction files that have content on disk which Automatic does not recognise.
    /// These require user action: keep existing or overwrite.
    #[serde(default)]
    pub instruction_conflicts: Vec<InstructionFileConflict>,
    /// Project-scoped custom skills/rules/agents/commands whose on-disk files
    /// differ from the stored project config. Require user action: adopt disk
    /// (favoured) or overwrite with Automatic's stored content.
    #[serde(default)]
    pub custom_conflicts: Vec<CustomAssetConflict>,
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
            custom_conflicts: vec![],
        });
    }

    let dir = PathBuf::from(&project.directory);
    if !dir.exists() {
        return Ok(DriftReport {
            drifted: false,
            agents: vec![],
            instruction_conflicts: vec![],
            custom_conflicts: vec![],
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

    let custom_conflicts = collect_custom_asset_conflicts(project, &effective_dir);
    let conflicting_agents: HashSet<String> = custom_conflicts
        .iter()
        .filter(|c| c.kind == CustomAssetKind::Agent)
        .map(|c| c.name.clone())
        .collect();
    let conflicting_commands: HashSet<String> = custom_conflicts
        .iter()
        .filter(|c| c.kind == CustomAssetKind::Command)
        .map(|c| c.name.clone())
        .collect();

    // Group attached hooks by target agent, mirroring
    // `sync::engine::sync_project_hooks_step`. A hook that fails to read from
    // the library is skipped rather than failing the whole drift check —
    // drift detection is read-only and best-effort throughout this file.
    let mut hooks_by_agent: std::collections::HashMap<String, Vec<crate::core::Hook>> =
        std::collections::HashMap::new();
    for hook_name in &project.hooks {
        if let Ok(hook) = crate::core::read_hook_parsed(hook_name) {
            hooks_by_agent
                .entry(hook.agent.clone())
                .or_default()
                .push(hook);
        }
    }
    let no_hooks: Vec<crate::core::Hook> = Vec::new();

    let mut agent_drifts: Vec<AgentDrift> = Vec::new();

    for agent_id in &project.agents {
        if let Some(agent_instance) = agent::from_id(agent_id) {
            let mut files: Vec<DriftedFile> = Vec::new();

            collect_mcp_drift(
                agent_instance,
                &effective_dir,
                &selected_servers,
                &mut files,
            );
            // Pass custom skill names as local so modified custom skills are
            // not reported as ordinary agent drift. They surface instead as
            // `custom_conflicts` with an adopt/overwrite prompt.
            collect_skills_drift(
                agent_instance,
                &effective_dir,
                &skill_contents,
                &all_selected_skill_names,
                &custom_skill_names,
                &mut files,
            );
            collect_agents_drift(
                agent_instance,
                &effective_dir,
                project.custom_agents.as_deref().unwrap_or(&[]),
                &project.user_agents,
                &conflicting_agents,
                &mut files,
            );
            collect_commands_drift(
                agent_instance,
                &effective_dir,
                &workspace_command_contents,
                custom_commands,
                &conflicting_commands,
                &mut files,
            );
            collect_hooks_drift(
                agent_instance,
                &effective_dir,
                hooks_by_agent.get(agent_id).unwrap_or(&no_hooks),
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

    let mut instruction_conflicts = collect_instruction_file_conflicts(project, &effective_dir);
    instruction_conflicts.extend(collect_shadowing_legacy_instruction_conflicts(
        project,
        &effective_dir,
    ));

    let drifted = !agent_drifts.is_empty()
        || !instruction_conflicts.is_empty()
        || !custom_conflicts.is_empty();
    Ok(DriftReport {
        drifted,
        agents: agent_drifts,
        instruction_conflicts,
        custom_conflicts,
    })
}

/// Public wrapper for use by the `commands` layer (backs the
/// `get_instruction_file_conflicts` Tauri command, a standalone check
/// independent of the full drift report).
/// Automatically resolves the effective directory based on project mode.
pub fn collect_instruction_conflicts_pub(
    project: &Project,
    dir: &PathBuf,
) -> Vec<InstructionFileConflict> {
    let effective_dir = match project.mode {
        ProjectMode::Silent => dir.join(".automatic").join("silent"),
        ProjectMode::Normal => dir.clone(),
    };
    let mut conflicts = collect_instruction_file_conflicts(project, &effective_dir);
    conflicts.extend(collect_shadowing_legacy_instruction_conflicts(
        project,
        &effective_dir,
    ));
    conflicts
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

/// Detect an unresolved legacy instruction file that still shadows the
/// current one in the vendor's own precedence order.
///
/// `sync::engine::migrate_legacy_instruction_file` runs at sync time and
/// auto-resolves most cases, but when both files carry different
/// non-empty user content it can't choose for the user — it leaves the
/// legacy file in place with only an stderr line. For a vendor where
/// `legacy_shadows_current` is true (Zed's `.rules`, Warp's `WARP.md`),
/// that stderr line is not enough: the vendor's own tool keeps reading the
/// legacy file, so whatever Automatic writes to `AGENTS.md` is invisible to
/// it until the user acts. This surfaces that as an instruction conflict
/// through the same channel [`collect_instruction_file_conflicts`] feeds,
/// so it reaches the UI on the very next drift check — including on every
/// check after that, for as long as the legacy file remains, which is
/// exactly the "still shadowing" signal this function looks for.
fn collect_shadowing_legacy_instruction_conflicts(
    project: &Project,
    dir: &PathBuf,
) -> Vec<InstructionFileConflict> {
    let mut conflicts = Vec::new();

    for spec in super::engine::LEGACY_INSTRUCTION_MIGRATIONS {
        if !spec.legacy_shadows_current {
            continue;
        }
        if !project.agents.iter().any(|a| a == spec.agent_id) {
            continue;
        }

        let legacy_path = dir.join(spec.legacy);
        if !legacy_path.is_file() {
            continue;
        }

        let Some(dir_str) = dir.to_str() else {
            continue;
        };
        let legacy_content =
            crate::core::read_project_file(dir_str, spec.legacy).unwrap_or_default();
        if legacy_content.trim().is_empty() {
            continue;
        }
        let current_content =
            crate::core::read_project_file(dir_str, spec.current).unwrap_or_default();
        if legacy_content.trim() == current_content.trim() {
            // Nothing actually diverges — the legacy file just hasn't been
            // deleted yet (e.g. its removal failed and was logged), not a
            // content conflict the user needs to resolve.
            continue;
        }

        let agent_labels: Vec<String> = agent::from_id(spec.agent_id)
            .map(|a| vec![a.label().to_string()])
            .unwrap_or_default();

        conflicts.push(InstructionFileConflict {
            filename: spec.legacy.to_string(),
            agent_labels,
            // Reframed from every other conflict this struct represents:
            // `disk_content` is what the vendor's tool is actually reading
            // right now, and `automatic_content` is what Automatic is
            // managing in `spec.current` instead — the comparison a user
            // needs to understand why recent changes aren't taking effect,
            // not a "did I edit this myself" diff against a stored snapshot.
            disk_content: legacy_content,
            automatic_content: current_content,
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

    // Seed the files this agent merges into.  A merge writer handed an empty
    // tempdir drops every key the real file carries, which would surface as
    // drift the user can never clear by syncing.
    for input in agent_instance.mcp_merge_inputs(dir) {
        let Ok(relative) = input.strip_prefix(dir) else {
            continue;
        };
        if !input.is_file() {
            continue;
        }
        let seeded = tmp.path().join(relative);
        if let Some(parent) = seeded.parent() {
            if fs::create_dir_all(parent).is_err() {
                return;
            }
        }
        if fs::copy(&input, &seeded).is_err() {
            return;
        }
    }

    let prepared = agent::prepare_mcp_servers(agent_instance, servers, dir);
    if agent_instance
        .write_mcp_config(tmp.path(), &prepared)
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
    skip_custom_names: &HashSet<String>,
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
    if agent::copy_commands_to_project(
        &tmp_hub,
        workspace_command_contents,
        custom_commands,
        &std::collections::HashSet::new(),
    )
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
            &std::collections::HashSet::new(),
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
                        // Conflicting custom commands surface via custom_conflicts.
                        let stem = file_name
                            .strip_suffix(".prompt.md")
                            .or_else(|| file_name.strip_suffix(".md"))
                            .or_else(|| file_name.strip_suffix(".toml"))
                            .unwrap_or(file_name.as_str());
                        if skip_custom_names.contains(stem) {
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
    skip_custom_names: &HashSet<String>,
    out: &mut Vec<DriftedFile>,
) {
    let agents_dir = match agent_instance.agents_dir(dir) {
        Some(d) => d,
        None => return,
    };

    // Full filenames, not bare machine names — see the matching fix in
    // `sync::helpers::sync_user_agents` and `Agent::agent_file_name`'s doc
    // comment. `Path::extension` only ever returns the last dot-segment, so
    // a compound extension (Copilot's `{name}.agent.md`) makes machine-name
    // recovery via `file_stem()` lossy; comparing whole filenames sidesteps
    // the problem instead of trying to invert a lossy transform.
    let mut expected_file_names: std::collections::HashSet<String> =
        std::collections::HashSet::new();

    // Add custom agent filenames
    for agent in custom_agents {
        let machine_name = extract_agent_machine_name(&agent.content)
            .unwrap_or_else(|| agent.name.to_lowercase().replace(' ', "-"));
        expected_file_names.insert(agent_instance.agent_file_name(&machine_name));
    }

    // Add user agent filenames (from workspace registry)
    for name in user_agent_names {
        if let Ok(content) = crate::core::read_subagent(name) {
            if let Ok(agent) = serde_json::from_str::<crate::core::Subagent>(&content) {
                let machine_name = extract_agent_machine_name(&agent.content)
                    .unwrap_or_else(|| name.to_lowercase().replace(' ', "-"));
                expected_file_names.insert(agent_instance.agent_file_name(&machine_name));
            }
        }
    }

    // Check for missing/modified custom agent files (skip conflicting — those
    // surface as custom_conflicts instead).
    for agent in custom_agents {
        if skip_custom_names.contains(&agent.name) {
            continue;
        }
        let machine_name = extract_agent_machine_name(&agent.content)
            .unwrap_or_else(|| agent.name.to_lowercase().replace(' ', "-"));
        let converted_content = agent_instance.convert_agent_content(&agent.content, &machine_name);
        let agent_path = agents_dir.join(agent_instance.agent_file_name(&machine_name));

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
                let agent_path = agents_dir.join(agent_instance.agent_file_name(&machine_name));

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

    // Check for stale *managed* agent files: not in the expected set, and
    // carrying the automatic-managed marker. A file without the marker is
    // left alone unconditionally — it may be something the user placed in
    // this directory by hand, and without the marker there is no way to
    // tell it apart from a stale Automatic file.
    if agents_dir.exists() {
        if let Ok(entries) = fs::read_dir(&agents_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
                    continue;
                };
                if expected_file_names.contains(file_name) || !agent::is_managed_agent_file(&path) {
                    continue;
                }
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

/// Collect hook drift entries for one agent into `out`. Two flavours, driven
/// by [`agent::Agent::hook_config_target`]:
///
/// - **Owned files** (Codex, Copilot, Droid): [`collect_owned_hooks_drift`]
///   does a whole-file byte compare, the same trick [`collect_mcp_drift`]
///   uses — but against the exact known path rather than a top-level
///   directory scan, since every hook config file lives nested and that scan
///   would see nothing.
/// - **Merge files** (Claude Code, Gemini CLI): [`collect_merged_hooks_drift`]
///   compares only the tagged-managed subset, so an unrelated edit elsewhere
///   in the shared settings file (model, permissions, …) never reads as hook
///   drift.
///
/// `None` from `hook_config_target` — no hooks capability, or Cursor's
/// sidecar-manifest mechanism, which neither flavour fits — means no
/// entries are collected here.
fn collect_hooks_drift(
    agent_instance: &dyn agent::Agent,
    dir: &PathBuf,
    hooks_for_agent: &[crate::core::Hook],
    out: &mut Vec<DriftedFile>,
) {
    match agent_instance.hook_config_target(dir) {
        Some(agent::HookConfigTarget::Owned { path }) => {
            collect_owned_hooks_drift(agent_instance, dir, &path, hooks_for_agent, out);
        }
        Some(agent::HookConfigTarget::Merged { path, key }) => {
            collect_merged_hooks_drift(agent_instance, dir, &path, key, hooks_for_agent, out);
        }
        None => {}
    }
}

fn relative_path_string(dir: &PathBuf, path: &PathBuf) -> String {
    path.strip_prefix(dir).unwrap_or(path).display().to_string()
}

/// Whole-file compare for an owned hooks file. Unlike every other collector
/// in this module, the expected state when there are no hooks is that the
/// file does not exist at all — `write_owned_hooks_file` deletes it rather
/// than leaving an empty shell — so an on-disk file with zero configured
/// hooks is itself drift ("stale"), not silently ignored.
fn collect_owned_hooks_drift(
    agent_instance: &dyn agent::Agent,
    dir: &PathBuf,
    expected_path: &PathBuf,
    hooks_for_agent: &[crate::core::Hook],
    out: &mut Vec<DriftedFile>,
) {
    let filename = relative_path_string(dir, expected_path);

    if hooks_for_agent.is_empty() {
        if expected_path.exists() {
            let actual = fs::read_to_string(expected_path).ok();
            out.push(DriftedFile {
                path: filename,
                reason: "stale".into(),
                expected: None,
                actual,
            });
        }
        return;
    }

    let tmp = match tempfile::tempdir() {
        Ok(t) => t,
        Err(_) => return,
    };
    if agent_instance
        .sync_hooks(tmp.path(), hooks_for_agent)
        .is_err()
    {
        return;
    }

    if !expected_path.exists() {
        out.push(DriftedFile {
            path: filename,
            reason: "missing".into(),
            expected: None,
            actual: None,
        });
        return;
    }

    let Ok(relative) = expected_path.strip_prefix(dir) else {
        return;
    };
    let tmp_path = tmp.path().join(relative);
    let Ok(expected) = fs::read_to_string(&tmp_path) else {
        return;
    };
    let actual = match fs::read_to_string(expected_path) {
        Ok(c) => c,
        Err(_) => {
            out.push(DriftedFile {
                path: filename,
                reason: "unreadable".into(),
                expected: None,
                actual: None,
            });
            return;
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

/// Subset compare for a merge-flavour hooks file. The expected subset is
/// computed by running the real `sync_hooks` against an *unseeded* tempdir:
/// since only the tagged-managed handlers are ever compared, there is
/// nothing else worth carrying over from the real file for this
/// computation, and the merge writer's own strip-then-merge behaviour
/// already isolates Automatic's handlers correctly on its own.
fn collect_merged_hooks_drift(
    agent_instance: &dyn agent::Agent,
    dir: &PathBuf,
    settings_path: &PathBuf,
    settings_key: &str,
    hooks_for_agent: &[crate::core::Hook],
    out: &mut Vec<DriftedFile>,
) {
    let filename = relative_path_string(dir, settings_path);

    let tmp = match tempfile::tempdir() {
        Ok(t) => t,
        Err(_) => return,
    };
    if agent_instance
        .sync_hooks(tmp.path(), hooks_for_agent)
        .is_err()
    {
        return;
    }
    let Ok(relative) = settings_path.strip_prefix(dir) else {
        return;
    };
    let tmp_path = tmp.path().join(relative);

    let expected_subset = read_managed_hook_subset(&tmp_path, settings_key);
    let actual_subset = read_managed_hook_subset(settings_path, settings_key);

    if expected_subset != actual_subset {
        out.push(DriftedFile {
            path: filename,
            reason: "modified".into(),
            expected: Some(pretty_hook_subset(&expected_subset)),
            actual: Some(pretty_hook_subset(&actual_subset)),
        });
    }
}

/// Read `path`, extract the tagged-managed handlers under `key`, and return
/// them as a normalised map. Absent or unparseable files yield an empty map
/// — drift detection is read-only and best-effort, matching every other
/// collector in this file, and an unparseable settings file is the merge
/// writer's problem to surface as a sync error, not this one's.
fn read_managed_hook_subset(path: &PathBuf, key: &str) -> Map<String, Value> {
    let Ok(raw) = fs::read_to_string(path) else {
        return Map::new();
    };
    let Ok(Value::Object(root)) = serde_json::from_str::<Value>(&raw) else {
        return Map::new();
    };
    let Some(Value::Object(hooks_obj)) = root.get(key).cloned() else {
        return Map::new();
    };
    agent::extract_managed_hook_handlers(&hooks_obj)
}

fn pretty_hook_subset(subset: &Map<String, Value>) -> String {
    serde_json::to_string_pretty(subset).unwrap_or_default()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, ClaudeCode, CodexCli, Cursor, GeminiCli};
    use serde_json::Map;
    use std::fs;
    use std::path::Path;
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

    /// Lay out a project the way the sync engine leaves one: the canonical
    /// `.agents/skills/` hub holds real directories, and each agent skill
    /// directory holds either a symlink back to the hub (`sync_mode`
    /// `"symlink"`) or its own copy (`sync_mode` `"copy"`).
    ///
    /// The shape is built here rather than by calling
    /// `symlink_skills_from_project` because that helper reads the real
    /// `~/.automatic*/settings.json` to choose a mode, which would make the
    /// outcome depend on the machine the test runs on.
    fn lay_out_synced_skill(
        root: &Path,
        agent_skills_dir: &Path,
        name: &str,
        content: &str,
        symlink: bool,
    ) {
        let hub = root.join(".agents").join("skills").join(name);
        fs::create_dir_all(&hub).unwrap();
        fs::write(hub.join("SKILL.md"), content).unwrap();

        if agent_skills_dir == root.join(".agents").join("skills") {
            return;
        }

        fs::create_dir_all(agent_skills_dir).unwrap();
        let link = agent_skills_dir.join(name);
        if symlink {
            #[cfg(unix)]
            std::os::unix::fs::symlink(&hub, &link).unwrap();
            #[cfg(windows)]
            std::os::windows::fs::symlink_dir(&hub, &link).unwrap();
        } else {
            fs::create_dir_all(&link).unwrap();
            fs::write(link.join("SKILL.md"), content).unwrap();
        }
    }

    fn assert_junie_drift_is_quiet(symlink: bool) {
        use crate::agent::Junie;

        let project_dir = tempdir().unwrap();
        let root = project_dir.path();
        let name = "drift-fixture-skill";
        let content = "# Fixture skill\n";

        for skills_dir in Junie.skill_dirs(root) {
            lay_out_synced_skill(root, &skills_dir, name, content, symlink);
        }

        let skill_contents = vec![(name.to_string(), content.to_string())];
        let selected = vec![name.to_string()];

        let mut files: Vec<DriftedFile> = Vec::new();
        collect_skills_drift(
            &Junie,
            &root.to_path_buf(),
            &skill_contents,
            &selected,
            &[],
            &mut files,
        );

        assert!(
            files.is_empty(),
            "Junie drift must be quiet in {} mode, got: {:?}",
            if symlink { "symlink" } else { "copy" },
            files
                .iter()
                .map(|f| format!("{} ({})", f.path, f.reason))
                .collect::<Vec<_>>()
        );
    }

    /// `.junie/skills` entered drift detection when `sync_skills` started
    /// deriving its targets from `skill_dirs()`.  Both sync modes must stay
    /// quiet, because a false positive here lights up every Junie project on
    /// the first run after release.
    #[test]
    fn junie_skill_dirs_are_quiet_after_a_symlink_mode_sync() {
        assert_junie_drift_is_quiet(true);
    }

    #[test]
    fn junie_skill_dirs_are_quiet_after_a_copy_mode_sync() {
        assert_junie_drift_is_quiet(false);
    }

    /// A skill missing from `.junie/skills` must now be reported.  Before
    /// `sync_skills` looped every `skill_dirs()` entry, the tempdir never
    /// contained `.junie/skills` and this drift was invisible.
    #[test]
    fn a_skill_missing_from_the_junie_directory_is_reported() {
        use crate::agent::Junie;

        let project_dir = tempdir().unwrap();
        let root = project_dir.path();
        let name = "drift-fixture-skill";
        let content = "# Fixture skill\n";

        let hub = root.join(".agents").join("skills").join(name);
        fs::create_dir_all(&hub).unwrap();
        fs::write(hub.join("SKILL.md"), content).unwrap();
        fs::create_dir_all(root.join(".junie").join("skills")).unwrap();

        let mut files: Vec<DriftedFile> = Vec::new();
        collect_skills_drift(
            &Junie,
            &root.to_path_buf(),
            &[(name.to_string(), content.to_string())],
            &[name.to_string()],
            &[],
            &mut files,
        );

        assert!(
            files
                .iter()
                .any(|f| f.reason == "missing" && f.path.starts_with(".junie/skills/")),
            "expected a missing entry under .junie/skills, got: {:?}",
            files
                .iter()
                .map(|f| format!("{} ({})", f.path, f.reason))
                .collect::<Vec<_>>()
        );
    }

    /// `opencode.json` is the user's own project config — `model`,
    /// `permission`, `instructions` and `agent` sit alongside `mcp`.  Since
    /// `write_mcp_config` merges rather than rebuilds, drift must seed the
    /// tempdir from the real file first.  Without that seeding the expected
    /// config would be missing every user key, and every OpenCode project would
    /// show drift that no amount of syncing could clear.
    #[test]
    fn opencode_drift_is_quiet_when_the_user_has_their_own_keys() {
        use crate::agent::OpenCode;

        let project_dir = tempdir().unwrap();
        let config = project_dir.path().join("opencode.json");
        fs::write(
            &config,
            "{\n  \"model\": \"anthropic/claude-opus-4\",\n  \"permission\": {}\n}\n",
        )
        .unwrap();

        let mut servers = Map::new();
        servers.insert(
            "linear".to_string(),
            serde_json::json!({ "type": "http", "url": "https://mcp.linear.app/mcp" }),
        );

        // Sync, exactly as `sync_agent_configs_step` does.
        let prepared = agent::prepare_mcp_servers(&OpenCode, &servers, project_dir.path());
        OpenCode
            .write_mcp_config(project_dir.path(), &prepared)
            .expect("write_mcp_config should succeed");

        let on_disk = fs::read_to_string(&config).unwrap();
        assert!(
            on_disk.contains("anthropic/claude-opus-4"),
            "the user's model choice must survive a sync:\n{on_disk}"
        );

        let mut files: Vec<DriftedFile> = Vec::new();
        collect_mcp_drift(
            &OpenCode,
            &project_dir.path().to_path_buf(),
            &servers,
            &mut files,
        );

        assert!(
            files.is_empty(),
            "Expected no MCP drift after sync, got: {:?}",
            files
                .iter()
                .map(|f| format!("{} ({})", f.path, f.reason))
                .collect::<Vec<_>>()
        );
    }

    /// Seeding must not hide real drift: a hand-edit inside the `mcp` key that
    /// Automatic owns still has to be reported.
    #[test]
    fn opencode_drift_still_reports_a_hand_edited_mcp_block() {
        use crate::agent::OpenCode;

        let project_dir = tempdir().unwrap();
        let config = project_dir.path().join("opencode.json");
        fs::write(
            &config,
            "{\n  \"model\": \"anthropic/claude-opus-4\",\n  \
             \"mcp\": { \"linear\": { \"type\": \"remote\", \"url\": \"https://wrong.example\" } }\n}\n",
        )
        .unwrap();

        let mut servers = Map::new();
        servers.insert(
            "linear".to_string(),
            serde_json::json!({ "type": "http", "url": "https://mcp.linear.app/mcp" }),
        );

        let mut files: Vec<DriftedFile> = Vec::new();
        collect_mcp_drift(
            &OpenCode,
            &project_dir.path().to_path_buf(),
            &servers,
            &mut files,
        );

        assert!(
            files
                .iter()
                .any(|f| f.path == "opencode.json" && f.reason == "modified"),
            "a hand-edited mcp block must still drift, got: {:?}",
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

    /// When a project-scoped custom skill's on-disk SKILL.md differs from the
    /// stored snapshot, drift must report a `custom_conflicts` entry (not
    /// ordinary agent skill drift), and sync must leave the on-disk file alone.
    #[test]
    fn custom_skill_conflict_reported_and_sync_preserves_disk() {
        use crate::core::{with_test_home, CustomSkill, Project};
        use crate::sync::engine::sync_project_without_autodetect;
        use crate::sync::helpers::collect_custom_skill_conflicts;

        let home = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        with_test_home(home.path().to_path_buf(), || {
            let hub = project_dir
                .path()
                .join(".agents")
                .join("skills")
                .join("local-rule");
            fs::create_dir_all(&hub).unwrap();
            let disk_content = "# On disk custom skill\nEdited outside Automatic.\n";
            let stored_content = "# Stored custom skill\nOriginal from Automatic.\n";
            fs::write(hub.join("SKILL.md"), disk_content).unwrap();

            let mut project = Project {
                name: "skill-conflict".to_string(),
                directory: project_dir.path().display().to_string(),
                agents: vec!["claude".to_string()],
                custom_skills: Some(vec![CustomSkill {
                    name: "local-rule".to_string(),
                    content: stored_content.to_string(),
                }]),
                ..Default::default()
            };

            let conflicts = collect_custom_skill_conflicts(&project, project_dir.path());
            assert_eq!(conflicts.len(), 1);
            assert_eq!(conflicts[0].name, "local-rule");
            assert_eq!(conflicts[0].disk_content, disk_content);
            assert_eq!(conflicts[0].automatic_content, stored_content);

            let report = check_project_drift(&project).expect("drift");
            assert!(report.drifted);
            assert_eq!(report.custom_conflicts.len(), 1);
            assert!(
                report
                    .agents
                    .iter()
                    .all(|a| { !a.files.iter().any(|f| f.path.contains("local-rule")) }),
                "custom skill conflicts must not also appear as agent drift"
            );

            sync_project_without_autodetect(&mut project).expect("sync");

            let after = fs::read_to_string(hub.join("SKILL.md")).expect("read after sync");
            assert_eq!(
                after, disk_content,
                "sync must favour on-disk custom skill content"
            );
            assert_ne!(after, stored_content);
        });
    }

    /// Custom commands and rules follow the same favour-disk conflict contract.
    #[test]
    fn custom_command_and_rule_conflicts_reported_and_sync_preserves_disk() {
        use crate::core::{with_test_home, CustomCommand, CustomRule, Project};
        use crate::sync::engine::sync_project_without_autodetect;
        use crate::sync::helpers::{
            collect_custom_command_conflicts, collect_custom_rule_conflicts, CustomAssetKind,
        };

        let home = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        with_test_home(home.path().to_path_buf(), || {
            let cmd_dir = project_dir.path().join(".agents").join("commands");
            fs::create_dir_all(&cmd_dir).unwrap();
            let disk_cmd = "---\nautomatic-managed: true\n---\n# Disk command\n";
            let stored_cmd = "# Stored command\n";
            fs::write(cmd_dir.join("my-cmd.md"), disk_cmd).unwrap();

            let instructions = project_dir.path().join(".automatic").join("instructions");
            fs::create_dir_all(&instructions).unwrap();
            let disk_rule = "<!-- managed by Automatic — do not edit by hand -->\n\n# Disk rule\n";
            let stored_rule = "# Stored rule";
            fs::write(instructions.join("custom-my-rule.md"), disk_rule).unwrap();

            let mut project = Project {
                name: "asset-conflict".to_string(),
                directory: project_dir.path().display().to_string(),
                agents: vec!["claude".to_string()],
                custom_commands: Some(vec![CustomCommand {
                    name: "my-cmd".to_string(),
                    content: stored_cmd.to_string(),
                }]),
                custom_rules: vec![CustomRule {
                    name: "My Rule".to_string(),
                    content: stored_rule.to_string(),
                }],
                instructions_index_mode: true,
                ..Default::default()
            };

            let cmd_conflicts = collect_custom_command_conflicts(&project, project_dir.path());
            assert_eq!(cmd_conflicts.len(), 1);
            assert_eq!(cmd_conflicts[0].kind, CustomAssetKind::Command);

            let rule_conflicts = collect_custom_rule_conflicts(&project, project_dir.path());
            assert_eq!(rule_conflicts.len(), 1);
            assert_eq!(rule_conflicts[0].kind, CustomAssetKind::Rule);
            assert_eq!(rule_conflicts[0].disk_content, "# Disk rule");

            let report = check_project_drift(&project).expect("drift");
            assert!(report.custom_conflicts.len() >= 2);

            sync_project_without_autodetect(&mut project).expect("sync");

            let after_cmd = fs::read_to_string(cmd_dir.join("my-cmd.md")).unwrap();
            assert_eq!(after_cmd, disk_cmd, "sync must favour on-disk command");

            let after_rule = fs::read_to_string(instructions.join("custom-my-rule.md")).unwrap();
            assert!(
                after_rule.contains("# Disk rule"),
                "sync must favour on-disk rule"
            );
            assert!(!after_rule.contains("# Stored rule"));
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
            &HashSet::new(),
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
            &HashSet::new(),
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
        crate::agent::copy_commands_to_project(
            &project_commands_dir,
            &old_workspace,
            &custom,
            &HashSet::new(),
        )
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
        collect_commands_drift(
            &ClaudeCode,
            &dir,
            &new_workspace,
            &custom,
            &HashSet::new(),
            &mut files,
        );

        assert!(
            files.iter().any(|f| f.reason == "modified"),
            "Expected a 'modified' drift entry when library and disk diverge, got: {:?}",
            files
                .iter()
                .map(|f| format!("{} ({})", f.path, f.reason))
                .collect::<Vec<_>>()
        );
    }

    // ── Hook drift ──────────────────────────────────────────────────────────

    fn cmd_hook(agent_id: &str, name: &str, event: &str, command: &str) -> crate::core::Hook {
        crate::core::Hook {
            name: name.to_string(),
            agent: agent_id.to_string(),
            event: event.to_string(),
            matcher: None,
            handler: crate::core::HookHandler::Command {
                command: command.to_string(),
            },
            timeout_sec: None,
            plugin_id: None,
            _author: None,
        }
    }

    #[test]
    fn no_drift_after_owned_hooks_sync() {
        let dir = tempdir().unwrap();
        let hooks = vec![cmd_hook("codex", "ping", "SessionStart", "echo hi")];
        CodexCli.sync_hooks(dir.path(), &hooks).unwrap();

        let mut files: Vec<DriftedFile> = Vec::new();
        collect_hooks_drift(&CodexCli, &dir.path().to_path_buf(), &hooks, &mut files);

        assert!(
            files.is_empty(),
            "Expected no drift after sync, got: {:?}",
            files
                .iter()
                .map(|f| format!("{} ({})", f.path, f.reason))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn owned_hooks_drift_detected_when_command_changes() {
        let dir = tempdir().unwrap();
        let synced = vec![cmd_hook("codex", "ping", "SessionStart", "echo old")];
        CodexCli.sync_hooks(dir.path(), &synced).unwrap();

        let current = vec![cmd_hook("codex", "ping", "SessionStart", "echo new")];
        let mut files: Vec<DriftedFile> = Vec::new();
        collect_hooks_drift(&CodexCli, &dir.path().to_path_buf(), &current, &mut files);

        assert!(
            files
                .iter()
                .any(|f| f.path == ".codex/hooks.json" && f.reason == "modified"),
            "Expected modified drift on .codex/hooks.json, got: {:?}",
            files
                .iter()
                .map(|f| format!("{} ({})", f.path, f.reason))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn owned_hooks_file_with_no_configured_hooks_is_stale() {
        let dir = tempdir().unwrap();
        // Simulate a leftover file from before the hook was detached: written
        // once, never cleaned up because sync_hooks was never called with an
        // empty set for this project (e.g. an interrupted sync).
        let synced = vec![cmd_hook("codex", "temp", "Stop", "echo bye")];
        CodexCli.sync_hooks(dir.path(), &synced).unwrap();

        let mut files: Vec<DriftedFile> = Vec::new();
        collect_hooks_drift(&CodexCli, &dir.path().to_path_buf(), &[], &mut files);

        assert!(
            files
                .iter()
                .any(|f| f.path == ".codex/hooks.json" && f.reason == "stale"),
            "Expected stale drift on .codex/hooks.json, got: {:?}",
            files
                .iter()
                .map(|f| format!("{} ({})", f.path, f.reason))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_drift_after_merged_hooks_sync() {
        let dir = tempdir().unwrap();
        let hooks = vec![cmd_hook("claude", "ping", "SessionStart", "echo hi")];
        ClaudeCode.sync_hooks(dir.path(), &hooks).unwrap();

        let mut files: Vec<DriftedFile> = Vec::new();
        collect_hooks_drift(&ClaudeCode, &dir.path().to_path_buf(), &hooks, &mut files);

        assert!(
            files.is_empty(),
            "Expected no drift after sync, got: {:?}",
            files
                .iter()
                .map(|f| format!("{} ({})", f.path, f.reason))
                .collect::<Vec<_>>()
        );
    }

    /// The whole point of comparing only the managed subset: an edit to
    /// `.claude/settings.json` that has nothing to do with hooks (here,
    /// `model`) must never surface as hook drift.
    #[test]
    fn merged_hooks_drift_ignores_unrelated_settings_edits() {
        let dir = tempdir().unwrap();
        let hooks = vec![cmd_hook("claude", "ping", "SessionStart", "echo hi")];
        ClaudeCode.sync_hooks(dir.path(), &hooks).unwrap();

        let settings_path = dir.path().join(".claude/settings.json");
        let raw = fs::read_to_string(&settings_path).unwrap();
        let mut settings: Value = serde_json::from_str(&raw).unwrap();
        settings["model"] = Value::String("claude-opus-4-7".to_string());
        fs::write(
            &settings_path,
            serde_json::to_string_pretty(&settings).unwrap(),
        )
        .unwrap();

        let mut files: Vec<DriftedFile> = Vec::new();
        collect_hooks_drift(&ClaudeCode, &dir.path().to_path_buf(), &hooks, &mut files);

        assert!(
            files.is_empty(),
            "An unrelated settings.json edit must not report as hook drift, got: {:?}",
            files
                .iter()
                .map(|f| format!("{} ({})", f.path, f.reason))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn merged_hooks_drift_detected_when_managed_handler_changes() {
        let dir = tempdir().unwrap();
        let synced = vec![cmd_hook("gemini", "ping", "SessionStart", "echo old")];
        GeminiCli.sync_hooks(dir.path(), &synced).unwrap();

        let current = vec![cmd_hook("gemini", "ping", "SessionStart", "echo new")];
        let mut files: Vec<DriftedFile> = Vec::new();
        collect_hooks_drift(&GeminiCli, &dir.path().to_path_buf(), &current, &mut files);

        assert!(
            files
                .iter()
                .any(|f| f.path == ".gemini/settings.json" && f.reason == "modified"),
            "Expected modified drift on .gemini/settings.json, got: {:?}",
            files
                .iter()
                .map(|f| format!("{} ({})", f.path, f.reason))
                .collect::<Vec<_>>()
        );
    }

    /// Cursor declares `hooks: true` but uses its own sidecar-manifest
    /// mechanism rather than either `HookConfigTarget` flavour, so
    /// `hook_config_target` stays `None` and this collector must produce
    /// nothing for it — not an error, not a false "missing" entry.
    #[test]
    fn cursor_produces_no_hook_drift() {
        let dir = tempdir().unwrap();
        let hooks = vec![cmd_hook("cursor", "ping", "sessionStart", "echo hi")];

        let mut files: Vec<DriftedFile> = Vec::new();
        collect_hooks_drift(&Cursor, &dir.path().to_path_buf(), &hooks, &mut files);

        assert!(
            files.is_empty(),
            "Cursor hooks are out of scope for this collector, got: {:?}",
            files
                .iter()
                .map(|f| format!("{} ({})", f.path, f.reason))
                .collect::<Vec<_>>()
        );
    }
}
