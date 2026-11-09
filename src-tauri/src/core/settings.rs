use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::agents::credential::delete_gateway_token;
use super::agents::GatewayConfig;
use super::custom_gateway::{ClearedCustomGateway, GatewayErase};
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
    /// Set to true once the user dismisses the welcome message on the dashboard.
    #[serde(default)]
    pub welcome_dismissed: bool,
    /// Default per-agent options applied when a new project is created.
    /// Keyed by agent id (e.g. `"claude"`).  Agents absent from this map
    /// use `AgentOptions::default()` when a project is created.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub default_agent_options: HashMap<String, AgentOptions>,
    /// Legacy: the app version at which bundled skills were last written
    /// to disk. Superseded by `library_version`; read-only from Phase 2
    /// onward, retained on the struct so older settings.json files still
    /// round-trip without data loss. `bootstrap::ensure_library_version`
    /// migrates the value into `library_version` on first boot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundled_skills_version: Option<String>,
    /// The content-library version at which bundled assets were last
    /// written to disk. When the current library version (see
    /// `bundled_library::version()`) differs from this value, bundled
    /// assets that still match the last-installed content are overwritten
    /// with the versions shipped in the new library. User edits are
    /// preserved: an on-disk file whose contents diverge from the last
    /// installed version is left alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library_version: Option<String>,
    /// The release version the user last viewed in the "What's New" section.
    /// Used to determine whether a badge/indicator should be shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub whats_new_seen_version: Option<String>,
    /// Master toggle for in-app AI features (Settings > Agents).
    ///
    /// `None` means the toggle has never been set explicitly — the effective
    /// state follows whether any agent API key is stored, preserving the
    /// pre-toggle behaviour for users who upgrade.
    /// `Some(true)` enables in-app AI features; `Some(false)` disables them
    /// even if a key is stored.
    ///
    /// Set automatically by `save_api_key` (to `Some(true)` on the first key
    /// add) and `delete_api_key` (to `Some(false)` when the last key is
    /// removed). User toggles in the UI always write an explicit `Some(...)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_features_enabled: Option<bool>,
    /// The active AI model provider ID (e.g. `"anthropic"`). When absent,
    /// `"anthropic"` is used. Subsequent PRs add additional provider IDs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_agent: Option<String>,
    /// Per-agent Cloudflare AI Gateway configuration, keyed by agent ID
    /// (e.g. `"anthropic"`). Agents absent from this map have no gateway.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub agent_gateways: HashMap<String, GatewayConfig>,
    /// User-selected model for each agent, keyed by agent ID. When absent for
    /// an agent, the hardcoded default from `agents::default_model` is used.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub agent_models: HashMap<String, String>,
    /// When true, Dev Servers run with the nvm-installed Node version named
    /// by the project's `.nvmrc` or `.node-version` (see `node_runtime`).
    /// Off by default: projects then use the Node on the app's `PATH`.
    #[serde(default)]
    pub nvm_enabled: bool,
    /// Where agents may link local folders and files into a context over
    /// MCP. Users can link anything from the app regardless.
    #[serde(default)]
    pub agent_folder_links: AgentFolderLinks,
    /// Where Automatic saves the folders Claude Code may edit for a project
    /// (see `core::claude_dirs`). Change it with `set_writable_folders_target`.
    #[serde(default)]
    pub writable_folders_target: WritableFoldersTarget,
    /// Whether the user agreed that Claude Code's user settings,
    /// `~/.claude/settings.json`, may hold project folders. Entries there
    /// apply to every Claude Code session on the machine, so Automatic writes
    /// none until this is true.
    #[serde(default)]
    pub writable_folders_user_consent: bool,
}

/// Where the folders Claude Code may edit for a project are saved.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WritableFoldersTarget {
    /// The project's own `.claude/settings.local.json` when Claude Code will
    /// honour it there, and user settings otherwise.
    #[default]
    PreferLocal,
    /// Always Claude Code's user settings.
    AlwaysUser,
}

