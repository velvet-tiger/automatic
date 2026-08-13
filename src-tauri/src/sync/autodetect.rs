use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use crate::agent;
use crate::core::{CustomSkill, Project};

use super::helpers::{add_unique, prune_shadowed_custom_skills};

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

    // Prune any `custom_skills` entries whose name is already in the project's
    // library-backed `skills` list.  Such entries are stale snapshots from
    // before the skill was promoted to the library and break drift detection.
    prune_shadowed_custom_skills(&mut updated_project);

    // Detect which agents are present by asking each agent to check.
    //
    // If the project already has an explicit agent list (set by the user in the
    // UI), restrict additions to agents that are both (a) detected in the
    // directory and (b) already in the user's selection.  This prevents
    // autodetect from silently adding agents the user never chose.
    //
    // For a truly blank project (agents list is empty) we fall back to adding
    // everything that is detected, giving a useful starting point.
    let user_agents: std::collections::HashSet<&str> =
        project.agents.iter().map(|s| s.as_str()).collect();
    let has_explicit_agents = !user_agents.is_empty();

    for a in agent::all() {
        if a.detect_in(&dir) {
            // Only add if the user has no preference yet, or if this agent
            // is already in the user's explicit selection.
            if !has_explicit_agents || user_agents.contains(a.id()) {
                add_unique(&mut updated_project.agents, a.id());
            }
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

    // Track names that are already accounted for as project-scoped custom skills,
    // so the same on-disk SKILL.md does not get imported twice if it appears in
    // more than one agent's skill_dirs (e.g. .claude/skills + .agents/skills).
    let mut existing_custom_names: HashSet<String> = updated_project
        .custom_skills
        .as_ref()
        .map(|skills| skills.iter().map(|s| s.name.clone()).collect())
        .unwrap_or_default();

    for skill_base_dir in &skill_dirs {
        if !skill_base_dir.exists() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(skill_base_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                let skill_file = path.join("SKILL.md");
                if !skill_file.exists() || !crate::core::is_valid_name(name) {
                    continue;
                }

                if global_skill_names.contains(name) {
                    // Skill exists in the global registry — track it as a
                    // normal (library-backed) project skill. The library copy
                    // is the source of truth for content.
                    add_unique(&mut updated_project.skills, name);
                    continue;
                }

                if updated_project.skills.iter().any(|s| s == name)
                    || existing_custom_names.contains(name)
                {
                    continue;
                }

                // Project-scoped skill: read SKILL.md content from disk and
                // promote it to a `custom_skill` so the content is portable in
                // the project JSON and surfaces in the Skills UI.
                let Ok(content) = fs::read_to_string(&skill_file) else {
                    continue;
                };
                let custom = CustomSkill {
                    name: name.to_string(),
                    content,
                };
                updated_project
                    .custom_skills
                    .get_or_insert_with(Vec::new)
                    .push(custom);
                existing_custom_names.insert(name.to_string());
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
                // Case-insensitive: the registry treats `Sentry`/`sentry` as one
                // server, so a re-discovered variant must not become a duplicate.
                if !crate::core::contains_ignore_ascii_case(&updated_project.mcp_servers, &name) {
                    updated_project.mcp_servers.push(name.clone());
                }
                discovered_servers.push((name, config_str));
            }
        }
    }

    // ── Detect tools declared by enabled plugins ─────────────────────────────
    //
    // Tools with `project_scoped: false` are skipped — a binary on PATH only
    // proves the machine-wide feature is installed, not that it belongs on
    // this project.
    //
    // Detection precedence (first match wins):
    //   1. detect_dir set  → present only if <project_dir>/<detect_dir> exists.
    //      The directory is the canonical "initialised in this project" signal;
    //      a binary on PATH is machine-level, not project-level.
    //   2. detect_dir unset, detect_binary set → present if binary is on PATH.
    //   3. Neither set → never auto-detected.
    //
    // No code from the tool itself is read or executed.
    if let Ok(tool_names) = crate::core::tools::list_tools() {
        for tool_name in &tool_names {
            if let Ok(raw) = crate::core::tools::read_tool(tool_name) {
                if let Ok(tool) = serde_json::from_str::<crate::core::tools::ToolDefinition>(&raw) {
                    if !tool.project_scoped {
                        continue;
                    }

                    let present = match tool.detect_dir.as_deref() {
                        Some(rel) => dir.join(rel).exists(),
                        None => tool
                            .detect_binary
                            .as_deref()
                            .map(crate::core::tools::which_binary)
                            .unwrap_or(false),
                    };

                    if present {
                        add_unique(&mut updated_project.tools, tool_name.as_str());
                    }
                }
            }
        }
    }

    Ok((updated_project, discovered_servers))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn autodetect_only_adds_claude_for_projects_with_only_claude_md() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("CLAUDE.md"), "This is the claude md file")
            .expect("write CLAUDE.md");

        let project = Project {
            name: "test-project".to_string(),
            directory: dir.path().display().to_string(),
            ..Default::default()
        };

        let (updated, discovered_servers) = autodetect_inner(&project).expect("autodetect");

        assert_eq!(
            updated.agents,
            vec!["claude".to_string()],
            "a project with only CLAUDE.md should detect only Claude"
        );
        assert!(
            discovered_servers.is_empty(),
            "plain CLAUDE.md should not imply any MCP server configs"
        );
    }

    #[test]
    fn project_local_skill_in_dot_agents_skills_promoted_to_custom_skill() {
        // A SKILL.md found in <project>/.agents/skills/<name> that is not in the
        // global library is promoted to a `custom_skill` with content read from
        // disk, so the Skills UI surfaces it under "Project Skills".
        let dir = tempdir().expect("tempdir");
        let skill_dir = dir.path().join(".agents").join("skills").join("my-local");
        fs::create_dir_all(&skill_dir).expect("mkdir");
        fs::write(skill_dir.join("SKILL.md"), "# Local skill body").expect("write");

        let project = Project {
            name: "p".into(),
            directory: dir.path().display().to_string(),
            ..Default::default()
        };

        let (updated, _) = autodetect_inner(&project).expect("autodetect");

        let custom = updated.custom_skills.expect("custom_skills populated");
        let entry = custom
            .iter()
            .find(|s| s.name == "my-local")
            .expect("my-local in custom_skills");
        assert_eq!(entry.content, "# Local skill body");
    }

    #[test]
    fn autodetect_prunes_custom_skills_now_in_library() {
        // A custom_skills entry whose name also appears in the project's
        // library-backed `skills` list must be pruned during autodetect.
        // The library version is the source of truth; the stale custom_skills
        // snapshot breaks drift detection.
        use crate::core::{save_skill, with_test_home, CustomSkill};

        let home = tempdir().expect("home tempdir");
        let dir = tempdir().expect("project tempdir");

        with_test_home(home.path().to_path_buf(), || {
            save_skill("promoted", "# Library version").expect("save_skill");

            let project = Project {
                name: "p".into(),
                directory: dir.path().display().to_string(),
                skills: vec!["promoted".into()],
                custom_skills: Some(vec![
                    CustomSkill {
                        name: "promoted".into(),
                        content: "# Stale snapshot".into(),
                    },
                    CustomSkill {
                        name: "still-local".into(),
                        content: "# Genuinely project-only".into(),
                    },
                ]),
                ..Default::default()
            };

            let (updated, _) = autodetect_inner(&project).expect("autodetect");

            let custom = updated
                .custom_skills
                .expect("custom_skills should retain still-local");
            assert!(
                custom.iter().all(|s| s.name != "promoted"),
                "stale custom_skill for library-backed name should be pruned"
            );
            assert!(
                custom.iter().any(|s| s.name == "still-local"),
                "project-only custom_skill must be preserved"
            );
        });
    }

    #[test]
    fn discovered_mcp_server_does_not_duplicate_existing_case_variant() {
        // Regression: the project already selects `sentry` (e.g. added lowercased
        // from the library), and the agent's own `.mcp.json` declares it as
        // `Sentry`. Discovery preserves the file's casing, but the registry
        // treats the two as one server, so the project list must not end up with
        // both.
        let dir = tempdir().expect("tempdir");
        fs::write(
            dir.path().join(".mcp.json"),
            r#"{"mcpServers":{"Sentry":{"command":"npx","args":["-y","sentry-mcp"]}}}"#,
        )
        .expect("write .mcp.json");

        let project = Project {
            name: "p".into(),
            directory: dir.path().display().to_string(),
            mcp_servers: vec!["sentry".into()],
            ..Default::default()
        };

        let (updated, _) = autodetect_inner(&project).expect("autodetect");

        let matches: Vec<&String> = updated
            .mcp_servers
            .iter()
            .filter(|s| s.eq_ignore_ascii_case("sentry"))
            .collect();
        assert_eq!(
            matches,
            vec![&"sentry".to_string()],
            "re-discovering `Sentry` must not add a second entry alongside `sentry`, got {:?}",
            updated.mcp_servers
        );
    }

    #[test]
    fn skill_seen_in_multiple_agent_dirs_only_promoted_once() {
        // The same SKILL.md may appear at .claude/skills/<name> AND
        // .agents/skills/<name> (e.g. a symlink). It should be added to
        // custom_skills exactly once.
        let dir = tempdir().expect("tempdir");
        let claude_skill = dir.path().join(".claude").join("skills").join("dup");
        let agents_skill = dir.path().join(".agents").join("skills").join("dup");
        fs::create_dir_all(&claude_skill).expect("mkdir claude");
        fs::create_dir_all(&agents_skill).expect("mkdir agents");
        fs::write(claude_skill.join("SKILL.md"), "# dup").expect("write claude");
        fs::write(agents_skill.join("SKILL.md"), "# dup").expect("write agents");

        let project = Project {
            name: "p".into(),
            directory: dir.path().display().to_string(),
            ..Default::default()
        };

        let (updated, _) = autodetect_inner(&project).expect("autodetect");

        let count = updated
            .custom_skills
            .as_ref()
            .map(|s| s.iter().filter(|s| s.name == "dup").count())
            .unwrap_or(0);
        assert_eq!(count, 1, "duplicate skill should only be added once");
    }
}
