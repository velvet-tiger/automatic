use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, Agent};

/// Claude Code agent — writes `.mcp.json` and stores skills under
/// `<project>/.claude/skills/<name>/SKILL.md`.
pub struct ClaudeCode;

impl Agent for ClaudeCode {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "claude"
    }

    fn label(&self) -> &'static str {
        "Claude Code"
    }

    fn config_description(&self) -> &'static str {
        ".mcp.json"
    }

    fn project_file_name(&self) -> &'static str {
        "CLAUDE.md"
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_in(&self, dir: &Path) -> bool {
        dir.join("CLAUDE.md").exists()
            || dir.join(".mcp.json").exists()
            || dir.join(".claude").join("settings.json").exists()
            || dir.join(".claude").join("skills").exists()
            || dir.join(".claude").join("commands").exists()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".claude").join("skills")]
    }

    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String> {
        // Claude Code uses Automatic's JSON format directly, with one tweak:
        // strip "type" from stdio entries for Claude Desktop backward-compat.
        let mut claude_servers = Map::new();

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
            claude_servers.insert(name.clone(), server);
        }

        let output = json!({ "mcpServers": Value::Object(claude_servers) });
        let path = dir.join(".mcp.json");
        let content =
            serde_json::to_string_pretty(&output).map_err(|e| format!("JSON error: {}", e))?;
        write_file_atomic(&path, content.as_bytes())
            .map_err(|e| format!("Failed to write .mcp.json: {}", e))?;

        Ok(path.display().to_string())
    }

    fn sync_instruction_rules(
        &self,
        project: &crate::core::Project,
        filename: &str,
        rule_names: &[String],
        custom_contents: &[String],
    ) -> Result<Option<Vec<String>>, String> {
        let opts = project
            .agent_options
            .get(self.id())
            .cloned()
            .unwrap_or_default();
        if !opts.claude_rules_in_dot_claude {
            return Ok(None);
        }

        let mut touched = Vec::new();
        if !rule_names.is_empty() {
            touched.extend(crate::core::sync_rules_to_dot_claude_rules(
                &project.directory,
                rule_names,
            )?);
        }

        if crate::core::inject_rules_into_project_file_with_custom(
            &project.directory,
            filename,
            &[],
            custom_contents,
        )? {
            touched.push(
                PathBuf::from(&project.directory)
                    .join(filename)
                    .display()
                    .to_string(),
            );
        }

        Ok(Some(touched))
    }

    // ── Cleanup ─────────────────────────────────────────────────────────

    fn capabilities(&self) -> super::AgentCapabilities {
        super::AgentCapabilities {
            commands: true,
            hooks: true,
            ..Default::default()
        }
    }

    fn hook_events(&self) -> &'static [&'static str] {
        CLAUDE_CODE_EVENTS
    }

    fn sync_hooks(
        &self,
        project_dir: &Path,
        hooks: &[crate::core::Hook],
    ) -> Result<Vec<String>, String> {
        sync_claude_code_hooks(project_dir, hooks)
    }

    fn hook_config_target(&self, dir: &Path) -> Option<super::HookConfigTarget> {
        Some(super::HookConfigTarget::Merged {
            path: dir.join(".claude").join("settings.json"),
            key: "hooks",
        })
    }

    // ── Config writing ──────────────────────────────────────────────────

    fn discover_mcp_servers(&self, dir: &Path) -> Map<String, Value> {
        let path = dir.join(".mcp.json");
        if !path.exists() {
            return Map::new();
        }
        // Claude's format is already canonical — no normalisation needed.
        discover_mcp_servers_from_json(&path, "mcpServers", identity)
    }

    fn detect_global_install(&self) -> bool {
        // The `claude` binary on PATH, or the ~/.claude config directory.
        super::cli_available("claude")
            || super::home_dir()
                .map(|h| h.join(".claude").exists())
                .unwrap_or(false)
    }

    fn discover_global_mcp_servers(&self) -> Map<String, Value> {
        let Some(home) = super::home_dir() else {
            return Map::new();
        };

        // ~/.claude.json is the single source of truth for user-scoped MCP
        // servers in Claude Code.  The top-level `mcpServers` object holds
        // servers added with `claude mcp add --scope user`.
        //
        // Note: local-scoped servers (default scope / `--scope local`) live
        // under `projects["<abs-path>"]["mcpServers"]` in the same file,
        // keyed by absolute project path.  We don't read those here because
        // this method has no project-path context — they are project-specific
        // and would pollute every other project's import list if surfaced
        // globally.
        discover_claude_global_config(&home.join(".claude.json"))
    }

    // ── Discovery ───────────────────────────────────────────────────────

    fn agents_dir(&self, dir: &Path) -> Option<PathBuf> {
        Some(dir.join(".claude").join("agents"))
    }

    fn commands_dir(&self, dir: &Path) -> Option<PathBuf> {
        Some(dir.join(".claude").join("commands"))
    }

    fn owned_config_paths(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".mcp.json")]
    }
}

