use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::Agent;

/// Codex CLI agent — writes `.codex/config.toml` and stores skills under
/// `<project>/.agents/skills/<name>/SKILL.md`.
pub struct CodexCli;

impl Agent for CodexCli {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "codex"
    }

    fn label(&self) -> &'static str {
        "Codex CLI"
    }

    fn config_description(&self) -> &'static str {
        ".codex/config.toml"
    }

    fn project_file_name(&self) -> &'static str {
        "AGENTS.md"
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_in(&self, dir: &Path) -> bool {
        dir.join(".codex").join("config.toml").exists()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".agents").join("skills")]
    }

    // ── Config writing ──────────────────────────────────────────────────

    /// TOML has no variable interpolation and Codex performs none, so a
    /// `${KEY}` placeholder would be handed to the server verbatim.  Codex's
    /// own mechanism is `env_vars`: a list of names to forward from the host
    /// environment, kept out of the `env` table so no value is written at all.
    fn rewrite_inherited_env(&self, server: &mut Map<String, Value>, keys: &[String]) {
        if let Some(Value::Object(env)) = server.get_mut("env") {
            for key in keys {
                env.remove(key);
            }
            if env.is_empty() {
                server.remove("env");
            }
        }

        let mut forwarded: Vec<Value> = server
            .get("env_vars")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for key in keys {
            let entry = Value::String(key.clone());
            if !forwarded.contains(&entry) {
                forwarded.push(entry);
            }
        }
        server.insert("env_vars".to_string(), Value::Array(forwarded));
    }

    fn mcp_merge_inputs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".codex").join("config.toml")]
    }

    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String> {
        let codex_dir = dir.join(".codex");
        if !codex_dir.exists() {
            fs::create_dir_all(&codex_dir)
                .map_err(|e| format!("Failed to create .codex/: {}", e))?;
        }

        let mut toml_content = String::new();

        for (name, config) in servers {
            let config = config.clone();
            let transport = config
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("stdio");

            toml_content.push_str(&format!("[mcp_servers.{}]\n", name));

            // Codex has no `type`/`transport` key: a `command` selects stdio and
            // a `url` selects streamable HTTP.  Supplying both is a
            // configuration error, so the transport picks exactly one branch.
            match transport {
                "http" | "sse" => {
                    if let Some(url) = config.get("url").and_then(|v| v.as_str()) {
                        toml_content.push_str(&format!("url = \"{}\"\n", escape_toml_string(url)));
                    }

                    // Codex spells static headers `http_headers`; a plain
                    // `headers` table is an unknown key and is ignored.
                    if let Some(headers) = config.get("headers").and_then(|v| v.as_object()) {
                        if !headers.is_empty() {
                            toml_content
                                .push_str(&format!("\n[mcp_servers.{}.http_headers]\n", name));
                            for (key, val) in headers {
                                if let Some(val_str) = val.as_str() {
                                    toml_content.push_str(&format!(
                                        "\"{}\" = \"{}\"\n",
                                        escape_toml_string(key),
                                        escape_toml_string(val_str)
                                    ));
                                }
                            }
                        }
                    }
                }
                _ => {
                    if let Some(command) = config.get("command").and_then(|v| v.as_str()) {
                        toml_content
                            .push_str(&format!("command = \"{}\"\n", escape_toml_string(command)));
                    }

                    if let Some(args) = config.get("args").and_then(|v| v.as_array()) {
                        let args_str: Vec<String> = args
                            .iter()
                            .filter_map(|a| a.as_str())
                            .map(|a| format!("\"{}\"", escape_toml_string(a)))
                            .collect();
                        toml_content.push_str(&format!("args = [{}]\n", args_str.join(", ")));
                    }

                    if let Some(cwd) = config.get("cwd").and_then(|v| v.as_str()) {
                        toml_content.push_str(&format!("cwd = \"{}\"\n", escape_toml_string(cwd)));
                    }

                    // Host variables to forward, populated by
                    // `rewrite_inherited_env` below.  Must precede the `env`
                    // sub-table: once a sub-table is opened, later bare keys
                    // would belong to it instead of the server table.
                    if let Some(env_vars) = config.get("env_vars").and_then(|v| v.as_array()) {
                        let names: Vec<String> = env_vars
                            .iter()
                            .filter_map(|v| v.as_str())
                            .map(|v| format!("\"{}\"", escape_toml_string(v)))
                            .collect();
                        if !names.is_empty() {
                            toml_content.push_str(&format!("env_vars = [{}]\n", names.join(", ")));
                        }
                    }

                    if let Some(env) = config.get("env").and_then(|v| v.as_object()) {
                        if !env.is_empty() {
                            toml_content.push_str(&format!("\n[mcp_servers.{}.env]\n", name));
                            for (key, val) in env {
                                if let Some(val_str) = val.as_str() {
                                    toml_content.push_str(&format!(
                                        "\"{}\" = \"{}\"\n",
                                        escape_toml_string(key),
                                        escape_toml_string(val_str)
                                    ));
                                }
                            }
                        }
                    }
                }
            }

            toml_content.push('\n');
        }

        let path = codex_dir.join("config.toml");
        let existing = read_existing_toml(&path);
        let final_content = merge_toml_mcp_section(&existing, &toml_content);

        fs::write(&path, final_content)
            .map_err(|e| format!("Failed to write .codex/config.toml: {}", e))?;

        Ok(path.display().to_string())
    }

    // ── Cleanup ─────────────────────────────────────────────────────────

    /// Codex CLI merges into `.codex/config.toml` which may contain model or
    /// history settings set by the user.  Strip only the `[mcp_servers.*]`
    /// sections rather than deleting the whole file.
    fn cleanup_mcp_config(&self, dir: &Path) -> Vec<String> {
        let path = dir.join(".codex").join("config.toml");
        if !path.exists() {
            return vec![];
        }
        let existing = read_existing_toml(&path);
        // Pass an empty mcp section to strip all [mcp_servers.*] blocks
        let stripped = merge_toml_mcp_section(&existing, "");
        let trimmed = stripped.trim();
        if trimmed.is_empty() {
            if fs::remove_file(&path).is_ok() {
                return vec![path.display().to_string()];
            }
        } else {
            if fs::write(&path, format!("{}\n", trimmed)).is_ok() {
                return vec![path.display().to_string()];
            }
        }
        vec![]
    }

    fn cleanup_mcp_preview(&self, dir: &Path) -> Vec<String> {
        let path = dir.join(".codex").join("config.toml");
        if path.exists() {
            vec![path.display().to_string()]
        } else {
            vec![]
        }
    }

    // ── Discovery ───────────────────────────────────────────────────────

    fn discover_mcp_servers(&self, _dir: &Path) -> Map<String, Value> {
        // Codex TOML import not implemented yet
        Map::new()
    }

    fn detect_global_install(&self) -> bool {
        super::cli_available("codex")
            || super::home_dir()
                .map(|h| h.join(".codex").exists())
                .unwrap_or(false)
    }

    fn extra_global_skill_dirs(&self) -> Vec<PathBuf> {
        match super::home_dir() {
            Some(home) => vec![home.join(".codex").join("skills")],
            None => vec![],
        }
    }

    fn discover_global_mcp_servers(&self) -> Map<String, Value> {
        let Some(home) = super::home_dir() else {
            return Map::new();
        };
        // ~/.codex/config.toml — user-level Codex CLI config
        let path = home.join(".codex").join("config.toml");
        discover_codex_global_config(&path)
    }

    fn agents_dir(&self, dir: &Path) -> Option<PathBuf> {
        Some(dir.join(".codex").join("agents"))
    }

    fn agents_file_ext(&self) -> &'static str {
        "toml"
    }

    fn convert_agent_content(&self, content: &str, name: &str) -> String {
        convert_md_to_codex_toml(content, name)
    }

    fn capabilities(&self) -> super::AgentCapabilities {
        super::AgentCapabilities {
            hooks: true,
            ..Default::default()
        }
    }

    fn hook_events(&self) -> &'static [&'static str] {
        CODEX_SUPPORTED_EVENTS
    }

    fn sync_hooks(
        &self,
        project_dir: &Path,
        hooks: &[crate::core::Hook],
    ) -> Result<Vec<String>, String> {
        sync_codex_hooks(project_dir, hooks)
    }

    fn hook_config_target(&self, dir: &Path) -> Option<super::HookConfigTarget> {
        Some(super::HookConfigTarget::Owned {
            path: dir.join(".codex").join("hooks.json"),
        })
    }
}

