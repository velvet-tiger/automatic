use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, Agent};

/// Z Code agent — Z.ai's desktop Agentic Development Environment
/// (<https://zcode.z.ai>).
///
/// - **Instructions** — `AGENTS.md` at the workspace root, shared with Codex,
///   Cursor and others.  Z Code itself prepends the user's global
///   `~/.zcode/AGENTS.md`; `CLAUDE.md` is only a one-time onboarding migration
///   source on Z Code's side, so no legacy migration is needed here.
/// - **Skills** — Claude-compatible `SKILL.md` folders under `.zcode/skills/`
///   (workspace) and `~/.zcode/skills/` (global).
/// - **MCP** — merged into the shared `.zcode/config.json` under the
///   `mcpServers` key.  Z Code also accepts a bare server map at the top
///   level, but Automatic always writes and reads the wrapped shape so other
///   config keys are never mistaken for servers.
/// - **Hooks / commands / sub-agents** — deliberately off.  Z Code ignores
///   project-level hooks for security (only `~/.zcode/cli/config.json` hooks
///   execute), and the workspace-level command and sub-agent paths are
///   undocumented (sub-agents are user-level `~/.zcode/agents/`, beta at
///   workspace scope).
pub struct ZCode;

impl Agent for ZCode {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "zcode"
    }

    fn label(&self) -> &'static str {
        "Z Code (Beta)"
    }

    fn config_description(&self) -> &'static str {
        ".zcode/config.json"
    }

    fn project_file_name(&self) -> &'static str {
        "AGENTS.md"
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_global_install(&self) -> bool {
        super::cli_available("zcode")
            || super::home_dir()
                .map(|h| h.join(".zcode").exists())
                .unwrap_or(false)
    }

    fn detect_in(&self, dir: &Path) -> bool {
        // `.zcode/` is Z Code's canonical workspace marker directory.  Do NOT
        // match on `AGENTS.md` alone — that file is shared with Codex CLI,
        // Cursor, Warp and others and would produce false positives.
        dir.join(".zcode").is_dir()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".zcode").join("skills")]
    }

    fn extra_global_skill_dirs(&self) -> Vec<PathBuf> {
        match super::home_dir() {
            Some(home) => vec![home.join(".zcode").join("skills")],
            None => vec![],
        }
    }

    // ── Capabilities ────────────────────────────────────────────────────

    /// Sub-agents stay off until Z Code documents a workspace-level agents
    /// directory (currently user-level `~/.zcode/agents/` only, with
    /// workspace support in beta).  Commands and hooks are off for the same
    /// reason: the workspace commands path is undocumented, and Z Code
    /// refuses to execute project-level hooks at all.
    fn capabilities(&self) -> super::AgentCapabilities {
        super::AgentCapabilities {
            agents: false,
            ..Default::default()
        }
    }

    fn mcp_note(&self) -> Option<&'static str> {
        Some(
            "Z Code merges MCP servers into the shared .zcode/config.json. \
             Project-level hooks in that file are ignored by Z Code for \
             security, and commands and sub-agents are user-level only \
             (~/.zcode/) \u{2014} Automatic does not sync them.",
        )
    }

    // ── Config writing ──────────────────────────────────────────────────

    fn mcp_merge_inputs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".zcode").join("config.json")]
    }

    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String> {
        let zcode_dir = dir.join(".zcode");
        if !zcode_dir.exists() {
            fs::create_dir_all(&zcode_dir)
                .map_err(|e| format!("Failed to create .zcode/: {}", e))?;
        }

        let path = zcode_dir.join("config.json");

        // Read existing config to preserve non-MCP keys.  A file we cannot
        // parse is an error rather than an empty starting point: the user's
        // own settings (and any hooks config) live here too.
        let mut root = super::read_mergeable_json_object(&path)?;

        // Z Code's server shape matches Claude Code's: stdio entries carry
        // `command`/`args`/`env` with no `type`, remote entries carry
        // `type`/`url`/`headers`.
        let mut zcode_servers = Map::new();
        for (name, config) in servers {
            let transport = config
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("stdio");

            let mut server = config.clone();
            if let Some(obj) = server.as_object_mut() {
                obj.remove("enabled");
                obj.remove("timeout");
                if transport == "http" || transport == "sse" {
                    // Keep type/url/headers, remove command/args that don't apply
                    obj.remove("command");
                    obj.remove("args");
                } else {
                    obj.remove("type");
                }
            }
            zcode_servers.insert(name.clone(), server);
        }

        root.insert("mcpServers".to_string(), Value::Object(zcode_servers));

        let content = serde_json::to_string_pretty(&Value::Object(root))
            .map_err(|e| format!("JSON error: {}", e))?;
        fs::write(&path, content)
            .map_err(|e| format!("Failed to write .zcode/config.json: {}", e))?;

        Ok(path.display().to_string())
    }

    // ── Discovery ───────────────────────────────────────────────────────

    fn discover_mcp_servers(&self, dir: &Path) -> Map<String, Value> {
        let path = dir.join(".zcode").join("config.json");
        if !path.exists() {
            return Map::new();
        }
        // Only the wrapped `mcpServers` shape is read.  Z Code tolerates a
        // bare server map at the top level, but reading that shape here would
        // misinterpret every other config key as a server definition.
        discover_mcp_servers_from_json(&path, "mcpServers", |v| v)
    }

    fn discover_global_mcp_servers(&self) -> Map<String, Value> {
        let Some(home) = super::home_dir() else {
            return Map::new();
        };
        // The desktop app and the bundled CLI keep separate config files;
        // read both.  The helper returns an empty map for missing files.
        let mut servers = discover_mcp_servers_from_json(
            &home.join(".zcode").join("config.json"),
            "mcpServers",
            |v| v,
        );
        let cli_config = home.join(".zcode").join("cli").join("config.json");
        for (name, config) in discover_mcp_servers_from_json(&cli_config, "mcpServers", |v| v) {
            servers.entry(name).or_insert(config);
        }
        // Z Code's own docs describe the CLI config's native shape as a
        // nested `mcp.servers` object rather than top-level `mcpServers`.
        // Read it additively so either shape surfaces; existing entries win.
        for (name, config) in
            super::discover_mcp_servers_from_json_at(&cli_config, &["mcp", "servers"], |v| v)
        {
            servers.entry(name).or_insert(config);
        }
        servers
    }

    // ── Cleanup ─────────────────────────────────────────────────────────

    /// Z Code merges into `.zcode/config.json`, which the user's own settings
    /// share.  Strip only the `mcpServers` key rather than deleting the whole
    /// file.  `owned_config_paths` stays empty for the same reason — the
    /// default cleanup deletes every path listed there.
    fn cleanup_mcp_config(&self, dir: &Path) -> Vec<String> {
        let path = dir.join(".zcode").join("config.json");
        if !path.exists() {
            return vec![];
        }
        let raw = match fs::read_to_string(&path) {
            Ok(r) => r,
            Err(_) => return vec![],
        };
        let mut root: Map<String, Value> = match serde_json::from_str::<Value>(&raw) {
            Ok(Value::Object(m)) => m,
            _ => return vec![],
        };
        if root.remove("mcpServers").is_none() {
            return vec![];
        }
        if root.is_empty() {
            if fs::remove_file(&path).is_ok() {
                return vec![path.display().to_string()];
            }
        } else {
            let content = match serde_json::to_string_pretty(&Value::Object(root)) {
                Ok(c) => c,
                Err(_) => return vec![],
            };
            if fs::write(&path, content).is_ok() {
                return vec![path.display().to_string()];
            }
        }
        vec![]
    }

    fn cleanup_mcp_preview(&self, dir: &Path) -> Vec<String> {
        let path = dir.join(".zcode").join("config.json");
        if path.exists() {
            vec![path.display().to_string()]
        } else {
            vec![]
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_detect_in_matches_zcode_directory() {
        let dir = tempdir().unwrap();
        assert!(!ZCode.detect_in(dir.path()));

        // AGENTS.md alone must NOT trigger detection (shared with other agents).
        fs::write(dir.path().join("AGENTS.md"), "").unwrap();
        assert!(!ZCode.detect_in(dir.path()));

        // `.zcode/` directory is the canonical workspace marker.
        fs::create_dir(dir.path().join(".zcode")).unwrap();
        assert!(ZCode.detect_in(dir.path()));
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

        let result = ZCode.write_mcp_config(dir.path(), &servers).unwrap();
        assert!(result.contains(".zcode") && result.contains("config.json"));

        let written = fs::read_to_string(dir.path().join(".zcode").join("config.json")).unwrap();
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
                "url": "https://example.com/mcp",
                "headers": { "Authorization": "Bearer tok" },
                "command": "stray"
            }),
        );

        ZCode.write_mcp_config(dir.path(), &servers).unwrap();
        let written = fs::read_to_string(dir.path().join(".zcode").join("config.json")).unwrap();
        let parsed: Value = serde_json::from_str(&written).unwrap();
        let server = &parsed["mcpServers"]["remote"];

        assert_eq!(server["type"], "http");
        assert_eq!(server["url"], "https://example.com/mcp");
        assert_eq!(server["headers"]["Authorization"], "Bearer tok");
        assert!(
            server.get("command").is_none(),
            "remote entries should not carry stdio fields"
        );
    }

    #[test]
    fn test_write_preserves_existing_config() {
        let dir = tempdir().unwrap();
        let zcode_dir = dir.path().join(".zcode");
        fs::create_dir_all(&zcode_dir).unwrap();

        // The user's own settings — including any hooks config, which Z Code
        // reads only from the user scope but a user may still keep here.
        let existing = json!({
            "hooks": { "enabled": true, "events": {} },
            "theme": "dark",
            "mcpServers": { "old": { "command": "old" } }
        });
        fs::write(
            zcode_dir.join("config.json"),
            serde_json::to_string_pretty(&existing).unwrap(),
        )
        .unwrap();

        let mut servers = Map::new();
        servers.insert("github".to_string(), json!({"command": "npx"}));
        ZCode.write_mcp_config(dir.path(), &servers).unwrap();

        let content = fs::read_to_string(zcode_dir.join("config.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        // Existing non-MCP keys preserved
        assert_eq!(parsed["theme"], "dark");
        assert_eq!(parsed["hooks"]["enabled"], true);
        // MCP servers replaced wholesale
        assert_eq!(parsed["mcpServers"]["github"]["command"], "npx");
        assert!(parsed["mcpServers"]["old"].is_null());
    }

    #[test]
    fn test_write_errors_on_malformed_config() {
        let dir = tempdir().unwrap();
        let zcode_dir = dir.path().join(".zcode");
        fs::create_dir_all(&zcode_dir).unwrap();
        fs::write(zcode_dir.join("config.json"), "{ not json").unwrap();

        let mut servers = Map::new();
        servers.insert("github".to_string(), json!({"command": "npx"}));

        let result = ZCode.write_mcp_config(dir.path(), &servers);
        assert!(result.is_err(), "malformed config must error, not clobber");
        assert_eq!(
            fs::read_to_string(zcode_dir.join("config.json")).unwrap(),
            "{ not json",
            "the malformed file must be left untouched"
        );
    }

    #[test]
    fn test_cleanup_strips_mcp_servers_key() {
        let dir = tempdir().unwrap();
        let zcode_dir = dir.path().join(".zcode");
        fs::create_dir_all(&zcode_dir).unwrap();

        let existing = json!({
            "theme": "dark",
            "mcpServers": { "auto": { "command": "automatic" } }
        });
        fs::write(
            zcode_dir.join("config.json"),
            serde_json::to_string_pretty(&existing).unwrap(),
        )
        .unwrap();

        let removed = ZCode.cleanup_mcp_config(dir.path());
        assert_eq!(removed.len(), 1);

        let content = fs::read_to_string(zcode_dir.join("config.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert!(parsed["mcpServers"].is_null());
        assert_eq!(parsed["theme"], "dark");
    }

    #[test]
    fn test_cleanup_deletes_empty_file() {
        let dir = tempdir().unwrap();
        let zcode_dir = dir.path().join(".zcode");
        fs::create_dir_all(&zcode_dir).unwrap();

        let existing = json!({
            "mcpServers": { "auto": { "command": "automatic" } }
        });
        fs::write(
            zcode_dir.join("config.json"),
            serde_json::to_string_pretty(&existing).unwrap(),
        )
        .unwrap();

        let removed = ZCode.cleanup_mcp_config(dir.path());
        assert_eq!(removed.len(), 1);
        assert!(!zcode_dir.join("config.json").exists());
    }

    #[test]
    fn test_cleanup_preserves_shared_agents_md() {
        let dir = tempdir().unwrap();
        let agents_md = dir.path().join("AGENTS.md");
        fs::write(&agents_md, "# shared\n").unwrap();

        let removed = ZCode.cleanup_mcp_config(dir.path());
        assert!(
            removed.is_empty(),
            "Z Code must not touch AGENTS.md on cleanup"
        );
        assert!(agents_md.exists());
    }

    #[test]
    fn test_discover_mcp_servers_reads_mcpservers_key() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join(".zcode")).unwrap();
        fs::write(
            dir.path().join(".zcode").join("config.json"),
            r#"{ "mcpServers": { "github": { "command": "npx", "args": ["@modelcontextprotocol/server-github"] } } }"#,
        )
        .unwrap();

        let servers = ZCode.discover_mcp_servers(dir.path());
        assert!(servers.contains_key("github"));
        assert_eq!(servers["github"]["command"], "npx");
    }

    #[test]
    fn test_discover_ignores_bare_map_shape() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join(".zcode")).unwrap();
        // Z Code accepts this shape, but Automatic deliberately does not read
        // it: without the `mcpServers` wrapper, other config keys would be
        // indistinguishable from server definitions.
        fs::write(
            dir.path().join(".zcode").join("config.json"),
            r#"{ "some-server": { "command": "x" }, "theme": "dark" }"#,
        )
        .unwrap();

        let servers = ZCode.discover_mcp_servers(dir.path());
        assert!(servers.is_empty());
    }

    #[test]
    fn test_capabilities() {
        let caps = ZCode.capabilities();
        assert!(caps.skills);
        assert!(caps.instructions);
        assert!(caps.mcp_servers);
        assert!(
            !caps.agents,
            "workspace sub-agents are beta in Z Code and the path is undocumented"
        );
        assert!(
            !caps.commands,
            "workspace commands path is undocumented — user-level only"
        );
        assert!(
            !caps.hooks,
            "Z Code ignores project-level hooks for security"
        );
    }

    #[test]
    fn test_owned_config_paths_is_empty() {
        let dir = tempdir().unwrap();
        // `.zcode/config.json` is shared with the user's own settings, so it
        // must never be listed for wholesale deletion.
        assert!(ZCode.owned_config_paths(dir.path()).is_empty());
    }

    #[test]
    fn test_skill_sync_writes_to_zcode_skills() {
        let dir = tempdir().unwrap();
        let skills = vec![("my-skill".to_string(), "# My Skill\n".to_string())];
        let selected = vec!["my-skill".to_string()];

        let written = ZCode
            .sync_skills(dir.path(), &skills, &selected, &[])
            .unwrap();
        assert_eq!(written.len(), 1);

        let content =
            fs::read_to_string(dir.path().join(".zcode/skills/my-skill/SKILL.md")).unwrap();
        assert_eq!(content, "# My Skill\n");
    }
}
