use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, Agent};

/// JetBrains Junie agent — writes `.junie/mcp/mcp.json` and stores skills
/// under `<project>/.agents/skills/<name>/SKILL.md`.
///
/// Junie's canonical project instructions file is now `.junie/AGENTS.md`
/// (Junie falls back to root `AGENTS.md`, then to the legacy
/// `.junie/guidelines.md` / `.junie/guidelines/`, which are still read but
/// deprecated). MCP config moved from a flat `.junie/mcp.json` into a
/// `.junie/mcp/` subdirectory for both project and global scope.
pub struct Junie;

impl Agent for Junie {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "junie"
    }

    fn label(&self) -> &'static str {
        "Junie (Beta)"
    }

    fn config_description(&self) -> &'static str {
        ".junie/mcp/mcp.json"
    }

    fn project_file_name(&self) -> &'static str {
        ".junie/AGENTS.md"
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_in(&self, dir: &Path) -> bool {
        dir.join(".junie").join("mcp").join("mcp.json").exists()
            || dir.join(".junie").join("AGENTS.md").exists()
            || dir.join(".junie").join("guidelines.md").exists()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![
            dir.join(".junie").join("skills"),
            dir.join(".agents").join("skills"),
        ]
    }

    // ── Capabilities ────────────────────────────────────────────────────

    fn capabilities(&self) -> super::AgentCapabilities {
        super::AgentCapabilities {
            agents: false,
            global_mcp_servers: true,
            ..Default::default()
        }
    }

    // ── Cleanup ─────────────────────────────────────────────────────────

    /// Junie owns the entire `.junie/` directory — remove it all on removal.
    fn cleanup_mcp_config(&self, dir: &Path) -> Vec<String> {
        let junie_dir = dir.join(".junie");
        if junie_dir.exists() {
            if fs::remove_dir_all(&junie_dir).is_ok() {
                return vec![junie_dir.display().to_string()];
            }
        }
        vec![]
    }

    fn cleanup_mcp_preview(&self, dir: &Path) -> Vec<String> {
        let junie_dir = dir.join(".junie");
        if junie_dir.exists() {
            vec![junie_dir.display().to_string()]
        } else {
            vec![]
        }
    }

    // ── Config writing ──────────────────────────────────────────────────

    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String> {
        // Junie uses the same mcpServers JSON format as Claude Code.
        let mut junie_servers = Map::new();

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
            junie_servers.insert(name.clone(), server);
        }

        let output = json!({ "mcpServers": Value::Object(junie_servers) });

        let mcp_dir = dir.join(".junie").join("mcp");
        if !mcp_dir.exists() {
            fs::create_dir_all(&mcp_dir)
                .map_err(|e| format!("Failed to create .junie/mcp/: {}", e))?;
        }

        let path = mcp_dir.join("mcp.json");
        let content =
            serde_json::to_string_pretty(&output).map_err(|e| format!("JSON error: {}", e))?;
        fs::write(&path, content)
            .map_err(|e| format!("Failed to write .junie/mcp/mcp.json: {}", e))?;

        Ok(path.display().to_string())
    }

    fn global_mcp_target(&self) -> Option<super::GlobalMcpTarget> {
        let home = super::home_dir()?;
        Some(super::GlobalMcpTarget {
            path: home.join(".junie").join("mcp").join("mcp.json"),
            reload_note: None,
        })
    }

    fn write_global_mcp_config(
        &self,
        desired: &Map<String, Value>,
        previously_managed: &[String],
    ) -> Result<super::GlobalMcpWriteReport, String> {
        let Some(target) = self.global_mcp_target() else {
            return Err("Home directory not available for Junie global MCP write".to_string());
        };

        // Mirror the project writer's entry dialect: strip type/enabled/timeout
        // for stdio entries; leave http/sse otherwise alone.
        let mut rendered = Map::new();
        for (name, config) in desired {
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
            rendered.insert(name.clone(), server);
        }

        super::merge_global_mcp_entries_json(&target.path, "mcpServers", &rendered, previously_managed)
    }

    // ── Discovery ───────────────────────────────────────────────────────

    fn discover_mcp_servers(&self, dir: &Path) -> Map<String, Value> {
        let path = dir.join(".junie").join("mcp").join("mcp.json");
        if !path.exists() {
            return Map::new();
        }
        // Junie's format matches Claude's — no normalisation needed.
        discover_mcp_servers_from_json(&path, "mcpServers", identity)
    }

    fn detect_global_install(&self) -> bool {
        // Junie is a JetBrains plugin; check for a JetBrains IDE.
        std::path::Path::new("/Applications/IntelliJ IDEA.app").exists()
            || std::path::Path::new("/Applications/GoLand.app").exists()
            || std::path::Path::new("/Applications/WebStorm.app").exists()
            || std::path::Path::new("/Applications/PyCharm.app").exists()
            || std::path::Path::new("/Applications/PhpStorm.app").exists()
            || std::path::Path::new("/Applications/CLion.app").exists()
            || std::path::Path::new("/Applications/Rider.app").exists()
            || super::home_dir()
                .map(|h| h.join(".junie").exists())
                .unwrap_or(false)
    }

    fn discover_global_mcp_servers(&self) -> Map<String, Value> {
        let Some(home) = super::home_dir() else {
            return Map::new();
        };
        // ~/.junie/mcp/mcp.json — documented global config path, same
        // mcpServers schema as the project-level file.
        let path = home.join(".junie").join("mcp").join("mcp.json");
        discover_mcp_servers_from_json(&path, "mcpServers", identity)
    }
}

