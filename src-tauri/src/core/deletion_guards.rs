//! The library assets Automatic refuses to delete.
//!
//! The delete commands, a remote source's removal and a remote source's
//! install all ask here, so they refuse the same names.

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

/// Why Automatic protects a library asset from being deleted or replaced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Protection {
    /// The built-in skill or MCP server.
    BuiltIn,
    /// A skill that ships with Automatic and is reinstalled on each launch.
    Bundled,
    /// A skill or rule a plugin provides. The plugin id is known for a skill.
    Plugin(Option<String>),
    /// A rule Automatic requires.
    Required,
    /// A Codex OpenAI sub-agent.
    Codex,
}

impl Protection {
    /// A short owner label, such as `built-in` or `plugin:build`.
    pub fn label(&self) -> String {
        match self {
            Protection::BuiltIn => "built-in".to_string(),
            Protection::Bundled => "bundled".to_string(),
            Protection::Plugin(Some(id)) => format!("plugin:{}", id),
            Protection::Plugin(None) => "plugin".to_string(),
            Protection::Required => "required".to_string(),
            Protection::Codex => "codex".to_string(),
        }
    }
}

/// How Automatic or a plugin protects the asset `name`, or `None` when it
/// is the user's to delete or replace. The name is compared exactly.
///
/// - A built-in or bundled skill, or one a plugin provides. A bundled skill
///   is reinstalled on the next launch, so its delete would look ignored.
/// - The built-in MCP server.
/// - A mandatory rule, or one a plugin provides.
/// - A Codex OpenAI sub-agent.
pub fn protection_of(kind: LibraryAssetKind, name: &str) -> Result<Option<Protection>, String> {
    let protection = match kind {
        LibraryAssetKind::Skill => {
            if super::mcp_servers::is_builtin_skill(name) {
                Some(Protection::BuiltIn)
            } else if super::skill_store::is_bundled_skill(name) {
                Some(Protection::Bundled)
            } else {
                super::app_plugins::plugin_id_for_skill(name)
                    .map(|plugin_id| Protection::Plugin(Some(plugin_id)))
            }
        }
        LibraryAssetKind::McpServer => {
            super::mcp_servers::is_builtin_mcp_server(name).then_some(Protection::BuiltIn)
        }
        LibraryAssetKind::Rule => {
            if super::rules::is_mandatory_rule(name) {
                Some(Protection::Required)
            } else if is_plugin_rule(name)? {
                Some(Protection::Plugin(None))
            } else {
                None
            }
        }
        LibraryAssetKind::Subagent => {
            (name.starts_with("codex-") && name.ends_with("-openai")).then_some(Protection::Codex)
        }
        LibraryAssetKind::Template | LibraryAssetKind::Command => None,
    };
    Ok(protection)
}

/// Refuse to delete an asset that Automatic or a plugin provides, as
/// [`protection_of`] lists. The error says why.
pub fn ensure_deletable(kind: LibraryAssetKind, name: &str) -> Result<(), String> {
    let Some(protection) = protection_of(kind, name)? else {
        return Ok(());
    };
    Err(match protection {
        Protection::BuiltIn if kind == LibraryAssetKind::Skill => {
            format!("Cannot delete built-in skill '{}'", name)
        }
        Protection::BuiltIn => format!("Cannot delete built-in MCP server '{}'", name),
        Protection::Bundled => format!(
            "Cannot delete bundled skill '{}' — it ships with Automatic and would be reinstalled on next launch",
            name
        ),
        Protection::Plugin(Some(plugin_id)) => format!(
            "Cannot delete skill '{}' — it is provided by plugin '{}'",
            name, plugin_id
        ),
        Protection::Plugin(None) => {
            format!("Cannot delete rule '{}' — it is provided by a plugin", name)
        }
        Protection::Required => format!(
            "Cannot delete rule '{}' — it is required by Automatic",
            name
        ),
        Protection::Codex => "Cannot delete Codex OpenAI agents.".to_string(),
    })
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
