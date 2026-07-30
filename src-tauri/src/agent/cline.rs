use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, Agent};

/// Cline agent — stores project skills under
/// `<project>/.cline/skills/<name>/SKILL.md`.
///
/// Cline's current project-level rules live under `.clinerules/` as Markdown
/// files. Automatic manages a single file inside that directory.
/// MCP settings for the CLI live in global CLI state under
/// `~/.cline/data/settings/` (or `$CLINE_DIR/data/settings/` when overridden),
/// so Automatic can discover them but does not sync them per project.
pub struct Cline;

impl Agent for Cline {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "cline"
    }

    fn label(&self) -> &'static str {
        "Cline (Beta)"
    }

    fn config_description(&self) -> &'static str {
        "Global CLI settings (~/.cline/data/settings/cline_mcp_settings.json)"
    }

    fn project_file_name(&self) -> &'static str {
        ".clinerules/automatic.md"
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_in(&self, dir: &Path) -> bool {
        dir.join(".clinerules").exists() || dir.join(".cline").join("skills").exists()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".cline").join("skills")]
    }

    // ── Capabilities ────────────────────────────────────────────────────

    fn capabilities(&self) -> super::AgentCapabilities {
        super::AgentCapabilities {
            mcp_servers: false,
            agents: false,
            ..Default::default()
        }
    }

    fn mcp_note(&self) -> Option<&'static str> {
        Some(
            "Cline CLI stores MCP servers in global CLI settings, not in a project file. Automatic can discover those settings but does not sync project-specific MCP config for Cline.",
        )
    }

    // ── Cleanup ─────────────────────────────────────────────────────────

    fn owned_config_paths(&self, dir: &Path) -> Vec<PathBuf> {
        let _ = dir;
        vec![]
    }

    // ── Config writing ──────────────────────────────────────────────────

    fn write_mcp_config(
        &self,
        _dir: &Path,
        _servers: &Map<String, Value>,
    ) -> Result<String, String> {
        // Cline stores MCP settings in global CLI state, not per-project.
        // Skip silently — the user is informed via `mcp_note()` in the UI.
        Ok(String::new())
    }

    // ── Discovery ───────────────────────────────────────────────────────

    fn discover_mcp_servers(&self, dir: &Path) -> Map<String, Value> {
        let _ = dir;
        Map::new()
    }

    fn extra_global_skill_dirs(&self) -> Vec<std::path::PathBuf> {
        // Cline stores skills in ~/.cline/skills/ at user level —
        // not covered by the standard ~/.agents/skills/ or ~/.claude/skills/ scan.
        match super::home_dir() {
            Some(home) => vec![home.join(".cline").join("skills")],
            None => vec![],
        }
    }

    fn detect_global_install(&self) -> bool {
        super::cli_available("cline")
            || self
                .global_mcp_settings_path()
                .is_some_and(|path| path.exists())
            || std::env::var_os("CLINE_DIR").is_some_and(|dir| PathBuf::from(dir).exists())
    }

    fn discover_global_mcp_servers(&self) -> Map<String, Value> {
        match self.global_mcp_settings_path() {
            Some(path) => discover_mcp_servers_from_json(&path, "mcpServers", identity),
            None => Map::new(),
        }
    }
}

impl Cline {
    fn global_mcp_settings_path(&self) -> Option<PathBuf> {
        if let Some(dir) = std::env::var_os("CLINE_DIR") {
            return Some(
                PathBuf::from(dir)
                    .join("data")
                    .join("settings")
                    .join("cline_mcp_settings.json"),
            );
        }

        super::home_dir().map(|home| {
            home.join(".cline")
                .join("data")
                .join("settings")
                .join("cline_mcp_settings.json")
        })
    }
}

/// Pass-through normaliser: Cline's format is already canonical.
fn identity(v: Value) -> Value {
    v
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_detect() {
        let dir = tempdir().unwrap();
        assert!(!Cline.detect_in(dir.path()));

        fs::create_dir_all(dir.path().join(".cline").join("skills")).unwrap();
        assert!(Cline.detect_in(dir.path()));
    }

    #[test]
    fn test_detect_clinerules() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".clinerules")).unwrap();
        assert!(Cline.detect_in(dir.path()));
    }

    #[test]
    fn test_mcp_capability_disabled() {
        assert!(!Cline.capabilities().mcp_servers);
        assert!(Cline.mcp_note().is_some());
    }

    #[test]
    fn test_project_file_name_uses_managed_rule_file() {
        assert_eq!(Cline.project_file_name(), ".clinerules/automatic.md");
    }

    #[test]
    fn test_discover_global_mcp_servers_uses_cline_dir_override() {
        let cline_dir = tempdir().unwrap();
        std::env::set_var("CLINE_DIR", cline_dir.path());
        let settings_dir = cline_dir.path().join("data").join("settings");
        fs::create_dir_all(&settings_dir).unwrap();
        fs::write(
            settings_dir.join("cline_mcp_settings.json"),
            r#"{"mcpServers":{"github":{"command":"npx","args":["-y","@modelcontextprotocol/server-github"]}}}"#,
        )
        .unwrap();

        let servers = Cline.discover_global_mcp_servers();
        std::env::remove_var("CLINE_DIR");

        assert!(servers.contains_key("github"));
    }
}
