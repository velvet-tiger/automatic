//! Agent module — each supported coding agent is its own type implementing
//! the [`Agent`] trait.
//!
//! ## Adding a new agent
//!
//! 1. Create `src/agent/my_agent.rs` with a public struct
//! 2. Implement `Agent` for it (the compiler enforces every method)
//! 3. Add a `mod my_agent;` line here
//! 4. Register an instance in [`all()`]
//!
//! Everything else (sync, autodetect, the frontend agent list) picks it up
//! automatically.

mod antigravity;
mod claude_code;
mod cline;
mod codex_cli;
mod cursor;
mod droid;
mod gemini_cli;
mod github_copilot;
mod goose;
mod junie;
mod kilo_code;
mod kiro;
mod opencode;
mod warp;

use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub use antigravity::Antigravity;
pub use claude_code::ClaudeCode;
pub use cline::Cline;
pub use codex_cli::CodexCli;
pub use cursor::Cursor;
pub use droid::Droid;
pub use gemini_cli::GeminiCli;
pub use github_copilot::GitHubCopilot;
pub use goose::Goose;
pub use junie::Junie;
pub use kilo_code::KiloCode;
pub use kiro::Kiro;
pub use opencode::OpenCode;
pub use warp::Warp;

// ── Trait ────────────────────────────────────────────────────────────────────

/// The contract every agent type must fulfil.
///
/// Each method corresponds to a capability that the sync/autodetect
/// orchestrator calls polymorphically.
pub trait Agent: Send + Sync {
    // ── Identity ────────────────────────────────────────────────────────

    /// Stable string id stored in `Project.agents` (e.g. `"claude"`).
    fn id(&self) -> &'static str;

    /// Human-friendly display name (e.g. `"Claude Code"`).
    fn label(&self) -> &'static str;

    /// Short description of the config file this agent uses.
    fn config_description(&self) -> &'static str;

    /// The filename used for the main project instructions file
    /// (e.g. `"CLAUDE.md"` for Claude Code, `"AGENTS.md"` for Codex).
    fn project_file_name(&self) -> &'static str;

    // ── Detection ───────────────────────────────────────────────────────

    /// Returns `true` if this agent appears to be in use in `dir`.
    fn detect_in(&self, dir: &Path) -> bool;

    /// Directories where this agent stores skills inside a project.
    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf>;

    // ── Config writing ──────────────────────────────────────────────────

    /// Write MCP server configs to the project directory in this agent's
    /// native format.  Returns the path of the file written.
    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String>;

    /// Copy selected skills into the project directory at the right
    /// location for this agent.  Returns the list of files written.
    ///
    /// `local_skill_names` lists skills that exist only in this project
    /// directory (not in the global registry).  These must be preserved
    /// during the cleanup phase rather than deleted.
    fn sync_skills(
        &self,
        dir: &Path,
        skill_contents: &[(String, String)],
        selected_names: &[String],
        local_skill_names: &[String],
    ) -> Result<Vec<String>, String>;

    // ── MCP capability ──────────────────────────────────────────────────

    /// Returns a human-readable note if this agent cannot have its MCP
    /// servers configured by Automatic (e.g. because the agent stores them
    /// in an internal database rather than a project file).
    ///
    /// `None` (the default) means Automatic writes MCP config normally.
    fn mcp_note(&self) -> Option<&'static str> {
        None
    }

    // ── Discovery ───────────────────────────────────────────────────────

    /// Scan this agent's config files in `dir` for MCP server definitions.
    /// Returns configs normalised to Nexus's canonical format.
    fn discover_mcp_servers(&self, dir: &Path) -> Map<String, Value>;

    // ── Cleanup ─────────────────────────────────────────────────────────

    /// Paths of MCP config files that are exclusively owned by Automatic for
    /// this agent.  These files are safe to delete outright when the agent is
    /// removed from a project.
    ///
    /// Agents that *merge* into shared config files (e.g. Gemini CLI writes
    /// into `.gemini/settings.json` which may contain other user settings)
    /// should return an empty vec here and override [`cleanup_mcp_config`]
    /// instead to strip only Automatic-managed sections.
    ///
    /// Default: empty vec — no files to delete.
    fn owned_config_paths(&self, _dir: &Path) -> Vec<PathBuf> {
        vec![]
    }

    /// Remove MCP configuration written by this agent from the project directory.
    /// Called when the agent is removed from a project.
    ///
    /// The default implementation deletes every file returned by
    /// [`owned_config_paths`] that exists on disk.  Agents that merge into
    /// shared config files should override this to strip only their managed
    /// sections rather than deleting the whole file.
    ///
    /// Returns paths of files deleted or modified.
    fn cleanup_mcp_config(&self, dir: &Path) -> Vec<String> {
        let mut removed = Vec::new();
        for path in self.owned_config_paths(dir) {
            if path.exists() {
                if fs::remove_file(&path).is_ok() {
                    removed.push(path.display().to_string());
                }
            }
        }
        removed
    }

    /// Returns the list of file/directory paths that *would* be affected when
    /// this agent's MCP config is cleaned up.  Used to populate the
    /// confirmation dialog shown to the user before removal.
    ///
    /// Default: owned_config_paths that currently exist on disk.
    fn cleanup_mcp_preview(&self, dir: &Path) -> Vec<String> {
        self.owned_config_paths(dir)
            .into_iter()
            .filter(|p| p.exists())
            .map(|p| p.display().to_string())
            .collect()
    }
}

