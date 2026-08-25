use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, Agent};

/// Gemini CLI agent — writes MCP servers into `.gemini/settings.json`
/// under the `mcpServers` key, preserving other settings.  Stores skills
/// under `<project>/.agents/skills/<name>/SKILL.md`.
pub struct GeminiCli;

impl Agent for GeminiCli {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "gemini"
    }

    fn label(&self) -> &'static str {
        "Gemini CLI (Beta)"
    }

    fn config_description(&self) -> &'static str {
        ".gemini/settings.json"
    }

    fn project_file_name(&self) -> &'static str {
        "GEMINI.md"
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_in(&self, dir: &Path) -> bool {
        dir.join("GEMINI.md").exists()
            || dir.join(".gemini").join("settings.json").exists()
            || dir.join(".gemini").exists()
            || dir.join(".gemini").join("commands").exists()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".agents").join("skills")]
    }

    fn capabilities(&self) -> super::AgentCapabilities {
        super::AgentCapabilities {
            commands: true,
            hooks: true,
            global_mcp_servers: true,
            ..Default::default()
        }
    }

    fn commands_dir(&self, dir: &Path) -> Option<PathBuf> {
        Some(dir.join(".gemini").join("commands"))
    }

    fn commands_file_ext(&self) -> &'static str {
        "toml"
    }

    fn convert_command_content(&self, content: &str, _name: &str) -> String {
        convert_md_command_to_gemini_toml(content)
    }

    fn hook_events(&self) -> &'static [&'static str] {
        GEMINI_CLI_EVENTS
    }

    fn sync_hooks(
        &self,
        project_dir: &Path,
        hooks: &[crate::core::Hook],
    ) -> Result<Vec<String>, String> {
        sync_gemini_hooks(project_dir, hooks)
    }

    fn hook_config_target(&self, dir: &Path) -> Option<super::HookConfigTarget> {
        Some(super::HookConfigTarget::Merged {
            path: dir.join(".gemini").join("settings.json"),
            key: "hooks",
        })
    }

    // ── Config writing ──────────────────────────────────────────────────

    fn mcp_merge_inputs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".gemini").join("settings.json")]
    }

    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String> {
        // Gemini CLI stores MCP servers in .gemini/settings.json under the
        // "mcpServers" key.  We must merge with existing settings to avoid
        // clobbering auth or model config.
        let gemini_dir = dir.join(".gemini");
        if !gemini_dir.exists() {
            fs::create_dir_all(&gemini_dir)
                .map_err(|e| format!("Failed to create .gemini/: {}", e))?;
        }

        let path = gemini_dir.join("settings.json");

        // Read existing settings.  A file we cannot parse is an error rather
        // than an empty starting point: the user's auth and model config lives
        // here, and writing over it would destroy the lot.
        let mut root = super::read_mergeable_json_object(&path)?;

        // Build the mcpServers object — Gemini uses the same format as
        // Claude Code (command/args/env, no "type" for stdio).
        let mut gemini_servers = Map::new();

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
            gemini_servers.insert(name.clone(), server);
        }

        root.insert("mcpServers".to_string(), Value::Object(gemini_servers));

        let content = serde_json::to_string_pretty(&Value::Object(root))
            .map_err(|e| format!("JSON error: {}", e))?;
        fs::write(&path, content)
            .map_err(|e| format!("Failed to write .gemini/settings.json: {}", e))?;

        Ok(path.display().to_string())
    }

    fn global_mcp_target(&self) -> Option<super::GlobalMcpTarget> {
        let home = super::home_dir()?;
        Some(super::GlobalMcpTarget {
            path: home.join(".gemini").join("settings.json"),
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
                "Home directory not available for Gemini CLI global MCP write".to_string(),
            );
        };

        // Mirror the project writer's entry dialect: strip type/enabled/timeout
        // for stdio entries; leave http/sse otherwise alone.  The shared merge
        // helper preserves every other top-level settings key.
        let mut rendered = Map::new();
        for (name, config) in desired {
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
            rendered.insert(name.clone(), server);
        }

        super::merge_global_mcp_entries_json(&target.path, "mcpServers", &rendered, previously_managed)
    }

    // ── Cleanup ─────────────────────────────────────────────────────────

    /// Gemini CLI merges into `.gemini/settings.json` which may contain user
    /// auth or model settings.  Strip only the `mcpServers` key rather than
    /// deleting the whole file.
    fn cleanup_mcp_config(&self, dir: &Path) -> Vec<String> {
        let path = dir.join(".gemini").join("settings.json");
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
            // Nothing to remove
            return vec![];
        }
        if root.is_empty() {
            // File would become `{}` — delete it entirely
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
        let path = dir.join(".gemini").join("settings.json");
        if path.exists() {
            vec![path.display().to_string()]
        } else {
            vec![]
        }
    }

    // ── Discovery ───────────────────────────────────────────────────────

    fn discover_mcp_servers(&self, dir: &Path) -> Map<String, Value> {
        let path = dir.join(".gemini").join("settings.json");
        if !path.exists() {
            return Map::new();
        }
        discover_mcp_servers_from_json(&path, "mcpServers", identity)
    }

    fn detect_global_install(&self) -> bool {
        super::cli_available("gemini")
            || super::home_dir()
                .map(|h| h.join(".gemini").exists())
                .unwrap_or(false)
    }

    fn discover_global_mcp_servers(&self) -> Map<String, Value> {
        let Some(home) = super::home_dir() else {
            return Map::new();
        };
        // ~/.gemini/settings.json — user-level Gemini CLI config
        let path = home.join(".gemini").join("settings.json");
        discover_mcp_servers_from_json(&path, "mcpServers", identity)
    }

    fn agents_dir(&self, dir: &Path) -> Option<PathBuf> {
        Some(dir.join(".gemini").join("agents"))
    }
}

