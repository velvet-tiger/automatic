//! Global (user-level) MCP writer orchestration.
//!
//! Connects three pieces:
//! - Persisted per-agent selection + ownership state (`core::global_mcp`).
//! - The MCP server registry and shared payload transforms
//!   (`sync::helpers::build_global_selected_servers`, `strip_internal_fields`,
//!   OAuth proxy-stub substitution).
//! - The per-agent global writer (`Agent::write_global_mcp_config`) via the
//!   shared entry-level merge (`agent::merge_global_mcp_entries_json`).
//!
//! Callers should use these entry points, not touch the state store directly,
//! so that the write pipeline and the ownership record stay in step.

use serde::Serialize;
use serde_json::{Map, Value};
use std::path::PathBuf;

use super::helpers::{build_global_selected_servers, load_mcp_server_configs};
use crate::agent::{self, GlobalMcpWriteReport};
use crate::core::global_mcp::{load_global_mcp_state, save_global_mcp_state, AgentGlobalMcp};

// ── Public shapes ────────────────────────────────────────────────────────────

/// One server the caller asked for that could not be written verbatim, tagged
/// with the reason so the UI can explain why.
#[derive(Debug, Clone, Serialize)]
pub struct RejectedServer {
    pub name: String,
    pub reason: String,
}

/// Full apply result the frontend renders as a toast/inline status.
#[derive(Debug, Serialize)]
pub struct GlobalMcpApplyReport {
    /// The underlying merge report. `unchanged` is the "in sync, no-op" case.
    #[serde(flatten)]
    pub write: GlobalMcpWriteReport,
    /// Servers the user asked for that were dropped before the write step,
    /// e.g. because they reference `${workspaceFolder}` or were absent from
    /// the registry.
    pub rejected: Vec<RejectedServer>,
    /// Reload semantics the UI should surface (verbatim from the target).
    pub reload_note: Option<&'static str>,
}

/// Classification the frontend uses to confirm changes before writing.
#[derive(Debug, Serialize)]
pub struct GlobalMcpPreview {
    pub target_path: String,
    pub target_exists: bool,
    /// Names of foreign entries already in the target file, in `servers_key`
    /// order.  Used for the "first-write-into-populated-file" confirmation.
    pub foreign_entries: Vec<String>,
    /// Names that would be created or updated.
    pub would_write: Vec<String>,
    /// Names that would be left untouched because a foreign entry exists.
    pub would_skip: Vec<String>,
    /// Names that would be deleted (were managed, no longer selected).
    pub would_remove: Vec<String>,
    /// Selected servers rejected before the write step.
    pub rejected: Vec<RejectedServer>,
}

/// Read-only status of one agent's global MCP file, for the tab header.
#[derive(Debug, Serialize)]
pub struct GlobalMcpStatus {
    /// True iff the agent implements a global writer.
    pub supported: bool,
    /// `mcp_note` for unsupported agents.
    pub note: Option<String>,
    /// Absent when `supported = false` or the home directory is unresolved.
    pub target_path: Option<String>,
    pub target_exists: bool,
    pub reload_note: Option<&'static str>,
    /// True iff the current selection would leave the target file unchanged.
    /// False when a re-apply would write, remove, or skip anything.
    pub in_sync: bool,
    /// Names in the current selection that would be created or updated by a
    /// re-apply — i.e. entries missing or drifted from what Automatic last
    /// wrote.
    pub missing: Vec<String>,
    /// Names in the current selection that would be skipped by a re-apply
    /// (foreign collision).
    pub skipped: Vec<String>,
}

// ── Public orchestration ─────────────────────────────────────────────────────

/// Persist a new selection for `agent_id` and apply it immediately.
///
/// `selection` replaces the agent's `selected` set atomically — callers pass
/// the full desired list, not a diff.  Ownership (`managed`) is updated to
/// exactly the entries written this apply (plus any previously-managed names
/// that survived because they were still desired but rendered unchanged).
pub fn apply_global_mcp(
    agent_id: &str,
    selection: Vec<String>,
) -> Result<GlobalMcpApplyReport, String> {
    let mut state = load_global_mcp_state()?;
    let entry = state.agents.entry(agent_id.to_string()).or_default();
    entry.selected = dedupe_preserving_order(selection);
    let selection = entry.selected.clone();
    let previously_managed = entry.managed.clone();
    // Drop the borrow before the apply pass takes another mut borrow.
    let _ = entry;
    let result = run_apply(agent_id, &selection, &previously_managed)?;
    let entry = state.agents.entry(agent_id.to_string()).or_default();
    apply_result_to_state(entry, &result, &selection);
    save_global_mcp_state(&state)?;
    Ok(result)
}

