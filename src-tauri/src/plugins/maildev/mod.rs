pub mod commands;
mod process;
mod types;

pub use types::MaildevStatus;

use crate::core::tools::ToolKind;
use crate::core::{
    PluginCategory, PluginManifest, PluginMcpServerDeclaration, PluginToolDeclaration,
};

/// Maildev's default web UI port, fixed since this plugin always launches
/// it with defaults (no port-configuration UI).
pub const ADMIN_URL: &str = "http://localhost:1080";

/// Return the manifest that describes the Maildev plugin to the Automatic
/// plugin registry. Called by `core::app_plugins::bundled_plugins()`.
///
/// Declares a "maildev" tool for detection/library visibility and an MCP
/// server entry that is installed into the Automatic MCP server registry
/// when the plugin is enabled. Maildev is a single machine-wide daemon, not
/// something scoped to an individual project, so there is no per-project
/// tab — the on/off toggle and admin link live in the global Tools area.
/// `project_scoped: false` keeps the tool out of each project's Tools tab,
/// since adding or removing it there would have no effect.
pub fn manifest() -> PluginManifest {
    PluginManifest {
        id: "maildev".to_string(),
        name: "Maildev".to_string(),
        description: "Local SMTP catcher for testing outgoing email during development, \
                      with a web inbox and an MCP server for inspecting captured mail."
            .to_string(),
        version: "1.0.0".to_string(),
        category: PluginCategory::Integrations,
        enabled_by_default: false,
        tool: Some(PluginToolDeclaration {
            name: "maildev".to_string(),
            display_name: "Maildev".to_string(),
            description: "Local SMTP catcher and web UI for testing outgoing email."
                .to_string(),
            url: "https://maildev.github.io/maildev/".to_string(),
            github_repo: Some("maildev/maildev".to_string()),
            kind: ToolKind::Server,
            detect_binary: Some("maildev".to_string()),
            detect_dir: None,
            provides_tab: false,
            project_scoped: false,
        }),
        skills: vec![],
        rules: vec![],
        mcp_servers: vec![PluginMcpServerDeclaration {
            name: "maildev".to_string(),
            config: serde_json::json!({
                "type": "http",
                "url": "http://localhost:1080/mcp"
            }),
        }],
    }
}
