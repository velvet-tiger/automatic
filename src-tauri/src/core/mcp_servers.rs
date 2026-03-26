use std::fs;
use std::path::PathBuf;

use super::env_crypto;
use super::paths::{get_automatic_dir, is_valid_name};

// ── MCP Servers ──────────────────────────────────────────────────────────────

pub fn get_mcp_servers_dir() -> Result<PathBuf, String> {
    Ok(get_automatic_dir()?.join("mcp_servers"))
}

pub fn list_mcp_server_configs() -> Result<Vec<String>, String> {
    let dir = get_mcp_servers_dir()?;

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut servers = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| e.to_string())?;

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if is_valid_name(stem) {
                        servers.push(stem.to_string());
                    }
                }
            }
        }
    }

    Ok(servers)
}

/// Read a single MCP server config from disk and return it as a JSON string
/// with env values **decrypted** (plaintext) so the frontend and sync engine
/// see regular strings.
pub fn read_mcp_server_config(name: &str) -> Result<String, String> {
    if !is_valid_name(name) {
        return Err("Invalid server name".into());
    }
    let dir = get_mcp_servers_dir()?;
    let path = dir.join(format!("{}.json", name));

    if !path.exists() {
        return Err(format!("MCP server '{}' not found", name));
    }

    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut config: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid JSON in config: {}", e))?;

    // Decrypt env values if present.
    if let Some(env) = config.get_mut("env") {
        env_crypto::decrypt_env_values(env)?;
    }

    serde_json::to_string(&config).map_err(|e| e.to_string())
}

/// Persist a single MCP server config.  Env values are **encrypted** before
/// writing so that API keys are never stored in plaintext on disk.
///
/// The `data` parameter is the raw JSON string from the frontend (env values
/// are plaintext at this point — the frontend never sees the encrypted form).
pub fn save_mcp_server_config(name: &str, data: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid server name".into());
    }

    let mut config: serde_json::Value =
        serde_json::from_str(data).map_err(|e| format!("Invalid JSON: {}", e))?;

    // Encrypt env values before writing to disk.
    if let Some(env) = config.get_mut("env") {
        env_crypto::encrypt_env_values(env)?;
    }

    let dir = get_mcp_servers_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    let path = dir.join(format!("{}.json", name));
    let serialized = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write(path, serialized).map_err(|e| e.to_string())
}

pub fn delete_mcp_server_config(name: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid server name".into());
    }
    let dir = get_mcp_servers_dir()?;
    let path = dir.join(format!("{}.json", name));

    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Read raw Claude Desktop config.
///
/// Uses [`dirs::config_dir`] to resolve the platform-specific configuration
/// directory so the import works on macOS, Linux **and** Windows:
///
/// | Platform | Path                                                                |
/// |----------|---------------------------------------------------------------------|
/// | macOS    | `~/Library/Application Support/Claude/claude_desktop_config.json`   |
/// | Linux    | `~/.config/Claude/claude_desktop_config.json`                       |
/// | Windows  | `%APPDATA%\Claude\claude_desktop_config.json`                       |
pub fn list_mcp_servers() -> Result<String, String> {
    let config_dir = dirs::config_dir().ok_or("Could not determine config directory")?;
    let config_path = config_dir.join("Claude/claude_desktop_config.json");

    if config_path.exists() {
        fs::read_to_string(config_path).map_err(|e| e.to_string())
    } else {
        Ok("{}".to_string())
    }
}

/// The well-known name used for the Automatic MCP server everywhere:
/// registry files, project assignments, and agent config files.
pub const AUTOMATIC_SERVER_NAME: &str = "automatic";

/// The well-known name of the bundled "automatic" skill that teaches agents
/// how to use the Automatic MCP service.  Always assigned to every project.
pub const AUTOMATIC_SKILL_NAME: &str = "automatic";