/// Which local folders an agent may link into a context. Agent-added
/// folders are read by any agent the context is attached to, so this bounds
/// what a prompt-injected agent could expose.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentFolderLinks {
    /// Only folders inside a registered project's directory.
    #[default]
    InsideProjects,
    /// Any folder except the home folder, its ancestors, system folders and
    /// known credential folders.
    Anywhere,
    /// Agents cannot link folders.
    None,
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
            welcome_dismissed: false,
            default_agent_options: HashMap::new(),
            bundled_skills_version: None,
            library_version: None,
            whats_new_seen_version: None,
            agent_features_enabled: None,
            active_agent: None,
            agent_gateways: HashMap::new(),
            agent_models: HashMap::new(),
            nvm_enabled: false,
            agent_folder_links: AgentFolderLinks::default(),
            writable_folders_target: WritableFoldersTarget::default(),
            writable_folders_user_consent: false,
        }
    }
}

pub fn read_settings() -> Result<Settings, String> {
    let path = get_automatic_dir()?.join("settings.json");
    if !path.exists() {
        return Ok(Settings::default());
    }
    // Bytes, so a file that is not UTF-8 reads as damaged, like any other
    // unparsable file, rather than as an error.
    let raw = fs::read(&path).map_err(|e| e.to_string())?;
    Ok(serde_json::from_slice(&raw).unwrap_or_default())
}

/// [`read_settings`] for a caller that writes the result back.
///
/// An unparsable file is an error here, not defaults, because writing defaults
/// back would lose every setting. The error gives the position only: the
/// parser's own message can quote a value from the file.
pub fn read_settings_to_update() -> Result<Settings, String> {
    let path = get_automatic_dir()?.join("settings.json");
    if !path.exists() {
        return Ok(Settings::default());
    }
    let raw = fs::read(&path).map_err(|e| e.to_string())?;
    serde_json::from_slice(&raw).map_err(|e| {
        format!(
            "{} could not be parsed (line {}, column {})",
            path.display(),
            e.line(),
            e.column()
        )
    })
}

/// Write settings. A file that cannot be parsed is replaced, so it heals, and a
/// copy stays beside it as `settings.json.damaged` for anyone who wants what
/// was in it.
///
/// Where writable folders go, and the user's agreement to user settings, keep
/// the values on disk: only [`update_settings`] with an explicit edit changes
/// them (see `core::claude_dirs`). A caller holding settings it read earlier
/// therefore cannot bring back an agreement the user has since withdrawn.
pub fn write_settings(settings: &Settings) -> Result<(), String> {
    update_settings(SettingsRead::Lenient, |stored| Settings {
        writable_folders_target: stored.writable_folders_target,
        writable_folders_user_consent: stored.writable_folders_user_consent,
        ..settings.clone()
    })
}

/// Write `settings` whole, including the writable-folders choices. For a reset
/// back to defaults.
fn replace_settings(settings: &Settings) -> Result<(), String> {
    update_settings(SettingsRead::Lenient, |_| settings.clone())
}

/// How [`update_settings`] treats a file that cannot be parsed.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsRead {
    /// Start from defaults, and keep a copy of the damaged file.
    Lenient,
    /// Refuse, because the edit would write defaults over every setting.
    Strict,
}

/// How many times [`update_settings`] starts again after another writer
/// changed the file between its read and its write.
const SETTINGS_WRITE_TRIES: usize = 3;

/// Read the settings, apply `edit`, and write the result, unless the file
/// changed since it was read. Then it reads again and retries, a few times.
pub fn update_settings(read: SettingsRead, edit: impl Fn(Settings) -> Settings) -> Result<(), String> {
    let path = get_automatic_dir()?.join("settings.json");
    for _ in 0..SETTINGS_WRITE_TRIES {
        // Bytes, not text: a file that is not UTF-8 is damaged like any
        // other unparsable file, and must still be replaced.
        let seen = match fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(format!("Could not read {}: {}", path.display(), e)),
        };
        let stored = match seen.as_deref().map(serde_json::from_slice::<Settings>) {
            None => Settings::default(),
            Some(Ok(stored)) => stored,
            Some(Err(e)) if read == SettingsRead::Strict => {
                return Err(format!(
                    "{} could not be parsed (line {}, column {})",
                    path.display(),
                    e.line(),
                    e.column()
                ))
            }
            Some(Err(_)) => Settings::default(),
        };
        keep_damaged_copy(&path)?;
        let raw = serde_json::to_string_pretty(&edit(stored)).map_err(|e| e.to_string())?;
        match write_unless_changed(&path, &raw, seen.as_deref())? {
            Written::Yes => return Ok(()),
            Written::Changed => continue,
        }
    }
    Err(format!(
        "{} kept changing while Automatic was writing it. Try again.",
        path.display()
    ))
}

