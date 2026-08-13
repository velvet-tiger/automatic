pub mod commands;
pub mod features;

use crate::core::tools::ToolKind;
use crate::core::{PluginCategory, PluginManifest, PluginToolDeclaration};

/// Return the manifest that describes the Build plugin to the Automatic plugin
/// registry.  Called by `core::app_plugins::bundled_plugins()`.
///
/// The Build plugin declares a "build" tool that can be added to individual
/// projects.  When the tool is present on a project, the Build tab (Features
/// kanban/issue tracker) appears in that project's UI.  Removing the tool
/// hides the tab without deleting any feature data.
pub fn manifest() -> PluginManifest {
    PluginManifest {
        id: "build".to_string(),
        name: "Build".to_string(),
        description: "Project issue tracking and feature planning. Adds a Build tab to each \
                      project with a kanban board for managing features, bugs, and tasks."
            .to_string(),
        version: "1.0.0".to_string(),
        category: PluginCategory::Core,
        enabled_by_default: false,
        tool: Some(PluginToolDeclaration {
            name: "build".to_string(),
            display_name: "Build".to_string(),
            description: "Project issue tracking and feature planning with a kanban board."
                .to_string(),
            url: "https://github.com/velvet-tiger/automatic".to_string(),
            github_repo: Some("velvet-tiger/automatic".to_string()),
            kind: ToolKind::Planning,
            detect_binary: None,
            detect_dir: None,
            provides_tab: true,
        }),
        skills: vec![],
        rules: vec![],
        mcp_servers: vec![],
    }
}
