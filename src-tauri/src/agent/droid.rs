use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, Agent};

/// Factory.ai Droid agent — writes `.factory/mcp.json` and stores skills
/// under `<project>/.agents/skills/<name>/SKILL.md`.
///
/// ## Project instructions
///
/// Droid reads `AGENTS.md` at the repository root (and any parent directory
/// up to the repo root) before planning any change.  A personal override at
/// `~/.factory/AGENTS.md` is also supported.
///
/// ## MCP config
///
/// Project-level MCP servers are stored in `.factory/mcp.json` under the
/// `mcpServers` key.  User-level servers live in `~/.factory/mcp.json`.
/// Each entry explicitly includes `"type": "stdio"` or `"type": "http"` —
/// unlike most other agents which omit `type` for stdio entries.
///
/// Format example:
/// ```json
/// { "mcpServers": { "linear": { "type": "http", "url": "..." },
///                   "local":  { "type": "stdio", "command": "npx", "args": [...] } } }
/// ```
pub struct Droid;

impl Agent for Droid {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "droid"
    }

    fn label(&self) -> &'static str {
        "Droid (Beta)"
    }

    fn config_description(&self) -> &'static str {
        ".factory/mcp.json"
    }

    fn project_file_name(&self) -> &'static str {
        "AGENTS.md"
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_in(&self, dir: &Path) -> bool {
        dir.join(".factory").join("mcp.json").exists()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".agents").join("skills")]
    }

    // ── Capabilities ────────────────────────────────────────────────────

    fn capabilities(&self) -> super::AgentCapabilities {
        super::AgentCapabilities {
            hooks: true,
            ..Default::default()
        }
    }

    /// Custom droids are Markdown with YAML frontmatter (`name`,
    /// `description`, `model`, `tools`, `reasoningEffort`, `mcpServers`) —
    /// close enough to the canonical sub-agent format that the default
    /// `convert_agent_content` and `agent_file_name` need no override here;
    /// only the directory name differs from Automatic's other Markdown-based
    /// vendors.
    fn agents_dir(&self, dir: &Path) -> Option<PathBuf> {
        Some(dir.join(".factory").join("droids"))
    }

    fn hook_events(&self) -> &'static [&'static str] {
        DROID_EVENTS
    }

    fn sync_hooks(
        &self,
        project_dir: &Path,
        hooks: &[crate::core::Hook],
    ) -> Result<Vec<String>, String> {
        sync_droid_hooks(project_dir, hooks)
    }

    fn hook_config_target(&self, dir: &Path) -> Option<super::HookConfigTarget> {
        Some(super::HookConfigTarget::Owned {
            path: dir.join(".factory").join("hooks.json"),
        })
    }

    // ── Cleanup ─────────────────────────────────────────────────────────

    fn owned_config_paths(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".factory").join("mcp.json")]
    }

    // ── Config writing ──────────────────────────────────────────────────

    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String> {
        // Droid uses mcpServers JSON format in .factory/mcp.json.
        let mut droid_servers = Map::new();

        for (name, config) in servers {
            let transport = config
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("stdio");

            let mut server = config.clone();
            if let Some(obj) = server.as_object_mut() {
                // Droid distinguishes stdio vs http via "type" field
                if transport == "stdio" {
                    obj.insert("type".to_string(), json!("stdio"));
                    obj.remove("enabled");
                    obj.remove("timeout");
                } else {
                    obj.insert("type".to_string(), json!("http"));
                }
            }
            droid_servers.insert(name.clone(), server);
        }

        let output = json!({ "mcpServers": Value::Object(droid_servers) });

        let factory_dir = dir.join(".factory");
        if !factory_dir.exists() {
            fs::create_dir_all(&factory_dir)
                .map_err(|e| format!("Failed to create .factory/: {}", e))?;
        }

        let path = factory_dir.join("mcp.json");
        let content =
            serde_json::to_string_pretty(&output).map_err(|e| format!("JSON error: {}", e))?;
        fs::write(&path, content)
            .map_err(|e| format!("Failed to write .factory/mcp.json: {}", e))?;

        Ok(path.display().to_string())
    }

    // ── Discovery ───────────────────────────────────────────────────────

    fn discover_mcp_servers(&self, dir: &Path) -> Map<String, Value> {
        let path = dir.join(".factory").join("mcp.json");
        if !path.exists() {
            return Map::new();
        }
        discover_mcp_servers_from_json(&path, "mcpServers", normalise_import)
    }

    fn detect_global_install(&self) -> bool {
        super::home_dir()
            .map(|h| h.join(".factory").exists())
            .unwrap_or(false)
    }

    fn discover_global_mcp_servers(&self) -> Map<String, Value> {
        let Some(home) = super::home_dir() else {
            return Map::new();
        };
        // ~/.factory/mcp.json — speculative global config path
        let path = home.join(".factory").join("mcp.json");
        discover_mcp_servers_from_json(&path, "mcpServers", normalise_import)
    }
}

/// Normalise Droid's explicit "type" field to Automatic's canonical format.
/// Droid uses `"type": "stdio"` and `"type": "http"` explicitly.
fn normalise_import(config: Value) -> Value {
    // Droid's format is close to canonical — just pass through.
    config
}