fn convert_md_command_to_gemini_toml(content: &str) -> String {
    let (frontmatter, body) = super::parse_frontmatter(content);
    let mut toml = String::from("automatic_managed = true\n");

    if let Some(description) = frontmatter.get("description") {
        toml.push_str(&format!(
            "description = \"{}\"\n",
            escape_toml_string(description)
        ));
    }

    toml.push_str(&format!("prompt = \"\"\"\n{}\n\"\"\"\n", body.trim()));

    toml
}

fn escape_toml_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Pass-through normaliser: Gemini's format is already canonical.
fn identity(v: Value) -> Value {
    v
}

// ── Hooks ────────────────────────────────────────────────────────────────────
//
// Gemini CLI merges hooks into the `hooks` key of the same
// `.gemini/settings.json` that `write_mcp_config` already merges `mcpServers`
// into — both read-modify-write the same file independently, which is safe
// since each only ever touches its own top-level key.
//
// Gemini's handler `timeout` is milliseconds; `core::Hook::timeout_sec` is
// seconds. That's the one thing standard_command_handler can't do, so this
// vendor needs its own handler builder.
//
// The documented group shape also carries an optional `sequential` flag.
// Automatic has no hook-ordering concept in `core::Hook` to derive it from,
// and the docs treat it as optional, so it is omitted rather than guessed —
// `no_group_extras` leaves it out entirely.

const GEMINI_CLI_EVENTS: &[&str] = &[
    "BeforeTool",
    "AfterTool",
    "BeforeAgent",
    "AfterAgent",
    "BeforeModel",
    "BeforeToolSelection",
    "AfterModel",
    "SessionStart",
    "SessionEnd",
    "Notification",
    "PreCompress",
];

fn gemini_handler(hook: &crate::core::Hook, command: &str) -> Value {
    let mut handler = Map::new();
    handler.insert("type".to_string(), Value::String("command".to_string()));
    handler.insert("command".to_string(), Value::String(command.to_string()));
    if let Some(timeout_sec) = hook.timeout_sec {
        handler.insert(
            "timeout".to_string(),
            Value::Number(serde_json::Number::from(u64::from(timeout_sec) * 1000)),
        );
    }
    Value::Object(handler)
}

