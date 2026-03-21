use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

use super::{sync_individual_skills, Agent, AgentCapabilities};

/// Goose agent (Block) — stores skills under
/// `<project>/.agents/skills/<name>/SKILL.md`.
///
/// ## Project instructions
///
/// Goose loads context files at session start.  The default search list is
/// `["AGENTS.md", ".goosehints"]` (configured via `CONTEXT_FILE_NAMES`).
/// `AGENTS.md` is checked first, so that is the file Automatic writes for this
/// agent.
///
/// ## MCP / extensions
///
/// Goose manages extensions entirely through its own YAML config file:
/// `~/.config/goose/config.yaml`.  Extensions are registered under the
/// `extensions` key as stdio commands or remote HTTP endpoints.  There is no
/// project-scoped MCP config file; all extensions are global.
///
/// Automatic can discover existing extensions from the global config (YAML
/// parsing) but **cannot write** Goose extension config — extensions must be
/// added through the `goose configure` CLI or the Goose Desktop GUI.
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
        "AGENTS.md (MCP configured via goose CLI/Desktop)"
    }

    fn project_file_name(&self) -> &'static str {
        // Goose's CONTEXT_FILE_NAMES default is ["AGENTS.md", ".goosehints"].
        // AGENTS.md is checked first, so it is the canonical project file.
        "AGENTS.md"
    }

    // ── Capabilities ────────────────────────────────────────────────────

    fn capabilities(&self) -> AgentCapabilities {
        // Goose has no project-level MCP config file.  Extensions are managed
        // globally via ~/.config/goose/config.yaml through the Goose CLI/Desktop.
        AgentCapabilities {
            mcp_servers: false,
            agents: false,
            ..Default::default()
        }
    }

    // ── MCP note ────────────────────────────────────────────────────────

    fn mcp_note(&self) -> Option<&'static str> {
        Some(
            "Goose manages extensions globally via ~/.config/goose/config.yaml. \
             Use `goose configure` or the Goose Desktop to add MCP servers \u{2014} \
             Automatic cannot write Goose\u{2019}s extension config.",
        )
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_in(&self, dir: &Path) -> bool {
        // .goosehints is the most unique Goose-specific marker.
        // AGENTS.md is too generic to use as a sole trigger.
        dir.join(".goosehints").exists()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".agents").join("skills")]
    }

    // ── Cleanup ─────────────────────────────────────────────────────────

    // No MCP config files to clean up — Goose MCP is managed externally.
    // owned_config_paths defaults to empty vec, which is correct here.

    // ── Config writing ──────────────────────────────────────────────────

    /// Goose does not use a project-level MCP config file.
    /// This is intentionally a no-op; extensions must be configured through
    /// the Goose CLI (`goose configure`) or Goose Desktop.
    fn write_mcp_config(
        &self,
        _dir: &Path,
        _servers: &Map<String, Value>,
    ) -> Result<String, String> {
        Ok(String::new())
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

    /// Goose has no project-level MCP config file to discover from.
    fn discover_mcp_servers(&self, _dir: &Path) -> Map<String, Value> {
        Map::new()
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
        // `type: "stdio"` or `type: "streamable_http"` maps onto Automatic's
        // canonical MCP server format.
        let path = home.join(".config").join("goose").join("config.yaml");
        discover_goose_global_config(&path)
    }
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
    let mut entry_type = String::new(); // "stdio", "sse", "streamable_http", etc.
    let mut entry_cmd = String::new(); // stdio: cmd field; http: uri field
    let mut entry_args: Vec<String> = Vec::new();
    let mut entry_envs: Map<String, Value> = Map::new();
    let mut in_block_args = false; // true while collecting `- item` lines for args

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
        cmd: &str, // stdio: command binary; streamable_http: uri
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
        // Only import stdio and streamable_http entries.
        // - "builtin" / "platform" / "frontend" / "inline_python" don't map to MCP.
        // - "sse" is deprecated in Goose (warns on load); skip it.
        // - empty type defaults to stdio behaviour.
        match entry_type {
            "stdio" | "" => {
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
                result.insert(name.to_string(), Value::Object(server));
            }
            "streamable_http" => {
                // `cmd` holds the `uri:` value for HTTP extensions.
                let mut server = serde_json::Map::new();
                server.insert("type".to_string(), Value::String("http".to_string()));
                server.insert("url".to_string(), Value::String(cmd.to_string()));
                if !envs.is_empty() {
                    server.insert("env".to_string(), Value::Object(envs.clone()));
                }
                result.insert(name.to_string(), Value::Object(server));
            }
            _ => {} // sse (deprecated), builtin, platform, etc — skip
        }
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
                    in_block_args = false;
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
                        in_block_args = false;
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
                        in_block_args = false;
                        phase = Phase::TopLevel;
                    }
                    continue;
                }

                if let Some(rest) = line.strip_prefix("type:") {
                    entry_type = unquote(strip_comment(rest.trim()));
                    in_block_args = false;
                } else if let Some(rest) = line.strip_prefix("cmd:") {
                    entry_cmd = unquote(strip_comment(rest.trim()));
                    in_block_args = false;
                } else if let Some(rest) = line.strip_prefix("uri:") {
                    // streamable_http extensions use `uri:` instead of `cmd:`.
                    // Store it in entry_cmd so flush() can handle it uniformly.
                    entry_cmd = unquote(strip_comment(rest.trim()));
                    in_block_args = false;
                } else if let Some(rest) = line.strip_prefix("args:") {
                    in_block_args = false;
                    // Inline flow sequence: args: [-y, "@scope/pkg"]  or  args: ["a", "b"]
                    let list_str = strip_comment(rest.trim());
                    if list_str.starts_with('[') && list_str.ends_with(']') {
                        let inner = &list_str[1..list_str.len() - 1];
                        entry_args = inner
                            .split(',')
                            .map(|s| unquote(s.trim()))
                            .filter(|s| !s.is_empty())
                            .collect();
                    } else if list_str.is_empty() {
                        // Block sequence follows on subsequent lines
                        in_block_args = true;
                    }
                } else if line.starts_with("- ") && in_block_args {
                    // Block-style args list item
                    entry_args.push(unquote(line[2..].trim()));
                } else if line.starts_with("envs:") {
                    in_block_args = false;
                    phase = Phase::InEnvs;
                } else {
                    in_block_args = false;
                }
            }
            Phase::InEnvs => {
                if indent <= 4 && !line.starts_with('-') {
                    // Back to entry level
                    phase = Phase::InEntry;
                    in_block_args = false;
                    // Re-process this line as an entry field
                    if let Some(rest) = line.strip_prefix("type:") {
                        entry_type = unquote(strip_comment(rest.trim()));
                    } else if let Some(rest) = line.strip_prefix("cmd:") {
                        entry_cmd = unquote(strip_comment(rest.trim()));
                    } else if let Some(rest) = line.strip_prefix("uri:") {
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
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_detect_goosehints() {
        let dir = tempdir().unwrap();
        assert!(!Goose.detect_in(dir.path()));

        fs::write(dir.path().join(".goosehints"), "").unwrap();
        assert!(Goose.detect_in(dir.path()));
    }

    #[test]
    fn test_project_file_name_is_agents_md() {
        // Goose checks AGENTS.md first (default CONTEXT_FILE_NAMES order).
        assert_eq!(Goose.project_file_name(), "AGENTS.md");
    }

    #[test]
    fn test_mcp_capability_disabled() {
        assert!(!Goose.capabilities().mcp_servers);
    }

    #[test]
    fn test_mcp_note_is_some() {
        assert!(Goose.mcp_note().is_some());
    }

    #[test]
    fn test_write_mcp_config_is_noop() {
        let dir = tempdir().unwrap();
        let mut servers = Map::new();
        servers.insert(
            "github".to_string(),
            serde_json::json!({"command": "npx", "args": ["@modelcontextprotocol/server-github"]}),
        );

        let result = Goose.write_mcp_config(dir.path(), &servers);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");

        // No files should have been written
        let entries: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_discover_mcp_servers_always_empty() {
        let dir = tempdir().unwrap();
        // Even if someone manually put a .goose/mcp.json there, we no longer
        // read it — project-level MCP discovery is not supported for Goose.
        assert!(Goose.discover_mcp_servers(dir.path()).is_empty());
    }

    #[test]
    fn test_skill_sync() {
        let dir = tempdir().unwrap();
        let skills = vec![("my-skill".to_string(), "# My Skill\n".to_string())];
        let selected = vec!["my-skill".to_string()];

        let written = Goose
            .sync_skills(dir.path(), &skills, &selected, &[])
            .unwrap();
        assert_eq!(written.len(), 1);
        assert!(written[0].contains("my-skill"));

        let content =
            fs::read_to_string(dir.path().join(".agents/skills/my-skill/SKILL.md")).unwrap();
        assert_eq!(content, "# My Skill\n");
    }

    #[test]
    fn test_discover_global_config_stdio_inline_args() {
        let dir = tempdir().unwrap();
        let config = r#"
GOOSE_PROVIDER: anthropic
GOOSE_MODEL: claude-3-5-sonnet

extensions:
  github:
    name: GitHub
    cmd: npx
    args: [-y, "@modelcontextprotocol/server-github"]
    enabled: true
    envs:
      GITHUB_PERSONAL_ACCESS_TOKEN: ghp_test123
    type: stdio
    timeout: 300
  memory:
    name: Memory
    cmd: npx
    args: [-y, "@modelcontextprotocol/server-memory"]
    enabled: true
    type: stdio
    timeout: 300
"#;
        fs::write(dir.path().join("config.yaml"), config).unwrap();
        let result = discover_goose_global_config(&dir.path().join("config.yaml"));

        assert!(result.contains_key("github"));
        let gh = &result["github"];
        assert_eq!(gh["command"].as_str().unwrap(), "npx");
        let args = gh["args"].as_array().unwrap();
        assert_eq!(args[0].as_str().unwrap(), "-y");
        assert_eq!(
            args[1].as_str().unwrap(),
            "@modelcontextprotocol/server-github"
        );
        assert_eq!(
            gh["env"]["GITHUB_PERSONAL_ACCESS_TOKEN"].as_str().unwrap(),
            "ghp_test123"
        );

        assert!(result.contains_key("memory"));
    }

    #[test]
    fn test_discover_global_config_block_args() {
        let dir = tempdir().unwrap();
        let config = r#"
extensions:
  mytool:
    name: mytool
    cmd: python
    args:
      - -m
      - my_mcp_server
      - --port
      - "8080"
    enabled: true
    type: stdio
    timeout: 300
"#;
        fs::write(dir.path().join("config.yaml"), config).unwrap();
        let result = discover_goose_global_config(&dir.path().join("config.yaml"));

        assert!(result.contains_key("mytool"), "mytool should be parsed");
        let tool = &result["mytool"];
        assert_eq!(tool["command"].as_str().unwrap(), "python");
        let args = tool["args"].as_array().unwrap();
        assert_eq!(args[0].as_str().unwrap(), "-m");
        assert_eq!(args[1].as_str().unwrap(), "my_mcp_server");
        assert_eq!(args[2].as_str().unwrap(), "--port");
        assert_eq!(args[3].as_str().unwrap(), "8080");
    }

    #[test]
    fn test_discover_global_config_streamable_http() {
        let dir = tempdir().unwrap();
        let config = r#"
extensions:
  linear:
    name: linear
    type: streamable_http
    uri: https://mcp.linear.app/mcp
    enabled: true
    timeout: 300
"#;
        fs::write(dir.path().join("config.yaml"), config).unwrap();
        let result = discover_goose_global_config(&dir.path().join("config.yaml"));

        assert!(
            result.contains_key("linear"),
            "streamable_http should be imported"
        );
        let srv = &result["linear"];
        assert_eq!(srv["type"].as_str().unwrap(), "http");
        assert_eq!(srv["url"].as_str().unwrap(), "https://mcp.linear.app/mcp");
    }

    #[test]
    fn test_discover_global_config_builtin_skipped() {
        let dir = tempdir().unwrap();
        let config = r#"
extensions:
  developer:
    name: developer
    type: builtin
    enabled: true
    timeout: 300
"#;
        fs::write(dir.path().join("config.yaml"), config).unwrap();
        let result = discover_goose_global_config(&dir.path().join("config.yaml"));

        assert!(
            !result.contains_key("developer"),
            "builtin extensions should be skipped"
        );
    }

    #[test]
    fn test_discover_global_config_mixed() {
        let dir = tempdir().unwrap();
        // Realistic config with both stdio and http extensions plus builtins
        let config = r#"
GOOSE_PROVIDER: openai
GOOSE_MODEL: gpt-4o

extensions:
  developer:
    bundled: true
    enabled: true
    name: developer
    timeout: 300
    type: builtin
    display_name: Developer
  github:
    enabled: true
    name: GitHub
    cmd: npx
    args: [-y, "@modelcontextprotocol/server-github"]
    envs:
      GITHUB_PERSONAL_ACCESS_TOKEN: ghp_abc123
    env_keys: []
    timeout: 300
    type: stdio
  linear:
    enabled: true
    name: linear
    type: streamable_http
    uri: https://mcp.linear.app/mcp
    timeout: 300
"#;
        fs::write(dir.path().join("config.yaml"), config).unwrap();
        let result = discover_goose_global_config(&dir.path().join("config.yaml"));

        // builtin should be absent
        assert!(!result.contains_key("developer"));
        // stdio should be present
        assert!(result.contains_key("github"));
        assert_eq!(result["github"]["command"].as_str().unwrap(), "npx");
        // streamable_http should be present
        assert!(result.contains_key("linear"));
        assert_eq!(result["linear"]["type"].as_str().unwrap(), "http");
    }
}