/// Read user-scoped MCP servers from Claude Code's `~/.claude.json`.
///
/// Claude Code stores MCP server configs at three scopes inside this file:
/// - User scope (`--scope user`): top-level `mcpServers` object
/// - Local scope (default / `--scope local`): `projects["<abs-path>"]["mcpServers"]`
///
/// This function reads only the top-level `mcpServers` — the user-scoped
/// entries that apply across all projects.  Per-project local-scope entries
/// are intentionally excluded because they are project-specific and we have
/// no project path context here.
fn discover_claude_global_config(path: &Path) -> Map<String, Value> {
    discover_mcp_servers_from_json(path, "mcpServers", identity)
}

/// Pass-through normaliser: Claude's format is already canonical.
fn identity(v: Value) -> Value {
    v
}

// ── Hooks ────────────────────────────────────────────────────────────────────
//
// Claude Code reads hooks from `.claude/settings.json` under the top-level
// `hooks` key. Each event maps to an array of matcher groups, and each
// matcher group has an array of handlers.
// `super::merge_hooks_into_json_settings` merges our entries into any
// pre-existing settings without disturbing keys the user owns (model,
// permissions, env, …) or hook entries the user wrote by hand, tagging every
// handler it emits with `_managedBy: "automatic"` and `_hookId: "<machine-
// name>"` so the next sync can tell managed and user-authored handlers
// apart. Claude Code ignores unknown JSON fields, so the tags are inert at
// runtime.

/// Every hook event Claude Code documents. Source:
/// <https://code.claude.com/docs/en/hooks>.
///
/// This drives the event picker in the Hooks UI via [`Agent::hook_events`].
/// `sync_claude_code_hooks` deliberately does not filter by this list — see
/// the note on that trait method. A user who knows about a new Claude event
/// before this list is updated must not silently lose their hook.
const CLAUDE_CODE_EVENTS: &[&str] = &[
    "SessionStart",
    "Setup",
    "SessionEnd",
    "UserPromptSubmit",
    "UserPromptExpansion",
    "Stop",
    "StopFailure",
    "PreToolUse",
    "PermissionRequest",
    "PermissionDenied",
    "PostToolUse",
    "PostToolUseFailure",
    "PostToolBatch",
    "SubagentStart",
    "SubagentStop",
    "TeammateIdle",
    "TaskCreated",
    "TaskCompleted",
    "FileChanged",
    "CwdChanged",
    "ConfigChange",
    "InstructionsLoaded",
    "PreCompact",
    "PostCompact",
    "Elicitation",
    "ElicitationResult",
    "Notification",
    "MessageDisplay",
    "WorktreeCreate",
    "WorktreeRemove",
];

fn sync_claude_code_hooks(
    project_dir: &Path,
    hooks: &[crate::core::Hook],
) -> Result<Vec<String>, String> {
    let settings_path = project_dir.join(".claude").join("settings.json");
    let spec = super::HookWriteSpec {
        supported_events: CLAUDE_CODE_EVENTS,
        scripts_dir: project_dir.join(".claude").join("hooks"),
        // Reference the script via ${CLAUDE_PROJECT_DIR} so the settings
        // file is portable across machines / containers.
        script_command: |file_name| format!("${{CLAUDE_PROJECT_DIR}}/.claude/hooks/{}", file_name),
        handler: super::standard_command_handler,
        group_extras: super::no_group_extras,
    };
    super::merge_hooks_into_json_settings(&settings_path, "hooks", hooks, &spec)
}

