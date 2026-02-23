use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

// ── Path Helpers ─────────────────────────────────────────────────────────────

pub fn get_skills_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".claude/skills"))
}

pub fn get_projects_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".nexus/projects"))
}

pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && !name.contains('\\') && name != "." && name != ".."
}

// ── Data Structures ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Project {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub directory: String,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub mcp_servers: Vec<String>,
    #[serde(default)]
    pub providers: Vec<String>,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

// ── API Keys ─────────────────────────────────────────────────────────────────

pub fn save_api_key(provider: &str, key: &str) -> Result<(), String> {
    let entry = Entry::new("nexus_desktop", provider).map_err(|e| e.to_string())?;
    entry.set_password(key).map_err(|e| e.to_string())
}

pub fn get_api_key(provider: &str) -> Result<String, String> {
    let entry = Entry::new("nexus_desktop", provider).map_err(|e| e.to_string())?;
    entry.get_password().map_err(|e| e.to_string())
}

// ── Skills ───────────────────────────────────────────────────────────────────

pub fn list_skills() -> Result<Vec<String>, String> {
    let skills_dir = get_skills_dir()?;

    if !skills_dir.exists() {
        return Ok(Vec::new());
    }

    let mut skills = Vec::new();
    let entries = fs::read_dir(skills_dir).map_err(|e| e.to_string())?;

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if is_valid_name(name) {
                        skills.push(name.to_string());
                    }
                }
            }
        }
    }

    Ok(skills)
}

pub fn read_skill(name: &str) -> Result<String, String> {
    if !is_valid_name(name) {
        return Err("Invalid skill name".into());
    }
    let skills_dir = get_skills_dir()?;
    let skill_path = skills_dir.join(name).join("SKILL.md");

    if skill_path.exists() {
        fs::read_to_string(skill_path).map_err(|e| e.to_string())
    } else {
        Ok("".to_string())
    }
}

pub fn save_skill(name: &str, content: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid skill name".into());
    }
    let skills_dir = get_skills_dir()?;
    let skill_dir = skills_dir.join(name);

    if !skill_dir.exists() {
        fs::create_dir_all(&skill_dir).map_err(|e| e.to_string())?;
    }

    let skill_path = skill_dir.join("SKILL.md");
    fs::write(skill_path, content).map_err(|e| e.to_string())
}

pub fn delete_skill(name: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid skill name".into());
    }
    let skills_dir = get_skills_dir()?;
    let skill_dir = skills_dir.join(name);

    if skill_dir.exists() {
        fs::remove_dir_all(&skill_dir).map_err(|e| e.to_string())?;
    }

    Ok(())
}

// ── MCP Servers ──────────────────────────────────────────────────────────────

pub fn get_mcp_servers_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".nexus/mcp_servers"))
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

pub fn read_mcp_server_config(name: &str) -> Result<String, String> {
    if !is_valid_name(name) {
        return Err("Invalid server name".into());
    }
    let dir = get_mcp_servers_dir()?;
    let path = dir.join(format!("{}.json", name));

    if path.exists() {
        fs::read_to_string(path).map_err(|e| e.to_string())
    } else {
        Err(format!("MCP server '{}' not found", name))
    }
}

pub fn save_mcp_server_config(name: &str, data: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid server name".into());
    }

    // Validate that data is valid JSON
    serde_json::from_str::<serde_json::Value>(data).map_err(|e| format!("Invalid JSON: {}", e))?;

    let dir = get_mcp_servers_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    let path = dir.join(format!("{}.json", name));
    fs::write(path, data).map_err(|e| e.to_string())
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

/// Import MCP servers from Claude Desktop config into the Nexus registry.
/// Returns the list of server names that were imported.
pub fn import_mcp_servers_from_claude() -> Result<Vec<String>, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    let config_path = home.join("Library/Application Support/Claude/claude_desktop_config.json");

    if !config_path.exists() {
        return Err("Claude Desktop config not found".into());
    }

    let raw = fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
    let parsed: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid JSON: {}", e))?;

    let servers = parsed
        .get("mcpServers")
        .and_then(|v| v.as_object())
        .ok_or("No mcpServers found in Claude Desktop config")?;

    let mut imported = Vec::new();

    for (name, config) in servers {
        if !is_valid_name(name) {
            continue;
        }
        let data =
            serde_json::to_string_pretty(config).map_err(|e| format!("JSON error: {}", e))?;
        save_mcp_server_config(name, &data)?;
        imported.push(name.clone());
    }

    Ok(imported)
}

