//! Writes the custom gateway into the `env` block of Claude Code's user
//! settings file, `~/.claude/settings.json`.
//!
//! That file belongs to the user. Automatic only changes three variables in
//! it, and only values it wrote itself or the user asked it to write. Any
//! other value in those variables stops the write and nothing changes.
//!
//! The merge logic is pure and works on parsed JSON. The file functions at the
//! bottom add the reading and the atomic write.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::agents::GatewayCredentialKind;

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
}

impl ClaudeCodeRecord {
    fn for_env(env: &GatewayEnv) -> Self {
        let token_salt = hex::encode(rand::random::<[u8; 16]>());
        let token_fingerprint = fingerprint(&token_salt, env.token);
        Self {
            base_url: env.base_url.to_string(),
            credential_kind: env.kind,
            token_salt,
            token_fingerprint,
        }
    }

    fn wrote_base_url(&self, value: &Value) -> bool {
        value.as_str() == Some(self.base_url.as_str())
    }

    fn wrote_token(&self, variable: &str, value: &Value) -> bool {
        variable == token_var(self.credential_kind)
            && value
                .as_str()
                .is_some_and(|v| fingerprint(&self.token_salt, v) == self.token_fingerprint)
    }
}

fn fingerprint(salt: &str, token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

// ── Pure merge logic ──────────────────────────────────────────────────────────

/// Parse the file's text into its root object.
fn parse_root(raw: &str, path: &Path) -> Result<Map<String, Value>, String> {
    let value: Value = serde_json::from_str(raw).map_err(|e| {
        format!(
            "{} is not valid JSON (line {}). Fix the file, then try again. Nothing was changed.",
            path.display(),
            e.line()
        )
    })?;
    match value {
        Value::Object(root) => Ok(root),
        _ => Err(format!(
            "{} does not hold a JSON object. Fix the file, then try again. Nothing was changed.",
            path.display()
        )),
    }
}

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
        let automatic_wrote = record.is_some_and(|record| {
            if variable == BASE_URL_VAR {
                record.wrote_base_url(value)
            } else {
                record.wrote_token(variable, value)
            }
        });
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

/// `root` without the values `record` says Automatic wrote, and the names of
/// the variables left in place because they now hold something else.
fn cleared(
    mut root: Map<String, Value>,
    record: &ClaudeCodeRecord,
) -> (Map<String, Value>, Vec<String>) {
    let mut left_alone = Vec::new();
    if let Some(Value::Object(env)) = root.get_mut("env") {
        let token_variable = token_var(record.credential_kind);
        for variable in [BASE_URL_VAR, token_variable] {
            let Some(value) = env.get(variable) else {
                continue;
            };
            let automatic_wrote = if variable == BASE_URL_VAR {
                record.wrote_base_url(value)
            } else {
                record.wrote_token(variable, value)
            };
            if automatic_wrote {
                env.remove(variable);
            } else {
                left_alone.push(variable.to_string());
            }
        }
    }
    (root, left_alone)
}

// ── File access ───────────────────────────────────────────────────────────────

/// Claude Code's user settings file.
pub(super) fn settings_path() -> Result<PathBuf, String> {
    Ok(super::paths::home_dir()?.join(".claude").join("settings.json"))
}

/// The file to read and replace. A symlink resolves to its target, so that a
/// settings file kept in a dotfiles repository stays linked.
fn resolved(path: &Path) -> Result<PathBuf, String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => path.canonicalize().map_err(|e| {
            format!(
                "{} is a link that could not be followed: {}. Nothing was changed.",
                path.display(),
                e
            )
        }),
        _ => Ok(path.to_path_buf()),
    }
}

/// The root object of the file, or `None` when there is no file yet.
fn read_root(target: &Path) -> Result<Option<Map<String, Value>>, String> {
    match fs::read_to_string(target) {
        Ok(raw) => parse_root(&raw, target).map(Some),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!(
            "Could not read {}: {}. Nothing was changed.",
            target.display(),
            e
        )),
    }
}

