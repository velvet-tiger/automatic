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
//!
//! # Tty safety
//!
//! The probe subprocess is built through [`crate::spawn_safe`], which
//! guarantees stdin is `/dev/null` and — on unix — that the child calls
//! `setsid(2)` before exec. Without those guarantees, an interactive login
//! shell spawned from inside a Claude Code MCP subprocess would inherit
//! Claude's real controlling terminal, call `tcsetpgrp` during its own
//! interactive setup, and take foreground group ownership away from the
//! host process — which then stops on `SIGTTIN` the next time it reads
//! stdin. See [`crate::spawn_safe`] for the full mechanism.

#[cfg(unix)]
pub fn fix_path_env() {
    use std::sync::mpsc;
    use std::time::Duration;

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        // The probe uses the tty-safe helper: stdin=/dev/null, stderr=/dev/null,
        // and the child runs in its own session via `setsid`. This is what
        // stops an interactive login shell from grabbing the parent's tty
        // and stalling MCP hosts like Claude Code with `SIGTTIN`.
        let mut cmd = crate::spawn_safe::safe_command_captured(&shell);
        cmd.args(["-ilc", "echo -n $PATH"]);
        let child = match cmd.spawn() {
            Ok(child) => child,
            Err(_) => {
                let _ = tx.send(Err(()));
                return;
            }
        };
        // `wait_with_output` reaps the child and drains its piped stdout,
        // avoiding the "abandoned process holding fds" problem the previous
        // `.output()` version was prone to when the outer `recv_timeout`
        // fired first.
        let result = child.wait_with_output().map_err(|_| ());
        let _ = tx.send(result);
    });

    // Bounded wait: a broken shell profile must never block app startup
    // indefinitely. On timeout the probe thread's child is left to be
    // reaped by the kernel — but because the child ran under `setsid` and
    // with `/dev/null` for stdin/stderr, it cannot interact with the host's
    // tty regardless of when it finishes.
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
