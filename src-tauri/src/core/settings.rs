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
    /// How items (skills, etc.) are synced to agent directories: `"symlink"` or `"copy"`.
    /// Renamed from the legacy `skill_sync_mode`; the alias ensures old settings files
    /// deserialise without data loss.
    #[serde(alias = "skill_sync_mode")]
    pub sync_mode: String,
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
    /// The release version the user last viewed in the "What's New" section.
    /// Used to determine whether a badge/indicator should be shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub whats_new_seen_version: Option<String>,
}

fn default_analytics_enabled() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sync_mode: "symlink".to_string(),
            analytics_enabled: true,
            wizard_completed: false,
            onboarding: OnboardingData::default(),
            default_agents: Vec::new(),
            getting_started: GettingStartedFlags::default(),
            welcome_dismissed: false,
            default_agent_options: HashMap::new(),
            bundled_skills_version: None,
            whats_new_seen_version: None,
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

/// Reinstall all bundled defaults (rules, templates, skills, bundled agents,
/// marketplace catalogues, and the Automatic MCP server) without touching
/// projects, memories, or app settings.
///
/// Every bundled file is force-overwritten so the on-disk copies match the
/// current binary — equivalent to what happens on a version upgrade but
/// scoped only to the factory-supplied content.
pub fn reinstall_defaults() -> Result<(), String> {
    super::init_marketplace_files(true)?;
    super::install_default_rules_inner(true)?;
    super::install_default_templates_inner(true)?;
    super::install_default_skills_inner(true)?;
    super::install_default_user_agents_inner(true)?;
    super::ensure_automatic_in_global_mcp()?;
    Ok(())
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
    super::init_marketplace_files(false)?;
    super::install_default_rules()?;
    super::install_default_templates()?;
    super::install_default_skills()?;
    super::install_default_user_agents()?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    // ── Path-injectable helpers ───────────────────────────────────────────────

    fn settings_path(base: &Path) -> std::path::PathBuf {
        base.join("settings.json")
    }

    fn read_at(base: &Path) -> Result<Settings, String> {
        let path = settings_path(base);
        if !path.exists() {
            return Ok(Settings::default());
        }
        let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        Ok(serde_json::from_str(&raw).unwrap_or_default())
    }

    fn write_at(base: &Path, settings: &Settings) -> Result<(), String> {
        let path = settings_path(base);
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
        }
        let raw = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
        fs::write(&path, raw).map_err(|e| e.to_string())
    }

    fn tmp() -> TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    // ── Default values ────────────────────────────────────────────────────────

    #[test]
    fn default_settings_have_expected_values() {
        let s = Settings::default();
        assert_eq!(s.sync_mode, "symlink");
        assert!(s.analytics_enabled);
        assert!(!s.wizard_completed);
        assert!(!s.welcome_dismissed);
        assert!(s.default_agents.is_empty());
    }

    // ── Read (missing file) ───────────────────────────────────────────────────

    #[test]
    fn read_returns_defaults_when_file_missing() {
        let dir = tmp();
        let settings = read_at(dir.path()).expect("read");
        assert_eq!(settings.sync_mode, "symlink");
        assert!(settings.analytics_enabled);
    }

    // ── Write + Read roundtrip ────────────────────────────────────────────────

    #[test]
    fn write_and_read_roundtrip() {
        let dir = tmp();
        let mut s = Settings::default();
        s.sync_mode = "copy".to_string();
        s.analytics_enabled = false;
        s.wizard_completed = true;

        write_at(dir.path(), &s).expect("write");
        let loaded = read_at(dir.path()).expect("read");

        assert_eq!(loaded.sync_mode, "copy");
        assert!(!loaded.analytics_enabled);
        assert!(loaded.wizard_completed);
    }

    #[test]
    fn write_creates_parent_directories() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let nested = tmp.path().join("a").join("b").join("c");
        let s = Settings::default();
        write_at(&nested, &s).expect("write to nested dir");
        assert!(settings_path(&nested).exists());
    }

    // ── Overwrite ─────────────────────────────────────────────────────────────

    #[test]
    fn second_write_overwrites_first() {
        let dir = tmp();

        let mut s1 = Settings::default();
        s1.wizard_completed = false;
        write_at(dir.path(), &s1).expect("write s1");

        let mut s2 = Settings::default();
        s2.wizard_completed = true;
        write_at(dir.path(), &s2).expect("write s2");

        let loaded = read_at(dir.path()).expect("read");
        assert!(loaded.wizard_completed);
    }

    // ── Default agents ────────────────────────────────────────────────────────

    #[test]
    fn default_agents_are_preserved() {
        let dir = tmp();
        let mut s = Settings::default();
        s.default_agents = vec!["claude".to_string(), "cursor".to_string()];
        write_at(dir.path(), &s).expect("write");

        let loaded = read_at(dir.path()).expect("read");
        assert_eq!(loaded.default_agents, vec!["claude", "cursor"]);
    }

    // ── Onboarding ────────────────────────────────────────────────────────────

    #[test]
    fn onboarding_data_round_trips() {
        let dir = tmp();
        let mut s = Settings::default();
        s.onboarding = OnboardingData {
            role: "fullstack".to_string(),
            ai_usage: "full_agentic".to_string(),
            agents: vec!["claude".to_string()],
            email: "test@example.com".to_string(),
        };
        write_at(dir.path(), &s).expect("write");

        let loaded = read_at(dir.path()).expect("read");
        assert_eq!(loaded.onboarding.role, "fullstack");
        assert_eq!(loaded.onboarding.email, "test@example.com");
    }

    // ── Corrupt file falls back to defaults ───────────────────────────────────

    #[test]
    fn corrupt_settings_file_falls_back_to_default() {
        let dir = tmp();
        fs::write(settings_path(dir.path()), "this is not json").expect("write corrupt");

        let loaded = read_at(dir.path()).expect("read");
        // unwrap_or_default() means we get defaults, not an error.
        assert_eq!(loaded.sync_mode, "symlink");
    }

    // ── Getting-started flags ─────────────────────────────────────────────────

    #[test]
    fn getting_started_flags_default_to_false() {
        let s = Settings::default();
        assert!(!s.getting_started.skill_installed);
        assert!(!s.getting_started.template_imported);
    }

    #[test]
    fn getting_started_flags_persist() {
        let dir = tmp();
        let mut s = Settings::default();
        s.getting_started.skill_installed = true;
        write_at(dir.path(), &s).expect("write");

        let loaded = read_at(dir.path()).expect("read");
        assert!(loaded.getting_started.skill_installed);
        assert!(!loaded.getting_started.template_imported);
    }
}
