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
#[cfg(test)]
mod mcp_format_tests;
mod opencode;
mod pi;
mod warp;
mod zed;

use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::core::Project;

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
pub use opencode::{
    clean_opencode_snapshots, clear_opencode_cache, CleanSnapshotsResult, ClearCacheResult,
    OpenCode,
};
pub use pi::Pi;
pub use warp::Warp;
pub use zed::Zed;

// ── Capabilities ─────────────────────────────────────────────────────────────

/// Declares which Automatic features an agent supports.
///
/// All fields default to `true`.  An agent overrides only the features it
/// cannot support (e.g. Warp sets `mcp_servers: false` because it manages MCP
/// via its own internal database rather than a project-level config file).
///
/// These flags are used by:
/// - The UI — to show/hide controls and display capability badges
/// - The sync pipeline — to skip steps that are not applicable
/// - The MCP server — to filter tools offered to a given agent
#[derive(Debug, Clone, Serialize)]
pub struct AgentCapabilities {
    /// Automatic can sync skills (SKILL.md files) to this agent's directory.
    pub skills: bool,
    /// This agent reads a project instructions file (e.g. `CLAUDE.md`).
    pub instructions: bool,
    /// Automatic can write MCP server config for this agent.
    pub mcp_servers: bool,
    /// Automatic can sync sub-agents to this agent's agents directory.
    /// Agents that don't have a sub-agent discovery location set this to false.
    pub agents: bool,
    /// Automatic can sync custom commands to this agent's commands directory.
    pub commands: bool,
    /// Automatic can sync lifecycle hooks to this agent's config.
    /// Only agents whose vendor ships a hook feature opt in.
    pub hooks: bool,
}

impl Default for AgentCapabilities {
    /// All capabilities enabled by default.
    fn default() -> Self {
        Self {
            skills: true,
            instructions: true,
            mcp_servers: true,
            agents: true,
            commands: false,
            hooks: false,
        }
    }
}

// ── Managed paths ──────────────────────────────────────────────────────────────

