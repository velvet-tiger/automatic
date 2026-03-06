use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

use super::paths::get_automatic_dir;

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
/// To add a new plugin:
///   1. Add an entry here with a stable `id`.
///   2. Implement the feature in the frontend, gated behind `usePlugin(id)`.
///   3. The user can then enable/disable it from Settings > Plugins.
fn bundled_plugins() -> Vec<PluginManifest> {
    vec![]
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

/// Enable or disable a plugin by id. Unknown ids are persisted so future
/// versions that add matching plugins pick up the user's preference.
pub fn set_app_plugin_enabled(id: &str, enabled: bool) -> Result<(), String> {
    let mut state = read_state()?;
    state.plugins.insert(id.to_string(), enabled);
    write_state(&state)
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
