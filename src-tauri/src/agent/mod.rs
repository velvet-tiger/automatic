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

mod claude_code;
mod codex_cli;
mod opencode;

use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub use claude_code::ClaudeCode;
pub use codex_cli::CodexCli;
pub use opencode::OpenCode;

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

    // ── Detection ───────────────────────────────────────────────────────

    /// Returns `true` if this agent appears to be in use in `dir`.
    fn detect_in(&self, dir: &Path) -> bool;

    /// Directories where this agent stores skills inside a project.
    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf>;

    // ── Config writing ──────────────────────────────────────────────────

    /// Write MCP server configs to the project directory in this agent's
    /// native format.  Returns the path of the file written.
    fn write_mcp_config(
        &self,
        dir: &Path,
        servers: &Map<String, Value>,
    ) -> Result<String, String>;

    /// Copy selected skills into the project directory at the right
    /// location for this agent.  Returns the list of files written.
    fn sync_skills(
        &self,
        dir: &Path,
        skill_contents: &[(String, String)],
        selected_names: &[String],
    ) -> Result<Vec<String>, String>;

    // ── Discovery ───────────────────────────────────────────────────────

    /// Scan this agent's config files in `dir` for MCP server definitions.
    /// Returns configs normalised to Nexus's canonical format.
    fn discover_mcp_servers(&self, dir: &Path) -> Map<String, Value>;
}

// ── Frontend DTO ────────────────────────────────────────────────────────────

/// Serialisable metadata about an agent, returned to the frontend.
#[derive(Debug, Serialize)]
pub struct AgentInfo {
    pub id: String,
    pub label: String,
    pub description: String,
}

impl AgentInfo {
    pub fn from_agent(agent: &dyn Agent) -> Self {
        Self {
            id: agent.id().to_string(),
            label: agent.label().to_string(),
            description: agent.config_description().to_string(),
        }
    }
}

// ── Registry ────────────────────────────────────────────────────────────────

/// Returns every registered agent instance.
///
/// To add a new agent, append it here.
pub fn all() -> Vec<&'static dyn Agent> {
    vec![&ClaudeCode, &CodexCli, &OpenCode]
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
/// 1) removing directories not in the selected skill list
/// 2) writing the currently selected skills
pub(crate) fn sync_individual_skills(
    base_dir: &Path,
    skills: &[(String, String)],
    selected_skill_names: &[String],
    written: &mut Vec<String>,
) -> Result<(), String> {
    let selected: HashSet<&str> = selected_skill_names.iter().map(|s| s.as_str()).collect();

    if base_dir.exists() {
        for entry in fs::read_dir(base_dir)
            .map_err(|e| format!("Failed to read {}: {}", base_dir.display(), e))?
        {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if crate::core::is_valid_name(name) && !selected.contains(name) {
                    fs::remove_dir_all(&path).map_err(|e| {
                        format!("Failed to remove skill dir '{}': {}", path.display(), e)
                    })?;
                }
            }
        }
    }

    if skills.is_empty() {
        return Ok(());
    }

    for (name, content) in skills {
        let skill_dir = base_dir.join(name);
        fs::create_dir_all(&skill_dir)
            .map_err(|e| format!("Failed to create skill dir: {}", e))?;
        let skill_path = skill_dir.join("SKILL.md");
        fs::write(&skill_path, content)
            .map_err(|e| format!("Failed to write skill '{}': {}", name, e))?;
        written.push(skill_path.display().to_string());
    }
    Ok(())
}

/// Build a markdown section containing all skills, wrapped in Nexus markers.
pub(crate) fn build_skills_markdown(skills: &[(String, String)]) -> String {
    let mut md = String::new();
    md.push_str("<!-- nexus:skills:start -->\n");
    md.push_str("<!-- This section is managed by Nexus. Do not edit manually. -->\n\n");

    for (name, content) in skills {
        md.push_str(&format!("## Skill: {}\n\n", name));
        md.push_str(content.trim());
        md.push_str("\n\n---\n\n");
    }

    md.push_str("<!-- nexus:skills:end -->\n");
    md
}

/// Merge a Nexus skills section into a file, replacing any existing section.
pub(crate) fn merge_skills_into_file(path: &Path, skills_section: &str) -> Result<(), String> {
    let existing = fs::read_to_string(path).unwrap_or_default();

    let start_marker = "<!-- nexus:skills:start -->";
    let end_marker = "<!-- nexus:skills:end -->";

    let final_content = if let (Some(start), Some(end)) =
        (existing.find(start_marker), existing.find(end_marker))
    {
        let before = &existing[..start];
        let after = &existing[end + end_marker.len()..];
        format!(
            "{}{}{}",
            before.trim_end(),
            format!("\n\n{}\n", skills_section),
            after.trim_start()
        )
    } else if existing.is_empty() {
        skills_section.to_string()
    } else {
        format!("{}\n\n{}", existing.trim_end(), skills_section)
    };

    fs::write(path, final_content)
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}

/// Remove the Nexus-managed skills section from a markdown file.
pub(crate) fn remove_skills_section_from_file(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    let existing = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

    let start_marker = "<!-- nexus:skills:start -->";
    let end_marker = "<!-- nexus:skills:end -->";

    let final_content = if let (Some(start), Some(end)) =
        (existing.find(start_marker), existing.find(end_marker))
    {
        let before = &existing[..start];
        let after = &existing[end + end_marker.len()..];
        format!(
            "{}{}",
            before.trim_end(),
            if after.trim().is_empty() { "" } else { "\n\n" }
        ) + after.trim_start()
    } else {
        existing
    };

    fs::write(path, final_content)
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))
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
        if name == "nexus" || !crate::core::is_valid_name(name) {
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