/// A single filesystem path that Automatic writes for an agent, tagged with
/// whether it is a directory.  Used to build `.gitignore` entries: directories
/// are emitted with a trailing slash, files without.
#[derive(Debug, Clone)]
pub struct ManagedPath {
    /// Absolute path under the project directory.
    pub path: PathBuf,
    /// `true` if this path is a directory Automatic writes into.
    pub is_dir: bool,
}

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

    /// Rewrite the "inherit from the environment" markers of one server entry.
    ///
    /// Automatic stores an empty string in `env` to mean "read this from the
    /// environment at launch rather than hardcoding it here".  Agents spell
    /// that differently, so [`prepare_mcp_servers`] delegates the rewrite per
    /// agent at write time and the secret itself never lands in a project file.
    ///
    /// The default substitutes Claude Code's `${KEY}` in place, which most
    /// agents follow.  Agents whose config format has no interpolation at all
    /// (Codex CLI's TOML) override this to reference the variables some other
    /// way, since a placeholder would reach the server as a literal string.
    fn rewrite_inherited_env(&self, server: &mut Map<String, Value>, keys: &[String]) {
        substitute_inherited_env(server, keys, |key| format!("${{{}}}", key));
    }

    /// Copy selected skills into the project directory at the right
    /// location for this agent.  Returns the list of files written.
    ///
    /// `local_skill_names` lists skills that exist only in this project
    /// directory (not in the global registry).  These must be preserved
    /// during the cleanup phase rather than deleted.
    ///
    /// The default writes every directory [`skill_dirs`] declares.  The sync
    /// engine does not call this — it populates the canonical
    /// `.agents/skills/` hub and then links each agent directory to it.  The
    /// only caller is drift detection, which writes the expected state into a
    /// tempdir and then compares every entry of `skill_dirs` against disk, so
    /// a directory this method skips is a directory drift can never check.
    fn sync_skills(
        &self,
        dir: &Path,
        skill_contents: &[(String, String)],
        selected_names: &[String],
        local_skill_names: &[String],
    ) -> Result<Vec<String>, String> {
        let mut written = Vec::new();
        for skills_dir in self.skill_dirs(dir) {
            sync_individual_skills(
                &skills_dir,
                skill_contents,
                selected_names,
                local_skill_names,
                &mut written,
            )?;
        }
        Ok(written)
    }

    /// Apply provider-specific instruction-file rule syncing.
    ///
    /// Return `Ok(Some(touched_paths))` when this agent has a custom
    /// instruction sync implementation and handled the write itself.
    /// Return `Ok(None)` to let the generic sync pipeline handle the file.
    fn sync_instruction_rules(
        &self,
        _project: &Project,
        _filename: &str,
        _rule_names: &[String],
        _custom_contents: &[String],
    ) -> Result<Option<Vec<String>>, String> {
        Ok(None)
    }

    // ── Capabilities ────────────────────────────────────────────────────

    /// Returns the set of Automatic features this agent supports.
    ///
    /// The default implementation returns all capabilities enabled.  Override
    /// this to disable specific features that this agent cannot support.
    ///
    /// Example — an agent that cannot have its MCP config written by Automatic:
    ///
    /// ```ignore
    /// fn capabilities(&self) -> AgentCapabilities {
    ///     AgentCapabilities { mcp_servers: false, ..Default::default() }
    /// }
    /// ```
    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities::default()
    }

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
    /// Returns configs normalised to Automatic's canonical format.
    fn discover_mcp_servers(&self, dir: &Path) -> Map<String, Value>;

    /// Returns `true` if this agent appears to be installed at the user level
    /// (binary present, or a global config directory exists in the home dir).
    ///
    /// Used during first-run to pre-filter which agents are worth scanning for
    /// global config.  The default implementation returns `false` so that agents
    /// without a reliable global install check are NOT auto-selected — they
    /// must override this method with a real detection heuristic.
    fn detect_global_install(&self) -> bool {
        false
    }

    /// Scan this agent's user-level (home-directory) config files for MCP
    /// server definitions that already exist outside of any project.
    ///
    /// Returns configs normalised to Automatic's canonical format, identical
    /// to what [`discover_mcp_servers`] returns for project-level files.
    ///
    /// The default implementation returns an empty map.  Agents with known
    /// global config paths should override this to read those files.
    fn discover_global_mcp_servers(&self) -> Map<String, Value> {
        Map::new()
    }

    /// Return home-directory skill directories that this agent uses
    /// **outside** of the external scan paths Automatic already tracks
    /// (`~/.agents/skills/` and `~/.claude/skills/`).
    ///
    /// Only paths that are genuinely additional need to be listed here —
    /// i.e. agent-specific global skill locations (e.g. `~/.cline/skills/`).
    /// Entries are scanned read-only so that skills installed outside
    /// Automatic are visible to the user and available for import into the
    /// managed library, but Automatic never writes to them.
    ///
    /// The default implementation returns an empty vec.  Agents with extra
    /// home-directory skill locations should override this.
    fn extra_global_skill_dirs(&self) -> Vec<PathBuf> {
        vec![]
    }

    // ── Sub-agents ─────────────────────────────────────────────────────────

    /// Return the directory where this agent looks for sub-agent definitions.
    /// Returns `None` if this agent does not support sub-agents.
    ///
    /// The default implementation returns `None`. Agents that support discovering
    /// sub-agents (like Claude Code with `.claude/agents/`) should override this.
    fn agents_dir(&self, _dir: &Path) -> Option<PathBuf> {
        None
    }

    /// Return the file extension for sub-agent files (without the dot).
    /// Default: "md" (Markdown with YAML frontmatter).
    /// Codex overrides this to return "toml".
    fn agents_file_ext(&self) -> &'static str {
        "md"
    }

    /// Convert agent content from the canonical format (Markdown + YAML frontmatter)
    /// to this agent's native format. Default: pass through unchanged.
    /// Codex overrides this to convert to TOML format.
    fn convert_agent_content(&self, content: &str, _name: &str) -> String {
        content.to_string()
    }

    /// Return the directory where this agent looks for custom command files.
    /// Returns `None` if this agent does not support project-local commands.
    fn commands_dir(&self, _dir: &Path) -> Option<PathBuf> {
        None
    }

    /// Return the file extension for command files (without the dot).
    fn commands_file_ext(&self) -> &'static str {
        "md"
    }

    /// Return the filename to use for a specific command.
    fn command_file_name(&self, machine_name: &str) -> String {
        format!("{machine_name}.{}", self.commands_file_ext())
    }

    /// Convert canonical Markdown command content into this agent's native format.
    fn convert_command_content(&self, content: &str, _name: &str) -> String {
        render_markdown_command(content)
    }

    // ── Hooks ──────────────────────────────────────────────────────────────

    /// Sync lifecycle hooks for this agent into the project directory.
    ///
    /// `hooks` is the set of hooks attached to the project whose `agent`
    /// field matches this agent's [`id`]. The implementation decides how to
    /// represent them on disk (e.g. merge into `.claude/settings.json`,
    /// write a dedicated `.codex/hooks.json`). Returns the paths of every
    /// file that was written or modified so the sync engine can track them
    /// for drift detection.
    ///
    /// The default implementation is a no-op so agents that don't support
    /// hooks don't have to override it. Agents that do must also set
    /// [`AgentCapabilities::hooks`] to `true`.
    fn sync_hooks(
        &self,
        _project_dir: &Path,
        _hooks: &[crate::core::Hook],
    ) -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }

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

    /// Every path under `dir` that Automatic writes for this agent, used to
    /// build the project's managed `.gitignore` block.
    ///
    /// The default composes the concrete files and directories the agent
    /// touches: its instruction file, skill directories, sub-agent and command
    /// directories, and owned MCP config files.  The pattern builder in
    /// `core::gitignore` then reduces these to the coarsest safe ignore — a
    /// subpath inside an agent's own directory (e.g. `.codex/agents`) collapses
    /// to the whole directory (`.codex/`), while a path inside a shared tool
    /// directory (`.github/`, `.vscode/`) is kept surgical.
    ///
    /// Agents that merge into a shared file and therefore keep it out of
    /// [`owned_config_paths`] (so cleanup does not clobber it) must list that
    /// file here explicitly if it should still be ignored — see the GitHub
    /// Copilot override for `.vscode/mcp.json`.
    fn managed_gitignore_paths(&self, dir: &Path) -> Vec<ManagedPath> {
        let mut out = vec![ManagedPath {
            path: dir.join(self.project_file_name()),
            is_dir: false,
        }];
        for d in self.skill_dirs(dir) {
            out.push(ManagedPath {
                path: d,
                is_dir: true,
            });
        }
        if let Some(d) = self.agents_dir(dir) {
            out.push(ManagedPath {
                path: d,
                is_dir: true,
            });
        }
        if let Some(d) = self.commands_dir(dir) {
            out.push(ManagedPath {
                path: d,
                is_dir: true,
            });
        }
        for p in self.owned_config_paths(dir) {
            out.push(ManagedPath {
                path: p,
                is_dir: false,
            });
        }
        out
    }
}

// ── Frontend DTO ────────────────────────────────────────────────────────────

/// Serialisable metadata about an agent, returned to the frontend.
#[derive(Debug, Serialize)]
pub struct AgentInfo {
    pub id: String,
    pub label: String,
    pub description: String,
    /// Which Automatic features this agent supports.
    pub capabilities: AgentCapabilities,
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
            capabilities: agent.capabilities(),
            mcp_note: agent.mcp_note().map(|s| s.to_string()),
        }
    }
}

// ── Registry ────────────────────────────────────────────────────────────────

/// Returns every registered agent instance, sorted alphabetically by label.
///
/// To add a new agent, append it to the vec below (order does not matter —
/// the vec is sorted before it is returned).
pub fn all() -> Vec<&'static dyn Agent> {
    let mut agents: Vec<&'static dyn Agent> = vec![
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
        &Pi,
        &Warp,
        &Zed,
    ];
    agents.sort_by(|a, b| a.label().to_lowercase().cmp(&b.label().to_lowercase()));
    agents
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

/// Copy skill directories from Automatic's managed library
/// (`~/.automatic/library/skills/`) into the project's canonical
/// `.agents/skills/` directory.  This is the first step of project sync —
/// it populates the project-local hub that other agent directories will
/// symlink to.
///
/// Each skill directory is copied recursively so that companion files
/// (`scripts/`, `docs/`, etc.) are included, not just `SKILL.md`.
///
/// `skill_contents` is used as a fallback: if a skill's source directory
/// cannot be found in the library, the SKILL.md content is written directly.
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
/// When the user's `sync_mode` setting is `"copy"`, files are copied
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
    let use_symlink = settings.sync_mode == "symlink";

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

