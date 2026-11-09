//! Writes the custom gateway into the `env` block of Claude Code's user
//! settings file, `~/.claude/settings.json`.
//!
//! That file belongs to the user. Automatic only changes three variables in
//! it, and only values it wrote itself or the user asked it to write. Any
//! other value in those variables stops the write and nothing changes.
//!
//! The merge logic is pure and works on parsed JSON. The file functions at the
//! bottom add the reading, and `claude_settings_file` the atomic write it
//! shares with the writable folders feature.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

use super::agents::GatewayCredentialKind;
use super::claude_settings_file::{
    followed, hold_settings_writes, load, user_settings_path, write_root, LoadedFile, Permissions,
};

pub(super) const BASE_URL_VAR: &str = "ANTHROPIC_BASE_URL";
pub(super) const AUTH_TOKEN_VAR: &str = "ANTHROPIC_AUTH_TOKEN";
pub(super) const API_KEY_VAR: &str = "ANTHROPIC_API_KEY";

/// The variable Claude Code reads the token from for this kind of credential.
fn token_var(kind: GatewayCredentialKind) -> &'static str {
    match kind {
        GatewayCredentialKind::Bearer => AUTH_TOKEN_VAR,
        GatewayCredentialKind::ApiKey => API_KEY_VAR,
    }
}

/// The values Automatic wants in Claude Code's settings.
pub(super) struct GatewayEnv<'a> {
    pub base_url: &'a str,
    pub kind: GatewayCredentialKind,
    pub token: &'a str,
}

/// Whether this save's values are already Automatic's, because the secret
/// store held them before the save began. A failed save leaves them in Claude
/// Code's file with no record, and they must not look like the user's.
#[derive(Clone, Copy, Default)]
pub(super) struct KnownValues {
    pub base_url: bool,
    pub token: bool,
}

/// What Automatic last wrote to Claude Code.
///
/// It holds a salted hash of the token, not the token. That is enough to tell
/// whether the file still holds Automatic's value, and lets a rotation replace
/// the old token even though the keychain has moved on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClaudeCodeRecord {
    base_url: String,
    credential_kind: GatewayCredentialKind,
    token_salt: String,
    token_fingerprint: String,
    /// Variables that already held the wanted value before Automatic wrote.
    /// The user set those, so a clear leaves them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    adopted: Vec<String>,
    /// The settings file the values were written to. `CLAUDE_CONFIG_DIR` can
    /// move Claude Code's settings later, and a clear must still reach this
    /// file. Records from before this field wrote to `~/.claude/settings.json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    settings_path: Option<PathBuf>,
}

impl ClaudeCodeRecord {
    /// The file this record's values were written to.
    fn written_to(&self) -> Result<PathBuf, String> {
        match &self.settings_path {
            Some(path) => Ok(path.clone()),
            None => Ok(super::paths::home_dir()?.join(".claude").join("settings.json")),
        }
    }

    fn for_env(env: &GatewayEnv, adopted: Vec<String>) -> Self {
        let token_salt = hex::encode(rand::random::<[u8; 16]>());
        let token_fingerprint = fingerprint(&token_salt, env.token);
        Self {
            base_url: env.base_url.to_string(),
            credential_kind: env.kind,
            token_salt,
            token_fingerprint,
            adopted,
            settings_path: None,
        }
    }

    /// Whether `value` in `variable` is the one this record describes.
    fn matches(&self, variable: &str, value: &Value) -> bool {
        let Some(value) = value.as_str() else {
            return false;
        };
        if variable == BASE_URL_VAR {
            return value == self.base_url;
        }
        variable == token_var(self.credential_kind)
            && fingerprint(&self.token_salt, value) == self.token_fingerprint
    }

    fn adopted(&self, variable: &str) -> bool {
        self.adopted.iter().any(|name| name == variable)
    }
}

fn fingerprint(salt: &str, token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

// ── Pure merge logic ──────────────────────────────────────────────────────────

/// The `env` object, or `None` when the file has no `env`.
fn env_of<'a>(
    root: &'a Map<String, Value>,
    path: &Path,
) -> Result<Option<&'a Map<String, Value>>, String> {
    match root.get("env") {
        None => Ok(None),
        Some(Value::Object(env)) => Ok(Some(env)),
        Some(_) => Err(format!(
            "The env setting in {} is not a JSON object. Fix the file, then try again. \
             Nothing was changed.",
            path.display()
        )),
    }
}