// ── Global config discovery ──────────────────────────────────────────────────

/// Parse `~/.codex/config.toml` and return any `[mcp_servers.*]` entries as
/// Automatic canonical MCP server configs.
fn discover_codex_global_config(path: &std::path::Path) -> Map<String, Value> {
    use serde_json::Value;
    use std::fs;

    let mut result = Map::new();

    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return result,
    };

    let doc: toml::Value = match toml::from_str(&content) {
        Ok(v) => v,
        Err(_) => return result,
    };

    let servers = match doc.get("mcp_servers").and_then(|v| v.as_table()) {
        Some(t) => t,
        None => return result,
    };

    for (name, entry) in servers {
        if !crate::core::is_valid_name(name) || name == "automatic" || name == "nexus" {
            continue;
        }
        let table = match entry.as_table() {
            Some(t) => t,
            None => continue,
        };

        let transport = table
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("stdio");

        let mut server = serde_json::Map::new();

        match transport {
            "http" | "sse" => {
                server.insert("type".to_string(), Value::String(transport.to_string()));
                if let Some(url) = table.get("url").and_then(|v| v.as_str()) {
                    server.insert("url".to_string(), Value::String(url.to_string()));
                }
                if let Some(headers) = table.get("headers").and_then(|v| v.as_table()) {
                    let hmap: serde_json::Map<String, Value> = headers
                        .iter()
                        .filter_map(|(k, v)| {
                            v.as_str()
                                .map(|s| (k.clone(), Value::String(s.to_string())))
                        })
                        .collect();
                    server.insert("headers".to_string(), Value::Object(hmap));
                }
            }
            _ => {
                if let Some(cmd) = table.get("command").and_then(|v| v.as_str()) {
                    server.insert("command".to_string(), Value::String(cmd.to_string()));
                }
                if let Some(args) = table.get("args").and_then(|v| v.as_array()) {
                    let arr: Vec<Value> = args
                        .iter()
                        .filter_map(|a| a.as_str().map(|s| Value::String(s.to_string())))
                        .collect();
                    if !arr.is_empty() {
                        server.insert("args".to_string(), Value::Array(arr));
                    }
                }
                if let Some(env) = table.get("env").and_then(|v| v.as_table()) {
                    let emap: serde_json::Map<String, Value> = env
                        .iter()
                        .filter_map(|(k, v)| {
                            v.as_str()
                                .map(|s| (k.clone(), Value::String(s.to_string())))
                        })
                        .collect();
                    if !emap.is_empty() {
                        server.insert("env".to_string(), Value::Object(emap));
                    }
                }
            }
        }

        if !server.is_empty() {
            result.insert(name.clone(), Value::Object(server));
        }
    }

    result
}