/// Copy a settings file that cannot be parsed, replacing any earlier copy.
///
/// The copy can hold anything the file held, so it is owner-only. It goes
/// through a temporary file, because a rename replaces a link at the copy's
/// path instead of writing through it.
fn keep_damaged_copy(path: &Path) -> Result<(), String> {
    let Ok(raw) = fs::read(path) else {
        return Ok(());
    };
    if serde_json::from_slice::<Settings>(&raw).is_ok() {
        return Ok(());
    }
    let failed = |e: &dyn std::fmt::Display| {
        format!("Could not keep a copy of the damaged settings file: {}", e)
    };
    let mut copy = path.as_os_str().to_owned();
    copy.push(".damaged");
    let parent = path.parent().ok_or_else(|| failed(&"it has no parent folder"))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| failed(&e))?;
    temp.write_all(&raw).map_err(|e| failed(&e))?;
    temp.as_file().sync_all().map_err(|e| failed(&e))?;
    temp.persist(&copy).map_err(|e| failed(&e.error))?;
    Ok(())
}

/// The file a write really changes. A symlink resolves to its target, so that
/// the link survives the rename, and a link to a file not yet created still
/// creates it, as a plain write would.
fn followed_link(path: &Path) -> Result<PathBuf, String> {
    let is_link = fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink());
    if !is_link {
        return Ok(path.to_path_buf());
    }
    if let Ok(target) = path.canonicalize() {
        return Ok(target);
    }
    let link = fs::read_link(path).map_err(|e| e.to_string())?;
    Ok(path.parent().map_or(link.clone(), |parent| parent.join(&link)))
}

/// Whether [`write_unless_changed`] replaced the file.
enum Written {
    Yes,
    /// The file no longer held what the caller read. Nothing was written.
    Changed,
}

/// [`write_atomically`], unless the file no longer holds `seen` (`None`: no
/// file) just before the rename.
fn write_unless_changed(path: &Path, text: &str, seen: Option<&[u8]>) -> Result<Written, String> {
    let target = followed_link(path)?;
    let temp = prepare_temp(&target, text)?;
    let current = match fs::read(&target) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("Could not read {}: {}", target.display(), e)),
    };
    if current.as_deref() != seen {
        return Ok(Written::Changed);
    }
    temp.persist(&target).map_err(|e| e.error.to_string())?;
    Ok(Written::Yes)
}

/// A temporary file beside `target` holding `text`, with `target`'s
/// permissions when it exists.
fn prepare_temp(target: &Path, text: &str) -> Result<tempfile::NamedTempFile, String> {
    let parent = target.parent().ok_or("The file has no parent folder")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    temp.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    if let Ok(existing) = fs::metadata(target) {
        fs::set_permissions(temp.path(), existing.permissions()).map_err(|e| e.to_string())?;
    }
    Ok(temp)
}

/// Replace a file in one step, so a crash cannot leave half of it. An existing
/// file keeps its permissions. A new file is readable only by its owner.
pub(super) fn write_atomically(path: &Path, text: &str) -> Result<(), String> {
    let target = followed_link(path)?;
    let parent = target.parent().ok_or("The file has no parent folder")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;

    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    temp.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    if let Ok(existing) = fs::metadata(&target) {
        fs::set_permissions(temp.path(), existing.permissions()).map_err(|e| e.to_string())?;
    }
    temp.persist(&target).map_err(|e| e.error.to_string())?;
    Ok(())
}

pub fn reset_settings() -> Result<(), String> {
    delete_all_gateway_tokens()?;
    replace_settings(&Settings::default())
}