/// Read raw Claude Desktop config (kept for backward compatibility).
pub fn list_mcp_servers() -> Result<String, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    let config_path = home.join("Library/Application Support/Claude/claude_desktop_config.json");

    if config_path.exists() {
        fs::read_to_string(config_path).map_err(|e| e.to_string())
    } else {
        Ok("{}".to_string())
    }
}

// ── Plugins ──────────────────────────────────────────────────────────────────

pub fn get_plugins_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".nexus/plugins"))
}

pub fn get_sessions_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".nexus/sessions.json"))
}

/// The name used in marketplace.json and for `claude plugin` commands.
const MARKETPLACE_NAME: &str = "nexus-plugins";

/// Current plugin version — bump when plugin content changes so Claude Code
/// picks up updates via its cache.
const PLUGIN_VERSION: &str = "0.1.0";

// ── Plugin file contents ────────────────────────────────────────────────────

const HOOKS_JSON: &str = r#"
{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "${CLAUDE_PLUGIN_ROOT}/scripts/register-session.sh"
          }
        ]
      }
    ],
    "SessionEnd": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "${CLAUDE_PLUGIN_ROOT}/scripts/deregister-session.sh"
          }
        ]
      }
    ]
  }
}
"#;

const REGISTER_SESSION_SH: &str = r#"#!/usr/bin/env bash
# register-session.sh — Called by the SessionStart hook.
# Reads hook JSON from stdin, writes an entry to ~/.nexus/sessions.json.
set -euo pipefail

SESSIONS_FILE="$HOME/.nexus/sessions.json"

# Read the full hook input from stdin
INPUT=$(cat)

SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')
CWD=$(echo "$INPUT"        | jq -r '.cwd // empty')
MODEL=$(echo "$INPUT"       | jq -r '.model // "unknown"')
SOURCE=$(echo "$INPUT"      | jq -r '.source // "unknown"')

# Nothing to do without a session id
if [ -z "$SESSION_ID" ]; then
  exit 0
fi

# Portable UTC timestamp
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

# Ensure the store file exists
if [ ! -f "$SESSIONS_FILE" ]; then
  mkdir -p "$(dirname "$SESSIONS_FILE")"
  echo '{}' > "$SESSIONS_FILE"
fi

# Add / update this session (atomic via temp file)
TMPFILE=$(mktemp)
jq --arg id "$SESSION_ID" \
   --arg cwd "$CWD" \
   --arg model "$MODEL" \
   --arg source "$SOURCE" \
   --arg ts "$TIMESTAMP" \
   '.[$id] = {
      "session_id": $id,
      "cwd":        $cwd,
      "model":      $model,
      "source":     $source,
      "started_at": $ts,
      "last_seen":  $ts
    }' \
   "$SESSIONS_FILE" > "$TMPFILE" && mv "$TMPFILE" "$SESSIONS_FILE"

# Prune stale sessions (started > 24 h ago).
# macOS uses -v, GNU date uses -d.  Skip cleanup if neither works.
CUTOFF=$(date -u -v-24H +"%Y-%m-%dT%H:%M:%SZ" 2>/dev/null \
      || date -u -d '24 hours ago' +"%Y-%m-%dT%H:%M:%SZ" 2>/dev/null \
      || echo "")

if [ -n "$CUTOFF" ]; then
  TMPFILE=$(mktemp)
  jq --arg cutoff "$CUTOFF" \
     'with_entries(select(.value.started_at >= $cutoff))' \
     "$SESSIONS_FILE" > "$TMPFILE" && mv "$TMPFILE" "$SESSIONS_FILE"
fi

exit 0
"#;

const DEREGISTER_SESSION_SH: &str = r#"#!/usr/bin/env bash
# deregister-session.sh — Called by the SessionEnd hook.
# Removes the session entry from ~/.nexus/sessions.json.
set -euo pipefail

SESSIONS_FILE="$HOME/.nexus/sessions.json"

INPUT=$(cat)
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')

if [ -z "$SESSION_ID" ] || [ ! -f "$SESSIONS_FILE" ]; then
  exit 0
fi

TMPFILE=$(mktemp)
jq --arg id "$SESSION_ID" 'del(.[$id])' \
   "$SESSIONS_FILE" > "$TMPFILE" && mv "$TMPFILE" "$SESSIONS_FILE"

exit 0
"#;

// ── Sessions reader ─────────────────────────────────────────────────────────

