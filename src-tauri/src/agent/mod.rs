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
#[cfg(test)]
mod contract_tests;
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
mod zcode;
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
pub use zcode::ZCode;
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

    /// Files under `dir` that [`write_mcp_config`] reads before it writes, i.e.
    /// the config files this agent merges into rather than owns outright.
    ///
    /// Drift detection asks each agent to write its expected config into an
    /// empty tempdir and then compares the result against the project.  A merge
    /// writer given an empty tempdir produces output missing every user key the
    /// real file carries, which reads as permanent, unresolvable drift.  Listing
    /// the inputs here lets `collect_mcp_drift` seed the tempdir first, so the
    /// comparison is against what a real sync would produce.
    ///
    /// Default: empty vec — the agent owns its config file outright, or writes
    /// none at all.
    fn mcp_merge_inputs(&self, _dir: &Path) -> Vec<PathBuf> {
        vec![]
    }

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

    /// Return the filename to use for a specific sub-agent. Mirrors
    /// [`command_file_name`](Agent::command_file_name).
    ///
    /// The default `{machine_name}.{agents_file_ext()}` is wrong for a
    /// vendor whose convention is a compound extension (GitHub Copilot's
    /// `{name}.agent.md`): [`Path::extension`] only ever returns the last
    /// dot-segment, so building `{name}.agent.md` by hand and later
    /// recovering `name` via `file_stem()` yields `{name}.agent`, not
    /// `{name}` — the bug that left Copilot's stale sub-agent files
    /// unrecoverable. Override this rather than trying to express a
    /// compound extension through `agents_file_ext()` alone.
    fn agent_file_name(&self, machine_name: &str) -> String {
        format!("{machine_name}.{}", self.agents_file_ext())
    }

    /// Convert agent content from the canonical format (Markdown + YAML frontmatter)
    /// to this agent's native format.
    ///
    /// The default injects the same `automatic-managed: true` frontmatter
    /// marker [`convert_command_content`](Agent::convert_command_content)'s
    /// default does, via the same [`render_markdown_command`] — the name is
    /// a holdover from commands being marked first, but the transformation
    /// (ensure managed Markdown frontmatter) is identical and agent sync
    /// needs it too, so cleanup can tell an Automatic-written sub-agent file
    /// apart from one the user authored by hand. Codex overrides this to
    /// convert to TOML format (and injects its own `automatic_managed =
    /// true` marker for the same reason); Kiro overrides it to convert to
    /// JSON.
    fn convert_agent_content(&self, content: &str, _name: &str) -> String {
        render_markdown_command(content)
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

    /// Lifecycle event names this agent's hook system accepts (e.g.
    /// `"SessionStart"`, `"PreToolUse"`), in the vendor's own casing.
    ///
    /// This is the single source of truth for the event picker in the Hooks
    /// UI. Agents whose [`sync_hooks`] filters by event (Codex CLI, Cursor)
    /// must build that filter from this same list rather than a second
    /// hand-written array, so the two can never drift apart the way
    /// `CODEX_CLI_EVENTS` in the frontend once did from
    /// `CODEX_SUPPORTED_EVENTS` in Rust.
    ///
    /// Not every agent that filters is required to: Claude Code intentionally
    /// leaves `sync_hooks` unfiltered even though it declares a full event
    /// list here, so that a user who learns of a new Claude event before this
    /// list is updated does not silently lose their hook. `hook_events()` is
    /// advisory for the UI in that case, not an enforcement mechanism.
    ///
    /// Default: empty — agents that don't support hooks don't have to
    /// override it. Agents that do must also set [`AgentCapabilities::hooks`]
    /// to `true`; a contract test enforces the two stay in sync.
    fn hook_events(&self) -> &'static [&'static str] {
        &[]
    }

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

    /// Where this agent's hook configuration lives on disk, for drift
    /// detection. `None` for agents without the `hooks` capability, and for
    /// Cursor, which uses its own sidecar-manifest mechanism rather than
    /// either [`HookConfigTarget`] flavour.
    ///
    /// `collect_mcp_drift`'s trick — write into a tempdir, then scan its
    /// top-level entries — doesn't reach here: every hook config file lives
    /// nested under a per-agent directory, never at the project root, so a
    /// top-level scan would silently check nothing. This method names the
    /// exact path (and, for merge writers, the key) instead of requiring
    /// drift detection to rediscover it.
    fn hook_config_target(&self, _dir: &Path) -> Option<HookConfigTarget> {
        None
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
    /// Lifecycle event names this agent's hook system accepts. Empty when
    /// `capabilities.hooks` is `false`. Drives the event picker on the Hooks
    /// page instead of a hand-maintained frontend list per agent.
    pub hook_events: Vec<String>,
}

