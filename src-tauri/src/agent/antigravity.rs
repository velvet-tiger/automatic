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
        "Antigravity (Beta)"
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

    // ── Cleanup ─────────────────────────────────────────────────────────

    /// `detect_in` matches the `.antigravity/` directory itself, so we must
    /// remove the whole directory on cleanup — not just `mcp.json` inside it —
    /// otherwise the empty dir re-triggers detection on the next autodetect.
    fn cleanup_mcp_config(&self, dir: &Path) -> Vec<String> {
        let ag_dir = dir.join(".antigravity");
        if ag_dir.exists() {
            if fs::remove_dir_all(&ag_dir).is_ok() {
                return vec![ag_dir.display().to_string()];
            }
        }
        vec![]
    }

    fn cleanup_mcp_preview(&self, dir: &Path) -> Vec<String> {
        let ag_dir = dir.join(".antigravity");
        if ag_dir.exists() {
            vec![ag_dir.display().to_string()]
        } else {
            vec![]
        }
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
            "automatic".to_string(),
            json!({"type":"stdio","command":"/usr/local/bin/automatic","args":["mcp-serve"]}),
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
    fn test_cleanup_removes_antigravity_dir() {
        let dir = tempdir().unwrap();
        let ag_dir = dir.path().join(".antigravity");
        fs::create_dir_all(&ag_dir).unwrap();
        fs::write(ag_dir.join("mcp.json"), "{}").unwrap();
        assert!(ag_dir.exists());

        let removed = Antigravity.cleanup_mcp_config(dir.path());
        assert_eq!(removed, vec![ag_dir.display().to_string()]);
        assert!(!ag_dir.exists(), ".antigravity/ should be deleted");
    }

    #[test]
    fn test_cleanup_preview_antigravity_dir() {
        let dir = tempdir().unwrap();
        assert!(Antigravity.cleanup_mcp_preview(dir.path()).is_empty());

        let ag_dir = dir.path().join(".antigravity");
        fs::create_dir_all(&ag_dir).unwrap();
        let preview = Antigravity.cleanup_mcp_preview(dir.path());
        assert_eq!(preview, vec![ag_dir.display().to_string()]);
    }

    #[test]
    fn test_write_stdio() {
        let dir = tempdir().unwrap();
        Antigravity
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join(".antigravity/mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert!(parsed["mcpServers"]["automatic"]["type"].is_null());
        assert!(parsed["mcpServers"]["automatic"]["command"]
            .as_str()
            .unwrap()
            .contains("automatic"));
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