pub(crate) fn parse_frontmatter(content: &str) -> (HashMap<String, String>, &str) {
    let mut frontmatter: HashMap<String, String> = HashMap::new();

    if !content.starts_with("---\n") && !content.starts_with("---\r\n") {
        return (frontmatter, content);
    }

    let after_first = &content[4..];
    let end_marker_pos = after_first
        .find("\n---")
        .or_else(|| after_first.find("\r\n---"));

    let end_marker_pos = match end_marker_pos {
        Some(pos) => pos,
        None => return (frontmatter, content),
    };

    let yaml_str = &after_first[..end_marker_pos];
    let body_start = end_marker_pos + 4;
    let body = if after_first[body_start..].starts_with('\n')
        || after_first[body_start..].starts_with("\r\n")
    {
        body_start + 1
    } else {
        body_start
    };
    let body = &after_first[body..];

    for line in yaml_str.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(colon_pos) = line.find(':') {
            let key = line[..colon_pos].trim();
            let mut value = line[colon_pos + 1..].trim();
            if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
                value = &value[1..value.len() - 1];
            } else if value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2 {
                value = &value[1..value.len() - 1];
            }
            frontmatter.insert(key.to_string(), value.to_string());
        }
    }

    (frontmatter, body)
}

pub(crate) fn render_markdown_command(content: &str) -> String {
    if content.starts_with("---\n") || content.starts_with("---\r\n") {
        let after_first = &content[4..];
        if let Some(end_marker_pos) = after_first
            .find("\n---")
            .or_else(|| after_first.find("\r\n---"))
        {
            let yaml_str = &after_first[..end_marker_pos];
            if yaml_str
                .lines()
                .any(|line| line.trim() == "automatic-managed: true")
            {
                return content.to_string();
            }

            let mut output = String::from("---\n");
            output.push_str(yaml_str);
            if !yaml_str.ends_with('\n') {
                output.push('\n');
            }
            output.push_str("automatic-managed: true\n---");
            output.push_str(&after_first[end_marker_pos + 4..]);
            if !output.ends_with('\n') {
                output.push('\n');
            }
            return output;
        }
    }

    let mut output = format!("---\nautomatic-managed: true\n---\n{}", content);
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

pub(crate) fn is_managed_command_file(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return false;
    };

    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };

    match ext {
        "md" => {
            let (frontmatter, _) = parse_frontmatter(&content);
            frontmatter
                .get("automatic-managed")
                .map(|value| value == "true")
                .unwrap_or(false)
        }
        "toml" => content
            .lines()
            .any(|line| line.trim() == "automatic_managed = true"),
        _ => false,
    }
}

pub(crate) fn copy_commands_to_project(
    project_commands_dir: &Path,
    workspace_commands: &[(String, String)],
    custom_commands: &[crate::core::CustomCommand],
    skip_custom_names: &std::collections::HashSet<String>,
) -> Result<Vec<String>, String> {
    let mut written = Vec::new();
    let mut expected: HashSet<String> = HashSet::new();
    let mut index_entries = Vec::new();

    let total_commands = workspace_commands.len() + custom_commands.len();
    if total_commands > 0 && !project_commands_dir.exists() {
        fs::create_dir_all(project_commands_dir).map_err(|e| {
            format!(
                "Failed to create commands dir '{}': {}",
                project_commands_dir.display(),
                e
            )
        })?;
    }

    for (name, content) in workspace_commands {
        let file_name = format!("{name}.md");
        expected.insert(file_name.clone());
        let path = project_commands_dir.join(&file_name);
        fs::write(&path, render_markdown_command(content))
            .map_err(|e| format!("Failed to write command '{}': {}", name, e))?;
        written.push(path.display().to_string());
        let (frontmatter, _) = parse_frontmatter(content);
        index_entries.push(CommandIndexEntry {
            name: name.clone(),
            description: frontmatter.get("description").cloned().unwrap_or_default(),
        });
    }

    for command in custom_commands {
        let file_name = format!("{}.md", command.name);
        expected.insert(file_name.clone());
        // Keep conflicting custom commands in `expected` so cleanup does not
        // delete them, but do not overwrite on-disk content.
        if skip_custom_names.contains(&command.name) {
            let (frontmatter, _) = parse_frontmatter(&command.content);
            index_entries.push(CommandIndexEntry {
                name: command.name.clone(),
                description: frontmatter.get("description").cloned().unwrap_or_default(),
            });
            continue;
        }
        let path = project_commands_dir.join(&file_name);
        fs::write(&path, render_markdown_command(&command.content))
            .map_err(|e| format!("Failed to write command '{}': {}", command.name, e))?;
        written.push(path.display().to_string());
        let (frontmatter, _) = parse_frontmatter(&command.content);
        index_entries.push(CommandIndexEntry {
            name: command.name.clone(),
            description: frontmatter.get("description").cloned().unwrap_or_default(),
        });
    }

    cleanup_stale_managed_command_files(project_commands_dir, &expected, &mut written)?;

    if !index_entries.is_empty() {
        let index_path = write_commands_index(project_commands_dir, &index_entries)?;
        written.push(index_path.display().to_string());
    } else if let Some(index_path) = cleanup_commands_index(project_commands_dir)? {
        written.push(index_path.display().to_string());
    }

    Ok(written)
}

pub(crate) fn symlink_commands_from_project(
    agent_commands_dir: &Path,
    project_commands_dir: &Path,
    workspace_commands: &[(String, String)],
    custom_commands: &[crate::core::CustomCommand],
    agent_instance: &dyn Agent,
) -> Result<Vec<String>, String> {
    let mut written = Vec::new();
    let mut expected: HashSet<String> = HashSet::new();
    let settings = crate::core::read_settings().unwrap_or_default();
    let use_symlink = settings.sync_mode == "symlink";

    let total_commands = workspace_commands.len() + custom_commands.len();
    if total_commands > 0 && !agent_commands_dir.exists() {
        fs::create_dir_all(agent_commands_dir).map_err(|e| {
            format!(
                "Failed to create commands dir '{}': {}",
                agent_commands_dir.display(),
                e
            )
        })?;
    }

    for (name, _) in workspace_commands {
        let file_name = agent_instance.command_file_name(name);
        expected.insert(file_name.clone());
        let link_path = agent_commands_dir.join(&file_name);
        let target_path = project_commands_dir.join(format!("{name}.md"));
        sync_command_link(&link_path, &target_path, use_symlink)?;
        written.push(link_path.display().to_string());
    }

    for command in custom_commands {
        let file_name = agent_instance.command_file_name(&command.name);
        expected.insert(file_name.clone());
        let link_path = agent_commands_dir.join(&file_name);
        let target_path = project_commands_dir.join(format!("{}.md", command.name));
        sync_command_link(&link_path, &target_path, use_symlink)?;
        written.push(link_path.display().to_string());
    }

    cleanup_stale_managed_command_files(agent_commands_dir, &expected, &mut written)?;

    Ok(written)
}

