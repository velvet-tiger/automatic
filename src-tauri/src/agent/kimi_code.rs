use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, Agent};

/// Kimi Code CLI agent — Moonshot AI's coding CLI (<https://www.kimi.com/code>).
///
/// - **Instructions** — shared `AGENTS.md` at the workspace root. Kimi also
///   reads `.kimi-code/AGENTS.md` and the cross-tool `~/.agents/AGENTS.md`;
///   Automatic writes the shared root file so Kimi picks it up alongside
///   Codex, Cursor, Warp and the others.
/// - **Skills** — Claude-compatible `SKILL.md` folders. Kimi natively reads
///   the cross-tool `.agents/skills/` hub, so Automatic writes there rather
///   than the Kimi-specific `.kimi-code/skills/`.
/// - **MCP** — dedicated `.kimi-code/mcp.json` with a top-level `mcpServers`
///   map (same shape as Claude Code and Cursor). Automatic owns the file
///   outright.
/// - **Sub-agents** — `.kimi-code/agents/`, one Markdown file per agent with
///   YAML frontmatter (`name`, `description`, `whenToUse`, `tools`,
///   `disallowedTools`, `model_preference`).
/// - **Commands / hooks** — off in this phase. Kimi's slash-command surface
///   overlaps with skills (`/skill:<name>`), and hooks are documented only
///   for the user-level `~/.kimi-code/config.toml`, which project sync should
///   never touch.
pub struct KimiCode;

impl Agent for KimiCode {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "kimi"
    }

    fn label(&self) -> &'static str {
        "Kimi Code"
    }

    fn config_description(&self) -> &'static str {
        ".kimi-code/mcp.json"
    }

    fn project_file_name(&self) -> &'static str {
        "AGENTS.md"
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_in(&self, dir: &Path) -> bool {
        // `.kimi-code/` is Kimi's canonical workspace marker.  Do NOT match on
        // `AGENTS.md` alone — that file is shared with Codex CLI, Cursor,
        // Warp, Z Code and others and would produce false positives.
        dir.join(".kimi-code").is_dir()
    }

    fn detect_global_install(&self) -> bool {
        super::cli_available("kimi")
            || super::home_dir()
                .map(|h| h.join(".kimi-code").exists())
                .unwrap_or(false)
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        // Kimi reads `.agents/skills/` natively (documented alongside
        // `.kimi-code/skills/`), so Automatic writes into the shared hub
        // rather than duplicating skills into a Kimi-specific directory.
        vec![dir.join(".agents").join("skills")]
    }

    fn extra_global_skill_dirs(&self) -> Vec<PathBuf> {
        match super::home_dir() {
            Some(home) => vec![home.join(".kimi-code").join("skills")],
            None => vec![],
        }
    }

    // ── Capabilities ────────────────────────────────────────────────────

    fn capabilities(&self) -> super::AgentCapabilities {
        super::AgentCapabilities {
            global_mcp_servers: true,
            ..Default::default()
        }
    }

    // ── Sub-agents ──────────────────────────────────────────────────────

    fn agents_dir(&self, dir: &Path) -> Option<PathBuf> {
        Some(dir.join(".kimi-code").join("agents"))
    }

    // ── Cleanup ─────────────────────────────────────────────────────────

    fn owned_config_paths(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".kimi-code").join("mcp.json")]
    }

    // ── Config writing ──────────────────────────────────────────────────

    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String> {
        // Kimi's schema: stdio entries carry `command`/`args`/`env`/`cwd`
        // with no `type`, HTTP entries carry a bare `url` (and optional
        // `headers`/`bearerTokenEnvVar`), and SSE entries mark themselves
        // with `"transport": "sse"` rather than a top-level `type`.
        let mut kimi_servers = Map::new();
        for (name, config) in servers {
            let transport = config
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("stdio");

            let mut server = config.clone();
            if let Some(obj) = server.as_object_mut() {
                obj.remove("enabled");
                obj.remove("timeout");
                obj.remove("type");
                match transport {
                    "http" => {
                        obj.remove("command");
                        obj.remove("args");
                    }
                    "sse" => {
                        obj.remove("command");
                        obj.remove("args");
                        obj.insert(
                            "transport".to_string(),
                            Value::String("sse".to_string()),
                        );
                    }
                    _ => {
                        obj.remove("url");
                        obj.remove("headers");
                    }
                }
            }
            kimi_servers.insert(name.clone(), server);
        }

        let kimi_dir = dir.join(".kimi-code");
        if !kimi_dir.exists() {
            fs::create_dir_all(&kimi_dir)
                .map_err(|e| format!("Failed to create .kimi-code/: {}", e))?;
        }

        let path = kimi_dir.join("mcp.json");
        let output = json!({ "mcpServers": Value::Object(kimi_servers) });
        let content =
            serde_json::to_string_pretty(&output).map_err(|e| format!("JSON error: {}", e))?;
        fs::write(&path, content)
            .map_err(|e| format!("Failed to write .kimi-code/mcp.json: {}", e))?;

        Ok(path.display().to_string())
    }

    fn global_mcp_target(&self) -> Option<super::GlobalMcpTarget> {
        let home = super::home_dir()?;
        Some(super::GlobalMcpTarget {
            path: home.join(".kimi-code").join("mcp.json"),
            reload_note: Some(
                "Kimi Code only registers servers for sessions started after the file changes.",
            ),
        })
    }

    fn write_global_mcp_config(
        &self,
        desired: &Map<String, Value>,
        previously_managed: &[String],
    ) -> Result<super::GlobalMcpWriteReport, String> {
        let Some(target) = self.global_mcp_target() else {
            return Err("Home directory not available for Kimi Code global MCP write".to_string());
        };

        // Mirror the project writer's Kimi dialect: strip type/enabled/timeout,
        // then for HTTP drop command/args, for SSE mark with `transport: "sse"`
        // and drop command/args, otherwise (stdio) drop url/headers.
        let mut rendered = Map::new();
        for (name, config) in desired {
            let transport = config
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("stdio");

            let mut server = config.clone();
            if let Some(obj) = server.as_object_mut() {
                obj.remove("enabled");
                obj.remove("timeout");
                obj.remove("type");
                match transport {
                    "http" => {
                        obj.remove("command");
                        obj.remove("args");
                    }
                    "sse" => {
                        obj.remove("command");
                        obj.remove("args");
                        obj.insert(
                            "transport".to_string(),
                            Value::String("sse".to_string()),
                        );
                    }
                    _ => {
                        obj.remove("url");
                        obj.remove("headers");
                    }
                }
            }
            rendered.insert(name.clone(), server);
        }

        super::merge_global_mcp_entries_json(&target.path, "mcpServers", &rendered, previously_managed)
    }

    // ── Discovery ───────────────────────────────────────────────────────

    fn discover_mcp_servers(&self, dir: &Path) -> Map<String, Value> {
        let path = dir.join(".kimi-code").join("mcp.json");
        if !path.exists() {
            return Map::new();
        }
        discover_mcp_servers_from_json(&path, "mcpServers", normalise_kimi_server)
    }

    fn discover_global_mcp_servers(&self) -> Map<String, Value> {
        let Some(home) = super::home_dir() else {
            return Map::new();
        };
        let path = home.join(".kimi-code").join("mcp.json");
        discover_mcp_servers_from_json(&path, "mcpServers", normalise_kimi_server)
    }
}