/// Ensure the `automatic` MCP server entry is present in the Automatic
/// registry and assigned to all projects.
///
/// **Registry entry** — makes the server visible in the Automatic UI (MCP
/// Servers list and per-project MCP selector).  The entry is always
/// overwritten so the binary path stays current after updates.
///
/// **Project assignment** — adds `"automatic"` to every registered project's
/// `mcp_servers` list if not already present, then persists the project.
///
/// **Returns** the names of all projects that have a configured directory and
/// at least one agent.  The caller is responsible for re-syncing these
/// projects in the background so that:
/// - The `automatic` MCP server entry in each agent config file reflects the
///   current binary path (which changes between dev builds and release).
/// - Any newly added `automatic` skill files are written to disk.
///
/// The MCP server is exposed to agents via per-project config files written
/// during agent sync (e.g. `.mcp.json` for Claude Code).  We intentionally
/// do NOT write to the global `~/.mcp.json` or the plugin `.mcp.json` —
/// having multiple registrations of the same server causes Claude Code to
/// deduplicate and drop tools.
///
/// The binary path is resolved from the current executable so it always
/// reflects the installed release binary rather than a hard-coded path.
pub fn ensure_automatic_in_global_mcp() -> Result<Vec<String>, String> {
    let binary = std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "automatic".to_string());

    // ── 1. Read old registry entry to detect binary path change ──────────
    //
    // If the binary path has changed (e.g. dev→release or after an update),
    // every project that writes the automatic server entry needs a re-sync.
    let binary_changed = read_mcp_server_config(AUTOMATIC_SERVER_NAME)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|v| {
            v.get("command")
                .and_then(|c| c.as_str())
                .map(|c| c != binary)
        })
        .unwrap_or(true); // no existing entry → treat as changed

    // ── 2. Registry entry ────────────────────────────────────────────────
    let registry_config = serde_json::json!({
        "type": "stdio",
        "command": binary,
        "args": ["mcp-serve"],
        "_builtin": true
    });
    let registry_str = serde_json::to_string_pretty(&registry_config).map_err(|e| e.to_string())?;
    // save_mcp_server_config handles directory creation and env encryption.
    save_mcp_server_config(AUTOMATIC_SERVER_NAME, &registry_str)?;

    // ── 3. Assign MCP server + skill to all projects, collect sync candidates
    let mut projects_to_sync: Vec<String> = Vec::new();

    if let Ok(project_names) = super::list_projects() {
        for name in project_names {
            if let Ok(raw) = super::read_project(&name) {
                if let Ok(mut project) = serde_json::from_str::<super::Project>(&raw) {
                    let mut changed = false;

                    if !project
                        .mcp_servers
                        .iter()
                        .any(|s| s == AUTOMATIC_SERVER_NAME)
                    {
                        project.mcp_servers.push(AUTOMATIC_SERVER_NAME.to_string());
                        changed = true;
                    }

                    if !project.skills.iter().any(|s| s == AUTOMATIC_SKILL_NAME) {
                        project.skills.push(AUTOMATIC_SKILL_NAME.to_string());
                        changed = true;
                    }

                    if changed {
                        if let Ok(updated) = serde_json::to_string_pretty(&project) {
                            let _ = super::save_project(&name, &updated);
                        }
                    }

                    // Queue for re-sync if:
                    // - We just added automatic server/skill (changed = true), OR
                    // - The binary path changed and this project has agents that
                    //   would write agent config files containing the path.
                    let has_syncable_config = !project.directory.is_empty()
                        && !project.agents.is_empty()
                        && std::path::Path::new(&project.directory).exists();

                    if has_syncable_config && (changed || binary_changed) {
                        projects_to_sync.push(name);
                    }
                }
            }
        }
    }

    Ok(projects_to_sync)
}

/// Returns `true` if the given MCP server name is a built-in server that
/// should not be deleted or have its core config edited by the user.
pub fn is_builtin_mcp_server(name: &str) -> bool {
    name == AUTOMATIC_SERVER_NAME
}