// ── Hooks ────────────────────────────────────────────────────────────────────
//
// Droid owns `.factory/hooks.json` outright; script bodies go in
// `.factory/hooks/`. Droid's optional per-group `commandRegex` has no
// equivalent field in `core::Hook`, so it is never emitted — extending the
// core model for one vendor is out of scope here.

const DROID_EVENTS: &[&str] = &[
    "PreToolUse",
    "PostToolUse",
    "UserPromptSubmit",
    "Notification",
    "Stop",
    "SubagentStop",
    "PreCompact",
    "SessionStart",
    "SessionEnd",
];

fn sync_droid_hooks(
    project_dir: &Path,
    hooks: &[crate::core::Hook],
) -> Result<Vec<String>, String> {
    let factory_dir = project_dir.join(".factory");
    let hooks_file = factory_dir.join("hooks.json");
    let scripts_dir = factory_dir.join("hooks");

    // Factory deprecated `.factory/hooks/hooks.json` in favour of
    // `.factory/hooks.json`. The legacy path now sits inside our own scripts
    // directory, where cleanup_managed_hook_scripts would never touch it —
    // it isn't a script and carries no managed-by-automatic marker. Remove
    // it explicitly so a stale legacy file doesn't shadow the current one.
    let legacy_hooks_file = scripts_dir.join("hooks.json");
    if legacy_hooks_file.exists() {
        if let Err(e) = fs::remove_file(&legacy_hooks_file) {
            eprintln!(
                "[automatic] Failed to remove legacy '{}': {}",
                legacy_hooks_file.display(),
                e
            );
        }
    }

    let spec = super::HookWriteSpec {
        supported_events: DROID_EVENTS,
        scripts_dir,
        script_command: |file_name| format!("./.factory/hooks/{}", file_name),
        handler: super::standard_command_handler,
        group_extras: super::no_group_extras,
    };
    super::write_owned_hooks_file(&hooks_file, hooks, &spec)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    fn stdio_servers() -> Map<String, Value> {
        let mut s = Map::new();
        s.insert(
            "automatic".to_string(),
            json!({"type":"stdio","command":"/usr/local/bin/automatic","args":["mcp-serve"]}),
        );
        s
    }

    fn http_servers() -> Map<String, Value> {
        let mut s = Map::new();
        s.insert(
            "linear".to_string(),
            json!({"type":"http","url":"https://mcp.linear.app/mcp"}),
        );
        s
    }

    #[test]
    fn test_detect() {
        let dir = tempdir().unwrap();
        assert!(!Droid.detect_in(dir.path()));

        fs::create_dir_all(dir.path().join(".factory")).unwrap();
        fs::write(dir.path().join(".factory/mcp.json"), "{}").unwrap();
        assert!(Droid.detect_in(dir.path()));
    }

    #[test]
    fn test_write_stdio() {
        let dir = tempdir().unwrap();
        Droid
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join(".factory/mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(
            parsed["mcpServers"]["automatic"]["type"].as_str().unwrap(),
            "stdio"
        );
        assert!(parsed["mcpServers"]["automatic"]["command"]
            .as_str()
            .unwrap()
            .contains("automatic"));
    }

    #[test]
    fn test_write_http() {
        let dir = tempdir().unwrap();
        Droid.write_mcp_config(dir.path(), &http_servers()).unwrap();

        let content = fs::read_to_string(dir.path().join(".factory/mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(
            parsed["mcpServers"]["linear"]["type"].as_str().unwrap(),
            "http"
        );
    }

    // ── Hook sync ───────────────────────────────────────────────────────────

    fn cmd_hook(name: &str, event: &str, command: &str) -> crate::core::Hook {
        crate::core::Hook {
            name: name.to_string(),
            agent: "droid".to_string(),
            event: event.to_string(),
            matcher: None,
            handler: crate::core::HookHandler::Command {
                command: command.to_string(),
            },
            timeout_sec: None,
            plugin_id: None,
            _author: None,
        }
    }

    #[test]
    fn hook_events_declares_nine_events() {
        assert_eq!(Droid.hook_events().len(), 9);
    }

    #[test]
    fn hook_sync_writes_dedicated_file() {
        let dir = tempdir().unwrap();
        let hooks = vec![cmd_hook("hi", "SessionStart", "echo hi")];
        let written = Droid.sync_hooks(dir.path(), &hooks).unwrap();

        let path = dir.path().join(".factory/hooks.json");
        assert!(path.exists());
        assert!(written.iter().any(|w| w.ends_with("hooks.json")));

        let raw = fs::read_to_string(&path).unwrap();
        let v: Value = serde_json::from_str(&raw).unwrap();
        let handler = &v["hooks"]["SessionStart"][0]["hooks"][0];
        assert_eq!(handler["type"], "command");
        assert_eq!(handler["command"], "echo hi");
    }

    #[test]
    fn hook_sync_removes_legacy_hooks_subdirectory_file() {
        let dir = tempdir().unwrap();
        let legacy_dir = dir.path().join(".factory/hooks");
        fs::create_dir_all(&legacy_dir).unwrap();
        fs::write(legacy_dir.join("hooks.json"), "{}").unwrap();

        Droid.sync_hooks(dir.path(), &[]).unwrap();

        assert!(!legacy_dir.join("hooks.json").exists());
    }
}
