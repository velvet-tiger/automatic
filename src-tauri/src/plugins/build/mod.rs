pub mod commands;
pub mod features;

use crate::core::tools::ToolKind;
use crate::core::{
    PluginCategory, PluginManifest, PluginRuleDeclaration, PluginSkillDeclaration,
    PluginToolDeclaration,
};

/// Machine name of the rule that tells agents how to use feature tracking.
pub const FEATURES_RULE: &str = "build-features";

/// Bundled content for [`FEATURES_RULE`]. The feature-tracking guidance lives
/// with the plugin, not in the mandatory `automatic-service` rule, so agents
/// are only told to track features on projects that have the Build tool.
const FEATURES_RULE_CONTENT: &str = include_str!("features.md");

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
            project_scoped: true,
        }),
        // Bundled (no `source`), so it installs from the app binary.
        skills: vec![PluginSkillDeclaration {
            name: "automatic-features".to_string(),
            source: None,
        }],
        rules: vec![PluginRuleDeclaration {
            machine_name: FEATURES_RULE.to_string(),
            display_name: "Build: Feature Tracking".to_string(),
        }],
        mcp_servers: vec![],
    }
}

/// Return the content for a plugin-owned rule by machine name.
/// Called by `core::app_plugins::get_plugin_rule_content`.
pub fn rule_content(machine_name: &str) -> Option<String> {
    match machine_name {
        FEATURES_RULE => Some(FEATURES_RULE_CONTENT.to_string()),
        _ => None,
    }
}

/// Name of the project tool that turns on feature tracking. Matches the
/// `tool.name` declared in [`manifest`].
pub const BUILD_TOOL: &str = "build";

/// Explain why feature tracking is unavailable for a project, or return
/// `None` when it is available.
///
/// Feature tracking needs both the Build plugin enabled and the `build` tool
/// on the project. Without the tool the Build board is hidden, so features
/// an agent writes would be invisible to the user.
pub fn feature_tracking_unavailable(
    project_name: &str,
    project_tools: &[String],
    plugin_enabled: bool,
) -> Option<String> {
    if plugin_enabled && project_tools.iter().any(|t| t == BUILD_TOOL) {
        return None;
    }
    let reason = if plugin_enabled {
        "the project does not have the Build tool"
    } else {
        "the Build plugin is disabled"
    };
    Some(format!(
        "Build is not enabled for project '{}' ({}). Feature tracking is \
         unavailable. Continue without it, and do not retry the feature tools.",
        project_name, reason
    ))
}

/// Check that feature tracking is available for a registered project.
///
/// Reads the project and the plugin state. Returns `Err` with an
/// agent-facing message when either read fails or Build is not enabled.
/// `project_name` is an identifier (a `local_key` or a name); the message
/// names the project by its registry name.
pub fn require_feature_tracking(project_name: &str) -> Result<(), String> {
    let raw = crate::core::read_project(project_name)
        .map_err(|e| format!("Failed to read project '{}': {}", project_name, e))?;
    let project: crate::core::Project = serde_json::from_str(&raw)
        .map_err(|e| format!("Failed to parse project '{}': {}", project_name, e))?;
    let plugin_enabled = crate::core::is_app_plugin_enabled("build")
        .map_err(|e| format!("Failed to read plugin state: {}", e))?;
    match feature_tracking_unavailable(&project.name, &project.tools, plugin_enabled) {
        Some(message) => Err(message),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn available_when_plugin_enabled_and_tool_present() {
        assert!(feature_tracking_unavailable("p", &tools(&["build"]), true).is_none());
    }

    #[test]
    fn unavailable_without_tool() {
        let msg = feature_tracking_unavailable("p", &tools(&["common-docs"]), true)
            .expect("should be unavailable");
        assert!(msg.contains("does not have the Build tool"));
        assert!(msg.contains("'p'"));
    }

    #[test]
    fn unavailable_when_plugin_disabled_even_with_tool() {
        let msg = feature_tracking_unavailable("p", &tools(&["build"]), false)
            .expect("should be unavailable");
        assert!(msg.contains("plugin is disabled"));
    }

    #[test]
    fn manifest_declares_matching_tool_rule_and_skill() {
        let m = manifest();
        assert_eq!(m.tool.as_ref().map(|t| t.name.as_str()), Some(BUILD_TOOL));
        assert!(m.rules.iter().any(|r| r.machine_name == FEATURES_RULE));
        assert!(m.skills.iter().any(|s| s.name == "automatic-features"));
        assert!(rule_content(FEATURES_RULE).is_some());
    }
}
