use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, Agent, ManagedPath};

/// GitHub Copilot agent — writes `.vscode/mcp.json` and stores skills under
/// `<project>/.agents/skills/<name>/SKILL.md`.
///
/// GitHub Copilot uses VS Code's MCP configuration format, which stores
/// servers under the `"servers"` key (not `"mcpServers"`).  stdio entries
/// omit the `"type"` field; http entries include `"type": "http"`.
pub struct GitHubCopilot;

impl Agent for GitHubCopilot {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "copilot"
    }

    fn label(&self) -> &'static str {
        "GitHub Copilot (Beta)"
    }

    fn config_description(&self) -> &'static str {
        ".vscode/mcp.json"
    }

    fn project_file_name(&self) -> &'static str {
        ".github/copilot-instructions.md"
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_in(&self, dir: &Path) -> bool {
        dir.join(".github").join("copilot-instructions.md").exists()
            || dir.join(".vscode").join("mcp.json").exists()
            || dir.join(".github").join("prompts").exists()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".agents").join("skills")]
    }

    // ── Capabilities ────────────────────────────────────────────────────

    fn capabilities(&self) -> super::AgentCapabilities {
        super::AgentCapabilities {
            agents: false,
            commands: true,
            hooks: true,
            ..Default::default()
        }
    }

    fn commands_dir(&self, dir: &Path) -> Option<PathBuf> {
        Some(dir.join(".github").join("prompts"))
    }

    fn command_file_name(&self, machine_name: &str) -> String {
        format!("{machine_name}.prompt.md")
    }

    fn hook_events(&self) -> &'static [&'static str] {
        GITHUB_COPILOT_EVENTS
    }

    fn sync_hooks(
        &self,
        project_dir: &Path,
        hooks: &[crate::core::Hook],
    ) -> Result<Vec<String>, String> {
        sync_github_copilot_hooks(project_dir, hooks)
    }

    /// Copilot has no directory of its own — it writes into `.github/` and
    /// `.vscode/`, which belong to GitHub and the editor.  The default would
    /// miss `.vscode/mcp.json` because that file is merged (not in
    /// `owned_config_paths`), so it is listed explicitly here.  The pattern
    /// builder keeps all three surgical rather than ignoring `.github/` or
    /// `.vscode/` wholesale.
    fn managed_gitignore_paths(&self, dir: &Path) -> Vec<ManagedPath> {
        vec![
            ManagedPath {
                path: dir.join(".github").join("copilot-instructions.md"),
                is_dir: false,
            },
            ManagedPath {
                path: dir.join(".github").join("prompts"),
                is_dir: true,
            },
            ManagedPath {
                path: dir.join(".github").join("hooks"),
                is_dir: true,
            },
            ManagedPath {
                path: dir.join(".vscode").join("mcp.json"),
                is_dir: false,
            },
        ]
    }

    // ── Config writing ──────────────────────────────────────────────────

    fn mcp_merge_inputs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".vscode").join("mcp.json")]
    }

    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String> {
        // VS Code / GitHub Copilot uses .vscode/mcp.json with a "servers"
        // key.  We must merge with any existing file to avoid clobbering
        // non-MCP settings.
        let vscode_dir = dir.join(".vscode");
        if !vscode_dir.exists() {
            fs::create_dir_all(&vscode_dir)
                .map_err(|e| format!("Failed to create .vscode/: {}", e))?;
        }

        let path = vscode_dir.join("mcp.json");

        // Read existing config.  A file we cannot parse is an error rather than
        // an empty starting point: `.vscode/mcp.json` is shared with the user's
        // editor config, and writing over it would destroy the lot.
        let mut root = super::read_mergeable_json_object(&path)?;

        // Build the servers object — VS Code format uses "servers" key,
        // stdio entries omit "type", http entries keep "type": "http".
        let mut copilot_servers = Map::new();

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
            copilot_servers.insert(name.clone(), server);
        }

        root.insert("servers".to_string(), Value::Object(copilot_servers));

        let content = serde_json::to_string_pretty(&Value::Object(root))
            .map_err(|e| format!("JSON error: {}", e))?;
        fs::write(&path, content)
            .map_err(|e| format!("Failed to write .vscode/mcp.json: {}", e))?;

        Ok(path.display().to_string())
    }

    // ── Cleanup ─────────────────────────────────────────────────────────

    /// GitHub Copilot merges into `.vscode/mcp.json` which may contain VS Code
    /// extension settings.  Strip only the `servers` key rather than deleting
    /// the file.
    fn cleanup_mcp_config(&self, dir: &Path) -> Vec<String> {
        let path = dir.join(".vscode").join("mcp.json");
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
        if root.remove("servers").is_none() {
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
        let path = dir.join(".vscode").join("mcp.json");
        if path.exists() {
            vec![path.display().to_string()]
        } else {
            vec![]
        }
    }

    // ── Discovery ───────────────────────────────────────────────────────

    fn discover_mcp_servers(&self, dir: &Path) -> Map<String, Value> {
        let path = dir.join(".vscode").join("mcp.json");
        if !path.exists() {
            return Map::new();
        }
        // VS Code uses "servers" key instead of "mcpServers"
        discover_mcp_servers_from_json(&path, "servers", identity)
    }

    fn detect_global_install(&self) -> bool {
        // VS Code or Cursor (which also uses Copilot) must be present.
        std::path::Path::new("/Applications/Visual Studio Code.app").exists()
            || std::path::Path::new("/Applications/Cursor.app").exists()
            || super::cli_available("code")
            || super::home_dir()
                .map(|h| h.join(".vscode").exists())
                .unwrap_or(false)
    }

    fn discover_global_mcp_servers(&self) -> Map<String, Value> {
        let Some(home) = super::home_dir() else {
            return Map::new();
        };
        // ~/.vscode/mcp.json — user-level VS Code MCP config
        let path = home.join(".vscode").join("mcp.json");
        discover_mcp_servers_from_json(&path, "servers", identity)
    }
}

