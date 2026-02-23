use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, sync_individual_skills, Agent};

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
        "GitHub Copilot"
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
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".agents").join("skills")]
    }

    // ── Config writing ──────────────────────────────────────────────────

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

        // Read existing config (if any)
        let mut root: Map<String, Value> = if path.exists() {
            let raw = fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read .vscode/mcp.json: {}", e))?;
            match serde_json::from_str::<Value>(&raw) {
                Ok(Value::Object(m)) => m,
                _ => Map::new(),
            }
        } else {
            Map::new()
        };

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

    fn sync_skills(
        &self,
        dir: &Path,
        skill_contents: &[(String, String)],
        selected_names: &[String],
    ) -> Result<Vec<String>, String> {
        let mut written = Vec::new();
        let skills_dir = dir.join(".agents").join("skills");
        sync_individual_skills(&skills_dir, skill_contents, selected_names, &mut written)?;
        Ok(written)
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
}

/// Pass-through normaliser: VS Code/Copilot format is close to canonical.
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
        assert!(parsed["servers"]["nexus"]["type"].is_null());
        assert!(parsed["servers"]["nexus"]["command"]
            .as_str()
            .unwrap()
            .contains("nexus"));
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
        assert!(parsed["servers"]["nexus"]["command"].is_string());
        assert!(parsed["servers"]["old"].is_null());
    }
}
