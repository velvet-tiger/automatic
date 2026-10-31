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
mod kimi_code;
mod kiro;
#[cfg(test)]
mod mcp_format_tests;

#[cfg(test)]
mod global_mcp_tests;
mod opencode;
mod pi;
mod removal;
mod warp;
mod zcode;
mod zed;

use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
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
pub use kimi_code::KimiCode;
pub use kiro::Kiro;
pub use opencode::{
    clean_opencode_snapshots, clear_opencode_cache, CleanSnapshotsResult, ClearCacheResult,
    OpenCode,
};
pub use pi::Pi;
pub use removal::{
    apply_removal_plan, plan_agent_removal, RemovalAction, RemovalEntry, RemovalMode, RemovalPlan,
};
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
    /// Automatic can write user-level (home-directory) MCP server config for
    /// this agent, so servers are available in every session/chat regardless of
    /// project.  Deliberately decoupled from [`mcp_servers`] — Cline, Warp and
    /// Antigravity all have global-yes / project-no, and Pi has the opposite.
    ///
    /// Defaults to `false`; each agent opts in with an actual writer.
    pub global_mcp_servers: bool,
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
    /// All capabilities enabled by default, except the opt-in ones
    /// (`commands`, `hooks`, `global_mcp_servers`).
    fn default() -> Self {
        Self {
            skills: true,
            instructions: true,
            mcp_servers: true,
            global_mcp_servers: false,
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

// ── Global MCP write target ────────────────────────────────────────────────────

/// The user-level (home-directory) MCP config file Automatic can write for an
/// agent.  Present iff `AgentCapabilities.global_mcp_servers` is `true`.
#[derive(Debug, Clone, Serialize)]
pub struct GlobalMcpTarget {
    /// Absolute path Automatic merges into.
    pub path: PathBuf,
    /// How the agent picks up changes to this file, e.g. "reconnects on save"
    /// or "requires a one-time approval in Warp > Settings > MCP servers".
    /// Surfaced verbatim in the UI so the user knows whether a restart is
    /// needed.
    pub reload_note: Option<&'static str>,
}

/// Outcome of one entry-level merge into a global MCP config file.
///
/// Names in `written` and `removed` join the ownership set for the next call;
/// `skipped` names surface in the UI so the user can rename the colliding
/// entries themselves.  `unchanged` means the file bytes were not rewritten:
/// used to skip watcher churn in auto-reloading agents like Droid/Kiro and to
/// shrink the race window against a running Claude Code process.
#[derive(Debug, Default, Serialize)]
pub struct GlobalMcpWriteReport {
    pub path: String,
    pub written: Vec<String>,
    pub removed: Vec<String>,
    pub skipped: Vec<String>,
    pub unchanged: bool,
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

    /// The user-level MCP config file Automatic can write for this agent, or
    /// `None` if the agent has no writable global config.  Must be `Some` iff
    /// [`AgentCapabilities::global_mcp_servers`] is `true`.
    ///
    /// The path is resolved at call time (some depend on `$CLINE_DIR` or
    /// similar) and may fall back to `None` when the home directory cannot be
    /// determined.
    fn global_mcp_target(&self) -> Option<GlobalMcpTarget> {
        None
    }

    /// Write `desired` into this agent's user-level MCP config file using
    /// entry-level merge semantics.
    ///
    /// - `desired`: already prepared (see [`prepare_global_mcp_servers`]) and
    ///   dialect-rendered.  Keys are server names.
    /// - `previously_managed`: the set of entry names Automatic wrote on the
    ///   last apply.  Deletion of an entry only happens for names in this set,
    ///   and a desired name that collides with an entry *not* in this set is
    ///   left untouched and returned in [`GlobalMcpWriteReport::skipped`] —
    ///   user-authored entries are never overwritten.
    ///
    /// Default: returns `Err` — override on any agent whose
    /// [`AgentCapabilities::global_mcp_servers`] is `true`.
    fn write_global_mcp_config(
        &self,
        _desired: &Map<String, Value>,
        _previously_managed: &[String],
    ) -> Result<GlobalMcpWriteReport, String> {
        Err(format!(
            "{} does not support global MCP server writes",
            self.label()
        ))
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

    /// Rewrite `${NAME}` environment references inside one server entry's
    /// `headers` into this agent's own spelling.
    ///
    /// Automatic stores a header such as `Authorization: Bearer ${TOKEN}` so
    /// the secret stays in the user's environment and never lands in a project
    /// file.  `${NAME}` is Claude Code's syntax, so the default leaves the
    /// headers untouched.  Agents that spell the reference differently
    /// override this; an agent that copied the literal through would send the
    /// placeholder text to the server as the credential.
    fn rewrite_header_env_references(&self, _server: &mut Map<String, Value>) {}

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

    /// Return the raw set of entry *names* present in this agent's global
    /// MCP config file.
    ///
    /// This differs from [`discover_global_mcp_servers`] in one specific way:
    /// it does not filter Automatic-generated `mcp-proxy` stubs.  Global-scope
    /// drift/status logic ([`crate::sync::global_mcp::preview_global_mcp`])
    /// needs to know which names are on disk regardless of whether they were
    /// written as a stub for a remote OAuth server — otherwise every managed
    /// remote server reports as permanently missing.
    ///
    /// The default implementation derives from `discover_global_mcp_servers`,
    /// which is correct for agents whose discovery does not filter stubs
    /// (Codex TOML, Goose YAML, Pi).  JSON-dialect agents should override to
    /// return the raw key set of their on-disk servers object.
    fn discover_global_mcp_entry_names(&self) -> HashSet<String> {
        self.discover_global_mcp_servers()
            .keys()
            .cloned()
            .collect()
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

    /// Return what differs between a sub-agent file on disk and `expected`,
    /// the file [`convert_agent_content`](Agent::convert_agent_content)
    /// renders from the stored copy.  Empty means the file matches.
    ///
    /// The conflict scan and the adopt guard use this rather than byte
    /// equality.  A conflict means sync would lose something the user wrote,
    /// and adopting must be able to end it.  A Markdown file missing only the
    /// `automatic-managed` marker loses nothing, because sync adds the marker.
    /// The byte comparison must stay first: Kiro's JSON is not Markdown, so
    /// rendering it can never equal `expected`.  A byte order mark is no
    /// content either, as some editors add one.  `automatic-managed: false`
    /// is the user's edit, and the render cannot keep it.  Codex overrides
    /// this to compare fields.
    fn agent_file_differences(&self, disk: &str, expected: &str) -> Vec<String> {
        let disk = disk.strip_prefix('\u{feff}').unwrap_or(disk);
        if disk == expected {
            return Vec::new();
        }
        if top_level_frontmatter_value(disk, "automatic-managed")
            .is_some_and(|managed| managed != "true")
        {
            return vec!["automatic-managed".to_string()];
        }
        if render_markdown_command(disk) == expected {
            Vec::new()
        } else {
            vec!["content".to_string()]
        }
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

    /// Strip Automatic's MCP entries from the files this agent merges into
    /// but does not own (its [`mcp_merge_inputs`](Agent::mcp_merge_inputs)
    /// minus [`owned_config_paths`](Agent::owned_config_paths)).  Called when
    /// the agent is removed from a project in Remove mode, for files outside
    /// the agent's [`owned_dirs`](Agent::owned_dirs).  Files the agent owns
    /// are deleted by the removal plan, not here.
    ///
    /// `managed_names` names the server entries Automatic wrote for this
    /// project — user-added servers in the same file must survive the strip.
    /// The same list feeds both the removal preview and its apply, so what
    /// the user sees is what removal does.
    ///
    /// Returns the paths of files modified or deleted.
    ///
    /// Default: nothing to strip.
    fn cleanup_mcp_config(&self, _dir: &Path, _managed_names: &[String]) -> Vec<String> {
        vec![]
    }

    /// Directories under `dir` that belong to this agent alone, such as
    /// `.claude/` or `.cursor/`.  Removing the agent in Remove mode deletes
    /// each one whole, including files the user added there.
    ///
    /// Never list a directory another agent or tool writes into (`.agents/`,
    /// `.github/`, `.vscode/`).  The contract tests reject an owned directory
    /// that overlaps another agent's paths.
    ///
    /// Default: empty vec.
    fn owned_dirs(&self, _dir: &Path) -> Vec<PathBuf> {
        vec![]
    }

    /// Files Automatic writes for this agent that no other trait method
    /// names, such as Cursor's hook manifest under `.automatic/state/`.
    /// Remove mode deletes them.
    ///
    /// Default: empty vec.
    fn owned_extra_files(&self, _dir: &Path) -> Vec<PathBuf> {
        vec![]
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
        &KimiCode,
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

/// `true` when `name` is one plain path component, so joining it to a
/// directory names an entry in that directory.  Names come from the
/// committed project config and from agent frontmatter: `../..` or an
/// absolute path would point a remove or a write outside the project.
pub(crate) fn is_plain_file_name(name: &str) -> bool {
    let mut parts = Path::new(name).components();
    matches!(
        (parts.next(), parts.next()),
        (Some(std::path::Component::Normal(_)), None)
    )
}

/// Escape text for a single-line basic string.  Such a string cannot hold a
/// raw line break either, so those are escaped along with the control
/// characters.  Tab is allowed and stays literal.
pub(crate) fn escape_toml_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' | '\r' => push_toml_unicode_escape(&mut out, c),
            c if is_forbidden_toml_control(c) => push_toml_unicode_escape(&mut out, c),
            c => out.push(c),
        }
    }
    out
}

/// `true` for a control character TOML forbids raw in any basic string.
/// Tab, line feed and carriage return are left to the caller: which of them
/// are allowed depends on the kind of string.
fn is_forbidden_toml_control(c: char) -> bool {
    matches!(
        c,
        '\u{0}'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}' | '\u{7f}'
    )
}

fn push_toml_unicode_escape(out: &mut String, c: char) {
    out.push_str(&format!("\\u{:04X}", c as u32));
}

/// Escape text for a `"""` multi-line basic string.  Unlike
/// [`escape_toml_string`], line breaks stay literal, and a quote is escaped
/// only inside a run of three or more, the only place one can end the
/// string.  So ordinary text renders unchanged.  A carriage return is
/// allowed only as part of a CRLF line break.
pub(crate) fn escape_toml_multiline_string(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => escaped.push_str("\\\\"),
            '\r' if chars.peek() != Some(&'\n') => push_toml_unicode_escape(&mut escaped, c),
            c if is_forbidden_toml_control(c) => push_toml_unicode_escape(&mut escaped, c),
            c => escaped.push(c),
        }
    }

    let mut out = String::with_capacity(escaped.len());
    let mut rest = escaped.as_str();
    while let Some(start) = rest.find('"') {
        let run = rest[start..].bytes().take_while(|b| *b == b'"').count();
        let quotes = &rest[start..start + run];
        out.push_str(&rest[..start]);
        if run >= 3 {
            out.push_str(&quotes.replace('"', "\\\""));
        } else {
            out.push_str(quotes);
        }
        rest = &rest[start + run..];
    }
    out.push_str(rest);
    out
}

/// Write `name` as one key of a TOML table header.  A bare key stays bare,
/// so existing files stay byte-identical.  Anything else is quoted, or a name
/// from project config could close the header and add tables of its own.
pub(crate) fn toml_table_key(name: &str) -> String {
    let bare = !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if bare {
        name.to_string()
    } else {
        format!("\"{}\"", escape_toml_string(name))
    }
}

/// Recover the machine name from a sub-agent file name: the reverse of
/// [`Agent::agent_file_name`].  The suffix that method writes is tried
/// first, then a plain `.{agents_file_ext()}`.  The two differ only for
/// Copilot, which loads both `planner.agent.md` and `planner.md` from
/// `.github/agents/` and names the agent after the file minus either
/// suffix (<https://docs.github.com/en/copilot/reference/custom-agents-configuration>).
/// `Path::file_stem` is not enough: it turns `planner.agent.md` into
/// `planner.agent`.
///
/// Returns `None` when `file_name` is not one of this agent's sub-agent
/// files, or when what is left is not a plain file name (`...agent.md`
/// leaves `..`).
pub(crate) fn agent_machine_name_from_file_name<'a>(
    agent: &dyn Agent,
    file_name: &'a str,
) -> Option<&'a str> {
    let written = agent.agent_file_name("");
    let plain = format!(".{}", agent.agents_file_ext());
    file_name
        .strip_suffix(written.as_str())
        .or_else(|| file_name.strip_suffix(plain.as_str()))
        .filter(|stem| is_plain_file_name(stem))
}

