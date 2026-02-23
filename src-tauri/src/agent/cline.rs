use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, sync_individual_skills, Agent};

/// Cline agent — writes `.cline/mcp.json` and stores skills under
/// `<project>/.cline/skills/<name>/SKILL.md`.
pub struct Cline;

impl Agent for Cline {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "cline"
    }

    fn label(&self) -> &'static str {
        "Cline"
    }

    fn config_description(&self) -> &'static str {
        ".cline/mcp.json"
    }

    fn project_file_name(&self) -> &'static str {
        "AGENTS.md"
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_in(&self, dir: &Path) -> bool {
        dir.join(".cline").join("mcp.json").exists()
            || dir.join(".clinerules").exists()
            || dir.join(".cline").join("skills").exists()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".cline").join("skills")]
    }

    // ── Config writing ──────────────────────────────────────────────────

    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String> {
        // Cline uses the same mcpServers JSON format as Claude Code.
        let mut cline_servers = Map::new();

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
            cline_servers.insert(name.clone(), server);
        }

        let output = json!({ "mcpServers": Value::Object(cline_servers) });

        let cline_dir = dir.join(".cline");
        if !cline_dir.exists() {
            fs::create_dir_all(&cline_dir)
                .map_err(|e| format!("Failed to create .cline/: {}", e))?;
        }

        let path = cline_dir.join("mcp.json");
        let content =
            serde_json::to_string_pretty(&output).map_err(|e| format!("JSON error: {}", e))?;
        fs::write(&path, content).map_err(|e| format!("Failed to write .cline/mcp.json: {}", e))?;

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
        let skills_dir = dir.join(".cline").join("skills");
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
        let path = dir.join(".cline").join("mcp.json");
        if !path.exists() {
            return Map::new();
        }
        discover_mcp_servers_from_json(&path, "mcpServers", identity)
    }
}

/// Pass-through normaliser: Cline's format is already canonical.
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

    #[test]
    fn test_detect() {
        let dir = tempdir().unwrap();
        assert!(!Cline.detect_in(dir.path()));

        fs::create_dir_all(dir.path().join(".cline")).unwrap();
        fs::write(dir.path().join(".cline/mcp.json"), "{}").unwrap();
        assert!(Cline.detect_in(dir.path()));
    }

    #[test]
    fn test_detect_clinerules() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join(".clinerules"), "").unwrap();
        assert!(Cline.detect_in(dir.path()));
    }

    #[test]
    fn test_write_stdio() {
        let dir = tempdir().unwrap();
        Cline
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join(".cline/mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert!(parsed["mcpServers"]["nexus"]["type"].is_null());
        assert!(parsed["mcpServers"]["nexus"]["command"]
            .as_str()
            .unwrap()
            .contains("nexus"));
    }
}
