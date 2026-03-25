use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, sync_individual_skills, Agent};

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
        dir.join(".mcp.json").exists()
            || dir.join(".claude").join("settings.json").exists()
            || dir.join(".claude").join("skills").exists()
            || dir.join(".claude").join("commands").exists()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".claude").join("skills")]
    }

    fn capabilities(&self) -> super::AgentCapabilities {
        super::AgentCapabilities {
            commands: true,
            ..Default::default()
        }
    }

    fn agents_dir(&self, dir: &Path) -> Option<PathBuf> {
        Some(dir.join(".claude").join("agents"))
    }

    fn commands_dir(&self, dir: &Path) -> Option<PathBuf> {
        Some(dir.join(".claude").join("commands"))
    }

    // ── Cleanup ─────────────────────────────────────────────────────────

    fn owned_config_paths(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".mcp.json")]
    }

    // ── Config writing ──────────────────────────────────────────────────

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
        fs::write(&path, content).map_err(|e| format!("Failed to write .mcp.json: {}", e))?;

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
        let skills_dir = dir.join(".claude").join("skills");
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
        assert!(!ClaudeCode.detect_in(dir.path()));

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
