use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, sync_individual_skills, Agent};

/// Goose agent (Block) — writes `.goose/mcp.json` and stores skills under
/// `<project>/.agents/skills/<name>/SKILL.md`.
///
/// Goose natively uses `~/.config/goose/config.yaml` for extensions (YAML),
/// but we write a project-level `.goose/mcp.json` using the standard
/// `mcpServers` format for consistency with Automatic's sync model.
pub struct Goose;

impl Agent for Goose {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "goose"
    }

    fn label(&self) -> &'static str {
        "Goose (Beta)"
    }

    fn config_description(&self) -> &'static str {
        ".goose/mcp.json"
    }

    fn project_file_name(&self) -> &'static str {
        ".goosehints"
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_in(&self, dir: &Path) -> bool {
        dir.join(".goosehints").exists() || dir.join(".goose").exists()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".agents").join("skills")]
    }

    // ── Cleanup ─────────────────────────────────────────────────────────

    /// `detect_in` matches the `.goose/` directory itself, so we must remove
    /// the whole directory on cleanup — not just `mcp.json` inside it —
    /// otherwise the empty dir re-triggers detection on the next autodetect.
    fn cleanup_mcp_config(&self, dir: &Path) -> Vec<String> {
        let goose_dir = dir.join(".goose");
        if goose_dir.exists() {
            if fs::remove_dir_all(&goose_dir).is_ok() {
                return vec![goose_dir.display().to_string()];
            }
        }
        vec![]
    }

    fn cleanup_mcp_preview(&self, dir: &Path) -> Vec<String> {
        let goose_dir = dir.join(".goose");
        if goose_dir.exists() {
            vec![goose_dir.display().to_string()]
        } else {
            vec![]
        }
    }

    // ── Config writing ──────────────────────────────────────────────────

    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String> {
        // Write mcpServers JSON to .goose/mcp.json.
        let mut goose_servers = Map::new();

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
            goose_servers.insert(name.clone(), server);
        }

        let output = json!({ "mcpServers": Value::Object(goose_servers) });

        let goose_dir = dir.join(".goose");
        if !goose_dir.exists() {
            fs::create_dir_all(&goose_dir)
                .map_err(|e| format!("Failed to create .goose/: {}", e))?;
        }

        let path = goose_dir.join("mcp.json");
        let content =
            serde_json::to_string_pretty(&output).map_err(|e| format!("JSON error: {}", e))?;
        fs::write(&path, content).map_err(|e| format!("Failed to write .goose/mcp.json: {}", e))?;

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

    // ── Discovery ───────────────────────────────────────────────────────

    fn discover_mcp_servers(&self, dir: &Path) -> Map<String, Value> {
        let path = dir.join(".goose").join("mcp.json");
        if !path.exists() {
            return Map::new();
        }
        discover_mcp_servers_from_json(&path, "mcpServers", identity)
    }

    fn detect_global_install(&self) -> bool {
        super::cli_available("goose")
            || super::home_dir()
                .map(|h| h.join(".config").join("goose").exists())
                .unwrap_or(false)
    }

    fn discover_global_mcp_servers(&self) -> Map<String, Value> {
        let Some(home) = super::home_dir() else {
            return Map::new();
        };
        // ~/.config/goose/config.yaml — Goose's global extension config.
        // Extensions are listed under the `extensions` map; each entry with
        // `type: "stdio"` maps onto Automatic's canonical MCP server format.
        let path = home.join(".config").join("goose").join("config.yaml");
        discover_goose_global_config(&path)
    }
}

/// Pass-through normaliser.
fn identity(v: Value) -> Value {
    v
}