/// Remove the secret store token of every configured gateway. Runs before a
/// reset or an erase removes the record of them.
///
/// A recorded token that cannot be removed stops the wipe, and the user can
/// try again. A gateway with no recorded token never does: reset and erase
/// are how a user recovers a broken setup, which can include a store that
/// does not work. The lenient read is deliberate for the same reason. A
/// corrupt file must not block a reset, though its tokens then stay behind.
fn delete_all_gateway_tokens() -> Result<(), String> {
    for (agent_id, gateway) in read_settings()?.agent_gateways {
        match delete_gateway_token(&agent_id) {
            Err(e) if gateway.cf_token_stored => {
                return Err(format!(
                    "could not remove the gateway token for '{}': {}",
                    agent_id, e
                ));
            }
            Err(e) => eprintln!("[automatic] could not check for a gateway token: {}", e),
            Ok(()) => {}
        }
    }
    Ok(())
}

/// Reinstall all bundled defaults (rules, templates, skills, bundled agents,
/// Discover catalogues, and the Automatic MCP server) without touching
/// projects, memories, or app settings.
///
/// Every bundled file is force-overwritten so the on-disk copies match the
/// current binary — equivalent to what happens on a version upgrade but
/// scoped only to the factory-supplied content.
pub fn reinstall_defaults() -> Result<(), String> {
    super::init_discover_files(true)?;
    super::install_default_rules_inner(true)?;
    super::install_default_instructions_inner(true)?;
    super::install_default_skills_inner(true)?;
    super::install_default_subagents_inner(true)?;
    super::ensure_automatic_in_global_mcp()?;
    Ok(())
}