// ── TOML Helpers ────────────────────────────────────────────────────────────

fn escape_toml_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn read_existing_toml(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

/// Replace existing `[mcp_servers.*]` sections in TOML while preserving
/// everything else.
pub fn merge_toml_mcp_section(existing: &str, mcp_section: &str) -> String {
    if existing.is_empty() {
        return mcp_section.to_string();
    }

    let mut output = String::new();
    let mut skip = false;

    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("[mcp_servers") {
            skip = true;
            continue;
        }
        if skip && trimmed.starts_with('[') && !trimmed.starts_with("[mcp_servers") {
            skip = false;
        }
        if !skip {
            output.push_str(line);
            output.push('\n');
        }
    }

    let trimmed = output.trim_end();
    if trimmed.is_empty() {
        mcp_section.to_string()
    } else {
        format!("{}\n\n{}", trimmed, mcp_section)
    }
}

// ── Agent Content Conversion ────────────────────────────────────────────────

/// Convert Markdown with YAML frontmatter to Codex TOML agent format.
/// Input: Markdown content with YAML frontmatter (the Automatic canonical format).
/// Output: TOML content for Codex agents.
fn convert_md_to_codex_toml(content: &str, fallback_name: &str) -> String {
    let (frontmatter, body) = super::parse_frontmatter(content);

    // Marks this file as Automatic-written so cleanup can tell it apart from
    // a TOML agent the user placed in .codex/agents/ by hand — the sub-agent
    // counterpart to Gemini CLI's identical convention for command TOML.
    let mut toml = String::from("automatic_managed = true\n");

    let name = frontmatter
        .get("name")
        .map(|s| s.as_str())
        .unwrap_or(fallback_name);
    toml.push_str(&format!("name = \"{}\"\n", escape_toml_string(name)));

    if let Some(desc) = frontmatter.get("description") {
        toml.push_str(&format!("description = \"{}\"\n", escape_toml_string(desc)));
    }

    if let Some(model) = frontmatter.get("model") {
        let codex_model = match model.as_str() {
            "inherit" => "inherit",
            "sonnet" => "gpt-5.4",
            "haiku" => "gpt-5.4-mini",
            "opus" => "gpt-5.4",
            other => other,
        };
        toml.push_str(&format!("model = \"{}\"\n", codex_model));
    }

    if frontmatter.contains_key("tools") {
        toml.push_str("sandbox_mode = \"read-only\"\n");
    }

    if let Some(max_turns) = frontmatter.get("maxTurns") {
        toml.push_str(&format!("max_turns = {}\n", max_turns));
    }

    if let Some(reasoning) = frontmatter.get("modelReasoningEffort") {
        toml.push_str(&format!("model_reasoning_effort = \"{}\"\n", reasoning));
    }

    let body_trimmed = body.trim();
    if !body_trimmed.is_empty() {
        toml.push_str(&format!(
            "\ndeveloper_instructions = \"\"\"\n{}\n\"\"\"\n",
            body_trimmed
        ));
    }

    toml
}

