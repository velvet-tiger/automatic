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

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_in(&self, dir: &Path) -> bool {
        dir.join(".mcp.json").exists()
            || dir.join(".claude").join("settings.json").exists()
            || dir.join(".claude").join("skills").exists()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".claude").join("skills")]
    }

    // ── Config writing ──────────────────────────────────────────────────

    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String> {
        // Claude Code uses Nexus's JSON format directly, with one tweak:
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
    ) -> Result<Vec<String>, String> {
        let mut written = Vec::new();
        let skills_dir = dir.join(".claude").join("skills");
        sync_individual_skills(&skills_dir, skill_contents, selected_names, &mut written)?;
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
            "nexus".to_string(),
            json!({"type":"stdio","command":"/usr/local/bin/nexus","args":["mcp-serve"]}),
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
        assert!(parsed["mcpServers"]["nexus"]["type"].is_null());
        assert!(parsed["mcpServers"]["nexus"]["command"]
            .as_str()
            .unwrap()
            .contains("nexus"));
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
}
