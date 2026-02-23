use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, sync_individual_skills, Agent};

/// Gemini CLI agent — writes MCP servers into `.gemini/settings.json`
/// under the `mcpServers` key, preserving other settings.  Stores skills
/// under `<project>/.agents/skills/<name>/SKILL.md`.
pub struct GeminiCli;

impl Agent for GeminiCli {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "gemini"
    }

    fn label(&self) -> &'static str {
        "Gemini CLI"
    }

    fn config_description(&self) -> &'static str {
        ".gemini/settings.json"
    }

    fn project_file_name(&self) -> &'static str {
        "GEMINI.md"
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_in(&self, dir: &Path) -> bool {
        dir.join("GEMINI.md").exists()
            || dir.join(".gemini").join("settings.json").exists()
            || dir.join(".gemini").exists()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".agents").join("skills")]
    }

    // ── Config writing ──────────────────────────────────────────────────

    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String> {
        // Gemini CLI stores MCP servers in .gemini/settings.json under the
        // "mcpServers" key.  We must merge with existing settings to avoid
        // clobbering auth or model config.
        let gemini_dir = dir.join(".gemini");
        if !gemini_dir.exists() {
            fs::create_dir_all(&gemini_dir)
                .map_err(|e| format!("Failed to create .gemini/: {}", e))?;
        }

        let path = gemini_dir.join("settings.json");

        // Read existing settings (if any)
        let mut root: Map<String, Value> = if path.exists() {
            let raw = fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read .gemini/settings.json: {}", e))?;
            match serde_json::from_str::<Value>(&raw) {
                Ok(Value::Object(m)) => m,
                _ => Map::new(),
            }
        } else {
            Map::new()
        };

        // Build the mcpServers object — Gemini uses the same format as
        // Claude Code (command/args/env, no "type" for stdio).
        let mut gemini_servers = Map::new();

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
            gemini_servers.insert(name.clone(), server);
        }

        root.insert("mcpServers".to_string(), Value::Object(gemini_servers));

        let content = serde_json::to_string_pretty(&Value::Object(root))
            .map_err(|e| format!("JSON error: {}", e))?;
        fs::write(&path, content)
            .map_err(|e| format!("Failed to write .gemini/settings.json: {}", e))?;

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
        let path = dir.join(".gemini").join("settings.json");
        if !path.exists() {
            return Map::new();
        }
        discover_mcp_servers_from_json(&path, "mcpServers", identity)
    }
}

/// Pass-through normaliser: Gemini's format is already canonical.
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

    #[test]
    fn test_detect() {
        let dir = tempdir().unwrap();
        assert!(!GeminiCli.detect_in(dir.path()));

        fs::write(dir.path().join("GEMINI.md"), "").unwrap();
        assert!(GeminiCli.detect_in(dir.path()));
    }

    #[test]
    fn test_write_preserves_existing_settings() {
        let dir = tempdir().unwrap();
        let gemini_dir = dir.path().join(".gemini");
        fs::create_dir_all(&gemini_dir).unwrap();

        // Write existing settings with non-MCP keys
        let existing = json!({
            "theme": "dark",
            "mcpServers": { "old": { "command": "old" } }
        });
        fs::write(
            gemini_dir.join("settings.json"),
            serde_json::to_string_pretty(&existing).unwrap(),
        )
        .unwrap();

        GeminiCli
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();

        let content = fs::read_to_string(gemini_dir.join("settings.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        // Existing non-MCP settings preserved
        assert_eq!(parsed["theme"].as_str().unwrap(), "dark");
        // MCP servers replaced
        assert!(parsed["mcpServers"]["nexus"]["command"].is_string());
        assert!(parsed["mcpServers"]["old"].is_null());
    }

    #[test]
    fn test_write_creates_dir() {
        let dir = tempdir().unwrap();
        GeminiCli
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join(".gemini/settings.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert!(parsed["mcpServers"]["nexus"]["command"]
            .as_str()
            .unwrap()
            .contains("nexus"));
    }
}