// ── Frontend DTO ────────────────────────────────────────────────────────────

/// Serialisable metadata about an agent, returned to the frontend.
#[derive(Debug, Serialize)]
pub struct AgentInfo {
    pub id: String,
    pub label: String,
    pub description: String,
    /// Human-readable note about MCP limitations, if any.
    /// `None` means Automatic manages MCP config for this agent normally.
    pub mcp_note: Option<String>,
}

impl AgentInfo {
    pub fn from_agent(agent: &dyn Agent) -> Self {
        Self {
            id: agent.id().to_string(),
            label: agent.label().to_string(),
            description: agent.config_description().to_string(),
            mcp_note: agent.mcp_note().map(|s| s.to_string()),
        }
    }
}

// ── Registry ────────────────────────────────────────────────────────────────

/// Returns every registered agent instance.
///
/// To add a new agent, append it here.
pub fn all() -> Vec<&'static dyn Agent> {
    vec![
        &ClaudeCode,
        &Cursor,
        &GitHubCopilot,
        &KiloCode,
        &Junie,
        &Cline,
        &Kiro,
        &GeminiCli,
        &Antigravity,
        &Droid,
        &Goose,
        &CodexCli,
        &OpenCode,
        &Warp,
    ]
}

/// Look up an agent by its string id (e.g. from `Project.agents`).
pub fn from_id(id: &str) -> Option<&'static dyn Agent> {
    all().into_iter().find(|a| a.id() == id)
}

// ── Shared Helpers ──────────────────────────────────────────────────────────
//
// Utility functions used by multiple agent implementations.  Kept here so
// that each agent file stays focused on its own format logic.

/// Sync individual skill files under `<base_dir>/<name>/SKILL.md` by:
/// 1) removing directories not in the selected skill list (preserving local skills)
/// 2) writing the currently selected skills
///
/// `preserve_names` lists skill directory names that should never be removed
/// (e.g. local skills that only exist in this project directory).
///
/// Used by individual agent `sync_skills()` implementations and by drift
/// detection (which writes expected state into a tempdir).
pub(crate) fn sync_individual_skills(
    base_dir: &Path,
    skills: &[(String, String)],
    selected_skill_names: &[String],
    preserve_names: &[String],
    written: &mut Vec<String>,
) -> Result<(), String> {
    cleanup_skill_dir(base_dir, selected_skill_names, preserve_names)?;

    for (name, content) in skills {
        let skill_dir = base_dir.join(name);
        fs::create_dir_all(&skill_dir).map_err(|e| format!("Failed to create skill dir: {}", e))?;
        let skill_path = skill_dir.join("SKILL.md");
        fs::write(&skill_path, content)
            .map_err(|e| format!("Failed to write skill '{}': {}", name, e))?;
        written.push(skill_dir.display().to_string());
    }
    Ok(())
}

/// Copy skill directories from the global registry (`~/.agents/skills/`) into
/// the project's canonical `.agents/skills/` directory.  This is the first step
/// of project sync — it populates the project-local hub that other agent
/// directories will symlink to.
///
/// Each skill directory is copied recursively so that companion files
/// (`scripts/`, `docs/`, etc.) are included, not just `SKILL.md`.
///
/// `skill_contents` is used as a fallback: if a skill's source directory
/// cannot be found in the global registry, the SKILL.md content is written
/// directly.
pub(crate) fn copy_skills_to_project(
    project_skills_dir: &Path,
    skills: &[(String, String)],
    selected_skill_names: &[String],
    preserve_names: &[String],
    written: &mut Vec<String>,
) -> Result<(), String> {
    cleanup_skill_dir(project_skills_dir, selected_skill_names, preserve_names)?;

    for (name, content) in skills {
        let target_dir = project_skills_dir.join(name);

        // Remove existing entry so we get a clean copy
        if let Ok(meta) = target_dir.symlink_metadata() {
            if meta.file_type().is_symlink() {
                let _ = fs::remove_file(&target_dir);
            } else if meta.is_dir() {
                let _ = fs::remove_dir_all(&target_dir);
            }
        }

        // Try to copy the full directory from the global registry
        let copied = if let Ok(Some(src_dir)) = crate::core::get_skill_dir(name) {
            copy_dir_recursive(&src_dir, &target_dir).is_ok()
        } else {
            false
        };

        if !copied {
            // Fallback: write just SKILL.md
            fs::create_dir_all(&target_dir)
                .map_err(|e| format!("Failed to create skill dir: {}", e))?;
            fs::write(target_dir.join("SKILL.md"), content)
                .map_err(|e| format!("Failed to write skill '{}': {}", name, e))?;
        }

        written.push(target_dir.display().to_string());
    }
    Ok(())
}

