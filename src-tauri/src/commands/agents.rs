use crate::agent;
use crate::core;

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