pub(crate) fn sync_commands_to_dir(
    commands_dir: &Path,
    workspace_commands: &[(String, String)],
    custom_commands: &[crate::core::CustomCommand],
    agent_instance: &dyn Agent,
    skip_custom_names: &std::collections::HashSet<String>,
) -> Result<Vec<String>, String> {
    let mut written = Vec::new();
    let mut expected: HashSet<String> = HashSet::new();

    let total_commands = workspace_commands.len() + custom_commands.len();
    if total_commands > 0 && !commands_dir.exists() {
        fs::create_dir_all(commands_dir).map_err(|e| {
            format!(
                "Failed to create commands dir '{}': {}",
                commands_dir.display(),
                e
            )
        })?;
    }

    for (name, content) in workspace_commands {
        let file_name = agent_instance.command_file_name(name);
        expected.insert(file_name.clone());
        let path = commands_dir.join(&file_name);
        let rendered = agent_instance.convert_command_content(content, name);
        fs::write(&path, rendered)
            .map_err(|e| format!("Failed to write command '{}': {}", name, e))?;
        written.push(path.display().to_string());
    }

    for command in custom_commands {
        let file_name = agent_instance.command_file_name(&command.name);
        expected.insert(file_name.clone());
        if skip_custom_names.contains(&command.name) {
            continue;
        }
        let path = commands_dir.join(&file_name);
        let rendered = agent_instance.convert_command_content(&command.content, &command.name);
        fs::write(&path, rendered)
            .map_err(|e| format!("Failed to write command '{}': {}", command.name, e))?;
        written.push(path.display().to_string());
    }

    cleanup_stale_managed_command_files(commands_dir, &expected, &mut written)?;

    Ok(written)
}

fn sync_command_link(
    link_path: &Path,
    target_path: &Path,
    use_symlink: bool,
) -> Result<(), String> {
    if let Ok(meta) = link_path.symlink_metadata() {
        if meta.file_type().is_symlink() || meta.is_file() {
            fs::remove_file(link_path).map_err(|e| {
                format!(
                    "Failed to replace command file '{}': {}",
                    link_path.display(),
                    e
                )
            })?;
        }
    }

    if use_symlink {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target_path, link_path).map_err(|e| {
                format!(
                    "Failed to symlink command '{}' -> '{}': {}",
                    link_path.display(),
                    target_path.display(),
                    e
                )
            })?;
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(target_path, link_path).map_err(|e| {
                format!(
                    "Failed to symlink command '{}' -> '{}': {}",
                    link_path.display(),
                    target_path.display(),
                    e
                )
            })?;
        }
    } else {
        fs::copy(target_path, link_path).map_err(|e| {
            format!(
                "Failed to copy command '{}' -> '{}': {}",
                target_path.display(),
                link_path.display(),
                e
            )
        })?;
    }

    Ok(())
}

fn cleanup_stale_managed_command_files(
    commands_dir: &Path,
    expected: &HashSet<String>,
    written: &mut Vec<String>,
) -> Result<(), String> {
    if commands_dir.exists() {
        for entry in fs::read_dir(commands_dir)
            .map_err(|e| format!("Failed to read {}: {}", commands_dir.display(), e))?
        {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };

            if expected.contains(file_name) || !is_managed_command_file(&path) {
                continue;
            }

            fs::remove_file(&path).map_err(|e| {
                format!("Failed to remove stale command '{}': {}", path.display(), e)
            })?;
            written.push(path.display().to_string());
        }

        if fs::read_dir(commands_dir)
            .map_err(|e| format!("Failed to read {}: {}", commands_dir.display(), e))?
            .next()
            .is_none()
        {
            let _ = fs::remove_dir(commands_dir);
        }
    }

    Ok(())
}

#[derive(Debug)]
struct CommandIndexEntry {
    name: String,
    description: String,
}

fn write_commands_index(
    commands_dir: &Path,
    entries: &[CommandIndexEntry],
) -> Result<std::path::PathBuf, String> {
    let mut sorted_entries: Vec<&CommandIndexEntry> = entries.iter().collect();
    sorted_entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    let mut content = String::from(
        "---\nautomatic-managed: true\n---\n# Commands Index\n\nThis index is generated by Automatic.\n\n",
    );

    for entry in sorted_entries {
        content.push_str(&format!("- `{}`\n", entry.name));
        if !entry.description.is_empty() {
            content.push_str(&format!("  - Description: {}\n", entry.description));
        }
        content.push_str(&format!("  - File: `.agents/commands/{}.md`\n", entry.name));
    }

    let index_path = commands_index_path(commands_dir);
    fs::write(&index_path, content).map_err(|e| {
        format!(
            "Failed to write command index '{}': {}",
            index_path.display(),
            e
        )
    })?;

    Ok(index_path)
}

fn commands_index_path(commands_dir: &Path) -> std::path::PathBuf {
    commands_dir
        .parent()
        .map(|parent| parent.join("commands-index.md"))
        .unwrap_or_else(|| commands_dir.join("commands-index.md"))
}

fn cleanup_commands_index(commands_dir: &Path) -> Result<Option<std::path::PathBuf>, String> {
    let index_path = commands_index_path(commands_dir);
    if index_path.exists() && is_managed_command_file(&index_path) {
        fs::remove_file(&index_path).map_err(|e| {
            format!(
                "Failed to remove stale command index '{}': {}",
                index_path.display(),
                e
            )
        })?;
        return Ok(Some(index_path));
    }

    Ok(None)
}

fn cleanup_command_files(agent_instance: &dyn Agent, dir: &Path) -> Vec<String> {
    let Some(commands_dir) = agent_instance.commands_dir(dir) else {
        return vec![];
    };
    if !commands_dir.exists() {
        return vec![];
    }

    let mut removed = Vec::new();
    if let Ok(entries) = fs::read_dir(&commands_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && is_managed_command_file(&path) && fs::remove_file(&path).is_ok() {
                removed.push(path.display().to_string());
            }
        }
    }

    let is_empty = fs::read_dir(&commands_dir)
        .ok()
        .and_then(|mut entries| entries.next())
        .is_none();
    if is_empty {
        let _ = fs::remove_dir(&commands_dir);
    }

    removed
}

