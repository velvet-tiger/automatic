use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::agent;
use crate::core::{self, CustomAgent, CustomCommand, Project, Subagent};

use super::autodetect::{autodetect_inner, AgentAdoption};
use super::read_project_text;

struct GlobalUserAgent {
    id: String,
    content: String,
}

pub fn rebuild_project_state(project: &Project) -> Result<Project, String> {
    // The agent list is kept as configured.  Rebuild re-derives what those
    // agents have on disk; to rebuild another agent the user adds it first.
    let mut seed = project.clone();
    seed.skills.clear();
    seed.mcp_servers.clear();
    seed.tools.clear();
    seed.user_agents.clear();
    seed.custom_agents = None;
    seed.user_commands.clear();
    seed.custom_commands = None;
    // Project-scoped skills are rediscovered from disk by autodetect, which
    // reads each SKILL.md and populates `custom_skills`. Clearing them here
    // makes rebuild authoritatively reflect what is on disk right now.
    seed.custom_skills = None;

    let (mut rebuilt, discovered_servers) = autodetect_inner(&seed, AgentAdoption::KeepSelected)?;

    if project.mcp_servers.iter().any(|name| name == "automatic")
        && !rebuilt.mcp_servers.iter().any(|name| name == "automatic")
    {
        rebuilt.mcp_servers.push("automatic".to_string());
    }

    for (name, config_str) in discovered_servers {
        let _ = core::add_discovered_mcp_server_config(&name, &config_str);
    }

    rebuilt.disabled_mcp_servers = project
        .disabled_mcp_servers
        .iter()
        .filter(|name| rebuilt.mcp_servers.contains(*name))
        .cloned()
        .collect();

    if rebuilt.directory.is_empty() || !Path::new(&rebuilt.directory).exists() {
        rebuilt.updated_at = chrono::Utc::now().to_rfc3339();
        return Ok(rebuilt);
    }

    let (user_agents, custom_agents) = discover_sub_agents(&rebuilt)?;
    rebuilt.user_agents = user_agents;
    rebuilt.custom_agents = if custom_agents.is_empty() {
        None
    } else {
        Some(custom_agents)
    };

    let (user_commands, custom_commands) = discover_sub_commands(&rebuilt)?;
    rebuilt.user_commands = user_commands;
    rebuilt.custom_commands = if custom_commands.is_empty() {
        None
    } else {
        Some(custom_commands)
    };

    rebuilt.updated_at = chrono::Utc::now().to_rfc3339();

    Ok(rebuilt)
}

fn discover_sub_agents(project: &Project) -> Result<(Vec<String>, Vec<CustomAgent>), String> {
    let project_dir = Path::new(&project.directory);
    let global_user_agents = load_global_user_agents()?;

    let mut user_agent_ids = HashSet::new();
    let mut custom_agents = Vec::new();
    let mut seen_custom_agent_ids = HashSet::new();

    for agent_id in &project.agents {
        let Some(agent_instance) = agent::from_id(agent_id) else {
            continue;
        };
        let Some(agents_dir) = agent_instance.agents_dir(project_dir) else {
            continue;
        };
        if !agents_dir.exists() {
            continue;
        }

        let entries = match fs::read_dir(&agents_dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for (path, fallback_id) in sub_agent_candidates(agent_instance, entries) {
            let fallback_id = fallback_id.as_str();
            let Some(raw) = read_discovered_file("sub-agent", project_dir, &path) else {
                continue;
            };

            if let Some(global_id) =
                match_global_user_agent(agent_instance, &raw, &global_user_agents)
            {
                user_agent_ids.insert(global_id);
                continue;
            }

            if let Some(custom_agent) = parse_custom_agent(agent_instance, &raw, fallback_id) {
                // A cloned repo controls the frontmatter. Adopting a name that
                // cannot be a file name would leave a project that fails to load.
                let custom_id = match crate::core::custom_agent_machine_name(&custom_agent) {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("[automatic] rebuild: not adopting {}: {e}", path.display());
                        continue;
                    }
                };
                if seen_custom_agent_ids.insert(custom_id) {
                    custom_agents.push(custom_agent);
                }
            }
        }
    }

    let mut user_agents: Vec<String> = user_agent_ids.into_iter().collect();
    user_agents.sort();
    custom_agents.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok((user_agents, custom_agents))
}

