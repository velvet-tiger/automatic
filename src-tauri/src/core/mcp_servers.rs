use std::fs;
use std::path::PathBuf;

use super::env_crypto;
use super::paths::{get_library_dir, is_valid_name};
use super::recently_added::{record_recently_added, remove_recently_added};

// ── MCP Servers ──────────────────────────────────────────────────────────────

pub fn get_mcp_servers_dir() -> Result<PathBuf, String> {
    Ok(get_library_dir()?.join("mcp_servers"))
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

    servers.sort();
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

    if config.get("_author").is_none() {
        if let Some(repo) = super::remote_sources::get_provenance("mcp_server", name)? {
            if let Some(obj) = config.as_object_mut() {
                obj.insert(
                    "_author".to_string(),
                    serde_json::json!({
                        "name": repo,
                        "repository_url": format!("https://github.com/{}", repo),
                    }),
                );
            }
        }
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
    let is_new = !path.exists();
    let serialized = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write(&path, serialized).map_err(|e| e.to_string())?;

    if is_new {
        record_recently_added("mcp_servers", name);
    }

    Ok(())
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

    remove_recently_added("mcp_servers", name);

    Ok(())
}

/// Reported availability of a configured MCP server, shown as a status
/// indicator on the project's MCP tab.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct McpServerAvailability {
    pub available: bool,
    pub message: Option<String>,
}

/// Check whether a configured MCP server currently looks available.
///
/// For `stdio` servers this resolves the configured command against an
/// absolute/relative path or the `PATH` environment variable — the same
/// resolution a shell does — without spawning the process. For `http` and
/// `sse` servers it sends a lightweight HTTP request to the configured URL;
/// any response, even an error status, counts as available, since the
/// question being answered is reachability, not authentication (OAuth
/// token validity already has its own indicator).
pub async fn check_mcp_server_status(name: &str) -> Result<McpServerAvailability, String> {
    let raw = read_mcp_server_config(name)?;
    let config: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid config for '{}': {}", name, e))?;

    let transport = config
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("stdio");

    match transport {
        "stdio" => Ok(check_stdio_command_available(
            config.get("command").and_then(|v| v.as_str()),
        )),
        "http" | "sse" => {
            check_http_endpoint_available(
                config.get("url").and_then(|v| v.as_str()),
                config.get("headers"),
            )
            .await
        }
        other => Ok(McpServerAvailability {
            available: false,
            message: Some(format!("Unknown transport '{}'", other)),
        }),
    }
}

fn check_stdio_command_available(command: Option<&str>) -> McpServerAvailability {
    let Some(command) = command.map(str::trim).filter(|c| !c.is_empty()) else {
        return McpServerAvailability {
            available: false,
            message: Some("No command configured.".to_string()),
        };
    };

    match resolve_command_path(command) {
        Some(path) => McpServerAvailability {
            available: true,
            message: Some(format!("Resolved to {}", path.display())),
        },
        None => McpServerAvailability {
            available: false,
            message: Some(format!(
                "Command '{}' was not found on disk or on PATH.",
                command
            )),
        },
    }
}

/// Resolves `command` the way a shell would: a path containing a separator
/// is checked directly, otherwise every directory on `PATH` is searched.
/// This only checks the file exists and is executable — it never spawns
/// the process, so it stays cheap enough to run on every tab load.
fn resolve_command_path(command: &str) -> Option<PathBuf> {
    let candidate = std::path::Path::new(command);
    if command.contains(std::path::MAIN_SEPARATOR) || candidate.is_absolute() {
        return is_executable_file(candidate).then(|| candidate.to_path_buf());
    }

    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let full = dir.join(command);
        if is_executable_file(&full) {
            return Some(full);
        }
        #[cfg(windows)]
        for ext in ["exe", "cmd", "bat"] {
            let with_ext = dir.join(format!("{command}.{ext}"));
            if is_executable_file(&with_ext) {
                return Some(with_ext);
            }
        }
    }
    None
}

