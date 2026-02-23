use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, sync_individual_skills, Agent};

/// Factory.ai Droid agent — writes `.factory/mcp.json` and stores skills
/// under `<project>/.agents/skills/<name>/SKILL.md`.
pub struct Droid;

impl Agent for Droid {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "droid"
    }

    fn label(&self) -> &'static str {
        "Droid"
    }

    fn config_description(&self) -> &'static str {
        ".factory/mcp.json"
    }

    fn project_file_name(&self) -> &'static str {
        "AGENTS.md"
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_in(&self, dir: &Path) -> bool {
        dir.join(".factory").join("mcp.json").exists()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".agents").join("skills")]
    }

    // ── Config writing ──────────────────────────────────────────────────

    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String> {
        // Droid uses mcpServers JSON format in .factory/mcp.json.
        let mut droid_servers = Map::new();

        for (name, config) in servers {
            let transport = config
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("stdio");

            let mut server = config.clone();
            if let Some(obj) = server.as_object_mut() {
                // Droid distinguishes stdio vs http via "type" field
                if transport == "stdio" {
                    obj.insert("type".to_string(), json!("stdio"));
                    obj.remove("enabled");
                    obj.remove("timeout");
                } else {
                    obj.insert("type".to_string(), json!("http"));
                }
            }
            droid_servers.insert(name.clone(), server);
        }

        let output = json!({ "mcpServers": Value::Object(droid_servers) });

        let factory_dir = dir.join(".factory");
        if !factory_dir.exists() {
            fs::create_dir_all(&factory_dir)
                .map_err(|e| format!("Failed to create .factory/: {}", e))?;
        }

        let path = factory_dir.join("mcp.json");
        let content =
            serde_json::to_string_pretty(&output).map_err(|e| format!("JSON error: {}", e))?;
        fs::write(&path, content)
            .map_err(|e| format!("Failed to write .factory/mcp.json: {}", e))?;

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
        let path = dir.join(".factory").join("mcp.json");
        if !path.exists() {
            return Map::new();
        }
        discover_mcp_servers_from_json(&path, "mcpServers", normalise_import)
    }
}

/// Normalise Droid's explicit "type" field to Nexus's canonical format.
/// Droid uses `"type": "stdio"` and `"type": "http"` explicitly.
fn normalise_import(config: Value) -> Value {
    // Droid's format is close to canonical — just pass through.
    config
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
            "linear".to_string(),
            json!({"type":"http","url":"https://mcp.linear.app/mcp"}),
        );
        s
    }

    #[test]
    fn test_detect() {
        let dir = tempdir().unwrap();
        assert!(!Droid.detect_in(dir.path()));

        fs::create_dir_all(dir.path().join(".factory")).unwrap();
        fs::write(dir.path().join(".factory/mcp.json"), "{}").unwrap();
        assert!(Droid.detect_in(dir.path()));
    }

    #[test]
    fn test_write_stdio() {
        let dir = tempdir().unwrap();
        Droid
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join(".factory/mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(
            parsed["mcpServers"]["nexus"]["type"].as_str().unwrap(),
            "stdio"
        );
        assert!(parsed["mcpServers"]["nexus"]["command"]
            .as_str()
            .unwrap()
            .contains("nexus"));
    }

    #[test]
    fn test_write_http() {
        let dir = tempdir().unwrap();
        Droid.write_mcp_config(dir.path(), &http_servers()).unwrap();

        let content = fs::read_to_string(dir.path().join(".factory/mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(
            parsed["mcpServers"]["linear"]["type"].as_str().unwrap(),
            "http"
        );
    }
}