/// Refuse when a variable holds a value Automatic would overwrite or remove,
/// and which neither Automatic wrote nor the user just asked for.
fn check_conflicts(
    env: &Map<String, Value>,
    desired: &GatewayEnv,
    record: Option<&ClaudeCodeRecord>,
    path: &Path,
) -> Result<(), String> {
    let wanted_var = token_var(desired.kind);
    for variable in [BASE_URL_VAR, AUTH_TOKEN_VAR, API_KEY_VAR] {
        let Some(value) = env.get(variable) else {
            continue;
        };
        let wanted = if variable == BASE_URL_VAR {
            Some(desired.base_url)
        } else if variable == wanted_var {
            Some(desired.token)
        } else {
            None
        };
        let same_as_wanted = wanted.is_some() && value.as_str() == wanted;
        let automatic_wrote = record.is_some_and(|record| record.matches(variable, value));
        if !same_as_wanted && !automatic_wrote {
            // The value is left out on purpose. It may be a token, and a URL
            // can hold a password.
            return Err(format!(
                "{} is already set in {} to a value Automatic did not write. \
                 Remove it from that file, then save again. Nothing was changed.",
                variable,
                path.display()
            ));
        }
    }
    Ok(())
}

/// `root` with the gateway in its `env`. Call [`check_conflicts`] first.
fn merged(mut root: Map<String, Value>, desired: &GatewayEnv) -> Map<String, Value> {
    let env = root
        .entry("env".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if let Value::Object(env) = env {
        let wanted_var = token_var(desired.kind);
        let other_var = if wanted_var == AUTH_TOKEN_VAR {
            API_KEY_VAR
        } else {
            AUTH_TOKEN_VAR
        };
        env.insert(BASE_URL_VAR.to_string(), desired.base_url.into());
        env.insert(wanted_var.to_string(), desired.token.into());
        env.remove(other_var);
    }
    root
}

/// The variables that already hold what is wanted, and that the user set.
fn adopted_variables(
    env: Option<&Map<String, Value>>,
    desired: &GatewayEnv,
    record: Option<&ClaudeCodeRecord>,
    known: KnownValues,
) -> Vec<String> {
    let Some(env) = env else {
        return Vec::new();
    };
    [
        (BASE_URL_VAR, desired.base_url, known.base_url),
        (token_var(desired.kind), desired.token, known.token),
    ]
    .into_iter()
    .filter(|(variable, wanted, known)| {
        let Some(value) = env.get(*variable).filter(|v| v.as_str() == Some(*wanted)) else {
            return false;
        };
        match record.filter(|r| r.matches(variable, value)) {
            Some(record) => record.adopted(variable),
            None => !known,
        }
    })
    .map(|(variable, _, _)| variable.to_string())
    .collect()
}

/// `root` without the gateway values `owned` accepts, and the names of the
/// gateway variables that remain. An `env` this empties is removed too.
fn cleared(
    mut root: Map<String, Value>,
    owned: impl Fn(&str, &Value) -> bool,
) -> (Map<String, Value>, Vec<String>) {
    let mut remaining = Vec::new();
    let mut emptied = false;
    if let Some(Value::Object(env)) = root.get_mut("env") {
        let mut removed = false;
        for variable in [BASE_URL_VAR, AUTH_TOKEN_VAR, API_KEY_VAR] {
            let Some(value) = env.get(variable) else {
                continue;
            };
            if owned(variable, value) {
                env.remove(variable);
                removed = true;
            } else {
                remaining.push(variable.to_string());
            }
        }
        emptied = removed && env.is_empty();
    }
    if emptied {
        root.remove("env");
    }
    (root, remaining)
}

// ── File access ───────────────────────────────────────────────────────────────

/// Claude Code's user settings file.
pub(super) fn settings_path() -> Result<PathBuf, String> {
    user_settings_path()
}

/// The file to write a token to. It must really be inside the settings folder.
/// A link out of it usually points into a dotfiles repository, where the token
/// could be committed.
fn token_target(path: &Path) -> Result<PathBuf, String> {
    let target = followed(path)?;
    if super::paths::claude_config_dir_override().is_some() {
        refuse_git_work_tree(&target)?;
    }
    let folder = path.parent().ok_or("The settings path has no parent folder")?;
    let (Some(home), Some(name), Some(target_folder)) =
        (folder.parent(), folder.file_name(), target.parent())
    else {
        return Err(format!(
            "{} is not a usable settings path. Nothing was changed.",
            path.display()
        ));
    };
    // A folder that does not exist yet cannot be a link. Any other failure
    // means the check cannot be made, so the write does not go ahead.
    let Some(home) = real_folder_if_present(home, path)? else {
        return Ok(target);
    };
    let allowed = home.join(name);
    let Some(real_folder) = real_folder_if_present(target_folder, path)? else {
        return Ok(target);
    };
    if !real_folder.starts_with(&allowed) {
        return Err(format!(
            "{} leads to {}, which is outside {}. Automatic will not write a token there, \
             because it could end up in a tracked dotfiles repository. Nothing was changed.",
            path.display(),
            target.display(),
            allowed.display()
        ));
    }
    Ok(target)
}

/// Refuse a token file inside a git work tree. `CLAUDE_CONFIG_DIR` can name a
/// real folder in a dotfiles repository, which the link check below cannot
/// see. The nearest folder that exists is resolved, then each folder above it
/// is checked for a `.git` entry. Nothing runs git.
fn refuse_git_work_tree(target: &Path) -> Result<(), String> {
    let Some(existing) = target.ancestors().skip(1).find(|folder| folder.exists()) else {
        return Ok(());
    };
    let real = existing.canonicalize().map_err(|e| {
        format!(
            "Could not check where {} leads: {}. Nothing was changed.",
            target.display(),
            e
        )
    })?;
    match real.ancestors().find(|folder| folder.join(".git").exists()) {
        Some(repository) => Err(format!(
            "{} is inside the git repository at {}. Automatic will not write a token \
             there, because it could be committed. Nothing was changed.",
            target.display(),
            repository.display()
        )),
        None => Ok(()),
    }
}

/// The real path of a folder, or `None` when it does not exist.
fn real_folder_if_present(folder: &Path, path: &Path) -> Result<Option<PathBuf>, String> {
    match folder.canonicalize() {
        Ok(real) => Ok(Some(real)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!(
            "Could not check where {} leads: {}. Nothing was changed.",
            path.display(),
            e
        )),
    }
}

/// Load the file and refuse everything a save must not touch.
fn load_for_save(
    path: &Path,
    desired: &GatewayEnv,
    record: Option<&ClaudeCodeRecord>,
) -> Result<(PathBuf, LoadedFile), String> {
    let target = token_target(path)?;
    let loaded = load(&target)?;
    if let Some(env) = env_of(&loaded.root, &target)? {
        check_conflicts(env, desired, record, &target)?;
    }
    Ok((target, loaded))
}

/// Check that a write would be accepted, without changing anything.
pub(super) fn check_file(
    path: &Path,
    desired: &GatewayEnv,
    record: Option<&ClaudeCodeRecord>,
) -> Result<(), String> {
    load_for_save(path, desired, record).map(|_| ())
}

/// Merge the gateway into the file and return the record of what was written.
///
/// The file is read and checked again here, so a change made since
/// [`check_file`] ran is not overwritten.
pub(super) fn write_file(
    path: &Path,
    desired: &GatewayEnv,
    record: Option<&ClaudeCodeRecord>,
    known: KnownValues,
) -> Result<ClaudeCodeRecord, String> {
    // The settings moved since the last save: take Automatic's values out of
    // the old file, or its token would stay there.
    if let Some(record) = record {
        let old = record.written_to()?;
        if old != path {
            clear_owned(&old, record)?;
        }
    }
    let _writes = hold_settings_writes();
    let (target, loaded) = load_for_save(path, desired, record)?;
    let adopted = adopted_variables(env_of(&loaded.root, &target)?, desired, record, known);
    write_root(
        &target,
        &merged(loaded.root, desired),
        loaded.text.as_deref(),
        Permissions::OwnerOnly,
    )?;
    let mut written = ClaudeCodeRecord::for_env(desired, adopted);
    written.settings_path = Some(path.to_path_buf());
    Ok(written)
}

/// Remove what Automatic wrote from the file. Returns the names of the gateway
/// variables that remain there.
///
/// A link out of the folder is followed here. Taking a token out of a file is
/// always safe, and it may have been linked after the save.
/// The file the record was written to is cleared too, when it is not `path`.
pub(super) fn clear_file(path: &Path, record: &ClaudeCodeRecord) -> Result<Vec<String>, String> {
    let mut remaining = Vec::new();
    let old = record.written_to()?;
    if old != path {
        remaining.extend(clear_owned(&old, record)?);
    }
    for variable in clear_owned(path, record)? {
        if !remaining.contains(&variable) {
            remaining.push(variable);
        }
    }
    Ok(remaining)
}

fn clear_owned(path: &Path, record: &ClaudeCodeRecord) -> Result<Vec<String>, String> {
    clear_where(path, |variable, value| {
        record.matches(variable, value) && !record.adopted(variable)
    })
}

/// Remove values that equal the ones given, for when the record is gone.
/// Passing `None` for both changes nothing and reports what is set.
pub(super) fn clear_matching(
    path: &Path,
    base_url: Option<&str>,
    token: Option<&str>,
) -> Result<Vec<String>, String> {
    clear_where(path, |variable, value| {
        let wanted = if variable == BASE_URL_VAR { base_url } else { token };
        wanted.is_some() && value.as_str() == wanted
    })
}

fn clear_where(
    path: &Path,
    owned: impl Fn(&str, &Value) -> bool,
) -> Result<Vec<String>, String> {
    let _writes = hold_settings_writes();
    let target = followed(path)?;
    let loaded = load(&target)?;
    let Some(text) = loaded.text else {
        return Ok(Vec::new());
    };
    env_of(&loaded.root, &target)?;
    let original = loaded.root.clone();
    let (root, remaining) = cleared(loaded.root, owned);
    if root != original {
        write_root(&target, &root, Some(&text), Permissions::Keep)?;
    }
    Ok(remaining)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    fn desired<'a>(kind: GatewayCredentialKind, token: &'a str) -> GatewayEnv<'a> {
        GatewayEnv {
            base_url: "https://gateway.example.com",
            kind,
            token,
        }
    }

    const NONE: KnownValues = KnownValues {
        base_url: false,
        token: false,
    };
    const BEARER: GatewayCredentialKind = GatewayCredentialKind::Bearer;
    const API_KEY: GatewayCredentialKind = GatewayCredentialKind::ApiKey;

    fn file_in(dir: &tempfile::TempDir, text: &str) -> PathBuf {
        let path = dir.path().join("settings.json");
        fs::write(&path, text).expect("write settings");
        path
    }

    fn json_in(path: &Path) -> Value {
        serde_json::from_str(&fs::read_to_string(path).expect("read")).expect("json")
    }

    #[test]
    fn a_new_file_gets_the_gateway() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nested").join("settings.json");

        write_file(&path, &desired(BEARER, "token"), None, NONE).expect("write");

        assert_eq!(
            json_in(&path),
            json!({ "env": {
                "ANTHROPIC_BASE_URL": "https://gateway.example.com",
                "ANTHROPIC_AUTH_TOKEN": "token",
            }})
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_new_file_is_readable_only_by_its_owner() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");

        write_file(&path, &desired(BEARER, "token"), None, NONE).expect("write");

        let mode = fs::metadata(&path).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn an_existing_file_loses_group_and_other_access_when_it_gets_a_token() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = file_in(&dir, "{}");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod");

        write_file(&path, &desired(BEARER, "token"), None, NONE).expect("write");

        let mode = fs::metadata(&path).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn a_clear_leaves_the_mode_alone() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        let record = write_file(&path, &desired(BEARER, "token"), None, NONE).expect("write");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).expect("chmod");

        clear_file(&path, &record).expect("clear");

        let mode = fs::metadata(&path).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, 0o640);
    }

    #[test]
    fn every_other_key_and_env_entry_is_kept() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = file_in(
            &dir,
            r#"{"zebra":1,"permissions":{"allow":["Bash"]},"env":{"KEEP":"me"},"apple":true}"#,
        );

        write_file(&path, &desired(BEARER, "token"), None, NONE).expect("write");

        let written = json_in(&path);
        assert_eq!(written["env"]["KEEP"], "me");
        assert_eq!(written["permissions"]["allow"][0], "Bash");
        assert_eq!(written["zebra"], 1);
        assert_eq!(written["apple"], true);
    }

    /// A home with a `.claude` folder, and the settings path inside it.
    fn claude_folder(dir: &tempfile::TempDir) -> PathBuf {
        let folder = dir.path().join(".claude");
        fs::create_dir_all(&folder).expect("mkdir");
        folder.join("settings.json")
    }

    #[cfg(unix)]
    #[test]
    fn a_link_inside_the_folder_stays_a_link_and_the_target_changes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let link = claude_folder(&dir);
        let target = dir.path().join(".claude").join("real-settings.json");
        fs::write(&target, "{}").expect("write target");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");

        write_file(&link, &desired(BEARER, "token"), None, NONE).expect("write");

        assert!(fs::symlink_metadata(&link).expect("meta").file_type().is_symlink());
        assert_eq!(json_in(&target)["env"]["ANTHROPIC_AUTH_TOKEN"], "token");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_out_of_the_folder_is_refused_by_the_check() {
        let dir = tempfile::tempdir().expect("tempdir");
        let link = claude_folder(&dir);
        let target = dir.path().join("dotfiles-settings.json");
        fs::write(&target, "{}").expect("write target");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");

        let err = check_file(&link, &desired(BEARER, "token"), None).expect_err("refused");

        assert!(err.contains("dotfiles"), "{err}");
        assert!(write_file(&link, &desired(BEARER, "token"), None, NONE).is_err());
        assert_eq!(fs::read_to_string(&target).expect("read"), "{}");
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_folder_outside_the_home_is_refused() {
        let dir = tempfile::tempdir().expect("tempdir");
        let elsewhere = dir.path().join("dotfiles");
        fs::create_dir_all(&elsewhere).expect("mkdir");
        let home = dir.path().join("home");
        fs::create_dir_all(&home).expect("mkdir");
        std::os::unix::fs::symlink(&elsewhere, home.join(".claude")).expect("symlink");
        let path = home.join(".claude").join("settings.json");

        assert!(check_file(&path, &desired(BEARER, "token"), None).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_that_cannot_be_checked_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let locked = dir.path().join("locked");
        fs::create_dir_all(locked.join(".claude")).expect("mkdir");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("chmod");
        let path = locked.join(".claude").join("settings.json");

        let checked = check_file(&path, &desired(BEARER, "token"), None);

        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("chmod");
        assert!(checked.expect_err("refused").contains("Could not check"));
    }

    #[cfg(unix)]
    #[test]
    fn a_clear_follows_a_link_out_of_the_folder() {
        let dir = tempfile::tempdir().expect("tempdir");
        let link = claude_folder(&dir);
        let record = write_file(&link, &desired(BEARER, "token"), None, NONE).expect("write");
        let target = dir.path().join("dotfiles-settings.json");
        fs::rename(&link, &target).expect("move");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");

        clear_file(&link, &record).expect("clear");

        assert_eq!(json_in(&target), json!({}));
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_link_is_refused() {
        let dir = tempfile::tempdir().expect("tempdir");
        let link = dir.path().join("settings.json");
        std::os::unix::fs::symlink(dir.path().join("gone.json"), &link).expect("symlink");

        let err = write_file(&link, &desired(BEARER, "token"), None, NONE).expect_err("refused");

        assert!(err.contains("link"), "{err}");
        assert!(fs::symlink_metadata(&link).expect("meta").file_type().is_symlink());
    }

    #[test]
    fn files_that_cannot_be_merged_are_refused_and_left_as_they_are() {
        let dir = tempfile::tempdir().expect("tempdir");
        for text in ["{not json", "", "[1, 2]", r#"{"env":"text"}"#, r#"{"env":null}"#] {
            let path = file_in(&dir, text);

            let saved = write_file(&path, &desired(BEARER, "token"), None, NONE);
            let checked = check_file(&path, &desired(BEARER, "token"), None);

            assert!(saved.is_err(), "write of {text:?} was accepted");
            assert!(checked.is_err(), "check of {text:?} was accepted");
            assert_eq!(fs::read_to_string(&path).expect("read"), text);
        }
    }

    #[test]
    fn a_value_automatic_did_not_write_is_refused_by_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        for variable in [BASE_URL_VAR, AUTH_TOKEN_VAR, API_KEY_VAR] {
            let text = json!({ "env": { variable: "theirs-secret" } }).to_string();
            let path = file_in(&dir, &text);

            let err = write_file(&path, &desired(BEARER, "token"), None, NONE).expect_err("refused");

            assert!(err.contains(variable), "{err}");
            assert!(err.contains("settings.json"), "{err}");
            assert!(!err.contains("theirs-secret"), "{err}");
            assert_eq!(fs::read_to_string(&path).expect("read"), text);
        }
    }

    #[test]
    fn a_value_that_is_not_text_is_a_conflict() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = file_in(&dir, r#"{"env":{"ANTHROPIC_AUTH_TOKEN":123}}"#);

        assert!(write_file(&path, &desired(BEARER, "token"), None, NONE).is_err());
    }

    #[test]
    fn the_same_values_are_accepted_even_with_no_record() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = file_in(
            &dir,
            r#"{"env":{"ANTHROPIC_BASE_URL":"https://gateway.example.com","ANTHROPIC_AUTH_TOKEN":"token"}}"#,
        );

        // This is the retry after the Automatic settings write failed.
        assert!(write_file(&path, &desired(BEARER, "token"), None, NONE).is_ok());
    }

    #[test]
    fn a_token_rotation_replaces_automatics_own_value() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        let first = write_file(&path, &desired(BEARER, "first"), None, NONE).expect("first");

        let second = write_file(&path, &desired(BEARER, "second"), Some(&first), NONE).expect("second");
        assert_eq!(json_in(&path)["env"]["ANTHROPIC_AUTH_TOKEN"], "second");

        // The record follows the rotation, so the old token is no longer ours.
        assert!(write_file(&path, &desired(BEARER, "third"), Some(&second), NONE).is_ok());
        let err = write_file(&path, &desired(BEARER, "fourth"), Some(&second), NONE).expect_err("stale");
        assert!(err.contains(AUTH_TOKEN_VAR), "{err}");
    }

    #[test]
    fn a_new_base_url_replaces_automatics_own_value() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        let record = write_file(&path, &desired(BEARER, "token"), None, NONE).expect("first");
        let moved = GatewayEnv {
            base_url: "https://other.example.com",
            kind: BEARER,
            token: "token",
        };

        write_file(&path, &moved, Some(&record), NONE).expect("second");

        assert_eq!(json_in(&path)["env"][BASE_URL_VAR], "https://other.example.com");
    }

    #[test]
    fn switching_credential_kind_removes_the_other_variable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        let record = write_file(&path, &desired(BEARER, "token"), None, NONE).expect("first");

        write_file(&path, &desired(API_KEY, "token"), Some(&record), NONE).expect("second");

        let env = json_in(&path)["env"].clone();
        assert_eq!(env[API_KEY_VAR], "token");
        assert!(env.get(AUTH_TOKEN_VAR).is_none(), "{env}");
    }

    #[test]
    fn switching_kind_will_not_remove_a_value_the_user_changed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        let record = write_file(&path, &desired(BEARER, "token"), None, NONE).expect("first");
        let text = fs::read_to_string(&path)
            .expect("read")
            .replace("\"token\"", "\"theirs\"");
        fs::write(&path, &text).expect("edit");

        let err = write_file(&path, &desired(API_KEY, "token"), Some(&record), NONE).expect_err("refused");

        assert!(err.contains(AUTH_TOKEN_VAR), "{err}");
        assert_eq!(fs::read_to_string(&path).expect("read"), text);
    }

    #[test]
    fn clearing_removes_only_what_automatic_wrote() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = file_in(&dir, r#"{"model":"opus","env":{"KEEP":"me"}}"#);
        let record = write_file(&path, &desired(BEARER, "token"), None, NONE).expect("write");

        let left_alone = clear_file(&path, &record).expect("clear");

        assert!(left_alone.is_empty());
        assert_eq!(json_in(&path), json!({ "model": "opus", "env": { "KEEP": "me" } }));
    }

    #[test]
    fn clearing_removes_an_env_it_emptied() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = file_in(&dir, r#"{"model":"opus"}"#);
        let record = write_file(&path, &desired(BEARER, "token"), None, NONE).expect("write");

        clear_file(&path, &record).expect("clear");

        assert_eq!(json_in(&path), json!({ "model": "opus" }));
    }

    #[test]
    fn clearing_keeps_an_env_that_was_already_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = file_in(&dir, r#"{"env":{}}"#);
        let record = ClaudeCodeRecord::for_env(&desired(BEARER, "token"), Vec::new());

        clear_file(&path, &record).expect("clear");

        assert_eq!(json_in(&path), json!({ "env": {} }));
    }

    #[test]
    fn values_the_user_already_set_are_left_by_a_clear_and_reported() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = file_in(
            &dir,
            r#"{"env":{"ANTHROPIC_BASE_URL":"https://gateway.example.com","ANTHROPIC_AUTH_TOKEN":"token"}}"#,
        );
        let first = write_file(&path, &desired(BEARER, "token"), None, NONE).expect("adopt");
        // A second save with the same values must not lose the adoption.
        let record = write_file(&path, &desired(BEARER, "token"), Some(&first), NONE).expect("again");

        let remaining = clear_file(&path, &record).expect("clear");

        assert_eq!(remaining, [BASE_URL_VAR, AUTH_TOKEN_VAR]);
        assert_eq!(json_in(&path)["env"][AUTH_TOKEN_VAR], "token");
    }

    #[test]
    fn values_the_secret_store_already_held_are_not_adopted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = file_in(
            &dir,
            r#"{"env":{"ANTHROPIC_BASE_URL":"https://gateway.example.com","ANTHROPIC_AUTH_TOKEN":"token"}}"#,
        );
        let known = KnownValues {
            base_url: true,
            token: true,
        };

        let record = write_file(&path, &desired(BEARER, "token"), None, known).expect("retry");

        assert!(clear_file(&path, &record).expect("clear").is_empty());
        assert!(json_in(&path).get("env").is_none());
    }

    #[test]
    fn an_adopted_value_stays_adopted_when_the_store_holds_it_too() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = file_in(&dir, r#"{"env":{"ANTHROPIC_AUTH_TOKEN":"token"}}"#);
        let known = KnownValues {
            base_url: true,
            token: true,
        };
        let first = write_file(&path, &desired(BEARER, "token"), None, NONE).expect("adopt");

        let second = write_file(&path, &desired(BEARER, "token"), Some(&first), known).expect("again");

        assert_eq!(clear_file(&path, &second).expect("clear"), [AUTH_TOKEN_VAR]);
    }

    #[test]
    fn a_value_automatic_replaced_is_no_longer_adopted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = file_in(&dir, r#"{"env":{"ANTHROPIC_AUTH_TOKEN":"token"}}"#);
        let first = write_file(&path, &desired(BEARER, "token"), None, NONE).expect("adopt");
        let second = write_file(&path, &desired(BEARER, "rotated"), Some(&first), NONE).expect("rotate");

        let remaining = clear_file(&path, &second).expect("clear");

        assert!(remaining.is_empty(), "{remaining:?}");
    }

    #[test]
    fn a_save_of_automatics_own_values_adopts_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        let first = write_file(&path, &desired(BEARER, "token"), None, NONE).expect("first");
        let second = write_file(&path, &desired(BEARER, "token"), Some(&first), NONE).expect("second");

        assert!(clear_file(&path, &second).expect("clear").is_empty());
    }

    #[test]
    fn clearing_reports_the_other_token_variable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        let record = write_file(&path, &desired(BEARER, "token"), None, NONE).expect("write");
        let text = fs::read_to_string(&path)
            .expect("read")
            .replace(AUTH_TOKEN_VAR, API_KEY_VAR);
        fs::write(&path, text).expect("edit");

        assert_eq!(clear_file(&path, &record).expect("clear"), [API_KEY_VAR]);
    }

    #[test]
    fn clearing_by_value_removes_exact_matches_and_reports_the_rest() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = file_in(
            &dir,
            r#"{"env":{"ANTHROPIC_BASE_URL":"https://theirs.example.com","ANTHROPIC_API_KEY":"token"}}"#,
        );

        let remaining =
            clear_matching(&path, Some("https://gateway.example.com"), Some("token")).expect("clear");

        assert_eq!(remaining, [BASE_URL_VAR]);
        assert!(json_in(&path)["env"].get(API_KEY_VAR).is_none());
    }

    #[test]
    fn clearing_with_no_values_only_reports() {
        let dir = tempfile::tempdir().expect("tempdir");
        let text = r#"{"env":{"ANTHROPIC_AUTH_TOKEN":"token"}}"#;
        let path = file_in(&dir, text);

        assert_eq!(clear_matching(&path, None, None).expect("report"), [AUTH_TOKEN_VAR]);
        assert_eq!(fs::read_to_string(&path).expect("read"), text);
    }

    #[test]
    fn clearing_leaves_a_changed_value_and_says_so() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        let record = write_file(&path, &desired(BEARER, "token"), None, NONE).expect("write");
        let edited = fs::read_to_string(&path)
            .expect("read")
            .replace("\"token\"", "\"theirs\"");
        fs::write(&path, edited).expect("edit");

        let left_alone = clear_file(&path, &record).expect("clear");

        assert_eq!(left_alone, [AUTH_TOKEN_VAR]);
        let env = json_in(&path)["env"].clone();
        assert_eq!(env[AUTH_TOKEN_VAR], "theirs");
        assert!(env.get(BASE_URL_VAR).is_none(), "{env}");
    }

    #[test]
    fn clearing_twice_is_safe() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        let record = write_file(&path, &desired(BEARER, "token"), None, NONE).expect("write");

        clear_file(&path, &record).expect("first");
        assert_eq!(clear_file(&path, &record), Ok(Vec::new()));
    }

    #[test]
    fn clearing_with_no_file_does_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        let record = ClaudeCodeRecord::for_env(&desired(BEARER, "token"), Vec::new());

        assert_eq!(clear_file(&path, &record), Ok(Vec::new()));
        assert!(!path.exists());
    }

    #[test]
    fn clearing_a_malformed_file_is_refused() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = file_in(&dir, "{not json");
        let record = ClaudeCodeRecord::for_env(&desired(BEARER, "token"), Vec::new());

        assert!(clear_file(&path, &record).is_err());
        assert_eq!(fs::read_to_string(&path).expect("read"), "{not json");
    }

    #[test]
    fn the_record_does_not_hold_the_token() {
        let record = ClaudeCodeRecord::for_env(&desired(BEARER, "plain-token"), Vec::new());

        let shown = serde_json::to_string(&record).expect("serialise");

        assert!(!shown.contains("plain-token"), "{shown}");
    }
}

