//! Per-agent selection and ownership state for global (user-level) MCP writes.
//!
//! Stored at `<automatic dir>/global_mcp.json`.  Kept separate from
//! `settings.json` because the frontend read-modify-writes the whole `Settings`
//! struct while backend flows (registry-change re-apply, OAuth authorise hook)
//! also write ownership records here; sharing one file would race and orphan
//! entries in agent config files that Automatic could no longer reconcile.
//!
//! `managed` is the ownership set: a name may only be removed from the agent's
//! global config file if it appears here.  See
//! [`crate::agent::merge_global_mcp_entries_json`] for the merge rules that
//! consume this state.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

use super::paths::get_automatic_dir;

/// Filename under [`get_automatic_dir`].  Path is resolved fresh on every
/// read/write so the debug/release split (`~/.automatic-dev` vs `~/.automatic`)
/// and the test-home override apply automatically.
const STATE_FILENAME: &str = "global_mcp.json";

/// Root state document.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GlobalMcpState {
    /// Keyed by agent id (e.g. `"claude"`).  Absent = never configured.
    #[serde(default)]
    pub agents: HashMap<String, AgentGlobalMcp>,
}

impl GlobalMcpState {
    /// The record `agent_id` reads and writes.  Every lookup goes through
    /// this: agents that write one file share a record, so a raw id would
    /// show an empty selection and a re-apply would remove the real one.
    pub fn record(&self, agent_id: &str) -> Option<&AgentGlobalMcp> {
        self.agents.get(&global_mcp_state_key(agent_id))
    }
}

/// The key of `agent_id`'s record in [`GlobalMcpState::agents`].
pub fn global_mcp_state_key(agent_id: &str) -> String {
    crate::agent::from_id(agent_id)
        .map_or(agent_id, |agent| agent.global_mcp_state_id())
        .to_string()
}

/// Per-agent slice of the state document.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentGlobalMcp {
    /// Registry server names the user selected for this agent's global config.
    #[serde(default)]
    pub selected: Vec<String>,
    /// Ownership set: entry names Automatic wrote into this agent's global
    /// config file on the last apply.  Removals and overwrites are only
    /// permitted for names in this set.
    #[serde(default)]
    pub managed: Vec<String>,
    /// Names that collided with foreign entries on the last apply and were
    /// left untouched.  Surfaced in the UI so the user can rename one side.
    #[serde(default)]
    pub skipped: Vec<String>,
    /// Names dropped from `selected` on the last apply because their config
    /// referenced `${workspaceFolder}` (no meaning at global scope).
    #[serde(default)]
    pub rejected: Vec<String>,
    /// The file `managed` refers to.  When the agent's target has moved,
    /// those names say nothing about the new file, where a same-named entry
    /// is the user's.  `None` on records from before this field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_path: Option<String>,
    /// RFC3339 timestamp of the last successful apply, `None` if never applied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_applied: Option<String>,
}

impl AgentGlobalMcp {
    /// The names this record owns in `target_path`.  A record from before
    /// targets were stored owns names only in `legacy_path`.
    pub fn managed_in(&self, target_path: &str, legacy_path: Option<&str>) -> Vec<String> {
        let owner = self.target_path.as_deref().or(legacy_path);
        if owner == Some(target_path) {
            self.managed.clone()
        } else {
            Vec::new()
        }
    }

    /// The names this record owns in `agent`'s current global target.
    pub fn managed_for(&self, agent: &dyn crate::agent::Agent) -> Vec<String> {
        let Some(target) = agent.global_mcp_target() else {
            return Vec::new();
        };
        let legacy = agent.legacy_global_mcp_path().map(|p| p.display().to_string());
        self.managed_in(&target.path.display().to_string(), legacy.as_deref())
    }
}

