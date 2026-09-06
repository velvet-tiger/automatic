use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, Agent};

/// Google Antigravity agent — stores skills under
/// `<project>/.agents/skills/<name>/SKILL.md`.
///
/// An Antigravity CLI now exists alongside the IDE and shares the same
/// harness and config (both MCP config files below and, per Google's own
/// docs, the instruction file) — one `Antigravity` implementation correctly
/// covers both.
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
/// Antigravity reads MCP servers from two files (per Google's own docs):
///
/// - Project scope: `<project>/.agents/mcp_config.json`
/// - Global scope:  `~/.gemini/config/mcp_config.json` (shared with the
///   Antigravity CLI)
///
/// Both files use the `mcpServers` root key.  Stdio entries are the standard
/// `{command, args, env}` shape with no explicit `type`.  Remote entries use
/// **`serverUrl`** (not `url` or `httpUrl`) and may carry `headers`,
/// `authProviderType`, `oauth`, `disabled`, and `disabledTools`.
///
/// ```json
/// {
///   "mcpServers": {
///     "local":  { "command": "npx", "args": ["-y", "..."] },
///     "remote": { "serverUrl": "https://example/mcp", "headers": {"X-Api-Key": "..."} }
///   }
/// }
/// ```
///
/// **Caveat:** Google's docs do not describe environment-variable
/// interpolation in either file.  Automatic still writes the shared
/// `${VAR}` placeholder for inherited env vars — the raw secret never
/// reaches disk — but if Antigravity does not interpolate, the child MCP
/// server will receive the literal `${VAR}` string.  Users must be told to
/// verify.  `.agents/` is the shared skills hub every agent reads from, so
/// `owned_config_paths` names `.agents/mcp_config.json` **specifically** —
/// listing `.agents/` wholesale would let cleanup delete the hub.
pub struct Antigravity;

/// Project-scope MCP config file: `<project>/.agents/mcp_config.json`.
fn project_mcp_config_path(dir: &Path) -> PathBuf {
    dir.join(".agents").join("mcp_config.json")
}

/// Render one canonical MCP entry into Antigravity's shape.
///
/// - stdio: strip `type`/`enabled`/`timeout`; pass through `command`, `args`,
///   `env`.
/// - remote: drop `type` (the presence of `serverUrl` identifies remote in
///   Antigravity's format) and rename `url` → `serverUrl`.  `headers`,
///   `authProviderType`, `oauth`, `disabled`, and `disabledTools` pass
///   through unchanged when present.
fn render_antigravity_entry(config: &Value) -> Value {
    let transport = config
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("stdio");

    let mut server = config.clone();
    let Some(obj) = server.as_object_mut() else {
        return server;
    };

    if transport == "stdio" {
        obj.remove("type");
        obj.remove("enabled");
        obj.remove("timeout");
    } else {
        // http / sse — Antigravity does not use `type`; `serverUrl` marks it.
        obj.remove("type");
        obj.remove("enabled");
        obj.remove("timeout");
        if let Some(url) = obj.remove("url") {
            obj.insert("serverUrl".to_string(), url);
        }
    }

    server
}

/// Normalise one Antigravity entry back into the canonical Automatic shape.
///
/// `serverUrl` → `url` with `type: "http"` (Antigravity does not distinguish
/// http vs sse in the shape; http is the safe default).
fn normalise_antigravity_entry(config: Value) -> Value {
    let Value::Object(mut obj) = config else {
        return config;
    };
    if let Some(server_url) = obj.remove("serverUrl") {
        obj.insert("url".to_string(), server_url);
        obj.entry("type".to_string())
            .or_insert(Value::String("http".to_string()));
    }
    Value::Object(obj)
}