/// Recover the machine name from a command file name: the reverse of
/// [`Agent::command_file_name`].  Only the suffix that method writes is
/// stripped.  Copilot's prompt files are `*.prompt.md`
/// (<https://code.visualstudio.com/docs/copilot/customization/prompt-files>),
/// so unlike [`agent_machine_name_from_file_name`] there is no plain `.md`
/// fallback.  `Path::file_stem` would turn `planner.prompt.md` into
/// `planner.prompt`.
///
/// Returns `None` when `file_name` is not one of this agent's command
/// files, or when what is left is not a plain file name (`...md` leaves
/// `..`).
pub(crate) fn command_machine_name_from_file_name<'a>(
    agent: &dyn Agent,
    file_name: &'a str,
) -> Option<&'a str> {
    let written = agent.command_file_name("");
    file_name
        .strip_suffix(written.as_str())
        .filter(|stem| is_plain_file_name(stem))
}

/// Log and report a name [`is_plain_file_name`] rejects.
fn skip_unsafe_name(kind: &str, name: &str) -> bool {
    if is_plain_file_name(name) {
        return false;
    }
    eprintln!("[automatic] sync: not syncing {kind} '{name}': the name is not a plain file name");
    true
}

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
    ensure_valid_skill_names(skills)?;
    cleanup_skill_dir(base_dir, selected_skill_names, preserve_names)?;

    for (name, content) in skills {
        // Drift writes the expected state into a tempdir through here, and
        // an absolute or `../` name would land outside it.
        if skip_unsafe_name("skill", name) {
            continue;
        }
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
    ensure_valid_skill_names(skills)?;
    cleanup_skill_dir(project_skills_dir, selected_skill_names, preserve_names)?;

    for (name, content) in skills {
        if skip_unsafe_name("skill", name) {
            continue;
        }
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
    ensure_valid_skill_names(skills)?;
    cleanup_skill_dir(agent_skills_dir, selected_skill_names, preserve_names)?;

    let settings = crate::core::read_settings().unwrap_or_default();
    let use_symlink = settings.sync_mode == "symlink";

    for (name, content) in skills {
        if skip_unsafe_name("skill", name) {
            continue;
        }
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

/// Check every skill name before a writer touches the disk. Custom skill
/// names come from a project's committed config, and each name becomes a
/// directory under the skills directory.
fn ensure_valid_skill_names(skills: &[(String, String)]) -> Result<(), String> {
    skills
        .iter()
        .try_for_each(|(name, _)| crate::core::ensure_valid_asset_name("Skill", name))
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

    let Some(span) = frontmatter_span(content) else {
        return (frontmatter, content);
    };

    let after_block = &content[span.body_start..];
    let body = if after_block.starts_with('\n') || after_block.starts_with("\r\n") {
        &after_block[1..]
    } else {
        after_block
    };

    for line in content[span.start..span.end].lines() {
        if let Some((key, value)) = parse_frontmatter_line(line) {
            frontmatter.insert(key.to_string(), value.to_string());
        }
    }

    (frontmatter, body)
}

/// Read one frontmatter line as a key and value: both trimmed, with one layer
/// of matching quotes stripped from the value.  There is no unescaping.
/// Returns `None` for a comment, a blank line, or a line with no colon.
fn parse_frontmatter_line(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (key, value) = line.split_once(':')?;
    let mut value = value.trim();
    if value.len() >= 2 {
        for quote in ['"', '\''] {
            if value.starts_with(quote) && value.ends_with(quote) {
                value = &value[1..value.len() - 1];
                break;
            }
        }
    }
    Some((key.trim(), value))
}

/// Where the frontmatter block sits in a file.
struct FrontmatterSpan {
    /// First byte of the frontmatter lines, after the opening line break.
    start: usize,
    /// End of the frontmatter lines, before the newline of the closing `---`.
    end: usize,
    /// First byte after the closing `---` and any spaces following it.
    body_start: usize,
}

/// Locate the frontmatter block.  The closing `---` must be a whole line, so
/// `-----` or `---x` closes nothing, though YAML allows trailing spaces and
/// tabs.  `None` when there is no complete block.  Every frontmatter reader
/// goes through this, so they cannot disagree on where a block ends.
fn frontmatter_span(content: &str) -> Option<FrontmatterSpan> {
    let start = if content.starts_with("---\r\n") {
        5
    } else if content.starts_with("---\n") {
        4
    } else {
        return None;
    };

    let mut line_start = start;
    loop {
        let rest = &content[line_start..];
        let line_len = rest.find('\n').map_or(rest.len(), |newline| newline + 1);
        let line = rest[..line_len].trim_end_matches('\n');
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.trim_end_matches([' ', '\t']) == "---" {
            return Some(FrontmatterSpan {
                start,
                end: line_start.saturating_sub(1).max(start),
                body_start: line_start + line.len(),
            });
        }
        if line_len == 0 || line_start + line_len >= content.len() {
            return None;
        }
        line_start += line_len;
    }
}

/// The frontmatter lines of `content`, or `None` when it has no block.
pub(crate) fn frontmatter_block(content: &str) -> Option<&str> {
    frontmatter_span(content).map(|span| &content[span.start..span.end])
}

/// `true` for text a frontmatter value can hold on one line.  A control
/// character or Unicode line break could end the line in some reader, which
/// would let a value add keys of its own or close the block.  Tab is the one
/// control character YAML allows inside a line.
fn fits_on_one_frontmatter_line(value: &str) -> bool {
    !value
        .chars()
        .any(|c| (c.is_control() && c != '\t') || matches!(c, '\u{2028}' | '\u{2029}'))
}

/// `true` when YAML would read a plain `value` as something other than that
/// text, so it has to be quoted.
fn frontmatter_value_needs_quotes(value: &str) -> bool {
    const LEADING_INDICATORS: &[char] = &[
        '[', ']', '{', '}', ',', '&', '*', '!', '|', '>', '\'', '"', '%', '@', '`', '#',
    ];
    const OTHER_TYPES: &[&str] = &[
        "", "-", "?", "~", "null", "true", "false", "yes", "no", "on", "off",
    ];
    let lowercase = value.to_ascii_lowercase();
    value != value.trim()
        || value.contains(": ")
        || value.contains(":\t")
        || value.contains(" #")
        || value.contains("\t#")
        || (value.starts_with(['-', '?']) && value[1..].starts_with([' ', '\t']))
        || value.ends_with(':')
        || value.starts_with(LEADING_INDICATORS)
        || OTHER_TYPES.contains(&lowercase.as_str())
        || lowercase.parse::<f64>().is_ok()
        || lowercase.starts_with("0x")
        || lowercase.starts_with("0o")
        || matches!(lowercase.as_str(), ".inf" | "+.inf" | "-.inf" | ".nan")
}

/// Write `value` as one frontmatter value, or return `None` when that cannot
/// be done safely.  This is the single gate for what reaches frontmatter.
///
/// `frontmatter_value_is_one_yaml_string` in the tests checks this against a
/// real YAML 1.2 parser, as well as [`parse_frontmatter`].
///
/// [`parse_frontmatter`] strips one layer of quotes and does not unescape, so
/// a quoted value must not need escaping.  Single quotes hold anything except
/// a single quote.  Double quotes also cannot hold a backslash, which YAML
/// would read as an escape.
pub(crate) fn render_frontmatter_value(value: &str) -> Option<String> {
    if !fits_on_one_frontmatter_line(value) {
        return None;
    }
    if !frontmatter_value_needs_quotes(value) {
        return Some(value.to_string());
    }
    if !value.contains('\'') {
        Some(format!("'{value}'"))
    } else if !value.contains('"') && !value.contains('\\') {
        Some(format!("\"{value}\""))
    } else {
        None
    }
}

/// `true` when `line` sets `key` at the top level of the block.  An indented
/// line belongs to a nested value, even though [`parse_frontmatter`] reads
/// it flat.
fn frontmatter_line_sets(line: &str, key: &str) -> bool {
    !line.starts_with([' ', '\t'])
        && parse_frontmatter_line(line).is_some_and(|(found, _)| found == key)
}

/// The value of a top-level `key`, read as [`parse_frontmatter`] reads it.
pub(crate) fn top_level_frontmatter_value<'a>(content: &'a str, key: &str) -> Option<&'a str> {
    frontmatter_block(content)?
        .split('\n')
        .rfind(|line| frontmatter_line_sets(line, key))
        .and_then(parse_frontmatter_line)
        .map(|(_, value)| value)
}

/// How many top-level lines set `key`.
pub(crate) fn top_level_frontmatter_key_count(content: &str, key: &str) -> usize {
    frontmatter_block(content).map_or(0, |block| {
        block
            .split('\n')
            .filter(|line| frontmatter_line_sets(line, key))
            .count()
    })
}

/// `true` when a flow collection (`[a, b` or `{a: 1`) is not closed on the
/// line that opens it.
fn opens_unclosed_flow_collection(value: &str) -> bool {
    if !value.starts_with(['[', '{']) {
        return false;
    }
    let mut depth = 0;
    let mut quote = None;
    for c in value.chars() {
        match (quote, c) {
            (Some(open), _) if c == open => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '[' | '{') => depth += 1,
            (None, ']' | '}') => depth -= 1,
            _ => {}
        }
    }
    depth > 0 || quote.is_some()
}

/// `true` when the value on `lines[index]` carries on past that line: a block
/// scalar, an unclosed quote or flow collection, or following lines that
/// belong to it.  A comment can sit between a key and its value, so comments
/// are skipped.  Editing only the first line would leave the rest behind.
fn frontmatter_value_continues(lines: &[&str], index: usize) -> bool {
    let value = lines[index].split_once(':').map_or("", |(_, value)| value.trim());
    let unclosed = |quote: char| {
        value.starts_with(quote) && (value.len() < 2 || !value.ends_with(quote))
    };
    if value.starts_with(['|', '>'])
        || unclosed('"')
        || unclosed('\'')
        || opens_unclosed_flow_collection(value)
    {
        return true;
    }
    lines[index + 1..]
        .iter()
        .map(|line| line.trim_end())
        .find(|line| !line.is_empty() && !line.trim_start().starts_with('#'))
        .is_some_and(|next| next.starts_with([' ', '\t', '-']) || !next.contains(':'))
}

/// `true` for a top-level line that sets no key of its own: `_key: x`, an
/// item of the list under the key before it, or the bracket closing a flow
/// collection.
fn continues_a_value(line: &str) -> bool {
    let line = line.trim_end();
    line.starts_with('_') || line == "-" || line.starts_with("- ") || matches!(line, "]" | "}")
}

/// `true` when `block` breaks a line in a way [`parse_frontmatter`] does not
/// see: a lone carriage return, or a line break some YAML readers honour.  A
/// comment or value could then hide a key behind it.
fn holds_a_hidden_line_break(block: &str) -> bool {
    // The block stops before the last line's `\n`, so a CRLF file ends in `\r`.
    block
        .trim_end_matches('\r')
        .replace("\r\n", "\n")
        .contains(['\r', '\u{85}', '\u{2028}', '\u{2029}'])
}

/// `true` when the value on a `key: value` line is set off from the colon.
/// Without that, YAML reads the whole line as one scalar and finds no key.
fn has_space_after_colon(line: &str) -> bool {
    line.split_once(':').is_some_and(|(_, value)| {
        value.is_empty() || value.starts_with([' ', '\t', '\r'])
    })
}

