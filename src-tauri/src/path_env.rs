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
//! # When the shell is slow
//!
//! The startup probe has a short limit so a broken profile cannot hold up
//! launch. A heavy profile (nvm, antigen, pyenv) can pass that limit right
//! after a reboot, when the disk cache is cold and many apps start at once.
//! When that happens, [`fix_path_env`] appends common install folders that
//! exist on disk, and [`reprobe_login_shell_path`] lets a caller that cannot
//! find a binary ask the shell once more with a longer limit.
//!
//! A re-probe calls `std::env::set_var` while other threads are running.
//! Rust's own env reads and `Command` spawns take std's env lock, so they
//! are safe. C code that calls `getenv` without that lock could race it. The
//! window is one `set_var`, once per process, which is accepted here.
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
use std::ffi::{OsStr, OsString};
#[cfg(unix)]
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::sync::Mutex;
#[cfg(unix)]
use std::time::Duration;

/// Limit for the probe at launch, which blocks app startup.
#[cfg(unix)]
const STARTUP_PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// Limit for a re-probe. Callers are already off the UI thread, and waiting
/// longer here beats reporting an installed binary as missing.
#[cfg(unix)]
const REPROBE_TIMEOUT: Duration = Duration::from_secs(15);

/// Markers around the `PATH` in the probe's output. An interactive login
/// shell can print banners before the command and logout text after it, so
/// the `PATH` is taken from between these and nowhere else.
#[cfg(unix)]
const PATH_START: &str = "__AUTOMATIC_PATH_START__";
#[cfg(unix)]
const PATH_END: &str = "__AUTOMATIC_PATH_END__";

/// Whether the shell's `PATH` has been adopted, and whether the one allowed
/// re-probe has been used.
#[cfg(unix)]
#[derive(Debug, Default)]
struct ProbeState {
    adopted: bool,
    reprobe_attempted: bool,
}

/// Guarded by a mutex so concurrent re-probes run one shell, not one each.
#[cfg(unix)]
static PROBE_STATE: Mutex<ProbeState> = Mutex::new(ProbeState {
    adopted: false,
    reprobe_attempted: false,
});

/// Adopt the login shell's `PATH`. If the shell does not answer in time,
/// log why and append common install folders instead.
#[cfg(unix)]
pub fn fix_path_env() {
    let mut state = PROBE_STATE.lock().unwrap_or_else(|e| e.into_inner());
    match probe_login_shell_path(STARTUP_PROBE_TIMEOUT) {
        Ok(path) => {
            std::env::set_var("PATH", path);
            state.adopted = true;
        }
        Err(reason) => {
            eprintln!(
                "[automatic] could not read PATH from the login shell ({}); adding common install folders instead",
                reason
            );
            append_fallback_dirs();
        }
    }
}

#[cfg(not(unix))]
pub fn fix_path_env() {}

/// Ask the login shell for its `PATH` once more, with a longer limit, when
/// the startup probe did not succeed. Callers should look their binary up
/// again afterwards whatever this does: a concurrent caller may have fixed
/// `PATH` while this one waited for the lock.
///
/// Runs the shell at most once per process. A profile that failed twice is
/// unlikely to work on a third try, and each attempt can cost
/// [`REPROBE_TIMEOUT`]. Callers must not be on the UI thread.
#[cfg(unix)]
pub fn reprobe_login_shell_path() {
    // Tests must never replace the test process's PATH or wait on the
    // developer's shell profile.
    if cfg!(test) {
        return;
    }
    let mut state = PROBE_STATE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(path) = reprobe_with(&mut state, || probe_login_shell_path(REPROBE_TIMEOUT)) {
        std::env::set_var("PATH", path);
    }
}

#[cfg(not(unix))]
pub fn reprobe_login_shell_path() {}

