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
    /// project.  The autodetect pass checks whether
    /// `<project_dir>/<detect_dir>` exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detect_dir: Option<String>,

    /// When `true`, this tool contributes a top-level tab in the project UI.
    /// Defaults to `false`.
    #[serde(default)]
    pub provides_tab: bool,

    /// When `true`, this tool is meaningful to add or remove on an
    /// individual project. When `false`, the tool represents a machine-wide
    /// feature (e.g. Maildev's local SMTP daemon) with no per-project
    /// effect, so the Project Tools tab should not offer it.
    pub project_scoped: bool,
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
            provides_tab: self.provides_tab,
            project_scoped: self.project_scoped,
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
/// installed to the managed library (`~/.automatic/library/skills/`) and
/// recorded in the registry with the plugin's id.  Plugin-provided skills
/// cannot be deleted by the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSkillDeclaration {
    /// The skill name (directory name in `~/.automatic/library/skills/`).
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

/// An MCP server declared by a plugin.  When the plugin is enabled, the
/// server config is written to the Automatic MCP server registry
/// (`~/.automatic/library/mcp_servers/`) so it is available to assign to any
/// project.  Intentionally left in place when the plugin is disabled — see
/// `sync_plugin_mcp_servers`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMcpServerDeclaration {
    /// The MCP server registry name (`{name}.json`).
    pub name: String,
    /// Raw MCP server config, in the same shape written to
    /// `~/.automatic/library/mcp_servers/<name>.json` (e.g.
    /// `{"type": "http", "url": "..."}`).
    pub config: serde_json::Value,
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
    /// MCP servers this plugin provides.  Installed into the registry on
    /// enable; left in place on disable (see `sync_plugin_mcp_servers`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<PluginMcpServerDeclaration>,
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
        crate::plugins::build::manifest(),
        crate::plugins::common_docs::manifest(),
        crate::plugins::dev_servers::manifest(),
        crate::plugins::maildev::manifest(),
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
/// - Tool whose `plugin_id` is no longer in the bundled plugin set → remove it
///   (covers plugins dropped from a release, e.g. Spec Kitty).
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

    remove_orphaned_plugin_tools(manifests);
}

/// Delete tool definitions whose `plugin_id` no longer maps to a bundled
/// plugin, then scrub those names from every project's `tools` list.
///
/// When a plugin is removed from `bundled_plugins()` (as Spec Kitty was),
/// `sync_plugin_tools` no longer sees it, so the on-disk tool file and any
/// project references would otherwise linger forever.  Manual tools
/// (`plugin_id: None`) are never touched.
fn remove_orphaned_plugin_tools(manifests: &[PluginManifest]) {
    let known_plugin_ids: std::collections::HashSet<&str> =
        manifests.iter().map(|m| m.id.as_str()).collect();

    let names = match super::tools::list_tools() {
        Ok(n) => n,
        Err(e) => {
            eprintln!(
                "[automatic] failed to list tools while removing orphaned plugin tools: {}",
                e
            );
            return;
        }
    };

    let mut removed: Vec<String> = Vec::new();
    for name in names {
        let definition = match read_tool_definition(&name) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let Some(ref plugin_id) = definition.plugin_id else {
            continue;
        };
        if known_plugin_ids.contains(plugin_id.as_str()) {
            continue;
        }
        if let Err(e) = delete_tool(&name) {
            eprintln!(
                "[automatic] failed to remove orphaned plugin tool '{}' (plugin '{}'): {}",
                name, plugin_id, e
            );
            continue;
        }
        eprintln!(
            "[automatic] removed orphaned plugin tool '{}' (plugin '{}' no longer bundled)",
            name, plugin_id
        );
        removed.push(name);
    }

    if !removed.is_empty() {
        scrub_tools_from_projects(&removed);
    }
}