/// Preview what an apply of `selection` would do without touching disk.
pub fn preview_global_mcp(
    agent_id: &str,
    selection: &[String],
) -> Result<GlobalMcpPreview, String> {
    let agent = agent::from_id(agent_id)
        .ok_or_else(|| format!("Unknown agent id: {}", agent_id))?;
    let target = agent
        .global_mcp_target()
        .ok_or_else(|| format!("{} does not support global MCP writes", agent.label()))?;

    let state = load_global_mcp_state()?;
    let previously_managed = state
        .agents
        .get(agent_id)
        .map(|a| a.managed.clone())
        .unwrap_or_default();

    // Snapshot foreign entries by round-tripping through the agent's own
    // discovery — it already knows how to find the servers map inside the
    // dialect-specific file layout.
    let existing = agent.discover_global_mcp_servers();
    let foreign_entries: Vec<String> = existing
        .keys()
        .filter(|name| !previously_managed.contains(name))
        .cloned()
        .collect();

    let mcp_config = load_mcp_server_configs()?;
    let (desired_map, rejected) = build_prepared_desired(agent, selection, &mcp_config);

    let previously_managed_set: std::collections::HashSet<&String> =
        previously_managed.iter().collect();
    let mut would_write = Vec::new();
    let mut would_skip = Vec::new();
    for name in desired_map.keys() {
        let is_managed = previously_managed_set.contains(name);
        let on_disk = existing.contains_key(name);
        if on_disk && !is_managed {
            // Foreign entry with the same name — left alone.
            would_skip.push(name.clone());
        } else if !on_disk {
            // Truly missing — a re-apply will create it.
            would_write.push(name.clone());
        }
        // else: managed AND on disk — Automatic already wrote it and the
        // registry render is deterministic per server, so treat as in sync.
        // Changes to the registry entry itself go through
        // `reapply_agents_referencing`, not through this classification.
    }
    let would_remove: Vec<String> = previously_managed
        .iter()
        .filter(|name| !desired_map.contains_key(*name))
        .cloned()
        .collect();

    Ok(GlobalMcpPreview {
        target_path: target.path.display().to_string(),
        target_exists: target.path.exists(),
        foreign_entries,
        would_write,
        would_skip,
        would_remove,
        rejected,
    })
}

/// Re-apply every agent whose current selection references `server_name`.
///
/// Called after a registry save/edit or OAuth authorise — the server's
/// rendered config changed, so agents that assigned it globally need to see
/// the new bytes.  Best-effort: individual failures are logged, never
/// propagated (mirrors `sync_projects_referencing_mcp_server`).
pub fn reapply_agents_referencing(server_name: &str) {
    let state = match load_global_mcp_state() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("global_mcp: load state failed during reapply: {}", e);
            return;
        }
    };
    for (agent_id, agent_state) in &state.agents {
        if !agent_state.selected.iter().any(|n| n == server_name) {
            continue;
        }
        if let Err(e) = apply_global_mcp(agent_id, agent_state.selected.clone()) {
            eprintln!(
                "global_mcp: reapply for {} after '{}' change failed: {}",
                agent_id, server_name, e
            );
        }
    }
}

/// Registry deletion: drop `server_name` from every agent's selection and
/// re-apply so the entry disappears from any file where it is managed.
pub fn prune_server_from_global(server_name: &str) {
    let state = match load_global_mcp_state() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("global_mcp: load state failed during prune: {}", e);
            return;
        }
    };
    for (agent_id, agent_state) in &state.agents {
        if !agent_state.selected.iter().any(|n| n == server_name) {
            continue;
        }
        let next: Vec<String> = agent_state
            .selected
            .iter()
            .filter(|n| n.as_str() != server_name)
            .cloned()
            .collect();
        if let Err(e) = apply_global_mcp(agent_id, next) {
            eprintln!(
                "global_mcp: prune of '{}' from {} failed: {}",
                server_name, agent_id, e
            );
        }
    }
}