impl Agent for Antigravity {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "antigravity"
    }

    fn label(&self) -> &'static str {
        "Antigravity (Beta)"
    }

    fn config_description(&self) -> &'static str {
        "GEMINI.md, .agents/mcp_config.json"
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
            agents: false,
            global_mcp_servers: true,
            ..Default::default()
        }
    }

    // ── MCP note ────────────────────────────────────────────────────────

    fn mcp_note(&self) -> Option<&'static str> {
        Some(
            "Antigravity reads project MCP from .agents/mcp_config.json and global MCP from \
             ~/.gemini/config/mcp_config.json. Environment variable interpolation is not \
             documented for either file, so any ${VAR} placeholder Automatic writes may reach \
             the child server as a literal string \u{2014} verify server env after sync.",
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

    /// Names `.agents/mcp_config.json` **specifically** — never `.agents/`
    /// wholesale.  The `.agents/` directory is the shared skills hub every
    /// agent reads from; listing it here would let cleanup delete the hub
    /// the first time Antigravity was removed from a project.
    fn owned_config_paths(&self, dir: &Path) -> Vec<PathBuf> {
        vec![project_mcp_config_path(dir)]
    }

    // ── Config writing ──────────────────────────────────────────────────

    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String> {
        let path = project_mcp_config_path(dir);

        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
            }
        }

        let mut rendered = Map::new();
        for (name, config) in servers {
            rendered.insert(name.clone(), render_antigravity_entry(config));
        }

        let output = json!({ "mcpServers": Value::Object(rendered) });
        let content =
            serde_json::to_string_pretty(&output).map_err(|e| format!("JSON error: {}", e))?;
        fs::write(&path, content)
            .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;

        Ok(path.display().to_string())
    }

    fn global_mcp_target(&self) -> Option<super::GlobalMcpTarget> {
        let home = super::home_dir()?;
        Some(super::GlobalMcpTarget {
            path: home.join(".gemini").join("config").join("mcp_config.json"),
            reload_note: None,
        })
    }

    fn write_global_mcp_config(
        &self,
        desired: &Map<String, Value>,
        previously_managed: &[String],
    ) -> Result<super::GlobalMcpWriteReport, String> {
        let Some(target) = self.global_mcp_target() else {
            return Err(
                "Home directory not available for Antigravity global MCP write".to_string(),
            );
        };

        // Mirror the project writer's dialect: strip type/enabled/timeout for
        // stdio, drop `type` and rename `url` → `serverUrl` for remote.  The
        // shared merge helper preserves every user-authored entry outside
        // Automatic's previously-managed set.
        let mut rendered = Map::new();
        for (name, config) in desired {
            rendered.insert(name.clone(), render_antigravity_entry(config));
        }

        super::merge_global_mcp_entries_json(&target.path, "mcpServers", &rendered, previously_managed)
    }

    // ── Discovery ───────────────────────────────────────────────────────

    fn discover_mcp_servers(&self, dir: &Path) -> Map<String, Value> {
        let path = project_mcp_config_path(dir);
        if !path.exists() {
            return Map::new();
        }
        discover_mcp_servers_from_json(&path, "mcpServers", normalise_antigravity_entry)
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
        discover_mcp_servers_from_json(&path, "mcpServers", normalise_antigravity_entry)
    }

    fn discover_global_mcp_entry_names(&self) -> std::collections::HashSet<String> {
        let Some(home) = super::home_dir() else {
            return std::collections::HashSet::new();
        };
        let path = home.join(".gemini").join("config").join("mcp_config.json");
        super::read_global_mcp_entry_names_json(&path, "mcpServers")
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
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
    fn test_mcp_capability_enabled() {
        assert!(Antigravity.capabilities().mcp_servers);
    }

    #[test]
    fn test_mcp_note_is_some() {
        assert!(Antigravity.mcp_note().is_some());
    }

    #[test]
    fn test_owned_config_paths_names_only_mcp_config_json() {
        let dir = tempdir().unwrap();
        let paths = Antigravity.owned_config_paths(dir.path());

        assert_eq!(paths, vec![dir.path().join(".agents/mcp_config.json")]);
        assert!(
            !paths.contains(&dir.path().join(".agents")),
            ".agents/ is the shared skills hub \u{2014} cleanup must never delete it"
        );
    }

    #[test]
    fn test_write_creates_agents_dir_and_writes_mcp_config_json() {
        let dir = tempdir().unwrap();
        let mut servers = Map::new();
        servers.insert(
            "sequential-thinking".to_string(),
            json!({"command": "npx", "args": ["-y", "@modelcontextprotocol/server-sequential-thinking"]}),
        );

        let path = Antigravity
            .write_mcp_config(dir.path(), &servers)
            .expect("write");
        assert_eq!(path, dir.path().join(".agents/mcp_config.json").display().to_string());

        let content = fs::read_to_string(dir.path().join(".agents/mcp_config.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(
            parsed["mcpServers"]["sequential-thinking"]["command"]
                .as_str()
                .unwrap(),
            "npx"
        );
    }

    #[test]
    fn test_write_stdio_strips_type_enabled_timeout() {
        let dir = tempdir().unwrap();
        let mut servers = Map::new();
        servers.insert(
            "srv".to_string(),
            json!({
                "type": "stdio",
                "command": "/usr/local/bin/srv",
                "args": ["--flag"],
                "enabled": true,
                "timeout": 30
            }),
        );

        Antigravity.write_mcp_config(dir.path(), &servers).unwrap();

        let content = fs::read_to_string(dir.path().join(".agents/mcp_config.json")).unwrap();
        let entry = &serde_json::from_str::<Value>(&content).unwrap()["mcpServers"]["srv"];

        assert!(entry.get("type").is_none(), "stdio has no `type` in the vendor format");
        assert!(entry.get("enabled").is_none());
        assert!(entry.get("timeout").is_none());
        assert_eq!(entry["command"].as_str().unwrap(), "/usr/local/bin/srv");
    }

    #[test]
    fn test_write_remote_renames_url_to_server_url_and_drops_type() {
        let dir = tempdir().unwrap();
        let mut servers = Map::new();
        servers.insert(
            "linear".to_string(),
            json!({
                "type": "http",
                "url": "https://mcp.linear.app/mcp",
                "headers": { "X-Client": "automatic" }
            }),
        );

        Antigravity.write_mcp_config(dir.path(), &servers).unwrap();

        let content = fs::read_to_string(dir.path().join(".agents/mcp_config.json")).unwrap();
        let entry = &serde_json::from_str::<Value>(&content).unwrap()["mcpServers"]["linear"];

        assert!(entry.get("type").is_none(), "Antigravity has no `type` \u{2014} `serverUrl` marks remote");
        assert!(entry.get("url").is_none(), "`url` must be renamed to `serverUrl`");
        assert!(entry.get("httpUrl").is_none(), "`httpUrl` is a different vendor's key");
        assert_eq!(entry["serverUrl"].as_str().unwrap(), "https://mcp.linear.app/mcp");
        assert_eq!(entry["headers"]["X-Client"].as_str().unwrap(), "automatic");
    }

    #[test]
    fn test_write_overwrites_existing_file() {
        // Owned file — no merging semantics.  A second sync must produce a
        // file that reflects the second server list exactly.
        let dir = tempdir().unwrap();
        let mut first = Map::new();
        first.insert("a".to_string(), json!({"command": "a"}));
        Antigravity.write_mcp_config(dir.path(), &first).unwrap();

        let mut second = Map::new();
        second.insert("b".to_string(), json!({"command": "b"}));
        Antigravity.write_mcp_config(dir.path(), &second).unwrap();

        let content = fs::read_to_string(dir.path().join(".agents/mcp_config.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert!(parsed["mcpServers"].get("a").is_none());
        assert_eq!(parsed["mcpServers"]["b"]["command"].as_str().unwrap(), "b");
    }

    #[test]
    fn test_discover_normalises_server_url_back_to_url_with_http_type() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".agents")).unwrap();
        fs::write(
            dir.path().join(".agents/mcp_config.json"),
            r#"{
              "mcpServers": {
                "local":  { "command": "srv", "args": ["--x"] },
                "remote": { "serverUrl": "https://example/mcp", "headers": {"a": "b"} }
              }
            }"#,
        )
        .unwrap();

        let servers = Antigravity.discover_mcp_servers(dir.path());
        assert_eq!(servers["local"]["command"].as_str().unwrap(), "srv");
        assert_eq!(servers["remote"]["url"].as_str().unwrap(), "https://example/mcp");
        assert_eq!(servers["remote"]["type"].as_str().unwrap(), "http");
        assert!(servers["remote"].get("serverUrl").is_none());
    }

    #[test]
    fn test_discover_returns_empty_when_file_absent() {
        let dir = tempdir().unwrap();
        assert!(Antigravity.discover_mcp_servers(dir.path()).is_empty());
    }

    #[test]
    fn test_round_trip_write_then_discover_preserves_shape() {
        let dir = tempdir().unwrap();
        let mut servers = Map::new();
        servers.insert(
            "stdio".to_string(),
            json!({"command": "srv", "args": ["--x"]}),
        );
        servers.insert(
            "remote".to_string(),
            json!({"type": "http", "url": "https://example/mcp"}),
        );

        Antigravity.write_mcp_config(dir.path(), &servers).unwrap();
        let discovered = Antigravity.discover_mcp_servers(dir.path());

        assert_eq!(discovered["stdio"]["command"].as_str().unwrap(), "srv");
        assert_eq!(discovered["remote"]["url"].as_str().unwrap(), "https://example/mcp");
        assert_eq!(discovered["remote"]["type"].as_str().unwrap(), "http");
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