/// Replace `target` in one step, so that Claude Code never reads half a file.
///
/// The temporary file sits next to the target, because a rename only works
/// within one filesystem. It starts with mode 0600. An existing target's
/// permissions are copied onto it, and a new file keeps 0600.
fn write_root(target: &Path, root: &Map<String, Value>) -> Result<(), String> {
    let failed = |e: &dyn std::fmt::Display| format!("Could not write {}: {}", target.display(), e);

    let parent = target
        .parent()
        .ok_or_else(|| failed(&"the path has no parent folder"))?;
    fs::create_dir_all(parent).map_err(|e| failed(&e))?;

    let mut text = serde_json::to_string_pretty(root).map_err(|e| failed(&e))?;
    text.push('\n');

    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| failed(&e))?;
    temp.write_all(text.as_bytes()).map_err(|e| failed(&e))?;
    temp.as_file().sync_all().map_err(|e| failed(&e))?;
    match fs::metadata(target) {
        Ok(existing) => {
            fs::set_permissions(temp.path(), existing.permissions()).map_err(|e| failed(&e))?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(failed(&e)),
    }
    temp.persist(target).map_err(|e| failed(&e.error))?;
    Ok(())
}

/// Check that a write would be accepted, without changing anything.
pub(super) fn check_file(
    path: &Path,
    desired: &GatewayEnv,
    record: Option<&ClaudeCodeRecord>,
) -> Result<(), String> {
    let target = resolved(path)?;
    let root = read_root(&target)?.unwrap_or_default();
    match env_of(&root, &target)? {
        Some(env) => check_conflicts(env, desired, record, &target),
        None => Ok(()),
    }
}

/// Merge the gateway into the file and return the record of what was written.
///
/// The file is read and checked again here, so a change made since
/// [`check_file`] ran is not overwritten.
pub(super) fn write_file(
    path: &Path,
    desired: &GatewayEnv,
    record: Option<&ClaudeCodeRecord>,
) -> Result<ClaudeCodeRecord, String> {
    check_file(path, desired, record)?;
    let target = resolved(path)?;
    let root = read_root(&target)?.unwrap_or_default();
    write_root(&target, &merged(root, desired))?;
    Ok(ClaudeCodeRecord::for_env(desired))
}