fn cleanup_command_preview(agent_instance: &dyn Agent, dir: &Path) -> Vec<String> {
    let Some(commands_dir) = agent_instance.commands_dir(dir) else {
        return vec![];
    };
    if !commands_dir.exists() {
        return vec![];
    }

    let mut preview = Vec::new();
    if let Ok(entries) = fs::read_dir(&commands_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && is_managed_command_file(&path) {
                preview.push(path.display().to_string());
            }
        }
    }
    preview
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
    removed.extend(cleanup_command_files(agent_instance, dir));

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
    preview.extend(cleanup_command_preview(agent_instance, dir));

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

/// Scan the agent-specific extra global skill directories returned by
/// [`Agent::extra_global_skill_dirs`] and return skills not already known
/// to Automatic (i.e. missing from the managed library and from the
/// external `~/.agents/skills/` / `~/.claude/skills/` scan paths).
///
/// Returns `(name, content)` pairs — the skill name and its `SKILL.md` content —
/// ready to be saved via `core::save_skill`.
pub(crate) fn collect_new_skills_from_extra_dirs(agent: &dyn Agent) -> Vec<(String, String)> {
    let known_names: std::collections::HashSet<String> = crate::core::list_skill_names()
        .unwrap_or_default()
        .into_iter()
        .collect();

    let mut results: Vec<(String, String)> = Vec::new();

    for dir in agent.extra_global_skill_dirs() {
        if !dir.exists() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !crate::core::is_valid_name(name) || known_names.contains(name) {
                continue;
            }
            let skill_file = path.join("SKILL.md");
            if let Ok(content) = fs::read_to_string(&skill_file) {
                results.push((name.to_string(), content));
            }
        }
    }

    results
}

/// Return the user's home directory, or `None` if it cannot be determined.
///
/// Thin wrapper around [`dirs::home_dir`] used by agent implementations to
/// resolve global config paths (e.g. `~/.claude/settings.json`).
pub(crate) fn home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}

/// Return `true` if `cli_name` resolves to an executable on `$PATH`.
///
/// Uses the system `which` command to avoid depending on an additional crate.
pub(crate) fn cli_available(cli_name: &str) -> bool {
    std::process::Command::new("which")
        .arg(cli_name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Replace each inherited key's `env` value in place with `render(key)`.
///
/// Shared by every agent whose config format supports variable expansion; the
/// only difference between them is the spelling of the placeholder.
pub(crate) fn substitute_inherited_env(
    server: &mut Map<String, Value>,
    keys: &[String],
    render: impl Fn(&str) -> String,
) {
    let Some(Value::Object(env)) = server.get_mut("env") else {
        return;
    };
    for key in keys {
        env.insert(key.clone(), Value::String(render(key)));
    }
}

/// Render the canonical MCP server map into the form an agent's writer expects:
/// expand "inherit from the environment" markers into that agent's own
/// placeholder syntax and drop Automatic-internal metadata.
///
/// Both the sync engine and drift detection must call this before
/// [`Agent::write_mcp_config`].  If only one of them does, the expected config
/// and the config on disk disagree and every project reports permanent drift.
pub(crate) fn prepare_mcp_servers(
    agent: &dyn Agent,
    servers: &Map<String, Value>,
) -> Map<String, Value> {
    let mut prepared = Map::new();

    for (name, config) in servers {
        let mut server = config.clone();
        if let Some(obj) = server.as_object_mut() {
            obj.retain(|key, _| !key.starts_with('_'));

            let inherited: Vec<String> = obj
                .get("env")
                .and_then(|env| env.as_object())
                .map(|env| {
                    env.iter()
                        .filter(|(_, value)| value.as_str() == Some(""))
                        .map(|(key, _)| key.clone())
                        .collect()
                })
                .unwrap_or_default();
            if !inherited.is_empty() {
                agent.rewrite_inherited_env(obj, &inherited);
            }
        }
        prepared.insert(name.clone(), server);
    }

    prepared
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
        // Never re-import an Automatic-generated proxy stub.  Remote (HTTP/SSE)
        // servers with a stored OAuth token are written into project files as a
        // local `mcp-proxy` stub (see `sync::helpers::build_selected_servers`) so
        // the token stays in the keychain.  Discovering that stub and saving it
        // back over the authoritative registry entry is what silently reverts a
        // remote server to "local" and breaks the proxy.
        if is_automatic_proxy_stub(config) {
            continue;
        }
        result.insert(name.clone(), normalise(config.clone()));
    }

    result
}

/// Read a JSON config file that Automatic merges into rather than owns.
///
/// An absent or empty file yields an empty object.  A file that exists, has
/// content, and does not parse is an error: the caller is about to write the
/// file back out, and falling through to an empty map would discard every
/// setting the user has in there.
pub(crate) fn read_mergeable_json_object(path: &Path) -> Result<Map<String, Value>, String> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let raw = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    if raw.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(&raw) {
        Ok(Value::Object(m)) => Ok(m),
        Ok(_) => Err(format!(
            "{} must contain a JSON object — fix it or remove the file",
            path.display()
        )),
        Err(e) => Err(format!(
            "Failed to parse {} — fix the syntax or remove the file: {}",
            path.display(),
            e
        )),
    }
}

