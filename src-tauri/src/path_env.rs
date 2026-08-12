//! Fixes up this process's `PATH` so subprocess spawning (npm/pnpm/yarn,
//! editor CLIs, MCP server binaries) can find whatever the user actually has
//! installed.
//!
//! On macOS and Linux, an app launched from Finder/Dock/a desktop launcher
//! does not go through the user's shell profile, so it inherits a minimal
//! system `PATH` (typically just `/usr/bin:/bin:/usr/sbin:/sbin`) — none of
//! nvm, Homebrew, volta, fnm, etc. are on it, even though a terminal in the
//! same account finds them fine. A binary is on disk and detectable with
//! `which` from a terminal, then reported missing from the GUI app, purely
//! because of how it was launched. Windows GUI apps inherit the full
//! registry-configured `PATH` natively, so there's nothing to fix there.
//!
//! The fix (the same one Electron/Tauri apps commonly use, e.g. the
//! `fix-path-env` package): ask the user's own login shell what its `PATH`
//! is, once, and adopt it for the rest of this process's lifetime.

#[cfg(unix)]
pub fn fix_path_env() {
    use std::sync::mpsc;
    use std::time::Duration;

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = std::process::Command::new(&shell)
            .args(["-ilc", "echo -n $PATH"])
            .output();
        // The receiver may already be gone if we timed out — ignore.
        let _ = tx.send(result);
    });

    // Bounded wait: a broken shell profile (e.g. one that hangs waiting on
    // input) must never block app startup indefinitely. On timeout the
    // probe thread is abandoned and the existing PATH is left untouched.
    let Ok(Ok(output)) = rx.recv_timeout(Duration::from_secs(3)) else {
        return;
    };
    if !output.status.success() {
        return;
    }
    let Ok(path) = String::from_utf8(output.stdout) else {
        return;
    };
    let path = path.trim();
    if !path.is_empty() {
        std::env::set_var("PATH", path);
    }
}

#[cfg(not(unix))]
pub fn fix_path_env() {}
