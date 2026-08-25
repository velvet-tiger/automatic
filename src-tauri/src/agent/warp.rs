use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, Agent, AgentCapabilities};

/// Warp agent — uses `AGENTS.md` as the project rules file and stores
/// skills under `<project>/.agents/skills/<name>/SKILL.md`.
///
/// Warp migrated from `WARP.md` to `AGENTS.md` as the canonical project rules
/// filename (the old name is still supported for backwards compatibility, but
/// new projects should use `AGENTS.md`).  Detection still matches `.warp/`
/// directories and legacy `WARP.md` files so that existing projects continue to
/// be recognised.
///
/// **MCP note**: Warp reads file-based MCP config from a global
/// `~/.warp/.mcp.json` and a project-level `.warp/.mcp.json` (`mcpServers`
/// key), and also auto-discovers Claude Code's `~/.claude.json` and Codex's
/// `~/.codex/config.toml`.  File-based servers need one-time approval inside
/// Warp before they start.  UI-added servers live in Warp Drive
/// (account-scoped, no local file).  Automatic does not yet write Warp's
/// files — it only discovers the global one.
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
            "Warp reads MCP servers from ~/.warp/.mcp.json (global) and .warp/.mcp.json \
             (project); file-based servers need one-time approval inside Warp \
             (Settings \u{203a} Agents \u{203a} MCP servers). Warp also auto-discovers Claude Code \
             and Codex configs, so servers Automatic writes for those agents may already \
             appear in Warp. Automatic does not yet write Warp\u{2019}s files.",
        )
    }

    // ── Cleanup ─────────────────────────────────────────────────────────

    /// Only `WARP.md` is Warp's alone.  `AGENTS.md` is deliberately absent:
    /// it is shared with Codex, Cursor, OpenCode and four others, and the
    /// default `cleanup_mcp_config` deletes every path listed here — so
    /// removing Warp from a project used to delete the instruction file every
    /// other agent still reads.
    fn owned_config_paths(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join("WARP.md")]
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

    // ── Discovery ───────────────────────────────────────────────────────

    /// Automatic does not sync a project-level Warp MCP file, so project
    /// discovery is empty.  (Warp itself reads `.warp/.mcp.json`, but writing
    /// it is future work — see the MCP note above.)
    fn discover_mcp_servers(&self, _dir: &Path) -> Map<String, Value> {
        Map::new()
    }

    fn discover_global_mcp_servers(&self) -> Map<String, Value> {
        let Some(home) = super::home_dir() else {
            return Map::new();
        };
        // ~/.warp/.mcp.json — Warp's global file-based MCP config, standard
        // `mcpServers` key.  Entries may carry a Warp-specific
        // `working_directory` field; it is harmless on import.
        let path = home.join(".warp").join(".mcp.json");
        discover_mcp_servers_from_json(&path, "mcpServers", identity)
    }
}

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
    fn test_owned_config_paths_covers_only_the_warp_specific_file() {
        let dir = tempdir().unwrap();
        let paths = Warp.owned_config_paths(dir.path());
        assert!(paths.contains(&dir.path().join("WARP.md")));
        assert!(
            !paths.contains(&dir.path().join("AGENTS.md")),
            "AGENTS.md is shared with seven other agents and is not Warp's to own"
        );
    }

    #[test]
    fn test_cleanup_leaves_shared_agents_md_alone() {
        let dir = tempdir().unwrap();
        let agents_md = dir.path().join("AGENTS.md");
        fs::write(&agents_md, "# Shared instructions\n").unwrap();

        use super::super::Agent as _;
        let removed = Warp.cleanup_mcp_config(dir.path());

        assert!(
            removed.is_empty(),
            "removing Warp must not report deleting a file it does not own: {removed:?}"
        );
        assert!(
            agents_md.exists(),
            "AGENTS.md is read by Codex, Cursor, OpenCode and others — removing \
             Warp from a project must not delete it"
        );
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