fn sync_gemini_hooks(
    project_dir: &Path,
    hooks: &[crate::core::Hook],
) -> Result<Vec<String>, String> {
    let settings_path = project_dir.join(".gemini").join("settings.json");
    let spec = super::HookWriteSpec {
        supported_events: GEMINI_CLI_EVENTS,
        scripts_dir: project_dir.join(".gemini").join("hooks"),
        // No vendor documentation on absolute-vs-relative script paths for
        // Gemini; mirrors Codex CLI's relative-from-project-root convention
        // for a sibling CLI tool rather than guessing at an env var Gemini
        // may not expand.
        script_command: |file_name| format!("./.gemini/hooks/{}", file_name),
        handler: gemini_handler,
        group_extras: super::no_group_extras,
    };
    super::merge_hooks_into_json_settings(&settings_path, "hooks", hooks, &spec)
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

    #[test]
    fn test_detect() {
        let dir = tempdir().unwrap();
        assert!(!GeminiCli.detect_in(dir.path()));

        fs::write(dir.path().join("GEMINI.md"), "").unwrap();
        assert!(GeminiCli.detect_in(dir.path()));
    }

    #[test]
    fn test_write_preserves_existing_settings() {
        let dir = tempdir().unwrap();
        let gemini_dir = dir.path().join(".gemini");
        fs::create_dir_all(&gemini_dir).unwrap();

        // Write existing settings with non-MCP keys
        let existing = json!({
            "theme": "dark",
            "mcpServers": { "old": { "command": "old" } }
        });
        fs::write(
            gemini_dir.join("settings.json"),
            serde_json::to_string_pretty(&existing).unwrap(),
        )
        .unwrap();

        GeminiCli
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();

        let content = fs::read_to_string(gemini_dir.join("settings.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        // Existing non-MCP settings preserved
        assert_eq!(parsed["theme"].as_str().unwrap(), "dark");
        // MCP servers replaced
        assert!(parsed["mcpServers"]["automatic"]["command"].is_string());
        assert!(parsed["mcpServers"]["old"].is_null());
    }

    #[test]
    fn test_write_creates_dir() {
        let dir = tempdir().unwrap();
        GeminiCli
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join(".gemini/settings.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert!(parsed["mcpServers"]["automatic"]["command"]
            .as_str()
            .unwrap()
            .contains("automatic"));
    }

    // ── Hook sync ───────────────────────────────────────────────────────────

    fn cmd_hook(name: &str, event: &str, command: &str) -> crate::core::Hook {
        crate::core::Hook {
            name: name.to_string(),
            agent: "gemini".to_string(),
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
    fn hook_events_declares_eleven_events() {
        assert_eq!(GeminiCli.hook_events().len(), 11);
    }

    #[test]
    fn hook_sync_converts_timeout_to_milliseconds() {
        let dir = tempdir().unwrap();
        let mut hook = cmd_hook("ping", "SessionStart", "echo hi");
        hook.timeout_sec = Some(5);

        GeminiCli.sync_hooks(dir.path(), &[hook]).unwrap();

        let raw = fs::read_to_string(dir.path().join(".gemini/settings.json")).unwrap();
        let v: Value = serde_json::from_str(&raw).unwrap();
        let handler = &v["hooks"]["SessionStart"][0]["hooks"][0];
        assert_eq!(handler["timeout"], 5000);
    }

    #[test]
    fn hook_sync_preserves_mcp_servers() {
        let dir = tempdir().unwrap();
        GeminiCli
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();

        let hooks = vec![cmd_hook("notify", "Notification", "echo hi")];
        GeminiCli.sync_hooks(dir.path(), &hooks).unwrap();

        let raw = fs::read_to_string(dir.path().join(".gemini/settings.json")).unwrap();
        let v: Value = serde_json::from_str(&raw).unwrap();
        assert!(v["mcpServers"]["automatic"]["command"].is_string());
        assert!(v["hooks"]["Notification"].is_array());
    }

    #[test]
    fn hook_sync_is_idempotent_across_repeats() {
        let dir = tempdir().unwrap();
        let hooks = vec![cmd_hook("compress", "PreCompress", "echo a")];
        GeminiCli.sync_hooks(dir.path(), &hooks).unwrap();
        GeminiCli.sync_hooks(dir.path(), &hooks).unwrap();

        let raw = fs::read_to_string(dir.path().join(".gemini/settings.json")).unwrap();
        let v: Value = serde_json::from_str(&raw).unwrap();
        let groups = v["hooks"]["PreCompress"].as_array().unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0]["hooks"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn hook_sync_malformed_settings_is_an_error_not_a_clobber() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".gemini")).unwrap();
        fs::write(dir.path().join(".gemini/settings.json"), "{ not json").unwrap();

        let hooks = vec![cmd_hook("a", "SessionStart", "echo a")];
        let result = GeminiCli.sync_hooks(dir.path(), &hooks);
        assert!(result.is_err());
    }
}
