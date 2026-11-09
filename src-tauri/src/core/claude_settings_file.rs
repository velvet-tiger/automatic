//! Reading and replacing a Claude Code settings file.
//!
//! Two writers change Claude Code's settings: the custom gateway writes the
//! `env` block (`claude_code_env`), and the writable folders feature writes
//! `permissions.additionalDirectories` (`claude_dirs`). Both use this one load
//! and write path, and hold [`hold_settings_writes`] across it, so that one
//! writer in this process never overwrites the other's change.
//!
//! The write re-serialises the whole file. Whitespace changes, and keys come
//! out in sorted order, because this crate builds `serde_json` without its
//! `preserve_order` feature.

use serde_json::{Map, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

static SETTINGS_WRITES: Mutex<()> = Mutex::new(());

/// Held from the read of a settings file to its replacement. Another process
/// can still edit the file in between; [`write_root`] refuses to overwrite
/// that edit.
pub(super) fn hold_settings_writes() -> MutexGuard<'static, ()> {
    // A writer that panicked left no half-written file behind, because every
    // write is a rename. The lock guards no data, so a poisoned one is safe.
    SETTINGS_WRITES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Claude Code's user settings file: `settings.json` in `CLAUDE_CONFIG_DIR`
/// when that is set, and `~/.claude/settings.json` otherwise.
pub(super) fn user_settings_path() -> Result<PathBuf, String> {
    Ok(super::paths::claude_config_dir()?.join("settings.json"))
}

/// Parse the file's text into its root object.
pub(super) fn parse_root(raw: &str, path: &Path) -> Result<Map<String, Value>, String> {
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

/// The file a path really is. A symlink resolves to its target, so that a
/// link stays a link when the file is replaced. A link that leads nowhere is
/// refused.
pub(super) fn followed(path: &Path) -> Result<PathBuf, String> {
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

/// The file as it was read.
pub(super) struct LoadedFile {
    /// The text, or `None` when there is no file yet.
    pub text: Option<String>,
    pub root: Map<String, Value>,
}

/// Read and parse `target`. A missing file loads as an empty object.
pub(super) fn load(target: &Path) -> Result<LoadedFile, String> {
    match fs::read_to_string(target) {
        Ok(text) => {
            let root = parse_root(&text, target)?;
            Ok(LoadedFile {
                text: Some(text),
                root,
            })
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(LoadedFile {
            text: None,
            root: Map::new(),
        }),
        Err(e) => Err(format!(
            "Could not read {}: {}. Nothing was changed.",
            target.display(),
            e
        )),
    }
}

/// Whether the file's permissions change along with its content.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Permissions {
    /// A token stays in the file, so group and other access is removed.
    OwnerOnly,
    /// The file holds no secret Automatic wrote, so the user's choice stands.
    Keep,
}

/// The mode for a file that holds a token.
#[cfg(unix)]
fn owner_only(mode: u32) -> u32 {
    mode & !0o077
}

/// Give the temporary file the permissions the target should end up with.
fn copy_permissions(
    temp: &Path,
    existing: fs::Permissions,
    permissions: Permissions,
) -> std::io::Result<()> {
    #[cfg(unix)]
    if permissions == Permissions::OwnerOnly {
        use std::os::unix::fs::PermissionsExt;
        return fs::set_permissions(
            temp,
            fs::Permissions::from_mode(owner_only(existing.mode())),
        );
    }
    #[cfg(not(unix))]
    let _ = permissions;
    fs::set_permissions(temp, existing)
}

/// Rename the finished temporary file over the target, unless the target has
/// changed since it was read. Claude Code edits this file too, and this
/// narrows the window in which its edit could be lost.
fn replace_unless_changed(
    temp: tempfile::NamedTempFile,
    target: &Path,
    seen: Option<&str>,
) -> Result<(), String> {
    if persist_unless_changed(temp, target, seen)? {
        Ok(())
    } else {
        Err(format!(
            "{} changed while Automatic was writing it. Nothing was changed. Try again.",
            target.display()
        ))
    }
}

/// [`replace_unless_changed`], reporting a change as `false` instead of an
/// error, for a caller that reads again and retries.
fn persist_unless_changed(
    temp: tempfile::NamedTempFile,
    target: &Path,
    seen: Option<&str>,
) -> Result<bool, String> {
    let current = match fs::read_to_string(target) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("Could not read {}: {}", target.display(), e)),
    };
    if current.as_deref() != seen {
        return Ok(false);
    }
    temp.persist(target)
        .map(|_| true)
        .map_err(|e| format!("Could not write {}: {}", target.display(), e.error))
}

/// Replace `target` in one step, so that Claude Code never reads half a file.
/// `seen` is the text the caller read, or `None` when there was no file.
///
/// The temporary file sits next to the target, because a rename only works
/// within one filesystem. It starts with mode 0600, which a new file keeps.
pub(super) fn write_root(
    target: &Path,
    root: &Map<String, Value>,
    seen: Option<&str>,
    permissions: Permissions,
) -> Result<(), String> {
    let failed = |e: &dyn std::fmt::Display| format!("Could not write {}: {}", target.display(), e);
    let mut text = serde_json::to_string_pretty(root).map_err(|e| failed(&e))?;
    text.push('\n');
    let temp = temp_beside(target, &text, permissions)?;
    replace_unless_changed(temp, target, seen)
}

/// Replace `target` with `text` in one step, unless it no longer holds `seen`
/// (`None`: no file). Returns `false`, and writes nothing, when it changed.
pub(super) fn replace_text_unless_changed(
    target: &Path,
    text: &str,
    seen: Option<&str>,
) -> Result<bool, String> {
    let temp = temp_beside(target, text, Permissions::Keep)?;
    persist_unless_changed(temp, target, seen)
}

/// A finished temporary file beside `target` holding `text`, with the
/// permissions `target` should end up with.
fn temp_beside(
    target: &Path,
    text: &str,
    permissions: Permissions,
) -> Result<tempfile::NamedTempFile, String> {
    let failed = |e: &dyn std::fmt::Display| format!("Could not write {}: {}", target.display(), e);
    let parent = target
        .parent()
        .ok_or_else(|| failed(&"the path has no parent folder"))?;
    fs::create_dir_all(parent).map_err(|e| failed(&e))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| failed(&e))?;
    temp.write_all(text.as_bytes()).map_err(|e| failed(&e))?;
    temp.as_file().sync_all().map_err(|e| failed(&e))?;
    match fs::metadata(target) {
        Ok(existing) => {
            copy_permissions(temp.path(), existing.permissions(), permissions)
                .map_err(|e| failed(&e))?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(failed(&e)),
    }
    Ok(temp)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_in(dir: &tempfile::TempDir, text: &str) -> PathBuf {
        let path = dir.path().join("settings.json");
        fs::write(&path, text).expect("write settings");
        path
    }

    #[test]
    fn a_file_changed_after_it_was_read_is_not_overwritten() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = file_in(&dir, "{}");
        let temp = tempfile::NamedTempFile::new_in(dir.path()).expect("temp");
        fs::write(&path, r#"{"theirs":true}"#).expect("concurrent edit");

        let err = replace_unless_changed(temp, &path, Some("{}")).expect_err("changed");

        assert!(err.contains("changed while"), "{err}");
        assert_eq!(fs::read_to_string(&path).expect("read"), r#"{"theirs":true}"#);
    }

    #[test]
    fn a_file_created_after_the_read_is_not_overwritten() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = file_in(&dir, "{}");
        let temp = tempfile::NamedTempFile::new_in(dir.path()).expect("temp");

        assert!(replace_unless_changed(temp, &path, None).is_err());
    }

    #[test]
    fn a_missing_file_loads_as_an_empty_object() {
        let dir = tempfile::tempdir().expect("tempdir");

        let loaded = load(&dir.path().join("settings.json")).expect("load");

        assert!(loaded.text.is_none());
        assert!(loaded.root.is_empty());
    }
}
