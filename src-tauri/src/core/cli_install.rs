//! Install / uninstall the `automatic` CLI on the user's `$PATH`.
//!
//! ## Layered approach
//!
//! There are two physically different binaries this module deals with:
//!
//! - The **GUI binary** (`automatic` / `Automatic.exe`) — the same binary
//!   that hosts the Tauri desktop app and the MCP stdio server. On Unix
//!   the GUI binary doubles as the CLI: a symlink from a directory on
//!   `$PATH` to this binary makes `automatic <verb>` work end-to-end.
//!
//! - The **console CLI binary** (`automatic-cli` / `automatic-cli.exe`) —
//!   built from `src/bin/automatic-cli.rs`. On Windows the GUI binary is
//!   linked as a windows-subsystem app so `cmd.exe` and PowerShell do not
//!   wait for it, which means output never flushes synchronously when the
//!   GUI binary is invoked as a CLI. The console binary exists to be the
//!   thing actually placed on `$PATH` in that case; it forwards directly
//!   to `automatic_lib::cli::run` so behaviour matches the GUI binary's
//!   in-process dispatch.
//!
//! ## Install path per platform
//!
//! - **macOS / Linux**: symlink `/usr/local/bin/automatic` → GUI binary
//!   when writable, otherwise `~/.local/bin/automatic`. One file, no
//!   registry mutation.
//! - **Windows**: copy `automatic-cli.exe` into
//!   `%LOCALAPPDATA%\Programs\automatic\bin\automatic.exe` and prepend
//!   that directory to `HKCU\Environment\Path`. No admin required, no
//!   symlinks (which need developer mode on Windows).
//!
//! All operations are idempotent: installing twice is a no-op; uninstalling
//! a missing entry is a no-op.

use serde::Serialize;
use std::path::{Path, PathBuf};

#[cfg(windows)]
const WINDOWS_BIN_DIR_NAME: &str = "automatic";
#[cfg(windows)]
const WINDOWS_CLI_EXE_NAME: &str = "automatic.exe";
#[cfg(windows)]
const WINDOWS_SOURCE_CLI_NAME: &str = "automatic-cli.exe";

#[cfg(unix)]
const UNIX_LINK_NAME: &str = "automatic";

/// Snapshot of the CLI install state, returned to the Settings page.
#[derive(Debug, Serialize)]
pub struct CliInstallStatus {
    /// Operating system family: `"macos" | "linux" | "windows" | "other"`.
    pub platform: String,
    /// Absolute path of the currently running GUI binary. On Windows this
    /// is the file the CLI binary would be copied from a sibling of.
    pub binary_path: String,
    /// Path where the CLI ends up after install. `None` on unsupported
    /// platforms.
    pub install_path: Option<String>,
    /// `"installed"` — install complete and `$PATH` includes the bin dir.
    /// `"stale"` — partial install (file but no PATH entry, or vice versa,
    /// or a symlink that no longer points at this binary).
    /// `"not_installed"` — nothing to find.
    /// `"unsupported"` — platform has no automatic install path.
    pub status: String,
    /// Hint shown to the user when the install location may need extra
    /// `$PATH` configuration (e.g. `~/.local/bin` is not always on PATH).
    pub path_hint: Option<String>,
}

/// Inspect the install state without making any changes.
pub fn status() -> Result<CliInstallStatus, String> {
    #[cfg(unix)]
    {
        unix::status_impl()
    }
    #[cfg(windows)]
    {
        windows::status_impl()
    }
    #[cfg(not(any(unix, windows)))]
    {
        let binary_path = current_binary_path()?;
        Ok(CliInstallStatus {
            platform: platform_name().to_string(),
            binary_path: binary_path.display().to_string(),
            install_path: None,
            status: "unsupported".to_string(),
            path_hint: None,
        })
    }
}

/// Install the CLI. Returns the resolved destination so the caller can
/// show it to the user. Safe to re-run — the call is idempotent.
pub fn install() -> Result<String, String> {
    #[cfg(unix)]
    {
        unix::install_impl()
    }
    #[cfg(windows)]
    {
        windows::install_impl()
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err("Automatic CLI install is not supported on this platform".to_string())
    }
}

/// Reverse the install. Only removes entries this app created — never
/// touches a file that does not look like our install artefact, and never
/// reorders unrelated `$PATH` entries on Windows.
pub fn uninstall() -> Result<String, String> {
    #[cfg(unix)]
    {
        unix::uninstall_impl()
    }
    #[cfg(windows)]
    {
        windows::uninstall_impl()
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err("Automatic CLI install is not supported on this platform".to_string())
    }
}

// ── Shared helpers ──────────────────────────────────────────────────────────

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

/// Compare two paths after canonicalising each, falling back to a literal
/// comparison if canonicalisation fails (e.g. the target was removed
/// between the symlink read and the comparison).
#[cfg(unix)]
fn paths_equal(a: &Path, b: &Path) -> bool {
    let canon_a = std::fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let canon_b = std::fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
    canon_a == canon_b
}