/// Write MCP servers in the OpenCode config dialect, merging into `path`.
///
/// The dialect: a top-level `mcp` object where stdio servers become
/// `type: "local"` with `command` as a `[command, ...args]` array and
/// `environment` in place of `env`, and HTTP/SSE servers become
/// `type: "remote"`.  OpenCode and Kilo both read it.
///
/// The file is shared with the user — it also carries their `model`,
/// `permission`, `instructions` and `agent` keys — so every key other than
/// `$schema` and `mcp` is preserved, and a malformed target is an error rather
/// than a clobber.
pub(crate) fn write_opencode_dialect_mcp_config(
    path: &Path,
    schema_url: &str,
    servers: &Map<String, Value>,
) -> Result<String, String> {
    let mut root = read_mergeable_json_object(path)?;

    let mut dialect_servers = Map::new();
    for (name, config) in servers {
        let transport = config
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("stdio");

        let mut server = Map::new();

        match transport {
            "http" | "sse" => {
                server.insert("type".to_string(), json!("remote"));

                if let Some(url) = config.get("url") {
                    server.insert("url".to_string(), url.clone());
                }
                if let Some(headers) = config.get("headers") {
                    server.insert("headers".to_string(), headers.clone());
                }
                if let Some(oauth) = config.get("oauth") {
                    server.insert("oauth".to_string(), oauth.clone());
                }
            }
            _ => {
                server.insert("type".to_string(), json!("local"));

                // command as array: [command, ...args]
                let mut cmd_array: Vec<Value> = Vec::new();
                if let Some(command) = config.get("command").and_then(|v| v.as_str()) {
                    cmd_array.push(json!(command));
                }
                if let Some(args) = config.get("args").and_then(|v| v.as_array()) {
                    for arg in args {
                        cmd_array.push(arg.clone());
                    }
                }
                if !cmd_array.is_empty() {
                    server.insert("command".to_string(), Value::Array(cmd_array));
                }

                // "environment" instead of "env"
                if let Some(env) = config.get("env").and_then(|v| v.as_object()) {
                    if !env.is_empty() {
                        server.insert("environment".to_string(), Value::Object(env.clone()));
                    }
                }
            }
        }

        if let Some(enabled) = config.get("enabled") {
            if enabled.as_bool() == Some(false) {
                server.insert("enabled".to_string(), json!(false));
            }
        }
        if let Some(timeout) = config.get("timeout") {
            server.insert("timeout".to_string(), timeout.clone());
        }

        dialect_servers.insert(name.clone(), Value::Object(server));
    }

    root.insert("$schema".to_string(), json!(schema_url));
    root.insert("mcp".to_string(), Value::Object(dialect_servers));

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
        }
    }

    let content = serde_json::to_string_pretty(&Value::Object(root))
        .map_err(|e| format!("JSON error: {}", e))?;
    fs::write(path, content).map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;

    Ok(path.display().to_string())
}

/// Convert one OpenCode-dialect MCP server entry back to Automatic's canonical
/// format — the inverse of [`write_opencode_dialect_mcp_config`].
///
/// - `type: "local"` → `type: "stdio"`, command array → command + args
/// - `type: "remote"` → `type: "http"`
/// - `environment` → `env`
///
/// Kept a plain `fn` so it can be passed to [`discover_mcp_servers_from_json`]
/// as a function pointer.
pub(crate) fn normalise_opencode_dialect_server(mut config: Value) -> Value {
    if let Some(obj) = config.as_object_mut() {
        if let Some(Value::String(t)) = obj.get("type") {
            if t == "local" {
                obj.insert("type".to_string(), json!("stdio"));
                if let Some(Value::Array(cmd_arr)) = obj.remove("command") {
                    if !cmd_arr.is_empty() {
                        obj.insert("command".to_string(), cmd_arr[0].clone());
                        if cmd_arr.len() > 1 {
                            obj.insert("args".to_string(), Value::Array(cmd_arr[1..].to_vec()));
                        }
                    }
                }
                if let Some(env) = obj.remove("environment") {
                    obj.insert("env".to_string(), env);
                }
            } else if t == "remote" {
                obj.insert("type".to_string(), json!("http"));
            }
        }
    }
    config
}

/// Return `true` if `config` is an Automatic-generated MCP proxy stub, i.e. a
/// local stdio entry that invokes the Automatic binary with the `mcp-proxy`
/// subcommand.  Such stubs are emitted for remote OAuth servers and must never
/// be imported back into the shared MCP registry.
pub(crate) fn is_automatic_proxy_stub(config: &Value) -> bool {
    fn mentions_mcp_proxy(value: Option<&Value>) -> bool {
        match value {
            // Claude Code / Cursor / most agents: `"args": ["mcp-proxy", name]`.
            Some(Value::Array(items)) => items.iter().any(|v| v.as_str() == Some("mcp-proxy")),
            // Agents that model the command as a single array/string may fold
            // the subcommand into `command`; cover that too.
            Some(Value::String(s)) => s == "mcp-proxy",
            _ => false,
        }
    }

    match config.as_object() {
        Some(obj) => mentions_mcp_proxy(obj.get("args")) || mentions_mcp_proxy(obj.get("command")),
        None => false,
    }
}

// ── Hook sync helpers ───────────────────────────────────────────────────────
//
// Shared by every agent that syncs lifecycle hooks (Claude Code, Codex CLI,
// Cursor).  Script-handler bodies are written into an agent-specific scripts
// directory with a shebang, a `managed-by-automatic` marker comment, and 0755
// permissions; the marker is what later identifies files safe to clean up.

/// Stable identifier used as the script filename stem — a slug derived from
/// the hook's display name, falling back to "hook" when it slugs to nothing.
pub(crate) fn hook_slug(hook: &crate::core::Hook) -> String {
    let slug: String = hook
        .name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if slug.is_empty() {
        "hook".to_string()
    } else {
        slug
    }
}

/// Pick a filename extension for a script handler based on its interpreter so
/// the filename hints at the content type.
pub(crate) fn hook_script_extension(interpreter: &str) -> &'static str {
    let lower = interpreter.trim().to_ascii_lowercase();
    if lower.ends_with("python") || lower.ends_with("python3") {
        "py"
    } else if lower.ends_with("node") || lower.ends_with("nodejs") {
        "js"
    } else if lower.ends_with("zsh") {
        "zsh"
    } else if lower.ends_with("fish") {
        "fish"
    } else if lower.ends_with("pwsh") || lower.ends_with("powershell") {
        "ps1"
    } else {
        "sh"
    }
}

/// Prepend a shebang derived from `interpreter` when the script has none.
pub(crate) fn hook_ensure_shebang(script: &str, interpreter: &str) -> String {
    let trimmed = script.trim_start();
    if trimmed.starts_with("#!") {
        return script.to_string();
    }
    let interp = interpreter.trim();
    if interp.is_empty() {
        return script.to_string();
    }
    let shebang = if interp.starts_with('/') {
        format!("#!{}\n", interp)
    } else {
        format!("#!/usr/bin/env {}\n", interp)
    };
    format!("{}{}", shebang, script)
}

/// Drop a marker comment into the script body so cleanup can later identify
/// files we wrote without disturbing user-authored scripts living next to
/// ours in the scripts directory.
pub(crate) fn hook_annotate_managed_script(body: &str) -> String {
    const MARKER: &str = "# managed-by-automatic — do not edit by hand\n";
    if body.contains("managed-by-automatic") {
        return body.to_string();
    }
    if let Some(rest) = body.strip_prefix("#!") {
        if let Some(newline_idx) = rest.find('\n') {
            let (shebang_line, rest_after) = body.split_at("#!".len() + newline_idx + 1);
            return format!("{}{}{}", shebang_line, MARKER, rest_after);
        }
    }
    format!("{}{}", MARKER, body)
}

