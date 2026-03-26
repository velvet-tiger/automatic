use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

use super::paths::get_automatic_dir;
use super::tools::{delete_tool, read_tool_definition, save_tool, ToolDefinition, ToolKind};

// ── Plugin types ─────────────────────────────────────────────────────────────

/// The category a plugin belongs to. Used for grouping in the UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginCategory {
    /// Core functionality extensions.
    Core,
    /// Plugins that add new agents or AI capabilities.
    Agents,
    /// Integrations with external tools or services.
    Integrations,
    /// Experimental features not yet stable.
    Experimental,
}

impl PluginCategory {
    pub fn label(&self) -> &'static str {
        match self {
            PluginCategory::Core => "Core",
            PluginCategory::Agents => "Agents",
            PluginCategory::Integrations => "Integrations",
            PluginCategory::Experimental => "Experimental",
        }
    }
}

/// A tool declared by a plugin.  When the plugin is enabled, Automatic
/// writes this definition to the tools registry so it appears in the Tools
/// workspace view and is included in project autodetection.
///
/// When the plugin is disabled, the tool is removed from the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginToolDeclaration {
    /// The `ToolDefinition.name` that will be written to the registry.
    pub name: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Short description.
    pub description: String,
    /// Canonical URL (e.g. GitHub repo).
    pub url: String,
    /// `"owner/repo"` — used to fetch the GitHub owner avatar.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github_repo: Option<String>,
    /// Broad tool category.
    pub kind: ToolKind,
    /// Binary name to check with `which` for PATH-based detection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detect_binary: Option<String>,
    /// Relative directory path that signals this tool is initialised in a
    /// project (e.g. `"kitty-specs"` for spec-kitty).  The autodetect pass
    /// checks whether `<project_dir>/<detect_dir>` exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detect_dir: Option<String>,
}

impl PluginToolDeclaration {
    /// Convert this declaration into the `ToolDefinition` that is written to
    /// `~/.automatic/tools/<name>.json`.
    pub fn to_tool_definition(&self, plugin_id: &str) -> ToolDefinition {
        let mut definition = ToolDefinition {
            name: self.name.clone(),
            display_name: self.display_name.clone(),
            description: self.description.clone(),
            url: self.url.clone(),
            github_repo: self.github_repo.clone(),
            kind: self.kind.clone(),
            detect_binary: self.detect_binary.clone(),
            binary_path: None,
            detect_dir: self.detect_dir.clone(),
            plugin_id: Some(plugin_id.to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        if let Ok(existing) = read_tool_definition(&self.name) {
            if existing.plugin_id.as_deref() == Some(plugin_id) {
                definition.binary_path = existing.binary_path;
            }
        }

        definition
    }
}

/// A skill declared by a plugin.  When the plugin is enabled, the skill is
/// installed to `~/.agents/skills/` and recorded in the registry with the
/// plugin's id.  Plugin-provided skills cannot be deleted by the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSkillDeclaration {
    /// The skill name (directory name under `~/.agents/skills/`).
    pub name: String,
}

/// A rule declared by a plugin.  When the plugin is enabled, the rule is
/// written to `~/.automatic/rules/`.  Plugin-provided rules cannot be deleted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRuleDeclaration {
    /// Machine name for the rule file (`{machine_name}.json`).
    pub machine_name: String,
    /// Human-readable display name.
    pub display_name: String,
}

/// Static definition of a bundled plugin. These are compiled into the binary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Stable unique identifier used as the key for enable/disable state.
    pub id: String,
    /// Human-readable display name.
    pub name: String,
    /// Short description shown in the Plugins settings page.
    pub description: String,
    /// Semver version string.
    pub version: String,
    /// Grouping category for UI display.
    pub category: PluginCategory,
    /// Whether the plugin is enabled when first seen by the user.
    pub enabled_by_default: bool,
    /// Optional tool this plugin declares.  When the plugin is enabled, the
    /// tool is written to `~/.automatic/tools/`.  When disabled, it is removed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<PluginToolDeclaration>,
    /// Skills this plugin provides.  Installed on enable, non-removable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<PluginSkillDeclaration>,
    /// Rules this plugin provides.  Installed on enable, non-removable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<PluginRuleDeclaration>,
}