/// Remove what Automatic wrote from the file. Returns the names of the
/// variables left in place because the user changed them.
pub(super) fn clear_file(path: &Path, record: &ClaudeCodeRecord) -> Result<Vec<String>, String> {
    let target = resolved(path)?;
    let Some(root) = read_root(&target)? else {
        return Ok(Vec::new());
    };
    env_of(&root, &target)?;
    let original = root.clone();
    let (root, left_alone) = cleared(root, record);
    if root != original {
        write_root(&target, &root)?;
    }
    Ok(left_alone)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn desired<'a>(kind: GatewayCredentialKind, token: &'a str) -> GatewayEnv<'a> {
        GatewayEnv {
            base_url: "https://gateway.example.com",
            kind,
            token,
        }
    }

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

        write_file(&path, &desired(BEARER, "token"), None).expect("write");

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

        write_file(&path, &desired(BEARER, "token"), None).expect("write");

        let mode = fs::metadata(&path).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn an_existing_file_keeps_its_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = file_in(&dir, "{}");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).expect("chmod");

        write_file(&path, &desired(BEARER, "token"), None).expect("write");

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

        write_file(&path, &desired(BEARER, "token"), None).expect("write");

        let written = json_in(&path);
        assert_eq!(written["env"]["KEEP"], "me");
        assert_eq!(written["permissions"]["allow"][0], "Bash");
        assert_eq!(written["zebra"], 1);
        assert_eq!(written["apple"], true);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_file_stays_a_link_and_the_target_changes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("dotfiles-settings.json");
        fs::write(&target, "{}").expect("write target");
        let link = dir.path().join("settings.json");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");

        write_file(&link, &desired(BEARER, "token"), None).expect("write");

        assert!(fs::symlink_metadata(&link).expect("meta").file_type().is_symlink());
        assert_eq!(json_in(&target)["env"]["ANTHROPIC_AUTH_TOKEN"], "token");
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_link_is_refused() {
        let dir = tempfile::tempdir().expect("tempdir");
        let link = dir.path().join("settings.json");
        std::os::unix::fs::symlink(dir.path().join("gone.json"), &link).expect("symlink");

        let err = write_file(&link, &desired(BEARER, "token"), None).expect_err("refused");

        assert!(err.contains("link"), "{err}");
        assert!(fs::symlink_metadata(&link).expect("meta").file_type().is_symlink());
    }

    #[test]
    fn files_that_cannot_be_merged_are_refused_and_left_as_they_are() {
        let dir = tempfile::tempdir().expect("tempdir");
        for text in ["{not json", "", "[1, 2]", r#"{"env":"text"}"#, r#"{"env":null}"#] {
            let path = file_in(&dir, text);

            let saved = write_file(&path, &desired(BEARER, "token"), None);
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

            let err = write_file(&path, &desired(BEARER, "token"), None).expect_err("refused");

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

        assert!(write_file(&path, &desired(BEARER, "token"), None).is_err());
    }

    #[test]
    fn the_same_values_are_accepted_even_with_no_record() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = file_in(
            &dir,
            r#"{"env":{"ANTHROPIC_BASE_URL":"https://gateway.example.com","ANTHROPIC_AUTH_TOKEN":"token"}}"#,
        );

        // This is the retry after the Automatic settings write failed.
        assert!(write_file(&path, &desired(BEARER, "token"), None).is_ok());
    }

    #[test]
    fn a_token_rotation_replaces_automatics_own_value() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        let first = write_file(&path, &desired(BEARER, "first"), None).expect("first");

        let second = write_file(&path, &desired(BEARER, "second"), Some(&first)).expect("second");
        assert_eq!(json_in(&path)["env"]["ANTHROPIC_AUTH_TOKEN"], "second");

        // The record follows the rotation, so the old token is no longer ours.
        assert!(write_file(&path, &desired(BEARER, "third"), Some(&second)).is_ok());
        let err = write_file(&path, &desired(BEARER, "fourth"), Some(&second)).expect_err("stale");
        assert!(err.contains(AUTH_TOKEN_VAR), "{err}");
    }

    #[test]
    fn a_new_base_url_replaces_automatics_own_value() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        let record = write_file(&path, &desired(BEARER, "token"), None).expect("first");
        let moved = GatewayEnv {
            base_url: "https://other.example.com",
            kind: BEARER,
            token: "token",
        };

        write_file(&path, &moved, Some(&record)).expect("second");

        assert_eq!(json_in(&path)["env"][BASE_URL_VAR], "https://other.example.com");
    }

    #[test]
    fn switching_credential_kind_removes_the_other_variable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        let record = write_file(&path, &desired(BEARER, "token"), None).expect("first");

        write_file(&path, &desired(API_KEY, "token"), Some(&record)).expect("second");

        let env = json_in(&path)["env"].clone();
        assert_eq!(env[API_KEY_VAR], "token");
        assert!(env.get(AUTH_TOKEN_VAR).is_none(), "{env}");
    }

    #[test]
    fn switching_kind_will_not_remove_a_value_the_user_changed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        let record = write_file(&path, &desired(BEARER, "token"), None).expect("first");
        let text = fs::read_to_string(&path)
            .expect("read")
            .replace("\"token\"", "\"theirs\"");
        fs::write(&path, &text).expect("edit");

        let err = write_file(&path, &desired(API_KEY, "token"), Some(&record)).expect_err("refused");

        assert!(err.contains(AUTH_TOKEN_VAR), "{err}");
        assert_eq!(fs::read_to_string(&path).expect("read"), text);
    }

    #[test]
    fn clearing_removes_only_what_automatic_wrote() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = file_in(&dir, r#"{"model":"opus","env":{"KEEP":"me"}}"#);
        let record = write_file(&path, &desired(BEARER, "token"), None).expect("write");

        let left_alone = clear_file(&path, &record).expect("clear");

        assert!(left_alone.is_empty());
        assert_eq!(json_in(&path), json!({ "model": "opus", "env": { "KEEP": "me" } }));
    }

    #[test]
    fn clearing_leaves_a_changed_value_and_says_so() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        let record = write_file(&path, &desired(BEARER, "token"), None).expect("write");
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
        let record = write_file(&path, &desired(BEARER, "token"), None).expect("write");

        clear_file(&path, &record).expect("first");
        assert_eq!(clear_file(&path, &record), Ok(Vec::new()));
    }

    #[test]
    fn clearing_with_no_file_does_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        let record = ClaudeCodeRecord::for_env(&desired(BEARER, "token"));

        assert_eq!(clear_file(&path, &record), Ok(Vec::new()));
        assert!(!path.exists());
    }

    #[test]
    fn clearing_a_malformed_file_is_refused() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = file_in(&dir, "{not json");
        let record = ClaudeCodeRecord::for_env(&desired(BEARER, "token"));

        assert!(clear_file(&path, &record).is_err());
        assert_eq!(fs::read_to_string(&path).expect("read"), "{not json");
    }

    #[test]
    fn the_record_does_not_hold_the_token() {
        let record = ClaudeCodeRecord::for_env(&desired(BEARER, "plain-token"));

        let shown = serde_json::to_string(&record).expect("serialise");

        assert!(!shown.contains("plain-token"), "{shown}");
    }
}