/// Compute read-only status of one agent's global MCP file.
pub fn global_mcp_status(agent_id: &str) -> Result<GlobalMcpStatus, String> {
    let agent = agent::from_id(agent_id)
        .ok_or_else(|| format!("Unknown agent id: {}", agent_id))?;
    let Some(target) = agent.global_mcp_target() else {
        return Ok(GlobalMcpStatus {
            supported: false,
            note: agent.mcp_note().map(|s| s.to_string()),
            target_path: None,
            target_exists: false,
            reload_note: None,
            in_sync: true,
            missing: vec![],
            skipped: vec![],
        });
    };

    let state = load_global_mcp_state()?;
    let selection = state
        .agents
        .get(agent_id)
        .map(|a| a.selected.clone())
        .unwrap_or_default();

    let preview = preview_global_mcp(agent_id, &selection)?;
    let in_sync = preview.would_write.is_empty()
        && preview.would_remove.is_empty()
        && preview.would_skip.is_empty()
        && preview.rejected.is_empty();

    Ok(GlobalMcpStatus {
        supported: true,
        note: agent.mcp_note().map(|s| s.to_string()),
        target_path: Some(target.path.display().to_string()),
        target_exists: target.path.exists(),
        reload_note: target.reload_note,
        in_sync,
        missing: preview.would_write,
        skipped: preview.would_skip,
    })
}

// ── Internals ────────────────────────────────────────────────────────────────

/// Run the write pipeline for `selection` without touching persisted state.
fn run_apply(
    agent_id: &str,
    selection: &[String],
    previously_managed: &[String],
) -> Result<GlobalMcpApplyReport, String> {
    let agent = agent::from_id(agent_id)
        .ok_or_else(|| format!("Unknown agent id: {}", agent_id))?;
    let target = agent
        .global_mcp_target()
        .ok_or_else(|| format!("{} does not support global MCP writes", agent.label()))?;

    let mcp_config = load_mcp_server_configs()?;
    let (desired, rejected) = build_prepared_desired(agent, selection, &mcp_config);

    let write = agent.write_global_mcp_config(&desired, previously_managed)?;
    Ok(GlobalMcpApplyReport {
        write,
        rejected,
        reload_note: target.reload_note,
    })
}

/// Build the prepared desired map plus the list of servers dropped along the
/// way (missing from the registry, or referencing `${workspaceFolder}`).
fn build_prepared_desired(
    agent: &dyn agent::Agent,
    selection: &[String],
    mcp_config: &Map<String, Value>,
) -> (Map<String, Value>, Vec<RejectedServer>) {
    let mut rejected: Vec<RejectedServer> = Vec::new();
    let mut effective_names: Vec<String> = Vec::with_capacity(selection.len());
    let mut seen = std::collections::HashSet::new();

    for name in selection {
        if !seen.insert(name.clone()) {
            continue;
        }
        if name == "automatic" {
            rejected.push(RejectedServer {
                name: name.clone(),
                reason: "The 'automatic' server is project-scoped only".to_string(),
            });
            continue;
        }
        match mcp_config.get(name) {
            None => rejected.push(RejectedServer {
                name: name.clone(),
                reason: "Server no longer exists in the registry".to_string(),
            }),
            Some(cfg) => {
                if agent::server_uses_workspace_folder(cfg) {
                    rejected.push(RejectedServer {
                        name: name.clone(),
                        reason: "Server config references ${workspaceFolder}, which has no meaning at global scope".to_string(),
                    });
                    continue;
                }
                effective_names.push(name.clone());
            }
        }
    }

    let raw = build_global_selected_servers(&effective_names, mcp_config);
    let prepared = agent::prepare_global_mcp_servers(agent, &raw);
    (prepared, rejected)
}

/// Update the state entry with the result of one apply.
///
/// Ownership rule from the plan:
/// `managed = written ∪ (previous ∩ still-desired)`.
/// - `written` covers new upserts.
/// - Names that were managed *and* still desired but produced no write (either
///   `unchanged` overall, or skipped because a foreign entry appeared in the
///   file since the last apply — an unusual state, but keep the record) stay
///   in the managed set so a later deselection can still delete them.
fn apply_result_to_state(
    entry: &mut AgentGlobalMcp,
    result: &GlobalMcpApplyReport,
    selection: &[String],
) {
    let desired: std::collections::HashSet<&String> = selection.iter().collect();
    let mut managed: Vec<String> = result.write.written.clone();
    for previous in &entry.managed {
        if desired.contains(previous) && !managed.iter().any(|n| n == previous) {
            managed.push(previous.clone());
        }
    }
    entry.managed = managed;
    entry.skipped = result.write.skipped.clone();
    entry.rejected = result.rejected.iter().map(|r| r.name.clone()).collect();
    entry.last_applied = Some(chrono::Utc::now().to_rfc3339());
}

fn dedupe_preserving_order(input: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(input.len());
    for name in input {
        if seen.insert(name.clone()) {
            out.push(name);
        }
    }
    out
}