/// Re-derive Automatic's canonical `type` field from Kimi's field-shape
/// convention (command → stdio, transport: sse → sse, url → http) and drop
/// the `transport` marker that only exists in Kimi's dialect.  Without this,
/// discovery would hand back entries that a subsequent write could not tell
/// apart from user-written ones.
fn normalise_kimi_server(v: Value) -> Value {
    let mut obj = match v {
        Value::Object(m) => m,
        other => return other,
    };
    if obj.get("transport").and_then(|t| t.as_str()) == Some("sse") {
        obj.remove("transport");
        obj.insert("type".to_string(), Value::String("sse".to_string()));
    } else if obj.contains_key("command") {
        obj.insert("type".to_string(), Value::String("stdio".to_string()));
    } else if obj.contains_key("url") {
        obj.insert("type".to_string(), Value::String("http".to_string()));
    }
    Value::Object(obj)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn test_detect_in_matches_kimi_directory() {
        let dir = tempdir().unwrap();
        assert!(!KimiCode.detect_in(dir.path()));

        // AGENTS.md alone must NOT trigger detection (shared with other agents).
        fs::write(dir.path().join("AGENTS.md"), "").unwrap();
        assert!(!KimiCode.detect_in(dir.path()));

        fs::create_dir(dir.path().join(".kimi-code")).unwrap();
        assert!(KimiCode.detect_in(dir.path()));
    }

    #[test]
    fn test_write_stdio_omits_type_and_strips_internal_fields() {
        let dir = tempdir().unwrap();
        let mut servers = Map::new();
        servers.insert(
            "github".to_string(),
            json!({
                "type": "stdio",
                "command": "npx",
                "args": ["-y", "@modelcontextprotocol/server-github"],
                "env": { "GITHUB_TOKEN": "ghp_test" },
                "enabled": true,
                "timeout": 30
            }),
        );

        KimiCode.write_mcp_config(dir.path(), &servers).unwrap();
        let written =
            fs::read_to_string(dir.path().join(".kimi-code/mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&written).unwrap();
        let server = &parsed["mcpServers"]["github"];

        assert_eq!(server["command"], "npx");
        assert_eq!(server["env"]["GITHUB_TOKEN"], "ghp_test");
        assert!(
            server.get("type").is_none(),
            "stdio entries must not include `type` in Kimi's schema"
        );
        assert!(server.get("enabled").is_none());
        assert!(server.get("timeout").is_none());
    }

    #[test]
    fn test_write_http_preserves_url_and_headers() {
        let dir = tempdir().unwrap();
        let mut servers = Map::new();
        servers.insert(
            "linear".to_string(),
            json!({
                "type": "http",
                "url": "https://mcp.linear.app/mcp",
                "headers": { "X-Client": "automatic" },
                "command": "stray"
            }),
        );

        KimiCode.write_mcp_config(dir.path(), &servers).unwrap();
        let written =
            fs::read_to_string(dir.path().join(".kimi-code/mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&written).unwrap();
        let server = &parsed["mcpServers"]["linear"];

        assert_eq!(server["url"], "https://mcp.linear.app/mcp");
        assert_eq!(server["headers"]["X-Client"], "automatic");
        assert!(
            server.get("type").is_none(),
            "HTTP entries carry no `type` in Kimi's schema"
        );
        assert!(
            server.get("command").is_none(),
            "stdio fields must not leak into HTTP entries"
        );
    }

    #[test]
    fn test_write_sse_uses_transport_key() {
        let dir = tempdir().unwrap();
        let mut servers = Map::new();
        servers.insert(
            "legacy".to_string(),
            json!({ "type": "sse", "url": "https://legacy.example.com/sse" }),
        );

        KimiCode.write_mcp_config(dir.path(), &servers).unwrap();
        let written =
            fs::read_to_string(dir.path().join(".kimi-code/mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&written).unwrap();
        let server = &parsed["mcpServers"]["legacy"];

        assert_eq!(server["transport"], "sse");
        assert_eq!(server["url"], "https://legacy.example.com/sse");
        assert!(server.get("type").is_none());
    }

    #[test]
    fn test_discover_reads_mcpservers_key_and_rehydrates_type() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join(".kimi-code")).unwrap();
        fs::write(
            dir.path().join(".kimi-code").join("mcp.json"),
            r#"{
                "mcpServers": {
                    "github": { "command": "npx", "args": ["-y", "server"] },
                    "linear": { "url": "https://mcp.linear.app/mcp" },
                    "legacy": { "url": "https://x.example/sse", "transport": "sse" }
                }
            }"#,
        )
        .unwrap();

        let servers = KimiCode.discover_mcp_servers(dir.path());
        assert_eq!(servers["github"]["type"], "stdio");
        assert_eq!(servers["linear"]["type"], "http");
        assert_eq!(servers["legacy"]["type"], "sse");
        assert!(
            servers["legacy"].get("transport").is_none(),
            "the SSE marker is Kimi-specific and must be dropped after normalisation"
        );
    }

    #[test]
    fn test_capabilities() {
        let caps = KimiCode.capabilities();
        assert!(caps.skills);
        assert!(caps.instructions);
        assert!(caps.mcp_servers);
        assert!(caps.agents, "Kimi has a project-level sub-agent directory");
        assert!(
            !caps.commands,
            "commands are off in this phase — Kimi has no dedicated command surface"
        );
        assert!(
            !caps.hooks,
            "Kimi hooks are documented only at the user scope; project sync leaves them alone"
        );
    }

    #[test]
    fn test_owned_config_paths_names_mcp_json() {
        let dir = tempdir().unwrap();
        let owned = KimiCode.owned_config_paths(dir.path());
        assert_eq!(owned.len(), 1);
        assert!(owned[0].ends_with(".kimi-code/mcp.json"));
    }

    #[test]
    fn test_skill_sync_writes_to_shared_hub() {
        let dir = tempdir().unwrap();
        let skills = vec![("my-skill".to_string(), "# My Skill\n".to_string())];
        let selected = vec!["my-skill".to_string()];

        let written = KimiCode
            .sync_skills(dir.path(), &skills, &selected, &[])
            .unwrap();
        assert_eq!(written.len(), 1);

        let content =
            fs::read_to_string(dir.path().join(".agents/skills/my-skill/SKILL.md")).unwrap();
        assert_eq!(content, "# My Skill\n");
    }
}
