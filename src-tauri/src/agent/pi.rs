use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, sync_individual_skills, Agent};

/// Pi agent — pi.dev's minimal agent harness.
///
/// Pi itself is configuration-light, but MCP servers and sub-agents are
/// supported through community extension packages installed via `pi install`:
///
/// - **MCP** — provided by `pi-mcp-adapter` (and `pi-mcp-extension`).  Reads
///   `.pi/mcp.json` (Pi project override) and `.mcp.json` (project shared).
///   Same JSON shape as Claude Code: `{ "mcpServers": { ... } }`.  Automatic
///   writes to the Pi-specific location (`.pi/mcp.json`) so it doesn't stomp
///   on Claude Code's `.mcp.json` if both agents are selected for the
///   project.
/// - **Sub-agents** — provided by `pi-subagents` and forks.  Stored as
///   Markdown files with YAML frontmatter under `.pi/agents/`, same shape as
///   Claude Code sub-agents.
///
/// Project instructions live in `AGENTS.md` at the project root.  Skills are
/// written to `.pi/skills/<name>/SKILL.md`.
pub struct Pi;

impl Agent for Pi {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "pi"
    }

    fn label(&self) -> &'static str {
        "Pi (Beta)"
    }

    fn config_description(&self) -> &'static str {
        ".pi/mcp.json (via pi-mcp-adapter)"
    }

    fn project_file_name(&self) -> &'static str {
        "AGENTS.md"
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_global_install(&self) -> bool {
        super::cli_available("pi")
            || super::home_dir()
                .map(|h| h.join(".pi").exists())
                .unwrap_or(false)
    }

    fn detect_in(&self, dir: &Path) -> bool {
        // `.pi/` is Pi's canonical project-level marker directory.  Do NOT
        // match on `AGENTS.md` alone — that file is shared with Codex CLI,
        // Gemini CLI and Warp and would produce false positives.
        dir.join(".pi").is_dir()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".pi").join("skills")]
    }

    fn extra_global_skill_dirs(&self) -> Vec<PathBuf> {
        match super::home_dir() {
            Some(home) => vec![home.join(".pi").join("agent").join("skills")],
            None => vec![],
        }
    }

    // ── Config writing ──────────────────────────────────────────────────

    /// Write MCP servers in Claude-compatible JSON format to `.pi/mcp.json`.
    /// `pi-mcp-adapter` reads this file as the Pi-specific override layer.
    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String> {
        let pi_dir = dir.join(".pi");
        if !pi_dir.exists() {
            fs::create_dir_all(&pi_dir).map_err(|e| format!("Failed to create .pi/: {}", e))?;
        }

        // Pi's MCP shape matches Claude Code's: strip the "type" field from
        // stdio entries (the adapter infers transport from `command` vs `url`)
        // and drop Automatic-internal fields the adapter doesn't expect.
        let mut pi_servers = Map::new();
        for (name, config) in servers {
            let transport = config
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("stdio");

            let mut server = config.clone();
            if let Some(obj) = server.as_object_mut() {
                if transport == "stdio" {
                    obj.remove("type");
                    obj.remove("enabled");
                    obj.remove("timeout");
                }
            }
            pi_servers.insert(name.clone(), server);
        }

        let output = json!({ "mcpServers": Value::Object(pi_servers) });
        let path = pi_dir.join("mcp.json");
        let content =
            serde_json::to_string_pretty(&output).map_err(|e| format!("JSON error: {}", e))?;
        fs::write(&path, content).map_err(|e| format!("Failed to write .pi/mcp.json: {}", e))?;

        Ok(path.display().to_string())
    }

    fn sync_skills(
        &self,
        dir: &Path,
        skill_contents: &[(String, String)],
        selected_names: &[String],
        local_skill_names: &[String],
    ) -> Result<Vec<String>, String> {
        let mut written = Vec::new();
        let skills_dir = dir.join(".pi").join("skills");
        sync_individual_skills(
            &skills_dir,
            skill_contents,
            selected_names,
            local_skill_names,
            &mut written,
        )?;
        Ok(written)
    }

    // ── Sub-agents ──────────────────────────────────────────────────────

    /// Sub-agents are stored as Markdown + YAML frontmatter under `.pi/agents/`,
    /// matching the format consumed by `pi-subagents` and its forks.
    fn agents_dir(&self, dir: &Path) -> Option<PathBuf> {
        Some(dir.join(".pi").join("agents"))
    }

    // ── Discovery ───────────────────────────────────────────────────────

    fn discover_mcp_servers(&self, dir: &Path) -> Map<String, Value> {
        let path = dir.join(".pi").join("mcp.json");
        if !path.exists() {
            return Map::new();
        }
        // Same shape as Claude Code — no normalisation needed.
        discover_mcp_servers_from_json(&path, "mcpServers", |v| v)
    }

    fn discover_global_mcp_servers(&self) -> Map<String, Value> {
        let Some(home) = super::home_dir() else {
            return Map::new();
        };
        // Pi's user-level override location.  The shared
        // `~/.config/mcp/mcp.json` is intentionally not read here — it is the
        // cross-tool shared config and another agent may already own it.
        discover_mcp_servers_from_json(&home.join(".pi").join("agent").join("mcp.json"), "mcpServers", |v| v)
    }

    // ── Cleanup ─────────────────────────────────────────────────────────

    /// Pi exclusively owns `.pi/mcp.json`.  `AGENTS.md` is shared with Codex,
    /// Gemini and Warp and is NOT listed here — removing Pi must not delete
    /// it if any of those agents are still selected.
    fn owned_config_paths(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".pi").join("mcp.json")]
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_detect_in_matches_pi_directory() {
        let dir = tempdir().unwrap();
        assert!(!Pi.detect_in(dir.path()));

        // AGENTS.md alone must NOT trigger detection (shared with other agents).
        fs::write(dir.path().join("AGENTS.md"), "").unwrap();
        assert!(!Pi.detect_in(dir.path()));

        // `.pi/` directory is the canonical Pi project marker.
        fs::create_dir(dir.path().join(".pi")).unwrap();
        assert!(Pi.detect_in(dir.path()));
    }

    #[test]
    fn test_write_mcp_config_stdio_strips_internal_fields() {
        let dir = tempdir().unwrap();
        let mut servers = Map::new();
        servers.insert(
            "github".to_string(),
            json!({
                "type": "stdio",
                "command": "npx",
                "args": ["@modelcontextprotocol/server-github"],
                "enabled": true,
                "timeout": 30
            }),
        );

        let result = Pi.write_mcp_config(dir.path(), &servers).unwrap();
        assert!(result.contains(".pi/mcp.json") || result.contains(".pi\\mcp.json"));

        let written =
            fs::read_to_string(dir.path().join(".pi").join("mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&written).unwrap();
        let server = &parsed["mcpServers"]["github"];

        assert_eq!(server["command"], "npx");
        assert!(
            server.get("type").is_none(),
            "stdio entries should not include `type`"
        );
        assert!(
            server.get("enabled").is_none(),
            "internal `enabled` flag should be stripped"
        );
        assert!(
            server.get("timeout").is_none(),
            "internal `timeout` field should be stripped"
        );
    }

    #[test]
    fn test_write_mcp_config_http_preserves_url() {
        let dir = tempdir().unwrap();
        let mut servers = Map::new();
        servers.insert(
            "remote".to_string(),
            json!({
                "type": "http",
                "url": "https://example.com/mcp"
            }),
        );

        Pi.write_mcp_config(dir.path(), &servers).unwrap();
        let written =
            fs::read_to_string(dir.path().join(".pi").join("mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&written).unwrap();
        let server = &parsed["mcpServers"]["remote"];

        assert_eq!(server["type"], "http");
        assert_eq!(server["url"], "https://example.com/mcp");
    }

    #[test]
    fn test_discover_mcp_servers_reads_pi_mcp_json() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join(".pi")).unwrap();
        fs::write(
            dir.path().join(".pi").join("mcp.json"),
            r#"{ "mcpServers": { "github": { "command": "npx", "args": ["@modelcontextprotocol/server-github"] } } }"#,
        )
        .unwrap();

        let servers = Pi.discover_mcp_servers(dir.path());
        assert!(servers.contains_key("github"));
        assert_eq!(servers["github"]["command"], "npx");
    }

    #[test]
    fn test_owned_config_paths_includes_only_pi_mcp_json() {
        let dir = tempdir().unwrap();
        let owned = Pi.owned_config_paths(dir.path());
        assert_eq!(owned, vec![dir.path().join(".pi").join("mcp.json")]);

        // AGENTS.md must never appear here — it's shared with Codex/Gemini/Warp.
        assert!(!owned.contains(&dir.path().join("AGENTS.md")));
    }

    #[test]
    fn test_cleanup_preserves_shared_agents_md() {
        let dir = tempdir().unwrap();
        let agents_md = dir.path().join("AGENTS.md");
        fs::write(&agents_md, "# shared\n").unwrap();

        let removed = Pi.cleanup_mcp_config(dir.path());
        assert!(
            removed.is_empty(),
            "Pi must not touch AGENTS.md on cleanup"
        );
        assert!(agents_md.exists());
    }

    #[test]
    fn test_cleanup_removes_pi_mcp_json() {
        let dir = tempdir().unwrap();
        let pi_dir = dir.path().join(".pi");
        fs::create_dir(&pi_dir).unwrap();
        let mcp = pi_dir.join("mcp.json");
        fs::write(&mcp, "{}").unwrap();

        let removed = Pi.cleanup_mcp_config(dir.path());
        assert_eq!(removed, vec![mcp.display().to_string()]);
        assert!(!mcp.exists());
    }

    #[test]
    fn test_capabilities_enables_mcp_and_subagents() {
        let caps = Pi.capabilities();
        assert!(caps.mcp_servers, "Pi supports MCP via pi-mcp-adapter");
        assert!(caps.agents, "Pi supports sub-agents via pi-subagents");
        assert!(caps.skills);
        assert!(caps.instructions);
        assert!(!caps.commands, "Pi commands (prompt templates) not synced yet");
        assert!(!caps.hooks, "Pi has no hook concept");
    }

    #[test]
    fn test_agents_dir_returns_pi_agents() {
        let dir = tempdir().unwrap();
        assert_eq!(
            Pi.agents_dir(dir.path()),
            Some(dir.path().join(".pi").join("agents"))
        );
    }

    #[test]
    fn test_agents_file_ext_is_md() {
        assert_eq!(Pi.agents_file_ext(), "md");
    }

    #[test]
    fn test_skill_sync_writes_to_pi_skills() {
        let dir = tempdir().unwrap();
        let skills = vec![("my-skill".to_string(), "# My Skill\n".to_string())];
        let selected = vec!["my-skill".to_string()];

        let written = Pi
            .sync_skills(dir.path(), &skills, &selected, &[])
            .unwrap();
        assert_eq!(written.len(), 1);

        let content =
            fs::read_to_string(dir.path().join(".pi/skills/my-skill/SKILL.md")).unwrap();
        assert_eq!(content, "# My Skill\n");
    }
}
