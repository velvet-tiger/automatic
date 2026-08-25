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
    /// RFC3339 timestamp of the last successful apply, `None` if never applied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_applied: Option<String>,
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