/// The sub-agent files in one agent directory, each with the machine name
/// its file name gives, in the order discovery should take them.
///
/// Copilot's `planner.md` and `planner.agent.md` both name `planner`, and
/// discovery keeps the first file per name.  `read_dir` order differs
/// between machines, so the files are sorted: the form Automatic writes
/// ([`agent::Agent::agent_file_name`]) first, then by file name.
fn sub_agent_candidates(
    agent_instance: &dyn agent::Agent,
    entries: fs::ReadDir,
) -> Vec<(std::path::PathBuf, String)> {
    let mut candidates: Vec<(bool, String, std::path::PathBuf, String)> = entries
        .flatten()
        .filter(|entry| !entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            let file_name = entry.file_name().into_string().ok()?;
            // Strip the agent's whole file-name suffix: `file_stem` would
            // name Copilot's `planner.agent.md` `planner.agent`.
            let machine_name =
                agent::agent_machine_name_from_file_name(agent_instance, &file_name)?.to_string();
            let not_written_form = file_name != agent_instance.agent_file_name(&machine_name);
            Some((not_written_form, file_name, entry.path(), machine_name))
        })
        .collect();
    candidates.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    candidates
        .into_iter()
        .map(|(_, _, path, machine_name)| (path, machine_name))
        .collect()
}

/// Read a file found in an agent's directory during rebuild, or `None` to
/// skip it.  What is read here is imported into the project's committed
/// config, so the read goes through [`read_project_text`]: a committed link
/// out of the project (`notes.md -> ~/.ssh/id_rsa`), a FIFO or an oversized
/// file is refused and logged, never imported.
fn read_discovered_file(kind: &str, project_dir: &Path, path: &Path) -> Option<String> {
    match read_project_text(project_dir, path) {
        Ok(content) => content,
        Err(reason) => {
            eprintln!("[automatic] rebuild: not importing {kind} file: {reason}");
            None
        }
    }
}

struct GlobalUserCommand {
    id: String,
    content: String,
}

fn discover_sub_commands(project: &Project) -> Result<(Vec<String>, Vec<CustomCommand>), String> {
    let project_dir = Path::new(&project.directory);
    let global_user_commands = load_global_user_commands()?;

    let mut user_command_ids = HashSet::new();
    let mut custom_commands = Vec::new();
    let mut seen_custom_command_names = HashSet::new();

    for agent_id in &project.agents {
        let Some(agent_instance) = agent::from_id(agent_id) else {
            continue;
        };
        let Some(commands_dir) = agent_instance.commands_dir(project_dir) else {
            continue;
        };
        if !commands_dir.exists() {
            continue;
        }

        let entries = match fs::read_dir(&commands_dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let path = entry.path();
            // Strip the agent's whole file-name suffix: `file_stem` would
            // name Copilot's `planner.prompt.md` `planner.prompt`.  Checking
            // before the read also keeps unrelated files out of the refusal
            // log.
            let Some(command_name) = path
                .file_name()
                .and_then(|value| value.to_str())
                .and_then(|name| agent::command_machine_name_from_file_name(agent_instance, name))
                .map(str::to_string)
            else {
                continue;
            };

            let Some(raw) = read_discovered_file("command", project_dir, &path) else {
                continue;
            };
            if !agent::is_managed_command_content(&path, &raw) {
                continue;
            }

            if let Some(global_id) =
                match_global_user_command(agent_instance, &raw, &global_user_commands)
            {
                user_command_ids.insert(global_id);
                continue;
            }

            // Managed file that doesn't match any global command — treat as custom.
            if !crate::core::is_valid_asset_name(&command_name) {
                eprintln!(
                    "[automatic] rebuild: not adopting {}: its name cannot be used as a file name",
                    path.display()
                );
                continue;
            }

            // Strip the agent-specific rendering to recover canonical content.
            // For most agents the content is already canonical Markdown.
            if seen_custom_command_names.insert(command_name.clone()) {
                custom_commands.push(CustomCommand {
                    name: command_name,
                    content: raw,
                });
            }
        }
    }

    let mut user_commands: Vec<String> = user_command_ids.into_iter().collect();
    user_commands.sort();
    custom_commands.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok((user_commands, custom_commands))
}