/// The re-probe decision, apart from the shell and the environment. Runs
/// `probe` only if neither the startup probe nor an earlier re-probe has
/// been tried to completion, and returns the `PATH` to adopt.
#[cfg(unix)]
fn reprobe_with(
    state: &mut ProbeState,
    probe: impl FnOnce() -> Result<String, String>,
) -> Option<String> {
    if state.adopted || state.reprobe_attempted {
        return None;
    }
    state.reprobe_attempted = true;
    match probe() {
        Ok(path) => {
            state.adopted = true;
            Some(path)
        }
        Err(reason) => {
            eprintln!(
                "[automatic] re-reading PATH from the login shell failed ({})",
                reason
            );
            None
        }
    }
}

/// Probe the user's `$SHELL`, falling back to zsh, the macOS default.
#[cfg(unix)]
fn probe_login_shell_path(timeout: Duration) -> Result<String, String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    probe_shell(&shell, timeout)
}

/// Run `shell` as a login shell, print its `PATH` between markers, and
/// return it, or a short reason the probe failed. Kills the shell if it
/// passes `timeout`.
#[cfg(unix)]
fn probe_shell(shell: &str, timeout: Duration) -> Result<String, String> {
    use std::io::Read;
    use std::time::Instant;

    // The probe uses the tty-safe helper: stdin=/dev/null, stderr=/dev/null,
    // and the child runs in its own session via `setsid`. This is what
    // stops an interactive login shell from grabbing the parent's tty
    // and stalling MCP hosts like Claude Code with `SIGTTIN`.
    let mut cmd = crate::spawn_safe::safe_command_captured(&shell);
    cmd.args([
        "-ilc",
        &format!("printf '{}%s{}' \"$PATH\"", PATH_START, PATH_END),
    ]);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("could not run '{}': {}", shell, e))?;

    // stdout is drained on its own thread so a chatty profile cannot fill
    // the pipe and stall the shell while this thread waits on it. Output
    // arrives in chunks over a channel, never through a join: a background
    // job started by the profile can hold the pipe open long after the
    // shell exits, and nothing here may wait for that job.
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("'{}' has no stdout pipe", shell))?;
    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while let Ok(n) = stdout.read(&mut buf) {
            if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });

    // Bounded wait: a broken shell profile must never block the caller
    // indefinitely. The shell ran under `setsid` with `/dev/null` for
    // stdin/stderr, so nothing it started can touch the host's tty.
    let deadline = Instant::now() + timeout;
    let timed_out = || {
        format!(
            "'{}' did not answer within {} seconds",
            shell,
            timeout.as_secs_f32()
        )
    };
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                kill_process_group(&mut child);
                return Err(timed_out());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(e) => return Err(format!("waiting for '{}' failed: {}", shell, e)),
        }
    };
    if !status.success() {
        return Err(format!("'{}' exited with {}", shell, status));
    }

    // Everything the shell printed is already in the pipe, so this loop
    // normally ends at the first parse. A shell that exits just before the
    // deadline still gets a short grace for the reader thread to pass its
    // output along, rather than a false timeout.
    let drain_deadline = deadline.max(Instant::now() + Duration::from_millis(250));
    let mut output = Vec::new();
    loop {
        if let Some(path) = parse_probe_output(&String::from_utf8_lossy(&output)) {
            return Ok(path);
        }
        let remaining = drain_deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(remaining) {
            Ok(chunk) => output.extend_from_slice(&chunk),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(format!("'{}' printed no PATH", shell))
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => return Err(timed_out()),
        }
    }
}

/// Kill a timed-out probe shell and everything it started, then reap it.
/// The shell called `setsid`, so its pid is also its process group id, and
/// signalling the group stops a hung child such as `nvm` too. std has no
/// `killpg`, so `/bin/kill` sends the group signal; `Child::kill` is the
/// fallback for the shell itself.
#[cfg(unix)]
fn kill_process_group(child: &mut std::process::Child) {
    let group = format!("-{}", child.id());
    let _ = crate::spawn_safe::safe_command("/bin/kill")
        .args(["-KILL", "--", &group])
        .status();
    let _ = child.kill();
    let _ = child.wait();
}