/// Read active sessions from the store file.  Returns the raw JSON string
/// (an object keyed by session_id).  Returns "{}" if the file doesn't exist.
pub fn list_sessions() -> Result<String, String> {
    let path = get_sessions_path()?;
    if path.exists() {
        fs::read_to_string(&path).map_err(|e| e.to_string())
    } else {
        Ok("{}".into())
    }
}

// ── Plugin writer ───────────────────────────────────────────────────────────

/// Helper: create a directory if it doesn't exist.
fn ensure_dir(path: &std::path::Path) -> Result<(), String> {
    if !path.exists() {
        fs::create_dir_all(path)
            .map_err(|e| format!("Failed to create {}: {}", path.display(), e))?;
    }
    Ok(())
}

/// Helper: write a file and return its path string.
fn write_file(path: &std::path::Path, content: &str) -> Result<(), String> {
    fs::write(path, content).map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}

/// Helper: make a file executable (Unix only).
#[cfg(unix)]
fn make_executable(path: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)
        .map_err(|e| format!("Failed to read metadata for {}: {}", path.display(), e))?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)
        .map_err(|e| format!("Failed to chmod {}: {}", path.display(), e))
}

#[cfg(not(unix))]
fn make_executable(_path: &std::path::Path) -> Result<(), String> {
    Ok(()) // no-op on Windows
}

/// Resolve the Nexus binary path.  Uses the current executable when available
/// (gives an absolute path that survives being called from any directory),
/// otherwise falls back to "nexus" on PATH.
fn find_nexus_binary() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "nexus".to_string())
}

/// Write the full Nexus plugin to disk.
fn write_nexus_plugin(plugin_dir: &std::path::Path) -> Result<(), String> {
    // .claude-plugin/plugin.json
    let manifest_dir = plugin_dir.join(".claude-plugin");
    ensure_dir(&manifest_dir)?;

    let plugin_json = serde_json::json!({
        "name": "nexus",
        "description": "Nexus desktop app integration — session tracking and MCP tools",
        "version": PLUGIN_VERSION
    });
    write_file(
        &manifest_dir.join("plugin.json"),
        &serde_json::to_string_pretty(&plugin_json).map_err(|e| format!("JSON error: {}", e))?,
    )?;

    // .mcp.json — makes the Nexus MCP server available in every session
    let nexus_binary = find_nexus_binary();
    let mcp_json = serde_json::json!({
        "mcpServers": {
            "nexus": {
                "command": nexus_binary,
                "args": ["mcp-serve"]
            }
        }
    });
    write_file(
        &plugin_dir.join(".mcp.json"),
        &serde_json::to_string_pretty(&mcp_json).map_err(|e| format!("JSON error: {}", e))?,
    )?;

    // hooks/hooks.json
    let hooks_dir = plugin_dir.join("hooks");
    ensure_dir(&hooks_dir)?;
    write_file(&hooks_dir.join("hooks.json"), HOOKS_JSON)?;

    // scripts/
    let scripts_dir = plugin_dir.join("scripts");
    ensure_dir(&scripts_dir)?;

    let register_path = scripts_dir.join("register-session.sh");
    write_file(&register_path, REGISTER_SESSION_SH)?;
    make_executable(&register_path)?;

    let deregister_path = scripts_dir.join("deregister-session.sh");
    write_file(&deregister_path, DEREGISTER_SESSION_SH)?;
    make_executable(&deregister_path)?;

    Ok(())
}

/// Ensure the local plugin marketplace directory exists with a valid
/// marketplace.json and the full Nexus plugin.
///
/// Layout:
///   ~/.nexus/plugins/
///   ├── .claude-plugin/
///   │   └── marketplace.json
///   └── nexus/
///       ├── .claude-plugin/
///       │   └── plugin.json
///       ├── .mcp.json
///       ├── hooks/
///       │   └── hooks.json
///       └── scripts/
///           ├── register-session.sh
///           └── deregister-session.sh
pub fn ensure_plugin_marketplace() -> Result<PathBuf, String> {
    let plugins_dir = get_plugins_dir()?;

    // ── marketplace manifest ────────────────────────────────────────────
    let manifest_dir = plugins_dir.join(".claude-plugin");
    ensure_dir(&manifest_dir)?;

    let marketplace_json = serde_json::json!({
        "name": MARKETPLACE_NAME,
        "owner": { "name": "Nexus" },
        "metadata": {
            "description": "Plugins bundled with the Nexus desktop app"
        },
        "plugins": [
            {
                "name": "nexus",
                "source": "./nexus",
                "description": "Nexus desktop app integration — session tracking via hooks",
                "version": PLUGIN_VERSION
            }
        ]
    });
    write_file(
        &manifest_dir.join("marketplace.json"),
        &serde_json::to_string_pretty(&marketplace_json)
            .map_err(|e| format!("JSON error: {}", e))?,
    )?;

    // ── nexus plugin ────────────────────────────────────────────────────
    write_nexus_plugin(&plugins_dir.join("nexus"))?;

    Ok(plugins_dir)
}