/// A plugin manifest combined with its current enabled/disabled state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEntry {
    #[serde(flatten)]
    pub manifest: PluginManifest,
    /// Whether the plugin is currently enabled by the user.
    pub enabled: bool,
}

/// Persisted file format: just a map of plugin_id -> enabled.
/// Unknown keys are preserved so that adding/removing plugins doesn't
/// corrupt user preferences.
#[derive(Debug, Default, Serialize, Deserialize)]
struct PluginState {
    #[serde(default)]
    plugins: HashMap<String, bool>,
}

// ── Bundled plugin registry ──────────────────────────────────────────────────

/// All plugins bundled with this release of Automatic.
///
/// Each plugin owns its manifest — see `plugins::<name>::manifest()`.
/// To add a new plugin: create the plugin module and add one line here.
fn bundled_plugins() -> Vec<PluginManifest> {
    vec![
        crate::plugins::spec_kitty::manifest(),
        crate::plugins::auto_docs::manifest(),
    ]
}

// ── Persistence ──────────────────────────────────────────────────────────────

fn state_path() -> Result<std::path::PathBuf, String> {
    Ok(get_automatic_dir()?.join("app_plugins.json"))
}

fn read_state() -> Result<PluginState, String> {
    let path = state_path()?;
    if !path.exists() {
        return Ok(PluginState::default());
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

fn write_state(state: &PluginState) -> Result<(), String> {
    let path = state_path()?;
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let raw = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    fs::write(&path, raw).map_err(|e| e.to_string())
}

// ── Tool sync ────────────────────────────────────────────────────────────────

/// Ensure the tools registry reflects the current enabled state of all plugins
/// that declare tools.  Called on app startup and after any plugin toggle.
///
/// - Enabled plugin with a tool declaration → write the tool to the registry.
/// - Disabled plugin with a tool declaration → remove the tool from the registry.
///
/// This is idempotent: re-running it with the same state produces no net change.
fn sync_plugin_tools(manifests: &[PluginManifest], state: &PluginState) {
    for manifest in manifests {
        let Some(ref decl) = manifest.tool else {
            continue;
        };

        let enabled = state
            .plugins
            .get(&manifest.id)
            .copied()
            .unwrap_or(manifest.enabled_by_default);

        if enabled {
            let def = decl.to_tool_definition(&manifest.id);
            match serde_json::to_string_pretty(&def) {
                Ok(json) => {
                    if let Err(e) = save_tool(&def.name, &json) {
                        eprintln!(
                            "[automatic] failed to register tool '{}' for plugin '{}': {}",
                            def.name, manifest.id, e
                        );
                    }
                }
                Err(e) => {
                    eprintln!("[automatic] failed to serialize tool '{}': {}", def.name, e);
                }
            }
        } else {
            // Best-effort removal; ignore errors if the file doesn't exist.
            let _ = delete_tool(&decl.name);
        }
    }
}

// ── Skill sync ──────────────────────────────────────────────────────────

/// Install or remove plugin-declared skills based on current plugin state.
///
/// - Enabled plugin with skill declarations → install via
///   `install_skills_from_bundle` and record source with `plugin_id`.
/// - This is idempotent: re-running produces no net change.
fn sync_plugin_skills(manifests: &[PluginManifest], state: &PluginState) {
    for manifest in manifests {
        if manifest.skills.is_empty() {
            continue;
        }

        let enabled = state
            .plugins
            .get(&manifest.id)
            .copied()
            .unwrap_or(manifest.enabled_by_default);

        if enabled {
            let names: Vec<String> = manifest.skills.iter().map(|s| s.name.clone()).collect();
            if let Err(e) = super::templates::install_skills_from_bundle(&names) {
                eprintln!(
                    "[automatic] failed to install skills for plugin '{}': {}",
                    manifest.id, e
                );
            }
        }
        // Note: we do NOT remove plugin skills on disable — they remain on
        // disk but are no longer marked as plugin-provided (the plugin_id
        // lookup is dynamic based on enabled state).
    }
}

// ── Rule sync ───────────────────────────────────────────────────────────

/// Install or remove plugin-declared rules based on current plugin state.
fn sync_plugin_rules(manifests: &[PluginManifest], state: &PluginState) {
    for manifest in manifests {
        if manifest.rules.is_empty() {
            continue;
        }

        let enabled = state
            .plugins
            .get(&manifest.id)
            .copied()
            .unwrap_or(manifest.enabled_by_default);

        if enabled {
            for decl in &manifest.rules {
                if let Some(content) = get_plugin_rule_content(&manifest.id, &decl.machine_name) {
                    if let Err(e) = super::rules::save_plugin_rule(
                        &decl.machine_name,
                        &decl.display_name,
                        &content,
                        &manifest.id,
                    ) {
                        eprintln!(
                            "[automatic] failed to install rule '{}' for plugin '{}': {}",
                            decl.machine_name, manifest.id, e
                        );
                    }
                }
            }
        }
        // Note: we do NOT remove plugin rules on disable — they remain on
        // disk but the plugin_id field is preserved so the UI can still
        // show them as plugin-provided until re-enabled or manually cleared.
    }
}

/// Retrieve the content for a plugin rule.  Each plugin module provides
/// a `rule_content(machine_name)` function; this dispatches to the right one.
fn get_plugin_rule_content(plugin_id: &str, machine_name: &str) -> Option<String> {
    match plugin_id {
        "auto-docs" => crate::plugins::auto_docs::rule_content(machine_name),
        _ => None,
    }
}

// ── Plugin-skill ownership lookup ───────────────────────────────────────

/// If the named skill is declared by an enabled plugin, return that plugin's
/// id.  Returns `None` if no enabled plugin owns the skill.
pub fn plugin_id_for_skill(skill_name: &str) -> Option<String> {
    let state = read_state().ok()?;
    for manifest in bundled_plugins() {
        let enabled = state
            .plugins
            .get(&manifest.id)
            .copied()
            .unwrap_or(manifest.enabled_by_default);
        if enabled && manifest.skills.iter().any(|s| s.name == skill_name) {
            return Some(manifest.id.clone());
        }
    }
    None
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Return all bundled plugins, merged with their current enabled/disabled state.
///
/// Plugins not present in the persisted state fall back to `enabled_by_default`.
pub fn list_app_plugins() -> Result<Vec<PluginEntry>, String> {
    let state = read_state()?;
    let plugins = bundled_plugins()
        .into_iter()
        .map(|manifest| {
            let enabled = state
                .plugins
                .get(&manifest.id)
                .copied()
                .unwrap_or(manifest.enabled_by_default);
            PluginEntry { manifest, enabled }
        })
        .collect();
    Ok(plugins)
}

/// Enable or disable a plugin by id.  Unknown ids are persisted so future
/// versions that add matching plugins pick up the user's preference.
///
/// When the plugin declares a tool, enabling/disabling it registers or removes
/// that tool from the tools registry.
pub fn set_app_plugin_enabled(id: &str, enabled: bool) -> Result<(), String> {
    let mut state = read_state()?;
    state.plugins.insert(id.to_string(), enabled);
    write_state(&state)?;

    // Sync all plugin-declared resources for the changed plugin.
    let manifests = bundled_plugins();
    sync_plugin_tools(&manifests, &state);
    sync_plugin_skills(&manifests, &state);
    sync_plugin_rules(&manifests, &state);

    Ok(())
}

/// Returns true if the given plugin id is currently enabled.
///
/// Defaults to the plugin's `enabled_by_default` value if no explicit
/// preference has been saved. Returns `false` for unknown plugin ids.
pub fn is_app_plugin_enabled(id: &str) -> Result<bool, String> {
    let state = read_state()?;
    if let Some(&enabled) = state.plugins.get(id) {
        return Ok(enabled);
    }
    // Fall back to the manifest default.
    let enabled = bundled_plugins()
        .iter()
        .find(|m| m.id == id)
        .map(|m| m.enabled_by_default)
        .unwrap_or(false);
    Ok(enabled)
}

/// Called once on app startup to reconcile the tools registry with the current
/// plugin states.  Ensures that a tool is present iff its declaring plugin is
/// enabled, even across app restarts.
pub fn reconcile_plugin_resources_on_startup() {
    match read_state() {
        Ok(state) => {
            let manifests = bundled_plugins();
            sync_plugin_tools(&manifests, &state);
            sync_plugin_skills(&manifests, &state);
            sync_plugin_rules(&manifests, &state);
        }
        Err(e) => {
            eprintln!(
                "[automatic] failed to reconcile plugin resources on startup: {}",
                e
            );
        }
    }
}