/// Write a script-handler body to `<scripts_dir>/<slug>.<ext>` with shebang,
/// managed marker, and 0755 permissions.  Returns the written path.
pub(crate) fn write_managed_hook_script(
    scripts_dir: &Path,
    hook: &crate::core::Hook,
    interpreter: &str,
    script: &str,
) -> Result<PathBuf, String> {
    let ext = hook_script_extension(interpreter);
    let path = scripts_dir.join(format!("{}.{}", hook_slug(hook), ext));
    let body = hook_annotate_managed_script(&hook_ensure_shebang(script, interpreter));
    fs::write(&path, body)
        .map_err(|e| format!("Failed to write hook script '{}': {}", path.display(), e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(&path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = fs::set_permissions(&path, perms);
        }
    }
    Ok(path)
}

/// Delete script files in the scripts directory whose body carries the
/// `managed-by-automatic` marker but are not in the current keep-list.
/// User-authored scripts (no marker) are never deleted.
pub(crate) fn cleanup_managed_hook_scripts(
    scripts_dir: &Path,
    keep_paths: &[PathBuf],
) -> Result<(), String> {
    if !scripts_dir.exists() {
        return Ok(());
    }
    let entries = match fs::read_dir(scripts_dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };

    let keep_names: HashSet<String> = keep_paths
        .iter()
        .filter_map(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
        })
        .collect();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if keep_names.contains(name) {
            continue;
        }
        if let Ok(content) = fs::read_to_string(&path) {
            if content.contains("managed-by-automatic") {
                let _ = fs::remove_file(&path);
            }
        }
    }
    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::CustomCommand;
    use std::collections::HashSet;
    use std::fs;
    use tempfile::tempdir;

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

    #[test]
    fn proxy_stub_is_detected() {
        // Standard shape written by build_selected_servers for a remote OAuth
        // server: local stdio invocation of the Automatic binary.
        let stub = serde_json::json!({
            "command": "/usr/local/bin/automatic",
            "args": ["mcp-proxy", "linear"],
        });
        assert!(is_automatic_proxy_stub(&stub));

        // A genuine remote config is not a stub.
        let remote = serde_json::json!({
            "type": "http",
            "url": "https://mcp.linear.app/mcp",
        });
        assert!(!is_automatic_proxy_stub(&remote));

        // A user's ordinary local stdio server is not a stub.
        let local = serde_json::json!({
            "command": "npx",
            "args": ["-y", "some-mcp-server"],
        });
        assert!(!is_automatic_proxy_stub(&local));
    }

    #[test]
    fn discover_skips_proxy_stub_but_keeps_real_servers() {
        // Regression test for the bug where a remote OAuth server (e.g. Linear)
        // silently reverted to "local" after a couple of syncs: its proxy stub
        // in `.mcp.json` was discovered and saved back over the registry entry.
        // Discovery must drop the stub while still importing real servers.
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join(".mcp.json");
        fs::write(
            &path,
            serde_json::to_string_pretty(&serde_json::json!({
                "mcpServers": {
                    // Automatic's own server — always excluded.
                    "automatic": { "command": "automatic", "args": ["mcp-serve"] },
                    // Proxy stub for a remote OAuth server — must be excluded so
                    // it never overwrites the authoritative remote registry entry.
                    "linear": { "command": "/usr/local/bin/automatic", "args": ["mcp-proxy", "linear"] },
                    // A genuine local server the user configured — must survive.
                    "fetch": { "command": "npx", "args": ["-y", "fetch-mcp"] },
                }
            }))
            .expect("serialize"),
        )
        .expect("write .mcp.json");

        let discovered = discover_mcp_servers_from_json(&path, "mcpServers", |v| v);

        assert!(
            !discovered.contains_key("automatic"),
            "automatic must never be imported"
        );
        assert!(
            !discovered.contains_key("linear"),
            "proxy stub must not be re-imported (would revert remote → local)"
        );
        assert!(
            discovered.contains_key("fetch"),
            "genuine local servers must still be discovered"
        );
    }

    #[test]
    fn sync_commands_writes_expected_commands_and_removes_stale_managed_files() {
        let dir = tempdir().expect("tempdir");
        let commands_dir = dir.path().join("commands");
        fs::create_dir_all(&commands_dir).expect("create commands dir");

        fs::write(
            commands_dir.join("stale.md"),
            "---\nautomatic-managed: true\n---\nStale command.\n",
        )
        .expect("write stale command");
        fs::write(
            commands_dir.join("keep.md"),
            "---\ndescription: user command\n---\nDo not remove.\n",
        )
        .expect("write unmanaged command");

        let written = sync_commands_to_dir(
            &commands_dir,
            &[(
                "workspace-cmd".to_string(),
                "---\ndescription: workspace\n---\nRun workspace.\n".to_string(),
            )],
            &[CustomCommand {
                name: "custom-cmd".to_string(),
                content: "Run custom.\n".to_string(),
            }],
            &ClaudeCode,
            &HashSet::new(),
        )
        .expect("sync commands");

        let workspace_path = commands_dir.join("workspace-cmd.md");
        let custom_path = commands_dir.join("custom-cmd.md");
        assert!(written.iter().any(|p| p.ends_with("workspace-cmd.md")));
        assert!(written.iter().any(|p| p.ends_with("custom-cmd.md")));
        assert!(written.iter().any(|p| p.ends_with("stale.md")));
        assert!(workspace_path.exists());
        assert!(custom_path.exists());
        assert!(!commands_dir.join("stale.md").exists());
        assert!(commands_dir.join("keep.md").exists());
        assert!(is_managed_command_file(&workspace_path));
        assert!(is_managed_command_file(&custom_path));
    }

    #[test]
    fn copy_commands_to_project_writes_selected_and_removes_stale_managed_files() {
        let dir = tempdir().expect("tempdir");
        let commands_dir = dir.path().join("commands");
        fs::create_dir_all(&commands_dir).expect("create commands dir");

        fs::write(
            commands_dir.join("stale.md"),
            "---\nautomatic-managed: true\n---\nStale command.\n",
        )
        .expect("write stale command");
        fs::write(
            commands_dir.join("keep.md"),
            "---\ndescription: user command\n---\nDo not remove.\n",
        )
        .expect("write unmanaged command");

        let written = copy_commands_to_project(
            &commands_dir,
            &[(
                "workspace-cmd".to_string(),
                "---\ndescription: workspace\n---\nRun workspace.\n".to_string(),
            )],
            &[CustomCommand {
                name: "custom-cmd".to_string(),
                content: "Run custom.\n".to_string(),
            }],
            &HashSet::new(),
        )
        .expect("copy commands");

        let workspace_path = commands_dir.join("workspace-cmd.md");
        let custom_path = commands_dir.join("custom-cmd.md");
        let index_path = dir.path().join("commands-index.md");
        assert!(written.iter().any(|p| p.ends_with("workspace-cmd.md")));
        assert!(written.iter().any(|p| p.ends_with("custom-cmd.md")));
        assert!(written.iter().any(|p| p.ends_with("stale.md")));
        assert!(written.iter().any(|p| p.ends_with("commands-index.md")));
        assert!(workspace_path.exists());
        assert!(custom_path.exists());
        assert!(index_path.exists());
        assert!(!commands_dir.join("stale.md").exists());
        assert!(commands_dir.join("keep.md").exists());
        assert!(is_managed_command_file(&workspace_path));
        assert!(is_managed_command_file(&custom_path));
        let index_content = fs::read_to_string(&index_path).expect("read command index");
        assert!(index_content.contains("# Commands Index"));
        assert!(index_content.contains("`custom-cmd`"));
        assert!(index_content.contains("`workspace-cmd`"));
        assert!(index_content.contains("Description: workspace"));
        assert!(index_content.contains("File: `.agents/commands/custom-cmd.md`"));
    }

    #[test]
    fn copy_commands_to_project_removes_managed_index_when_no_commands_remain() {
        let dir = tempdir().expect("tempdir");
        let commands_dir = dir.path().join("commands");
        fs::create_dir_all(&commands_dir).expect("create commands dir");

        copy_commands_to_project(
            &commands_dir,
            &[(
                "workspace-cmd".to_string(),
                "---\ndescription: workspace\n---\nRun workspace.\n".to_string(),
            )],
            &[],
            &HashSet::new(),
        )
        .expect("seed commands");

        assert!(dir.path().join("commands-index.md").exists());

        let written = copy_commands_to_project(&commands_dir, &[], &[], &HashSet::new())
            .expect("clear commands");

        assert!(written.iter().any(|p| p.ends_with("commands-index.md")));
        assert!(!dir.path().join("commands-index.md").exists());
        assert!(!commands_dir.join("workspace-cmd.md").exists());
    }

    #[test]
    fn symlink_commands_from_project_reuses_canonical_markdown_commands() {
        let dir = tempdir().expect("tempdir");
        let project_commands_dir = dir.path().join(".agents").join("commands");
        fs::create_dir_all(&project_commands_dir).expect("create project commands dir");

        copy_commands_to_project(
            &project_commands_dir,
            &[(
                "workspace-cmd".to_string(),
                "---\ndescription: workspace\n---\nRun workspace.\n".to_string(),
            )],
            &[CustomCommand {
                name: "custom-cmd".to_string(),
                content: "Run custom.\n".to_string(),
            }],
            &HashSet::new(),
        )
        .expect("copy commands");

        let agent_commands_dir = dir.path().join(".claude").join("commands");
        fs::create_dir_all(&agent_commands_dir).expect("create agent commands dir");
        fs::write(
            agent_commands_dir.join("stale.md"),
            "---\nautomatic-managed: true\n---\nStale command.\n",
        )
        .expect("write stale command");
        fs::write(
            agent_commands_dir.join("keep.md"),
            "---\ndescription: user command\n---\nDo not remove.\n",
        )
        .expect("write unmanaged command");

        let written = symlink_commands_from_project(
            &agent_commands_dir,
            &project_commands_dir,
            &[("workspace-cmd".to_string(), String::new())],
            &[CustomCommand {
                name: "custom-cmd".to_string(),
                content: String::new(),
            }],
            &ClaudeCode,
        )
        .expect("symlink commands");

        let workspace_path = agent_commands_dir.join("workspace-cmd.md");
        let custom_path = agent_commands_dir.join("custom-cmd.md");
        assert!(written.iter().any(|p| p.ends_with("workspace-cmd.md")));
        assert!(written.iter().any(|p| p.ends_with("custom-cmd.md")));
        assert!(written.iter().any(|p| p.ends_with("stale.md")));
        assert_eq!(
            fs::read_to_string(&workspace_path).expect("read workspace command"),
            fs::read_to_string(project_commands_dir.join("workspace-cmd.md"))
                .expect("read canonical workspace command")
        );
        assert_eq!(
            fs::read_to_string(&custom_path).expect("read custom command"),
            fs::read_to_string(project_commands_dir.join("custom-cmd.md"))
                .expect("read canonical custom command")
        );
        assert!(!agent_commands_dir.join("stale.md").exists());
        assert!(agent_commands_dir.join("keep.md").exists());

        if crate::core::read_settings()
            .map(|settings| settings.sync_mode == "symlink")
            .unwrap_or(true)
        {
            let workspace_meta = workspace_path
                .symlink_metadata()
                .expect("workspace command metadata");
            assert!(
                workspace_meta.file_type().is_symlink(),
                "expected workspace command to be symlinked in symlink mode"
            );
        }
    }

    #[test]
    fn copy_skills_to_project_writes_selected_and_removes_stale_entries() {
        let dir = tempdir().expect("tempdir");
        let skills_dir = dir.path().join("skills");
        fs::create_dir_all(&skills_dir).expect("create skills dir");
        fs::create_dir_all(skills_dir.join("stale-skill")).expect("create stale skill");
        fs::create_dir_all(skills_dir.join("local-skill")).expect("create local skill");

        let mut written = Vec::new();
        copy_skills_to_project(
            &skills_dir,
            &[("fresh-skill".to_string(), "# Fresh skill\n".to_string())],
            &["fresh-skill".to_string()],
            &["local-skill".to_string()],
            &mut written,
        )
        .expect("copy skills");

        assert!(written.iter().any(|p| p.ends_with("fresh-skill")));
        assert!(skills_dir.join("fresh-skill").join("SKILL.md").exists());
        assert!(!skills_dir.join("stale-skill").exists());
        assert!(skills_dir.join("local-skill").exists());
    }
}
