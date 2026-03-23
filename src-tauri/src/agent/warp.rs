use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

use super::{sync_individual_skills, Agent, AgentCapabilities};

/// Warp agent — uses `AGENTS.md` as the project rules file and stores
/// skills under `<project>/.agents/skills/<name>/SKILL.md`.
///
/// Warp migrated from `WARP.md` to `AGENTS.md` as the canonical project rules
/// filename (the old name is still supported for backwards compatibility, but
/// new projects should use `AGENTS.md`).  Detection still matches `.warp/`
/// directories and legacy `WARP.md` files so that existing projects continue to
/// be recognised.
///
/// **MCP note**: Warp manages MCP servers through its own GUI and internal
/// database — there is no project-level config file that Automatic can write.
/// MCP servers must be configured manually inside the Warp app
/// (Settings › MCP Servers or Warp Drive › MCP Servers).
pub struct Warp;

impl Agent for Warp {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "warp"
    }

    fn label(&self) -> &'static str {
        "Warp (Beta)"
    }

    fn config_description(&self) -> &'static str {
        "AGENTS.md (MCP configured in Warp app)"
    }

    fn project_file_name(&self) -> &'static str {
        // Warp's canonical project rules file is now AGENTS.md.
        // WARP.md is still recognised for backwards compatibility.
        "AGENTS.md"
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_global_install(&self) -> bool {
        // Warp ships as a macOS app bundle. Also check for the ~/.warp/
        // config directory as a fallback for non-standard installs.
        std::path::Path::new("/Applications/Warp.app").exists()
            || super::home_dir()
                .map(|h| h.join(".warp").exists())
                .unwrap_or(false)
    }

    fn detect_in(&self, dir: &Path) -> bool {
        // Detect via the `.warp/` directory or legacy `WARP.md`.
        // We do NOT match on AGENTS.md alone because that is shared with
        // Codex CLI and many other agents — a Warp-specific marker must
        // also be present to avoid false positives.
        dir.join("WARP.md").exists() || dir.join(".warp").is_dir()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        // Warp reads from `.agents/skills/` (recommended) and `.warp/skills/`.
        // We sync to the standard location; Warp picks it up automatically.
        vec![dir.join(".agents").join("skills")]
    }

    // ── Capabilities ────────────────────────────────────────────────────

    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            mcp_servers: false,
            agents: false,
            ..Default::default()
        }
    }

    // ── MCP note ────────────────────────────────────────────────────────

    fn mcp_note(&self) -> Option<&'static str> {
        Some(
            "Warp manages MCP servers through its own app (Settings \u{203a} AI \u{203a} MCP Servers \
             or Warp Drive \u{203a} MCP Servers). Automatic cannot write Warp\u{2019}s MCP config \
             \u{2014} add servers manually in Warp.",
        )
    }

    // ── Cleanup ─────────────────────────────────────────────────────────

    /// Return both the current canonical file (`AGENTS.md`) and the legacy
    /// file (`WARP.md`) so that either variant is cleaned up when Warp is
    /// removed from a project.  Only paths that actually exist on disk are
    /// acted on by the default `cleanup_mcp_config` implementation.
    fn owned_config_paths(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join("AGENTS.md"), dir.join("WARP.md")]
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
    fn test_owned_config_paths_includes_both_files() {
        let dir = tempdir().unwrap();
        let paths = Warp.owned_config_paths(dir.path());
        // Both the canonical AGENTS.md and legacy WARP.md are included.
        assert!(paths.contains(&dir.path().join("AGENTS.md")));
        assert!(paths.contains(&dir.path().join("WARP.md")));
    }

    #[test]
    fn test_cleanup_removes_agents_md() {
        let dir = tempdir().unwrap();
        let agents_md = dir.path().join("AGENTS.md");
        fs::write(&agents_md, "# Warp context\n").unwrap();
        assert!(agents_md.exists());

        // cleanup_mcp_config uses the default impl which deletes owned_config_paths
        // that exist on disk.  Only AGENTS.md exists here.
        use super::super::Agent as _;
        let removed = Warp.cleanup_mcp_config(dir.path());
        assert_eq!(removed, vec![agents_md.display().to_string()]);
        assert!(!agents_md.exists(), "AGENTS.md should have been deleted");
    }

    #[test]
    fn test_cleanup_removes_warp_md_legacy() {
        let dir = tempdir().unwrap();
        let warp_md = dir.path().join("WARP.md");
        fs::write(&warp_md, "# Warp context\n").unwrap();
        assert!(warp_md.exists());

        use super::super::Agent as _;
        let removed = Warp.cleanup_mcp_config(dir.path());
        assert_eq!(removed, vec![warp_md.display().to_string()]);
        assert!(!warp_md.exists(), "WARP.md should have been deleted");
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
