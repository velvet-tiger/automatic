use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

use super::{
    discover_mcp_servers_from_json, normalise_opencode_dialect_server,
    write_opencode_dialect_mcp_config, Agent,
};

/// Kilo agent — writes `.kilo/kilo.json` (or an existing root `kilo.json`)
/// and stores skills under `<project>/.agents/skills/<name>/SKILL.md`.
///
/// Kilo Code rebranded to Kilo and rebuilt its configuration around an
/// OpenCode-derived format: project config at `kilo.json[c]` or
/// `.kilo/kilo.json[c]`, global config at `~/.config/kilo/kilo.json[c]`, MCP
/// servers under a top-level `mcp` key using the same `type: "local"/"remote"`
/// dialect OpenCode writes. `.kilocode/mcp.json`, the pre-rebrand path, is no
/// longer read by Kilo; see `migrate_legacy_kilocode` in `sync/engine.rs` for
/// the one-time migration off it.
pub struct KiloCode;

/// The config file Automatic reads from and writes to for a given project.
///
/// An existing root `kilo.json`/`kilo.jsonc` takes precedence over
/// `.kilo/kilo.json`/`.kilo/kilo.jsonc` (matching Kilo's own precedence:
/// project-root config over the `.kilo/` subdirectory). Absent any existing
/// file, new projects default to `.kilo/kilo.json`.
fn resolve_config_path(dir: &Path) -> PathBuf {
    for candidate in [
        dir.join("kilo.json"),
        dir.join("kilo.jsonc"),
        dir.join(".kilo").join("kilo.json"),
        dir.join(".kilo").join("kilo.jsonc"),
    ] {
        if candidate.is_file() {
            return candidate;
        }
    }
    dir.join(".kilo").join("kilo.json")
}

/// Candidate global config paths, in the order Kilo would read them.
fn global_config_candidates() -> Vec<PathBuf> {
    match dirs::home_dir() {
        Some(home) => {
            let base = home.join(".config").join("kilo");
            vec![base.join("kilo.json"), base.join("kilo.jsonc")]
        }
        None => vec![],
    }
}

