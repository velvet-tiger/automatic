//! The user's own saves, imports and deletes of library resources.
//!
//! Each one runs through [`super::remote_sources::as_user_write`], so a
//! remote source no longer owns a resource once the user has written or
//! deleted it, and a later update or removal of the source leaves it alone.
//! A source's own install and a cloud pull write through the plain save
//! functions instead, as neither is the user's write.

use super::remote_sources::as_user_write;

/// Save a skill the user wrote or imported.
pub fn save_skill(name: &str, content: &str) -> Result<(), String> {
    as_user_write("skill", name, || super::skills::save_skill(name, content))
}

/// Save a rule the user wrote.
pub fn save_rule(machine_name: &str, name: &str, content: &str) -> Result<(), String> {
    as_user_write("rule", machine_name, || {
        super::rules::save_rule(machine_name, name, content)
    })
}

/// Save a project template the user wrote.
pub fn save_template(name: &str, data: &str) -> Result<(), String> {
    as_user_write("template", name, || {
        super::templates::save_template(name, data)
    })
}

/// Save a sub-agent the user wrote.
pub fn save_subagent(machine_name: &str, name: &str, content: &str) -> Result<(), String> {
    as_user_write("agent", machine_name, || {
        super::subagents::save_subagent(machine_name, name, content)
    })
}

/// Save a command the user wrote.
pub fn save_user_command(machine_name: &str, content: &str) -> Result<(), String> {
    as_user_write("command", machine_name, || {
        super::commands::save_user_command(machine_name, content)
    })
}

/// Save an MCP server config the user wrote.
///
/// A save that changes only env values keeps the source's ownership. The
/// user fills in API keys for a server a source installed, and that is not
/// taking the server over: an update keeps those values, see
/// `remote_sources::install_mcp_server`. Any other change makes the server
/// the user's.
pub fn save_mcp_server(name: &str, data: &str) -> Result<(), String> {
    if changes_only_env_values(name, data)? {
        return super::mcp_servers::save_user_mcp_server_config(name, data);
    }
    as_user_write("mcp_server", name, || {
        super::mcp_servers::save_user_mcp_server_config(name, data)
    })
}

/// Whether `data` differs from the stored config of `name` only in the
/// values of env keys both have. A server that is not stored, or a config
/// that is not JSON, is more than an env change.
///
/// Both sides are first put in the form the GUI editor saves
/// (`normalizeConfig` then `cleanConfig` in `McpServers.tsx`), so a save
/// from the editor that only fills in a key compares equal. See
/// [`as_the_editor_saves`].
fn changes_only_env_values(name: &str, data: &str) -> Result<bool, String> {
    let Some(stored) = super::mcp_servers::read_mcp_server_config_if_present(name)? else {
        return Ok(false);
    };
    let (Ok(stored), Ok(incoming)) = (
        serde_json::from_str::<serde_json::Value>(&stored),
        serde_json::from_str::<serde_json::Value>(data),
    ) else {
        return Ok(false);
    };
    Ok(without_env_values(stored) == without_env_values(incoming))
}

/// A config with each env value blanked, keeping the keys, in the form
/// [`as_the_editor_saves`] gives it.
fn without_env_values(config: serde_json::Value) -> serde_json::Value {
    let mut config = as_the_editor_saves(config);
    if let Some(env) = config.get_mut("env").and_then(|e| e.as_object_mut()) {
        for value in env.values_mut() {
            *value = serde_json::Value::String(String::new());
        }
    }
    config
}

/// `config` with the differences the GUI editor introduces removed, where
/// none of them changes how the server runs:
///
/// - A missing `type` is `stdio`, whatever other fields the config has, as
///   every agent writer reads it. The editor's own `normalizeConfig` makes
///   a url-only config `http`, so that edit counts as a change: it changes
///   how agents run the server.
/// - Every field whose name starts with `_` (Automatic's `_author`,
///   `_builtin`, `_discovered` markers) is dropped.
/// - An empty `command`, `args`, `env`, `cwd`, `url` or `headers` is dropped,
///   as is a null value. The editor omits them, and sync treats a missing
///   field the same way.
/// - `oauth` keeps only its non-empty members, and is dropped when none is
///   left, as `cleanConfig` does.
/// - `enabled: true` is dropped, as it is the default; `enabled: false`
///   stays. A `timeout` that is not a positive number is dropped.
///
/// Fields `cleanConfig` also drops are kept here, so a save that removes
/// them counts as a change and the user takes the server over: any field the
/// editor does not know, and the fields of the other transport (`url`,
/// `headers` and `oauth` on a stdio server; `command`, `args`, `env` and
/// `cwd` on an http one). Agents can read those, so dropping one could
/// change how the server runs.
fn as_the_editor_saves(mut config: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    let Some(object) = config.as_object_mut() else {
        return config;
    };
    object.retain(|key, value| !key.starts_with('_') && !value.is_null());
    if !object.contains_key("type") {
        object.insert("type".to_string(), Value::String("stdio".to_string()));
    }
    if let Some(oauth) = object.get_mut("oauth").and_then(|o| o.as_object_mut()) {
        oauth.retain(|_, v| !is_empty_value(v));
    }
    for key in ["command", "args", "env", "cwd", "url", "headers", "oauth"] {
        if object.get(key).is_some_and(is_empty_value) {
            object.remove(key);
        }
    }
    if object.get("enabled") == Some(&Value::Bool(true)) {
        object.remove("enabled");
    }
    if object
        .get("timeout")
        .is_some_and(|t| !t.as_f64().is_some_and(|t| t > 0.0))
    {
        object.remove("timeout");
    }
    config
}

fn is_empty_value(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => true,
        serde_json::Value::String(s) => s.is_empty(),
        serde_json::Value::Array(a) => a.is_empty(),
        serde_json::Value::Object(o) => o.is_empty(),
        _ => false,
    }
}

/// Delete a skill at the user's request.
pub fn delete_skill(name: &str) -> Result<(), String> {
    as_user_write("skill", name, || super::skills::delete_skill(name))
}

/// Delete a rule at the user's request.
pub fn delete_rule(machine_name: &str) -> Result<(), String> {
    as_user_write("rule", machine_name, || {
        super::rules::delete_rule(machine_name)
    })
}

/// Delete a project template at the user's request.
pub fn delete_template(name: &str) -> Result<(), String> {
    as_user_write("template", name, || super::templates::delete_template(name))
}

/// Delete a sub-agent at the user's request.
pub fn delete_subagent(machine_name: &str) -> Result<(), String> {
    as_user_write("agent", machine_name, || {
        super::subagents::delete_subagent(machine_name)
    })
}

/// Delete a command at the user's request.
pub fn delete_user_command(machine_name: &str) -> Result<(), String> {
    as_user_write("command", machine_name, || {
        super::commands::delete_user_command(machine_name)
    })
}

/// Delete an MCP server at the user's request.
pub fn delete_mcp_server(name: &str) -> Result<(), String> {
    as_user_write("mcp_server", name, || {
        super::mcp_servers::delete_mcp_server_config(name)
    })
}