/// `true` when a YAML reader and [`parse_frontmatter`] take the same text
/// from a `name:` line.  The value must be one the gate would write as it
/// stands, or be quoted with nothing inside to unescape.  So an alias, an
/// anchor, an inline comment or a Unicode space at either end is refused.
fn name_line_reads_the_same(line: &str) -> bool {
    if !has_space_after_colon(line) {
        return false;
    }
    let raw = line
        .split_once(':')
        .map_or("", |(_, value)| value.trim_matches([' ', '\t', '\r']));
    if raw.is_empty() {
        return true;
    }
    for quote in ['"', '\''] {
        if raw.len() >= 2 && raw.starts_with(quote) && raw.ends_with(quote) {
            let inner = &raw[1..raw.len() - 1];
            return !inner.contains(quote) && !inner.contains('\\');
        }
    }
    render_frontmatter_value(raw).as_deref() == Some(raw)
}

/// The key a YAML reader could see in `content` that [`parse_frontmatter`]
/// cannot, when there is one.  The agent's name and managed marker decide
/// where a file syncs and whether Automatic owns it, so both must read the
/// same to every reader.  That rules out a quoted key, a `name:` that runs on
/// to later lines, and any top-level line that is not a plain `key: value`:
/// `? name`, `{name: x}`, `<<: {name: x}` and `&a name: x` all set a key this
/// parser never sees.  Quoted keys are compared without unescaping, so one
/// holding a backslash could spell anything and is reported too.
pub(crate) fn frontmatter_key_in_another_form(content: &str) -> Option<&'static str> {
    const NAME: &str = "name";
    const MANAGED: &str = "automatic-managed";

    let block = frontmatter_block(content)?;
    if holds_a_hidden_line_break(block) {
        return Some(NAME);
    }
    let lines: Vec<&str> = block.split('\n').collect();
    for (index, line) in lines.iter().enumerate() {
        if line.trim().is_empty() || line.starts_with([' ', '\t', '#']) {
            continue;
        }
        if frontmatter_line_sets(line, MANAGED) && !has_space_after_colon(line) {
            return Some(MANAGED);
        }
        if frontmatter_line_sets(line, NAME)
            && (frontmatter_value_continues(&lines, index) || !name_line_reads_the_same(line))
        {
            return Some(NAME);
        }
        let Some(first) = line.chars().next() else {
            continue;
        };
        if first == '"' || first == '\'' {
            let key = line[1..].split(first).next().unwrap_or_default();
            if key == MANAGED {
                return Some(MANAGED);
            }
            if key == NAME || key.contains('\\') {
                return Some(NAME);
            }
        } else if !first.is_alphanumeric() && !continues_a_value(line) {
            return Some(NAME);
        }
    }
    None
}

/// Set or remove one top-level frontmatter key and leave every other byte
/// alone.  An existing line is replaced where it stands.  The parser keeps
/// the last line for a repeated key, so that is the one replaced.  A new key
/// goes at the end of the block.  `value: None` removes every line for `key`.
///
/// Refuses when [`render_frontmatter_value`] refuses the value, or when the
/// existing value runs over several lines.
pub(crate) fn set_frontmatter_value(
    content: &str,
    key: &str,
    value: Option<&str>,
) -> Result<String, FrontmatterEditRefusal> {
    let rendered = match value {
        Some(value) => Some(
            render_frontmatter_value(value).ok_or(FrontmatterEditRefusal::UnsafeValue)?,
        ),
        None => None,
    };
    write_frontmatter_line(content, key, rendered.as_deref())
}

/// [`set_frontmatter_value`] for an integer, which stays a number in YAML.
pub(crate) fn set_frontmatter_integer(
    content: &str,
    key: &str,
    value: i64,
) -> Result<String, FrontmatterEditRefusal> {
    write_frontmatter_line(content, key, Some(&value.to_string()))
}

/// Why a frontmatter key could not be edited.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FrontmatterEditRefusal {
    /// The new value cannot be written safely.
    UnsafeValue,
    /// The existing value runs over several lines.
    MultiLineValue,
}

/// The edit behind [`set_frontmatter_value`], with the value already rendered.
fn write_frontmatter_line(
    content: &str,
    key: &str,
    rendered: Option<&str>,
) -> Result<String, FrontmatterEditRefusal> {
    let line = rendered.map(|rendered| format!("{key}: {rendered}"));
    let Some(span) = frontmatter_span(content) else {
        return Ok(match line {
            Some(line) => format!("---\n{line}\n---\n{content}"),
            None => content.to_string(),
        });
    };

    if span.start == span.end {
        let Some(line) = line else {
            return Ok(content.to_string());
        };
        let line_break = &content[3..span.start];
        return Ok(format!(
            "{}{line}{line_break}{}",
            &content[..span.start],
            &content[span.end..]
        ));
    }

    let mut lines: Vec<&str> = content[span.start..span.end].split('\n').collect();
    let matching: Vec<usize> = (0..lines.len())
        .filter(|&index| frontmatter_line_sets(lines[index], key))
        .collect();
    let affected = match (&line, matching.last()) {
        (Some(_), Some(last)) => std::slice::from_ref(last),
        _ => &matching[..],
    };
    if affected
        .iter()
        .any(|&index| frontmatter_value_continues(&lines, index))
    {
        return Err(FrontmatterEditRefusal::MultiLineValue);
    }

    let carriage_return = if content.starts_with("---\r\n") { "\r" } else { "" };
    let replacement = line.map(|line| format!("{line}{carriage_return}"));
    match (&replacement, matching.last()) {
        (None, _) => {
            let mut index = 0;
            lines.retain(|_| {
                index += 1;
                !matching.contains(&(index - 1))
            });
        }
        (Some(replacement), Some(&last)) => lines[last] = replacement,
        (Some(replacement), None) => lines.push(replacement),
    }

    Ok(format!(
        "{}{}{}",
        &content[..span.start],
        lines.join("\n"),
        &content[span.end..]
    ))
}

/// Replace the Markdown body and leave the frontmatter and the whitespace
/// around the body alone.
pub(crate) fn replace_markdown_body(content: &str, body: &str) -> String {
    let body_start = frontmatter_span(content).map_or(0, |span| span.body_start);
    let (head, old_body) = content.split_at(body_start);
    let line_break = if content.contains("\r\n") { "\r\n" } else { "\n" };
    let separator = if head.is_empty() { "" } else { line_break };
    if body.is_empty() {
        return format!("{head}{separator}");
    }
    if old_body.trim().is_empty() {
        return format!("{head}{separator}{body}{line_break}");
    }
    let leading = old_body.len() - old_body.trim_start().len();
    let trailing = old_body.trim_end().len();
    format!(
        "{head}{}{body}{}",
        &old_body[..leading],
        &old_body[trailing..]
    )
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
    let (rendered, line_break) = match frontmatter_span(content) {
        Some(span) => {
            let yaml = &content[span.start..span.end];
            if yaml
                .lines()
                .any(|line| line.trim() == "automatic-managed: true")
            {
                return content.to_string();
            }
            let line_break = &content[3..span.start];
            let existing = yaml.trim_end_matches('\r');
            let separator = if existing.is_empty() { "" } else { line_break };
            let body = &content[span.body_start..];
            (
                format!("---{line_break}{existing}{separator}automatic-managed: true{line_break}---{body}"),
                line_break,
            )
        }
        None => {
            let line_break = if content.contains("\r\n") { "\r\n" } else { "\n" };
            (
                format!("---{line_break}automatic-managed: true{line_break}---{line_break}{content}"),
                line_break,
            )
        }
    };
    if rendered.ends_with('\n') {
        rendered
    } else {
        rendered + line_break
    }
}

pub(crate) fn is_managed_command_file(path: &Path) -> bool {
    fs::read_to_string(path).is_ok_and(|content| is_managed_command_content(path, &content))
}

/// [`is_managed_command_file`] for text already read from `path`, so a
/// caller that read the file through a guarded reader need not read it
/// again.  `path` supplies only the extension.
pub(crate) fn is_managed_command_content(path: &Path, content: &str) -> bool {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return false;
    };

    match ext {
        "md" => {
            let (frontmatter, _) = parse_frontmatter(content);
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

/// `true` when rebuild refuses the command file at `path`, for its name or
/// because it cannot be read as project text.  Sync must then leave the file
/// alone: rebuild dropped it for being unreadable, not for being stale.  The
/// whole file name stands in for the command name, because a refused
/// character is refused in either, and `root` may be a hub with no agent.
pub(crate) fn command_file_is_refused(root: &Path, path: &Path) -> bool {
    let name_refused = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_none_or(|name| !crate::core::is_valid_asset_name(name));
    name_refused || crate::sync::read_project_text(root, path).is_err()
}

/// `true` for a regular file at `path` that rebuild cannot read, because it
/// is too large or not UTF-8.  It may be the only copy of what it holds, so
/// sync must not replace it.  Replacing a link loses nothing.
fn is_unreadable_regular_file(root: &Path, path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| meta.is_file())
        && crate::sync::read_project_text(root, path).is_err()
}

/// Write one command file unless [`is_unreadable_regular_file`] says not to.
fn write_command_file(root: &Path, path: &Path, rendered: String) -> Result<bool, String> {
    if is_unreadable_regular_file(root, path) {
        eprintln!(
            "[automatic] sync: not replacing command '{}': it cannot be read",
            path.display()
        );
        return Ok(false);
    }
    crate::sync::write_project_file_or_skip(root, path, rendered)
}

/// Sub-agent counterpart to [`is_managed_command_file`], with one extra
/// branch: Kiro's format is JSON, which no command vendor uses. Each
/// branch's marker matches the casing convention its own converter already
/// uses for every other key in that format — kebab-case YAML frontmatter,
/// snake_case TOML, camelCase JSON — rather than a single spelling forced
/// across three different serialisations.
pub(crate) fn is_managed_agent_file(path: &Path) -> bool {
    fs::read_to_string(path).is_ok_and(|content| is_managed_agent_content(path, &content))
}

/// [`is_managed_agent_file`] for text already read from `path`.  `path`
/// supplies only the extension.
pub(crate) fn is_managed_agent_content(path: &Path, content: &str) -> bool {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return false;
    };

    match ext {
        "md" => {
            let (frontmatter, _) = parse_frontmatter(content);
            frontmatter
                .get("automatic-managed")
                .map(|value| value == "true")
                .unwrap_or(false)
        }
        "toml" => content
            .lines()
            .any(|line| line.trim() == "automatic_managed = true"),
        "json" => serde_json::from_str::<Value>(content)
            .ok()
            .and_then(|v| v.get("automaticManaged").and_then(|m| m.as_bool()))
            .unwrap_or(false),
        _ => false,
    }
}

/// Check every command name before a writer touches the disk. Custom command
/// names come from a project's committed config, and each name becomes a
/// file name in a commands directory.
fn ensure_valid_command_names(
    workspace_commands: &[(String, String)],
    custom_commands: &[crate::core::CustomCommand],
) -> Result<(), String> {
    for (name, _) in workspace_commands {
        crate::core::ensure_valid_asset_name("Command", name)?;
    }
    for command in custom_commands {
        crate::core::ensure_valid_asset_name("Custom command", &command.name)?;
    }
    Ok(())
}

