use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, sync_individual_skills, Agent};

/// Google Antigravity agent — writes `.antigravity/mcp.json` and stores
/// skills under `<project>/.agents/skills/<name>/SKILL.md`.
///
/// Antigravity is Google's coding agent/IDE.  MCP config is stored in
/// `.antigravity/mcp.json` using the standard `mcpServers` JSON format.
pub struct Antigravity;

impl Agent for Antigravity {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "antigravity"
    }

    fn label(&self) -> &'static str {
        "Antigravity"
    }

    fn config_description(&self) -> &'static str {
        ".antigravity/mcp.json"
    }

    fn project_file_name(&self) -> &'static str {
        "AGENTS.md"
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_in(&self, dir: &Path) -> bool {
        dir.join(".antigravity").join("mcp.json").exists() || dir.join(".antigravity").exists()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".agents").join("skills")]
    }

    // ── Config writing ──────────────────────────────────────────────────

    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String> {
        let mut ag_servers = Map::new();

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
            ag_servers.insert(name.clone(), server);
        }

        let output = json!({ "mcpServers": Value::Object(ag_servers) });

        let ag_dir = dir.join(".antigravity");
        if !ag_dir.exists() {
            fs::create_dir_all(&ag_dir)
                .map_err(|e| format!("Failed to create .antigravity/: {}", e))?;
        }

        let path = ag_dir.join("mcp.json");
        let content =
            serde_json::to_string_pretty(&output).map_err(|e| format!("JSON error: {}", e))?;
        fs::write(&path, content)
            .map_err(|e| format!("Failed to write .antigravity/mcp.json: {}", e))?;

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
        let path = dir.join(".antigravity").join("mcp.json");
        if !path.exists() {
            return Map::new();
        }
        discover_mcp_servers_from_json(&path, "mcpServers", identity)
    }
}

/// Pass-through normaliser: Antigravity's format is already canonical.
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
            json!({"type":"http","url":"https://api.example.com/mcp"}),
        );
        s
    }

    #[test]
    fn test_detect() {
        let dir = tempdir().unwrap();
        assert!(!Antigravity.detect_in(dir.path()));

        fs::create_dir_all(dir.path().join(".antigravity")).unwrap();
        fs::write(dir.path().join(".antigravity/mcp.json"), "{}").unwrap();
        assert!(Antigravity.detect_in(dir.path()));
    }

    #[test]
    fn test_detect_dir_only() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".antigravity")).unwrap();
        assert!(Antigravity.detect_in(dir.path()));
    }

    #[test]
    fn test_write_stdio() {
        let dir = tempdir().unwrap();
        Antigravity
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join(".antigravity/mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert!(parsed["mcpServers"]["nexus"]["type"].is_null());
        assert!(parsed["mcpServers"]["nexus"]["command"]
            .as_str()
            .unwrap()
            .contains("nexus"));
    }

    #[test]
    fn test_write_http() {
        let dir = tempdir().unwrap();
        Antigravity
            .write_mcp_config(dir.path(), &http_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join(".antigravity/mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(
            parsed["mcpServers"]["remote-api"]["type"].as_str().unwrap(),
            "http"
        );
    }
}
