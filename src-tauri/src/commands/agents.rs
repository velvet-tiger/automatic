use crate::agent;
use crate::core;
use serde_json::Value;

// ── Agents ───────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_agents() -> Vec<agent::AgentInfo> {
    agent::all()
        .iter()
        .map(|a| agent::AgentInfo::from_agent(*a))
        .collect()
}

/// Returns each agent with the list of projects that reference it.
#[tauri::command]
pub fn list_agents_with_projects() -> Result<String, String> {
    let agents = agent::all();
    let project_names = core::list_projects().unwrap_or_default();

    // Read all projects once
    let projects: Vec<core::Project> = project_names
        .iter()
        .filter_map(|name| {
            core::read_project(name)
                .ok()
                .and_then(|raw| serde_json::from_str::<core::Project>(&raw).ok())
        })
        .collect();

    let result: Vec<serde_json::Value> = agents
        .iter()
        .map(|a| {
            let agent_projects: Vec<serde_json::Value> = projects
                .iter()
                .filter(|p| p.agents.iter().any(|id| id == a.id()))
                .map(|p| {
                    serde_json::json!({
                        "name": p.name,
                        "directory": p.directory,
                    })
                })
                .collect();

            serde_json::json!({
                "id": a.id(),
                "label": a.label(),
                "description": a.config_description(),
                "project_file": a.project_file_name(),
                "capabilities": a.capabilities(),
                "mcp_note": a.mcp_note(),
                "projects": agent_projects,
            })
        })
        .collect();

    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// Detect which agents are installed on the current machine by running each
/// agent's `detect_global_install()` heuristic (binary on PATH, app bundle,
/// or characteristic config directory).
///
/// Returns a JSON array of agent id strings for every agent that appears to
/// be installed, e.g. `["claude", "cursor", "goose"]`.
#[tauri::command]
pub fn detect_installed_agents() -> Result<String, String> {
    let installed: Vec<&str> = agent::all()
        .into_iter()
        .filter(|a| a.detect_global_install())
        .map(|a| a.id())
        .collect();
    serde_json::to_string(&installed).map_err(|e| e.to_string())
}

/// Scan the user-level (global) config of each requested agent for existing
/// MCP server definitions and skills not yet in Automatic's registry.
/// Read-only — nothing is written to disk.
///
/// `agent_ids` is the array of agent id strings selected by the user in the
/// wizard (e.g. `["claude", "cursor"]`).  The special value `"other"` is
/// silently ignored.
///
/// Returns a JSON array where each element describes one agent:
/// ```json
/// [
///   { "agent_id": "claude", "agent_label": "Claude Code",
///     "server_count": 2, "server_names": ["github", "linear"],
///     "skill_count": 1, "skill_names": ["git-commit"] },
///   ...
/// ]
/// ```
#[tauri::command]
pub fn detect_agent_global_configs(agent_ids: Vec<String>) -> Result<String, String> {
    let mut results: Vec<Value> = Vec::new();

    for id in &agent_ids {
        if id == "other" {
            continue;
        }
        let agent = match agent::from_id(id) {
            Some(a) => a,
            None => continue,
        };

        let servers = agent.discover_global_mcp_servers();
        let server_names: Vec<Value> = servers.keys().map(|k| Value::String(k.clone())).collect();

        let new_skills = agent::collect_new_skills_from_extra_dirs(agent);
        let skill_names: Vec<Value> = new_skills
            .iter()
            .map(|(name, _)| Value::String(name.clone()))
            .collect();

        results.push(serde_json::json!({
            "agent_id": agent.id(),
            "agent_label": agent.label(),
            "server_count": servers.len(),
            "server_names": server_names,
            "skill_count": new_skills.len(),
            "skill_names": skill_names,
        }));
    }

    serde_json::to_string(&results).map_err(|e| e.to_string())
}

/// Import MCP server configs discovered in the user-level (global) config of
/// each requested agent into Automatic's global MCP server registry.
///
/// This is idempotent: existing registry entries with the same name are
/// overwritten with the freshly-read config so that the registry stays in sync
/// with what the agent currently has configured.
///
/// `agent_ids` is the JSON-serialised array of agent id strings.  The special
/// value `"other"` is silently ignored.
///
/// Returns a JSON array of the server names that were imported.
#[tauri::command]
pub fn import_agent_global_configs(agent_ids: Vec<String>) -> Result<String, String> {
    let mut imported: Vec<String> = Vec::new();

    for id in &agent_ids {
        if id == "other" {
            continue;
        }
        let agent = match agent::from_id(id) {
            Some(a) => a,
            None => continue,
        };

        let servers = agent.discover_global_mcp_servers();
        for (name, config) in servers {
            let config_str = serde_json::to_string_pretty(&config)
                .map_err(|e| format!("Failed to serialise config for '{}': {}", name, e))?;
            core::save_mcp_server_config(&name, &config_str)?;
            imported.push(name);
        }
    }

    // Deduplicate (multiple agents may have the same server name)
    imported.sort();
    imported.dedup();

    serde_json::to_string(&imported).map_err(|e| e.to_string())
}

/// Import skills found in agent-specific extra global skill directories
/// (e.g. `~/.cline/skills/`) that are not yet in Automatic's registry.
///
/// Each discovered skill is saved to `~/.agents/skills/` (the canonical
/// location) so it becomes available to all agents and projects.
///
/// Skills already present in `~/.agents/skills/` or `~/.claude/skills/`
/// are skipped — this only imports genuinely new skills.
///
/// `agent_ids` is the array of agent id strings.  The special value
/// `"other"` is silently ignored.
///
/// Returns a JSON array of the skill names that were imported.
#[tauri::command]
pub fn import_agent_global_skills(agent_ids: Vec<String>) -> Result<String, String> {
    let mut imported: Vec<String> = Vec::new();

    for id in &agent_ids {
        if id == "other" {
            continue;
        }
        let agent = match agent::from_id(id) {
            Some(a) => a,
            None => continue,
        };

        for (name, content) in agent::collect_new_skills_from_extra_dirs(agent) {
            core::save_skill(&name, &content)?;
            imported.push(name);
        }
    }

    imported.sort();
    imported.dedup();

    serde_json::to_string(&imported).map_err(|e| e.to_string())
}
