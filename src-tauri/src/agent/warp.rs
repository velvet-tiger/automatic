use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

use super::{sync_individual_skills, Agent};

/// Warp agent — uses `AGENTS.md` as the project rules file and stores
/// skills under `<project>/.agents/skills/<name>/SKILL.md`.
///
/// **MCP note**: Warp manages MCP servers through its own GUI and SQLite
/// database — there is no project-level config file that Automatic can write.
/// MCP servers must be configured manually inside the Warp app
/// (Settings > MCP Servers).
pub struct Warp;

impl Agent for Warp {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "warp"
    }

    fn label(&self) -> &'static str {
        "Warp"
    }

    fn config_description(&self) -> &'static str {
        "AGENTS.md (MCP configured in Warp app)"
    }

    fn project_file_name(&self) -> &'static str {
        "AGENTS.md"
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_in(&self, dir: &Path) -> bool {
        // AGENTS.md is shared with Codex CLI — only count it as Warp when
        // a Warp-specific marker is also present.
        dir.join("WARP.md").exists() || dir.join(".warp").is_dir()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        // Warp reads from `.agents/skills/` (recommended) and `.warp/skills/`.
        // We sync to the standard location; Warp picks it up automatically.
        vec![dir.join(".agents").join("skills")]
    }

    // ── MCP note ────────────────────────────────────────────────────────

    fn mcp_note(&self) -> Option<&'static str> {
        Some(
            "Warp manages MCP servers through its own app (Settings \u{203a} MCP Servers). \
             Automatic cannot write Warp's MCP config — add servers manually in Warp.",
        )
    }

    // ── Config writing ──────────────────────────────────────────────────

    /// Warp does not expose a writable project-level MCP config file.
    /// This is intentionally a no-op; MCP servers must be added manually
    /// inside the Warp app.
    fn write_mcp_config(
        &self,
        _dir: &Path,
        _servers: &Map<String, Value>,
    ) -> Result<String, String> {
        Ok(String::new())
    }

    fn sync_skills(
        &self,
        dir: &Path,
        skill_contents: &[(String, String)],
        selected_names: &[String],
        local_skill_names: &[String],
    ) -> Result<Vec<String>, String> {
        let mut written = Vec::new();
        let skills_dir = dir.join(".agents").join("skills");
        sync_individual_skills(
            &skills_dir,
            skill_contents,
            selected_names,
            local_skill_names,
            &mut written,
        )?;
        Ok(written)
    }

    // ── Discovery ───────────────────────────────────────────────────────

    /// Warp stores MCP config in its app database — nothing discoverable
    /// from the project directory.
    fn discover_mcp_servers(&self, _dir: &Path) -> Map<String, Value> {
        Map::new()
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_detect_warp_md() {
        let dir = tempdir().unwrap();
        assert!(!Warp.detect_in(dir.path()));

        fs::write(dir.path().join("WARP.md"), "").unwrap();
        assert!(Warp.detect_in(dir.path()));
    }

    #[test]
    fn test_detect_warp_dir() {
        let dir = tempdir().unwrap();
        assert!(!Warp.detect_in(dir.path()));

        fs::create_dir(dir.path().join(".warp")).unwrap();
        assert!(Warp.detect_in(dir.path()));
    }

    #[test]
    fn test_write_mcp_config_is_noop() {
        let dir = tempdir().unwrap();
        let mut servers = Map::new();
        servers.insert(
            "github".to_string(),
            serde_json::json!({"command": "npx", "args": ["@modelcontextprotocol/server-github"]}),
        );

        let result = Warp.write_mcp_config(dir.path(), &servers);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");

        // No files should have been written
        let entries: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_mcp_note_is_some() {
        assert!(Warp.mcp_note().is_some());
    }

    #[test]
    fn test_skill_sync() {
        let dir = tempdir().unwrap();
        let skills = vec![("my-skill".to_string(), "# My Skill\n".to_string())];
        let selected = vec!["my-skill".to_string()];

        let written = Warp
            .sync_skills(dir.path(), &skills, &selected, &[])
            .unwrap();
        assert_eq!(written.len(), 1);
        assert!(written[0].contains("my-skill"));

        let content =
            fs::read_to_string(dir.path().join(".agents/skills/my-skill/SKILL.md")).unwrap();
        assert_eq!(content, "# My Skill\n");
    }
}