/// Pass-through normaliser: Junie's format is already canonical.
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
        assert!(!Junie.detect_in(dir.path()));

        fs::create_dir_all(dir.path().join(".junie/mcp")).unwrap();
        fs::write(dir.path().join(".junie/mcp/mcp.json"), "{}").unwrap();
        assert!(Junie.detect_in(dir.path()));
    }

    #[test]
    fn test_detect_agents_md() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".junie")).unwrap();
        fs::write(dir.path().join(".junie/AGENTS.md"), "").unwrap();
        assert!(Junie.detect_in(dir.path()));
    }

    #[test]
    fn test_cleanup_removes_junie_dir() {
        let dir = tempdir().unwrap();
        let junie_dir = dir.path().join(".junie");
        fs::create_dir_all(junie_dir.join("mcp")).unwrap();
        fs::write(junie_dir.join("mcp/mcp.json"), "{}").unwrap();
        fs::write(junie_dir.join("AGENTS.md"), "# Guidelines").unwrap();

        let removed = Junie.cleanup_mcp_config(dir.path());

        assert!(!junie_dir.exists(), ".junie/ should be deleted");
        assert_eq!(removed, vec![junie_dir.display().to_string()]);
    }

    #[test]
    fn test_cleanup_preview() {
        let dir = tempdir().unwrap();
        let junie_dir = dir.path().join(".junie");

        // No .junie dir — nothing to preview
        assert!(Junie.cleanup_mcp_preview(dir.path()).is_empty());

        fs::create_dir_all(&junie_dir).unwrap();
        let preview = Junie.cleanup_mcp_preview(dir.path());
        assert_eq!(preview, vec![junie_dir.display().to_string()]);
    }

    #[test]
    fn test_detect_guidelines() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".junie")).unwrap();
        fs::write(dir.path().join(".junie/guidelines.md"), "").unwrap();
        assert!(Junie.detect_in(dir.path()));
    }

    #[test]
    fn test_write_stdio() {
        let dir = tempdir().unwrap();
        Junie
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join(".junie/mcp/mcp.json")).unwrap();
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
        Junie.write_mcp_config(dir.path(), &http_servers()).unwrap();

        let content = fs::read_to_string(dir.path().join(".junie/mcp/mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(
            parsed["mcpServers"]["remote-api"]["type"].as_str().unwrap(),
            "http"
        );
        assert_eq!(
            parsed["mcpServers"]["remote-api"]["url"].as_str().unwrap(),
            "https://api.example.com/mcp"
        );
    }
}
