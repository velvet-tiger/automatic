use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, sync_individual_skills, Agent};

/// Goose agent (Block) — writes `.goose/mcp.json` and stores skills under
/// `<project>/.agents/skills/<name>/SKILL.md`.
///
/// Goose natively uses `~/.config/goose/config.yaml` for extensions (YAML),
/// but we write a project-level `.goose/mcp.json` using the standard
/// `mcpServers` format for consistency with Nexus's sync model.
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
}

/// Pass-through normaliser.
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