// ── Hooks ────────────────────────────────────────────────────────────────────
//
// Codex CLI supports a smaller event set than Claude Code. We write the full
// `.codex/hooks.json` file (Automatic owns it; users wanting more control can
// fall back to `.codex/config.toml`'s inline `[hooks]` table, which we do not
// touch). Script-type handlers are written to `.codex/hooks/{slug}.sh`.

const CODEX_SUPPORTED_EVENTS: &[&str] = &[
    "SessionStart",
    "SessionEnd",
    "PreToolUse",
    "PermissionRequest",
    "PostToolUse",
    "PreCompact",
    "PostCompact",
    "UserPromptSubmit",
    "SubagentStart",
    "SubagentStop",
    "Stop",
];

fn sync_codex_hooks(
    project_dir: &Path,
    hooks: &[crate::core::Hook],
) -> Result<Vec<String>, String> {
    let hooks_file = project_dir.join(".codex").join("hooks.json");
    let spec = super::HookWriteSpec {
        supported_events: CODEX_SUPPORTED_EVENTS,
        scripts_dir: project_dir.join(".codex").join("hooks"),
        script_command: |file_name| format!("./.codex/hooks/{}", file_name),
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

    #[test]
    fn hook_events_declares_eleven_events() {
        let events = CodexCli.hook_events();
        assert_eq!(
            events.len(),
            11,
            "expected 11 documented Codex CLI hook events, found {}",
            events.len()
        );
        for event in [
            "SessionEnd",
            "PreCompact",
            "PostCompact",
            "SubagentStart",
            "SubagentStop",
        ] {
            assert!(
                events.contains(&event),
                "'{event}' is documented upstream but missing from CODEX_SUPPORTED_EVENTS"
            );
        }
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
        assert!(!CodexCli.detect_in(dir.path()));

        fs::create_dir_all(dir.path().join(".codex")).unwrap();
        fs::write(dir.path().join(".codex/config.toml"), "").unwrap();
        assert!(CodexCli.detect_in(dir.path()));
    }

    #[test]
    fn test_write_stdio() {
        let dir = tempdir().unwrap();
        CodexCli
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join(".codex/config.toml")).unwrap();
        assert!(content.contains("[mcp_servers.automatic]"));
        assert!(content.contains("[mcp_servers.github]"));
        assert!(content.contains("GITHUB_TOKEN"));
    }

    #[test]
    fn test_write_http() {
        let dir = tempdir().unwrap();
        CodexCli
            .write_mcp_config(dir.path(), &http_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join(".codex/config.toml")).unwrap();
        assert!(content.contains("[mcp_servers.remote-api]"));
        // Codex infers streamable HTTP from the presence of `url`; it has no
        // `type`/`transport` key.
        assert!(!content.contains("type = "));
        assert!(content.contains("url = \"https://api.example.com/mcp\""));
        assert!(content.contains("[mcp_servers.remote-api.http_headers]"));
        assert!(content.contains("Authorization"));
    }

    // ── Hooks ───────────────────────────────────────────────────────────────

    fn codex_cmd_hook(name: &str, event: &str, command: &str) -> crate::core::Hook {
        crate::core::Hook {
            name: name.to_string(),
            agent: "codex".to_string(),
            event: event.to_string(),
            matcher: None,
            handler: crate::core::HookHandler::Command {
                command: command.to_string(),
            },
            timeout_sec: Some(45),
            plugin_id: None,
            _author: None,
        }
    }

    #[test]
    fn codex_hook_sync_writes_dedicated_file() {
        let dir = tempdir().unwrap();
        let hooks = vec![codex_cmd_hook("hi", "SessionStart", "echo hi")];
        let written = CodexCli.sync_hooks(dir.path(), &hooks).unwrap();
        let path = dir.path().join(".codex/hooks.json");
        assert!(path.exists());
        assert!(written.iter().any(|w| w.ends_with("hooks.json")));

        let raw = fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let handler = &v["hooks"]["SessionStart"][0]["hooks"][0];
        assert_eq!(handler["type"], "command");
        assert_eq!(handler["command"], "echo hi");
        assert_eq!(handler["timeout"], 45);
    }

    #[test]
    fn codex_hook_sync_skips_unsupported_events() {
        let dir = tempdir().unwrap();
        // Claude-only event — Codex must skip it without failing the sync.
        let hooks = vec![codex_cmd_hook("setup", "Setup", "echo nope")];
        let written = CodexCli.sync_hooks(dir.path(), &hooks).unwrap();
        assert!(written.is_empty());
        assert!(!dir.path().join(".codex/hooks.json").exists());
    }

    #[test]
    fn codex_hook_sync_accepts_the_five_newly_added_events() {
        let dir = tempdir().unwrap();
        let hooks = vec![
            codex_cmd_hook("a", "SessionEnd", "echo a"),
            codex_cmd_hook("b", "PreCompact", "echo b"),
            codex_cmd_hook("c", "PostCompact", "echo c"),
            codex_cmd_hook("d", "SubagentStart", "echo d"),
            codex_cmd_hook("e", "SubagentStop", "echo e"),
        ];
        let written = CodexCli.sync_hooks(dir.path(), &hooks).unwrap();
        assert!(!written.is_empty());
        let raw = fs::read_to_string(dir.path().join(".codex/hooks.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        for event in [
            "SessionEnd",
            "PreCompact",
            "PostCompact",
            "SubagentStart",
            "SubagentStop",
        ] {
            assert!(
                v["hooks"].get(event).is_some(),
                "expected '{event}' to be written to hooks.json"
            );
        }
    }

    #[test]
    fn codex_hook_sync_removes_file_when_no_hooks() {
        let dir = tempdir().unwrap();
        // Pre-existing hooks file from an earlier sync.
        let hooks = vec![codex_cmd_hook("temp", "Stop", "echo bye")];
        CodexCli.sync_hooks(dir.path(), &hooks).unwrap();
        assert!(dir.path().join(".codex/hooks.json").exists());

        CodexCli.sync_hooks(dir.path(), &[]).unwrap();
        assert!(!dir.path().join(".codex/hooks.json").exists());
    }

    #[test]
    fn test_toml_merge() {
        let existing =
            "[model]\nprovider = \"anthropic\"\n\n[mcp_servers.old_server]\ncommand = \"old\"\n";
        let new_mcp = "[mcp_servers.automatic]\ncommand = \"automatic\"\n\n";
        let merged = merge_toml_mcp_section(existing, new_mcp);

        assert!(merged.contains("[model]"));
        assert!(merged.contains("[mcp_servers.automatic]"));
        assert!(!merged.contains("[mcp_servers.old_server]"));
    }
}
