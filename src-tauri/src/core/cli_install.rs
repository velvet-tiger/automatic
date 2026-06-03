//! Install / uninstall the `automatic` CLI on the user's `$PATH`.
//!
//! The same binary serves the GUI, the MCP server, and the CLI; this module
//! creates a symlink from a directory on `$PATH` to that binary so users can
//! invoke `automatic <verb>` from any shell without bundling a second
//! executable.
//!
//! ## Path selection
//!
//! - **macOS / Linux**: prefer `/usr/local/bin` when it exists and is
//!   writable. Otherwise fall back to `~/.local/bin`, creating it if
//!   necessary. `~/.local/bin` is on the default `$PATH` for most modern
//!   shells via `~/.profile` / `~/.zprofile`; when it is not, the caller
//!   surfaces a hint with `path_hint`.
//! - **Windows**: not implemented in v1. The Settings page reads
//!   `status` and shows manual install instructions.
//!
//! All operations are best-effort and idempotent: installing twice is a
//! no-op; uninstalling a missing or unrelated file is a no-op.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// Symlink basename used everywhere on disk. Single source of truth so
/// install, status, and uninstall stay aligned.
const LINK_NAME: &str = "automatic";

/// Snapshot of the CLI install state, returned to the Settings page.
#[derive(Debug, Serialize)]
pub struct CliInstallStatus {
    /// Operating system family: `"macos" | "linux" | "windows" | "other"`.
    pub platform: String,
    /// Absolute path of the currently running binary.
    pub binary_path: String,
    /// Path where the symlink would be created. `None` on unsupported
    /// platforms (Windows in v1).
    pub install_path: Option<String>,
    /// `"installed"` — symlink exists and resolves to `binary_path`.
    /// `"stale"` — file at `install_path` exists but resolves elsewhere.
    /// `"not_installed"` — nothing at `install_path`.
    /// `"unsupported"` — platform has no automatic install path.
    pub status: String,
    /// Hint shown to the user when `install_path` is on a directory that
    /// may not be on `$PATH` by default (e.g. `~/.local/bin`).
    pub path_hint: Option<String>,
}

/// Inspect the install path without making any changes.
pub fn status() -> Result<CliInstallStatus, String> {
    let binary_path = current_binary_path()?;
    let install_path = preferred_install_path();
    let platform = platform_name().to_string();

    let Some(install_path) = install_path else {
        return Ok(CliInstallStatus {
            platform,
            binary_path: binary_path.display().to_string(),
            install_path: None,
            status: "unsupported".to_string(),
            path_hint: None,
        });
    };

    let install_path_str = install_path.display().to_string();
    let status = if install_path.exists() {
        match std::fs::read_link(&install_path) {
            Ok(target) if paths_equal(&target, &binary_path) => "installed",
            Ok(_) => "stale",
            Err(_) => "stale",
        }
    } else {
        "not_installed"
    };

    Ok(CliInstallStatus {
        platform,
        binary_path: binary_path.display().to_string(),
        install_path: Some(install_path_str),
        status: status.to_string(),
        path_hint: path_hint_for(&install_path),
    })
}

/// Create (or replace) the symlink at the preferred install path. Returns
/// the resolved install path so the caller can show it to the user.
pub fn install() -> Result<String, String> {
    let binary_path = current_binary_path()?;
    let install_path = preferred_install_path()
        .ok_or_else(|| "Automatic CLI install is not supported on this platform".to_string())?;

    if let Some(parent) = install_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
        }
    }

    // Remove any existing entry first so install is idempotent. Only remove
    // symlinks or regular files — never recurse into a directory by accident.
    if install_path.exists() || install_path.symlink_metadata().is_ok() {
        std::fs::remove_file(&install_path).map_err(|e| {
            format!(
                "Failed to remove existing {} before reinstall: {}",
                install_path.display(),
                e
            )
        })?;
    }

    create_symlink(&binary_path, &install_path)?;
    Ok(install_path.display().to_string())
}