/// Load the state document, or `Default` if the file does not exist.
///
/// A malformed file returns `Default` rather than an error — the file is
/// backend-owned bookkeeping, not user-authored config, and a corrupt copy
/// would otherwise prevent the app from starting.  Callers that need to be
/// paranoid can call [`state_file_exists`] first.
pub fn load_global_mcp_state() -> Result<GlobalMcpState, String> {
    let path = get_automatic_dir()?.join(STATE_FILENAME);
    if !path.exists() {
        return Ok(GlobalMcpState::default());
    }
    let raw = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    if raw.trim().is_empty() {
        return Ok(GlobalMcpState::default());
    }
    Ok(serde_json::from_str::<GlobalMcpState>(&raw).unwrap_or_default())
}

/// Persist the state document, creating the parent directory if needed.
pub fn save_global_mcp_state(state: &GlobalMcpState) -> Result<(), String> {
    let dir = get_automatic_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create {}: {}", dir.display(), e))?;
    }
    let path = dir.join(STATE_FILENAME);
    let raw = serde_json::to_string_pretty(state)
        .map_err(|e| format!("Failed to serialise global MCP state: {}", e))?;
    fs::write(&path, raw).map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
    Ok(())
}

/// `true` if the state file exists on disk.  Distinguishes a "genuinely empty"
/// state from a fresh install for the UI.
pub fn state_file_exists() -> bool {
    match get_automatic_dir() {
        Ok(dir) => dir.join(STATE_FILENAME).exists(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::paths::with_test_home;
    use tempfile::TempDir;

    #[test]
    fn a_legacy_record_owns_names_only_in_its_default_file() {
        let legacy = AgentGlobalMcp {
            managed: vec!["alpha".to_string()],
            ..Default::default()
        };
        assert_eq!(legacy.managed_in("/home/me/.pi/agent/mcp.json", Some("/home/me/.pi/agent/mcp.json")), vec!["alpha"]);
        assert!(legacy.managed_in("/work/pi/mcp.json", Some("/home/me/.pi/agent/mcp.json")).is_empty());
        assert!(legacy.managed_in("/work/pi/mcp.json", None).is_empty());

        let current = AgentGlobalMcp {
            target_path: Some("/work/pi/mcp.json".to_string()),
            ..legacy
        };
        assert_eq!(current.managed_in("/work/pi/mcp.json", Some("/home/me/.pi/agent/mcp.json")), vec!["alpha"]);
    }

    #[test]
    fn load_returns_default_when_file_absent() {
        let tmp = TempDir::new().unwrap();
        with_test_home(tmp.path().to_path_buf(), || {
            let state = load_global_mcp_state().unwrap();
            assert!(state.agents.is_empty());
        });
    }

    #[test]
    fn round_trip_preserves_selection_and_ownership() {
        let tmp = TempDir::new().unwrap();
        with_test_home(tmp.path().to_path_buf(), || {
            let mut state = GlobalMcpState::default();
            state.agents.insert(
                "cursor".to_string(),
                AgentGlobalMcp {
                    target_path: None,
                    selected: vec!["fs".to_string(), "web".to_string()],
                    managed: vec!["fs".to_string()],
                    skipped: vec!["web".to_string()],
                    rejected: vec![],
                    last_applied: Some("2026-08-26T12:00:00Z".to_string()),
                },
            );
            save_global_mcp_state(&state).unwrap();
            let loaded = load_global_mcp_state().unwrap();
            let cursor = loaded.agents.get("cursor").expect("cursor entry");
            assert_eq!(cursor.selected, vec!["fs", "web"]);
            assert_eq!(cursor.managed, vec!["fs"]);
            assert_eq!(cursor.skipped, vec!["web"]);
            assert_eq!(cursor.last_applied.as_deref(), Some("2026-08-26T12:00:00Z"));
        });
    }

    #[test]
    fn malformed_file_falls_back_to_default() {
        let tmp = TempDir::new().unwrap();
        with_test_home(tmp.path().to_path_buf(), || {
            let path = get_automatic_dir().unwrap().join(STATE_FILENAME);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "{ not json").unwrap();
            let loaded = load_global_mcp_state().unwrap();
            assert!(loaded.agents.is_empty());
        });
    }
}