/// Erase Automatic's data. Returns what stayed in Claude Code's settings.
///
/// The gateway comes first, because its record is in the folder being erased.
/// When Claude Code's file cannot be cleared the erase stops, or the token
/// would stay in that file with nothing left that knows about it.
pub fn erase_app_data() -> Result<ClearedCustomGateway, String> {
    let gateway = super::custom_gateway::clear_gateway_for_erase();
    if let GatewayErase::FileNotCleared(error) = gateway {
        return Err(error);
    }
    delete_all_gateway_tokens()?;

    let automatic_dir = get_automatic_dir()?;
    if automatic_dir.exists() {
        fs::remove_dir_all(&automatic_dir).map_err(|e| e.to_string())?;
    }

    // Recreate a clean baseline immediately so the app still has bundled
    // defaults (rules/instruction templates/skill registry metadata) without
    // requiring a restart.
    write_settings(&Settings::default())?;
    super::init_discover_files(false)?;
    super::install_default_rules()?;
    super::install_default_instructions()?;
    super::install_default_skills()?;
    super::install_default_subagents()?;
    // The erase has already happened, so a missing profile must not report
    // it as failed. The next launch tries again.
    if let Err(e) = super::seed_default_profile() {
        eprintln!("[automatic] default profile error: {}", e);
    }

    match gateway {
        GatewayErase::Cleared(cleared) => Ok(cleared),
        GatewayErase::TokenNotDeleted(error) => Err(format!(
            "App data was erased, but the custom gateway token could not be removed from the \
             keychain: {}",
            error
        )),
        GatewayErase::FileNotCleared(error) => Err(error),
    }
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

    // ── Healing and atomic writes ─────────────────────────────────────────────

    fn real_settings_path() -> std::path::PathBuf {
        get_automatic_dir().expect("dir").join("settings.json")
    }

    fn damaged_copy_path() -> std::path::PathBuf {
        get_automatic_dir().expect("dir").join("settings.json.damaged")
    }

    fn damage_settings(text: &str) {
        fs::create_dir_all(get_automatic_dir().expect("dir")).expect("mkdir");
        fs::write(real_settings_path(), text).expect("damage");
    }

    #[test]
    fn a_write_heals_a_damaged_file_and_keeps_a_copy() {
        let _home = crate::core::temp_test_home();
        damage_settings("{\"sync_mode\": \"co");

        let settings = Settings {
            wizard_completed: true,
            ..Settings::default()
        };
        write_settings(&settings).expect("heal");

        assert!(read_settings().expect("read").wizard_completed);
        assert_eq!(fs::read_to_string(damaged_copy_path()).expect("copy"), "{\"sync_mode\": \"co");
    }

    #[test]
    fn a_later_damage_replaces_the_earlier_copy() {
        let _home = crate::core::temp_test_home();
        damage_settings("first damage");
        write_settings(&Settings::default()).expect("heal");
        damage_settings("second damage");
        write_settings(&Settings::default()).expect("heal");

        assert_eq!(fs::read_to_string(damaged_copy_path()).expect("copy"), "second damage");
    }

    #[cfg(unix)]
    #[test]
    fn the_damaged_copy_is_owner_only_and_does_not_follow_a_link() {
        use std::os::unix::fs::PermissionsExt;
        let _home = crate::core::temp_test_home();
        damage_settings("private damage");
        let elsewhere = tmp();
        let outside = elsewhere.path().join("outside.txt");
        fs::write(&outside, "untouched").expect("outside");
        std::os::unix::fs::symlink(&outside, damaged_copy_path()).expect("symlink");

        write_settings(&Settings::default()).expect("heal");

        assert_eq!(fs::read_to_string(&outside).expect("read"), "untouched");
        let copy = fs::symlink_metadata(damaged_copy_path()).expect("meta");
        assert!(copy.file_type().is_file());
        assert_eq!(copy.permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn a_healthy_write_leaves_no_copy() {
        let _home = crate::core::temp_test_home();
        write_settings(&Settings::default()).expect("write");
        write_settings(&Settings::default()).expect("write");

        assert!(!damaged_copy_path().exists());
    }

    #[test]
    fn reset_works_on_a_damaged_file() {
        let _home = crate::core::temp_test_home();
        damage_settings("{not json");

        reset_settings().expect("reset");

        assert_eq!(fs::read_to_string(damaged_copy_path()).expect("copy"), "{not json");
        assert!(serde_json::from_str::<Settings>(
            &fs::read_to_string(real_settings_path()).expect("read")
        )
        .is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn a_write_follows_a_symlinked_file() {
        let _home = crate::core::temp_test_home();
        let elsewhere = tmp();
        let target = elsewhere.path().join("settings.json");
        fs::write(&target, "{\"sync_mode\":\"copy\"}").expect("target");
        fs::create_dir_all(get_automatic_dir().expect("dir")).expect("mkdir");
        std::os::unix::fs::symlink(&target, real_settings_path()).expect("symlink");

        write_settings(&Settings::default()).expect("write");

        let link = fs::symlink_metadata(real_settings_path()).expect("meta");
        assert!(link.file_type().is_symlink());
        assert!(fs::read_to_string(&target).expect("read").contains("\"symlink\""));
    }

    #[cfg(unix)]
    #[test]
    fn a_write_through_a_dangling_link_creates_the_target() {
        let _home = crate::core::temp_test_home();
        let elsewhere = tmp();
        let target = elsewhere.path().join("settings.json");
        fs::create_dir_all(get_automatic_dir().expect("dir")).expect("mkdir");
        std::os::unix::fs::symlink(&target, real_settings_path()).expect("symlink");

        write_settings(&Settings::default()).expect("write");

        assert!(target.exists());
    }

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

    // ── Legacy `bundled_skills_version` round-trip ──────────────────────────

    /// Older settings files carry `bundled_skills_version` (the pre-Phase-2
    /// marker keyed against `CARGO_PKG_VERSION`) and no `library_version`.
    /// They must deserialise cleanly with the legacy value preserved.
    #[test]
    fn legacy_bundled_skills_version_round_trips() {
        let dir = tmp();
        fs::write(
            settings_path(dir.path()),
            r#"{"sync_mode":"symlink","analytics_enabled":true,"bundled_skills_version":"1.19.0"}"#,
        )
        .expect("write legacy settings");
        let loaded = read_at(dir.path()).expect("read legacy");
        assert_eq!(loaded.bundled_skills_version.as_deref(), Some("1.19.0"));
        assert_eq!(loaded.library_version, None);
    }

    #[test]
    fn library_version_round_trips() {
        let dir = tmp();
        let mut s = Settings::default();
        s.library_version = Some("0.1.0".to_string());
        write_at(dir.path(), &s).expect("write");
        let loaded = read_at(dir.path()).expect("read");
        assert_eq!(loaded.library_version.as_deref(), Some("0.1.0"));
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

    // ── Legacy keys ───────────────────────────────────────────────────────────

    /// Settings files written before the Getting Started checklist was removed
    /// still carry a `getting_started` object. They must keep parsing: a parse
    /// failure makes `read_settings` fall back to defaults and lose every value.
    #[test]
    fn legacy_getting_started_key_still_parses() {
        let legacy = r#"{
            "sync_mode": "copy",
            "wizard_completed": true,
            "default_agents": ["claude"],
            "getting_started": { "skill_installed": true, "template_imported": false }
        }"#;

        // Parse directly so a regression reports the serde error itself.
        let parsed: Settings = serde_json::from_str(legacy).expect("legacy settings parse");
        assert_eq!(parsed.sync_mode, "copy");

        // And through the file path, where a parse failure would silently become defaults.
        let dir = tmp();
        fs::write(settings_path(dir.path()), legacy).expect("write legacy settings");
        let loaded = read_at(dir.path()).expect("read");
        assert_eq!(loaded.sync_mode, "copy");
        assert!(loaded.wizard_completed);
        assert_eq!(loaded.default_agents, vec!["claude".to_string()]);
    }

    // ── Agent features toggle ─────────────────────────────────────────────────

    #[test]
    fn agent_features_enabled_defaults_to_none() {
        let s = Settings::default();
        assert_eq!(s.agent_features_enabled, None);
    }

    #[test]
    fn agent_features_enabled_round_trips() {
        let dir = tmp();
        let mut s = Settings::default();
        s.agent_features_enabled = Some(true);
        write_at(dir.path(), &s).expect("write");
        let loaded = read_at(dir.path()).expect("read");
        assert_eq!(loaded.agent_features_enabled, Some(true));

        s.agent_features_enabled = Some(false);
        write_at(dir.path(), &s).expect("write");
        let loaded = read_at(dir.path()).expect("read");
        assert_eq!(loaded.agent_features_enabled, Some(false));
    }

    #[test]
    fn legacy_settings_without_toggle_deserialise_to_none() {
        let dir = tmp();
        // Pre-toggle settings.json that lacks the field entirely.
        let raw = r#"{"sync_mode":"symlink","analytics_enabled":true,"wizard_completed":true}"#;
        fs::write(settings_path(dir.path()), raw).expect("write legacy");

        let loaded = read_at(dir.path()).expect("read");
        assert_eq!(loaded.agent_features_enabled, None);
    }

    #[test]
    fn agent_features_enabled_none_is_omitted_from_serialised_json() {
        let s = Settings::default();
        let raw = serde_json::to_string(&s).expect("serialise");
        // skip_serializing_if = "Option::is_none" should keep legacy files clean.
        assert!(!raw.contains("agent_features_enabled"), "raw was: {raw}");
    }

    #[test]
    fn nvm_enabled_defaults_to_false_for_existing_settings_files() {
        let parsed: Settings = serde_json::from_str(r#"{"sync_mode":"copy"}"#).unwrap();
        assert!(!parsed.nvm_enabled);
        assert_eq!(parsed.sync_mode, "copy");
    }

    #[test]
    fn nvm_enabled_round_trips() {
        let dir = tmp();
        let settings = Settings {
            nvm_enabled: true,
            ..Settings::default()
        };
        write_at(dir.path(), &settings).unwrap();
        assert!(read_at(dir.path()).unwrap().nvm_enabled);
    }

    #[test]
    fn agent_folder_links_defaults_to_inside_projects() {
        let parsed: Settings = serde_json::from_str(r#"{"sync_mode":"copy"}"#).unwrap();
        assert_eq!(parsed.agent_folder_links, AgentFolderLinks::InsideProjects);
    }

    #[test]
    fn agent_folder_links_round_trips() {
        let dir = tmp();
        for value in [AgentFolderLinks::Anywhere, AgentFolderLinks::None] {
            let settings = Settings {
                agent_folder_links: value,
                ..Settings::default()
            };
            write_at(dir.path(), &settings).unwrap();
            assert_eq!(read_at(dir.path()).unwrap().agent_folder_links, value);
        }
        let raw = serde_json::to_string(&Settings::default()).unwrap();
        assert!(raw.contains(r#""agent_folder_links":"inside_projects""#), "{raw}");
    }
}
