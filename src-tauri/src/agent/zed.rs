use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, sync_individual_skills, Agent};

/// Zed agent — writes MCP servers into `.zed/settings.json` under the
/// `context_servers` key, preserving other settings.  Also writes global
/// config to `~/.config/zed/settings.json`.  Stores skills under
/// `<project>/.agents/skills/<name>/SKILL.md`.
pub struct Zed;

impl Agent for Zed {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "zed"
    }

    fn label(&self) -> &'static str {
        "Zed (Beta)"
    }

    fn config_description(&self) -> &'static str {
        ".zed/settings.json"
    }

    fn project_file_name(&self) -> &'static str {
        ".rules"
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_in(&self, dir: &Path) -> bool {
        dir.join(".zed").join("settings.json").exists()
            || dir.join(".rules").exists()
            || dir.join(".zed").exists()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".agents").join("skills")]
    }

    // ── Config writing ──────────────────────────────────────────────────

    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String> {
        let zed_dir = dir.join(".zed");
        if !zed_dir.exists() {
            fs::create_dir_all(&zed_dir)
                .map_err(|e| format!("Failed to create .zed/: {}", e))?;
        }

        let path = zed_dir.join("settings.json");

        // Read existing settings to preserve non-MCP config
        let mut root: Map<String, Value> = if path.exists() {
            let raw = fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read .zed/settings.json: {}", e))?;
            match serde_json::from_str::<Value>(&raw) {
                Ok(Value::Object(m)) => m,
                _ => Map::new(),
            }
        } else {
            Map::new()
        };

        // Build the context_servers object — Zed uses command/args/env directly
        let mut zed_servers = Map::new();

        for (name, config) in servers {
            let transport = config
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("stdio");

            let mut server = config.clone();
            if let Some(obj) = server.as_object_mut() {
                // Strip fields that are not part of Zed's format
                obj.remove("type");
                obj.remove("enabled");
                obj.remove("timeout");

                // Zed remote servers use "url" + optional "headers", which
                // matches the canonical format already.  For stdio servers
                // the canonical "command"/"args"/"env" also match.
                if transport == "http" || transport == "sse" {
                    // Keep url/headers, remove command/args that don't apply
                    obj.remove("command");
                    obj.remove("args");
                }
            }
            zed_servers.insert(name.clone(), server);
        }

        root.insert(
            "context_servers".to_string(),
            Value::Object(zed_servers),
        );

        let content = serde_json::to_string_pretty(&Value::Object(root))
            .map_err(|e| format!("JSON error: {}", e))?;
        fs::write(&path, content)
            .map_err(|e| format!("Failed to write .zed/settings.json: {}", e))?;

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

    // ── Cleanup ─────────────────────────────────────────────────────────

    /// Zed merges into `.zed/settings.json` which may contain user agent,
    /// font, and theme settings.  Strip only the `context_servers` key
    /// rather than deleting the whole file.
    fn cleanup_mcp_config(&self, dir: &Path) -> Vec<String> {
        let path = dir.join(".zed").join("settings.json");
        if !path.exists() {
            return vec![];
        }
        let raw = match fs::read_to_string(&path) {
            Ok(r) => r,
            Err(_) => return vec![],
        };
        let mut root: Map<String, Value> = match serde_json::from_str::<Value>(&raw) {
            Ok(Value::Object(m)) => m,
            _ => return vec![],
        };
        if root.remove("context_servers").is_none() {
            return vec![];
        }
        if root.is_empty() {
            if fs::remove_file(&path).is_ok() {
                return vec![path.display().to_string()];
            }
        } else {
            let content = match serde_json::to_string_pretty(&Value::Object(root)) {
                Ok(c) => c,
                Err(_) => return vec![],
            };
            if fs::write(&path, content).is_ok() {
                return vec![path.display().to_string()];
            }
        }
        vec![]
    }

    fn cleanup_mcp_preview(&self, dir: &Path) -> Vec<String> {
        let path = dir.join(".zed").join("settings.json");
        if path.exists() {
            vec![path.display().to_string()]
        } else {
            vec![]
        }
    }

    // ── Discovery ───────────────────────────────────────────────────────

    fn discover_mcp_servers(&self, dir: &Path) -> Map<String, Value> {
        let path = dir.join(".zed").join("settings.json");
        if !path.exists() {
            return Map::new();
        }
        discover_mcp_servers_from_json(&path, "context_servers", normalise_zed_server)
    }

    fn detect_global_install(&self) -> bool {
        Path::new("/Applications/Zed.app").exists()
            || super::cli_available("zed")
            || super::cli_available("zeditor")
            || global_config_dir().map(|d| d.exists()).unwrap_or(false)
    }

    fn discover_global_mcp_servers(&self) -> Map<String, Value> {
        let Some(config_dir) = global_config_dir() else {
            return Map::new();
        };
        let path = config_dir.join("settings.json");
        discover_mcp_servers_from_json(&path, "context_servers", normalise_zed_server)
    }

    fn agents_dir(&self, dir: &Path) -> Option<PathBuf> {
        Some(dir.join(".zed").join("agents"))
    }
}

/// Returns the platform-appropriate Zed global config directory.
///
/// - macOS / Linux: `~/.config/zed/`
fn global_config_dir() -> Option<PathBuf> {
    super::home_dir().map(|h| h.join(".config").join("zed"))
}

