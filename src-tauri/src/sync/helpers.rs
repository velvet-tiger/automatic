use serde_json::{json, Map, Value};
use std::fs;
use std::path::PathBuf;

/// Load MCP server configs from the Automatic registry (~/.automatic/mcp_servers/).
pub(crate) fn load_mcp_server_configs() -> Result<Map<String, Value>, String> {
    let names = crate::core::list_mcp_server_configs()?;
    let mut servers = Map::new();

    for name in names {
        match crate::core::read_mcp_server_config(&name) {
            Ok(raw) => {
                if let Ok(config) = serde_json::from_str::<Value>(&raw) {
                    servers.insert(name, config);
                }
            }
            Err(_) => continue,
        }
    }

    Ok(servers)
}

/// Read all skill contents from the global registry for the given names.
pub(crate) fn load_skill_contents(skill_names: &[String]) -> Vec<(String, String)> {
    let mut contents = Vec::new();
    for name in skill_names {
        match crate::core::read_skill(name) {
            Ok(content) if !content.is_empty() => {
                contents.push((name.clone(), content));
            }
            _ => {}
        }
    }
    contents
}

/// Find the Automatic binary path.
pub(crate) fn find_automatic_binary() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "automatic".to_string())
}

/// Strip any legacy `<!-- automatic:skills:start -->…<!-- automatic:skills:end -->`
/// managed section from a project file.  Returns the path if the file was
/// modified, or None if no cleanup was needed.
pub(crate) fn clean_project_file(dir: &PathBuf, filename: &str) -> Result<Option<String>, String> {
    let path = dir.join(filename);
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;

    let start_marker = "<!-- automatic:skills:start -->";
    let end_marker = "<!-- automatic:skills:end -->";

    if let (Some(start), Some(end)) = (content.find(start_marker), content.find(end_marker)) {
        let before = &content[..start];
        let after = &content[end + end_marker.len()..];
        let cleaned = format!(
            "{}{}",
            before.trim_end(),
            if after.trim().is_empty() {
                "\n".to_string()
            } else {
                format!("\n\n{}", after.trim_start())
            }
        );
        fs::write(&path, cleaned).map_err(|e| e.to_string())?;
        Ok(Some(path.display().to_string()))
    } else {
        Ok(None)
    }
}

pub(crate) fn add_unique(items: &mut Vec<String>, value: &str) -> bool {
    if items.iter().any(|v| v == value) {
        false
    } else {
        items.push(value.to_string());
        true
    }
}

/// Build the selected MCP server map for a project, applying all
/// transformations that the sync engine uses (stripping internal `_`-prefixed
/// fields, substituting OAuth proxy configs for HTTP servers with stored
/// tokens).
///
/// Both `engine.rs` and `drift.rs` must use this function to ensure the
/// expected config matches what is actually written to disk.
pub(crate) fn build_selected_servers(
    project_name: &str,
    server_names: &[String],
    mcp_config: &Map<String, Value>,
) -> Map<String, Value> {
    let mut selected_servers = Map::new();
    let automatic_binary = find_automatic_binary();

    // Always include the Automatic MCP server
    selected_servers.insert(
        "automatic".to_string(),
        json!({
            "command": automatic_binary,
            "args": ["mcp-serve"],
            "env": {
                "AUTOMATIC_PROJECT": project_name
            }
        }),
    );

    // Add project-selected MCP servers from the Automatic registry.
    // Strip Automatic-internal fields (prefixed with `_`) before writing to agent files.
    //
    // For HTTP servers that have a stored OAuth token in the keychain, we emit
    // a local stdio proxy config instead of the remote URL.  This keeps the
    // token out of every project file — the proxy loads it from the keychain
    // at runtime.
    for server_name in server_names {
        if let Some(server_config) = mcp_config.get(server_name) {
            let cleaned = strip_internal_fields(server_config.clone());

            // Check if this is an HTTP server with a stored OAuth token.
            let is_http = cleaned
                .get("type")
                .and_then(|v| v.as_str())
                .map(|t| t == "http" || t == "sse")
                .unwrap_or(false);
            let has_token = crate::proxy::has_oauth_token(server_name);

            if is_http && has_token {
                // Emit a local proxy config instead of the remote URL.
                selected_servers.insert(
                    server_name.clone(),
                    json!({
                        "command": automatic_binary,
                        "args": ["mcp-proxy", server_name],
                    }),
                );
            } else {
                selected_servers.insert(server_name.clone(), cleaned);
            }
        }
    }

    selected_servers
}

/// Remove fields whose names start with `_` from a JSON object.
/// These are Automatic-internal metadata fields (e.g. `_author`) that should
/// never be written to agent configuration files.
pub(crate) fn strip_internal_fields(mut value: Value) -> Value {
    if let Value::Object(ref mut map) = value {
        map.retain(|key, _| !key.starts_with('_'));
    }
    value
}