// ── Unix: symlink to GUI binary ─────────────────────────────────────────────

#[cfg(unix)]
mod unix {
    use super::*;

    pub(super) fn status_impl() -> Result<CliInstallStatus, String> {
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

    pub(super) fn install_impl() -> Result<String, String> {
        let binary_path = current_binary_path()?;
        let install_path = preferred_install_path().ok_or_else(|| {
            "Automatic CLI install is not supported on this platform".to_string()
        })?;

        if let Some(parent) = install_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
            }
        }

        if install_path.exists() || install_path.symlink_metadata().is_ok() {
            std::fs::remove_file(&install_path).map_err(|e| {
                format!(
                    "Failed to remove existing {} before reinstall: {}",
                    install_path.display(),
                    e
                )
            })?;
        }

        std::os::unix::fs::symlink(&binary_path, &install_path).map_err(|e| {
            format!(
                "Failed to create symlink {} → {}: {}",
                install_path.display(),
                binary_path.display(),
                e
            )
        })?;

        Ok(install_path.display().to_string())
    }

    pub(super) fn uninstall_impl() -> Result<String, String> {
        let binary_path = current_binary_path()?;
        let install_path = preferred_install_path()
            .ok_or_else(|| "Automatic CLI install is not supported on this platform".to_string())?;

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

    /// `/usr/local/bin` is preferred only when it exists *and* is writable
    /// without elevation; otherwise we fall back to `~/.local/bin`.
    fn preferred_install_path() -> Option<PathBuf> {
        let usr_local = PathBuf::from("/usr/local/bin");
        if usr_local.exists() && dir_is_writable(&usr_local) {
            return Some(usr_local.join(UNIX_LINK_NAME));
        }
        let home = dirs::home_dir()?;
        Some(home.join(".local").join("bin").join(UNIX_LINK_NAME))
    }

    /// True when the current process can create a file in `dir`. Probes
    /// with a short-lived temp file; the result is best-effort and may
    /// flip between calls if permissions change.
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
}

// ── Windows: copy CLI binary + registry PATH update ─────────────────────────

#[cfg(windows)]
mod windows {
    use super::*;

    pub(super) fn status_impl() -> Result<CliInstallStatus, String> {
        let binary_path = current_binary_path()?;
        let bin_dir = bin_dir()?;
        let install_path = bin_dir.join(WINDOWS_CLI_EXE_NAME);

        let file_present = install_path.is_file();
        let on_path = user_path_contains(&bin_dir).unwrap_or(false);
        let status = match (file_present, on_path) {
            (true, true) => "installed",
            (false, false) => "not_installed",
            // Half-installed: useful to surface so the user can reinstall
            // to fix it without us silently mutating either side.
            _ => "stale",
        };

        let path_hint = if status == "installed" {
            Some(
                "Open a new terminal to pick up the updated PATH \
                 — already-running shells use the value from their start."
                    .to_string(),
            )
        } else {
            None
        };

        Ok(CliInstallStatus {
            platform: platform_name().to_string(),
            binary_path: binary_path.display().to_string(),
            install_path: Some(install_path.display().to_string()),
            status: status.to_string(),
            path_hint,
        })
    }

    pub(super) fn install_impl() -> Result<String, String> {
        let bin_dir = bin_dir()?;
        let install_path = bin_dir.join(WINDOWS_CLI_EXE_NAME);
        let source = source_cli_binary()?;

        std::fs::create_dir_all(&bin_dir).map_err(|e| {
            format!("Failed to create {}: {}", bin_dir.display(), e)
        })?;

        // Overwrite is fine — the source is the canonical version that
        // ships with this install of Automatic.
        std::fs::copy(&source, &install_path).map_err(|e| {
            format!(
                "Failed to copy {} → {}: {}",
                source.display(),
                install_path.display(),
                e
            )
        })?;

        add_to_user_path(&bin_dir)?;
        Ok(install_path.display().to_string())
    }

    pub(super) fn uninstall_impl() -> Result<String, String> {
        let bin_dir = bin_dir()?;
        let install_path = bin_dir.join(WINDOWS_CLI_EXE_NAME);

        let mut steps: Vec<String> = Vec::new();
        if install_path.exists() {
            std::fs::remove_file(&install_path).map_err(|e| {
                format!("Failed to remove {}: {}", install_path.display(), e)
            })?;
            steps.push(format!("Removed {}", install_path.display()));
        }

        if user_path_contains(&bin_dir).unwrap_or(false) {
            remove_from_user_path(&bin_dir)?;
            steps.push(format!("Removed {} from PATH", bin_dir.display()));
        }

        // Try to remove the now-empty bin dir; ignore failures.
        let _ = std::fs::remove_dir(&bin_dir);

        if steps.is_empty() {
            Ok(format!("{} was not installed", install_path.display()))
        } else {
            Ok(steps.join("; "))
        }
    }

