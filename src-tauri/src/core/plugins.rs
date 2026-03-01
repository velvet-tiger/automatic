use std::fs;
use std::path::PathBuf;

use super::paths::get_automatic_dir;

// ── Plugins ──────────────────────────────────────────────────────────────────

pub fn get_plugins_dir() -> Result<PathBuf, String> {
    Ok(get_automatic_dir()?.join("plugins"))
}

pub fn get_sessions_path() -> Result<PathBuf, String> {
    Ok(get_automatic_dir()?.join("sessions.json"))
}

/// The name used in marketplace.json and for `claude plugin` commands.
const MARKETPLACE_NAME: &str = "automatic-plugins";

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
# Reads hook JSON from stdin, writes an entry to the Automatic sessions file.
# Uses .automatic-dev in debug builds (detected via AUTOMATIC_DEV env var),
# otherwise uses .automatic.
set -euo pipefail

if [ "${AUTOMATIC_DEV:-0}" = "1" ]; then
  SESSIONS_FILE="$HOME/.automatic-dev/sessions.json"
else
  SESSIONS_FILE="$HOME/.automatic/sessions.json"
fi

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
# Removes the session entry from the Automatic sessions file.
# Uses .automatic-dev in debug builds (detected via AUTOMATIC_DEV env var),
# otherwise uses .automatic.
set -euo pipefail

if [ "${AUTOMATIC_DEV:-0}" = "1" ]; then
  SESSIONS_FILE="$HOME/.automatic-dev/sessions.json"
else
  SESSIONS_FILE="$HOME/.automatic/sessions.json"
fi

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

/// Resolve the Automatic binary path.  Uses the current executable when available
/// (gives an absolute path that survives being called from any directory),
/// otherwise falls back to "automatic" on PATH.
fn find_automatic_binary() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "automatic".to_string())
}

/// Write the full Automatic plugin to disk.
fn write_automatic_plugin(plugin_dir: &std::path::Path) -> Result<(), String> {
    // .claude-plugin/plugin.json
    let manifest_dir = plugin_dir.join(".claude-plugin");
    ensure_dir(&manifest_dir)?;

    let plugin_json = serde_json::json!({
        "name": "automatic",
        "description": "Automatic desktop app integration — session tracking and MCP tools",
        "version": PLUGIN_VERSION
    });
    write_file(
        &manifest_dir.join("plugin.json"),
        &serde_json::to_string_pretty(&plugin_json).map_err(|e| format!("JSON error: {}", e))?,
    )?;

    // .mcp.json — makes the Automatic MCP server available in every session
    let automatic_binary = find_automatic_binary();
    let mcp_json = serde_json::json!({
        "mcpServers": {
            "automatic": {
                "command": automatic_binary,
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
/// marketplace.json and the full Automatic plugin.
///
/// Layout:
///   ~/.automatic[-dev]/plugins/
///   ├── .claude-plugin/
///   │   └── marketplace.json
///   └── automatic/
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
        "owner": { "name": "Automatic" },
        "metadata": {
            "description": "Plugins bundled with the Automatic desktop app"
        },
        "plugins": [
            {
                "name": "automatic",
                "source": "./automatic",
                "description": "Automatic desktop app integration — session tracking via hooks",
                "version": PLUGIN_VERSION
            }
        ]
    });
    write_file(
        &manifest_dir.join("marketplace.json"),
        &serde_json::to_string_pretty(&marketplace_json)
            .map_err(|e| format!("JSON error: {}", e))?,
    )?;

    // ── automatic plugin ─────────────────────────────────────────────────
    write_automatic_plugin(&plugins_dir.join("automatic"))?;

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

    // Install the automatic plugin (idempotent — reinstall is a no-op)
    let install_result = std::process::Command::new("claude")
        .args([
            "plugin",
            "install",
            &format!("automatic@{}", MARKETPLACE_NAME),
        ])
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
