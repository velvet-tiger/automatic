use crate::core;

// ── User Agents (global registry) ─────────────────────────────────────────────

#[tauri::command]
pub fn get_user_agents() -> Result<Vec<core::UserAgentEntry>, String> {
    core::list_all_user_agents()
}

#[tauri::command]
pub fn read_user_agent(machine_name: String) -> Result<String, String> {
    core::read_user_agent(&machine_name)
}

#[tauri::command]
pub fn save_user_agent(machine_name: String, name: String, content: String) -> Result<(), String> {
    // Codex agents are read-only
    if machine_name.starts_with("codex-") && machine_name.ends_with("-openai") {
        return Err("Cannot modify Codex OpenAI agents. Duplicate to create a local copy.".into());
    }

    // Validate machine name format
    if !core::is_valid_agent_machine_name(&machine_name) {
        return Err(
            "Invalid agent machine name. Use lowercase letters, digits, and hyphens only.".into(),
        );
    }

    // Validate that the content includes valid frontmatter with a name
    if !content.trim().starts_with("---\n") {
        return Err("Agent content must start with YAML frontmatter (---)".into());
    }

    core::save_user_agent(&machine_name, &name, &content)
}

#[tauri::command]
pub fn delete_user_agent(machine_name: String) -> Result<(), String> {
    // Codex agents cannot be deleted
    if machine_name.starts_with("codex-") && machine_name.ends_with("-openai") {
        return Err("Cannot delete Codex OpenAI agents.".into());
    }

    core::delete_user_agent(&machine_name)
}

/// Return all projects that reference a user agent.
/// For sub-agents (project-local agents), this returns projects that have
/// the agent in their `custom_agents` list.
#[tauri::command]
pub fn get_projects_referencing_user_agent(
    agent_machine_name: String,
) -> Result<Vec<core::ProjectRef>, String> {
    let projects = core::list_projects()?;
    let mut referencing = Vec::new();

    for project_name in projects {
        let raw = core::read_project(&project_name)?;
        if let Ok(project) = serde_json::from_str::<core::Project>(&raw) {
            // Check if this project has the agent in custom_agents
            let has_agent = project
                .custom_agents
                .as_ref()
                .map(|agents| {
                    agents.iter().any(|a| {
                        // Match by extracting machine name from content frontmatter
                        // or by comparing the name field
                        let content_machine = extract_machine_name_from_content(&a.content);
                        content_machine.as_deref() == Some(agent_machine_name.as_str())
                            || a.name.to_lowercase().replace(' ', "-") == agent_machine_name
                    })
                })
                .unwrap_or(false);

            if has_agent {
                referencing.push(core::ProjectRef {
                    name: project_name,
                    directory: project.directory,
                });
            }
        }
    }

    Ok(referencing)
}

fn extract_machine_name_from_content(content: &str) -> Option<String> {
    if !content.starts_with("---\n") {
        return None;
    }
    let end = content[4..].find("\n---")?;
    let yaml = &content[4..end + 4];
    for line in yaml.lines() {
        let line = line.trim();
        if let Some(name_val) = line.strip_prefix("name:") {
            let name = name_val.trim().trim_matches('"').trim_matches('\'');
            if !name.is_empty() {
                // Convert to machine name format
                return Some(name.to_lowercase().replace(' ', "-"));
            }
        }
    }
    None
}
