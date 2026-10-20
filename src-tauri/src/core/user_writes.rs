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

/// A config with each env value blanked, keeping the keys, and without the
/// `_author` display field a read adds.
fn without_env_values(mut config: serde_json::Value) -> serde_json::Value {
    if let Some(object) = config.as_object_mut() {
        object.remove("_author");
        if let Some(env) = object.get_mut("env").and_then(|e| e.as_object_mut()) {
            for value in env.values_mut() {
                *value = serde_json::Value::String(String::new());
            }
        }
    }
    config
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
