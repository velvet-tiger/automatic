use serde_json::{json, Map, Value};
use std::fs;
use std::path::PathBuf;

/// Load MCP server configs from the Automatic registry (~/.automatic/mcp_servers/).
pub(crate) fn load_mcp_server_configs() -> Result<Map<String, Value>, String> {
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
///
/// Uses the raw SKILL.md content (without companion resource formatting) so
/// that the content written to project skill files during sync matches exactly
/// what drift detection compares against.  Companion files are handled
/// separately by `copy_skills_to_project` which copies the full directory.
pub(crate) fn load_skill_contents(skill_names: &[String]) -> Vec<(String, String)> {
    let mut contents = Vec::new();
    for name in skill_names {
        match crate::core::read_skill_raw(name) {
            Ok(content) if !content.is_empty() => {
                contents.push((name.clone(), content));
            }
            _ => {}
        }
    }
    contents
}

/// Find the Automatic binary path.
pub(crate) fn find_automatic_binary() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "automatic".to_string())
}

/// Strip any legacy `<!-- automatic:skills:start -->…<!-- automatic:skills:end -->`
/// managed section from a project file.  Returns the path if the file was
/// modified, or None if no cleanup was needed.
pub(crate) fn clean_project_file(dir: &PathBuf, filename: &str) -> Result<Option<String>, String> {
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

/// Strip any `<!-- automatic:rules:start -->…<!-- automatic:rules:end -->`
/// managed section from a project file.  Used when switching a Claude project
/// to the `.claude/rules/` mode so the two representations do not co-exist.
/// Returns the path if the file was modified, or None if no cleanup was needed.
pub(crate) fn clean_project_file_rules_section(
    dir: &PathBuf,
    filename: &str,
) -> Result<Option<String>, String> {
    let path = dir.join(filename);
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let cleaned = crate::core::strip_rules_section_pub(&content);

    if cleaned != content {
        fs::write(&path, cleaned).map_err(|e| e.to_string())?;
        Ok(Some(path.display().to_string()))
    } else {
        Ok(None)
    }
}

pub(crate) fn add_unique(items: &mut Vec<String>, value: &str) -> bool {
    if items.iter().any(|v| v == value) {
        false
    } else {
        items.push(value.to_string());
        true
    }
}

/// Build the selected MCP server map for a project, applying all
/// transformations that the sync engine uses (stripping internal `_`-prefixed
/// fields, substituting OAuth proxy configs for HTTP servers with stored
/// tokens).
///
/// Both `engine.rs` and `drift.rs` must use this function to ensure the
/// expected config matches what is actually written to disk.
pub(crate) fn build_selected_servers(
    project_name: &str,
    server_names: &[String],
    mcp_config: &Map<String, Value>,
) -> Map<String, Value> {
    let mut selected_servers = Map::new();
    let automatic_binary = find_automatic_binary();

    // Always include the Automatic MCP server
    selected_servers.insert(
        "automatic".to_string(),
        json!({
            "command": automatic_binary,
            "args": ["mcp-serve"],
            "env": {
                "AUTOMATIC_PROJECT": project_name
            }
        }),
    );

    // Add project-selected MCP servers from the Automatic registry.
    // Strip Automatic-internal fields (prefixed with `_`) before writing to agent files.
    //
    // For HTTP servers that have a stored OAuth token in the keychain, we emit
    // a local stdio proxy config instead of the remote URL.  This keeps the
    // token out of every project file — the proxy loads it from the keychain
    // at runtime.
    for server_name in server_names {
        if let Some(server_config) = mcp_config.get(server_name) {
            let cleaned = strip_internal_fields(server_config.clone());

            // Check if this is an HTTP server with a stored OAuth token.
            let is_http = cleaned
                .get("type")
                .and_then(|v| v.as_str())
                .map(|t| t == "http" || t == "sse")
                .unwrap_or(false);
            let has_token = crate::proxy::has_oauth_token(server_name);

            if is_http && has_token {
                // Emit a local proxy config instead of the remote URL.
                selected_servers.insert(
                    server_name.clone(),
                    json!({
                        "command": automatic_binary,
                        "args": ["mcp-proxy", server_name],
                    }),
                );
            } else {
                // For stdio servers, replace empty env values with ${KEY} so
                // the agent expands them from the shell environment at runtime.
                let mut server = cleaned;
                apply_env_inheritance(&mut server);
                selected_servers.insert(server_name.clone(), server);
            }
        }
    }

    selected_servers
}

/// Remove fields whose names start with `_` from a JSON object.
/// These are Automatic-internal metadata fields (e.g. `_author`) that should
/// never be written to agent configuration files.
pub(crate) fn strip_internal_fields(mut value: Value) -> Value {
    if let Value::Object(ref mut map) = value {
        map.retain(|key, _| !key.starts_with('_'));
    }
    value
}

/// Replace empty env values with `${KEY}` so the agent's shell-variable
/// expansion picks up the value from the user's environment at runtime,
/// keeping the literal secret out of the project config file.
///
/// Claude Code supports `${VAR}` / `${VAR:-default}` expansion in `.mcp.json`.
/// An empty string stored in Automatic signals "inherit from shell".
pub(crate) fn apply_env_inheritance(config: &mut Value) {
    if let Some(env) = config.get_mut("env") {
        if let Value::Object(ref mut map) = env {
            for (key, val) in map.iter_mut() {
                if val.as_str() == Some("") {
                    *val = Value::String(format!("${{{}}}", key));
                }
            }
        }
    }
}

/// Extract the machine name (slug) from agent frontmatter content.
/// Returns the `name` field converted to lowercase with spaces replaced by hyphens.
pub(crate) fn extract_agent_machine_name(content: &str) -> Option<String> {
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
                return Some(name.to_lowercase().replace(' ', "-"));
            }
        }
    }
    None
}

