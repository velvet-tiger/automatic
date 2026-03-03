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