fn load_global_user_commands() -> Result<Vec<GlobalUserCommand>, String> {
    let mut result = Vec::new();
    for entry in core::list_user_commands()? {
        let content = match core::read_user_command(&entry.id) {
            Ok(content) => content,
            Err(_) => continue,
        };
        result.push(GlobalUserCommand {
            id: entry.id,
            content,
        });
    }
    Ok(result)
}

fn match_global_user_command(
    agent_instance: &dyn agent::Agent,
    raw_disk_content: &str,
    global_user_commands: &[GlobalUserCommand],
) -> Option<String> {
    for global_cmd in global_user_commands {
        let expected = agent_instance.convert_command_content(&global_cmd.content, &global_cmd.id);
        if expected == raw_disk_content {
            return Some(global_cmd.id.clone());
        }
    }
    None
}

fn load_global_user_agents() -> Result<Vec<GlobalUserAgent>, String> {
    let mut result = Vec::new();
    for entry in core::list_subagents()? {
        let raw = match core::read_subagent(&entry.id) {
            Ok(raw) => raw,
            Err(_) => continue,
        };
        let subagent = match serde_json::from_str::<Subagent>(&raw) {
            Ok(subagent) => subagent,
            Err(_) => continue,
        };
        result.push(GlobalUserAgent {
            id: entry.id,
            content: subagent.content,
        });
    }
    Ok(result)
}

fn match_global_user_agent(
    agent_instance: &dyn agent::Agent,
    raw_disk_content: &str,
    global_user_agents: &[GlobalUserAgent],
) -> Option<String> {
    for global_agent in global_user_agents {
        let expected =
            agent_instance.convert_agent_content(&global_agent.content, &global_agent.id);
        if expected == raw_disk_content {
            return Some(global_agent.id.clone());
        }
    }
    None
}

/// Turn a sub-agent file in `agent_instance`'s own format back into the
/// canonical Markdown a [`CustomAgent`] stores: the reverse of
/// [`agent::Agent::convert_agent_content`].  `fallback_id` names the agent
/// when the file does not.  Returns `None` for a format with no reverse
/// conversion (Kiro's JSON) and for a file that does not parse.
fn agent_file_to_markdown(
    agent_instance: &dyn agent::Agent,
    raw_disk_content: &str,
    fallback_id: &str,
) -> Option<String> {
    match agent_instance.agents_file_ext() {
        "md" => Some(raw_disk_content.to_string()),
        "toml" => convert_codex_toml_to_markdown(raw_disk_content, fallback_id),
        _ => None,
    }
}

fn parse_custom_agent(
    agent_instance: &dyn agent::Agent,
    raw_disk_content: &str,
    fallback_id: &str,
) -> Option<CustomAgent> {
    let canonical_content = agent_file_to_markdown(agent_instance, raw_disk_content, fallback_id)?;

    let name = crate::core::extract_agent_name(&canonical_content).unwrap_or_else(|| fallback_id.to_string());

    Some(CustomAgent {
        name,
        content: canonical_content,
    })
}