/// Diagnostics-friendly getter used by the tab header and drift refinement in
/// `sync/drift.rs`: which entry names does Automatic currently manage for
/// `agent_id`?  Always returns an empty vec if no state exists.
pub fn managed_entries_for(agent_id: &str) -> Vec<String> {
    load_global_mcp_state()
        .ok()
        .and_then(|s| s.agents.get(agent_id).map(|a| a.managed.clone()))
        .unwrap_or_default()
}

// Suppress unused-import warnings until a caller in the same crate lands.
#[allow(dead_code)]
fn _keep_pathbuf_used(_: PathBuf) {}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::with_test_home;
    use tempfile::TempDir;

    /// Seed a minimal MCP registry with `<name>` as a stdio server so
    /// `build_global_selected_servers` can find it.
    fn seed_registry(name: &str) {
        let raw = serde_json::to_string(&serde_json::json!({
            "type": "stdio",
            "command": "echo",
            "args": [name],
        }))
        .unwrap();
        crate::core::save_mcp_server_config(name, &raw).unwrap();
    }

    #[test]
    fn apply_persists_selection_and_populates_managed() {
        let tmp = TempDir::new().unwrap();
        with_test_home(tmp.path().to_path_buf(), || {
            seed_registry("alpha");
            let report =
                apply_global_mcp("cursor", vec!["alpha".to_string()]).expect("apply");
            assert!(report.write.written.contains(&"alpha".to_string()));

            let state = load_global_mcp_state().unwrap();
            let cursor = state.agents.get("cursor").expect("state entry");
            assert_eq!(cursor.selected, vec!["alpha"]);
            assert_eq!(cursor.managed, vec!["alpha"]);
        });
    }

    #[test]
    fn deselect_removes_only_managed_entries() {
        let tmp = TempDir::new().unwrap();
        with_test_home(tmp.path().to_path_buf(), || {
            seed_registry("alpha");
            apply_global_mcp("cursor", vec!["alpha".to_string()]).unwrap();
            let report = apply_global_mcp("cursor", Vec::new()).expect("apply empty");
            assert!(report.write.removed.contains(&"alpha".to_string()));

            let state = load_global_mcp_state().unwrap();
            let cursor = state.agents.get("cursor").expect("state entry");
            assert!(cursor.selected.is_empty());
            assert!(cursor.managed.is_empty());
        });
    }

    #[test]
    fn prune_drops_deleted_server_from_every_agent() {
        let tmp = TempDir::new().unwrap();
        with_test_home(tmp.path().to_path_buf(), || {
            seed_registry("shared");
            apply_global_mcp("cursor", vec!["shared".to_string()]).unwrap();
            apply_global_mcp("kiro", vec!["shared".to_string()]).unwrap();

            prune_server_from_global("shared");
            let state = load_global_mcp_state().unwrap();
            for agent_id in ["cursor", "kiro"] {
                let entry = state.agents.get(agent_id).expect(agent_id);
                assert!(
                    !entry.selected.contains(&"shared".to_string()),
                    "{}: selection kept 'shared'",
                    agent_id
                );
                assert!(
                    !entry.managed.contains(&"shared".to_string()),
                    "{}: managed kept 'shared'",
                    agent_id
                );
            }
        });
    }

    #[test]
    fn preview_classifies_writes_and_removes() {
        let tmp = TempDir::new().unwrap();
        with_test_home(tmp.path().to_path_buf(), || {
            seed_registry("alpha");
            seed_registry("beta");
            apply_global_mcp("cursor", vec!["alpha".to_string()]).unwrap();

            let preview =
                preview_global_mcp("cursor", &["beta".to_string()]).expect("preview");
            assert_eq!(preview.would_write, vec!["beta".to_string()]);
            assert_eq!(preview.would_remove, vec!["alpha".to_string()]);
        });
    }

    #[test]
    fn status_reflects_in_sync_after_apply() {
        let tmp = TempDir::new().unwrap();
        with_test_home(tmp.path().to_path_buf(), || {
            seed_registry("alpha");
            apply_global_mcp("cursor", vec!["alpha".to_string()]).unwrap();
            let status = global_mcp_status("cursor").expect("status");
            assert!(status.supported);
            assert!(status.in_sync);
            assert!(status.missing.is_empty());
        });
    }

    #[test]
    fn unsupported_agent_status_carries_the_mcp_note() {
        let tmp = TempDir::new().unwrap();
        with_test_home(tmp.path().to_path_buf(), || {
            // Goose is the deferred (unsupported) agent in v1.
            let status = global_mcp_status("goose").expect("status");
            assert!(!status.supported);
            assert!(status.target_path.is_none());
            assert!(status.note.is_some());
        });
    }
}