#[cfg(unix)]
fn is_executable_file(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(path: &std::path::Path) -> bool {
    path.is_file()
}

async fn check_http_endpoint_available(
    url: Option<&str>,
    headers: Option<&serde_json::Value>,
) -> Result<McpServerAvailability, String> {
    let Some(url) = url.map(str::trim).filter(|u| !u.is_empty()) else {
        return Ok(McpServerAvailability {
            available: false,
            message: Some("No URL configured.".to_string()),
        });
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {}", e))?;

    let mut request = client.head(url);
    if let Some(header_map) = headers.and_then(|h| h.as_object()) {
        for (key, value) in header_map {
            if let Some(value_str) = value.as_str() {
                request = request.header(key.as_str(), value_str);
            }
        }
    }

    match request.send().await {
        Ok(response) => Ok(McpServerAvailability {
            available: true,
            message: Some(format!(
                "Responded with HTTP {}",
                response.status().as_u16()
            )),
        }),
        Err(e) => Ok(McpServerAvailability {
            available: false,
            message: Some(format!("Not reachable: {}", e)),
        }),
    }
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

/// Resolve the path of the Automatic binary that gets written into MCP
/// configs (the registry entry and every per-project agent config file).
///
/// `std::env::current_exe()` returns the *invocation* path on macOS: a
/// process spawned through the CLI symlink (`/usr/local/bin/automatic`) sees
/// the symlink itself, while the GUI process sees the app-bundle binary —
/// two different strings for the same file. Writing whichever string the
/// current process happened to be invoked by made the GUI and
/// symlink-spawned `mcp-serve` processes perpetually overwrite each other's
/// configs. Instead:
///
/// 1. Canonicalize `current_exe()` so the invocation alias is stripped.
/// 2. Prefer the stable CLI symlink when it resolves to this same binary —
///    it survives app moves/updates, and every process that resolves through
///    this function then emits the identical string.
pub fn automatic_binary_path() -> String {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return "automatic".to_string(),
    };
    let canonical = std::fs::canonicalize(&exe).unwrap_or(exe);

    #[cfg(unix)]
    {
        for candidate in unix_cli_symlink_candidates() {
            if std::fs::canonicalize(&candidate)
                .map(|c| c == canonical)
                .unwrap_or(false)
            {
                return candidate.display().to_string();
            }
        }
    }

    canonical.display().to_string()
}

/// The locations `cli_install` may have placed the `automatic` CLI symlink.
/// Kept in sync with `cli_install::unix::preferred_install_path`.
#[cfg(unix)]
fn unix_cli_symlink_candidates() -> Vec<std::path::PathBuf> {
    let mut candidates = vec![std::path::PathBuf::from("/usr/local/bin/automatic")];
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join(".local").join("bin").join("automatic"));
    }
    candidates
}

