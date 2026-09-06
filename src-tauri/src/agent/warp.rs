use serde_json::{Map, Value};
use std::fs;
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
/// (account-scoped, no local file).
///
/// Both files use the Claude-compatible dialect: stdio entries are
/// `{command, args, env}` (no `type`), HTTP/SSE entries carry `type` plus
/// `url` and optional `headers`.  Warp additionally understands a
/// `working_directory` field; it is left alone on write and imported
/// unchanged on discovery.
pub struct Warp;

/// Project-scope MCP config file: `<project>/.warp/.mcp.json`.
fn project_mcp_config_path(dir: &Path) -> PathBuf {
    dir.join(".warp").join(".mcp.json")
}

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
            agents: false,
            global_mcp_servers: true,
            ..Default::default()
        }
    }

    // ── MCP note ────────────────────────────────────────────────────────

    fn mcp_note(&self) -> Option<&'static str> {
        Some(
            "Warp reads MCP servers from .warp/.mcp.json (project) and ~/.warp/.mcp.json \
             (global); both files need one-time approval inside Warp (Settings \u{203a} AI \u{203a} \
             MCP servers) before a server starts. Warp also auto-discovers Claude Code and \
             Codex configs, so servers Automatic writes for those agents may already appear \
             in Warp.",
        )
    }

    // ── Cleanup ─────────────────────────────────────────────────────────

    /// `WARP.md` and `.warp/.mcp.json` are Warp's alone.  `AGENTS.md` is
    /// deliberately absent: it is shared with Codex, Cursor, OpenCode and four
    /// others, and the default `cleanup_mcp_config` deletes every path listed
    /// here — so removing Warp from a project used to delete the instruction
    /// file every other agent still reads.
    fn owned_config_paths(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join("WARP.md"), project_mcp_config_path(dir)]
    }

    // ── Config writing ──────────────────────────────────────────────────

    fn mcp_merge_inputs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![project_mcp_config_path(dir)]
    }

    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String> {
        // Warp reads .warp/.mcp.json in the Claude-compatible dialect (root
        // key `mcpServers`, stdio entries without a `type`).  The file is
        // dedicated to MCP config today, but the writer merges rather than
        // clobbers so any top-level key a user adds later survives.
        let path = project_mcp_config_path(dir);
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
            }
        }

        // A file that exists but does not parse is an error rather than an
        // empty starting point: overwriting it would destroy every key the
        // user has in there.
        let mut root = super::read_mergeable_json_object(&path)?;

        // Claude-compatible dialect: strip `type`/`enabled`/`timeout` from
        // stdio entries; leave HTTP/SSE entries otherwise untouched (the
        // Warp-specific `working_directory` field is harmless).
        let mut warp_servers = Map::new();
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
            warp_servers.insert(name.clone(), server);
        }

        root.insert("mcpServers".to_string(), Value::Object(warp_servers));

        let content = serde_json::to_string_pretty(&Value::Object(root))
            .map_err(|e| format!("JSON error: {}", e))?;
        fs::write(&path, content)
            .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;

        Ok(path.display().to_string())
    }

    fn global_mcp_target(&self) -> Option<super::GlobalMcpTarget> {
        let home = super::home_dir()?;
        Some(super::GlobalMcpTarget {
            path: home.join(".warp").join(".mcp.json"),
            reload_note: Some(
                "Warp requires a one-time approval in Settings > AI > MCP servers.",
            ),
        })
    }

    fn write_global_mcp_config(
        &self,
        desired: &Map<String, Value>,
        previously_managed: &[String],
    ) -> Result<super::GlobalMcpWriteReport, String> {
        let Some(target) = self.global_mcp_target() else {
            return Err("Home directory not available for Warp global MCP write".to_string());
        };

        // Warp's dialect keeps configs as-is (its Warp-specific
        // `working_directory` field is harmless on write); strip only the
        // internal enabled/timeout markers to keep the file readable.
        let mut rendered = Map::new();
        for (name, config) in desired {
            let mut server = config.clone();
            if let Some(obj) = server.as_object_mut() {
                obj.remove("enabled");
                obj.remove("timeout");
            }
            rendered.insert(name.clone(), server);
        }

        super::merge_global_mcp_entries_json(&target.path, "mcpServers", &rendered, previously_managed)
    }

    // ── Discovery ───────────────────────────────────────────────────────

    /// Read `<project>/.warp/.mcp.json` in the Claude-compatible dialect.
    /// Absent file returns an empty map.
    fn discover_mcp_servers(&self, dir: &Path) -> Map<String, Value> {
        let path = project_mcp_config_path(dir);
        if !path.exists() {
            return Map::new();
        }
        discover_mcp_servers_from_json(&path, "mcpServers", identity)
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

    fn discover_global_mcp_entry_names(&self) -> std::collections::HashSet<String> {
        let Some(home) = super::home_dir() else {
            return std::collections::HashSet::new();
        };
        let path = home.join(".warp").join(".mcp.json");
        super::read_global_mcp_entry_names_json(&path, "mcpServers")
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
    fn test_mcp_capability_enabled() {
        assert!(Warp.capabilities().mcp_servers);
    }

    #[test]
    fn test_mcp_note_is_some() {
        assert!(Warp.mcp_note().is_some());
    }

    #[test]
    fn test_owned_config_paths_includes_warp_md_and_project_mcp_file() {
        let dir = tempdir().unwrap();
        let paths = Warp.owned_config_paths(dir.path());
        assert!(paths.contains(&dir.path().join("WARP.md")));
        assert!(paths.contains(&dir.path().join(".warp/.mcp.json")));
        assert!(
            !paths.contains(&dir.path().join("AGENTS.md")),
            "AGENTS.md is shared with seven other agents and is not Warp's to own"
        );
        assert!(
            !paths.contains(&dir.path().join(".warp")),
            ".warp/ may hold other Warp state \u{2014} cleanup must never delete the directory"
        );
    }

    #[test]
    fn test_write_creates_warp_dir_and_writes_mcp_json() {
        let dir = tempdir().unwrap();
        let mut servers = Map::new();
        servers.insert(
            "github".to_string(),
            serde_json::json!({"command": "npx", "args": ["-y", "@modelcontextprotocol/server-github"]}),
        );

        let path = Warp.write_mcp_config(dir.path(), &servers).expect("write");
        assert_eq!(path, dir.path().join(".warp/.mcp.json").display().to_string());

        let content = fs::read_to_string(dir.path().join(".warp/.mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(
            parsed["mcpServers"]["github"]["command"].as_str().unwrap(),
            "npx"
        );
    }

    #[test]
    fn test_write_stdio_strips_type_enabled_timeout() {
        let dir = tempdir().unwrap();
        let mut servers = Map::new();
        servers.insert(
            "srv".to_string(),
            serde_json::json!({
                "type": "stdio",
                "command": "/usr/local/bin/srv",
                "args": ["--flag"],
                "enabled": true,
                "timeout": 30
            }),
        );

        Warp.write_mcp_config(dir.path(), &servers).unwrap();

        let content = fs::read_to_string(dir.path().join(".warp/.mcp.json")).unwrap();
        let entry = &serde_json::from_str::<Value>(&content).unwrap()["mcpServers"]["srv"];

        assert!(
            entry.get("type").is_none(),
            "stdio has no `type` in the Claude-compatible dialect"
        );
        assert!(entry.get("enabled").is_none());
        assert!(entry.get("timeout").is_none());
        assert_eq!(entry["command"].as_str().unwrap(), "/usr/local/bin/srv");
    }

    #[test]
    fn test_write_remote_keeps_type_and_url() {
        let dir = tempdir().unwrap();
        let mut servers = Map::new();
        servers.insert(
            "linear".to_string(),
            serde_json::json!({
                "type": "http",
                "url": "https://mcp.linear.app/mcp",
                "headers": { "X-Client": "automatic" }
            }),
        );

        Warp.write_mcp_config(dir.path(), &servers).unwrap();

        let content = fs::read_to_string(dir.path().join(".warp/.mcp.json")).unwrap();
        let entry = &serde_json::from_str::<Value>(&content).unwrap()["mcpServers"]["linear"];

        assert_eq!(entry["type"].as_str().unwrap(), "http");
        assert_eq!(entry["url"].as_str().unwrap(), "https://mcp.linear.app/mcp");
        assert_eq!(entry["headers"]["X-Client"].as_str().unwrap(), "automatic");
    }

    #[test]
    fn test_write_preserves_unrelated_top_level_keys() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".warp")).unwrap();
        fs::write(
            dir.path().join(".warp/.mcp.json"),
            r#"{ "_userKey": "keep", "mcpServers": { "old": { "command": "x" } } }"#,
        )
        .unwrap();

        let mut servers = Map::new();
        servers.insert(
            "new".to_string(),
            serde_json::json!({"command": "n"}),
        );
        Warp.write_mcp_config(dir.path(), &servers).unwrap();

        let content = fs::read_to_string(dir.path().join(".warp/.mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["_userKey"].as_str().unwrap(), "keep");
        assert!(parsed["mcpServers"].get("old").is_none());
        assert_eq!(parsed["mcpServers"]["new"]["command"].as_str().unwrap(), "n");
    }

    #[test]
    fn test_write_errors_on_malformed_existing_file() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".warp")).unwrap();
        fs::write(dir.path().join(".warp/.mcp.json"), "{ not json").unwrap();

        let mut servers = Map::new();
        servers.insert("srv".to_string(), serde_json::json!({"command": "x"}));

        let result = Warp.write_mcp_config(dir.path(), &servers);
        assert!(
            result.is_err(),
            "must not silently overwrite an unparseable existing file"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join(".warp/.mcp.json")).unwrap(),
            "{ not json",
            "the unparseable file must be left exactly as the user left it"
        );
    }

    #[test]
    fn test_discover_reads_project_mcp_file() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".warp")).unwrap();
        fs::write(
            dir.path().join(".warp/.mcp.json"),
            r#"{
              "mcpServers": {
                "local":  { "command": "srv", "args": ["--x"] },
                "remote": { "type": "http", "url": "https://example/mcp" }
              }
            }"#,
        )
        .unwrap();

        let servers = Warp.discover_mcp_servers(dir.path());
        assert_eq!(servers["local"]["command"].as_str().unwrap(), "srv");
        assert_eq!(servers["remote"]["url"].as_str().unwrap(), "https://example/mcp");
    }

    #[test]
    fn test_discover_returns_empty_when_file_absent() {
        let dir = tempdir().unwrap();
        assert!(Warp.discover_mcp_servers(dir.path()).is_empty());
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
