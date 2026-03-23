use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Data Structures ──────────────────────────────────────────────────────────

// ── skill.json (velvet-tiger/skills-json spec) ───────────────────────────────

/// Publisher-side package metadata for an AI agent skill package.
/// Lives at the root of a skill repo as `skill.json`.
/// Spec: https://github.com/velvet-tiger/skills-json
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SkillsJson {
    /// URL to the JSON Schema (optional, for validation tooling).
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    /// Package identifier. Lowercase, hyphens allowed.
    pub name: String,
    /// Semver version.
    pub version: String,
    /// One-line package summary.
    pub description: String,
    /// Package author.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<SkillsJsonAuthor>,
    /// SPDX license identifier. Inherited by skills unless overridden.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Source repository info.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<SkillsJsonRepository>,
    /// Documentation URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// Package-level search terms.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    /// Array of skill entries (minimum 1).
    pub skills: Vec<SkillsJsonSkill>,
}

/// Author info — object form (npm shorthand strings are not supported here,
/// use the object form for serialization).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkillsJsonAuthor {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

/// Repository info.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkillsJsonRepository {
    #[serde(rename = "type")]
    pub repo_type: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,
}

/// A single skill entry within a `skill.json` package.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkillsJsonSkill {
    /// Unique skill identifier. Should match directory name.
    pub name: String,
    /// Relative path from skill.json to the skill directory.
    pub path: String,
    /// What this skill does and when to use it.
    pub description: String,
    /// Skill-specific version override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// SRI hash of skill directory contents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity: Option<String>,
    /// Main instruction file relative to path. Defaults to "SKILL.md".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entrypoint: Option<String>,
    /// Primary category for organisation and filtering.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Search and filter tags.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// SPDX license override for this skill.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Compatibility constraints.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires: Option<SkillsJsonRequires>,
    /// Other skill names within this package that must also be installed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
}

impl SkillsJsonSkill {
    /// Resolve the entrypoint filename. Returns "SKILL.md" when not specified.
    pub fn entrypoint_file(&self) -> &str {
        self.entrypoint.as_deref().unwrap_or("SKILL.md")
    }
}

/// Compatibility constraints for a skill.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SkillsJsonRequires {
    /// Required tool availability (e.g. ["python3", "node"]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    /// External skill dependencies ("package/skill-name" format).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
    /// Minimum agent versions keyed by agent slug.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub min_agent_versions: HashMap<String, String>,
}

/// Remote origin of a skill imported from skills.sh, or the bundled origin
/// for skills shipped with the app.
/// Stored in ~/.automatic/skills.json keyed by skill name.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkillSource {
    /// GitHub owner/repo, e.g. "vercel-labs/skills".
    /// For bundled skills this is "automatic/automatic-app".
    pub source: String,
    /// Full skills.sh id, e.g. "vercel-labs/skills/find-skills".
    /// For bundled skills this is "automatic/automatic-app/<name>".
    pub id: String,
    /// "github" for registry-imported skills; "bundled" for skills shipped
    /// with the Automatic app.  Defaults to "github" when absent so existing
    /// registry entries are not broken.
    #[serde(default = "default_skill_source_kind")]
    pub kind: String,
}

fn default_skill_source_kind() -> String {
    "github".to_string()
}

// ── Agent Options ─────────────────────────────────────────────────────────────

/// Per-agent configuration options stored in a project.
///
/// Each agent has a corresponding `AgentOptions` entry that can override
/// default sync behaviour for that specific agent within a project.
/// Fields default to the recommended value so that existing projects without
/// this struct in their JSON behave identically to newly created ones.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentOptions {
    /// **Claude Code only.**  When `true` (the default), rules are written as
    /// individual Markdown files under `.claude/rules/` rather than being
    /// injected as an `<!-- automatic:rules:start/end -->` block inside
    /// `CLAUDE.md`.  This matches the format recommended by the Claude Code
    /// documentation and is easier for Claude to discover and follow.
    ///
    /// Set to `false` to revert to the legacy inline-injection behaviour.
    #[serde(default = "default_true")]
    pub claude_rules_in_dot_claude: bool,
}

impl Default for AgentOptions {
    fn default() -> Self {
        Self {
            claude_rules_in_dot_claude: true,
        }
    }
}

fn default_true() -> bool {
    true
}