/// Sync project-local custom agents to a project's agents directory.
///
/// For each custom agent:
/// 1. Extract the machine name from the agent's frontmatter
/// 2. Write to `agents_dir/{machine_name}.md`
///
/// Returns the list of files written.
pub(crate) fn sync_custom_agents(
    agents_dir: &std::path::Path,
    custom_agents: &[crate::core::CustomAgent],
    agent: &dyn crate::agent::Agent,
) -> Result<Vec<String>, String> {
    if custom_agents.is_empty() {
        return Ok(Vec::new());
    }

    if !agents_dir.exists() {
        fs::create_dir_all(agents_dir).map_err(|e| e.to_string())?;
    }

    let mut written = Vec::new();
    let ext = agent.agents_file_ext();

    for custom_agent in custom_agents {
        let machine_name = extract_agent_machine_name(&custom_agent.content)
            .unwrap_or_else(|| custom_agent.name.to_lowercase().replace(' ', "-"));
        let converted_content = agent.convert_agent_content(&custom_agent.content, &machine_name);
        let path = agents_dir.join(format!("{}.{}", machine_name, ext));

        fs::write(&path, &converted_content).map_err(|e| e.to_string())?;
        written.push(path.display().to_string());
    }

    Ok(written)
}

/// Clean up all custom agent files from an agents directory.
/// Used when removing an agent from a project.
/// Returns the list of files removed.
pub(crate) fn cleanup_custom_agents(agents_dir: &std::path::Path, ext: &str) -> Vec<String> {
    let mut removed = Vec::new();

    if agents_dir.exists() {
        if let Ok(entries) = fs::read_dir(agents_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == ext) {
                    if fs::remove_file(&path).is_ok() {
                        removed.push(path.display().to_string());
                    }
                }
            }
        }
        // Remove the agents directory itself if now empty
        let _ = fs::remove_dir(agents_dir);
    }

    removed
}

/// Sync workspace user agents (from `~/.automatic/agents/`) to a project's
/// agents directory.
///
/// For each selected agent:
/// 1. Read the agent content from the global registry
/// 2. Convert to the target format if needed (e.g., TOML for Codex)
/// 3. Write to `agents_dir/{machine_name}.{ext}`
/// 4. Remove stale agent files not in the selected list (but NOT custom agents)
pub(crate) fn sync_user_agents(
    agents_dir: &std::path::Path,
    user_agent_names: &[String],
    custom_agent_names: &[String],
    agent: &dyn crate::agent::Agent,
) -> Result<Vec<String>, String> {
    if !agents_dir.exists() {
        fs::create_dir_all(agents_dir).map_err(|e| e.to_string())?;
    }

    let mut written = Vec::new();
    let mut expected_names: std::collections::HashSet<String> =
        user_agent_names.iter().cloned().collect();
    let ext = agent.agents_file_ext();

    // Write each selected agent
    for name in user_agent_names {
        if let Ok(content) = crate::core::read_user_agent(name) {
            let user_agent: crate::core::UserAgent =
                serde_json::from_str(&content).map_err(|e| e.to_string())?;
            let machine_name = extract_agent_machine_name(&user_agent.content)
                .unwrap_or_else(|| name.to_lowercase().replace(' ', "-"));
            let converted_content = agent.convert_agent_content(&user_agent.content, &machine_name);
            let path = agents_dir.join(format!("{}.{}", machine_name, ext));

            fs::write(&path, &converted_content).map_err(|e| e.to_string())?;
            written.push(path.display().to_string());
            expected_names.insert(machine_name);
        }
    }

    // Also add custom agent names to expected set so they're not removed as stale
    for name in custom_agent_names {
        expected_names.insert(name.clone());
    }

    // Remove stale agent files (agents not in user_agents OR custom_agents)
    if agents_dir.exists() {
        if let Ok(entries) = fs::read_dir(agents_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == ext) {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        // Only remove if it's a valid machine name and not expected
                        if crate::core::is_valid_agent_machine_name(stem)
                            && !expected_names.contains(stem)
                        {
                            if fs::remove_file(&path).is_ok() {
                                written.push(path.display().to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(written)
}