    /// Per-user install root: `%LOCALAPPDATA%\Programs\automatic\bin\`.
    ///
    /// `%LOCALAPPDATA%` is writable by the current user without admin and
    /// is the standard location for per-user app installs on Windows
    /// (the same location MSIX and many MSI per-user installs use).
    fn bin_dir() -> Result<PathBuf, String> {
        let local = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| "%LOCALAPPDATA% is not set".to_string())?;
        Ok(local.join("Programs").join(WINDOWS_BIN_DIR_NAME).join("bin"))
    }

    /// Find the bundled `automatic-cli.exe`. The Tauri Windows installer
    /// must place this next to the GUI binary; the source path is therefore
    /// `<dir-of-current_exe>\automatic-cli.exe`.
    ///
    /// During `cargo run` of the GUI, the dev profile produces both
    /// binaries side by side in `target/debug/`, so the same lookup works
    /// without bundle wiring.
    fn source_cli_binary() -> Result<PathBuf, String> {
        let exe = current_binary_path()?;
        let dir = exe
            .parent()
            .ok_or_else(|| "Failed to resolve install directory".to_string())?;
        let cli = dir.join(WINDOWS_SOURCE_CLI_NAME);
        if !cli.exists() {
            return Err(format!(
                "CLI binary not found at {} — was Automatic installed with the CLI bundle?",
                cli.display()
            ));
        }
        Ok(cli)
    }

    /// Read `HKCU\Environment\Path` and return true if `dir` is present in
    /// any of the `;`-separated entries (case-insensitive, canonicalised).
    fn user_path_contains(dir: &Path) -> Result<bool, String> {
        let entries = read_user_path_entries()?;
        let target = canonical_lossy(dir);
        Ok(entries
            .into_iter()
            .any(|entry| canonical_lossy(&PathBuf::from(entry)) == target))
    }

    fn add_to_user_path(dir: &Path) -> Result<(), String> {
        let mut entries = read_user_path_entries()?;
        let target = canonical_lossy(dir);
        let already_present = entries
            .iter()
            .any(|entry| canonical_lossy(&PathBuf::from(entry)) == target);
        if already_present {
            return Ok(());
        }
        entries.insert(0, dir.display().to_string());
        write_user_path_entries(&entries)?;
        broadcast_environment_change();
        Ok(())
    }

    fn remove_from_user_path(dir: &Path) -> Result<(), String> {
        let entries = read_user_path_entries()?;
        let target = canonical_lossy(dir);
        let mut changed = false;
        let kept: Vec<String> = entries
            .into_iter()
            .filter(|entry| {
                let drop = canonical_lossy(&PathBuf::from(entry)) == target;
                if drop {
                    changed = true;
                }
                !drop
            })
            .collect();
        if !changed {
            return Ok(());
        }
        write_user_path_entries(&kept)?;
        broadcast_environment_change();
        Ok(())
    }

    fn read_user_path_entries() -> Result<Vec<String>, String> {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let env = match hkcu.open_subkey_with_flags("Environment", KEY_READ) {
            Ok(env) => env,
            Err(e) => return Err(format!("Failed to open HKCU\\Environment: {}", e)),
        };
        let raw: String = env.get_value("Path").unwrap_or_default();
        Ok(raw
            .split(';')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect())
    }

    fn write_user_path_entries(entries: &[String]) -> Result<(), String> {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (env, _) = hkcu
            .create_subkey("Environment")
            .map_err(|e| format!("Failed to open HKCU\\Environment: {}", e))?;
        let joined = entries.join(";");
        env.set_value("Path", &joined)
            .map_err(|e| format!("Failed to write HKCU\\Environment\\Path: {}", e))
    }

    /// Broadcast `WM_SETTINGCHANGE("Environment")` so already-running
    /// shells and Explorer windows pick up the new PATH without requiring
    /// a logout. Failures are silent — the registry write itself is the
    /// source of truth; the broadcast is just a courtesy.
    fn broadcast_environment_change() {
        use windows_sys::Win32::Foundation::{HWND_BROADCAST, LPARAM, WPARAM};
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SendMessageTimeoutW, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
        };
        let param: Vec<u16> = "Environment\0".encode_utf16().collect();
        let mut result: usize = 0;
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0 as WPARAM,
                param.as_ptr() as LPARAM,
                SMTO_ABORTIFHUNG,
                5000,
                &mut result as *mut usize,
            );
        }
    }

    /// Lower-case canonical form used for case-insensitive PATH comparison.
    /// Windows is case-insensitive but the registry preserves the user's
    /// casing, so we normalise before comparing without rewriting the
    /// original entry.
    fn canonical_lossy(path: &Path) -> String {
        path.to_string_lossy().trim_end_matches('\\').to_lowercase()
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

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
}
