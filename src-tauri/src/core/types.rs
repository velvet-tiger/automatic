use serde::{Deserialize, Serialize};

// ── Data Structures ──────────────────────────────────────────────────────────

/// Remote origin of a skill imported from skills.sh.
/// Stored in ~/.automatic/skills.json keyed by skill name.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkillSource {
    /// GitHub owner/repo, e.g. "vercel-labs/skills"
    pub source: String,
    /// Full skills.sh id, e.g. "vercel-labs/skills/find-skills"
    pub id: String,
}

/// A skill entry with its name and which global directories it exists in.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkillEntry {
    pub name: String,
    /// Exists in `~/.agents/skills/` (agentskills.io standard)
    pub in_agents: bool,
    /// Exists in `~/.claude/skills/` (Claude Code)
    pub in_claude: bool,
    /// Remote origin from ~/.automatic/skills.json, if this was imported from skills.sh
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SkillSource>,
    /// True if the skill directory contains any files or subdirectories besides SKILL.md
    #[serde(default)]
    pub has_resources: bool,
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
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    /// Clerk user ID of the user who created this project.  Populated by the
    /// frontend from the useProfile hook.  Used for future team/cloud sync.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    /// Rules attached to each project instruction file.  Maps filename
    /// (e.g. "CLAUDE.md") to an ordered list of rule names whose content is
    /// appended below the user-authored content when the file is written.
    /// In unified mode the key `"_unified"` is used for all files.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub file_rules: std::collections::HashMap<String, Vec<String>>,
    /// `"unified"` — one set of instructions written to all agent files.
    /// `"per-agent"` (default) — each agent file is edited independently.
    #[serde(default = "default_instruction_mode")]
    pub instruction_mode: String,
}

fn default_instruction_mode() -> String {
    "per-agent".to_string()
}