fn convert_codex_toml_to_markdown(content: &str, fallback_id: &str) -> Option<String> {
    let value = toml::from_str::<toml::Value>(content).ok()?;
    let table = value.as_table()?;

    let name = table
        .get("name")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(fallback_id);
    let description = table
        .get("description")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let model = table
        .get("model")
        .and_then(|value| value.as_str())
        .unwrap_or("inherit");
    let max_turns = table.get("max_turns").and_then(|value| value.as_integer());
    let reasoning = table
        .get("model_reasoning_effort")
        .and_then(|value| value.as_str());
    let instructions = table
        .get("developer_instructions")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    // Rebuild has no round-trip check, so a value the gate refuses skips the
    // whole file rather than storing something other than it says.
    let line = |key: &str, value: &str| match agent::render_frontmatter_value(value) {
        Some(rendered) => Some(format!("{key}: {rendered}")),
        None => {
            eprintln!(
                "[automatic] rebuild: not importing sub-agent '{fallback_id}': its {key} cannot be stored as frontmatter"
            );
            None
        }
    };

    let mut frontmatter = vec![
        "---".to_string(),
        line("name", name)?,
        line("description", description)?,
        line("model", model)?,
    ];

    // `convert_md_to_codex_toml` writes `sandbox_mode = "read-only"` whenever
    // `tools` is present.  Emitting `tools` for any other file would silently
    // tighten its permissions, and the adopt guard would refuse it.
    let read_only =
        table.get("sandbox_mode").and_then(|value| value.as_str()) == Some("read-only");
    if read_only {
        frontmatter.push("tools: inherit".to_string());
    }

    if let Some(max_turns) = max_turns {
        frontmatter.push(format!("maxTurns: {}", max_turns));
    }
    if let Some(reasoning) = reasoning {
        frontmatter.push(line("modelReasoningEffort", reasoning)?);
    }
    frontmatter.push("---".to_string());

    let mut markdown = frontmatter.join("\n");
    markdown.push_str("\n\n");
    markdown.push_str(instructions.trim());
    markdown.push('\n');
    Some(markdown)
}

