use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

use super::paths::get_automatic_dir;
use super::types::AgentOptions;

// ── Settings (~/.automatic/settings.json) ────────────────────────────────────

/// Onboarding answers captured by the first-run wizard.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct OnboardingData {
    /// The user's primary development role (e.g. "fullstack", "backend").
    #[serde(default)]
    pub role: String,
    /// How the user incorporates AI into their workflow (e.g. "full_agentic").
    #[serde(default)]
    pub ai_usage: String,
    /// Agent IDs the user selected during onboarding.
    #[serde(default)]
    pub agents: Vec<String>,
    /// Email address provided for newsletter subscription (empty if skipped).
    #[serde(default)]
    pub email: String,
}

/// Tracks which "Getting Started" checklist items have been completed.
/// Each flag is set to true the first time the corresponding action is taken.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct GettingStartedFlags {
    /// Set to true the first time a skill is installed from the skill store.
    #[serde(default)]
    pub skill_installed: bool,
    /// Set to true the first time a project template is imported.
    #[serde(default)]
    pub template_imported: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub skill_sync_mode: String,
    /// Whether the user has opted in to anonymous analytics.
    /// Defaults to true; can be disabled in Settings.
    #[serde(default = "default_analytics_enabled")]
    pub analytics_enabled: bool,
    /// Set to true once the first-run wizard has been completed.
    /// If false (or absent), the wizard is shown on next launch.
    #[serde(default)]
    pub wizard_completed: bool,
    /// Answers collected during the first-run wizard.
    #[serde(default)]
    pub onboarding: OnboardingData,
    /// Agent IDs that are automatically pre-selected when creating a new project.
    #[serde(default)]
    pub default_agents: Vec<String>,
    /// Tracks which getting-started checklist items the user has completed.
    #[serde(default)]
    pub getting_started: GettingStartedFlags,
    /// Set to true once the user dismisses the welcome message on the dashboard.
    #[serde(default)]
    pub welcome_dismissed: bool,
    /// Default per-agent options applied when a new project is created.
    /// Keyed by agent id (e.g. `"claude"`).  Agents absent from this map
    /// use `AgentOptions::default()` when a project is created.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub default_agent_options: HashMap<String, AgentOptions>,
    /// The app version at which bundled skills were last written to disk.
    /// When the current app version differs from this value, all bundled
    /// skills are overwritten with the versions shipped in the new binary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundled_skills_version: Option<String>,
}

fn default_analytics_enabled() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            skill_sync_mode: "symlink".to_string(),
            analytics_enabled: true,
            wizard_completed: false,
            onboarding: OnboardingData::default(),
            default_agents: Vec::new(),
            getting_started: GettingStartedFlags::default(),
            welcome_dismissed: false,
            default_agent_options: HashMap::new(),
            bundled_skills_version: None,
        }
    }
}

pub fn read_settings() -> Result<Settings, String> {
    let path = get_automatic_dir()?.join("settings.json");
    if !path.exists() {
        return Ok(Settings::default());
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

pub fn write_settings(settings: &Settings) -> Result<(), String> {
    let path = get_automatic_dir()?.join("settings.json");
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let raw = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&path, raw).map_err(|e| e.to_string())
}

pub fn reset_settings() -> Result<(), String> {
    write_settings(&Settings::default())
}

pub fn erase_app_data() -> Result<(), String> {
    let automatic_dir = get_automatic_dir()?;
    if automatic_dir.exists() {
        fs::remove_dir_all(&automatic_dir).map_err(|e| e.to_string())?;
    }

    // Recreate a clean baseline immediately so the app still has bundled
    // defaults (rules/instruction templates/skill registry metadata) without
    // requiring a restart.
    write_settings(&Settings::default())?;
    super::install_default_rules()?;
    super::install_default_templates()?;
    super::install_default_skills()?;

    Ok(())
}

/// Mark a getting-started flag as completed and persist settings.
/// This is a best-effort operation — errors are logged but not propagated
/// so that the primary action (install / import) is never blocked.
pub fn mark_skill_installed() -> Result<(), String> {
    let mut settings = read_settings()?;
    if !settings.getting_started.skill_installed {
        settings.getting_started.skill_installed = true;
        write_settings(&settings)?;
    }
    Ok(())
}

pub fn mark_template_imported() -> Result<(), String> {
    let mut settings = read_settings()?;
    if !settings.getting_started.template_imported {
        settings.getting_started.template_imported = true;
        write_settings(&settings)?;
    }
    Ok(())
}

pub fn dismiss_welcome() -> Result<(), String> {
    let mut settings = read_settings()?;
    if !settings.welcome_dismissed {
        settings.welcome_dismissed = true;
        write_settings(&settings)?;
    }
    Ok(())
}