/// Normalise a Zed `context_servers` entry to Automatic's canonical format.
///
/// Zed's stdio servers already use `command`/`args`/`env` which matches the
/// canonical format.  Remote servers use `url`/`headers`.  We add a `type`
/// field so downstream code can distinguish transport.
fn normalise_zed_server(v: Value) -> Value {
    let Some(obj) = v.as_object() else {
        return v;
    };
    let mut out = obj.clone();

    if !out.contains_key("type") {
        if out.contains_key("url") {
            out.insert("type".to_string(), Value::String("http".to_string()));
        } else {
            out.insert("type".to_string(), Value::String("stdio".to_string()));
        }
    }
    Value::Object(out)
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
            json!({"type":"http","url":"https://api.example.com/mcp","headers":{"Authorization":"Bearer tok_abc123"}}),
        );
        s
    }

    #[test]
    fn test_detect() {
        let dir = tempdir().unwrap();
        assert!(!Zed.detect_in(dir.path()));

        fs::create_dir_all(dir.path().join(".zed")).unwrap();
        fs::write(dir.path().join(".zed/settings.json"), "{}").unwrap();
        assert!(Zed.detect_in(dir.path()));
    }

    #[test]
    fn test_detect_rules_file() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join(".rules"), "").unwrap();
        assert!(Zed.detect_in(dir.path()));
    }

    #[test]
    fn test_write_stdio() {
        let dir = tempdir().unwrap();
        Zed.write_mcp_config(dir.path(), &stdio_servers()).unwrap();

        let content = fs::read_to_string(dir.path().join(".zed/settings.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        // stdio entries should have "type" stripped
        assert!(parsed["context_servers"]["automatic"]["type"].is_null());
        assert!(parsed["context_servers"]["automatic"]["command"]
            .as_str()
            .unwrap()
            .contains("automatic"));
        assert_eq!(
            parsed["context_servers"]["github"]["command"].as_str().unwrap(),
            "npx"
        );
    }

    #[test]
    fn test_write_http() {
        let dir = tempdir().unwrap();
        Zed.write_mcp_config(dir.path(), &http_servers()).unwrap();

        let content = fs::read_to_string(dir.path().join(".zed/settings.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(
            parsed["context_servers"]["remote-api"]["url"]
                .as_str()
                .unwrap(),
            "https://api.example.com/mcp"
        );
        // HTTP servers should not have command/args
        assert!(parsed["context_servers"]["remote-api"]["command"].is_null());
    }

    #[test]
    fn test_write_preserves_existing_settings() {
        let dir = tempdir().unwrap();
        let zed_dir = dir.path().join(".zed");
        fs::create_dir_all(&zed_dir).unwrap();

        let existing = json!({
            "agent": { "default_model": { "provider": "ollama", "model": "qwen3.5" } },
            "ui_font_size": 16,
            "context_servers": { "old": { "command": "old" } }
        });
        fs::write(
            zed_dir.join("settings.json"),
            serde_json::to_string_pretty(&existing).unwrap(),
        )
        .unwrap();

        Zed.write_mcp_config(dir.path(), &stdio_servers()).unwrap();

        let content = fs::read_to_string(zed_dir.join("settings.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        // Existing non-MCP settings preserved
        assert_eq!(parsed["ui_font_size"].as_u64().unwrap(), 16);
        assert_eq!(
            parsed["agent"]["default_model"]["provider"].as_str().unwrap(),
            "ollama"
        );
        // MCP servers replaced
        assert!(parsed["context_servers"]["automatic"]["command"].is_string());
        assert!(parsed["context_servers"]["old"].is_null());
    }

    #[test]
    fn test_cleanup_strips_context_servers() {
        let dir = tempdir().unwrap();
        let zed_dir = dir.path().join(".zed");
        fs::create_dir_all(&zed_dir).unwrap();

        let existing = json!({
            "ui_font_size": 16,
            "context_servers": { "auto": { "command": "automatic" } }
        });
        fs::write(
            zed_dir.join("settings.json"),
            serde_json::to_string_pretty(&existing).unwrap(),
        )
        .unwrap();

        let removed = Zed.cleanup_mcp_config(dir.path());
        assert_eq!(removed.len(), 1);

        let content = fs::read_to_string(zed_dir.join("settings.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert!(parsed["context_servers"].is_null());
        assert_eq!(parsed["ui_font_size"].as_u64().unwrap(), 16);
    }

    #[test]
    fn test_cleanup_deletes_empty_file() {
        let dir = tempdir().unwrap();
        let zed_dir = dir.path().join(".zed");
        fs::create_dir_all(&zed_dir).unwrap();

        let existing = json!({
            "context_servers": { "auto": { "command": "automatic" } }
        });
        fs::write(
            zed_dir.join("settings.json"),
            serde_json::to_string_pretty(&existing).unwrap(),
        )
        .unwrap();

        let removed = Zed.cleanup_mcp_config(dir.path());
        assert_eq!(removed.len(), 1);
        assert!(!zed_dir.join("settings.json").exists());
    }

    #[test]
    fn test_discover_mcp_servers() {
        let dir = tempdir().unwrap();
        let zed_dir = dir.path().join(".zed");
        fs::create_dir_all(&zed_dir).unwrap();

        let settings = json!({
            "context_servers": {
                "my-server": { "command": "/usr/bin/my-server", "args": ["--port", "3000"] }
            }
        });
        fs::write(
            zed_dir.join("settings.json"),
            serde_json::to_string_pretty(&settings).unwrap(),
        )
        .unwrap();

        let servers = Zed.discover_mcp_servers(dir.path());
        assert!(servers.contains_key("my-server"));
        // Normaliser should add type: stdio
        assert_eq!(
            servers["my-server"]["type"].as_str().unwrap(),
            "stdio"
        );
    }

    #[test]
    fn test_normalise_adds_type() {
        let stdio = json!({"command": "foo", "args": []});
        let result = normalise_zed_server(stdio);
        assert_eq!(result["type"].as_str().unwrap(), "stdio");

        let http = json!({"url": "https://example.com/mcp"});
        let result = normalise_zed_server(http);
        assert_eq!(result["type"].as_str().unwrap(), "http");
    }
}