/// Create directory symlinks from an agent's skill directory to the project's
/// canonical `.agents/skills/` directory.  This is the second step of project
/// sync — agents that store skills somewhere other than `.agents/skills/`
/// (e.g. `.claude/skills/`, `.cline/skills/`) get symlinks pointing back to
/// the project hub.
///
/// When the user's `skill_sync_mode` setting is `"copy"`, files are copied
/// instead of symlinked.
pub(crate) fn symlink_skills_from_project(
    agent_skills_dir: &Path,
    project_skills_dir: &Path,
    skills: &[(String, String)],
    selected_skill_names: &[String],
    preserve_names: &[String],
    written: &mut Vec<String>,
) -> Result<(), String> {
    cleanup_skill_dir(agent_skills_dir, selected_skill_names, preserve_names)?;

    let settings = crate::core::read_settings().unwrap_or_default();
    let use_symlink = settings.skill_sync_mode == "symlink";

    for (name, content) in skills {
        let link_path = agent_skills_dir.join(name);
        let target_dir = project_skills_dir.join(name);

        // Remove existing entry
        if let Ok(meta) = link_path.symlink_metadata() {
            if meta.file_type().is_symlink() {
                let _ = fs::remove_file(&link_path);
            } else if meta.is_dir() {
                let _ = fs::remove_dir_all(&link_path);
            }
        }

        let mut linked = false;
        if use_symlink && target_dir.exists() {
            #[cfg(unix)]
            {
                if std::os::unix::fs::symlink(&target_dir, &link_path).is_ok() {
                    linked = true;
                }
            }
            #[cfg(windows)]
            {
                if std::os::windows::fs::symlink_dir(&target_dir, &link_path).is_ok() {
                    linked = true;
                }
            }
        }

        if !linked {
            // Fallback: create directory and write SKILL.md as a copy
            fs::create_dir_all(&link_path)
                .map_err(|e| format!("Failed to create skill dir: {}", e))?;
            fs::write(link_path.join("SKILL.md"), content)
                .map_err(|e| format!("Failed to write skill '{}': {}", name, e))?;
        }

        written.push(link_path.display().to_string());
    }
    Ok(())
}

/// Remove skill entries from `base_dir` that are not in the selected set
/// and not in the preserve set.  Handles both real directories and symlinks.
fn cleanup_skill_dir(
    base_dir: &Path,
    selected_skill_names: &[String],
    preserve_names: &[String],
) -> Result<(), String> {
    let selected: HashSet<&str> = selected_skill_names.iter().map(|s| s.as_str()).collect();
    let preserved: HashSet<&str> = preserve_names.iter().map(|s| s.as_str()).collect();

    if !base_dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(base_dir)
        .map_err(|e| format!("Failed to read {}: {}", base_dir.display(), e))?
    {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let meta = match path.symlink_metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        // Accept real directories and symlinks (which may point to directories)
        if !meta.is_dir() && !meta.file_type().is_symlink() {
            continue;
        }

        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if crate::core::is_valid_name(name)
                && !selected.contains(name)
                && !preserved.contains(name)
            {
                if meta.file_type().is_symlink() {
                    fs::remove_file(&path).map_err(|e| {
                        format!("Failed to remove skill symlink '{}': {}", path.display(), e)
                    })?;
                } else {
                    fs::remove_dir_all(&path).map_err(|e| {
                        format!("Failed to remove skill dir '{}': {}", path.display(), e)
                    })?;
                }
            }
        }
    }
    Ok(())
}

/// Recursively copy a directory and all its contents.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst)
        .map_err(|e| format!("Failed to create dir '{}': {}", dst.display(), e))?;

    for entry in
        fs::read_dir(src).map_err(|e| format!("Failed to read dir '{}': {}", src.display(), e))?
    {
        let entry = entry.map_err(|e| e.to_string())?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).map_err(|e| {
                format!(
                    "Failed to copy '{}' -> '{}': {}",
                    src_path.display(),
                    dst_path.display(),
                    e
                )
            })?;
        }
    }
    Ok(())
}