/// Remove the symlink iff it points to the current binary. Leaves
/// unrelated files alone so users who manually placed something at the
/// same path do not lose it.
pub fn uninstall() -> Result<String, String> {
    let binary_path = current_binary_path()?;
    let Some(install_path) = preferred_install_path() else {
        return Err("Automatic CLI install is not supported on this platform".to_string());
    };

    if !install_path.exists() && install_path.symlink_metadata().is_err() {
        return Ok(format!("{} was not installed", install_path.display()));
    }

    match std::fs::read_link(&install_path) {
        Ok(target) if paths_equal(&target, &binary_path) => {
            std::fs::remove_file(&install_path)
                .map_err(|e| format!("Failed to remove {}: {}", install_path.display(), e))?;
            Ok(format!("Removed {}", install_path.display()))
        }
        Ok(target) => Err(format!(
            "Refusing to remove {} — points to {} (not the Automatic binary)",
            install_path.display(),
            target.display()
        )),
        Err(_) => Err(format!(
            "Refusing to remove {} — not a symlink",
            install_path.display()
        )),
    }
}

// ── Internals ───────────────────────────────────────────────────────────────

fn current_binary_path() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| format!("Failed to resolve current binary: {}", e))
}

fn platform_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "other"
    }
}

/// Compute the install path for the current platform.
///
/// `/usr/local/bin` is preferred only when it exists *and* is writable
/// without elevation; otherwise we fall back to `~/.local/bin`, which the
/// user can create without sudo.
fn preferred_install_path() -> Option<PathBuf> {
    if cfg!(target_os = "macos") || cfg!(target_os = "linux") {
        let usr_local = PathBuf::from("/usr/local/bin");
        if usr_local.exists() && dir_is_writable(&usr_local) {
            return Some(usr_local.join(LINK_NAME));
        }
        let home = dirs::home_dir()?;
        Some(home.join(".local").join("bin").join(LINK_NAME))
    } else {
        None
    }
}

/// True when the current process can create a file in `dir`. Probes with a
/// short-lived temp file; the result is best-effort and may flip between
/// calls if permissions change.
fn dir_is_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".automatic-cli-write-probe-{}", std::process::id()));
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

fn path_hint_for(install_path: &Path) -> Option<String> {
    let parent = install_path.parent()?;
    if parent.ends_with(".local/bin") {
        Some(
            "If `automatic` is not found in your shell, add `~/.local/bin` to your PATH \
             (e.g. `export PATH=\"$HOME/.local/bin:$PATH\"` in `~/.zprofile` or `~/.profile`)."
                .to_string(),
        )
    } else {
        None
    }
}

#[cfg(unix)]
fn create_symlink(target: &Path, link: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(target, link).map_err(|e| {
        format!(
            "Failed to create symlink {} → {}: {}",
            link.display(),
            target.display(),
            e
        )
    })
}

#[cfg(windows)]
fn create_symlink(_target: &Path, _link: &Path) -> Result<(), String> {
    // Windows symlinks need admin or developer mode. Defer to v2.
    Err("Automatic CLI install is not yet implemented on Windows".to_string())
}

/// Compare two paths after canonicalising each, falling back to a literal
/// comparison if canonicalisation fails (e.g. the target was removed
/// between the symlink read and the comparison).
fn paths_equal(a: &Path, b: &Path) -> bool {
    let canon_a = std::fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let canon_b = std::fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
    canon_a == canon_b
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn paths_equal_handles_symlinks() {
        let tmp = TempDir::new().unwrap();
        let real = tmp.path().join("real");
        std::fs::write(&real, "binary").unwrap();
        let link = tmp.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(paths_equal(&link, &real));
    }

    #[test]
    fn dir_is_writable_true_for_temp() {
        let tmp = TempDir::new().unwrap();
        assert!(dir_is_writable(tmp.path()));
    }
}
