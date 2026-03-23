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
    let (frontmatter, body) = parse_frontmatter(content);

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

/// Parse YAML frontmatter from Markdown content.
/// Returns (frontmatter_map, body_content).
fn parse_frontmatter(content: &str) -> (std::collections::HashMap<String, String>, &str) {
    use std::collections::HashMap;
    let mut frontmatter: HashMap<String, String> = HashMap::new();

    if !content.starts_with("---\n") && !content.starts_with("---\r\n") {
        return (frontmatter, content);
    }

    let after_first = &content[4..];
    let end_marker_pos = after_first
        .find("\n---")
        .or_else(|| after_first.find("\r\n---"));

    let end_marker_pos = match end_marker_pos {
        Some(pos) => pos,
        None => return (frontmatter, content),
    };

    let yaml_str = &after_first[..end_marker_pos];
    let body_start = end_marker_pos + 4;
    let body = if after_first[body_start..].starts_with('\n')
        || after_first[body_start..].starts_with("\r\n")
    {
        body_start + 1
    } else {
        body_start
    };
    let body = &after_first[body..];

    for line in yaml_str.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(colon_pos) = line.find(':') {
            let key = line[..colon_pos].trim();
            let mut value = line[colon_pos + 1..].trim();
            if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
                value = &value[1..value.len() - 1];
            } else if value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2 {
                value = &value[1..value.len() - 1];
            }
            frontmatter.insert(key.to_string(), value.to_string());
        }
    }

    (frontmatter, body)
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