pub(crate) fn copy_commands_to_project(
    root: &Path,
    project_commands_dir: &Path,
    workspace_commands: &[(String, String)],
    custom_commands: &[crate::core::CustomCommand],
    skip_custom_names: &std::collections::HashSet<String>,
) -> Result<Vec<String>, String> {
    ensure_valid_command_names(workspace_commands, custom_commands)?;
    let mut written = Vec::new();
    let mut expected: HashSet<String> = HashSet::new();
    let mut index_entries = Vec::new();

    let total_commands = workspace_commands.len() + custom_commands.len();
    if total_commands > 0 {
        crate::sync::create_project_dir(root, project_commands_dir)?;
    }

    for (name, content) in workspace_commands {
        if skip_unsafe_name("command", name) {
            continue;
        }
        let file_name = format!("{name}.md");
        expected.insert(file_name.clone());
        let path = project_commands_dir.join(&file_name);
        if crate::sync::write_project_file_or_skip(root, &path, render_markdown_command(content))? {
            written.push(path.display().to_string());
        }
        let (frontmatter, _) = parse_frontmatter(content);
        index_entries.push(CommandIndexEntry {
            name: name.clone(),
            description: frontmatter.get("description").cloned().unwrap_or_default(),
        });
    }

    for command in custom_commands {
        if skip_unsafe_name("command", &command.name) {
            continue;
        }
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
        if crate::sync::write_project_file_or_skip(
            root,
            &path,
            render_markdown_command(&command.content),
        )? {
            written.push(path.display().to_string());
        }
        let (frontmatter, _) = parse_frontmatter(&command.content);
        index_entries.push(CommandIndexEntry {
            name: command.name.clone(),
            description: frontmatter.get("description").cloned().unwrap_or_default(),
        });
    }

    cleanup_stale_managed_command_files(root, project_commands_dir, &expected, &mut written)?;

    if !index_entries.is_empty() {
        let index_path = write_commands_index(root, project_commands_dir, &index_entries)?;
        written.push(index_path.display().to_string());
    } else if let Some(index_path) = cleanup_commands_index(root, project_commands_dir)? {
        written.push(index_path.display().to_string());
    }

    Ok(written)
}

pub(crate) fn symlink_commands_from_project(
    root: &Path,
    agent_commands_dir: &Path,
    project_commands_dir: &Path,
    workspace_commands: &[(String, String)],
    custom_commands: &[crate::core::CustomCommand],
    agent_instance: &dyn Agent,
) -> Result<Vec<String>, String> {
    ensure_valid_command_names(workspace_commands, custom_commands)?;
    let mut written = Vec::new();
    let mut expected: HashSet<String> = HashSet::new();
    let settings = crate::core::read_settings().unwrap_or_default();
    let use_symlink = settings.sync_mode == "symlink";

    let total_commands = workspace_commands.len() + custom_commands.len();
    if total_commands > 0 {
        crate::sync::create_project_dir(root, agent_commands_dir)?;
    }

    for (name, _) in workspace_commands {
        if skip_unsafe_name("command", name) {
            continue;
        }
        let file_name = agent_instance.command_file_name(name);
        expected.insert(file_name.clone());
        let link_path = agent_commands_dir.join(&file_name);
        let target_path = project_commands_dir.join(format!("{name}.md"));
        if sync_command_link(&link_path, &target_path, root, use_symlink)? {
            written.push(link_path.display().to_string());
        }
    }

    for command in custom_commands {
        if skip_unsafe_name("command", &command.name) {
            continue;
        }
        let file_name = agent_instance.command_file_name(&command.name);
        expected.insert(file_name.clone());
        let link_path = agent_commands_dir.join(&file_name);
        let target_path = project_commands_dir.join(format!("{}.md", command.name));
        if sync_command_link(&link_path, &target_path, root, use_symlink)? {
            written.push(link_path.display().to_string());
        }
    }

    cleanup_stale_managed_command_files(root, agent_commands_dir, &expected, &mut written)?;

    Ok(written)
}

/// `true` when `path` resolves, through every link along it, to a regular
/// file inside `root` and outside any `.git` directory (a clone's
/// `.git/config` can carry a token in its remote URL).
fn is_regular_file_in(root: &Path, path: &Path) -> bool {
    let Ok(target) = fs::canonicalize(path) else {
        return false;
    };
    let inside = fs::canonicalize(root).is_ok_and(|root| {
        target.strip_prefix(&root).is_ok_and(|rel| {
            !rel.components()
                .any(|c| c.as_os_str().eq_ignore_ascii_case(".git"))
        })
    });
    inside && fs::metadata(&target).is_ok_and(|m| m.is_file())
}

pub(crate) fn sync_commands_to_dir(
    root: &Path,
    commands_dir: &Path,
    workspace_commands: &[(String, String)],
    custom_commands: &[crate::core::CustomCommand],
    agent_instance: &dyn Agent,
    skip_custom_names: &std::collections::HashSet<String>,
) -> Result<Vec<String>, String> {
    ensure_valid_command_names(workspace_commands, custom_commands)?;
    let mut written = Vec::new();
    let mut expected: HashSet<String> = HashSet::new();

    let total_commands = workspace_commands.len() + custom_commands.len();
    if total_commands > 0 {
        crate::sync::create_project_dir(root, commands_dir)?;
    }

    for (name, content) in workspace_commands {
        if skip_unsafe_name("command", name) {
            continue;
        }
        let file_name = agent_instance.command_file_name(name);
        expected.insert(file_name.clone());
        let path = commands_dir.join(&file_name);
        let rendered = agent_instance.convert_command_content(content, name);
        if write_command_file(root, &path, rendered)? {
            written.push(path.display().to_string());
        }
    }

    for command in custom_commands {
        if skip_unsafe_name("command", &command.name) {
            continue;
        }
        let file_name = agent_instance.command_file_name(&command.name);
        expected.insert(file_name.clone());
        if skip_custom_names.contains(&command.name) {
            continue;
        }
        let path = commands_dir.join(&file_name);
        let rendered = agent_instance.convert_command_content(&command.content, &command.name);
        if write_command_file(root, &path, rendered)? {
            written.push(path.display().to_string());
        }
    }

    cleanup_stale_managed_command_files(root, commands_dir, &expected, &mut written)?;

    Ok(written)
}