impl Agent for KiloCode {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "kilo"
    }

    fn label(&self) -> &'static str {
        "Kilo (Beta)"
    }

    fn config_description(&self) -> &'static str {
        ".kilo/kilo.json"
    }

    fn project_file_name(&self) -> &'static str {
        "AGENTS.md"
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_in(&self, dir: &Path) -> bool {
        dir.join("kilo.json").exists()
            || dir.join("kilo.jsonc").exists()
            || dir.join(".kilo").exists()
            // Legacy marker: pre-rebrand Kilo Code projects, until migrated.
            || dir.join(".kilocode").exists()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".agents").join("skills")]
    }

    // ── Capabilities ────────────────────────────────────────────────────

    fn capabilities(&self) -> super::AgentCapabilities {
        super::AgentCapabilities {
            agents: false,
            ..Default::default()
        }
    }

    // ── Cleanup ─────────────────────────────────────────────────────────

    fn owned_config_paths(&self, dir: &Path) -> Vec<PathBuf> {
        vec![resolve_config_path(dir)]
    }

    // ── Config writing ──────────────────────────────────────────────────

    fn mcp_merge_inputs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![resolve_config_path(dir)]
    }

    /// Merge into the resolved config path rather than rebuilding it — the
    /// file may carry the user's own Kilo settings alongside `mcp`. Kilo has
    /// no published `$schema`, unlike OpenCode, so none is written.
    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String> {
        write_opencode_dialect_mcp_config(&resolve_config_path(dir), None, servers)
    }

    // ── Discovery ───────────────────────────────────────────────────────

    fn discover_mcp_servers(&self, dir: &Path) -> Map<String, Value> {
        let path = resolve_config_path(dir);
        if !path.is_file() {
            return Map::new();
        }
        discover_mcp_servers_from_json(&path, "mcp", normalise_opencode_dialect_server)
    }

    fn detect_global_install(&self) -> bool {
        super::cli_available("kilo") || global_config_candidates().iter().any(|p| p.is_file())
    }

    fn discover_global_mcp_servers(&self) -> Map<String, Value> {
        for candidate in global_config_candidates() {
            if candidate.is_file() {
                return discover_mcp_servers_from_json(
                    &candidate,
                    "mcp",
                    normalise_opencode_dialect_server,
                );
            }
        }
        Map::new()
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
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
    fn test_detect_kilo_json() {
        let dir = tempdir().unwrap();
        assert!(!KiloCode.detect_in(dir.path()));
        fs::write(dir.path().join("kilo.json"), "{}").unwrap();
        assert!(KiloCode.detect_in(dir.path()));
    }

    #[test]
    fn test_detect_kilo_jsonc() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("kilo.jsonc"), "{}").unwrap();
        assert!(KiloCode.detect_in(dir.path()));
    }

    #[test]
    fn test_detect_dot_kilo_dir() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".kilo")).unwrap();
        assert!(KiloCode.detect_in(dir.path()));
    }

    #[test]
    fn test_detect_legacy_kilocode_marker() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".kilocode")).unwrap();
        assert!(
            KiloCode.detect_in(dir.path()),
            ".kilocode/ must still be recognised until migrated"
        );
    }

    #[test]
    fn test_write_defaults_to_dot_kilo_kilo_json_with_no_schema() {
        let dir = tempdir().unwrap();
        let path = KiloCode
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();

        assert_eq!(path, dir.path().join(".kilo/kilo.json").display().to_string());
        let content = fs::read_to_string(dir.path().join(".kilo/kilo.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert!(
            parsed.get("$schema").is_none(),
            "Kilo has no published schema; $schema should not be written"
        );
        assert_eq!(parsed["mcp"]["automatic"]["type"].as_str().unwrap(), "local");
        assert_eq!(
            parsed["mcp"]["automatic"]["command"][0].as_str().unwrap(),
            "/usr/local/bin/automatic"
        );
        assert_eq!(
            parsed["mcp"]["github"]["environment"]["GITHUB_TOKEN"]
                .as_str()
                .unwrap(),
            "ghp_test123"
        );
    }

    #[test]
    fn test_write_http_as_remote() {
        let dir = tempdir().unwrap();
        KiloCode
            .write_mcp_config(dir.path(), &http_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join(".kilo/kilo.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(parsed["mcp"]["remote-api"]["type"].as_str().unwrap(), "remote");
        assert_eq!(
            parsed["mcp"]["remote-api"]["url"].as_str().unwrap(),
            "https://api.example.com/mcp"
        );
    }

    #[test]
    fn test_write_prefers_existing_root_kilo_json() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("kilo.json"), r#"{"model":"gpt-5"}"#).unwrap();

        let path = KiloCode
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();

        assert_eq!(path, dir.path().join("kilo.json").display().to_string());
        assert!(
            !dir.path().join(".kilo").exists(),
            "must not also create .kilo/ when a root file already exists"
        );
    }

    #[test]
    fn test_write_merge_preserves_unrelated_keys() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".kilo")).unwrap();
        fs::write(
            dir.path().join(".kilo/kilo.json"),
            r#"{"model":"gpt-5","agent":{"custom":true}}"#,
        )
        .unwrap();

        KiloCode
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join(".kilo/kilo.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(parsed["model"].as_str().unwrap(), "gpt-5");
        assert!(parsed["agent"]["custom"].as_bool().unwrap());
        assert!(parsed["mcp"]["automatic"].is_object());
    }

    #[test]
    fn test_owned_config_paths_never_includes_agents_md() {
        let dir = tempdir().unwrap();
        let paths = KiloCode.owned_config_paths(dir.path());
        assert!(
            !paths.contains(&dir.path().join("AGENTS.md")),
            "AGENTS.md is shared with seven other agents and is not Kilo's to own"
        );
    }

    #[test]
    fn test_discover_reads_opencode_dialect() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".kilo")).unwrap();
        fs::write(
            dir.path().join(".kilo/kilo.json"),
            r#"{"mcp":{"github":{"type":"local","command":["npx","-y","server-github"],"environment":{"GITHUB_TOKEN":"tok"}}}}"#,
        )
        .unwrap();

        let servers = KiloCode.discover_mcp_servers(dir.path());
        assert_eq!(servers["github"]["type"].as_str().unwrap(), "stdio");
        assert_eq!(servers["github"]["command"].as_str().unwrap(), "npx");
        assert_eq!(
            servers["github"]["env"]["GITHUB_TOKEN"].as_str().unwrap(),
            "tok"
        );
    }
}