/// Drop each of `tool_names` from every project's `tools` array.
///
/// Mirrors the rule-migration scrub: prefer `.automatic/project.json` when a
/// project directory is set, otherwise update the registry entry directly.
fn scrub_tools_from_projects(tool_names: &[String]) {
    let projects_dir = match super::paths::get_projects_dir() {
        Ok(p) => p,
        Err(_) => return,
    };
    if !projects_dir.exists() {
        return;
    }
    let entries = match fs::read_dir(&projects_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let registry_path = entry.path();
        if !registry_path.is_file()
            || registry_path.extension().and_then(|e| e.to_str()) != Some("json")
        {
            continue;
        }
        let raw = match fs::read_to_string(&registry_path) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let mut value: serde_json::Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let project_dir = value
            .get("directory")
            .and_then(|d| d.as_str())
            .filter(|d| !d.is_empty())
            .map(|d| d.to_string());

        if let Some(ref dir) = project_dir {
            let config_path = std::path::PathBuf::from(dir)
                .join(".automatic")
                .join("project.json");
            if config_path.exists() {
                if let Ok(config_raw) = fs::read_to_string(&config_path) {
                    if let Ok(mut config) = serde_json::from_str::<serde_json::Value>(&config_raw) {
                        if remove_tools_from_value(&mut config, tool_names) {
                            if let Ok(pretty) = serde_json::to_string_pretty(&config) {
                                let _ = fs::write(&config_path, pretty);
                            }
                        }
                    }
                }
                continue;
            }
        }

        if remove_tools_from_value(&mut value, tool_names) {
            if let Ok(pretty) = serde_json::to_string_pretty(&value) {
                let _ = fs::write(&registry_path, pretty);
            }
        }
    }
}

