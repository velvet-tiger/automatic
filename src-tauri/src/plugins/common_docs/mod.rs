use crate::core::tools::ToolKind;
use crate::core::{
    PluginCategory, PluginManifest, PluginRuleDeclaration, PluginSkillDeclaration,
    PluginToolDeclaration,
};

/// Bundled rule content for the common-docs plugin.
const RULE_CONTENT: &str = include_str!("../../../rules/common-docs/documentation.md");

/// Return the manifest that describes this plugin to the Automatic plugin
/// registry.  Called by `core::app_plugins::bundled_plugins()`.
pub fn manifest() -> PluginManifest {
    PluginManifest {
        id: "common-docs".to_string(),
        name: "Common Docs".to_string(),
        description: "Standard documentation structure for projects. Adds scaffolding and \
                      navigation skills plus a documentation guidelines rule."
            .to_string(),
        version: "1.0.0".to_string(),
        category: PluginCategory::Core,
        enabled_by_default: false,
        tool: Some(PluginToolDeclaration {
            name: "common-docs".to_string(),
            display_name: "Common Docs".to_string(),
            description: "Standard documentation structure for projects. Scaffolds docs/ \
                          directories and enforces documentation guidelines."
                .to_string(),
            url: "https://github.com/velvet-tiger/common-docs".to_string(),
            github_repo: Some("velvet-tiger/common-docs".to_string()),
            kind: ToolKind::DocGen,
            detect_binary: None,
            detect_dir: None,
        }),
        skills: vec![
            PluginSkillDeclaration {
                name: "common-docs-scaffold".to_string(),
                source: Some("velvet-tiger/common-docs".to_string()),
            },
            PluginSkillDeclaration {
                name: "common-docs-find".to_string(),
                source: Some("velvet-tiger/common-docs".to_string()),
            },
        ],
        rules: vec![PluginRuleDeclaration {
            machine_name: "common-docs-documentation".to_string(),
            display_name: "Common Docs".to_string(),
        }],
    }
}

/// Return the content for a plugin-owned rule by machine name.
/// Called by `core::app_plugins::get_plugin_rule_content`.
pub fn rule_content(machine_name: &str) -> Option<String> {
    match machine_name {
        "common-docs-documentation" => Some(RULE_CONTENT.to_string()),
        _ => None,
    }
}
