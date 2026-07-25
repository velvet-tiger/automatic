use std::collections::HashSet;

use serde_json::{json, Map, Value};
use std::fs;
use std::path::PathBuf;

use crate::core::Project;

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

/// Build the combined skill contents and custom skill name list for a project,
/// deduplicating custom_skills entries whose names already appear in the
/// project's library-backed `skills` list.  Library wins because
/// `copy_skills_to_project` writes library content to disk; a stale
/// custom_skills snapshot would otherwise produce perpetual drift.
pub(crate) fn build_skill_contents(
    project: &Project,
) -> (Vec<(String, String)>, Vec<String>) {
    let mut skill_contents = load_skill_contents(&project.skills);
    let library_skill_names: HashSet<&str> = project.skills.iter().map(|s| s.as_str()).collect();
    let custom_skills = project.custom_skills.as_deref().unwrap_or(&[]);
    for cs in custom_skills {
        if library_skill_names.contains(cs.name.as_str()) {
            continue;
        }
        skill_contents.push((cs.name.clone(), cs.content.clone()));
    }
    let custom_skill_names: Vec<String> = custom_skills
        .iter()
        .filter(|s| !library_skill_names.contains(s.name.as_str()))
        .map(|s| s.name.clone())
        .collect();
    (skill_contents, custom_skill_names)
}

/// Prune `custom_skills` entries whose name already appears in the project's
/// library-backed `skills` list.  Returns `true` if any entries were removed.
pub(crate) fn prune_shadowed_custom_skills(project: &mut Project) -> bool {
    let library_names: HashSet<&str> = project.skills.iter().map(|s| s.as_str()).collect();
    let Some(custom) = project.custom_skills.as_mut() else {
        return false;
    };
    let before = custom.len();
    custom.retain(|cs| !library_names.contains(cs.name.as_str()));
    let changed = custom.len() != before;
    if custom.is_empty() {
        project.custom_skills = None;
    }
    changed
}

/// Find the Automatic binary path.
///
/// Delegates to the canonical resolver so the sync engine, drift check, and
/// MCP registry all emit the identical path string regardless of how the
/// current process was invoked (GUI bundle vs CLI symlink).
pub(crate) fn find_automatic_binary() -> String {
    crate::core::automatic_binary_path()
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
        if server_name == "automatic" {
            continue;
        }
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
                // Empty env values are left as-is here: they are the canonical
                // "inherit from the environment" marker, and each agent renders
                // them in its own placeholder syntax via
                // `agent::prepare_mcp_servers` at write time.
                selected_servers.insert(server_name.clone(), cleaned);
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
        if let Ok(content) = crate::core::read_subagent(name) {
            let user_agent: crate::core::Subagent =
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::ClaudeCode;
    use crate::core::{save_subagent, CustomAgent};
    use std::sync::{Mutex, OnceLock};
    use tempfile::TempDir;

    fn tmp() -> TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct HomeGuard {
        original: Option<String>,
    }

    impl HomeGuard {
        fn set(path: &std::path::Path) -> Self {
            let original = std::env::var("HOME").ok();
            unsafe {
                std::env::set_var("HOME", path);
            }
            Self { original }
        }
    }

    impl Drop for HomeGuard {
        fn drop(&mut self) {
            unsafe {
                if let Some(original) = &self.original {
                    std::env::set_var("HOME", original);
                } else {
                    std::env::remove_var("HOME");
                }
            }
        }
    }

    #[test]
    fn sync_custom_agents_uses_frontmatter_name_and_fallback_name() {
        let dir = tmp();
        let agents_dir = dir.path().join("agents");
        let custom_agents = vec![
            CustomAgent {
                name: "Display Name".to_string(),
                content: "---\nname: Planner Agent\n---\n\nPlan carefully.\n".to_string(),
            },
            CustomAgent {
                name: "Fallback Agent".to_string(),
                content: "No frontmatter here.\n".to_string(),
            },
        ];

        let written = sync_custom_agents(&agents_dir, &custom_agents, &ClaudeCode).expect("sync");

        assert_eq!(written.len(), 2);
        assert!(agents_dir.join("planner-agent.md").exists());
        assert!(agents_dir.join("fallback-agent.md").exists());
        assert!(fs::read_to_string(agents_dir.join("planner-agent.md"))
            .expect("read planner")
            .contains("Plan carefully."));
    }

    #[test]
    fn sync_user_agents_writes_selected_and_removes_stale_non_custom_files() {
        let _lock = env_lock().lock().expect("env lock");
        let home = tmp();
        let _home_guard = HomeGuard::set(home.path());
        let project = tmp();
        let agents_dir = project.path().join("agents");
        fs::create_dir_all(&agents_dir).expect("create agents dir");

        save_subagent(
            "reviewer-agent",
            "Reviewer Agent",
            "---\nname: Reviewer Agent\n---\n\nReview thoroughly.\n",
        )
        .expect("save reviewer subagent");

        let stale_path = agents_dir.join("stale-agent.md");
        fs::write(&stale_path, "---\nname: Stale Agent\n---\n\nOld content.\n")
            .expect("write stale agent");

        let custom_path = agents_dir.join("custom-agent.md");
        fs::write(&custom_path, "---\nname: Custom Agent\n---\n\nKeep me.\n")
            .expect("write custom agent");

        let written = sync_user_agents(
            &agents_dir,
            &["reviewer-agent".to_string()],
            &["custom-agent".to_string()],
            &ClaudeCode,
        )
        .expect("sync user agents");

        assert!(written.iter().any(|p| p.ends_with("reviewer-agent.md")));
        assert!(written.iter().any(|p| p.ends_with("stale-agent.md")));
        assert!(agents_dir.join("reviewer-agent.md").exists());
        assert!(!stale_path.exists());
        assert!(custom_path.exists());
        assert!(fs::read_to_string(agents_dir.join("reviewer-agent.md"))
            .expect("read synced user agent")
            .contains("Review thoroughly."));
    }
}
