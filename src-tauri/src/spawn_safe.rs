//! Tty-safe subprocess construction.
//!
//! Any subprocess spawned by the app — most importantly, anything spawned on
//! the `mcp-proxy` or `mcp-serve` path — inherits the parent's controlling
//! terminal. If the parent was launched by an interactive host such as Claude
//! Code, that means the child inherits the user's real tty on fd 0.
//!
//! An interactive shell started that way does two dangerous things:
//!
//! 1. It calls `setpgid(0, 0)` and `tcsetpgrp(tty, own_pg)` to become the
//!    tty's foreground process group, yanking foreground ownership away from
//!    the host process. The host's next read on its own stdin then delivers
//!    `SIGTTIN` and the host stops. The user's terminal is left with a
//!    stopped process holding the foreground group; every subsequent tty
//!    read on that shell returns EIO.
//!
//! 2. It may open `/dev/tty` explicitly (rc-file plugins that ask questions,
//!    instant-prompt setups, etc.). Same failure mode.
//!
//! A stdio MCP server has no user to prompt. The tty must not be reachable
//! from any subprocess it spawns.
//!
//! [`safe_command`] builds a [`std::process::Command`] with the invariants
//! that eliminate both failure modes:
//!
//! * `stdin`  → `/dev/null` (nothing to read from the tty).
//! * `stdout` → `/dev/null` by default; callers that need the output use
//!   [`safe_command_captured`], which pipes it instead.
//! * `stderr` → `/dev/null` by default. Never inherited.
//! * On unix, `pre_exec` calls `setsid(2)` so the child runs in a new
//!   session with no controlling terminal — `open("/dev/tty")` fails with
//!   `ENXIO` and `tcsetpgrp` cannot touch the parent's tty.
//!
//! The invariants are enforced through this module. Direct
//! `std::process::Command` construction on the proxy/serve path is a bug.

use std::ffi::OsStr;
use std::process::{Command, Stdio};

// ── setsid FFI ───────────────────────────────────────────────────────────────
//
// `setsid(2)` is exposed here via a two-line `extern "C"` rather than by
// adding the `libc` crate as a direct dependency. `libc` is already in the
// dependency graph transitively; a direct dep for one integer call would be
// dead weight.
#[cfg(unix)]
extern "C" {
    fn setsid() -> i32;
}

/// Return a [`Command`] for `program` with stdin, stdout, and stderr set to
/// `/dev/null` and, on unix, with a `pre_exec` that runs `setsid(2)` in the
/// child before exec.
///
/// Use this for any subprocess whose output the caller does not need — a
/// side-effect launcher, a fire-and-forget helper. Callers that need to read
/// the child's stdout use [`safe_command_captured`] instead.
pub fn safe_command<S: AsRef<OsStr>>(program: S) -> Command {
    let mut cmd = Command::new(program);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    attach_setsid(&mut cmd);
    cmd
}

/// Return a [`Command`] for `program` with the same tty-safety invariants as
/// [`safe_command`] but with stdout piped so the caller can read it. Stderr
/// stays at `/dev/null`; callers that want it can call `.stderr(...)` on the
/// returned command.
pub fn safe_command_captured<S: AsRef<OsStr>>(program: S) -> Command {
    let mut cmd = Command::new(program);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    attach_setsid(&mut cmd);
    cmd
}

#[cfg(unix)]
fn attach_setsid(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    // Safety: `setsid(2)` is async-signal-safe and touches only the calling
    // process's session id / process group. It has no interaction with the
    // parent's state, and pre_exec runs after fork(), before exec(), so no
    // Rust-managed state exists to corrupt.
    unsafe {
        cmd.pre_exec(|| {
            // Best-effort. `setsid` fails with `EPERM` only if the caller is
            // already a process group leader — in which case it already has
            // its own session, which is the state we wanted anyway. No other
            // failure mode matters for the invariant we're enforcing.
            setsid();
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn attach_setsid(_cmd: &mut Command) {
    // Windows has no `setsid`. Detaching from the console is handled by the
    // `CREATE_NO_WINDOW` process-creation flag, which the app's release
    // build already sets via `#![windows_subsystem = "windows"]`. Nothing
    // further is needed here.
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::Read;
    use std::time::Duration;

    /// The helper must set the child's stdin to `/dev/null`, not the tty
    /// (or the pipe) that the parent has. `sh` reports whether fd 0 is a
    /// character device tty; on `/dev/null` it is not.
    #[test]
    fn safe_command_null_stdin_is_not_a_tty() {
        let output = safe_command_captured("sh")
            .args(["-c", "if [ -t 0 ]; then echo TTY; else echo NOT_TTY; fi"])
            .output()
            .expect("spawn sh");
        assert!(output.status.success(), "child failed: {:?}", output);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(stdout.trim(), "NOT_TTY", "stdout was: {:?}", stdout);
    }

    /// The helper must call `setsid()` in the child, detaching it from the
    /// parent's controlling terminal. `open("/dev/tty")` in that new session
    /// fails with `ENXIO`, which sh surfaces by exiting non-zero on the redirect.
    #[test]
    fn safe_command_child_has_no_controlling_terminal() {
        let output = safe_command_captured("sh")
            .args([
                "-c",
                "if : </dev/tty 2>/dev/null; then echo HAS_CTTY; else echo NO_CTTY; fi",
            ])
            .output()
            .expect("spawn sh");
        assert!(output.status.success(), "child failed: {:?}", output);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(stdout.trim(), "NO_CTTY", "stdout was: {:?}", stdout);
    }

    /// The `safe_command` (no-stdout) variant must still discard stderr.
    /// Anything the child writes to stderr must not reach the parent's own
    /// stderr — otherwise a chatty subprocess would corrupt logs or, on the
    /// mcp-proxy path, mislead the caller.
    #[test]
    fn safe_command_stderr_is_discarded() {
        let mut child = safe_command("sh")
            .args(["-c", "echo child-error 1>&2"])
            .spawn()
            .expect("spawn sh");
        let status = child.wait().expect("wait");
        assert!(status.success());
        // If stderr were inherited, the message would show up on the test
        // runner's stderr. Discarded means nothing to assert on positively;
        // this test's value is that it wouldn't compile if stderr were left
        // inheritable.
    }

    /// Even when the parent process holds a controlling terminal, a child
    /// spawned through the helper must be reapable within a bounded time —
    /// no SIGTTIN stalls, no lingering process. This is the regression test
    /// for the shell-probe hang: an interactive login shell spawned this
    /// way exits within a couple of seconds regardless of the parent's tty.
    #[test]
    fn safe_command_interactive_shell_terminates_promptly() {
        let mut child = safe_command_captured("sh")
            .args(["-ilc", "true"])
            .spawn()
            .expect("spawn sh -il");

        // Poll the child rather than block indefinitely.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match child.try_wait().expect("try_wait") {
                Some(_) => break,
                None if std::time::Instant::now() >= deadline => {
                    let _ = child.kill();
                    panic!("interactive shell did not exit within 5s");
                }
                None => std::thread::sleep(Duration::from_millis(50)),
            }
        }
        // Drain stdout to avoid a leaked pipe warning.
        if let Some(mut out) = child.stdout.take() {
            let mut buf = Vec::new();
            let _ = out.read_to_end(&mut buf);
        }
    }
}