/// Remove all Automatic-managed resources for a specific agent from a project
/// directory.  Called after the user confirms removal of an agent.
///
/// Steps performed:
/// 1. Call [`Agent::cleanup_mcp_config`] — removes or strips the agent's MCP
///    config file.
/// 2. Remove agent-specific skill directories (those returned by
///    [`Agent::skill_dirs`] that are NOT the shared `.agents/skills/` hub).
/// 3. If no agents in `remaining_agent_ids` use the `.agents/skills/` hub,
///    remove it too, and attempt to remove the now-empty `.agents/` directory.
///
/// Returns the list of paths that were successfully removed or modified.
pub(crate) fn cleanup_agent_from_project(
    agent_instance: &dyn Agent,
    dir: &Path,
    remaining_agent_ids: &[String],
) -> Vec<String> {
    let mut removed = Vec::new();
    let hub = dir.join(".agents").join("skills");

    // 1. Clean up MCP config
    removed.extend(agent_instance.cleanup_mcp_config(dir));

    // 2. Remove agent-specific skill directories (never the shared hub)
    for skill_dir in agent_instance.skill_dirs(dir) {
        if skill_dir != hub && skill_dir.exists() {
            if fs::remove_dir_all(&skill_dir).is_ok() {
                removed.push(skill_dir.display().to_string());
            }
        }
    }

    // 3. Remove the hub if no remaining agents use it
    let remaining_uses_hub = remaining_agent_ids
        .iter()
        .any(|id| from_id(id).map_or(false, |a| a.skill_dirs(dir).iter().any(|d| d == &hub)));

    if !remaining_uses_hub && hub.exists() {
        if fs::remove_dir_all(&hub).is_ok() {
            removed.push(hub.display().to_string());
            // Attempt to remove the parent .agents/ dir if it is now empty
            let agents_dir = dir.join(".agents");
            let _ = fs::remove_dir(&agents_dir); // silently ignored if not empty
        }
    }

    removed
}

/// Returns a list of file/directory paths that *would* be removed when
/// [`cleanup_agent_from_project`] is called.  Used to populate the
/// confirmation dialog before the user commits to the removal.
pub(crate) fn cleanup_agent_preview(
    agent_instance: &dyn Agent,
    dir: &Path,
    remaining_agent_ids: &[String],
) -> Vec<String> {
    let mut preview = Vec::new();
    let hub = dir.join(".agents").join("skills");

    // MCP config files
    preview.extend(agent_instance.cleanup_mcp_preview(dir));

    // Agent-specific skill directories
    for skill_dir in agent_instance.skill_dirs(dir) {
        if skill_dir != hub && skill_dir.exists() {
            preview.push(skill_dir.display().to_string());
        }
    }

    // Hub if no remaining agent uses it
    let remaining_uses_hub = remaining_agent_ids
        .iter()
        .any(|id| from_id(id).map_or(false, |a| a.skill_dirs(dir).iter().any(|d| d == &hub)));

    if !remaining_uses_hub && hub.exists() {
        preview.push(hub.display().to_string());
    }

    preview
}

/// Read a JSON config file containing MCP server definitions, extract them,
/// and optionally normalise each entry with the provided closure.
///
/// `root_key` is the top-level JSON key that holds the servers map
/// (e.g. `"mcpServers"` for Claude, `"mcp"` for OpenCode).
pub(crate) fn discover_mcp_servers_from_json(
    path: &Path,
    root_key: &str,
    normalise: fn(Value) -> Value,
) -> Map<String, Value> {
    let mut result = Map::new();

    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return result,
    };

    let map: Map<String, Value> = match serde_json::from_str::<Value>(&content) {
        Ok(Value::Object(m)) => m,
        _ => return result,
    };

    let servers_obj = match map.get(root_key) {
        Some(Value::Object(s)) => s,
        _ => return result,
    };

    for (name, config) in servers_obj {
        // Skip Automatic-managed entries — "automatic" (current) and "nexus"
        // (legacy name, pre-rename) are always injected at sync/drift-check
        // time from the live binary path and project name.  Storing them in
        // the shared registry would pollute every other project's drift
        // baseline with a stale project name.
        if name == "automatic" || name == "nexus" || !crate::core::is_valid_name(name) {
            continue;
        }
        result.insert(name.clone(), normalise(config.clone()));
    }

    result
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_from_id_roundtrips() {
        for agent in all() {
            let found = from_id(agent.id());
            assert!(found.is_some(), "from_id({}) returned None", agent.id());
            assert_eq!(found.unwrap().id(), agent.id());
        }
        assert!(from_id("unknown").is_none());
    }

    #[test]
    fn test_all_agents_have_unique_ids() {
        let ids: Vec<&str> = all().iter().map(|a| a.id()).collect();
        let unique: HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique.len());
    }
}
