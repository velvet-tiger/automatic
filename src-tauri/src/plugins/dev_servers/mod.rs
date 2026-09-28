pub mod commands;
mod detect;
mod process;
pub(crate) mod registry;
mod types;

pub use types::{DevServerStatus, LogLine, LogStream, NpmScriptEntry, PackageManager, ServerConfig};

use crate::core::tools::ToolKind;
use crate::core::{PluginCategory, PluginManifest, PluginToolDeclaration};

/// Move dev servers still keyed by a project's old name under its store
/// key: running processes and the registry file. Dev servers are keyed by
/// `local_key` (stage 3b step 2), which a rename does not change, so this
/// only matters for a file the startup migration has not reached yet.
/// `store_key` is the project's `local_key`, or its new name when it has
/// none. Called from the project rename command. Errors only from the
/// registry step (for example when a file already exists under
/// `store_key`); the process step cannot fail.
pub fn adopt_legacy_project(old_name: &str, store_key: &str) -> Result<(), String> {
    process::rename_project(old_name, store_key);
    registry::rename_project(old_name, store_key)
}

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
