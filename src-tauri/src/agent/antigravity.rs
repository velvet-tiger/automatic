use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, Agent};

/// Google Antigravity agent — stores skills under
/// `<project>/.agents/skills/<name>/SKILL.md`.
///
/// An Antigravity CLI now exists alongside the IDE and shares the same
/// harness and config (both the global MCP config below and, per Google's
/// own docs, the instruction file) — one `Antigravity` implementation
/// correctly covers both.
///
/// ## Project instructions
///
/// Antigravity reads `GEMINI.md` at the project root — it does **not** treat
/// `AGENTS.md` as special (confirmed via community testing).  Global
/// instructions also live in `~/.gemini/GEMINI.md`, shared with Gemini CLI.
///
/// In addition, Antigravity has a rules system: individual Markdown files in
/// `.agents/rules/` (workspace) activated manually, always-on, by model
/// decision, or by glob pattern.  Rule syncing is not currently supported by
/// Automatic.
///
/// Note: `.agent/rules/` is retained for backward compatibility.
///
/// ## Skills
///
/// Workspace skills: `<project>/.agents/skills/<name>/SKILL.md` ✓
/// Global (user-installed) skills: `~/.gemini/config/skills/<name>/SKILL.md`
///                   (not synced by Automatic — managed globally)
/// Built-in skills ship from `~/.gemini/antigravity/builtin/skills/` (IDE)
/// and `~/.gemini/antigravity-cli/builtin/skills/` (CLI) — Automatic never
/// touches either.
///
/// Note: `.agent/skills/` is retained for backward compatibility.
///
/// ## MCP config
///
/// Antigravity manages MCP servers globally through its own UI:
/// Agent session → "…" → MCP Servers → Manage MCP Servers → View raw config.
/// There is still no project-scoped MCP config file, so `write_mcp_config`
/// stays a no-op and `mcp_note` stays accurate — but the global file's path
/// is now documented and read-only discovery from it is implemented:
/// `~/.gemini/config/mcp_config.json`, shared by the IDE and the CLI.
///
/// Format uses `mcpServers` with standard stdio entries (no explicit `type`):
/// ```json
/// { "mcpServers": { "my-server": { "command": "npx", "args": ["-y", "..."] } } }
/// ```
///
/// One documented caveat: environment variable interpolation does not work
/// in that file, so any values in it are hardcoded. Automatic must not write
/// it even once a project-scoped path exists to write to — only read from it.
pub struct Antigravity;

impl Agent for Antigravity {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "antigravity"
    }

    fn label(&self) -> &'static str {
        "Antigravity (Beta)"
    }

    fn config_description(&self) -> &'static str {
        "GEMINI.md (MCP configured via Antigravity UI)"
    }

    fn project_file_name(&self) -> &'static str {
        // Antigravity reads GEMINI.md (confirmed via community testing —
        // it does NOT treat AGENTS.md as special, despite the open standard).
        // Global rules also live in ~/.gemini/GEMINI.md, shared with Gemini CLI.
        "GEMINI.md"
    }

    // ── Capabilities ────────────────────────────────────────────────────

    fn capabilities(&self) -> super::AgentCapabilities {
        super::AgentCapabilities {
            // MCP config is global, managed via the Antigravity UI.
            mcp_servers: false,
            agents: false,
            ..Default::default()
        }
    }

    // ── MCP note ────────────────────────────────────────────────────────

    fn mcp_note(&self) -> Option<&'static str> {
        Some(
            "Antigravity manages MCP servers via its own UI: Agent session \u{2192} \u{22ef} \u{2192} \
             MCP Servers \u{2192} Manage MCP Servers \u{2192} View raw config. \
             Automatic cannot write Antigravity\u{2019}s mcp_config.json.",
        )
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_in(&self, dir: &Path) -> bool {
        // GEMINI.md is shared with Gemini CLI so we cannot use it as a sole
        // marker — a Gemini-specific indicator must also be present.
        // The .antigravity/ directory is created by the Antigravity app itself
        // and is the most reliable project-level signal.
        dir.join(".antigravity").is_dir()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".agents").join("skills")]
    }

    // ── Cleanup ─────────────────────────────────────────────────────────

    // No project-level MCP config files to clean up.
    // owned_config_paths defaults to empty vec, which is correct here.

    // ── Config writing ──────────────────────────────────────────────────

    /// Antigravity has no project-level MCP config file.
    /// This is intentionally a no-op; servers must be added through the
    /// Antigravity MCP Servers panel.
    fn write_mcp_config(
        &self,
        _dir: &Path,
        _servers: &Map<String, Value>,
    ) -> Result<String, String> {
        Ok(String::new())
    }

    // ── Discovery ───────────────────────────────────────────────────────

    /// No project-level MCP config file to discover from.
    fn discover_mcp_servers(&self, _dir: &Path) -> Map<String, Value> {
        Map::new()
    }

    fn detect_global_install(&self) -> bool {
        std::path::Path::new("/Applications/Antigravity.app").exists()
    }

    fn discover_global_mcp_servers(&self) -> Map<String, Value> {
        let Some(home) = super::home_dir() else {
            return Map::new();
        };
        // ~/.gemini/config/mcp_config.json — shared by the Antigravity IDE
        // and the Antigravity CLI.
        let path = home.join(".gemini").join("config").join("mcp_config.json");
        discover_mcp_servers_from_json(&path, "mcpServers", identity)
    }
}

/// Pass-through normaliser: Antigravity's format is already canonical.
fn identity(v: Value) -> Value {
    v
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_detect_on_antigravity_dir() {
        let dir = tempdir().unwrap();
        assert!(!Antigravity.detect_in(dir.path()));

        // The .antigravity/ directory is created by the Antigravity app itself.
        fs::create_dir_all(dir.path().join(".antigravity")).unwrap();
        assert!(Antigravity.detect_in(dir.path()));
    }

    #[test]
    fn test_mcp_capability_disabled() {
        assert!(!Antigravity.capabilities().mcp_servers);
    }

    #[test]
    fn test_mcp_note_is_some() {
        assert!(Antigravity.mcp_note().is_some());
    }

    #[test]
    fn test_write_mcp_config_is_noop() {
        let dir = tempdir().unwrap();
        let mut servers = Map::new();
        servers.insert(
            "sequential-thinking".to_string(),
            json!({"command": "npx", "args": ["-y", "@modelcontextprotocol/server-sequential-thinking"]}),
        );

        let result = Antigravity.write_mcp_config(dir.path(), &servers);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");

        let entries: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
        assert!(entries.is_empty(), "no files should be written");
    }

    #[test]
    fn test_discover_mcp_servers_always_empty() {
        let dir = tempdir().unwrap();
        assert!(Antigravity.discover_mcp_servers(dir.path()).is_empty());
    }

    #[test]
    fn test_skill_sync() {
        let dir = tempdir().unwrap();
        let skills = vec![("my-skill".to_string(), "# My Skill\n".to_string())];
        let selected = vec!["my-skill".to_string()];

        let written = Antigravity
            .sync_skills(dir.path(), &skills, &selected, &[])
            .unwrap();
        assert_eq!(written.len(), 1);

        let content =
            fs::read_to_string(dir.path().join(".agents/skills/my-skill/SKILL.md")).unwrap();
        assert_eq!(content, "# My Skill\n");
    }
}