/// Pick the `PATH` out of the probe's stdout: the text between the last
/// start marker and the end marker after it.
#[cfg(unix)]
fn parse_probe_output(stdout: &str) -> Option<String> {
    let start = stdout.rfind(PATH_START)? + PATH_START.len();
    let len = stdout[start..].find(PATH_END)?;
    let path = stdout[start..start + len].trim();
    (!path.is_empty()).then(|| path.to_string())
}

/// Append the fallback folders that exist on disk to this process's `PATH`.
#[cfg(unix)]
fn append_fallback_dirs() {
    let Some(home) = dirs::home_dir() else {
        return;
    };
    let current = std::env::var_os("PATH").unwrap_or_default();
    let candidates = fallback_dirs(&home);
    match path_with_fallbacks(&current, &candidates, |dir| dir.is_dir()) {
        Ok(path) => std::env::set_var("PATH", path),
        Err(e) => eprintln!("[automatic] could not add fallback folders to PATH: {}", e),
    }
}

/// Folders where package managers and Node tools are commonly installed,
/// in the order they should be searched.
#[cfg(unix)]
fn fallback_dirs(home: &Path) -> Vec<PathBuf> {
    vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/opt/homebrew/sbin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/home/linuxbrew/.linuxbrew/bin"),
        home.join("Library/pnpm"),
        home.join(".local/share/pnpm"),
        home.join(".volta/bin"),
        home.join(".bun/bin"),
        home.join(".local/share/mise/shims"),
        home.join(".asdf/shims"),
        home.join(".local/bin"),
    ]
}