/// Pass-through normaliser: VS Code/Copilot format is close to canonical.
fn identity(v: Value) -> Value {
    v
}

// ── Hooks ────────────────────────────────────────────────────────────────────
//
// GitHub Copilot reads hook config from any `*.json` file under
// `.github/hooks/`. Automatic writes one file it owns outright,
// `.github/hooks/automatic.json`, rather than one file per hook. All 8
// documented events are a strict subset of Claude Code's names, and the
// handler shape follows the same `{type, command, timeout?}` convention
// every other JSON-based vendor in this set uses — Copilot's own docs don't
// spell out a different one.

const GITHUB_COPILOT_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PreCompact",
    "SubagentStart",
    "SubagentStop",
    "Stop",
];

fn sync_github_copilot_hooks(
    project_dir: &Path,
    hooks: &[crate::core::Hook],
) -> Result<Vec<String>, String> {
    let hooks_dir = project_dir.join(".github").join("hooks");
    let hooks_file = hooks_dir.join("automatic.json");
    let spec = super::HookWriteSpec {
        supported_events: GITHUB_COPILOT_EVENTS,
        scripts_dir: hooks_dir,
        script_command: |file_name| format!("./.github/hooks/{}", file_name),
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
            "remote-api".to_string(),
            json!({"type":"http","url":"https://api.example.com/mcp","headers":{"Authorization":"Bearer tok_abc123"}}),
        );
        s
    }

    #[test]
    fn test_detect() {
        let dir = tempdir().unwrap();
        assert!(!GitHubCopilot.detect_in(dir.path()));

        fs::create_dir_all(dir.path().join(".github")).unwrap();
        fs::write(dir.path().join(".github/copilot-instructions.md"), "").unwrap();
        assert!(GitHubCopilot.detect_in(dir.path()));
    }

    #[test]
    fn test_detect_vscode_mcp() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".vscode")).unwrap();
        fs::write(dir.path().join(".vscode/mcp.json"), "{}").unwrap();
        assert!(GitHubCopilot.detect_in(dir.path()));
    }

    #[test]
    fn test_write_stdio() {
        let dir = tempdir().unwrap();
        GitHubCopilot
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join(".vscode/mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        // Uses "servers" key, not "mcpServers"
        assert!(parsed["servers"]["automatic"]["type"].is_null());
        assert!(parsed["servers"]["automatic"]["command"]
            .as_str()
            .unwrap()
            .contains("automatic"));
    }

    #[test]
    fn test_write_http() {
        let dir = tempdir().unwrap();
        GitHubCopilot
            .write_mcp_config(dir.path(), &http_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join(".vscode/mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(
            parsed["servers"]["remote-api"]["type"].as_str().unwrap(),
            "http"
        );
        assert_eq!(
            parsed["servers"]["remote-api"]["url"].as_str().unwrap(),
            "https://api.example.com/mcp"
        );
    }

    #[test]
    fn test_write_preserves_existing_settings() {
        let dir = tempdir().unwrap();
        let vscode_dir = dir.path().join(".vscode");
        fs::create_dir_all(&vscode_dir).unwrap();

        // Write existing config with non-MCP keys
        let existing = json!({
            "inputs": [{ "id": "api-key", "type": "promptString" }],
            "servers": { "old": { "command": "old" } }
        });
        fs::write(
            vscode_dir.join("mcp.json"),
            serde_json::to_string_pretty(&existing).unwrap(),
        )
        .unwrap();

        GitHubCopilot
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();

        let content = fs::read_to_string(vscode_dir.join("mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        // Existing non-server keys preserved
        assert!(parsed["inputs"].is_array());
        // Servers replaced
        assert!(parsed["servers"]["automatic"]["command"].is_string());
        assert!(parsed["servers"]["old"].is_null());
    }

    // ── Hook sync ───────────────────────────────────────────────────────────

    fn cmd_hook(name: &str, event: &str, command: &str) -> crate::core::Hook {
        crate::core::Hook {
            name: name.to_string(),
            agent: "copilot".to_string(),
            event: event.to_string(),
            matcher: None,
            handler: crate::core::HookHandler::Command {
                command: command.to_string(),
            },
            timeout_sec: Some(30),
            plugin_id: None,
            _author: None,
        }
    }

    #[test]
    fn hook_events_declares_eight_events() {
        assert_eq!(GitHubCopilot.hook_events().len(), 8);
    }

    #[test]
    fn hook_sync_writes_one_owned_file() {
        let dir = tempdir().unwrap();
        let hooks = vec![cmd_hook("hi", "SessionStart", "echo hi")];
        let written = GitHubCopilot.sync_hooks(dir.path(), &hooks).unwrap();

        let path = dir.path().join(".github/hooks/automatic.json");
        assert!(path.exists());
        assert!(written.iter().any(|w| w.ends_with("automatic.json")));

        let raw = fs::read_to_string(&path).unwrap();
        let v: Value = serde_json::from_str(&raw).unwrap();
        let handler = &v["hooks"]["SessionStart"][0]["hooks"][0];
        assert_eq!(handler["type"], "command");
        assert_eq!(handler["command"], "echo hi");
        assert_eq!(handler["timeout"], 30);
    }

    #[test]
    fn hook_sync_skips_unsupported_events() {
        let dir = tempdir().unwrap();
        // Claude-only event — Copilot must skip it without failing the sync.
        let hooks = vec![cmd_hook("setup", "Setup", "echo nope")];
        let written = GitHubCopilot.sync_hooks(dir.path(), &hooks).unwrap();
        assert!(written.is_empty());
        assert!(!dir.path().join(".github/hooks/automatic.json").exists());
    }

    #[test]
    fn hook_sync_removes_owned_file_when_hook_set_is_empty() {
        let dir = tempdir().unwrap();
        let hooks = vec![cmd_hook("temp", "Stop", "echo bye")];
        GitHubCopilot.sync_hooks(dir.path(), &hooks).unwrap();
        assert!(dir.path().join(".github/hooks/automatic.json").exists());

        GitHubCopilot.sync_hooks(dir.path(), &[]).unwrap();
        assert!(!dir.path().join(".github/hooks/automatic.json").exists());
    }
}