/// Writes `content` to `final_path` via a temp file + rename, so a crash or
/// kill mid-write can never leave `.mcp.json` truncated or corrupt — which
/// would otherwise drop every configured MCP server for the project, not
/// just the one being synced.
fn write_file_atomic(final_path: &Path, content: &[u8]) -> std::io::Result<()> {
    let mut tmp_name = final_path.as_os_str().to_os_string();
    tmp_name.push(".tmp");
    let tmp_path = PathBuf::from(tmp_name);

    // Remove a stale tmp left behind by a previous crash, if any.
    let _ = fs::remove_file(&tmp_path);

    {
        let mut tmp = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)?;
        std::io::Write::write_all(&mut tmp, content)?;
        tmp.sync_all()?;
    } // file handle closed before rename

    fs::rename(&tmp_path, final_path).inspect_err(|_| {
        // Best-effort cleanup so a failed rename doesn't leave litter.
        let _ = fs::remove_file(&tmp_path);
    })
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn hook_events_declares_all_thirty_events_including_message_display() {
        let events = ClaudeCode.hook_events();
        assert_eq!(
            events.len(),
            30,
            "expected 30 documented Claude Code hook events, found {}",
            events.len()
        );
        assert!(
            events.contains(&"MessageDisplay"),
            "MessageDisplay is documented but missing from CLAUDE_CODE_EVENTS"
        );
    }

    fn stdio_servers() -> Map<String, Value> {
        let mut s = Map::new();
        s.insert(
            "automatic".to_string(),
            json!({"type":"stdio","command":"/usr/local/bin/automatic","args":["mcp-serve"]}),
        );
        s.insert(
            "github".to_string(),
            json!({"type":"stdio","command":"npx","args":["-y","@modelcontextprotocol/server-github"],"env":{"GITHUB_TOKEN":"ghp_test123"}}),
        );
        s
    }

    fn http_servers() -> Map<String, Value> {
        let mut s = Map::new();
        s.insert(
            "remote-api".to_string(),
            json!({"type":"http","url":"https://api.example.com/mcp","headers":{"Authorization":"Bearer tok_abc123"},"oauth":{"clientId":"client_123","scope":"read"}}),
        );
        s
    }

    #[test]
    fn test_detect() {
        let dir = tempdir().unwrap();
        assert!(!ClaudeCode.detect_in(dir.path()));

        fs::write(dir.path().join("CLAUDE.md"), "# Claude").unwrap();
        assert!(ClaudeCode.detect_in(dir.path()));

        fs::remove_file(dir.path().join("CLAUDE.md")).unwrap();
        fs::write(dir.path().join(".mcp.json"), "{}").unwrap();
        assert!(ClaudeCode.detect_in(dir.path()));
    }

    #[test]
    fn test_write_stdio() {
        let dir = tempdir().unwrap();
        ClaudeCode
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join(".mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        // stdio entries should have "type" stripped
        assert!(parsed["mcpServers"]["automatic"]["type"].is_null());
        assert!(parsed["mcpServers"]["automatic"]["command"]
            .as_str()
            .unwrap()
            .contains("automatic"));
        assert_eq!(
            parsed["mcpServers"]["github"]["command"].as_str().unwrap(),
            "npx"
        );
    }

    #[test]
    fn test_write_http() {
        let dir = tempdir().unwrap();
        ClaudeCode
            .write_mcp_config(dir.path(), &http_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join(".mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(
            parsed["mcpServers"]["remote-api"]["type"].as_str().unwrap(),
            "http"
        );
        assert_eq!(
            parsed["mcpServers"]["remote-api"]["url"].as_str().unwrap(),
            "https://api.example.com/mcp"
        );
        assert!(
            parsed["mcpServers"]["remote-api"]["headers"]["Authorization"]
                .as_str()
                .is_some()
        );
        assert_eq!(
            parsed["mcpServers"]["remote-api"]["oauth"]["clientId"]
                .as_str()
                .unwrap(),
            "client_123"
        );
    }

    #[test]
    fn test_write_mcp_config_leaves_no_tmp_file_and_overwrites_cleanly() {
        let dir = tempdir().unwrap();
        ClaudeCode
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();
        ClaudeCode
            .write_mcp_config(dir.path(), &http_servers())
            .unwrap();

        assert!(!dir.path().join(".mcp.json.tmp").exists());
        let content = fs::read_to_string(dir.path().join(".mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert!(parsed["mcpServers"]["remote-api"].is_object());
        assert!(parsed["mcpServers"]["automatic"].is_null());
    }

    #[test]
    fn test_write_mcp_config_recovers_from_stale_tmp_file() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join(".mcp.json.tmp"), "leftover from a crash").unwrap();

        ClaudeCode
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();

        assert!(!dir.path().join(".mcp.json.tmp").exists());
        let content = fs::read_to_string(dir.path().join(".mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert!(parsed["mcpServers"]["automatic"]["command"].is_string());
    }

    // ── discover_claude_global_config tests ─────────────────────────────────

    #[test]
    fn test_discover_global_missing_file() {
        let dir = tempdir().unwrap();
        // No ~/.claude.json — should return empty map, not panic.
        let result = discover_claude_global_config(&dir.path().join(".claude.json"));
        assert!(result.is_empty());
    }

    #[test]
    fn test_discover_global_user_scoped_stdio() {
        // Simulates `claude mcp add --scope user my-server -- npx -y @foo/bar`
        let dir = tempdir().unwrap();
        let claude_json = json!({
            "numStartups": 5,
            "mcpServers": {
                "github": {
                    "command": "npx",
                    "args": ["-y", "@modelcontextprotocol/server-github"],
                    "env": { "GITHUB_TOKEN": "ghp_test" }
                }
            },
            "projects": {}
        });
        fs::write(
            dir.path().join(".claude.json"),
            serde_json::to_string(&claude_json).unwrap(),
        )
        .unwrap();

        let result = discover_claude_global_config(&dir.path().join(".claude.json"));

        assert!(
            result.contains_key("github"),
            "should find user-scoped server"
        );
        assert_eq!(result["github"]["command"].as_str().unwrap(), "npx");
        assert_eq!(result["github"]["args"][0].as_str().unwrap(), "-y");
        assert_eq!(
            result["github"]["env"]["GITHUB_TOKEN"].as_str().unwrap(),
            "ghp_test"
        );
    }

    #[test]
    fn test_discover_global_user_scoped_http() {
        // Simulates `claude mcp add --scope user --transport http sentry https://mcp.sentry.dev/mcp`
        let dir = tempdir().unwrap();
        let claude_json = json!({
            "mcpServers": {
                "sentry": {
                    "type": "http",
                    "url": "https://mcp.sentry.dev/mcp"
                }
            }
        });
        fs::write(
            dir.path().join(".claude.json"),
            serde_json::to_string(&claude_json).unwrap(),
        )
        .unwrap();

        let result = discover_claude_global_config(&dir.path().join(".claude.json"));

        assert!(result.contains_key("sentry"));
        assert_eq!(result["sentry"]["type"].as_str().unwrap(), "http");
        assert_eq!(
            result["sentry"]["url"].as_str().unwrap(),
            "https://mcp.sentry.dev/mcp"
        );
    }

    #[test]
    fn test_discover_global_automatic_server_skipped() {
        // The "automatic" entry must always be injected fresh at sync time —
        // it must never be imported from an existing config.
        let dir = tempdir().unwrap();
        let claude_json = json!({
            "mcpServers": {
                "automatic": {
                    "command": "/old/path/to/nexus",
                    "args": ["mcp-serve"]
                },
                "github": {
                    "command": "npx",
                    "args": ["-y", "@modelcontextprotocol/server-github"]
                }
            }
        });
        fs::write(
            dir.path().join(".claude.json"),
            serde_json::to_string(&claude_json).unwrap(),
        )
        .unwrap();

        let result = discover_claude_global_config(&dir.path().join(".claude.json"));

        assert!(
            !result.contains_key("automatic"),
            "automatic server must be filtered out"
        );
        assert!(
            result.contains_key("github"),
            "other servers should be kept"
        );
    }

    // ── Hook sync ───────────────────────────────────────────────────────────

    fn cmd_hook(
        name: &str,
        event: &str,
        matcher: Option<&str>,
        command: &str,
    ) -> crate::core::Hook {
        crate::core::Hook {
            name: name.to_string(),
            agent: "claude".to_string(),
            event: event.to_string(),
            matcher: matcher.map(|s| s.to_string()),
            handler: crate::core::HookHandler::Command {
                command: command.to_string(),
            },
            timeout_sec: None,
            plugin_id: None,
            _author: None,
        }
    }

    fn path_hook(name: &str, event: &str, path: &str) -> crate::core::Hook {
        crate::core::Hook {
            name: name.to_string(),
            agent: "claude".to_string(),
            event: event.to_string(),
            matcher: None,
            handler: crate::core::HookHandler::Path {
                path: path.to_string(),
                interpreter: None,
            },
            timeout_sec: None,
            plugin_id: None,
            _author: None,
        }
    }

    fn script_hook(name: &str, event: &str, interpreter: &str, script: &str) -> crate::core::Hook {
        crate::core::Hook {
            name: name.to_string(),
            agent: "claude".to_string(),
            event: event.to_string(),
            matcher: None,
            handler: crate::core::HookHandler::Script {
                interpreter: interpreter.to_string(),
                script: script.to_string(),
            },
            timeout_sec: Some(60),
            plugin_id: None,
            _author: None,
        }
    }

    #[test]
    fn hook_sync_merges_into_empty_settings() {
        let dir = tempdir().unwrap();
        let hooks = vec![cmd_hook(
            "log-session",
            "SessionStart",
            None,
            "echo session started",
        )];

        let written = ClaudeCode.sync_hooks(dir.path(), &hooks).expect("sync");
        let settings_path = dir.path().join(".claude/settings.json");
        assert!(settings_path.exists());
        assert!(written.iter().any(|p| p.ends_with("settings.json")));

        let raw = fs::read_to_string(&settings_path).unwrap();
        let v: Value = serde_json::from_str(&raw).unwrap();
        let groups = v["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(groups.len(), 1);
        let handlers = groups[0]["hooks"].as_array().unwrap();
        assert_eq!(handlers.len(), 1);
        assert_eq!(handlers[0]["type"], "command");
        assert_eq!(handlers[0]["command"], "echo session started");
        assert_eq!(handlers[0]["_managedBy"], "automatic");
        assert_eq!(handlers[0]["_hookId"], "log-session");
    }

    #[test]
    fn hook_sync_preserves_unrelated_settings_keys() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".claude")).unwrap();
        let preexisting = json!({
            "model": "claude-opus-4-7",
            "permissions": { "allow": ["Bash(npm test)"] }
        });
        fs::write(
            dir.path().join(".claude/settings.json"),
            serde_json::to_string_pretty(&preexisting).unwrap(),
        )
        .unwrap();

        let hooks = vec![cmd_hook("ping", "Stop", None, "echo bye")];
        ClaudeCode.sync_hooks(dir.path(), &hooks).expect("sync");

        let raw = fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
        let v: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["model"], "claude-opus-4-7");
        assert_eq!(v["permissions"]["allow"][0], "Bash(npm test)");
        assert!(v["hooks"]["Stop"].is_array());
    }

    #[test]
    fn hook_sync_preserves_user_written_hooks_in_same_event() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".claude")).unwrap();
        let preexisting = json!({
            "hooks": {
                "SessionStart": [{
                    "hooks": [{
                        "type": "command",
                        "command": "echo user-owned"
                    }]
                }]
            }
        });
        fs::write(
            dir.path().join(".claude/settings.json"),
            serde_json::to_string_pretty(&preexisting).unwrap(),
        )
        .unwrap();

        let hooks = vec![cmd_hook("managed", "SessionStart", None, "echo managed")];
        ClaudeCode.sync_hooks(dir.path(), &hooks).expect("sync");

        let raw = fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
        let v: Value = serde_json::from_str(&raw).unwrap();
        let handlers = v["hooks"]["SessionStart"][0]["hooks"].as_array().unwrap();
        let commands: Vec<&str> = handlers
            .iter()
            .map(|h| h["command"].as_str().unwrap())
            .collect();
        assert!(
            commands.contains(&"echo user-owned"),
            "user hook removed: {:?}",
            commands
        );
        assert!(
            commands.contains(&"echo managed"),
            "managed hook missing: {:?}",
            commands
        );
    }

    #[test]
    fn hook_sync_is_idempotent_across_repeats() {
        let dir = tempdir().unwrap();
        let hooks = vec![
            cmd_hook("a", "PreToolUse", Some("Bash"), "echo a"),
            cmd_hook("b", "PreToolUse", Some("Edit"), "echo b"),
        ];
        ClaudeCode.sync_hooks(dir.path(), &hooks).unwrap();
        ClaudeCode.sync_hooks(dir.path(), &hooks).unwrap();

        let raw = fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
        let v: Value = serde_json::from_str(&raw).unwrap();
        let groups = v["hooks"]["PreToolUse"].as_array().unwrap();
        // One group per distinct matcher, each with exactly one handler.
        assert_eq!(groups.len(), 2);
        for group in groups {
            assert_eq!(group["hooks"].as_array().unwrap().len(), 1);
        }
    }

    #[test]
    fn hook_sync_removes_managed_entries_when_detached() {
        let dir = tempdir().unwrap();
        let initial = vec![cmd_hook("transient", "Stop", None, "echo before")];
        ClaudeCode.sync_hooks(dir.path(), &initial).unwrap();

        // Re-sync with the hook removed — managed entry should vanish, and
        // because nothing else is left under "Stop", the event key should be
        // pruned, and because nothing's left under "hooks", that key too.
        ClaudeCode.sync_hooks(dir.path(), &[]).unwrap();

        let raw = fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
        let v: Value = serde_json::from_str(&raw).unwrap();
        assert!(v.get("hooks").is_none(), "hooks key not pruned: {:?}", v);
    }

    #[test]
    fn hook_sync_writes_script_file_and_references_it() {
        let dir = tempdir().unwrap();
        let hooks = vec![script_hook(
            "logger",
            "SessionStart",
            "bash",
            "echo $CLAUDE_PROJECT_DIR\n",
        )];
        ClaudeCode.sync_hooks(dir.path(), &hooks).unwrap();

        let script_path = dir.path().join(".claude/hooks/logger.sh");
        assert!(script_path.exists(), "script file not written");
        let body = fs::read_to_string(&script_path).unwrap();
        assert!(body.starts_with("#!/usr/bin/env bash"));
        assert!(body.contains("managed-by-automatic"));
        assert!(body.contains("echo $CLAUDE_PROJECT_DIR"));

        let raw = fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
        let v: Value = serde_json::from_str(&raw).unwrap();
        let handler = &v["hooks"]["SessionStart"][0]["hooks"][0];
        assert_eq!(
            handler["command"].as_str().unwrap(),
            "${CLAUDE_PROJECT_DIR}/.claude/hooks/logger.sh"
        );
        assert_eq!(handler["timeout"], 60);
    }

    #[test]
    fn hook_sync_path_handler_references_user_owned_file() {
        let dir = tempdir().unwrap();
        // User points at a script that already lives in their repo.
        let hooks = vec![path_hook(
            "logger",
            "SessionStart",
            "${CLAUDE_PROJECT_DIR}/scripts/log-session.sh",
        )];
        ClaudeCode.sync_hooks(dir.path(), &hooks).unwrap();

        // Automatic must NOT have created a script file under .claude/hooks/
        // — the user owns that file.
        assert!(!dir.path().join(".claude/hooks").exists());

        let raw = fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
        let v: Value = serde_json::from_str(&raw).unwrap();
        let handler = &v["hooks"]["SessionStart"][0]["hooks"][0];
        assert_eq!(
            handler["command"].as_str().unwrap(),
            "${CLAUDE_PROJECT_DIR}/scripts/log-session.sh"
        );
    }

    #[test]
    fn hook_sync_cleans_orphaned_managed_scripts_only() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".claude/hooks")).unwrap();
        // A user-authored script next to our managed ones — must survive.
        fs::write(
            dir.path().join(".claude/hooks/user-script.sh"),
            "#!/usr/bin/env bash\necho user only\n",
        )
        .unwrap();

        // First sync writes a managed script.
        ClaudeCode
            .sync_hooks(
                dir.path(),
                &[script_hook("temp", "Stop", "bash", "echo temp\n")],
            )
            .unwrap();
        assert!(dir.path().join(".claude/hooks/temp.sh").exists());

        // Re-sync without that hook drops the managed file but keeps user one.
        ClaudeCode.sync_hooks(dir.path(), &[]).unwrap();
        assert!(!dir.path().join(".claude/hooks/temp.sh").exists());
        assert!(dir.path().join(".claude/hooks/user-script.sh").exists());
    }

    #[test]
    fn test_discover_global_local_scoped_not_imported() {
        // Local-scoped servers live under projects["<path>"]["mcpServers"].
        // They must NOT be surfaced by discover_global — they are
        // project-specific and have no meaning outside that project.
        let dir = tempdir().unwrap();
        let claude_json = json!({
            "mcpServers": {
                "user-tool": { "command": "npx", "args": ["-y", "user-tool"] }
            },
            "projects": {
                "/Users/someone/my-project": {
                    "mcpServers": {
                        "local-only-tool": {
                            "command": "npx",
                            "args": ["-y", "local-tool"]
                        }
                    }
                }
            }
        });
        fs::write(
            dir.path().join(".claude.json"),
            serde_json::to_string(&claude_json).unwrap(),
        )
        .unwrap();

        let result = discover_claude_global_config(&dir.path().join(".claude.json"));

        assert!(
            result.contains_key("user-tool"),
            "user-scoped server should be present"
        );
        assert!(
            !result.contains_key("local-only-tool"),
            "local-scoped server must not be imported globally"
        );
    }
}
