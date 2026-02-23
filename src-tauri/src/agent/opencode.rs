use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, sync_individual_skills, Agent};

/// OpenCode agent — writes `.opencode.json` and stores skills under
/// `<project>/.agents/skills/<name>/SKILL.md`.
pub struct OpenCode;

impl Agent for OpenCode {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "opencode"
    }

    fn label(&self) -> &'static str {
        "OpenCode"
    }

    fn config_description(&self) -> &'static str {
        ".opencode.json"
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_in(&self, dir: &Path) -> bool {
        dir.join("opencode.json").exists()
            || dir.join(".opencode.json").exists()
            || dir.join(".agents").join("skills").exists()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".agents").join("skills")]
    }

    // ── Config writing ──────────────────────────────────────────────────

    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String> {
        let mut oc_servers = Map::new();

        for (name, config) in servers {
            let config = config.clone();
            let transport = config
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("stdio");

            let mut server = Map::new();

            match transport {
                "http" | "sse" => {
                    server.insert("type".to_string(), json!("remote"));

                    if let Some(url) = config.get("url") {
                        server.insert("url".to_string(), url.clone());
                    }
                    if let Some(headers) = config.get("headers") {
                        server.insert("headers".to_string(), headers.clone());
                    }
                    if let Some(oauth) = config.get("oauth") {
                        server.insert("oauth".to_string(), oauth.clone());
                    }
                }
                _ => {
                    // stdio → OpenCode "local"
                    server.insert("type".to_string(), json!("local"));

                    // command as array: [command, ...args]
                    let mut cmd_array: Vec<Value> = Vec::new();
                    if let Some(command) = config.get("command").and_then(|v| v.as_str()) {
                        cmd_array.push(json!(command));
                    }
                    if let Some(args) = config.get("args").and_then(|v| v.as_array()) {
                        for arg in args {
                            cmd_array.push(arg.clone());
                        }
                    }
                    if !cmd_array.is_empty() {
                        server.insert("command".to_string(), Value::Array(cmd_array));
                    }

                    // "environment" instead of "env"
                    if let Some(env) = config.get("env").and_then(|v| v.as_object()) {
                        if !env.is_empty() {
                            server.insert("environment".to_string(), Value::Object(env.clone()));
                        }
                    }
                }
            }

            if let Some(enabled) = config.get("enabled") {
                if enabled.as_bool() == Some(false) {
                    server.insert("enabled".to_string(), json!(false));
                }
            }
            if let Some(timeout) = config.get("timeout") {
                server.insert("timeout".to_string(), timeout.clone());
            }

            oc_servers.insert(name.clone(), Value::Object(server));
        }

        let output = json!({ "mcp": Value::Object(oc_servers) });
        let path = dir.join(".opencode.json");
        let content =
            serde_json::to_string_pretty(&output).map_err(|e| format!("JSON error: {}", e))?;
        fs::write(&path, content).map_err(|e| format!("Failed to write .opencode.json: {}", e))?;

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
        let mut result = Map::new();

        for filename in &["opencode.json", ".opencode.json"] {
            let path = dir.join(filename);
            if path.exists() {
                let found = discover_mcp_servers_from_json(&path, "mcp", normalise_import);
                result.extend(found);
            }
        }

        result
    }
}

/// Convert an OpenCode MCP server config to Nexus's canonical format.
///
/// - `type: "local"` → `type: "stdio"`, command array → command + args
/// - `type: "remote"` → `type: "http"`
/// - `environment` → `env`
fn normalise_import(mut config: Value) -> Value {
    if let Some(obj) = config.as_object_mut() {
        if let Some(Value::String(t)) = obj.get("type") {
            if t == "local" {
                obj.insert("type".to_string(), json!("stdio"));
                if let Some(Value::Array(cmd_arr)) = obj.remove("command") {
                    if !cmd_arr.is_empty() {
                        obj.insert("command".to_string(), cmd_arr[0].clone());
                        if cmd_arr.len() > 1 {
                            obj.insert("args".to_string(), Value::Array(cmd_arr[1..].to_vec()));
                        }
                    }
                }
                if let Some(env) = obj.remove("environment") {
                    obj.insert("env".to_string(), env);
                }
            } else if t == "remote" {
                obj.insert("type".to_string(), json!("http"));
            }
        }
    }
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
        assert!(!OpenCode.detect_in(dir.path()));

        fs::write(dir.path().join(".opencode.json"), "{}").unwrap();
        assert!(OpenCode.detect_in(dir.path()));
    }

    #[test]
    fn test_write_stdio() {
        let dir = tempdir().unwrap();
        OpenCode
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join(".opencode.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(parsed["mcp"]["nexus"]["type"].as_str().unwrap(), "local");
        let cmd = parsed["mcp"]["nexus"]["command"].as_array().unwrap();
        assert_eq!(cmd[0].as_str().unwrap(), "/usr/local/bin/nexus");
        assert_eq!(cmd[1].as_str().unwrap(), "mcp-serve");

        let env = parsed["mcp"]["github"]["environment"].as_object().unwrap();
        assert_eq!(env["GITHUB_TOKEN"].as_str().unwrap(), "ghp_test123");
    }

    #[test]
    fn test_write_http() {
        let dir = tempdir().unwrap();
        OpenCode
            .write_mcp_config(dir.path(), &http_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join(".opencode.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(
            parsed["mcp"]["remote-api"]["type"].as_str().unwrap(),
            "remote"
        );
        assert_eq!(
            parsed["mcp"]["remote-api"]["url"].as_str().unwrap(),
            "https://api.example.com/mcp"
        );
        assert_eq!(
            parsed["mcp"]["remote-api"]["oauth"]["clientId"]
                .as_str()
                .unwrap(),
            "client_123"
        );
    }
}