pub fn rebuild_instruction_snapshots(project: &Project) {
    if project.directory.is_empty() {
        return;
    }

    let mut seen = HashSet::new();
    for agent_id in &project.agents {
        let Some(agent_instance) = agent::from_id(agent_id) else {
            continue;
        };
        if !agent_instance.capabilities().instructions {
            continue;
        }

        let filename = agent_instance.project_file_name().to_string();
        if !seen.insert(filename.clone()) {
            continue;
        }

        if let Ok(user_content) = core::read_project_file(&project.directory, &filename) {
            let _ = core::save_instruction_snapshot(&project.directory, &filename, &user_content);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, ClaudeCode};
    use std::fs;
    use tempfile::tempdir;

    fn make_project(dir: &std::path::Path) -> Project {
        Project {
            name: "test-project".into(),
            directory: dir.to_string_lossy().into(),
            agents: vec!["claude".into()],
            ..Default::default()
        }
    }

    #[test]
    fn discover_commands_empty_dir_returns_empty() {
        let project_dir = tempdir().unwrap();
        let project = make_project(project_dir.path());

        let (user_cmds, custom_cmds) = discover_sub_commands(&project).unwrap();
        assert!(user_cmds.is_empty());
        assert!(custom_cmds.is_empty());
    }

    #[test]
    fn discover_commands_finds_managed_command_as_custom() {
        let project_dir = tempdir().unwrap();
        let commands_dir = ClaudeCode.commands_dir(project_dir.path()).unwrap();
        fs::create_dir_all(&commands_dir).unwrap();

        // Write a managed command file (as sync_commands_to_dir would produce)
        let content = "---\nautomatic-managed: true\n---\nDo the thing.\n";
        fs::write(commands_dir.join("my-command.md"), content).unwrap();

        let project = make_project(project_dir.path());
        let (user_cmds, custom_cmds) = discover_sub_commands(&project).unwrap();

        // No global registry match, so it appears as a custom command
        assert!(user_cmds.is_empty());
        assert_eq!(custom_cmds.len(), 1);
        assert_eq!(custom_cmds[0].name, "my-command");
        assert_eq!(custom_cmds[0].content, content);
    }

    #[test]
    fn discover_agents_skips_a_traversal_frontmatter_name() {
        let project_dir = tempdir().unwrap();
        let agents_dir = ClaudeCode.agents_dir(project_dir.path()).unwrap();
        fs::create_dir_all(&agents_dir).unwrap();
        fs::write(
            agents_dir.join("evil.md"),
            "---\nname: ../../../evil\n---\nEscape.\n",
        )
        .unwrap();
        fs::write(agents_dir.join("good.md"), "---\nname: Good\n---\nFine.\n").unwrap();

        let project = make_project(project_dir.path());
        let (_, custom_agents) = discover_sub_agents(&project).unwrap();

        let names: Vec<&str> = custom_agents.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["Good"]);
    }

    #[test]
    fn discover_commands_skips_unmanaged_files() {
        let project_dir = tempdir().unwrap();
        let commands_dir = ClaudeCode.commands_dir(project_dir.path()).unwrap();
        fs::create_dir_all(&commands_dir).unwrap();

        // Write a command file WITHOUT automatic-managed marker
        let content = "---\ndescription: user-created\n---\nHello world.\n";
        fs::write(commands_dir.join("user-cmd.md"), content).unwrap();

        let project = make_project(project_dir.path());
        let (user_cmds, custom_cmds) = discover_sub_commands(&project).unwrap();

        assert!(user_cmds.is_empty());
        assert!(custom_cmds.is_empty());
    }

    #[test]
    fn match_global_user_command_matches_rendered_content() {
        let source_content = "---\ndescription: greet\n---\nSay hello.\n";
        let rendered = ClaudeCode.convert_command_content(source_content, "greet");

        let globals = vec![GlobalUserCommand {
            id: "greet".into(),
            content: source_content.into(),
        }];

        let matched = match_global_user_command(&ClaudeCode, &rendered, &globals);
        assert_eq!(matched, Some("greet".into()));
    }

    #[test]
    fn match_global_user_command_returns_none_for_different_content() {
        let globals = vec![GlobalUserCommand {
            id: "greet".into(),
            content: "---\ndescription: greet\n---\nSay hello.\n".into(),
        }];

        let disk_content = "---\nautomatic-managed: true\n---\nDifferent content.\n";
        let matched = match_global_user_command(&ClaudeCode, disk_content, &globals);
        assert_eq!(matched, None);
    }

    #[test]
    fn discover_commands_deduplicates_across_agents() {
        let project_dir = tempdir().unwrap();
        let commands_dir = ClaudeCode.commands_dir(project_dir.path()).unwrap();
        fs::create_dir_all(&commands_dir).unwrap();

        let content = "---\nautomatic-managed: true\n---\nDo the thing.\n";
        fs::write(commands_dir.join("my-command.md"), content).unwrap();

        // Project references claude-code twice (shouldn't happen, but tests dedup)
        let mut project = make_project(project_dir.path());
        project.agents.push("claude-code".into());

        let (_, custom_cmds) = discover_sub_commands(&project).unwrap();
        assert_eq!(custom_cmds.len(), 1, "same command should not appear twice");
    }

    #[test]
    fn discover_commands_sorts_results() {
        let project_dir = tempdir().unwrap();
        let commands_dir = ClaudeCode.commands_dir(project_dir.path()).unwrap();
        fs::create_dir_all(&commands_dir).unwrap();

        let content = "---\nautomatic-managed: true\n---\n";
        fs::write(commands_dir.join("zeta-cmd.md"), content).unwrap();
        fs::write(commands_dir.join("alpha-cmd.md"), content).unwrap();

        let project = make_project(project_dir.path());
        let (_, custom_cmds) = discover_sub_commands(&project).unwrap();

        assert_eq!(custom_cmds.len(), 2);
        assert_eq!(custom_cmds[0].name, "alpha-cmd");
        assert_eq!(custom_cmds[1].name, "zeta-cmd");
    }

    #[test]
    fn converts_codex_toml_to_canonical_markdown() {
        let toml = "name = \"Research Helper\"\n\
description = \"Looks through code\"\n\
model = \"gpt-5.4\"\n\
max_turns = 12\n\
model_reasoning_effort = \"high\"\n\n\
developer_instructions = \"\"\"\n\
Inspect the repo carefully.\n\
\"\"\"\n";

        let markdown =
            convert_codex_toml_to_markdown(toml, "research-helper").expect("markdown conversion");

        assert!(markdown.contains("name: Research Helper"));
        assert!(markdown.contains("description: Looks through code"));
        assert!(markdown.contains("model: gpt-5.4"));
        assert!(markdown.contains("maxTurns: 12"));
        assert!(markdown.contains("modelReasoningEffort: high"));
        assert!(markdown.contains("Inspect the repo carefully."));
    }

    #[test]
    fn rebuild_keeps_the_configured_agents_only() {
        let home = tempdir().unwrap();
        let project_dir = tempdir().unwrap();
        fs::write(project_dir.path().join("CLAUDE.md"), "# Claude").unwrap();
        fs::create_dir_all(project_dir.path().join(".claude")).unwrap();

        crate::core::with_test_home(home.path().to_path_buf(), || {
            let cursor_only = Project {
                agents: vec!["cursor".into()],
                ..make_project(project_dir.path())
            };
            let rebuilt = rebuild_project_state(&cursor_only).unwrap();
            assert_eq!(rebuilt.agents, vec!["cursor".to_string()]);

            let no_agents = Project {
                agents: vec![],
                ..make_project(project_dir.path())
            };
            let rebuilt = rebuild_project_state(&no_agents).unwrap();
            assert!(rebuilt.agents.is_empty(), "rebuild must not adopt detected agents");
        });
    }

    /// Copilot loads both `planner.agent.md` and `planner.md` from
    /// `.github/agents/`, named after the file minus either suffix.  A file
    /// without a `name:` field must be named `planner`, not `planner.agent`.
    #[test]
    fn discover_agents_strips_both_copilot_suffixes() {
        let home = tempdir().unwrap();
        let project_dir = tempdir().unwrap();
        let agents_dir = project_dir.path().join(".github").join("agents");
        fs::create_dir_all(&agents_dir).unwrap();
        fs::write(
            agents_dir.join("planner.agent.md"),
            "---\ndescription: Plans\n---\nPlan.\n",
        )
        .unwrap();
        fs::write(
            agents_dir.join("helper.md"),
            "---\ndescription: Helps\n---\nHelp.\n",
        )
        .unwrap();

        crate::core::with_test_home(home.path().to_path_buf(), || {
            let project = Project {
                agents: vec!["copilot".into()],
                ..make_project(project_dir.path())
            };
            let (user_agents, custom_agents) = discover_sub_agents(&project).unwrap();
            assert!(user_agents.is_empty());
            let names: Vec<&str> = custom_agents.iter().map(|a| a.name.as_str()).collect();
            assert_eq!(names, vec!["helper", "planner"]);
        });
    }

    /// `planner.md` and `planner.agent.md` both name `planner` for Copilot.
    /// Discovery must take the form Automatic writes on every machine,
    /// whatever order `read_dir` returns.
    #[test]
    fn discover_agents_prefers_the_written_copilot_file() {
        let home = tempdir().unwrap();
        let project_dir = tempdir().unwrap();
        let agents_dir = project_dir.path().join(".github").join("agents");
        fs::create_dir_all(&agents_dir).unwrap();
        let written = "---\ndescription: Written\n---\nWritten form.\n";
        fs::write(agents_dir.join("planner.agent.md"), written).unwrap();
        fs::write(
            agents_dir.join("planner.md"),
            "---\ndescription: Plain\n---\nPlain form.\n",
        )
        .unwrap();

        crate::core::with_test_home(home.path().to_path_buf(), || {
            let project = Project {
                agents: vec!["copilot".into()],
                ..make_project(project_dir.path())
            };
            let (_, custom_agents) = discover_sub_agents(&project).unwrap();
            assert_eq!(custom_agents.len(), 1, "{custom_agents:?}");
            assert_eq!(custom_agents[0].name, "planner");
            assert_eq!(custom_agents[0].content, written);
        });
    }

    /// The written form wins even when another file sorts before it.
    /// `a.md` names itself `planner` and sorts before `planner.agent.md`,
    /// so only the written-form preference keeps `planner.agent.md`.
    #[test]
    fn discover_agents_prefers_the_written_form_over_name_order() {
        let home = tempdir().unwrap();
        let project_dir = tempdir().unwrap();
        let agents_dir = project_dir.path().join(".github").join("agents");
        fs::create_dir_all(&agents_dir).unwrap();
        let written = "---\nname: planner\ndescription: Written\n---\nWritten form.\n";
        fs::write(agents_dir.join("planner.agent.md"), written).unwrap();
        fs::write(
            agents_dir.join("a.md"),
            "---\nname: planner\ndescription: Other\n---\nOther file.\n",
        )
        .unwrap();

        crate::core::with_test_home(home.path().to_path_buf(), || {
            let project = Project {
                agents: vec!["copilot".into()],
                ..make_project(project_dir.path())
            };
            let (_, custom_agents) = discover_sub_agents(&project).unwrap();
            assert_eq!(custom_agents.len(), 1, "{custom_agents:?}");
            assert_eq!(custom_agents[0].content, written);
        });
    }

    /// Copilot names prompt files `{name}.prompt.md`.  A managed
    /// `planner.prompt.md` must be discovered as `planner`, or the next
    /// sync writes `planner.prompt.prompt.md`.
    #[test]
    fn discover_commands_strips_the_copilot_prompt_suffix() {
        let home = tempdir().unwrap();
        let project_dir = tempdir().unwrap();
        let prompts_dir = project_dir.path().join(".github").join("prompts");
        fs::create_dir_all(&prompts_dir).unwrap();
        fs::write(
            prompts_dir.join("planner.prompt.md"),
            "---\nautomatic-managed: true\n---\nPlan.\n",
        )
        .unwrap();

        crate::core::with_test_home(home.path().to_path_buf(), || {
            let project = Project {
                agents: vec!["copilot".into()],
                ..make_project(project_dir.path())
            };
            let (_, custom_commands) = discover_sub_commands(&project).unwrap();
            let names: Vec<&str> = custom_commands.iter().map(|c| c.name.as_str()).collect();
            assert_eq!(names, vec!["planner"]);
        });
    }

    /// Rebuild imports what it finds into committed config, so a committed
    /// link out of the project under an agents or commands directory must
    /// not be read.
    #[cfg(unix)]
    #[test]
    fn discover_skips_links_out_of_the_project() {
        let home = tempdir().unwrap();
        let project_dir = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let secret_agent = outside.path().join("agent.md");
        fs::write(&secret_agent, "---\nname: secret\n---\nSECRET\n").unwrap();
        let secret_command = outside.path().join("command.md");
        fs::write(&secret_command, "---\nautomatic-managed: true\n---\nSECRET\n").unwrap();

        let agents_dir = ClaudeCode.agents_dir(project_dir.path()).unwrap();
        fs::create_dir_all(&agents_dir).unwrap();
        std::os::unix::fs::symlink(&secret_agent, agents_dir.join("notes.md")).unwrap();
        let commands_dir = ClaudeCode.commands_dir(project_dir.path()).unwrap();
        fs::create_dir_all(&commands_dir).unwrap();
        std::os::unix::fs::symlink(&secret_command, commands_dir.join("notes.md")).unwrap();

        crate::core::with_test_home(home.path().to_path_buf(), || {
            let project = make_project(project_dir.path());
            let (_, custom_agents) = discover_sub_agents(&project).unwrap();
            assert!(custom_agents.is_empty(), "{custom_agents:?}");
            let (_, custom_commands) = discover_sub_commands(&project).unwrap();
            assert!(custom_commands.is_empty(), "{custom_commands:?}");
        });
    }

    /// `tools` converts back to `sandbox_mode = "read-only"`, so only a
    /// read-only file may carry it.
    #[test]
    fn codex_toml_gets_tools_only_when_read_only() {
        let plain = convert_codex_toml_to_markdown("name = \"planner\"\n", "planner").unwrap();
        assert!(!plain.contains("tools:"), "{plain}");

        let read_only = convert_codex_toml_to_markdown(
            "name = \"planner\"\nsandbox_mode = \"read-only\"\n",
            "planner",
        )
        .unwrap();
        assert!(read_only.contains("tools: inherit\n"), "{read_only}");

        let writable = convert_codex_toml_to_markdown(
            "name = \"planner\"\nsandbox_mode = \"workspace-write\"\n",
            "planner",
        )
        .unwrap();
        assert!(!writable.contains("tools:"), "{writable}");
    }

    /// A body with a backslash and `"""` survives TOML and back unchanged.
    #[test]
    fn codex_toml_round_trips_an_escaped_body() {
        let body = "Match \\d and \"\"\"quoted\"\"\".";
        let content = format!("---\nname: planner\ndescription: Plans\n---\n{body}\n");
        let rendered = agent::CodexCli.convert_agent_content(&content, "planner");

        let markdown = convert_codex_toml_to_markdown(&rendered, "planner").unwrap();

        let (_, round_tripped) = agent::parse_frontmatter(&markdown);
        assert_eq!(round_tripped.trim(), body);
    }

    /// A control character in the body survives TOML and back unchanged.
    #[test]
    fn codex_toml_round_trips_a_control_character() {
        let body = "Ring \u{7} once.";
        let content = format!("---\nname: planner\ndescription: Plans\n---\n{body}\n");
        let rendered = agent::CodexCli.convert_agent_content(&content, "planner");

        let markdown = convert_codex_toml_to_markdown(&rendered, "planner").unwrap();

        let (_, round_tripped) = agent::parse_frontmatter(&markdown);
        assert_eq!(round_tripped.trim(), body);
    }

    /// `parse_frontmatter` strips quotes and whitespace, so a value holding
    /// them must be quoted or the Markdown changes what the file said.
    #[test]
    fn codex_toml_quotes_values_the_frontmatter_parser_would_change() {
        let markdown = convert_codex_toml_to_markdown(
            "name = \"planner\"\ndescription = '\"Plans\"'\nmodel = \"gpt \"\n",
            "planner",
        )
        .unwrap();

        let (frontmatter, _) = agent::parse_frontmatter(&markdown);
        assert_eq!(frontmatter["description"], "\"Plans\"");
        assert_eq!(frontmatter["model"], "gpt ");
    }

    /// There is no round-trip check here, so a value that could add keys of
    /// its own or close the block skips the whole file.
    #[test]
    fn codex_toml_with_a_value_the_gate_refuses_is_skipped() {
        for value in ["x\\n---\\nfoo: bar", "a\\u2028b", "a\\u0085b"] {
            let toml = format!("name = \"planner\"\ndescription = \"{value}\"\n");
            assert_eq!(convert_codex_toml_to_markdown(&toml, "planner"), None, "{value}");
        }
    }

    #[test]
    fn name_extraction_reads_a_crlf_block_and_keeps_the_first_name() {
        let crlf = "---\r\nname: First One\r\nname: second\r\n---\r\nBody.\r\n";
        assert_eq!(core::extract_agent_machine_name(crlf).as_deref(), Some("first-one"));
        assert_eq!(core::extract_agent_name(crlf).as_deref(), Some("First One"));

        let lf = "---\nname:\nname: \"Second\"\n---\n";
        assert_eq!(core::extract_agent_name(lf).as_deref(), Some("Second"));
    }
}
