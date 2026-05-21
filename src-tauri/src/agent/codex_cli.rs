use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::{sync_individual_skills, Agent};

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

            match transport {
                "http" | "sse" => {
                    toml_content.push_str(&format!("type = \"{}\"\n", transport));

                    if let Some(url) = config.get("url").and_then(|v| v.as_str()) {
                        toml_content.push_str(&format!("url = \"{}\"\n", escape_toml_string(url)));
                    }

                    if let Some(headers) = config.get("headers").and_then(|v| v.as_object()) {
                        if !headers.is_empty() {
                            toml_content.push_str(&format!("\n[mcp_servers.{}.headers]\n", name));
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

                    if let Some(env) = config.get("env").and_then(|v| v.as_object()) {
                        if !env.is_empty() {
                            toml_content.push_str(&format!("\n[mcp_servers.{}.env]\n", name));
                            for (key, val) in env {
                                if let Some(val_str) = val.as_str() {
                                    toml_content.push_str(&format!(
                                        "{} = \"{}\"\n",
                                        key,
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

    fn sync_skills(
        &self,
        dir: &Path,
        skill_contents: &[(String, String)],
        selected_names: &[String],
        local_skill_names: &[String],
    ) -> Result<Vec<String>, String> {
        let mut written = Vec::new();
        let skills_dir = dir.join(".agents").join("skills");
        sync_individual_skills(
            &skills_dir,
            skill_contents,
            selected_names,
            local_skill_names,
            &mut written,
        )?;
        Ok(written)
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

    fn sync_hooks(
        &self,
        project_dir: &Path,
        hooks: &[crate::core::Hook],
    ) -> Result<Vec<String>, String> {
        sync_codex_hooks(project_dir, hooks)
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

    let mut toml = String::new();

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
    "PreToolUse",
    "PermissionRequest",
    "PostToolUse",
    "UserPromptSubmit",
    "Stop",
];

fn sync_codex_hooks(
    project_dir: &Path,
    hooks: &[crate::core::Hook],
) -> Result<Vec<String>, String> {
    let codex_dir = project_dir.join(".codex");
    let hooks_file = codex_dir.join("hooks.json");
    let scripts_dir = codex_dir.join("hooks");
    let mut written = Vec::new();

    // If there are no hooks, remove the file so Codex doesn't see a stale
    // config. The scripts directory cleanup runs the same way as Claude Code.
    let usable_hooks: Vec<&crate::core::Hook> = hooks
        .iter()
        .filter(|h| {
            if CODEX_SUPPORTED_EVENTS.contains(&h.event.as_str()) {
                true
            } else {
                eprintln!(
                    "[automatic] Codex CLI does not support hook event '{}' — skipping hook '{}'",
                    h.event, h.name
                );
                false
            }
        })
        .collect();

    if usable_hooks.is_empty() {
        if hooks_file.exists() {
            let _ = fs::remove_file(&hooks_file);
        }
        cleanup_codex_scripts(&scripts_dir, &[])?;
        return Ok(written);
    }

    fs::create_dir_all(&codex_dir)
        .map_err(|e| format!("Failed to create .codex/: {}", e))?;

    let mut managed_script_paths: Vec<PathBuf> = Vec::new();
    let mut needs_scripts_dir = false;
    for hook in &usable_hooks {
        if matches!(hook.handler, crate::core::HookHandler::Script { .. }) {
            needs_scripts_dir = true;
            break;
        }
    }
    if needs_scripts_dir {
        fs::create_dir_all(&scripts_dir)
            .map_err(|e| format!("Failed to create .codex/hooks/: {}", e))?;
    }

    for hook in &usable_hooks {
        if let crate::core::HookHandler::Script {
            interpreter,
            script,
        } = &hook.handler
        {
            let ext = codex_script_extension(interpreter);
            let slug = codex_hook_slug(hook);
            let path = scripts_dir.join(format!("{}.{}", slug, ext));
            let body = codex_annotate_managed_script(&codex_ensure_shebang(script, interpreter));
            fs::write(&path, body)
                .map_err(|e| format!("Failed to write Codex hook script '{}': {}", path.display(), e))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = fs::metadata(&path) {
                    let mut perms = meta.permissions();
                    perms.set_mode(0o755);
                    let _ = fs::set_permissions(&path, perms);
                }
            }
            managed_script_paths.push(path.clone());
            written.push(path.display().to_string());
        }
    }

    // Build the hooks.json document. Codex's schema is similar to Claude
    // Code's: { event: [ { matcher?, hooks: [ { type, command, timeout? } ] } ] }.
    use std::collections::BTreeMap;
    type HandlersByMatcher = BTreeMap<Option<String>, Vec<Value>>;
    let mut grouped: BTreeMap<String, HandlersByMatcher> = BTreeMap::new();
    for hook in &usable_hooks {
        let handler = codex_handler_value(hook);
        grouped
            .entry(hook.event.clone())
            .or_default()
            .entry(hook.matcher.clone())
            .or_default()
            .push(handler);
    }

    let mut hooks_root = Map::new();
    for (event, matchers) in grouped {
        let mut groups = Vec::new();
        for (matcher, handlers) in matchers {
            let mut group = Map::new();
            if let Some(m) = matcher {
                group.insert("matcher".to_string(), Value::String(m));
            }
            group.insert("hooks".to_string(), Value::Array(handlers));
            groups.push(Value::Object(group));
        }
        hooks_root.insert(event, Value::Array(groups));
    }

    let document = serde_json::json!({ "hooks": hooks_root });
    let pretty = serde_json::to_string_pretty(&document)
        .map_err(|e| format!("JSON error: {}", e))?;
    fs::write(&hooks_file, format!("{}\n", pretty))
        .map_err(|e| format!("Failed to write .codex/hooks.json: {}", e))?;
    written.push(hooks_file.display().to_string());

    cleanup_codex_scripts(&scripts_dir, &managed_script_paths)?;

    Ok(written)
}

fn codex_handler_value(hook: &crate::core::Hook) -> Value {
    let mut handler = Map::new();
    handler.insert("type".to_string(), Value::String("command".to_string()));
    let command_str = match &hook.handler {
        crate::core::HookHandler::Command { command } => command.clone(),
        crate::core::HookHandler::Script { interpreter, .. } => {
            let ext = codex_script_extension(interpreter);
            let slug = codex_hook_slug(hook);
            format!("./.codex/hooks/{}.{}", slug, ext)
        }
        // The user owns the file — pass the path through verbatim.
        crate::core::HookHandler::Path { path, .. } => path.clone(),
    };
    handler.insert("command".to_string(), Value::String(command_str));
    if let Some(t) = hook.timeout_sec {
        handler.insert("timeout".to_string(), Value::Number(serde_json::Number::from(t)));
    }
    Value::Object(handler)
}

fn codex_hook_slug(hook: &crate::core::Hook) -> String {
    let slug: String = hook
        .name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if slug.is_empty() {
        "hook".to_string()
    } else {
        slug
    }
}

fn codex_script_extension(interpreter: &str) -> &'static str {
    let lower = interpreter.trim().to_ascii_lowercase();
    if lower.ends_with("python") || lower.ends_with("python3") {
        "py"
    } else if lower.ends_with("node") || lower.ends_with("nodejs") {
        "js"
    } else if lower.ends_with("zsh") {
        "zsh"
    } else if lower.ends_with("fish") {
        "fish"
    } else if lower.ends_with("pwsh") || lower.ends_with("powershell") {
        "ps1"
    } else {
        "sh"
    }
}

fn codex_ensure_shebang(script: &str, interpreter: &str) -> String {
    let trimmed = script.trim_start();
    if trimmed.starts_with("#!") {
        return script.to_string();
    }
    let interp = interpreter.trim();
    if interp.is_empty() {
        return script.to_string();
    }
    let shebang = if interp.starts_with('/') {
        format!("#!{}\n", interp)
    } else {
        format!("#!/usr/bin/env {}\n", interp)
    };
    format!("{}{}", shebang, script)
}

fn codex_annotate_managed_script(body: &str) -> String {
    const MARKER: &str = "# managed-by-automatic — do not edit by hand\n";
    if body.contains("managed-by-automatic") {
        return body.to_string();
    }
    if let Some(rest) = body.strip_prefix("#!") {
        if let Some(newline_idx) = rest.find('\n') {
            let (shebang_line, rest_after) = body.split_at("#!".len() + newline_idx + 1);
            return format!("{}{}{}", shebang_line, MARKER, rest_after);
        }
    }
    format!("{}{}", MARKER, body)
}

fn cleanup_codex_scripts(
    scripts_dir: &Path,
    keep_paths: &[PathBuf],
) -> Result<(), String> {
    if !scripts_dir.exists() {
        return Ok(());
    }
    let entries = match fs::read_dir(scripts_dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };
    let keep_names: std::collections::HashSet<String> = keep_paths
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(|s| s.to_string()))
        .collect();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if keep_names.contains(name) {
            continue;
        }
        if let Ok(content) = fs::read_to_string(&path) {
            if content.contains("managed-by-automatic") {
                let _ = fs::remove_file(&path);
            }
        }
    }
    Ok(())
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
        assert!(content.contains("type = \"http\""));
        assert!(content.contains("url = \"https://api.example.com/mcp\""));
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
        let hooks = vec![codex_cmd_hook("compact", "PreCompact", "echo nope")];
        let written = CodexCli.sync_hooks(dir.path(), &hooks).unwrap();
        assert!(written.is_empty());
        assert!(!dir.path().join(".codex/hooks.json").exists());
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
