use crate::core::{
    PluginCategory, PluginManifest, PluginRuleDeclaration, PluginSkillDeclaration,
};

/// Bundled rule content for the auto-docs plugin.
const RULE_CONTENT: &str = include_str!("../../../rules/auto-docs/documentation.md");

/// Return the manifest that describes this plugin to the Automatic plugin
/// registry.  Called by `core::app_plugins::bundled_plugins()`.
pub fn manifest() -> PluginManifest {
    PluginManifest {
        id: "auto-docs".to_string(),
        name: "Auto Docs".to_string(),
        description: "Standard documentation structure for projects. Adds scaffolding and \
                      navigation skills plus a documentation guidelines rule."
            .to_string(),
        version: "1.0.0".to_string(),
        category: PluginCategory::Core,
        enabled_by_default: false,
        tool: None,
        skills: vec![
            PluginSkillDeclaration {
                name: "automatic-docs".to_string(),
            },
            PluginSkillDeclaration {
                name: "automatic-docs-find".to_string(),
            },
        ],
        rules: vec![PluginRuleDeclaration {
            machine_name: "auto-docs-documentation".to_string(),
            display_name: "Auto Docs".to_string(),
        }],
    }
}

/// Return the content for a plugin-owned rule by machine name.
/// Called by `core::app_plugins::get_plugin_rule_content`.
pub fn rule_content(machine_name: &str) -> Option<String> {
    match machine_name {
        "auto-docs-documentation" => Some(RULE_CONTENT.to_string()),
        _ => None,
    }
}