/// Remove each of `tool_names` from a project's `tools` array.
/// Returns `true` if the array shrank.
fn remove_tools_from_value(project: &mut serde_json::Value, tool_names: &[String]) -> bool {
    let Some(arr) = project.get_mut("tools").and_then(|v| v.as_array_mut()) else {
        return false;
    };
    let before = arr.len();
    arr.retain(|e| {
        e.as_str()
            .map(|name| !tool_names.iter().any(|t| t == name))
            .unwrap_or(true)
    });
    arr.len() != before
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
                if let Err(e) = super::install_skills_from_bundle(&bundled_names) {
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

// ── MCP server sync ──────────────────────────────────────────────────────

/// Install plugin-declared MCP servers into the registry based on current
/// plugin state.
///
/// Mirrors `sync_plugin_rules`: on disable we deliberately leave the
/// registry entry in place rather than deleting it.  A project may already
/// have this MCP server assigned in its `mcp_servers` list — silently
/// removing the registry entry out from under it on a mere plugin-disable
/// is a much bigger, more surprising action than toggling a feature flag.
fn sync_plugin_mcp_servers(manifests: &[PluginManifest], state: &PluginState) {
    for manifest in manifests {
        if manifest.mcp_servers.is_empty() {
            continue;
        }

        let enabled = state
            .plugins
            .get(&manifest.id)
            .copied()
            .unwrap_or(manifest.enabled_by_default);

        if !enabled {
            continue;
        }

        for decl in &manifest.mcp_servers {
            if let Err(e) =
                super::mcp_servers::save_mcp_server_config(&decl.name, &decl.config.to_string())
            {
                eprintln!(
                    "[automatic] failed to register MCP server '{}' for plugin '{}': {}",
                    decl.name, manifest.id, e
                );
            }
        }
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
pub fn strip_plugin_resources(project: &mut super::types::Project, removed_tool_names: &[String]) {
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

        let rule_names: Vec<String> = manifest
            .rules
            .iter()
            .map(|r| r.machine_name.clone())
            .collect();
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
    sync_plugin_mcp_servers(&manifests, &state);

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
/// enabled, even across app restarts.  Also removes tools whose declaring
/// plugin is no longer bundled.
pub fn reconcile_plugin_resources_on_startup() {
    match read_state() {
        Ok(state) => {
            let manifests = bundled_plugins();
            sync_plugin_tools(&manifests, &state);
            sync_plugin_skills(&manifests, &state);
            sync_plugin_rules(&manifests, &state);
            sync_plugin_mcp_servers(&manifests, &state);
        }
        Err(e) => {
            eprintln!(
                "[automatic] failed to reconcile plugin resources on startup: {}",
                e
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::paths::{get_library_dir, get_projects_dir, with_test_home};
    use std::path::Path;
    use tempfile::TempDir;

    fn with_temp_home(test: impl FnOnce(&Path)) {
        let tmp = TempDir::new().expect("tempdir");
        with_test_home(tmp.path().to_path_buf(), || test(tmp.path()));
    }

    fn write_tool_file(tools_dir: &Path, name: &str, plugin_id: Option<&str>) {
        fs::create_dir_all(tools_dir).expect("tools dir");
        let mut def = serde_json::json!({
            "name": name,
            "display_name": name,
            "description": "test tool",
            "url": "https://example.com",
            "kind": "other",
            "created_at": "2026-01-01T00:00:00Z",
        });
        if let Some(pid) = plugin_id {
            def["plugin_id"] = serde_json::Value::String(pid.to_string());
        }
        fs::write(
            tools_dir.join(format!("{}.json", name)),
            serde_json::to_string_pretty(&def).unwrap(),
        )
        .unwrap();
    }

    fn known_manifest(id: &str) -> PluginManifest {
        PluginManifest {
            id: id.to_string(),
            name: id.to_string(),
            description: "test".into(),
            version: "1.0.0".into(),
            category: PluginCategory::Core,
            enabled_by_default: true,
            tool: None,
            skills: vec![],
            rules: vec![],
            mcp_servers: vec![],
        }
    }

    #[test]
    fn remove_tools_from_value_drops_matching_names() {
        let mut project = serde_json::json!({
            "tools": ["build", "spec-kitty", "common-docs"]
        });
        let changed = remove_tools_from_value(
            &mut project,
            &["spec-kitty".to_string()],
        );
        assert!(changed);
        assert_eq!(
            project["tools"],
            serde_json::json!(["build", "common-docs"])
        );
    }

    #[test]
    fn remove_tools_from_value_noop_when_absent() {
        let mut project = serde_json::json!({ "tools": ["build"] });
        let changed = remove_tools_from_value(
            &mut project,
            &["spec-kitty".to_string()],
        );
        assert!(!changed);
        assert_eq!(project["tools"], serde_json::json!(["build"]));
    }

    #[test]
    fn orphan_cleanup_deletes_unbundled_plugin_tool_and_scrubs_project() {
        with_temp_home(|_home| {
            let tools_dir = get_library_dir().unwrap().join("tools");
            write_tool_file(&tools_dir, "build", Some("build"));
            write_tool_file(&tools_dir, "spec-kitty", Some("spec-kitty"));
            write_tool_file(&tools_dir, "manual-tool", None);

            let projects_dir = get_projects_dir().unwrap();
            fs::create_dir_all(&projects_dir).unwrap();
            let proj_tmp = TempDir::new().unwrap();
            let proj_dir = proj_tmp.path().to_path_buf();

            fs::write(
                projects_dir.join("demo.json"),
                serde_json::to_string_pretty(&serde_json::json!({
                    "name": "demo",
                    "directory": proj_dir.to_string_lossy(),
                }))
                .unwrap(),
            )
            .unwrap();

            let config_dir = proj_dir.join(".automatic");
            fs::create_dir_all(&config_dir).unwrap();
            fs::write(
                config_dir.join("project.json"),
                serde_json::to_string_pretty(&serde_json::json!({
                    "name": "demo",
                    "directory": proj_dir.to_string_lossy(),
                    "tools": ["build", "spec-kitty"]
                }))
                .unwrap(),
            )
            .unwrap();

            // Only "build" is still bundled — "spec-kitty" is orphaned.
            remove_orphaned_plugin_tools(&[known_manifest("build")]);

            assert!(tools_dir.join("build.json").exists());
            assert!(
                !tools_dir.join("spec-kitty.json").exists(),
                "orphaned plugin tool should be deleted"
            );
            assert!(
                tools_dir.join("manual-tool.json").exists(),
                "manual tools must not be deleted"
            );

            let config: serde_json::Value = serde_json::from_str(
                &fs::read_to_string(config_dir.join("project.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(
                config["tools"],
                serde_json::json!(["build"]),
                "project tools should drop the orphaned name"
            );
        });
    }

    #[test]
    fn orphan_cleanup_is_idempotent_when_nothing_orphaned() {
        with_temp_home(|_home| {
            let tools_dir = get_library_dir().unwrap().join("tools");
            write_tool_file(&tools_dir, "build", Some("build"));

            remove_orphaned_plugin_tools(&[known_manifest("build")]);
            remove_orphaned_plugin_tools(&[known_manifest("build")]);

            assert!(tools_dir.join("build.json").exists());
        });
    }

    // ── sync_plugin_mcp_servers ─────────────────────────────────────────────

    fn known_manifest_with_mcp_server(id: &str, mcp_name: &str) -> PluginManifest {
        let mut m = known_manifest(id);
        m.mcp_servers = vec![PluginMcpServerDeclaration {
            name: mcp_name.to_string(),
            config: serde_json::json!({"type": "http", "url": "http://localhost:1080/mcp"}),
        }];
        m
    }

    fn state_with(id: &str, enabled: bool) -> PluginState {
        let mut state = PluginState::default();
        state.plugins.insert(id.to_string(), enabled);
        state
    }

    #[test]
    fn sync_plugin_mcp_servers_writes_registry_entry_when_enabled() {
        with_temp_home(|_home| {
            let manifests = [known_manifest_with_mcp_server("maildev", "maildev")];
            let state = state_with("maildev", true);

            sync_plugin_mcp_servers(&manifests, &state);

            let raw = super::super::mcp_servers::read_mcp_server_config("maildev")
                .expect("registry entry should exist");
            let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
            assert_eq!(value["url"].as_str(), Some("http://localhost:1080/mcp"));
        });
    }

    #[test]
    fn sync_plugin_mcp_servers_skips_write_when_disabled() {
        with_temp_home(|_home| {
            let manifests = [known_manifest_with_mcp_server("maildev", "maildev")];
            let state = state_with("maildev", false);

            sync_plugin_mcp_servers(&manifests, &state);

            assert!(super::super::mcp_servers::read_mcp_server_config("maildev").is_err());
        });
    }

    #[test]
    fn sync_plugin_mcp_servers_does_not_delete_registry_entry_on_disable() {
        with_temp_home(|_home| {
            let manifests = [known_manifest_with_mcp_server("maildev", "maildev")];

            sync_plugin_mcp_servers(&manifests, &state_with("maildev", true));
            assert!(super::super::mcp_servers::read_mcp_server_config("maildev").is_ok());

            sync_plugin_mcp_servers(&manifests, &state_with("maildev", false));
            assert!(
                super::super::mcp_servers::read_mcp_server_config("maildev").is_ok(),
                "registry entry must remain after the declaring plugin is disabled"
            );
        });
    }

    #[test]
    fn enabling_maildev_plugin_writes_mcp_registry_entry() {
        with_temp_home(|_home| {
            set_app_plugin_enabled("maildev", true).expect("enable");
            let raw = super::super::mcp_servers::read_mcp_server_config("maildev")
                .expect("registry entry should exist");
            let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
            assert_eq!(value["url"].as_str(), Some("http://localhost:1080/mcp"));
        });
    }

    #[test]
    fn disabling_maildev_plugin_leaves_mcp_registry_entry_in_place() {
        with_temp_home(|_home| {
            set_app_plugin_enabled("maildev", true).expect("enable");
            set_app_plugin_enabled("maildev", false).expect("disable");
            assert!(
                super::super::mcp_servers::read_mcp_server_config("maildev").is_ok(),
                "registry entry must survive disabling the plugin"
            );
        });
    }
}