/// True when two binary path strings refer to the same file. Falls back to
/// literal comparison when canonicalisation fails (e.g. a stale path whose
/// target no longer exists — that is a genuine change).
fn same_automatic_binary(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => false,
    }
}

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
    let binary = automatic_binary_path();

    // ── 1. Read old registry entry to detect binary path change ──────────
    //
    // If the binary path has changed (e.g. dev→release or after an update),
    // every project that writes the automatic server entry needs a re-sync.
    // Compared via `same_automatic_binary`, not string equality: the GUI
    // process and a symlink-spawned `mcp-serve` see different `current_exe()`
    // strings for the *same* binary, and treating that as a change made every
    // spawn re-sync (and rewrite files in) every project.
    let binary_changed = read_mcp_server_config(AUTOMATIC_SERVER_NAME)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|v| {
            v.get("command")
                .and_then(|c| c.as_str())
                .map(|c| !same_automatic_binary(c, &binary))
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
    use crate::core::paths::with_test_home;
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

    /// Two invocation aliases of the same file (e.g. the CLI symlink vs the
    /// app-bundle binary) must compare equal; a dead path must not.
    #[cfg(unix)]
    #[test]
    fn same_automatic_binary_resolves_symlink_aliases() {
        let dir = tempfile::tempdir().expect("tempdir");
        let real = dir.path().join("real-binary");
        std::fs::write(&real, b"bin").expect("write real");
        let alias = dir.path().join("alias");
        std::os::unix::fs::symlink(&real, &alias).expect("symlink");

        assert!(
            same_automatic_binary(alias.to_str().unwrap(), real.to_str().unwrap()),
            "a symlink alias of the same binary must not count as a path change"
        );
        assert!(
            !same_automatic_binary(real.to_str().unwrap(), "/stale/path/automatic"),
            "a dead path is a genuine change"
        );
    }

    /// A registry entry holding a symlink *alias* of the currently running
    /// binary is not a path change — flagging it as one made every
    /// `mcp-serve` spawn re-sync (and rewrite files in) every project,
    /// because the GUI and symlink-spawned processes see different
    /// `current_exe()` strings for the same file.
    #[cfg(unix)]
    #[test]
    fn ensure_automatic_in_global_mcp_ignores_symlink_alias_of_current_binary() {
        let home = tempfile::tempdir().expect("tempdir");
        let project_dir = tempfile::tempdir().expect("project dir");
        let alias_dir = tempfile::tempdir().expect("alias dir");

        // Symlink alias pointing at the real current test binary.
        let exe = std::env::current_exe().expect("current exe");
        let alias = alias_dir.path().join("automatic");
        std::os::unix::fs::symlink(&exe, &alias).expect("symlink to current exe");

        with_test_home(home.path().to_path_buf(), || {
            let seeded = serde_json::json!({
                "type": "stdio",
                "command": alias.to_str().unwrap(),
                "args": ["mcp-serve"],
                "_builtin": true
            });
            save_mcp_server_config(AUTOMATIC_SERVER_NAME, &seeded.to_string())
                .expect("seed alias registry entry");

            // Project already fully assigned so `changed` stays false and the
            // only thing that could queue it is a (spurious) binary change.
            let project = super::super::Project {
                name: "demo".to_string(),
                directory: project_dir.path().to_string_lossy().to_string(),
                agents: vec!["claude".to_string()],
                mcp_servers: vec![AUTOMATIC_SERVER_NAME.to_string()],
                skills: vec![AUTOMATIC_SKILL_NAME.to_string()],
                ..Default::default()
            };
            let serialized = serde_json::to_string_pretty(&project).expect("serialize project");
            super::super::save_project("demo", &serialized).expect("save project");

            let projects_to_sync =
                ensure_automatic_in_global_mcp().expect("ensure automatic entry");
            assert!(
                projects_to_sync.is_empty(),
                "an alias of the current binary must not queue re-syncs, got {:?}",
                projects_to_sync
            );
        });
    }

    #[test]
    fn ensure_automatic_in_global_mcp_detects_stale_binary_path_and_flags_resync() {
        let home = tempfile::tempdir().expect("tempdir");
        let project_dir = tempfile::tempdir().expect("project dir");

        with_test_home(home.path().to_path_buf(), || {
            // Seed a registry entry whose command path can never match the
            // real test binary's `current_exe()`, simulating what's left
            // behind after the app moves or updates.
            save_mcp_server_config(
                AUTOMATIC_SERVER_NAME,
                r#"{"type":"stdio","command":"/stale/path/automatic","args":["mcp-serve"],"_builtin":true}"#,
            )
            .expect("seed stale registry entry");

            let project = super::super::Project {
                name: "demo".to_string(),
                directory: project_dir.path().to_string_lossy().to_string(),
                agents: vec!["claude".to_string()],
                mcp_servers: vec![AUTOMATIC_SERVER_NAME.to_string()],
                ..Default::default()
            };
            let serialized = serde_json::to_string_pretty(&project).expect("serialize project");
            super::super::save_project("demo", &serialized).expect("save project");

            let projects_to_sync =
                ensure_automatic_in_global_mcp().expect("ensure automatic entry");
            assert!(
                projects_to_sync.contains(&"demo".to_string()),
                "a project referencing a stale automatic binary path should be queued for re-sync, got {:?}",
                projects_to_sync
            );

            let updated_registry =
                read_mcp_server_config(AUTOMATIC_SERVER_NAME).expect("read updated registry entry");
            let value: serde_json::Value =
                serde_json::from_str(&updated_registry).expect("parse registry entry");
            assert_ne!(
                value["command"].as_str(),
                Some("/stale/path/automatic"),
                "registry entry should be repaired to the current binary path, not left stale"
            );
        });
    }

    // ── MCP server availability checks ──────────────────────────────────────

    #[test]
    fn resolve_command_path_finds_absolute_executable_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let script = tmp.path().join("fake-server");
        fs::write(&script, "#!/bin/sh\necho hi\n").expect("write script");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod");
        }

        let resolved = resolve_command_path(script.to_str().expect("utf8 path"));
        assert_eq!(resolved.as_deref(), Some(script.as_path()));
    }

    #[cfg(unix)]
    #[test]
    fn resolve_command_path_rejects_non_executable_absolute_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let script = tmp.path().join("not-executable");
        // fs::write creates the file without the executable bit set.
        fs::write(&script, "just data").expect("write file");

        assert!(resolve_command_path(script.to_str().expect("utf8 path")).is_none());
    }

    #[test]
    fn resolve_command_path_finds_bare_command_on_path() {
        // "sh" is present on PATH in every unix dev/CI environment this runs in.
        assert!(resolve_command_path("sh").is_some());
    }

    #[test]
    fn resolve_command_path_returns_none_for_unknown_bare_command() {
        assert!(resolve_command_path("definitely-not-a-real-command-xyz123").is_none());
    }

    #[test]
    fn check_stdio_command_available_reports_missing_command_as_unavailable() {
        let status = check_stdio_command_available(None);
        assert!(!status.available);
        assert!(status.message.is_some());
    }

    #[tokio::test]
    async fn check_http_endpoint_available_treats_any_response_as_available() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind local listener");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                use tokio::io::AsyncWriteExt;
                let _ = stream
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                    .await;
            }
        });

        let url = format!("http://{}/", addr);
        let status = check_http_endpoint_available(Some(&url), None)
            .await
            .expect("check status");
        assert!(status.available);
    }

    #[tokio::test]
    async fn check_http_endpoint_available_reports_unreachable_url_as_unavailable() {
        // Port 1 is a privileged port essentially never bound to in dev/CI
        // environments, so a connection attempt reliably fails.
        let status = check_http_endpoint_available(Some("http://127.0.0.1:1/"), None)
            .await
            .expect("check status");
        assert!(!status.available);
    }

    #[tokio::test]
    async fn check_http_endpoint_available_reports_missing_url() {
        let status = check_http_endpoint_available(None, None)
            .await
            .expect("check status");
        assert!(!status.available);
    }

    #[test]
    fn read_config_hydrates_remote_author_from_provenance() {
        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), || {
            save_mcp_server_config("remote-server", r#"{"type":"stdio","command":"npx"}"#)
                .expect("save config");
            super::super::remote_sources::record_provenance(
                "mcp_server",
                "remote-server",
                "octocat/remote-servers",
            )
            .expect("record provenance");

            let raw = read_mcp_server_config("remote-server").expect("read config");
            let value: serde_json::Value = serde_json::from_str(&raw).expect("parse config");

            assert_eq!(
                value["_author"]["repository_url"].as_str(),
                Some("https://github.com/octocat/remote-servers")
            );
        });
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