#[cfg(test)]
mod config_dir_tests {
    use super::*;
    use crate::core::paths::{temp_test_home, with_test_claude_config_dir};
    use std::fs;

    fn desired(token: &str) -> GatewayEnv<'_> {
        GatewayEnv {
            base_url: "https://gateway.example.com",
            kind: GatewayCredentialKind::Bearer,
            token,
        }
    }

    const NONE: KnownValues = KnownValues {
        base_url: false,
        token: false,
    };

    #[test]
    fn a_clear_after_claude_config_dir_moved_reaches_the_old_file() {
        let _home = temp_test_home();
        let old = settings_path().expect("path");
        let record = write_file(&old, &desired("secret-token"), None, NONE).expect("save");
        let config = crate::core::paths::home_dir().unwrap().join("config");
        fs::create_dir_all(&config).unwrap();

        with_test_claude_config_dir(config.clone(), || {
            let now = settings_path().expect("path");
            assert_eq!(now, config.join("settings.json"));
            clear_file(&now, &record).expect("clear");
        });

        assert!(!fs::read_to_string(&old).unwrap().contains("secret-token"));
    }

    #[test]
    fn a_token_is_not_written_into_a_config_dir_inside_a_git_repository() {
        let _home = temp_test_home();
        let dotfiles = crate::core::paths::home_dir().unwrap().join("dotfiles");
        fs::create_dir_all(dotfiles.join(".git")).unwrap();
        let config = dotfiles.join("claude");
        fs::create_dir_all(&config).unwrap();

        with_test_claude_config_dir(config.clone(), || {
            let path = settings_path().expect("path");
            let err = check_file(&path, &desired("secret-token"), None).expect_err("refused");
            assert!(err.contains("git repository"), "{err}");
            assert!(write_file(&path, &desired("secret-token"), None, NONE).is_err());
        });
        assert!(!config.join("settings.json").exists());
    }
}
