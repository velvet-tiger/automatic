mod commands;

pub use commands::*;

use crate::core::tools::ToolKind;
use crate::core::{PluginCategory, PluginManifest, PluginToolDeclaration};

/// Return the manifest that describes this plugin to the Automatic plugin
/// registry.  Called by `core::app_plugins::bundled_plugins()` — all
/// plugin-specific identity data lives here, not in core.
pub fn manifest() -> PluginManifest {
    PluginManifest {
        id: "spec-kitty".to_string(),
        name: "Spec Kitty".to_string(),
        description: "Spec-driven development for AI coding agents. Tracks specs, plans, and \
                      work packages via kitty-specs/ in your project."
            .to_string(),
        version: "1.0.0".to_string(),
        category: PluginCategory::Integrations,
        enabled_by_default: false,
        tool: Some(PluginToolDeclaration {
            name: "spec-kitty".to_string(),
            display_name: "Spec Kitty".to_string(),
            description: "Spec-driven development CLI for AI agents. Generates spec.md, \
                          plan.md, and tasks.md in kitty-specs/<feature>/ and provides a \
                          live kanban dashboard."
                .to_string(),
            url: "https://github.com/Priivacy-ai/spec-kitty".to_string(),
            github_repo: Some("Priivacy-ai/spec-kitty".to_string()),
            kind: ToolKind::DocGen,
            detect_binary: Some("spec-kitty".to_string()),
            detect_dir: Some("kitty-specs".to_string()),
        }),
        skills: vec![],
        rules: vec![],
    }
}
