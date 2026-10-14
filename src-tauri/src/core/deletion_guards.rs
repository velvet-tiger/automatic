//! The library assets Automatic refuses to delete.
//!
//! The delete commands and a remote source's removal both ask here, so they
//! refuse the same names.

use std::fs;

/// A kind of library asset that can be deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryAssetKind {
    Skill,
    McpServer,
    Rule,
    Template,
    Command,
    Subagent,
}

/// Refuse to delete an asset that Automatic or a plugin provides. The error
/// says why.
///
/// - A built-in or bundled skill, or one a plugin provides. A bundled skill
///   is reinstalled on the next launch, so its delete would look ignored.
/// - The built-in MCP server.
/// - A mandatory rule, or one a plugin provides.
/// - A Codex OpenAI sub-agent.
pub fn ensure_deletable(kind: LibraryAssetKind, name: &str) -> Result<(), String> {
    match kind {
        LibraryAssetKind::Skill => {
            if super::mcp_servers::is_builtin_skill(name) {
                return Err(format!("Cannot delete built-in skill '{}'", name));
            }
            if super::skill_store::is_bundled_skill(name) {
                return Err(format!(
                    "Cannot delete bundled skill '{}' — it ships with Automatic and would be reinstalled on next launch",
                    name
                ));
            }
            if let Some(plugin_id) = super::app_plugins::plugin_id_for_skill(name) {
                return Err(format!(
                    "Cannot delete skill '{}' — it is provided by plugin '{}'",
                    name, plugin_id
                ));
            }
        }
        LibraryAssetKind::McpServer => {
            if super::mcp_servers::is_builtin_mcp_server(name) {
                return Err(format!("Cannot delete built-in MCP server '{}'", name));
            }
        }
        LibraryAssetKind::Rule => {
            if super::rules::is_mandatory_rule(name) {
                return Err(format!(
                    "Cannot delete rule '{}' — it is required by Automatic",
                    name
                ));
            }
            if is_plugin_rule(name)? {
                return Err(format!(
                    "Cannot delete rule '{}' — it is provided by a plugin",
                    name
                ));
            }
        }
        LibraryAssetKind::Subagent => {
            if name.starts_with("codex-") && name.ends_with("-openai") {
                return Err("Cannot delete Codex OpenAI agents.".into());
            }
        }
        LibraryAssetKind::Template | LibraryAssetKind::Command => {}
    }
    Ok(())
}

/// Whether the stored rule `name` names a plugin as its owner. A rule that
/// is missing, or does not parse, has no owner.
fn is_plugin_rule(name: &str) -> Result<bool, String> {
    let path = super::rules::get_rules_dir()?.join(format!("{}.json", name));
    let Ok(raw) = fs::read_to_string(&path) else {
        return Ok(false);
    };
    Ok(serde_json::from_str::<super::rules::Rule>(&raw)
        .map(|rule| rule.plugin_id.is_some())
        .unwrap_or(false))
}