/// A skill entry with its name and which global directories it exists in.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkillEntry {
    pub name: String,
    /// Which global sources contain this skill: e.g., ["agents", "claude", "codex", "cline"]
    /// "agents" refers to ~/.agents/skills/; other values match agent IDs.
    #[serde(default)]
    pub sources: Vec<String>,
    /// Remote origin from ~/.automatic/skills.json, if this was imported from skills.sh
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SkillSource>,
    /// True if the skill directory contains any files or subdirectories besides SKILL.md
    #[serde(default)]
    pub has_resources: bool,
    /// License from the SKILL.md frontmatter `license:` field, if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Project {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub directory: String,
    #[serde(default)]
    pub skills: Vec<String>,
    /// Skills that exist only in the project directory, not in the global
    /// registry.  Discovered during autodetection but never auto-imported.
    #[serde(default)]
    pub local_skills: Vec<String>,
    #[serde(default)]
    pub mcp_servers: Vec<String>,
    #[serde(default)]
    pub providers: Vec<String>,
    #[serde(default)]
    pub agents: Vec<String>,
    /// Tool names assigned to this project. Tool definitions live in
    /// `~/.automatic/tools/`. Populated by autodetection or manual addition.
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    /// Most recent project activity timestamp (ISO 8601 UTC). Updated whenever
    /// an activity row is appended for this project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activity: Option<String>,
    /// Clerk user ID of the user who created this project.  Populated by the
    /// frontend from the useProfile hook.  Used for future team/cloud sync.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    /// Rules attached to each project instruction file.  Maps filename
    /// (e.g. "CLAUDE.md") to an ordered list of rule names whose content is
    /// appended below the user-authored content when the file is written.
    /// In unified mode the key `"_unified"` is used for all files.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub file_rules: HashMap<String, Vec<String>>,
    /// `"unified"` — one set of instructions written to all agent files.
    /// `"per-agent"` (default) — each agent file is edited independently.
    #[serde(default = "default_instruction_mode")]
    pub instruction_mode: String,
    /// Per-agent configuration options keyed by agent id (e.g. `"claude"`).
    /// Agents not present in this map use their `AgentOptions::default()`.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub agent_options: HashMap<String, AgentOptions>,
    /// Hash of the full content Automatic last wrote to each instruction file.
    /// Maps filename (e.g. `"AGENTS.md"`) to a hex-encoded hash.  Used by
    /// drift detection to identify files that were modified externally.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub instruction_file_hashes: HashMap<String, String>,
    /// Inline custom rules stored directly in the project (not in the global
    /// rule registry). These are injected into instruction files in the same
    /// way as global rules, but are scoped to this project only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_rules: Vec<CustomRule>,
    /// Workspace agent names selected for this project. These are written
    /// to the agent's sub-agent directory (e.g. `.claude/agents/`) on sync.
    /// Agent machine names reference files in `~/.automatic/agents/`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_agents: Vec<String>,
    /// Inline custom sub-agents stored directly in the project configuration.
    /// These are written to each agent's sub-agent directory (e.g.
    /// `.claude/agents/`) during sync. Unlike workspace user_agents, custom
    /// agents are project-scoped and travel with the project JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_agents: Option<Vec<CustomAgent>>,
}

/// An inline rule stored directly inside a project configuration.
/// Unlike global rules (which live in `~/.automatic/rules/`), custom rules
/// are project-scoped and travel with the project JSON.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CustomRule {
    /// Short human-readable name (shown in the UI).
    pub name: String,
    /// Markdown content that will be injected into instruction files.
    pub content: String,
}

/// A named group that relates two or more projects to each other.
///
/// When a project is synced, Automatic looks up all groups that contain it
/// and injects a short context block into each agent instruction file (after
/// the user content but before the rules section) so that the agent knows
/// about related projects and their locations.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ProjectGroup {
    /// Unique identifier and display name for the group.
    pub name: String,
    /// Optional one-line summary shown in the UI and injected into
    /// instruction files as the group description.
    #[serde(default)]
    pub description: String,
    /// Ordered list of project names belonging to this group.
    #[serde(default)]
    pub projects: Vec<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

fn default_instruction_mode() -> String {
    "per-agent".to_string()
}

/// A user-defined sub-agent stored directly in a project configuration.
/// Unlike global user agents (which live in `~/.automatic/agents/`),
/// custom agents are project-scoped and travel with the project JSON.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CustomAgent {
    /// Human-readable display name (from frontmatter `name` field).
    pub name: String,
    /// Full Markdown content including frontmatter.
    pub content: String,
}

/// A lightweight reference to a project: name + directory.
/// Used when listing projects that reference a rule or agent.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProjectRef {
    pub name: String,
    pub directory: String,
}