impl AgentInfo {
    pub fn from_agent(agent: &dyn Agent) -> Self {
        Self {
            id: agent.id().to_string(),
            label: agent.label().to_string(),
            description: agent.config_description().to_string(),
            capabilities: agent.capabilities(),
            mcp_note: agent.mcp_note().map(|s| s.to_string()),
            hook_events: agent.hook_events().iter().map(|s| s.to_string()).collect(),
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
        &ZCode,
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

/// Ensure `content`'s YAML frontmatter carries `automatic-managed: true`,
/// adding a frontmatter block if there was none. Despite the name, this is
/// generic Markdown-with-frontmatter tagging with no command-specific
/// behaviour — [`Agent::convert_agent_content`]'s default reuses it verbatim
/// for the same reason [`Agent::convert_command_content`]'s default does:
/// cleanup needs a content-level way to tell an Automatic-written file apart
/// from one the user authored by hand, and a filename convention alone
/// (especially a compound one like `{name}.agent.md`) isn't reliable for
/// that — see [`is_managed_command_file`] and [`is_managed_agent_file`].
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

/// Sub-agent counterpart to [`is_managed_command_file`], with one extra
/// branch: Kiro's format is JSON, which no command vendor uses. Each
/// branch's marker matches the casing convention its own converter already
/// uses for every other key in that format — kebab-case YAML frontmatter,
/// snake_case TOML, camelCase JSON — rather than a single spelling forced
/// across three different serialisations.
pub(crate) fn is_managed_agent_file(path: &Path) -> bool {
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
        "json" => serde_json::from_str::<Value>(&content)
            .ok()
            .and_then(|v| v.get("automaticManaged").and_then(|m| m.as_bool()))
            .unwrap_or(false),
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
/// placeholder syntax, substitute VS Code launch variables against the project
/// on disk, and drop Automatic-internal metadata.
///
/// `workspace_folder` is the project directory whose config is being written.
/// It is the value substituted for `${workspaceFolder}`, matching VS Code's
/// definition. Cursor (and other VS Code-derived clients) expand this variable
/// natively; Claude Code and most others do not, so an unexpanded literal in
/// `.mcp.json` becomes a launch path that never resolves.
///
/// Both the sync engine and drift detection must call this before
/// [`Agent::write_mcp_config`].  If only one of them does, the expected config
/// and the config on disk disagree and every project reports permanent drift.
pub(crate) fn prepare_mcp_servers(
    agent: &dyn Agent,
    servers: &Map<String, Value>,
    workspace_folder: &Path,
) -> Map<String, Value> {
    let mut prepared = Map::new();
    let home = home_dir();

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
        expand_workspace_variables(&mut server, workspace_folder, home.as_deref());
        prepared.insert(name.clone(), server);
    }

    prepared
}

/// Recursively substitute `${workspaceFolder}` and `${userHome}` inside every
/// string in `value`. Runs against a whole server map entry so that
/// dialect-specific fields (`command`, `args`, `cwd`, `env`, `headers`, and
/// anything nested) are all covered without enumerating them here.
///
/// `${env:FOO}` is intentionally left alone: Automatic's inherited-env system
/// (see [`substitute_inherited_env`]) already owns that placeholder and each
/// agent rewrites it into its own dialect.
fn expand_workspace_variables(value: &mut Value, workspace_folder: &Path, home: Option<&Path>) {
    match value {
        Value::String(s) => {
            if !s.contains("${") {
                return;
            }
            let mut out = s.replace("${workspaceFolder}", &workspace_folder.to_string_lossy());
            if let Some(home) = home {
                out = out.replace("${userHome}", &home.to_string_lossy());
            }
            *s = out;
        }
        Value::Array(items) => {
            for item in items {
                expand_workspace_variables(item, workspace_folder, home);
            }
        }
        Value::Object(map) => {
            for (_, v) in map.iter_mut() {
                expand_workspace_variables(v, workspace_folder, home);
            }
        }
        _ => {}
    }
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
///
/// `schema_url` is `None` for dialects with no published schema — writing a
/// `$schema` key that points at the wrong vendor's schema would be worse than
/// omitting it.
pub(crate) fn write_opencode_dialect_mcp_config(
    path: &Path,
    schema_url: Option<&str>,
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

    if let Some(schema_url) = schema_url {
        root.insert("$schema".to_string(), json!(schema_url));
    }
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

// ── Shared hooks writers ────────────────────────────────────────────────────
//
// Claude Code, Codex CLI and (from Phase 5) Gemini CLI, GitHub Copilot and
// Droid all group hooks by event then matcher into a stable order, append
// into an existing matcher group rather than duplicating it, write script
// bodies before the config that references them, prune empty groups and
// events, and clean up managed scripts against a keep-list. Only two things
// actually differ between vendors: whether the config file is merged into
// (shared with user settings) or owned outright, and the shape of one
// handler object. `HookWriteSpec` captures the second; the two entry points
// below capture the first.
//
// Cursor stays out of this — camelCase events and a sidecar
// deep-equal manifest at `.automatic/state/cursor-hooks.json` (see
// `cursor.rs`) don't fit either shape.

/// Key tagging a handler object as Automatic-managed, and the sibling key
/// carrying its stable identity. Only meaningful for the merge flavour
/// ([`merge_hooks_into_json_settings`]) — a shared file needs to tell managed
/// handlers apart from user-authored ones on every re-sync; an owned file
/// (`write_owned_hooks_file`) is fully regenerated each time, so nothing
/// needs tagging.
const HOOK_MANAGED_KEY: &str = "_managedBy";
const HOOK_MANAGED_VALUE: &str = "automatic";
const HOOK_ID_KEY: &str = "_hookId";

/// Describes where and how one agent's hook configuration is written.
/// Returned by [`Agent::hook_config_target`] for drift detection — the two
/// variants mirror the two writer entry points below.
pub enum HookConfigTarget {
    /// This agent owns `path` outright ([`write_owned_hooks_file`]). The
    /// whole file is regenerated every sync, so a whole-file byte compare is
    /// sufficient — and the file is expected to be *absent* entirely when
    /// there are no hooks for this agent, not merely empty.
    Owned { path: PathBuf },
    /// This agent merges hooks into `path` under the top-level `key`
    /// ([`merge_hooks_into_json_settings`]). The file may carry unrelated
    /// settings this agent doesn't own, so a whole-file compare would report
    /// drift for e.g. a model or permissions edit that has nothing to do
    /// with hooks. Only the tagged-managed subset under `key` is compared —
    /// see [`extract_managed_hook_handlers`].
    Merged { path: PathBuf, key: &'static str },
}

/// Per-vendor configuration for the two hooks-writer entry points below.
pub(crate) struct HookWriteSpec {
    /// Events this agent's hook system accepts. Consulted only by
    /// [`write_owned_hooks_file`], which filters and warns on skip.
    /// [`merge_hooks_into_json_settings`] ignores it — Claude Code, the only
    /// merge-flavour writer today, is deliberately left unfiltered so a hook
    /// using an event not yet catalogued in [`Agent::hook_events`] still
    /// syncs. Vendors normally pass their own `hook_events()` here regardless
    /// of which entry point they call, since it is true information about
    /// the vendor even when this particular writer doesn't act on it.
    pub supported_events: &'static [&'static str],
    /// Directory script-handler bodies are written into (e.g.
    /// `<project>/.codex/hooks`).
    pub scripts_dir: PathBuf,
    /// Render the command string a Script handler resolves to, given the
    /// filename (not full path) [`write_managed_hook_script`] wrote it to.
    /// Each vendor keeps its own portable prefix, e.g.
    /// `${CLAUDE_PROJECT_DIR}/.claude/hooks/<file>` vs `./.codex/hooks/<file>`.
    pub script_command: fn(&str) -> String,
    /// Build one handler object for `hook`, given its already-resolved
    /// command string. Most vendors emit `{type: "command", command,
    /// timeout?}`; see [`standard_command_handler`].
    pub handler: fn(&crate::core::Hook, &str) -> Value,
    /// Extra keys merged onto every matcher-group object, e.g. Gemini's
    /// optional `sequential` flag. Empty for vendors with no group-level
    /// config.
    pub group_extras: fn() -> Map<String, Value>,
}

/// The handler shape shared by every vendor implemented so far:
/// `{"type": "command", "command": ..., "timeout"?: n}`.
pub(crate) fn standard_command_handler(hook: &crate::core::Hook, command: &str) -> Value {
    let mut handler = Map::new();
    handler.insert("type".to_string(), Value::String("command".to_string()));
    handler.insert("command".to_string(), Value::String(command.to_string()));
    if let Some(timeout) = hook.timeout_sec {
        handler.insert(
            "timeout".to_string(),
            Value::Number(serde_json::Number::from(timeout)),
        );
    }
    Value::Object(handler)
}

/// No group-level extras — the default for vendors without one.
pub(crate) fn no_group_extras() -> Map<String, Value> {
    Map::new()
}

/// Resolve the command string for `hook`'s handler: verbatim for
/// `Command`/`Path`, or vendor-rendered via [`HookWriteSpec::script_command`]
/// for `Script` (the file itself is written separately by
/// [`write_script_handler_bodies`]).
fn resolve_hook_command_string(hook: &crate::core::Hook, spec: &HookWriteSpec) -> String {
    match &hook.handler {
        crate::core::HookHandler::Command { command } => command.clone(),
        crate::core::HookHandler::Script { interpreter, .. } => {
            let ext = hook_script_extension(interpreter);
            let file_name = format!("{}.{}", hook_slug(hook), ext);
            (spec.script_command)(&file_name)
        }
        crate::core::HookHandler::Path { path, .. } => path.clone(),
    }
}

fn build_plain_handler(hook: &crate::core::Hook, spec: &HookWriteSpec) -> Value {
    let command = resolve_hook_command_string(hook, spec);
    (spec.handler)(hook, &command)
}

/// [`build_plain_handler`] plus the managed-by-automatic tags, for the merge
/// flavour's idempotent strip-then-merge cycle.
fn build_tagged_handler(hook: &crate::core::Hook, spec: &HookWriteSpec) -> Value {
    let mut handler = build_plain_handler(hook, spec);
    if let Some(obj) = handler.as_object_mut() {
        obj.insert(
            HOOK_MANAGED_KEY.to_string(),
            Value::String(HOOK_MANAGED_VALUE.to_string()),
        );
        obj.insert(HOOK_ID_KEY.to_string(), Value::String(hook_slug(hook)));
    }
    handler
}

/// Write every `Script`-handler body in `hooks` to `scripts_dir`, creating
/// the directory only if at least one is present. Returns the written paths
/// (also appended to `written`) so the caller can pass them as the keep-list
/// to a later [`cleanup_managed_hook_scripts`] call.
fn write_script_handler_bodies(
    scripts_dir: &Path,
    hooks: &[&crate::core::Hook],
    written: &mut Vec<String>,
) -> Result<Vec<PathBuf>, String> {
    let mut managed_script_paths = Vec::new();
    let needs_scripts_dir = hooks
        .iter()
        .any(|h| matches!(h.handler, crate::core::HookHandler::Script { .. }));
    if needs_scripts_dir {
        fs::create_dir_all(scripts_dir)
            .map_err(|e| format!("Failed to create {}: {}", scripts_dir.display(), e))?;
    }
    for hook in hooks {
        if let crate::core::HookHandler::Script {
            interpreter,
            script,
        } = &hook.handler
        {
            let path = write_managed_hook_script(scripts_dir, hook, interpreter, script)?;
            managed_script_paths.push(path.clone());
            written.push(path.display().to_string());
        }
    }
    Ok(managed_script_paths)
}

/// Handler values grouped by event, then by matcher within that event.
/// `BTreeMap` at both levels keeps output order stable so repeated syncs
/// produce byte-identical documents.
type HandlersByEventAndMatcher =
    std::collections::BTreeMap<String, std::collections::BTreeMap<Option<String>, Vec<Value>>>;

/// Group `hooks` by event then matcher, building each handler value with
/// `build_handler`.
fn group_hooks_by_event_and_matcher<'a>(
    hooks: impl Iterator<Item = &'a crate::core::Hook>,
    build_handler: impl Fn(&crate::core::Hook) -> Value,
) -> HandlersByEventAndMatcher {
    let mut grouped = HandlersByEventAndMatcher::new();
    for hook in hooks {
        grouped
            .entry(hook.event.clone())
            .or_default()
            .entry(hook.matcher.clone())
            .or_default()
            .push(build_handler(hook));
    }
    grouped
}

/// Insert grouped handlers into `hooks_obj`. For each `(event, matcher)`,
/// append into an existing matcher group with the same matcher value (so
/// user-authored and managed handlers coexist in one group) rather than
/// duplicating it; otherwise append a new group seeded with `group_extras`.
///
/// Starting from an empty `hooks_obj` (the owned-file flavour, which
/// regenerates its document from scratch every sync) degenerates to "append
/// a fresh group for every matcher" — the same shape Codex CLI's hand-rolled
/// writer used to build directly.
fn insert_grouped_handlers(
    hooks_obj: &mut Map<String, Value>,
    grouped: HandlersByEventAndMatcher,
    group_extras: fn() -> Map<String, Value>,
) {
    for (event, matchers) in grouped {
        let event_entry = hooks_obj
            .entry(event.clone())
            .or_insert_with(|| Value::Array(Vec::new()));
        let Some(event_arr) = event_entry.as_array_mut() else {
            continue;
        };

        for (matcher, handlers) in matchers {
            let existing_idx = event_arr.iter().position(|group| {
                let group_matcher = group.get("matcher").and_then(|v| v.as_str());
                match (&matcher, group_matcher) {
                    (Some(m), Some(g)) => m == g,
                    (None, None) => true,
                    _ => false,
                }
            });

            if let Some(idx) = existing_idx {
                if let Some(group_obj) = event_arr[idx].as_object_mut() {
                    let group_hooks = group_obj
                        .entry("hooks".to_string())
                        .or_insert_with(|| Value::Array(Vec::new()));
                    if let Some(arr) = group_hooks.as_array_mut() {
                        arr.extend(handlers);
                    }
                }
            } else {
                let mut group = group_extras();
                if let Some(m) = matcher {
                    group.insert("matcher".to_string(), Value::String(m));
                }
                group.insert("hooks".to_string(), Value::Array(handlers));
                event_arr.push(Value::Object(group));
            }
        }
    }
}

/// Remove every handler in `hooks_obj` carrying the managed-by-automatic tag,
/// leaving user-authored handlers untouched. Merge-flavour only: an owned
/// file has nothing to preserve across syncs.
fn drop_managed_hook_handlers(hooks_obj: &mut Map<String, Value>) {
    for event_value in hooks_obj.values_mut() {
        let Some(groups) = event_value.as_array_mut() else {
            continue;
        };
        for group in groups.iter_mut() {
            let Some(group_obj) = group.as_object_mut() else {
                continue;
            };
            let Some(handlers) = group_obj.get_mut("hooks").and_then(|h| h.as_array_mut()) else {
                continue;
            };
            handlers.retain(|handler| {
                !handler
                    .get(HOOK_MANAGED_KEY)
                    .and_then(|v| v.as_str())
                    .map(|s| s == HOOK_MANAGED_VALUE)
                    .unwrap_or(false)
            });
        }
    }
}

/// Remove matcher groups left empty by [`drop_managed_hook_handlers`], and
/// events left with no groups at all.
fn prune_empty_hook_entries(hooks_obj: &mut Map<String, Value>) {
    let mut empty_events = Vec::new();
    for (event, value) in hooks_obj.iter_mut() {
        let Some(groups) = value.as_array_mut() else {
            continue;
        };
        groups.retain(|group| {
            group
                .get("hooks")
                .and_then(|h| h.as_array())
                .map(|arr| !arr.is_empty())
                .unwrap_or(false)
        });
        if groups.is_empty() {
            empty_events.push(event.clone());
        }
    }
    for event in empty_events {
        hooks_obj.remove(&event);
    }
}

/// Extract only the tagged-managed handlers from a merge-flavour hooks
/// object, in the same `{event: [{matcher?, hooks: [...]}]}` shape, pruned
/// of any group or event left with nothing in it. The inverse of
/// [`drop_managed_hook_handlers`] — used by drift detection
/// (`sync::drift::collect_hooks_drift`) to compare only the subset of a
/// shared settings file Automatic actually owns, ignoring both
/// user-authored hooks and any unrelated settings in the same file.
pub(crate) fn extract_managed_hook_handlers(hooks_obj: &Map<String, Value>) -> Map<String, Value> {
    let mut extracted = hooks_obj.clone();
    for event_value in extracted.values_mut() {
        let Some(groups) = event_value.as_array_mut() else {
            continue;
        };
        for group in groups.iter_mut() {
            let Some(group_obj) = group.as_object_mut() else {
                continue;
            };
            let Some(handlers) = group_obj.get_mut("hooks").and_then(|h| h.as_array_mut()) else {
                continue;
            };
            handlers.retain(|handler| {
                handler
                    .get(HOOK_MANAGED_KEY)
                    .and_then(|v| v.as_str())
                    .map(|s| s == HOOK_MANAGED_VALUE)
                    .unwrap_or(false)
            });
        }
    }
    prune_empty_hook_entries(&mut extracted);
    extracted
}

/// Write hooks into a config file this agent owns outright (Codex CLI today;
/// Copilot and Droid from Phase 5). The whole document is regenerated from
/// `hooks` on every call — nothing is merged in from what's already there —
/// so when every hook is filtered out or the input is empty, the file is
/// deleted rather than left behind as a stale empty shell.
pub(crate) fn write_owned_hooks_file(
    hooks_file: &Path,
    hooks: &[crate::core::Hook],
    spec: &HookWriteSpec,
) -> Result<Vec<String>, String> {
    let mut written = Vec::new();

    let usable_hooks: Vec<&crate::core::Hook> = hooks
        .iter()
        .filter(|h| {
            if spec.supported_events.contains(&h.event.as_str()) {
                true
            } else {
                eprintln!(
                    "[automatic] hook event '{}' is not supported by this agent — skipping hook '{}'",
                    h.event, h.name
                );
                false
            }
        })
        .collect();

    if usable_hooks.is_empty() {
        if hooks_file.exists() {
            let _ = fs::remove_file(hooks_file);
        }
        cleanup_managed_hook_scripts(&spec.scripts_dir, &[])?;
        return Ok(written);
    }

    if let Some(parent) = hooks_file.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
    }

    let managed_script_paths =
        write_script_handler_bodies(&spec.scripts_dir, &usable_hooks, &mut written)?;

    let grouped = group_hooks_by_event_and_matcher(usable_hooks.into_iter(), |h| {
        build_plain_handler(h, spec)
    });
    let mut hooks_root = Map::new();
    insert_grouped_handlers(&mut hooks_root, grouped, spec.group_extras);

    let document = json!({ "hooks": hooks_root });
    let pretty =
        serde_json::to_string_pretty(&document).map_err(|e| format!("JSON error: {}", e))?;
    fs::write(hooks_file, format!("{}\n", pretty))
        .map_err(|e| format!("Failed to write {}: {}", hooks_file.display(), e))?;
    written.push(hooks_file.display().to_string());

    cleanup_managed_hook_scripts(&spec.scripts_dir, &managed_script_paths)?;

    Ok(written)
}

/// Merge hooks into a JSON settings file this agent shares with other
/// settings (Claude Code today; Gemini CLI from Phase 5), under
/// `settings_key` (e.g. `"hooks"`). Every previously-managed handler is
/// stripped and the current hook set re-merged on top, so the on-disk state
/// depends only on the current hook set, not on prior sync history — but
/// user-authored handlers, and every other top-level key, survive untouched.
///
/// Unlike [`write_owned_hooks_file`], this always writes: the file may carry
/// settings this agent doesn't own, so an empty hook set still needs the
/// managed section cleared out, not the file left alone or deleted.
pub(crate) fn merge_hooks_into_json_settings(
    settings_path: &Path,
    settings_key: &str,
    hooks: &[crate::core::Hook],
    spec: &HookWriteSpec,
) -> Result<Vec<String>, String> {
    let mut written = Vec::new();

    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
    }

    let hook_refs: Vec<&crate::core::Hook> = hooks.iter().collect();
    let managed_script_paths =
        write_script_handler_bodies(&spec.scripts_dir, &hook_refs, &mut written)?;

    let mut root = read_mergeable_json_object(settings_path)?;

    let key_value = root
        .entry(settings_key.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let key_obj = key_value.as_object_mut().ok_or_else(|| {
        format!(
            "`{}` in {} must be an object",
            settings_key,
            settings_path.display()
        )
    })?;

    drop_managed_hook_handlers(key_obj);
    let grouped = group_hooks_by_event_and_matcher(hooks.iter(), |h| build_tagged_handler(h, spec));
    insert_grouped_handlers(key_obj, grouped, spec.group_extras);

    prune_empty_hook_entries(key_obj);
    if key_obj.is_empty() {
        root.remove(settings_key);
    }

    let pretty = serde_json::to_string_pretty(&Value::Object(root))
        .map_err(|e| format!("JSON error: {}", e))?;
    fs::write(settings_path, format!("{}\n", pretty))
        .map_err(|e| format!("Failed to write {}: {}", settings_path.display(), e))?;
    written.push(settings_path.display().to_string());

    cleanup_managed_hook_scripts(&spec.scripts_dir, &managed_script_paths)?;

    Ok(written)
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

    /// `list_agents` (the Tauri command Hooks.tsx calls) serialises
    /// `AgentInfo` straight to JSON. Pin the exact shape the frontend reads
    /// `hook_events` from, so a rename here shows up as a Rust test failure
    /// instead of a silently empty event picker in the UI.
    #[test]
    fn agent_info_serialises_hook_events_as_a_plain_string_array() {
        let codex = AgentInfo::from_agent(from_id("codex").unwrap());
        let value = serde_json::to_value(&codex).unwrap();
        let events = value["hook_events"]
            .as_array()
            .expect("hook_events must serialise as a JSON array");
        assert_eq!(events.len(), 11);
        assert!(events.iter().any(|v| v == "SubagentStop"));

        let claude = AgentInfo::from_agent(from_id("claude").unwrap());
        let value = serde_json::to_value(&claude).unwrap();
        let events = value["hook_events"].as_array().unwrap();
        assert!(events.iter().any(|v| v == "MessageDisplay"));

        // An agent with no hooks capability reports an empty list, not a
        // missing key — the frontend indexes it unconditionally.
        let goose = AgentInfo::from_agent(from_id("goose").unwrap());
        let value = serde_json::to_value(&goose).unwrap();
        assert_eq!(value["hook_events"].as_array().unwrap().len(), 0);
    }

    /// One case per format `convert_agent_content` can produce, matching
    /// each vendor's own marker convention: kebab-case YAML frontmatter for
    /// the default Markdown path, snake_case for Codex's TOML, camelCase for
    /// Kiro's JSON. A file lacking the marker — hand-authored, or simply the
    /// wrong shape — must never read as managed regardless of extension.
    #[test]
    fn is_managed_agent_file_recognises_every_format() {
        let dir = tempdir().unwrap();

        let managed_md = dir.path().join("managed.md");
        fs::write(&managed_md, "---\nautomatic-managed: true\n---\nBody.\n").unwrap();
        assert!(is_managed_agent_file(&managed_md));

        let hand_written_md = dir.path().join("hand-written.md");
        fs::write(&hand_written_md, "---\nname: Mine\n---\nBody.\n").unwrap();
        assert!(!is_managed_agent_file(&hand_written_md));

        let managed_toml = dir.path().join("managed.toml");
        fs::write(&managed_toml, "automatic_managed = true\nname = \"x\"\n").unwrap();
        assert!(is_managed_agent_file(&managed_toml));

        let hand_written_toml = dir.path().join("hand-written.toml");
        fs::write(&hand_written_toml, "name = \"x\"\n").unwrap();
        assert!(!is_managed_agent_file(&hand_written_toml));

        let managed_json = dir.path().join("managed.json");
        fs::write(&managed_json, r#"{"automaticManaged": true, "name": "x"}"#).unwrap();
        assert!(is_managed_agent_file(&managed_json));

        let hand_written_json = dir.path().join("hand-written.json");
        fs::write(&hand_written_json, r#"{"name": "x"}"#).unwrap();
        assert!(!is_managed_agent_file(&hand_written_json));
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