/// Link or copy one hub command into an agent's commands directory.
/// Returns whether it did; a refused entry is skipped with a log line.
fn sync_command_link(
    link_path: &Path,
    target_path: &Path,
    project_root: &Path,
    use_symlink: bool,
) -> Result<bool, String> {
    if is_unreadable_regular_file(project_root, link_path) {
        eprintln!(
            "[automatic] sync: not replacing command '{}': it cannot be read",
            link_path.display()
        );
        return Ok(false);
    }
    if let Ok(meta) = link_path.symlink_metadata() {
        if meta.file_type().is_symlink() || meta.is_file() {
            fs::remove_file(link_path).map_err(|e| {
                format!(
                    "Failed to replace command file '{}': {}",
                    link_path.display(),
                    e
                )
            })?;
        } else {
            // A FIFO would block the copy, and a directory is not ours.
            eprintln!(
                "[automatic] sync: not replacing command '{}': it is not a regular file",
                link_path.display()
            );
            return Ok(false);
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
        // Automatic writes every hub entry as a regular file inside the
        // project.  Anything else was placed there by hand or by a cloned
        // repo (a FIFO, a link at the file or at `.agents/`), and copying it
        // would block or bring outside content into the project.
        if target_path.symlink_metadata().is_ok()
            && !is_regular_file_in(project_root, target_path)
        {
            eprintln!(
                "[automatic] sync: not copying command '{}': it is not a regular file inside the project",
                target_path.display()
            );
            return Ok(false);
        }
        fs::copy(target_path, link_path).map_err(|e| {
            format!(
                "Failed to copy command '{}' -> '{}': {}",
                target_path.display(),
                link_path.display(),
                e
            )
        })?;
    }

    Ok(true)
}

fn cleanup_stale_managed_command_files(
    root: &Path,
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

            // The guarded check goes first: the marker check reads the whole
            // file with no size limit.
            if expected.contains(file_name)
                || command_file_is_refused(root, &path)
                || !is_managed_command_file(&path)
            {
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
    root: &Path,
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
    crate::sync::write_project_file(root, &index_path, content)?;

    Ok(index_path)
}

fn commands_index_path(commands_dir: &Path) -> std::path::PathBuf {
    commands_dir
        .parent()
        .map(|parent| parent.join("commands-index.md"))
        .unwrap_or_else(|| commands_dir.join("commands-index.md"))
}

fn cleanup_commands_index(
    root: &Path,
    commands_dir: &Path,
) -> Result<Option<std::path::PathBuf>, String> {
    let index_path = commands_index_path(commands_dir);
    // A guarded read: a committed link to `/dev/zero` or a FIFO must not hang
    // sync.  What cannot be read as project text is not Automatic's index.
    let managed = crate::sync::read_project_text(root, &index_path)
        .ok()
        .flatten()
        .is_some_and(|content| is_managed_command_content(&index_path, &content));
    if managed {
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
///
/// Under `cfg(test)` this first consults [`crate::core::paths::test_home_override`]
/// so global-scope tests can redirect reads and writes into a tempdir via
/// [`crate::core::paths::with_test_home`].  The override is thread-local; every
/// test that installs one must do its work on the same thread (cargo's default
/// per-test threading is fine).  Without an override, tests get
/// [`crate::core::default_test_home`], never the real `~`.
pub(crate) fn home_dir() -> Option<PathBuf> {
    #[cfg(test)]
    return Some(
        crate::core::test_home_override().unwrap_or_else(crate::core::default_test_home),
    );

    #[cfg(not(test))]
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

/// One piece of a header value: literal text, or the name of an environment
/// variable referenced as `${NAME}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HeaderValuePart {
    Literal(String),
    EnvReference(String),
}

/// VS Code launch variables share the `${...}` shape but are substituted by
/// [`expand_workspace_variables`], so they are never environment references.
const LAUNCH_VARIABLES: &[&str] = &["workspaceFolder", "userHome"];

fn is_env_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !LAUNCH_VARIABLES.contains(&name)
}

/// Length of the environment variable name at the start of `text` when it is
/// followed at once by `}`.  `None` when `text` does not open with `NAME}`.
///
/// Scans only the name itself, so a value made of many unterminated `${`
/// costs one pass in [`parse_header_value`] and not one pass per `${`.
fn env_reference_name_len(text: &str) -> Option<usize> {
    let len = text
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(text.len());
    let name = &text[..len];
    (text[len..].starts_with('}') && is_env_variable_name(name)).then_some(len)
}

/// Split a header value into literal text and `${NAME}` environment
/// references.
///
/// Only a bare variable name counts as a reference.  Anything else inside
/// `${...}` — `${env:NAME}`, `${NAME:-default}`, a launch variable — stays
/// literal, so a value written in another dialect passes through unchanged.
pub(crate) fn parse_header_value(value: &str) -> Vec<HeaderValuePart> {
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut rest = value;

    while let Some(start) = rest.find("${") {
        let after_open = &rest[start + 2..];
        match env_reference_name_len(after_open) {
            Some(len) => {
                literal.push_str(&rest[..start]);
                if !literal.is_empty() {
                    parts.push(HeaderValuePart::Literal(std::mem::take(&mut literal)));
                }
                parts.push(HeaderValuePart::EnvReference(after_open[..len].to_string()));
                rest = &after_open[len + 1..];
            }
            None => {
                literal.push_str(&rest[..start + 2]);
                rest = after_open;
            }
        }
    }

    literal.push_str(rest);
    if !literal.is_empty() {
        parts.push(HeaderValuePart::Literal(literal));
    }
    parts
}

/// Undo [`substitute_header_env_references`] on a server entry read back from
/// an agent's own config file: every `<open>NAME}` in `headers` becomes the
/// canonical `${NAME}`.
///
/// The sync engine saves discovered entries over the registry.  Without this,
/// the second sync would store one agent's spelling as the canonical value and
/// every other agent would be handed a placeholder it cannot resolve.
///
/// `open` is the agent's opening token, `${env:` or `{env:`.  A match directly
/// after `$` is skipped when `open` itself has no `$`, so OpenCode's `{env:`
/// does not fire inside another dialect's `${env:NAME}`.
pub(crate) fn restore_header_env_references(server: &mut Value, open: &str) {
    let Some(Value::Object(headers)) = server.get_mut("headers") else {
        return;
    };
    for value in headers.values_mut() {
        let Some(text) = value.as_str() else {
            continue;
        };
        let mut restored = String::new();
        let mut rest = text;
        while let Some(start) = rest.find(open) {
            let after_open = &rest[start + open.len()..];
            let inside_other_dialect = !open.starts_with('$') && rest[..start].ends_with('$');
            match env_reference_name_len(after_open).filter(|_| !inside_other_dialect) {
                Some(len) => {
                    restored.push_str(&rest[..start]);
                    restored.push_str(&format!("${{{}}}", &after_open[..len]));
                    rest = &after_open[len + 1..];
                }
                None => {
                    restored.push_str(&rest[..start + open.len()]);
                    rest = after_open;
                }
            }
        }
        restored.push_str(rest);
        *value = Value::String(restored);
    }
}

/// Rewrite every `${NAME}` reference in a server entry's `headers` in place
/// with `render(NAME)`.
///
/// Shared by the agents whose config format interpolates variables inside a
/// header value; the only difference between them is the placeholder spelling.
pub(crate) fn substitute_header_env_references(
    server: &mut Map<String, Value>,
    render: impl Fn(&str) -> String,
) {
    let Some(Value::Object(headers)) = server.get_mut("headers") else {
        return;
    };
    for value in headers.values_mut() {
        let Some(text) = value.as_str() else {
            continue;
        };
        let rewritten: String = parse_header_value(text)
            .into_iter()
            .map(|part| match part {
                HeaderValuePart::Literal(text) => text,
                HeaderValuePart::EnvReference(name) => render(&name),
            })
            .collect();
        *value = Value::String(rewritten);
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
    prepare_mcp_servers_impl(agent, servers, Some(workspace_folder))
}

/// Global-scope sibling of [`prepare_mcp_servers`].
///
/// Same `_`-strip and inherited-env rewrite; `${userHome}` is expanded but
/// `${workspaceFolder}` is left alone because there is no project directory
/// at global scope.  Callers are expected to filter servers whose config
/// still references `${workspaceFolder}` via [`server_uses_workspace_folder`]
/// before calling this, otherwise the literal reaches the agent config file
/// and becomes a launch path that never resolves.
pub(crate) fn prepare_global_mcp_servers(
    agent: &dyn Agent,
    servers: &Map<String, Value>,
) -> Map<String, Value> {
    prepare_mcp_servers_impl(agent, servers, None)
}

fn prepare_mcp_servers_impl(
    agent: &dyn Agent,
    servers: &Map<String, Value>,
    workspace_folder: Option<&Path>,
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
            agent.rewrite_header_env_references(obj);
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
///
/// `workspace_folder = None` skips `${workspaceFolder}` substitution — used by
/// the global-scope writer, which has no project to substitute for.  Servers
/// still carrying `${workspaceFolder}` after preparation are meaningless at
/// global scope; the eligibility helper [`server_uses_workspace_folder`]
/// filters them out before they reach the writer.
fn expand_workspace_variables(
    value: &mut Value,
    workspace_folder: Option<&Path>,
    home: Option<&Path>,
) {
    match value {
        Value::String(s) => {
            if !s.contains("${") {
                return;
            }
            let mut out = s.clone();
            if let Some(workspace_folder) = workspace_folder {
                out = out.replace("${workspaceFolder}", &workspace_folder.to_string_lossy());
            }
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

/// Return `true` if any string anywhere in `value` contains the literal
/// `${workspaceFolder}`.  Used at global-scope selection time to mark servers
/// ineligible for global assignment: `${workspaceFolder}` has no meaning
/// without a project, and expanding it to an empty or arbitrary path would
/// silently point the agent at the wrong directory.
pub(crate) fn server_uses_workspace_folder(value: &Value) -> bool {
    match value {
        Value::String(s) => s.contains("${workspaceFolder}"),
        Value::Array(items) => items.iter().any(server_uses_workspace_folder),
        Value::Object(map) => map.values().any(server_uses_workspace_folder),
        _ => false,
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
    discover_mcp_servers_from_json_at(path, &[root_key], normalise)
}

/// Largest MCP config [`read_discovery_file`] reads.  Far above any real
/// project config, but user-level files such as `~/.claude.json` hold more
/// than MCP servers and can grow to several megabytes.
const MAX_DISCOVERY_BYTES: u64 = 64 * 1024 * 1024;

/// Read an MCP config for discovery, or `None` when it is absent, not a
/// regular file, too large, or not UTF-8.  Project discovery runs on a timer
/// against cloned repos, so a config that is a FIFO or a link to `/dev/zero`
/// must not block or exhaust memory.  `stat` follows links without opening
/// the file, so a FIFO is never opened.
fn read_discovery_file(path: &Path) -> Option<String> {
    if !fs::metadata(path).is_ok_and(|m| m.is_file()) {
        return None;
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(MAX_DISCOVERY_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_DISCOVERY_BYTES {
        eprintln!(
            "[automatic] skipping MCP discovery: {} is larger than {MAX_DISCOVERY_BYTES} bytes",
            path.display()
        );
        return None;
    }
    String::from_utf8(bytes).ok()
}

/// Same as [`discover_mcp_servers_from_json`], but walks a nested key path to
/// the servers map (e.g. `["mcp", "servers"]` for Z Code's CLI config).
pub(crate) fn discover_mcp_servers_from_json_at(
    path: &Path,
    key_path: &[&str],
    normalise: fn(Value) -> Value,
) -> Map<String, Value> {
    let mut result = Map::new();

    let Some(content) = read_discovery_file(path) else {
        return result;
    };

    let root: Value = match serde_json::from_str::<Value>(&content) {
        Ok(v @ Value::Object(_)) => v,
        _ => return result,
    };

    let mut node = &root;
    for key in key_path {
        node = match node.get(key) {
            Some(v) => v,
            None => return result,
        };
    }

    let servers_obj = match node {
        Value::Object(s) => s,
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

/// Read the raw set of MCP server entry names from a JSON config file, without
/// the proxy-stub filter that [`discover_mcp_servers_from_json`] applies.
///
/// The import-oriented discovery skips Automatic-generated `mcp-proxy` stubs
/// to avoid re-slurping them back into the shared registry.  For drift/status
/// checks we want the opposite: the stub is a valid on-disk entry and its
/// name still needs to compare against the managed set.  This helper keeps
/// the `automatic`/`nexus`/invalid-name skips (drift never wants to treat
/// those as foreign) but preserves stubs.
pub(crate) fn read_global_mcp_entry_names_json(path: &Path, root_key: &str) -> HashSet<String> {
    read_global_mcp_entry_names_json_at(path, &[root_key])
}

/// Nested-key-path variant of [`read_global_mcp_entry_names_json`], mirroring
/// [`discover_mcp_servers_from_json_at`].
pub(crate) fn read_global_mcp_entry_names_json_at(
    path: &Path,
    key_path: &[&str],
) -> HashSet<String> {
    let mut names = HashSet::new();

    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return names,
    };

    let root: Value = match serde_json::from_str::<Value>(&content) {
        Ok(v @ Value::Object(_)) => v,
        _ => return names,
    };

    let mut node = &root;
    for key in key_path {
        node = match node.get(key) {
            Some(v) => v,
            None => return names,
        };
    }

    let servers_obj = match node {
        Value::Object(s) => s,
        _ => return names,
    };

    for name in servers_obj.keys() {
        if name == "automatic" || name == "nexus" || !crate::core::is_valid_name(name) {
            continue;
        }
        names.insert(name.clone());
    }

    names
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
        dialect_servers.insert(name.clone(), render_opencode_entry(config));
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

/// Render one MCP server entry into the OpenCode-dialect shape used by the
/// top-level `mcp` object.
///
/// This is the per-entry work shared by [`write_opencode_dialect_mcp_config`]
/// (project-scope, replace-whole-key writer) and the global-scope
/// [`Agent::write_global_mcp_config`] implementations for OpenCode and Kilo,
/// which need to render entries individually before feeding them to
/// [`merge_global_mcp_entries_json`].
///
/// The dialect:
/// - `type: "stdio"` (or absent) becomes `type: "local"`, with `command`
///   rendered as a `[command, ...args]` array and `env` renamed to
///   `environment`.
/// - `type: "http"`/`"sse"` becomes `type: "remote"`, preserving `url`,
///   `headers` and `oauth` as-is.
/// - `enabled: false` and `timeout` are copied through when present.
pub(crate) fn render_opencode_entry(config: &Value) -> Value {
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

    Value::Object(server)
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

// ── Global MCP entry-level merge ────────────────────────────────────────────

/// Entry-level merge for a JSON-dialect global MCP config file.
///
/// Reads the file at `path`, locates `root[servers_key]`, and applies the
/// desired-vs-managed diff described in the plan:
///
/// - Names in `previously_managed` that are absent from `desired` are removed
///   (returned as [`GlobalMcpWriteReport::removed`]).
/// - For each `(name, cfg)` in `desired`: if the file already carries `name`
///   and it is **not** in `previously_managed` it is a foreign entry — left
///   untouched, returned as [`GlobalMcpWriteReport::skipped`].  Otherwise the
///   entry is upserted (returned as [`GlobalMcpWriteReport::written`]).
/// - If the servers map is empty after the merge, the key is stripped from
///   root (matching the empty-map behaviour of the hook merge and the
///   OpenCode-dialect writer).
///
/// Never clobber: an unparseable target file is an error, not an empty
/// starting point — every non-managed key in the same file is preserved
/// byte-for-byte otherwise.  A pretty-printed JSON re-serialise identical to
/// the pre-read bytes skips the write entirely (`unchanged = true`), which is
/// what protects auto-reloading agents (Droid/Kiro) from watcher churn and
/// keeps the race window against a running Claude Code as small as possible.
pub(crate) fn merge_global_mcp_entries_json(
    path: &Path,
    servers_key: &str,
    desired: &Map<String, Value>,
    previously_managed: &[String],
) -> Result<GlobalMcpWriteReport, String> {
    merge_global_mcp_entries_json_at(path, &[servers_key], desired, previously_managed)
}

/// Nested-key-path variant of [`merge_global_mcp_entries_json`], mirroring
/// [`discover_mcp_servers_from_json_at`] and
/// [`read_global_mcp_entry_names_json_at`].
///
/// Used by dialects whose servers map lives under a nested path (e.g.
/// `["mcp", "servers"]` for Z Code's CLI config).  The last segment names the
/// servers map; the preceding segments name intermediate objects that are
/// created on write and pruned on empty.
pub(crate) fn merge_global_mcp_entries_json_at(
    path: &Path,
    key_path: &[&str],
    desired: &Map<String, Value>,
    previously_managed: &[String],
) -> Result<GlobalMcpWriteReport, String> {
    assert!(
        !key_path.is_empty(),
        "merge_global_mcp_entries_json_at requires at least one key"
    );

    // Read the raw bytes for the no-op comparison as well as the parsed root.
    let original_bytes = if path.exists() {
        fs::read(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))?
    } else {
        Vec::new()
    };
    let root = read_mergeable_json_object(path)?;

    // Walk the key path, extracting each intermediate object by value so we can
    // re-attach it at the end.  A present-but-not-object intermediate is an
    // error rather than a silent clobber.
    let mut path_stack: Vec<(String, Map<String, Value>)> = Vec::new();
    let mut cursor = root;
    for key in &key_path[..key_path.len() - 1] {
        let next = match cursor.remove(*key) {
            Some(Value::Object(m)) => m,
            Some(_) => {
                return Err(format!(
                    "{} contains a non-object value under \"{}\" — refusing to overwrite",
                    path.display(),
                    key
                ));
            }
            None => Map::new(),
        };
        path_stack.push(((*key).to_string(), cursor));
        cursor = next;
    }
    let servers_key = key_path[key_path.len() - 1];

    // Extract the current servers map. Absent -> empty; present-but-not-object
    // -> error rather than clobber.
    let mut entries: Map<String, Value> = match cursor.remove(servers_key) {
        Some(Value::Object(m)) => m,
        Some(_) => {
            return Err(format!(
                "{} contains a non-object value under \"{}\" — refusing to overwrite",
                path.display(),
                servers_key
            ));
        }
        None => Map::new(),
    };

    let mut report = GlobalMcpWriteReport {
        path: path.display().to_string(),
        ..Default::default()
    };

    // ── Remove: names Automatic wrote last time that are no longer desired.
    for name in previously_managed {
        if desired.contains_key(name) {
            continue;
        }
        if entries.remove(name).is_some() {
            report.removed.push(name.clone());
        }
    }

    // ── Upsert / skip on foreign collision.
    let previously_managed_set: std::collections::HashSet<&String> =
        previously_managed.iter().collect();
    for (name, cfg) in desired {
        if entries.contains_key(name) && !previously_managed_set.contains(name) {
            report.skipped.push(name.clone());
            continue;
        }
        entries.insert(name.clone(), cfg.clone());
        report.written.push(name.clone());
    }

    // ── Re-attach the servers key (or drop it if now empty).
    if !entries.is_empty() {
        cursor.insert(servers_key.to_string(), Value::Object(entries));
    }

    // ── Re-attach each intermediate object, pruning any that end up empty.
    //    An empty intermediate is dropped so the file does not accumulate
    //    stub `{"mcp": {}}` objects after every server is removed.
    while let Some((key, mut parent)) = path_stack.pop() {
        if !cursor.is_empty() {
            parent.insert(key, Value::Object(cursor));
        }
        cursor = parent;
    }
    let root = cursor;

    // ── Serialise + skip-when-unchanged.
    let mut content = serde_json::to_string_pretty(&Value::Object(root))
        .map_err(|e| format!("JSON serialise error: {}", e))?;
    // Match the trailing-newline convention of the existing writers, which
    // makes diffs and hand-edits play nice with editors.
    if !content.ends_with('\n') {
        content.push('\n');
    }

    if content.as_bytes() == original_bytes.as_slice() {
        report.unchanged = true;
        return Ok(report);
    }

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
        }
    }
    fs::write(path, content).map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;

    Ok(report)
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
    project_dir: &Path,
    scripts_dir: &Path,
    hook: &crate::core::Hook,
    interpreter: &str,
    script: &str,
) -> Result<PathBuf, String> {
    let ext = hook_script_extension(interpreter);
    let path = scripts_dir.join(format!("{}.{}", hook_slug(hook), ext));
    let body = hook_annotate_managed_script(&hook_ensure_shebang(script, interpreter));
    // A committed link at the script's name, or at its directory, would
    // otherwise get this body written, and made executable, elsewhere.  A
    // refusal is an error rather than a skip: the hook config written next
    // would still run whatever the link points at.
    crate::sync::write_project_file(project_dir, &path, body)?;
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
    /// The project root.  Script bodies must resolve inside it.
    pub project_dir: PathBuf,
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
    project_dir: &Path,
    scripts_dir: &Path,
    hooks: &[&crate::core::Hook],
    written: &mut Vec<String>,
) -> Result<Vec<PathBuf>, String> {
    let mut managed_script_paths = Vec::new();
    let needs_scripts_dir = hooks
        .iter()
        .any(|h| matches!(h.handler, crate::core::HookHandler::Script { .. }));
    if needs_scripts_dir {
        crate::sync::create_project_dir(project_dir, scripts_dir)?;
    }
    for hook in hooks {
        if let crate::core::HookHandler::Script {
            interpreter,
            script,
        } = &hook.handler
        {
            let path =
                write_managed_hook_script(project_dir, scripts_dir, hook, interpreter, script)?;
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
        write_script_handler_bodies(
            &spec.project_dir,
            &spec.scripts_dir,
            &usable_hooks,
            &mut written,
        )?;

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
        write_script_handler_bodies(
            &spec.project_dir,
            &spec.scripts_dir,
            &hook_refs,
            &mut written,
        )?;

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

    /// This proves the round trip through `parse_frontmatter` only.  No YAML
    /// crate is in the tree, so a real YAML reader is not checked.
    #[test]
    fn render_frontmatter_value_round_trips_through_the_parser() {
        for value in [
            "plain",
            "with: colon",
            "ends with colon:",
            "has # hash",
            "\"double\"",
            "'single'",
            "trailing ",
            " leading",
            "\"",
            "[list]",
            "{map}",
            "&anchor",
            "*alias",
            "!tag",
            "|block",
            ">fold",
            "%directive",
            "@at",
            "`tick",
            "- dash",
            "? question",
            "#hash",
            "back\\slash: yes",
            "\tleading tab",
            "trailing tab\t",
            "",
            "-",
            "?",
            "~",
            "null",
            "true",
            "false",
        ] {
            let rendered = render_frontmatter_value(value).unwrap_or_else(|| panic!("{value}"));
            let content = format!("---\nkey: {rendered}\n---\n");
            let (frontmatter, _) = parse_frontmatter(&content);
            assert_eq!(frontmatter.get("key").map(String::as_str), Some(value));
        }
    }

    /// `value` must be accepted, and in a block of three keys a real YAML
    /// parser must read it as one string and find no extra key.
    fn frontmatter_value_is_one_yaml_string(value: &str) {
        let Some(rendered) = render_frontmatter_value(value) else {
            panic!("{value:?} was refused");
        };
        let block = format!("first: x\nkey: {rendered}\nlast: y\n");
        let docs = yaml_rust2::YamlLoader::load_from_str(&block)
            .unwrap_or_else(|e| panic!("{value:?} as {rendered:?}: {e}"));
        assert_eq!(docs.len(), 1, "{value:?}");
        let map = docs[0].as_hash().unwrap_or_else(|| panic!("{value:?}"));
        assert_eq!(map.len(), 3, "{value:?} as {rendered:?}");
        assert_eq!(
            docs[0]["key"],
            yaml_rust2::Yaml::String(value.to_string()),
            "{value:?} as {rendered:?}"
        );
    }

    const YAML_VALUES: &[&str] = &[
        "plain", "two words", "a:b", "a: b", "a :b", "a#b", "a #b", "#a", "a:", ":", "::", ": a",
        "-", "--", "---", "- a", "-a", "?", "??", "? a", "?a", "~", "~a", "null", "Null", "NULL",
        "nullable", "true", "True", "TRUE", "truthy", "false", "False", "FALSE", "yes", "Yes",
        "no", "No", "on", "On", "off", "Off", "0", "1", "-1", "+1", "1.5", "-1.5", "1e3", "1E3",
        ".5", "5.", ".inf", "-.inf", "+.inf", ".Inf", ".INF", ".nan", ".NaN", ".NAN", "0x1F",
        "0o17", "0b1", "1_000", "2001-12-14", "12:30:00", "[a]", "[", "]", "{a}", "{", "}", ",",
        ", a", "a,b", "a, b", "&a", "*a", "!a", "!!str a", "|", "|-", ">", ">-", "'", "\"", "%a",
        "@a", "`a", "a'b", "a\"b", "'a", "a'", "\"a", "a\"", "'a'", "\"a\"", "it's: x",
        "tab\tinside", "\tlead", "trail\t", " lead", "trail ", "  both  ", "",
        "back\\slash", "\\n", "a: \\n", "c:\\temp", "é: ü", "日本語", "emoji 🎉", "<<", "=",
        "key: [x]", "--- a", "... a", "...", "a\u{a0}b", "gpt-5.4", "inherit", "Read, Grep",
    ];

    /// The gate must hold against a real parser, not only the repo's own.
    #[test]
    fn accepted_frontmatter_values_are_one_yaml_string() {
        for value in YAML_VALUES {
            frontmatter_value_is_one_yaml_string(value);
        }
    }

    /// The same for what a user might build from those pieces.
    #[test]
    fn combined_frontmatter_values_are_one_yaml_string_or_refused() {
        let pieces = [
            "a", ":", ": ", "#", " #", "'", "\"", "\\", "-", "- ", "?", "? ", "[", "{", "]", "}", ",",
            "&", "*", "!", "|", ">", "%", "@", "`", "~", "\t", " ",
        ];
        for first in pieces {
            for second in pieces {
                for third in ["", "a", "1"] {
                    let value = format!("{first}{second}{third}");
                    if render_frontmatter_value(&value).is_some() {
                        frontmatter_value_is_one_yaml_string(&value);
                    }
                }
            }
        }
    }

    /// YAML allows spaces after the closing marker.  Anything else on the
    /// line stops it being one.
    #[test]
    fn a_closing_line_may_have_trailing_spaces() {
        for close in ["--- ", "---  ", "---\t", "--- \t "] {
            for line_break in ["\n", "\r\n"] {
                let content = format!("---{line_break}a: 1{line_break}{close}{line_break}Body.{line_break}");
                let (frontmatter, body) = parse_frontmatter(&content);
                assert_eq!(frontmatter.get("a").map(String::as_str), Some("1"), "{close:?}");
                assert_eq!(body.trim(), "Body.", "{close:?}");
            }
        }
    }

    #[test]
    fn only_a_marker_closes_the_block() {
        for close in ["---x", "-----", "--- x", " ---"] {
            let content = format!("---\na: 1\n{close}\nBody.\n");
            assert!(frontmatter_block(&content).is_none(), "{close:?}");
        }
    }

    /// The edit keeps the closing line as the user wrote it.
    #[test]
    fn editing_keeps_a_closing_line_with_trailing_spaces() {
        assert_eq!(
            set_frontmatter_value("---\na: 1\n---  \nBody.\n", "a", Some("two")).unwrap(),
            "---\na: two\n---  \nBody.\n"
        );
        assert_eq!(
            replace_markdown_body("---\na: 1\n---  \nBody.\n", "New."),
            "---\na: 1\n---  \nNew.\n"
        );
    }

    #[test]
    fn an_integer_stays_a_number_in_frontmatter() {
        assert_eq!(
            set_frontmatter_integer("---\nmaxTurns: 1\n---\n", "maxTurns", 5).unwrap(),
            "---\nmaxTurns: 5\n---\n"
        );
    }

    #[test]
    fn render_frontmatter_value_leaves_a_plain_value_alone() {
        for value in ["plain", "it's fine", "say \"hi\" now", "a:b", "x-y", "a#b", "tab\tinside"] {
            assert_eq!(render_frontmatter_value(value).as_deref(), Some(value));
        }
    }

    #[test]
    fn render_frontmatter_value_refuses_what_cannot_stay_on_one_line() {
        for value in [
            "a\nb",
            "a\r",
            "a\u{7}b",
            "a\u{7f}b",
            "a\u{85}b",
            "a\u{2028}b",
            "a\u{2029}b",
            "x\n---\nfoo: bar",
        ] {
            assert_eq!(render_frontmatter_value(value), None, "{value:?}");
        }
    }

    /// The parser does not unescape, so these have no safe quoted form.
    #[test]
    fn render_frontmatter_value_refuses_a_value_it_cannot_quote() {
        for value in ["\"it's\"", "'both\"", "both: 'and\"", "it's: back\\slash"] {
            assert_eq!(render_frontmatter_value(value), None, "{value}");
        }
    }

    #[test]
    fn set_frontmatter_value_replaces_a_line_where_it_stands() {
        let content = "---\nb: 1\na: 2\nc: 3\n---\nBody.\n";
        assert_eq!(
            set_frontmatter_value(content, "a", Some("nine")).unwrap(),
            "---\nb: 1\na: nine\nc: 3\n---\nBody.\n"
        );
    }

    #[test]
    fn set_frontmatter_value_replaces_the_line_the_parser_reads() {
        let content = "---\na: 1\na: 2\n---\n";
        assert_eq!(
            set_frontmatter_value(content, "a", Some("nine")).unwrap(),
            "---\na: 1\na: nine\n---\n"
        );
    }

    #[test]
    fn set_frontmatter_value_adds_a_new_key_at_the_end_of_the_block() {
        assert_eq!(
            set_frontmatter_value("---\na: 1\n---\nBody.\n", "b", Some("two")).unwrap(),
            "---\na: 1\nb: two\n---\nBody.\n"
        );
        assert_eq!(
            set_frontmatter_value("Body.\n", "b", Some("two")).unwrap(),
            "---\nb: two\n---\nBody.\n"
        );
    }

    #[test]
    fn set_frontmatter_value_removes_a_key_and_only_that_key() {
        assert_eq!(
            set_frontmatter_value("---\na: 1\n# a: comment\nb: 2\n---\n", "a", None).unwrap(),
            "---\n# a: comment\nb: 2\n---\n"
        );
        assert_eq!(
            set_frontmatter_value("---\na: 1\nb: 2\n---\n", "b", None).unwrap(),
            "---\na: 1\n---\n"
        );
        assert_eq!(set_frontmatter_value("Body.\n", "a", None).unwrap(), "Body.\n");
    }

    #[test]
    fn set_frontmatter_value_keeps_crlf_line_endings() {
        let content = "---\r\na: 1\r\nb: 2\r\n---\r\nBody.\r\n";
        assert_eq!(
            set_frontmatter_value(content, "a", Some("nine")).unwrap(),
            "---\r\na: nine\r\nb: 2\r\n---\r\nBody.\r\n"
        );
        assert_eq!(
            set_frontmatter_value(content, "c", Some("three")).unwrap(),
            "---\r\na: 1\r\nb: 2\r\nc: three\r\n---\r\nBody.\r\n"
        );
    }

    #[test]
    fn set_frontmatter_value_ignores_nested_keys() {
        let content = "---\nhooks:\n  name: inner\n  tools: nested\nname: planner\n---\n";
        assert_eq!(
            set_frontmatter_value(content, "name", Some("new")).unwrap(),
            "---\nhooks:\n  name: inner\n  tools: nested\nname: new\n---\n"
        );
        let added = set_frontmatter_value(content, "tools", Some("inherit")).unwrap();
        assert!(added.contains("  tools: nested\n"), "{added}");
        assert!(added.ends_with("tools: inherit\n---\n"), "{added}");
        assert_eq!(top_level_frontmatter_value(content, "tools"), None);
        assert_eq!(top_level_frontmatter_value(&added, "tools"), Some("inherit"));
    }

    /// A value over several lines cannot be edited by its first line, or the
    /// rest stays behind as junk in the block.
    #[test]
    fn set_frontmatter_value_refuses_a_multi_line_value() {
        for block in [
            "tools: |\n  Read\n  Grep\n",
            "tools: >-\n  Read\n",
            "tools: |\n",
            "tools:\n  - Read\n  - Grep\n",
            "tools:\n- Read\n- Grep\n",
            "tools: \"Read,\n  Grep\"\n",
            "tools: 'Read,\n  Grep'\n",
            "tools: [Read,\n  Grep]\n",
            "tools: [Read,\nGrep]\n",
            "tools:\n# a comment\n  - Read\n",
            "tools:\n# see: the list\n  - Read\n",
            "tools:\n# one\n\n# two\n  Read\n",
            "tools: [Read,\n",
            "tools: {a: 1,\n",
            "tools: [\"a]\", b,\n",
        ] {
            let content = format!("---\nname: planner\n{block}after: 1\n---\n");
            assert_eq!(
                set_frontmatter_value(&content, "tools", None),
                Err(FrontmatterEditRefusal::MultiLineValue),
                "{block}"
            );
            assert_eq!(
                set_frontmatter_value(&content, "tools", Some("x")),
                Err(FrontmatterEditRefusal::MultiLineValue),
                "{block}"
            );
        }
    }

    /// A closed value followed by a comment and the next key is one line.
    #[test]
    fn set_frontmatter_value_edits_a_value_that_ends_on_its_line() {
        for block in [
            "tools: [Read, Grep]\n",
            "tools: {a: 1}\n",
            "tools: [\"a]\", \"b\"]\n",
            "tools: x\n# a comment\n",
            "tools:\n# a comment\n",
        ] {
            let content = format!("---\nname: planner\n{block}after: 1\n---\n");
            let updated = set_frontmatter_value(&content, "tools", Some("new")).unwrap();
            assert!(updated.contains("tools: new\n"), "{block}");
        }
    }

    #[test]
    fn set_frontmatter_value_handles_an_empty_block() {
        assert_eq!(
            set_frontmatter_value("---\n---\nBody.\n", "a", Some("one")).unwrap(),
            "---\na: one\n---\nBody.\n"
        );
        assert_eq!(
            set_frontmatter_value("---\r\n---\r\nBody.\r\n", "a", Some("one")).unwrap(),
            "---\r\na: one\r\n---\r\nBody.\r\n"
        );
    }

    #[test]
    fn parse_frontmatter_reads_an_empty_block() {
        for content in ["---\n---\nBody.\n", "---\r\n---\r\nBody.\n"] {
            let (frontmatter, body) = parse_frontmatter(content);
            assert!(frontmatter.is_empty());
            assert_eq!(body.trim(), "Body.");
        }
    }

    #[test]
    fn set_frontmatter_value_refuses_a_value_the_gate_refuses() {
        assert_eq!(
            set_frontmatter_value("---\na: 1\n---\n", "a", Some("x\ny: 2")),
            Err(FrontmatterEditRefusal::UnsafeValue)
        );
    }

    #[test]
    fn replace_markdown_body_keeps_the_frontmatter_and_surrounding_whitespace() {
        assert_eq!(
            replace_markdown_body("---\na: 1\n---\n\nOld.\n", "New."),
            "---\na: 1\n---\n\nNew.\n"
        );
        assert_eq!(replace_markdown_body("Old.\n", "New."), "New.\n");
    }

    #[test]
    fn replace_markdown_body_fills_and_empties_a_body() {
        assert_eq!(
            replace_markdown_body("---\na: 1\n---\n", "New."),
            "---\na: 1\n---\nNew.\n"
        );
        assert_eq!(
            replace_markdown_body("---\na: 1\n---\n\nOld.\n", ""),
            "---\na: 1\n---\n"
        );
    }

    #[test]
    fn frontmatter_needs_a_closing_line_of_three_dashes() {
        for content in [
            "---\n-----\nBody\n",
            "---\n---x\nBody\n",
            "---\na: 1\n-----\nBody\n",
            "---\na: 1\n",
            "---\n",
        ] {
            let (frontmatter, body) = parse_frontmatter(content);
            assert!(frontmatter.is_empty(), "{content:?}");
            assert_eq!(body, content);
            assert_eq!(frontmatter_block(content), None, "{content:?}");
        }
    }

    #[test]
    fn frontmatter_closes_at_a_line_break_or_the_end_of_the_text() {
        for content in [
            "---\na: 1\n---\nBody\n",
            "---\na: 1\n---\n",
            "---\na: 1\n---",
            "---\r\na: 1\r\n---\r\nBody\r\n",
            "---\r\na: 1\r\n---",
        ] {
            let (frontmatter, _) = parse_frontmatter(content);
            assert_eq!(frontmatter.get("a").map(String::as_str), Some("1"), "{content:?}");
        }
    }

    /// `---\nname: x\n-----\n---\n` would otherwise read `-----` as the end,
    /// so a later real closing line is what counts.
    #[test]
    fn frontmatter_skips_a_longer_dash_line_to_the_real_closing_line() {
        let content = "---\nname: x\n-----\nmore: 1\n---\nBody\n";
        assert_eq!(frontmatter_block(content), Some("name: x\n-----\nmore: 1"));
    }

    #[test]
    fn render_markdown_command_output_for_lf_is_pinned() {
        assert_eq!(
            render_markdown_command("---\nname: x\n---\nBody\n"),
            "---\nname: x\nautomatic-managed: true\n---\nBody\n"
        );
        assert_eq!(
            render_markdown_command("---\nname: x\n---\nBody"),
            "---\nname: x\nautomatic-managed: true\n---\nBody\n"
        );
        assert_eq!(
            render_markdown_command("Body\n"),
            "---\nautomatic-managed: true\n---\nBody\n"
        );
        let managed = "---\nautomatic-managed: true\n---\nBody\n";
        assert_eq!(render_markdown_command(managed), managed);
    }

    #[test]
    fn render_markdown_command_keeps_a_crlf_file_crlf() {
        for content in [
            "---\r\nname: x\r\n---\r\nOne\r\nTwo\r\n",
            "---\r\nname: x\r\n---\r\nOne",
            "---\r\n---\r\nOne\r\n",
            "One\r\nTwo\r\n",
        ] {
            let rendered = render_markdown_command(content);
            assert!(rendered.contains("automatic-managed: true\r\n"), "{rendered:?}");
            assert!(!rendered.replace("\r\n", "").contains('\n'), "{rendered:?}");
            assert_eq!(parse_frontmatter(&rendered).0["automatic-managed"], "true");
        }
    }

    /// `---x` closes nothing, so the marker must go into a block of its own,
    /// where a reader will find it.
    #[test]
    fn render_markdown_command_does_not_trust_a_false_closing_line() {
        let rendered = render_markdown_command("---\nname: x\n---x\nBody\n");
        assert_eq!(parse_frontmatter(&rendered).0["automatic-managed"], "true");
    }

    /// Sync adds the marker, so a file missing only that loses nothing.
    #[test]
    fn agent_differences_ignore_a_missing_marker() {
        let disk = "---\nname: planner\n---\nPlan.\n";
        let expected = ClaudeCode.convert_agent_content(disk, "planner");
        assert_ne!(disk, expected);
        assert!(ClaudeCode.agent_file_differences(disk, &expected).is_empty());
    }

    #[test]
    fn agent_differences_report_a_body_change_as_content() {
        let expected = ClaudeCode.convert_agent_content("---\nname: planner\n---\nPlan.\n", "planner");
        let disk = expected.replace("Plan.", "Edited.");
        assert_eq!(
            ClaudeCode.agent_file_differences(&disk, &expected),
            vec!["content"]
        );
    }

    #[test]
    fn agent_differences_treat_a_byte_order_mark_as_no_content() {
        let expected = ClaudeCode.convert_agent_content("---\nname: planner\n---\nPlan.\n", "planner");
        let disk = format!("\u{feff}{expected}");
        assert!(ClaudeCode.agent_file_differences(&disk, &expected).is_empty());
    }

    #[test]
    fn agent_differences_report_automatic_managed_false() {
        let expected = ClaudeCode.convert_agent_content("---\nname: planner\n---\nPlan.\n", "planner");
        let disk = expected.replace("automatic-managed: true", "automatic-managed: false");
        assert_eq!(
            ClaudeCode.agent_file_differences(&disk, &expected),
            vec!["automatic-managed"]
        );
    }

    /// Kiro's JSON only ever matches byte for byte.
    #[test]
    fn agent_differences_match_an_identical_kiro_file() {
        let expected = Kiro.convert_agent_content("---\nname: planner\n---\nPlan.\n", "planner");
        assert!(Kiro.agent_file_differences(&expected, &expected).is_empty());
    }

    /// Discovery runs on a timer against cloned repos.  A config that is a
    /// FIFO or a link to `/dev/zero` yields nothing and does not block.
    #[cfg(unix)]
    #[test]
    fn discovery_skips_a_config_that_is_not_a_regular_file() {
        let dir = tempdir().unwrap();
        let fifo = dir.path().join("fifo.json");
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("run mkfifo");
        assert!(status.success());
        let zero = dir.path().join("zero.json");
        std::os::unix::fs::symlink("/dev/zero", &zero).unwrap();

        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let found = [fifo, zero]
                .iter()
                .map(|p| discover_mcp_servers_from_json(p, "mcpServers", |v| v).len())
                .sum::<usize>();
            let _ = tx.send(found);
        });
        let found = rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("discovery blocked");
        assert_eq!(found, 0);
    }

    /// Copy mode must not follow a hub entry that is a link out of the
    /// project: the copy is skipped and the outside file is never read.
    #[cfg(unix)]
    #[test]
    fn copy_mode_skips_a_hub_command_that_is_not_a_regular_file() {
        let project = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let secret = outside.path().join("secret.md");
        fs::write(&secret, "OUTSIDE").unwrap();
        let hub = project.path().join(".agents/commands");
        let agent_dir = project.path().join(".claude/commands");
        fs::create_dir_all(&hub).unwrap();
        fs::create_dir_all(&agent_dir).unwrap();
        std::os::unix::fs::symlink(&secret, hub.join("x.md")).unwrap();

        let root = project.path();
        sync_command_link(&agent_dir.join("x.md"), &hub.join("x.md"), root, false)
            .expect("skip is not an error");
        assert!(!agent_dir.join("x.md").exists());

        fs::create_dir_all(project.path().join(".git")).unwrap();
        fs::write(project.path().join(".git/config"), "[remote]").unwrap();
        std::os::unix::fs::symlink(project.path().join(".git/config"), hub.join("g.md")).unwrap();
        sync_command_link(&agent_dir.join("g.md"), &hub.join("g.md"), root, false)
            .expect("skip is not an error");
        assert!(!agent_dir.join("g.md").exists());

        fs::write(hub.join("y.md"), "inside").unwrap();
        sync_command_link(&agent_dir.join("y.md"), &hub.join("y.md"), root, false)
            .expect("copy");
        assert_eq!(fs::read_to_string(agent_dir.join("y.md")).unwrap(), "inside");
    }

    /// A linked `.agents` directory is caught too, not only a link at the
    /// command file itself.
    #[cfg(unix)]
    #[test]
    fn copy_mode_skips_a_hub_reached_through_a_linked_parent() {
        let project = tempdir().unwrap();
        let outside = tempdir().unwrap();
        fs::create_dir_all(outside.path().join("commands")).unwrap();
        fs::write(outside.path().join("commands/x.md"), "OUTSIDE").unwrap();
        std::os::unix::fs::symlink(outside.path(), project.path().join(".agents")).unwrap();
        let agent_dir = project.path().join(".claude/commands");
        fs::create_dir_all(&agent_dir).unwrap();

        sync_command_link(
            &agent_dir.join("x.md"),
            &project.path().join(".agents/commands/x.md"),
            project.path(),
            false,
        )
        .expect("skip is not an error");
        assert!(!agent_dir.join("x.md").exists());
    }

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
        assert_eq!(events.len(), 12);
        assert!(events.iter().any(|v| v == "SubagentStop"));
        assert!(events.iter().any(|v| v == "Interrupt"));

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
    fn raw_entry_names_helper_keeps_proxy_stubs_but_still_skips_automatic() {
        // Sibling of `discover_skips_proxy_stub_but_keeps_real_servers`: the
        // drift/status path needs to see the stub as a present entry so that a
        // managed remote OAuth server does not report as permanently missing,
        // but must still skip `automatic`/`nexus`/invalid names (never
        // "foreign" — they are reserved).
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join(".mcp.json");
        fs::write(
            &path,
            serde_json::to_string_pretty(&serde_json::json!({
                "mcpServers": {
                    "automatic": { "command": "automatic", "args": ["mcp-serve"] },
                    "linear": { "command": "/usr/local/bin/automatic", "args": ["mcp-proxy", "linear"] },
                    "fetch": { "command": "npx", "args": ["-y", "fetch-mcp"] },
                }
            }))
            .expect("serialize"),
        )
        .expect("write .mcp.json");

        let names = read_global_mcp_entry_names_json(&path, "mcpServers");

        assert!(
            !names.contains("automatic"),
            "reserved name 'automatic' must still be skipped"
        );
        assert!(
            names.contains("linear"),
            "proxy stub must count toward the on-disk name set for drift purposes"
        );
        assert!(names.contains("fetch"), "real servers still present");
    }

    #[test]
    fn discover_at_nested_key_path_reads_servers_and_applies_the_same_filters() {
        // Z Code's CLI config nests servers under `mcp.servers`.  The nested
        // walker must find them and apply the same automatic/proxy-stub
        // filtering as the single-key variant.
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("config.json");
        fs::write(
            &path,
            serde_json::to_string_pretty(&serde_json::json!({
                "theme": "dark",
                "mcp": {
                    "servers": {
                        "automatic": { "command": "automatic", "args": ["mcp-serve"] },
                        "memory": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-memory"] },
                    }
                }
            }))
            .expect("serialize"),
        )
        .expect("write config.json");

        let discovered = discover_mcp_servers_from_json_at(&path, &["mcp", "servers"], |v| v);
        assert_eq!(discovered.len(), 1);
        assert!(discovered.contains_key("memory"));

        // A missing intermediate key yields an empty map, not a panic.
        assert!(discover_mcp_servers_from_json_at(&path, &["mcp", "missing"], |v| v).is_empty());
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
            dir.path(),
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
            dir.path(),
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
            dir.path(),
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

        let written = copy_commands_to_project(dir.path(), &commands_dir, &[], &[], &HashSet::new())
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
            dir.path(),
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
            dir.path(),
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

    #[test]
    fn skill_writers_refuse_a_traversal_name_and_change_nothing() {
        let dir = tempdir().expect("tempdir");
        let project = dir.path().join("project");
        let hub = project.join(".agents").join("skills");
        let agent_dir = project.join(".claude").join("skills");
        fs::create_dir_all(hub.join("stale-skill")).expect("create stale skill");
        let skills = [("../../../evil".to_string(), "# Evil\n".to_string())];
        let selected = ["../../../evil".to_string()];

        let mut written = Vec::new();
        let err = copy_skills_to_project(&hub, &skills, &selected, &[], &mut written)
            .expect_err("a traversal name must be refused");
        assert!(err.contains("Skill"), "{err}");
        let err =
            symlink_skills_from_project(&agent_dir, &hub, &skills, &selected, &[], &mut written)
                .expect_err("a traversal name must be refused");
        assert!(err.contains("Skill"), "{err}");
        let err = sync_individual_skills(&agent_dir, &skills, &selected, &[], &mut written)
            .expect_err("a traversal name must be refused");
        assert!(err.contains("Skill"), "{err}");

        assert!(written.is_empty());
        assert!(!dir.path().join("evil").exists());
        assert!(hub.join("stale-skill").exists(), "cleanup did not run");
        assert!(!agent_dir.exists());
    }

    #[test]
    fn command_writers_refuse_a_traversal_name_and_write_nothing() {
        let dir = tempdir().expect("tempdir");
        let project = dir.path().join("project");
        let hub = project.join(".agents").join("commands");
        let agent_dir = project.join(".claude").join("commands");
        let custom = [crate::core::CustomCommand {
            name: "../../../.claude/agents/evil".to_string(),
            content: "Escape.\n".to_string(),
        }];

        let err = copy_commands_to_project(&project, &hub, &[], &custom, &HashSet::new())
            .expect_err("a traversal name must be refused");
        assert!(err.contains("Custom command"), "{err}");
        let err =
            sync_commands_to_dir(&project, &agent_dir, &[], &custom, &ClaudeCode, &HashSet::new())
                .expect_err("a traversal name must be refused");
        assert!(err.contains("Custom command"), "{err}");
        let err =
            symlink_commands_from_project(&project, &agent_dir, &hub, &[], &custom, &ClaudeCode)
                .expect_err("a traversal name must be refused");
        assert!(err.contains("Custom command"), "{err}");

        assert!(!hub.exists());
        assert!(!agent_dir.exists());
        assert!(!dir.path().join(".claude").exists());
    }

    /// Case (c): Gemini's `.toml` commands are written through the contained
    /// writer, so a committed link at a command's name is skipped.
    #[cfg(unix)]
    #[test]
    fn sync_commands_to_dir_skips_a_command_that_links_out_of_the_project() {
        let project = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let secret = outside.path().join("secret");
        fs::write(&secret, "OUTSIDE").unwrap();
        let commands_dir = GeminiCli.commands_dir(project.path()).expect("gemini commands dir");
        fs::create_dir_all(&commands_dir).unwrap();
        std::os::unix::fs::symlink(&secret, commands_dir.join("deploy.toml")).unwrap();
        let commands = [
            CustomCommand {
                name: "deploy".into(),
                content: "Ship it.\n".into(),
            },
            CustomCommand {
                name: "review".into(),
                content: "Look hard.\n".into(),
            },
        ];

        let written = sync_commands_to_dir(
            project.path(),
            &commands_dir,
            &[],
            &commands,
            &GeminiCli,
            &HashSet::new(),
        )
        .expect("sync");

        assert_eq!(fs::read_to_string(&secret).unwrap(), "OUTSIDE");
        assert!(commands_dir.join("review.toml").is_file());
        assert!(!written.iter().any(|p| p.ends_with("deploy.toml")), "{written:?}");
    }

    /// Case (d): copy mode checks the hub against the root it is given, the
    /// directory being synced, not the hub's grandparent.  A hub reached
    /// through a link out of that root is not copied.
    #[cfg(unix)]
    #[test]
    fn copy_mode_checks_the_hub_against_the_given_root() {
        let project = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let silent = project.path().join(".automatic").join("silent");
        fs::create_dir_all(&silent).unwrap();
        // The hub's grandparent is `outside`, which contains it.
        let hub = outside.path().join(".agents").join("commands");
        fs::create_dir_all(&hub).unwrap();
        fs::write(hub.join("x.md"), "OUTSIDE").unwrap();
        std::os::unix::fs::symlink(outside.path().join(".agents"), silent.join(".agents")).unwrap();
        let agent_dir = silent.join(".claude").join("commands");
        fs::create_dir_all(&agent_dir).unwrap();

        sync_command_link(
            &agent_dir.join("x.md"),
            &silent.join(".agents/commands/x.md"),
            &silent,
            false,
        )
        .expect("skip is not an error");

        assert!(!agent_dir.join("x.md").exists());
    }

    /// Names come from the committed config and from frontmatter.  A name
    /// that is not one plain component never reaches a remove or a write,
    /// so `../..` cannot delete the project's parent and an absolute name
    /// cannot replace an outside file.  The writers refuse such a name with
    /// an error before they touch the disk.
    #[cfg(unix)]
    #[test]
    fn unsafe_skill_and_command_names_are_refused() {
        let base = tempdir().unwrap();
        let project = base.path().join("project");
        let skills_dir = project.join(".agents").join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        // `<name>.md` for the absolute name below is exactly this file.
        let precious = base.path().join("precious.md");
        fs::write(&precious, "KEEP").unwrap();

        let mut written = Vec::new();
        copy_skills_to_project(
            &skills_dir,
            &[("../..".to_string(), "x".to_string())],
            &["../..".to_string()],
            &[],
            &mut written,
        )
        .expect_err("an unsafe name is refused");
        assert!(precious.exists(), "the project's parent must survive");
        assert!(written.is_empty());

        let agent_dir = project.join(".claude").join("commands");
        let hub = project.join(".agents").join("commands");
        fs::create_dir_all(&hub).unwrap();
        let absolute = precious.with_extension("");
        let commands = [CustomCommand {
            name: absolute.display().to_string(),
            content: "x".into(),
        }];
        symlink_commands_from_project(
            &project,
            &agent_dir,
            &hub,
            &[],
            &commands,
            &ClaudeCode,
        )
        .expect_err("an unsafe name is refused");
        assert_eq!(fs::read_to_string(&precious).unwrap(), "KEEP");
        assert!(is_plain_file_name("deploy") && !is_plain_file_name("a/b"));

        // Drift's tempdir writer: an absolute name must not escape it.
        let escape = base.path().join("escape");
        let mut written = Vec::new();
        sync_individual_skills(
            &skills_dir,
            &[(escape.display().to_string(), "x".to_string())],
            &[],
            &[],
            &mut written,
        )
        .expect_err("an unsafe name is refused");
        assert!(!escape.exists());
    }
}
