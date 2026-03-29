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
    /// Optional GitHub source ("owner/repo") for fetching the skill remotely.
    /// When set, the skill is fetched from the repo instead of the app bundle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
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
        crate::plugins::common_docs::manifest(),
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
/// - Bundled skills (no `source`) are installed from the app binary.
/// - Remote skills (with `source`) are fetched from GitHub in the background.
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
            // Split skills into bundled (no source) and remote (has source).
            let bundled_names: Vec<String> = manifest
                .skills
                .iter()
                .filter(|s| s.source.is_none())
                .map(|s| s.name.clone())
                .collect();

            if !bundled_names.is_empty() {
                if let Err(e) = super::templates::install_skills_from_bundle(&bundled_names) {
                    eprintln!(
                        "[automatic] failed to install bundled skills for plugin '{}': {}",
                        manifest.id, e
                    );
                }
            }

            // Fetch remote skills in the background.
            let remote_skills: Vec<(String, String)> = manifest
                .skills
                .iter()
                .filter_map(|s| s.source.as_ref().map(|src| (s.name.clone(), src.clone())))
                .collect();

            let plugin_id = manifest.id.clone();
            if !remote_skills.is_empty() {
                tauri::async_runtime::spawn(async move {
                    for (name, source) in &remote_skills {
                        // Skip if already installed.
                        if super::skills::skill_exists(name) {
                            continue;
                        }
                        match super::skill_store::fetch_remote_skill_content(source, name).await {
                            Ok(content) => {
                                if let Err(e) = super::skills::save_skill(name, &content) {
                                    eprintln!(
                                        "[automatic] failed to save remote skill '{}' for plugin '{}': {}",
                                        name, plugin_id, e
                                    );
                                } else {
                                    let id = format!("{}/{}", source, name);
                                    if let Err(e) = super::skill_store::record_skill_source(
                                        name, source, &id, "github",
                                    ) {
                                        eprintln!(
                                            "[automatic] failed to record source for skill '{}': {}",
                                            name, e
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!(
                                    "[automatic] failed to fetch remote skill '{}' from '{}': {}",
                                    name, source, e
                                );
                            }
                        }
                    }
                });
            }
        } else {
            // Plugin disabled: delete its skills so they do not linger as
            // decoupled, unattributed entries in the user's skill library.
            for decl in &manifest.skills {
                if super::skills::skill_exists(&decl.name) {
                    if let Err(e) = super::skills::delete_skill(&decl.name) {
                        eprintln!(
                            "[automatic] failed to remove skill '{}' for disabled plugin '{}': {}",
                            decl.name, manifest.id, e
                        );
                    }
                }
            }
        }
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
        "common-docs" => crate::plugins::common_docs::rule_content(machine_name),
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

// ── Project-level plugin resource enrichment ─────────────────────────────────

/// When plugin tools are newly added to a project, add the plugin's declared
/// skills and rules to the project so they are persisted and synced.
///
/// `new_tool_names` should contain only the tool names that were just added
/// (i.e. present in the incoming project but absent from the existing one).
pub fn enrich_project_with_plugin_resources(
    project: &mut super::types::Project,
    new_tool_names: &[String],
) {
    let state = match read_state() {
        Ok(s) => s,
        Err(_) => return,
    };

    for manifest in bundled_plugins() {
        let enabled = state
            .plugins
            .get(&manifest.id)
            .copied()
            .unwrap_or(manifest.enabled_by_default);
        if !enabled {
            continue;
        }

        // Check if this plugin's tool is among the newly added tools.
        let tool_added = manifest
            .tool
            .as_ref()
            .map(|t| new_tool_names.contains(&t.name))
            .unwrap_or(false);

        if !tool_added {
            continue;
        }

        // Add plugin skills to the project's skill list.
        for decl in &manifest.skills {
            if !project.skills.contains(&decl.name) {
                project.skills.push(decl.name.clone());
            }
        }

        // Add plugin rules to the project's rule list.
        let project_rules = project
            .file_rules
            .entry("_project".to_string())
            .or_insert_with(Vec::new);
        for decl in &manifest.rules {
            if !project_rules.contains(&decl.machine_name) {
                project_rules.push(decl.machine_name.clone());
            }
        }
    }
}

/// When plugin tools are removed from a project, strip the plugin's declared
/// skills and rules from the project.
pub fn strip_plugin_resources(
    project: &mut super::types::Project,
    removed_tool_names: &[String],
) {
    for manifest in bundled_plugins() {
        let tool_removed = manifest
            .tool
            .as_ref()
            .map(|t| removed_tool_names.contains(&t.name))
            .unwrap_or(false);

        if !tool_removed {
            continue;
        }

        let skill_names: Vec<String> = manifest.skills.iter().map(|s| s.name.clone()).collect();
        project.skills.retain(|s| !skill_names.contains(s));

        let rule_names: Vec<String> = manifest.rules.iter().map(|r| r.machine_name.clone()).collect();
        if let Some(project_rules) = project.file_rules.get_mut("_project") {
            project_rules.retain(|r| !rule_names.contains(r));
        }
    }
}

// ── Locked resource query ────────────────────────────────────────────────────

/// Given a list of tool names on a project, return skill and rule names that
/// are provided by enabled plugins whose tool is in the list.
pub fn get_plugin_locked_resources(tool_names: &[String]) -> (Vec<String>, Vec<String>) {
    let state = match read_state() {
        Ok(s) => s,
        Err(_) => return (vec![], vec![]),
    };

    let mut skills = Vec::new();
    let mut rules = Vec::new();

    for manifest in bundled_plugins() {
        let enabled = state
            .plugins
            .get(&manifest.id)
            .copied()
            .unwrap_or(manifest.enabled_by_default);
        if !enabled {
            continue;
        }

        let tool_present = manifest
            .tool
            .as_ref()
            .map(|t| tool_names.contains(&t.name))
            .unwrap_or(false);
        if !tool_present {
            continue;
        }

        for decl in &manifest.skills {
            skills.push(decl.name.clone());
        }
        for decl in &manifest.rules {
            rules.push(decl.machine_name.clone());
        }
    }

    (skills, rules)
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