/// Parse Goose's global `~/.config/goose/config.yaml` and extract stdio/sse
/// extensions as Automatic canonical MCP server configs.
///
/// Goose config YAML (extensions section):
/// ```yaml
/// extensions:
///   my-server:
///     type: stdio
///     cmd: npx
///     args: ["-y", "@example/server"]
///     envs:
///       API_KEY: secret
///     enabled: true
/// ```
///
/// We parse this without an external YAML library using a line-oriented state
/// machine that handles the common indented-block structure.  Deeply nested or
/// non-standard YAML constructs are skipped safely.
fn discover_goose_global_config(path: &std::path::Path) -> Map<String, Value> {
    use std::fs;

    let mut result = Map::new();

    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return result,
    };

    // State machine phases
    #[derive(PartialEq)]
    enum Phase {
        TopLevel,
        InExtensions,
        InEntry,
        InEnvs,
    }

    let mut phase = Phase::TopLevel;
    let mut current_name = String::new();
    let mut entry_type = String::new(); // "stdio", "sse", etc.
    let mut entry_cmd = String::new();
    let mut entry_args: Vec<String> = Vec::new();
    let mut entry_envs: Map<String, Value> = Map::new();

    /// Strip an inline YAML comment and leading/trailing whitespace.
    fn strip_comment(s: &str) -> &str {
        // A '#' that is not inside quotes is a comment start.
        // For our simple values (strings, plain scalars) we can just split on
        // the first unquoted '#'.
        let bytes = s.as_bytes();
        let mut in_quote = false;
        let mut quote_char = b'"';
        for (i, &b) in bytes.iter().enumerate() {
            if in_quote {
                if b == quote_char {
                    in_quote = false;
                }
            } else if b == b'"' || b == b'\'' {
                in_quote = true;
                quote_char = b;
            } else if b == b'#' {
                return s[..i].trim_end();
            }
        }
        s.trim_end()
    }

    /// Unquote a YAML scalar value (remove surrounding ' or ").
    fn unquote(s: &str) -> String {
        let s = s.trim();
        if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
            s[1..s.len() - 1].to_string()
        } else {
            s.to_string()
        }
    }

    fn flush(
        name: &str,
        entry_type: &str,
        cmd: &str,
        args: &[String],
        envs: &Map<String, Value>,
        result: &mut Map<String, Value>,
    ) {
        if name.is_empty() || cmd.is_empty() {
            return;
        }
        if !crate::core::is_valid_name(name) {
            return;
        }
        // Only import stdio/sse entries — builtin Goose extensions don't map to MCP.
        if entry_type != "stdio" && entry_type != "sse" && !entry_type.is_empty() {
            return;
        }
        let mut server = serde_json::Map::new();
        server.insert("command".to_string(), Value::String(cmd.to_string()));
        if !args.is_empty() {
            server.insert(
                "args".to_string(),
                Value::Array(args.iter().map(|a| Value::String(a.clone())).collect()),
            );
        }
        if !envs.is_empty() {
            server.insert("env".to_string(), Value::Object(envs.clone()));
        }
        if entry_type == "sse" {
            server.insert("type".to_string(), Value::String("sse".to_string()));
        }
        result.insert(name.to_string(), Value::Object(server));
    }

    for raw_line in content.lines() {
        let indent = raw_line.len() - raw_line.trim_start().len();
        let line = raw_line.trim();

        // Skip blank lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        match phase {
            Phase::TopLevel => {
                if line.starts_with("extensions:") {
                    phase = Phase::InExtensions;
                }
            }
            Phase::InExtensions => {
                // Return to top-level on any non-indented non-comment line
                if indent == 0 && !line.starts_with('-') {
                    phase = Phase::TopLevel;
                    continue;
                }
                // Extension name: two-space indent, ends with ':'
                if indent == 2 && line.ends_with(':') {
                    // Flush previous entry
                    flush(
                        &current_name,
                        &entry_type,
                        &entry_cmd,
                        &entry_args,
                        &entry_envs,
                        &mut result,
                    );
                    current_name = line.trim_end_matches(':').to_string();
                    entry_type.clear();
                    entry_cmd.clear();
                    entry_args.clear();
                    entry_envs.clear();
                    phase = Phase::InEntry;
                }
            }
            Phase::InEntry => {
                if indent <= 2 && !line.starts_with('-') {
                    // End of entry (new key at same or lower indent)
                    if indent == 2 && line.ends_with(':') {
                        flush(
                            &current_name,
                            &entry_type,
                            &entry_cmd,
                            &entry_args,
                            &entry_envs,
                            &mut result,
                        );
                        current_name = line.trim_end_matches(':').to_string();
                        entry_type.clear();
                        entry_cmd.clear();
                        entry_args.clear();
                        entry_envs.clear();
                        // stay in InEntry
                    } else if indent == 0 {
                        flush(
                            &current_name,
                            &entry_type,
                            &entry_cmd,
                            &entry_args,
                            &entry_envs,
                            &mut result,
                        );
                        phase = Phase::TopLevel;
                    }
                    continue;
                }

                if let Some(rest) = line.strip_prefix("type:") {
                    entry_type = unquote(strip_comment(rest.trim()));
                } else if let Some(rest) = line.strip_prefix("cmd:") {
                    entry_cmd = unquote(strip_comment(rest.trim()));
                } else if let Some(rest) = line.strip_prefix("args:") {
                    // Inline list: args: ["a", "b"]  or  args: [a, b]
                    let list_str = strip_comment(rest.trim());
                    if list_str.starts_with('[') && list_str.ends_with(']') {
                        let inner = &list_str[1..list_str.len() - 1];
                        entry_args = inner
                            .split(',')
                            .map(|s| unquote(s.trim()))
                            .filter(|s| !s.is_empty())
                            .collect();
                    }
                    // Multi-line args (- item per line) handled in the list state
                    // below via indent; we accept the inline-only form here.
                } else if line.starts_with("- ") && !entry_cmd.is_empty() {
                    // Multi-line args list items
                    entry_args.push(unquote(line[2..].trim()));
                } else if line.starts_with("envs:") {
                    phase = Phase::InEnvs;
                }
            }
            Phase::InEnvs => {
                if indent <= 4 && !line.starts_with('-') {
                    // Back to entry level
                    phase = Phase::InEntry;
                    // Re-process this line as an entry field
                    if let Some(rest) = line.strip_prefix("type:") {
                        entry_type = unquote(strip_comment(rest.trim()));
                    } else if let Some(rest) = line.strip_prefix("cmd:") {
                        entry_cmd = unquote(strip_comment(rest.trim()));
                    }
                    continue;
                }
                // KEY: VALUE pairs inside envs block
                if let Some((k, v)) = line.split_once(':') {
                    let key = k.trim().to_string();
                    let val = unquote(strip_comment(v.trim()));
                    if !key.is_empty() {
                        entry_envs.insert(key, Value::String(val));
                    }
                }
            }
        }
    }

    // Flush final entry
    flush(
        &current_name,
        &entry_type,
        &entry_cmd,
        &entry_args,
        &entry_envs,
        &mut result,
    );

    result
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
        assert!(!Goose.detect_in(dir.path()));

        fs::write(dir.path().join(".goosehints"), "").unwrap();
        assert!(Goose.detect_in(dir.path()));
    }

    #[test]
    fn test_cleanup_removes_goose_dir() {
        let dir = tempdir().unwrap();
        let goose_dir = dir.path().join(".goose");
        fs::create_dir_all(&goose_dir).unwrap();
        fs::write(goose_dir.join("mcp.json"), "{}").unwrap();
        assert!(goose_dir.exists());

        let removed = Goose.cleanup_mcp_config(dir.path());
        assert_eq!(removed, vec![goose_dir.display().to_string()]);
        assert!(!goose_dir.exists(), ".goose/ should be deleted");
    }

    #[test]
    fn test_cleanup_preview_goose_dir() {
        let dir = tempdir().unwrap();
        assert!(Goose.cleanup_mcp_preview(dir.path()).is_empty());

        let goose_dir = dir.path().join(".goose");
        fs::create_dir_all(&goose_dir).unwrap();
        let preview = Goose.cleanup_mcp_preview(dir.path());
        assert_eq!(preview, vec![goose_dir.display().to_string()]);
    }

    #[test]
    fn test_write_stdio() {
        let dir = tempdir().unwrap();
        Goose
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join(".goose/mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert!(parsed["mcpServers"]["automatic"]["type"].is_null());
        assert!(parsed["mcpServers"]["automatic"]["command"]
            .as_str()
            .unwrap()
            .contains("automatic"));
    }
}