/// Shell out to the `claude` CLI to register the local marketplace and
/// install plugins.  Skips silently if `claude` is not on PATH (the user
/// may not have Claude Code installed yet).
pub fn install_plugin_marketplace() -> Result<String, String> {
    let plugins_dir = ensure_plugin_marketplace()?;
    let plugins_path = plugins_dir
        .to_str()
        .ok_or("Plugin path contains invalid UTF-8")?;

    // Check if claude CLI is available
    let claude_check = std::process::Command::new("claude")
        .arg("--version")
        .output();

    if claude_check.is_err() {
        return Ok("claude CLI not found — skipping plugin marketplace install".into());
    }

    // Register the marketplace (idempotent — re-adding updates it)
    let add_result = std::process::Command::new("claude")
        .args(["plugin", "marketplace", "add", plugins_path])
        .output()
        .map_err(|e| format!("Failed to run claude plugin marketplace add: {}", e))?;

    if !add_result.status.success() {
        let stderr = String::from_utf8_lossy(&add_result.stderr);
        // "already added" is fine — treat as success
        if !stderr.contains("already") {
            return Err(format!("claude plugin marketplace add failed: {}", stderr));
        }
    }

    // Install the nexus plugin (idempotent — reinstall is a no-op)
    let install_result = std::process::Command::new("claude")
        .args(["plugin", "install", &format!("nexus@{}", MARKETPLACE_NAME)])
        .output()
        .map_err(|e| format!("Failed to run claude plugin install: {}", e))?;

    if !install_result.status.success() {
        let stderr = String::from_utf8_lossy(&install_result.stderr);
        if !stderr.contains("already installed") {
            return Err(format!("claude plugin install failed: {}", stderr));
        }
    }

    Ok("Plugin marketplace registered and plugins installed".into())
}

// ── Projects ─────────────────────────────────────────────────────────────────

pub fn list_projects() -> Result<Vec<String>, String> {
    let projects_dir = get_projects_dir()?;

    if !projects_dir.exists() {
        return Ok(Vec::new());
    }

    let mut projects = Vec::new();
    let entries = fs::read_dir(&projects_dir).map_err(|e| e.to_string())?;

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if is_valid_name(stem) {
                        projects.push(stem.to_string());
                    }
                }
            }
        }
    }

    Ok(projects)
}

pub fn read_project(name: &str) -> Result<String, String> {
    if !is_valid_name(name) {
        return Err("Invalid project name".into());
    }
    let projects_dir = get_projects_dir()?;
    let project_path = projects_dir.join(format!("{}.json", name));

    if project_path.exists() {
        let raw = fs::read_to_string(&project_path).map_err(|e| e.to_string())?;
        let project = match serde_json::from_str::<Project>(&raw) {
            Ok(p) => p,
            Err(_) => Project {
                name: name.to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
                updated_at: chrono::Utc::now().to_rfc3339(),
                ..Default::default()
            },
        };

        let fixed = serde_json::to_string(&project).map_err(|e| e.to_string())?;
        if raw != fixed {
            let _ = fs::write(&project_path, &fixed);
        }
        Ok(fixed)
    } else {
        Err(format!("Project '{}' not found", name))
    }
}

pub fn save_project(name: &str, data: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid project name".into());
    }

    // Validate that data is valid JSON matching the Project structure
    serde_json::from_str::<Project>(data).map_err(|e| format!("Invalid project data: {}", e))?;

    let projects_dir = get_projects_dir()?;
    if !projects_dir.exists() {
        fs::create_dir_all(&projects_dir).map_err(|e| e.to_string())?;
    }

    let project_path = projects_dir.join(format!("{}.json", name));
    fs::write(project_path, data).map_err(|e| e.to_string())
}

pub fn delete_project(name: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid project name".into());
    }
    let projects_dir = get_projects_dir()?;
    let project_path = projects_dir.join(format!("{}.json", name));

    if project_path.exists() {
        fs::remove_file(&project_path).map_err(|e| e.to_string())?;
    }

    Ok(())
}
