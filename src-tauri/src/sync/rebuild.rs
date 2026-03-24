use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::agent;
use crate::core::{self, CustomAgent, Project, UserAgent};

use super::autodetect::autodetect_inner;

struct GlobalUserAgent {
    id: String,
    content: String,
}

pub fn rebuild_project_state(project: &Project) -> Result<Project, String> {
    let mut seed = project.clone();
    seed.agents.clear();
    seed.skills.clear();
    seed.local_skills.clear();
    seed.mcp_servers.clear();
    seed.tools.clear();
    seed.user_agents.clear();
    seed.custom_agents = None;

    let (mut rebuilt, discovered_servers) = autodetect_inner(&seed)?;

    if project.mcp_servers.iter().any(|name| name == "automatic")
        && !rebuilt.mcp_servers.iter().any(|name| name == "automatic")
    {
        rebuilt.mcp_servers.push("automatic".to_string());
    }

    for (name, config_str) in discovered_servers {
        let _ = core::save_mcp_server_config(&name, &config_str);
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

        let ext = agent_instance.agents_file_ext();
        let entries = match fs::read_dir(&agents_dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() || !path.extension().is_some_and(|value| value == ext) {
                continue;
            }

            let raw = match fs::read_to_string(&path) {
                Ok(raw) => raw,
                Err(_) => continue,
            };

            if let Some(global_id) =
                match_global_user_agent(agent_instance, &raw, &global_user_agents)
            {
                user_agent_ids.insert(global_id);
                continue;
            }

            let fallback_id = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("custom-agent");
            if let Some(custom_agent) = parse_custom_agent(agent_instance, &raw, fallback_id) {
                let custom_id = extract_agent_machine_name(&custom_agent.content)
                    .unwrap_or_else(|| fallback_id.to_string());
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

fn load_global_user_agents() -> Result<Vec<GlobalUserAgent>, String> {
    let mut result = Vec::new();
    for entry in core::list_user_agents()? {
        let raw = match core::read_user_agent(&entry.id) {
            Ok(raw) => raw,
            Err(_) => continue,
        };
        let user_agent = match serde_json::from_str::<UserAgent>(&raw) {
            Ok(user_agent) => user_agent,
            Err(_) => continue,
        };
        result.push(GlobalUserAgent {
            id: entry.id,
            content: user_agent.content,
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

fn parse_custom_agent(
    agent_instance: &dyn agent::Agent,
    raw_disk_content: &str,
    fallback_id: &str,
) -> Option<CustomAgent> {
    let canonical_content = match agent_instance.agents_file_ext() {
        "md" => raw_disk_content.to_string(),
        "toml" => convert_codex_toml_to_markdown(raw_disk_content, fallback_id)?,
        _ => return None,
    };

    let name = extract_frontmatter_value(&canonical_content, "name")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback_id.to_string());

    Some(CustomAgent {
        name,
        content: canonical_content,
    })
}

fn extract_frontmatter_value(content: &str, key: &str) -> Option<String> {
    if !content.starts_with("---\n") {
        return None;
    }
    let end = content[4..].find("\n---")?;
    let yaml = &content[4..end + 4];
    for line in yaml.lines() {
        let line = line.trim();
        let prefix = format!("{}:", key);
        if let Some(value) = line.strip_prefix(&prefix) {
            let value = value.trim().trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn extract_agent_machine_name(content: &str) -> Option<String> {
    extract_frontmatter_value(content, "name").map(|value| value.to_lowercase().replace(' ', "-"))
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

    let mut frontmatter = vec![
        "---".to_string(),
        format!("name: {}", name),
        format!("description: {}", description),
        format!("model: {}", model),
        "tools: inherit".to_string(),
    ];

    if let Some(max_turns) = max_turns {
        frontmatter.push(format!("maxTurns: {}", max_turns));
    }
    if let Some(reasoning) = reasoning {
        frontmatter.push(format!("modelReasoningEffort: {}", reasoning));
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
}