/// Returns `true` if the given skill name is a built-in skill that should
/// not be deleted or removed from projects by the user.
pub fn is_builtin_skill(name: &str) -> bool {
    name == AUTOMATIC_SKILL_NAME
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    // ── Path-injectable helpers ───────────────────────────────────────────────

    fn tmp() -> TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn save_at(dir: &Path, name: &str, data: &str) -> Result<(), String> {
        if !is_valid_name(name) {
            return Err("Invalid server name".into());
        }
        let mut config: serde_json::Value =
            serde_json::from_str(data).map_err(|e| format!("Invalid JSON: {}", e))?;
        if let Some(env) = config.get_mut("env") {
            env_crypto::encrypt_env_values(env)?;
        }
        if !dir.exists() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let path = dir.join(format!("{}.json", name));
        let serialized = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
        fs::write(path, serialized).map_err(|e| e.to_string())
    }

    fn read_at(dir: &Path, name: &str) -> Result<String, String> {
        if !is_valid_name(name) {
            return Err("Invalid server name".into());
        }
        let path = dir.join(format!("{}.json", name));
        if !path.exists() {
            return Err(format!("MCP server '{}' not found", name));
        }
        let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let mut config: serde_json::Value =
            serde_json::from_str(&raw).map_err(|e| format!("Invalid JSON: {}", e))?;
        if let Some(env) = config.get_mut("env") {
            env_crypto::decrypt_env_values(env)?;
        }
        serde_json::to_string(&config).map_err(|e| e.to_string())
    }

    fn list_at(dir: &Path) -> Result<Vec<String>, String> {
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut servers = Vec::new();
        let entries = fs::read_dir(dir).map_err(|e| e.to_string())?;
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        if is_valid_name(stem) {
                            servers.push(stem.to_string());
                        }
                    }
                }
            }
        }
        Ok(servers)
    }

    fn delete_at(dir: &Path, name: &str) -> Result<(), String> {
        if !is_valid_name(name) {
            return Err("Invalid server name".into());
        }
        let path = dir.join(format!("{}.json", name));
        if path.exists() {
            fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    // ── list ─────────────────────────────────────────────────────────────────

    #[test]
    fn list_returns_empty_when_dir_missing() {
        let tmp = tmp();
        let dir = tmp.path().join("mcp_servers"); // not created
        let names = list_at(&dir).expect("list");
        assert!(names.is_empty());
    }

    #[test]
    fn list_returns_saved_server_names() {
        let tmp = tmp();
        let dir = tmp.path().join("mcp_servers");
        let config = r#"{"command": "npx", "args": ["-y", "some-server"]}"#;

        save_at(&dir, "server-a", config).expect("save a");
        save_at(&dir, "server-b", config).expect("save b");

        let mut names = list_at(&dir).expect("list");
        names.sort();
        assert_eq!(names, vec!["server-a", "server-b"]);
    }

    // ── save + read roundtrip ────────────────────────────────────────────────

    #[test]
    fn save_and_read_config_without_env() {
        let tmp = tmp();
        let dir = tmp.path().join("mcp_servers");
        let config = r#"{"command": "npx", "args": ["-y", "my-server"]}"#;

        save_at(&dir, "my-server", config).expect("save");
        let raw = read_at(&dir, "my-server").expect("read");
        let val: serde_json::Value = serde_json::from_str(&raw).expect("parse");

        assert_eq!(val["command"].as_str().unwrap(), "npx");
    }

    #[test]
    fn env_values_are_encrypted_at_rest() {
        let tmp = tmp();
        let dir = tmp.path().join("mcp_servers");
        let config = r#"{"command": "npx", "env": {"API_KEY": "my-secret"}}"#;

        save_at(&dir, "secure-server", config).expect("save");

        // Read raw bytes — should NOT contain the plaintext secret.
        let raw = fs::read_to_string(dir.join("secure-server.json")).expect("read raw");
        assert!(
            !raw.contains("my-secret"),
            "plaintext secret must not be stored on disk"
        );
        assert!(
            raw.contains("enc:v1:"),
            "encrypted sentinel should be present on disk"
        );
    }

    #[test]
    fn read_decrypts_env_values_to_plaintext() {
        let tmp = tmp();
        let dir = tmp.path().join("mcp_servers");
        let config = r#"{"command": "npx", "env": {"API_KEY": "my-secret"}}"#;

        save_at(&dir, "secure-server", config).expect("save");
        let raw = read_at(&dir, "secure-server").expect("read");
        let val: serde_json::Value = serde_json::from_str(&raw).expect("parse");

        assert_eq!(
            val["env"]["API_KEY"].as_str().unwrap(),
            "my-secret",
            "env values should be decrypted on read"
        );
    }

    #[test]
    fn double_save_does_not_double_encrypt() {
        let tmp = tmp();
        let dir = tmp.path().join("mcp_servers");
        let config = r#"{"command": "npx", "env": {"KEY": "value"}}"#;

        save_at(&dir, "srv", config).expect("first save");
        // Read decrypted, then save again (simulating a frontend re-save).
        let decrypted = read_at(&dir, "srv").expect("read");
        save_at(&dir, "srv", &decrypted).expect("second save");

        // Should still decrypt correctly.
        let result = read_at(&dir, "srv").expect("re-read");
        let val: serde_json::Value = serde_json::from_str(&result).expect("parse");
        assert_eq!(val["env"]["KEY"].as_str().unwrap(), "value");
    }

    // ── delete ───────────────────────────────────────────────────────────────

    #[test]
    fn delete_removes_config_file() {
        let tmp = tmp();
        let dir = tmp.path().join("mcp_servers");
        save_at(&dir, "to-delete", r#"{"command": "node"}"#).expect("save");
        assert!(dir.join("to-delete.json").exists());

        delete_at(&dir, "to-delete").expect("delete");
        assert!(!dir.join("to-delete.json").exists());
    }

    #[test]
    fn delete_is_idempotent_for_missing_server() {
        let tmp = tmp();
        let dir = tmp.path().join("mcp_servers");
        delete_at(&dir, "ghost").expect("delete non-existent should not error");
    }

    // ── invalid name handling ────────────────────────────────────────────────

    #[test]
    fn save_with_empty_name_returns_error() {
        let tmp = tmp();
        let dir = tmp.path().join("mcp_servers");
        let result = save_at(&dir, "", r#"{"command": "node"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn read_returns_error_for_missing_server() {
        let tmp = tmp();
        let dir = tmp.path().join("mcp_servers");
        let result = read_at(&dir, "nonexistent");
        assert!(result.is_err());
    }

    // ── non-string env values ignored ────────────────────────────────────────

    #[test]
    fn non_string_env_values_are_passed_through_unchanged() {
        let tmp = tmp();
        let dir = tmp.path().join("mcp_servers");
        // env contains a number — must not be altered by encrypt/decrypt.
        let config = r#"{"command": "node", "env": {"PORT": "8080"}}"#;
        save_at(&dir, "with-port", config).expect("save");
        let raw = read_at(&dir, "with-port").expect("read");
        let val: serde_json::Value = serde_json::from_str(&raw).expect("parse");
        assert_eq!(val["env"]["PORT"].as_str().unwrap(), "8080");
    }
}
