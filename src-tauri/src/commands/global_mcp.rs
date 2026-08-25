//! Tauri commands for the Providers > MCP tab.
//!
//! Thin wrappers over `sync::global_mcp` — no logic here beyond decoding the
//! frontend's parameter shape.

use serde::Serialize;
use serde_json::json;

use crate::agent;
use crate::core::global_mcp::{load_global_mcp_state, AgentGlobalMcp};
use crate::sync::global_mcp::{
    self as orchestrator, GlobalMcpApplyReport, GlobalMcpPreview, GlobalMcpStatus,
};

/// Per-agent slice served to the frontend, keyed by agent id.
#[derive(Debug, Serialize)]
pub struct AgentGlobalMcpView {
    pub selected: Vec<String>,
    pub managed: Vec<String>,
    pub skipped: Vec<String>,
    pub rejected: Vec<String>,
    pub last_applied: Option<String>,
    pub supported: bool,
    pub target_path: Option<String>,
    pub reload_note: Option<&'static str>,
}

/// Return the full state document plus per-agent capability + target metadata.
///
/// Returned as a JSON string to match the `list_agents_with_projects` convention
/// and keep the frontend's typed shape untouched by future backend refactors.
#[tauri::command]
pub fn get_global_mcp_state() -> Result<String, String> {
    let state = load_global_mcp_state()?;
    let mut agents: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();

    for agent in agent::all() {
        let empty = AgentGlobalMcp::default();
        let record = state.agents.get(agent.id()).unwrap_or(&empty);
        let target = agent.global_mcp_target();
        let view = AgentGlobalMcpView {
            selected: record.selected.clone(),
            managed: record.managed.clone(),
            skipped: record.skipped.clone(),
            rejected: record.rejected.clone(),
            last_applied: record.last_applied.clone(),
            supported: target.is_some(),
            target_path: target.as_ref().map(|t| t.path.display().to_string()),
            reload_note: target.as_ref().and_then(|t| t.reload_note),
        };
        agents.insert(
            agent.id().to_string(),
            serde_json::to_value(view).map_err(|e| e.to_string())?,
        );
    }

    serde_json::to_string(&json!({ "agents": agents })).map_err(|e| e.to_string())
}

/// Eligible registry servers, tagged with why each ineligible one is hidden.
///
/// `automatic` and `${workspaceFolder}` users are marked ineligible with a
/// human-readable reason so the frontend can hide-but-explain rather than
/// silently omit them.
#[tauri::command]
pub fn list_global_eligible_mcp_servers() -> Result<String, String> {
    let names = crate::core::list_mcp_server_configs()?;
    let mut out: Vec<serde_json::Value> = Vec::with_capacity(names.len());
    for name in names {
        let mut eligible = true;
        let mut reason: Option<String> = None;
        if name == "automatic" {
            eligible = false;
            reason = Some("Project-scoped only".to_string());
        } else if let Ok(raw) = crate::core::read_mcp_server_config(&name) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
                if agent::server_uses_workspace_folder(&value) {
                    eligible = false;
                    reason = Some("References ${workspaceFolder}".to_string());
                }
            }
        }
        out.push(json!({
            "name": name,
            "eligible": eligible,
            "reason": reason,
        }));
    }
    serde_json::to_string(&out).map_err(|e| e.to_string())
}

/// Preview what applying `servers` for `agent_id` would do.
#[tauri::command]
pub fn preview_global_mcp_apply(
    agent_id: String,
    servers: Vec<String>,
) -> Result<GlobalMcpPreview, String> {
    orchestrator::preview_global_mcp(&agent_id, &servers)
}

/// Persist the new selection and apply it immediately.
#[tauri::command]
pub fn set_global_mcp_servers(
    agent_id: String,
    servers: Vec<String>,
) -> Result<GlobalMcpApplyReport, String> {
    orchestrator::apply_global_mcp(&agent_id, servers)
}

/// Re-apply the persisted selection for `agent_id` (Re-apply button on the tab).
#[tauri::command]
pub fn reapply_global_mcp(agent_id: String) -> Result<GlobalMcpApplyReport, String> {
    let state = load_global_mcp_state()?;
    let selection = state
        .agents
        .get(&agent_id)
        .map(|a| a.selected.clone())
        .unwrap_or_default();
    orchestrator::apply_global_mcp(&agent_id, selection)
}

/// Status for the tab header (target path, in-sync pill, reload hint).
#[tauri::command]
pub fn get_global_mcp_status(agent_id: String) -> Result<GlobalMcpStatus, String> {
    orchestrator::global_mcp_status(&agent_id)
}