/// `current` with each candidate that `exists` and is not already present
/// appended to the end, so anything the inherited `PATH` names still wins.
#[cfg(unix)]
fn path_with_fallbacks(
    current: &OsStr,
    candidates: &[PathBuf],
    exists: impl Fn(&Path) -> bool,
) -> Result<OsString, std::env::JoinPathsError> {
    // An empty `PATH` splits into one empty entry, which would mean "the
    // current directory" once joined.
    let mut dirs: Vec<PathBuf> = std::env::split_paths(current)
        .filter(|dir| !dir.as_os_str().is_empty())
        .collect();
    for candidate in candidates {
        if exists(candidate) && !dirs.contains(candidate) {
            dirs.push(candidate.clone());
        }
    }
    std::env::join_paths(dirs)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn wrapped(path: &str) -> String {
        format!("{}{}{}", PATH_START, path, PATH_END)
    }

    #[test]
    fn parse_probe_output_ignores_banners_and_logout_text() {
        let out = format!(
            "Welcome to antigen\n\n{}Bye from .zlogout\n",
            wrapped("/opt/homebrew/bin:/usr/bin")
        );
        assert_eq!(
            parse_probe_output(&out).as_deref(),
            Some("/opt/homebrew/bin:/usr/bin")
        );
    }

    #[test]
    fn parse_probe_output_uses_the_last_start_marker() {
        let out = format!("{} junk {}", PATH_START, wrapped("/usr/bin"));
        assert_eq!(parse_probe_output(&out).as_deref(), Some("/usr/bin"));
    }

    #[test]
    fn parse_probe_output_rejects_missing_markers_and_empty_path() {
        assert_eq!(parse_probe_output(""), None);
        assert_eq!(parse_probe_output("/usr/bin:/bin"), None);
        assert_eq!(parse_probe_output(PATH_START), None);
        assert_eq!(parse_probe_output(&wrapped("  ")), None);
    }

    #[test]
    fn reprobe_runs_once_and_adopts_the_result() {
        let mut state = ProbeState::default();
        let path = reprobe_with(&mut state, || Ok("/opt/homebrew/bin".into()));
        assert_eq!(path.as_deref(), Some("/opt/homebrew/bin"));
        assert!(state.adopted);
        assert_eq!(
            reprobe_with(&mut state, || panic!("must not probe again")),
            None
        );
    }

    #[test]
    fn reprobe_is_not_retried_after_a_failure() {
        let mut state = ProbeState::default();
        assert_eq!(reprobe_with(&mut state, || Err("timed out".into())), None);
        assert!(!state.adopted);
        assert_eq!(
            reprobe_with(&mut state, || panic!("must not probe again")),
            None
        );
    }

    #[test]
    fn reprobe_is_skipped_once_the_startup_probe_succeeded() {
        let mut state = ProbeState {
            adopted: true,
            reprobe_attempted: false,
        };
        assert_eq!(reprobe_with(&mut state, || panic!("must not probe")), None);
    }

    #[test]
    fn fallbacks_are_appended_only_when_present_and_missing() {
        let current = OsStr::new("/usr/bin:/opt/homebrew/bin");
        let candidates = vec![
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/home/u/Library/pnpm"),
        ];
        let path = path_with_fallbacks(current, &candidates, |dir| {
            dir != Path::new("/usr/local/bin")
        })
        .unwrap();
        assert_eq!(
            path,
            OsString::from("/usr/bin:/opt/homebrew/bin:/home/u/Library/pnpm")
        );
    }

    #[test]
    fn fallbacks_on_an_empty_path() {
        let candidates = vec![PathBuf::from("/opt/homebrew/bin")];
        let path = path_with_fallbacks(OsStr::new(""), &candidates, |_| true).unwrap();
        assert_eq!(path, OsString::from("/opt/homebrew/bin"));
    }

    #[test]
    fn fallback_dirs_include_pnpm_home() {
        let dirs = fallback_dirs(Path::new("/Users/u"));
        assert!(dirs.contains(&PathBuf::from("/Users/u/Library/pnpm")));
        assert!(dirs.contains(&PathBuf::from("/opt/homebrew/bin")));
    }

    #[test]
    fn probe_reads_path_from_a_real_shell() {
        // Uses /bin/sh with a known PATH rather than the developer's own
        // profile, so the result does not depend on the machine.
        let path = probe_shell("/bin/sh", Duration::from_secs(10)).unwrap();
        assert!(path.contains("/usr/bin"), "unexpected PATH: {path}");
    }

    /// Writes an executable script standing in for the user's shell. It
    /// ignores the `-ilc` arguments the probe passes.
    fn fake_shell(dir: &Path, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let script = dir.join("fake-shell");
        std::fs::write(&script, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        script
    }

    fn process_is_alive(pid: &str) -> bool {
        std::process::Command::new("/bin/kill")
            .args(["-0", pid])
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[test]
    fn probe_kills_a_shell_and_its_children_past_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("child.pid");
        let script = fake_shell(
            dir.path(),
            &format!("sleep 30 &\necho $! > '{}'\nwait", pid_file.display()),
        );

        let started = std::time::Instant::now();
        let err = probe_shell(script.to_str().unwrap(), Duration::from_millis(500)).unwrap_err();
        assert!(err.contains("did not answer"), "unexpected error: {err}");
        assert!(started.elapsed() < Duration::from_secs(5));

        let child_pid = std::fs::read_to_string(&pid_file).unwrap();
        let child_pid = child_pid.trim();
        // A SIGKILLed process can linger briefly until it is reaped.
        let gone = (0..40).any(|_| {
            std::thread::sleep(Duration::from_millis(50));
            !process_is_alive(child_pid)
        });
        assert!(gone, "the shell's child {child_pid} survived the timeout");
    }

    #[test]
    fn probe_does_not_wait_for_a_background_job_holding_stdout() {
        let dir = tempfile::tempdir().unwrap();
        let script = fake_shell(
            dir.path(),
            &format!("sleep 30 &\nprintf '{PATH_START}/usr/bin{PATH_END}'"),
        );

        let started = std::time::Instant::now();
        let path = probe_shell(script.to_str().unwrap(), Duration::from_secs(10)).unwrap();
        assert_eq!(path, "/usr/bin");
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
