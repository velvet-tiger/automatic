pub mod commands;
mod detect;
mod process;
mod registry;
mod types;

pub use types::{DevServerStatus, LogLine, LogStream, NpmScriptEntry, PackageManager, ServerConfig};

use crate::core::tools::ToolKind;
use crate::core::{PluginCategory, PluginManifest, PluginToolDeclaration};

/// Return the manifest that describes the Dev Servers plugin to the
/// Automatic plugin registry. Called by `core::app_plugins::bundled_plugins()`.
///
/// Declares a "dev-servers" tool that, when added to a project, gives it a
/// Servers tab for starting, stopping, and monitoring npm, pnpm, and yarn
/// dev servers. Enabling the plugin also surfaces a cross-project "Servers"
/// view under the global Tools section.
pub fn manifest() -> PluginManifest {
    PluginManifest {
        id: "dev-servers".to_string(),
        name: "Dev Servers".to_string(),
        description: "Start, stop, and monitor npm, pnpm, and yarn dev servers for a project. \
                      Adds a Servers tab to each project and a cross-project view under Tools."
            .to_string(),
        version: "1.0.0".to_string(),
        category: PluginCategory::Core,
        enabled_by_default: false,
        tool: Some(PluginToolDeclaration {
            name: "dev-servers".to_string(),
            display_name: "Servers".to_string(),
            description: "Start, stop, and monitor npm, pnpm, and yarn dev servers.".to_string(),
            url: "https://github.com/velvet-tiger/automatic".to_string(),
            github_repo: Some("velvet-tiger/automatic".to_string()),
            kind: ToolKind::Server,
            detect_binary: None,
            // `detect_dir` just checks this path exists under the project
            // directory — that works for a file, not just a directory.
            detect_dir: Some("package.json".to_string()),
            provides_tab: true,
            project_scoped: true,
        }),
        skills: vec![],
        rules: vec![],
        mcp_servers: vec![],
    }
}
