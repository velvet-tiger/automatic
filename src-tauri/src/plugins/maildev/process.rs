//! Runtime process management for the Maildev plugin.
//!
//! Unlike Dev Servers, Maildev is a single machine-wide daemon — there is
//! nothing to configure and no per-project scoping, so state here is a
//! single optional slot rather than a registry keyed by config id.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use super::types::MaildevStatus;

/// Cap on captured stderr lines, kept only to surface a diagnostic message
/// (e.g. "EADDRINUSE") when the process exits unexpectedly — never exposed
/// as a paginated log.
const MAX_STDERR_LINES: usize = 20;

struct RunningMaildev {
    child: Child,
    pid: u32,
    started_at: String,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
}

fn slot() -> &'static Mutex<Option<RunningMaildev>> {
    static SLOT: OnceLock<Mutex<Option<RunningMaildev>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// Joins a captured stderr tail into a single diagnostic string. Pure and
/// spawn-free so it can be unit tested without a real process.
fn tail_to_error(tail: &VecDeque<String>) -> Option<String> {
    if tail.is_empty() {
        return None;
    }
    Some(tail.iter().cloned().collect::<Vec<_>>().join("\n"))
}

#[cfg(windows)]
fn build_command() -> Command {
    // The global npm install puts a `.cmd` shim on PATH; `CreateProcess`
    // (unlike cmd.exe's own PATH search) does not probe for that extension.
    // Routing through `cmd /C` gets the same shim resolution cmd.exe itself
    // would apply — mirrors `dev_servers::process::build_command`.
    let mut cmd = Command::new("cmd");
    cmd.args(["/C", "maildev", "--mcp"]);
    cmd
}

#[cfg(not(windows))]
fn build_command() -> Command {
    let mut cmd = Command::new("maildev");
    cmd.arg("--mcp");
    cmd
}

fn spawn_stdout_drain<R: std::io::Read + Send + 'static>(stream: R) {
    // Piped stdout must be drained continuously — once the OS pipe buffer
    // fills, the child blocks on its next write() and appears to hang.
    std::thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines() {
            if line.is_err() {
                break;
            }
        }
    });
}

fn spawn_stderr_drain<R: std::io::Read + Send + 'static>(
    stream: R,
    tail: Arc<Mutex<VecDeque<String>>>,
) {
    std::thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines() {
            let Ok(text) = line else { break };
            let mut buf = tail.lock().unwrap();
            if buf.len() >= MAX_STDERR_LINES {
                buf.pop_front();
            }
            buf.push_back(text);
        }
    });
}

#[cfg(unix)]
fn terminate_and_wait(child: &mut Child, pid: u32) {
    let pgid = pid as i32;
    // Negative pid targets the whole process group.
    let _ = Command::new("kill").args(["-TERM", &format!("-{}", pgid)]).status();
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(200));
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
    }
    let _ = Command::new("kill").args(["-KILL", &format!("-{}", pgid)]).status();
    let _ = child.wait();
}

#[cfg(windows)]
fn terminate_and_wait(child: &mut Child, pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/T", "/F", "/PID", &pid.to_string()])
        .status();
    let _ = child.wait();
}

fn status_from_running(running: &mut RunningMaildev) -> MaildevStatus {
    let exit_status = running.child.try_wait().ok().flatten();
    let is_running = exit_status.is_none();
    MaildevStatus {
        running: is_running,
        pid: if is_running { Some(running.pid) } else { None },
        started_at: Some(running.started_at.clone()),
        exit_code: exit_status.and_then(|s| s.code()),
        error: if is_running {
            None
        } else {
            tail_to_error(&running.stderr_tail.lock().unwrap())
        },
    }
}

/// Start Maildev with `--mcp`. Fails if it is already running, or if the
/// `maildev` binary cannot be found on `$PATH`.
pub fn start() -> Result<MaildevStatus, String> {
    if crate::core::tools::find_binary_on_path("maildev").is_none() {
        return Err("'maildev' was not found on $PATH. Install it with: npm install -g maildev".into());
    }

    let mut guard = slot().lock().unwrap();
    if let Some(running) = guard.as_mut() {
        if matches!(running.child.try_wait(), Ok(None)) {
            return Err("Maildev is already running".into());
        }
    }

    let mut command = build_command();
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // New process group so `stop` can signal the whole tree.
    #[cfg(unix)]
    command.process_group(0);

    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to start Maildev: {}", e))?;
    let pid = child.id();

    let stderr_tail: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
    if let Some(stdout) = child.stdout.take() {
        spawn_stdout_drain(stdout);
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_stderr_drain(stderr, Arc::clone(&stderr_tail));
    }

    let mut running = RunningMaildev {
        child,
        pid,
        started_at: chrono::Utc::now().to_rfc3339(),
        stderr_tail,
    };
    let status = status_from_running(&mut running);
    *guard = Some(running);
    Ok(status)
}

/// Stop Maildev, killing the whole process tree it spawned. A no-op
/// (returning the current, already-stopped status) if it already exited on
/// its own. Errors if it was never started.
pub fn stop() -> Result<MaildevStatus, String> {
    let mut guard = slot().lock().unwrap();
    let running = guard.as_mut().ok_or("Maildev has not been started")?;

    if matches!(running.child.try_wait(), Ok(None)) {
        terminate_and_wait(&mut running.child, running.pid);
    }

    Ok(status_from_running(running))
}

/// Current status. Calling `try_wait` here (rather than trusting a cached
/// flag) is what lets a crash surface as "stopped" on the next poll without
/// a dedicated watcher thread.
pub fn status() -> MaildevStatus {
    let mut guard = slot().lock().unwrap();
    match guard.as_mut() {
        Some(running) => status_from_running(running),
        None => MaildevStatus {
            running: false,
            pid: None,
            started_at: None,
            exit_code: None,
            error: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_to_error_returns_none_when_empty() {
        assert_eq!(tail_to_error(&VecDeque::new()), None);
    }

    #[test]
    fn tail_to_error_joins_lines_in_order() {
        let mut tail = VecDeque::new();
        tail.push_back("Error: listen EADDRINUSE".to_string());
        tail.push_back("    at Server.setupListenHandle".to_string());
        assert_eq!(
            tail_to_error(&tail),
            Some("Error: listen EADDRINUSE\n    at Server.setupListenHandle".to_string())
        );
    }

    /// Exercises the real spawn/kill path end to end — this is the property
    /// the module exists to get right, and not meaningfully testable any
    /// other way. Skips (rather than fails) when `maildev` is not on
    /// `$PATH`, matching `dev_servers::process`'s convention: no mocked
    /// binary is substituted.
    ///
    /// Checks the *specific* pid `start()` returns via `kill -0`, not a
    /// broad `pgrep -f maildev` — a developer machine may already have an
    /// unrelated, independently-started `maildev --mcp` bound to the
    /// default port, and a name-based match would confuse that instance's
    /// continued presence for this test's own process leaking.
    #[cfg(unix)]
    #[test]
    fn start_runs_and_stop_terminates_the_process() {
        if crate::core::tools::find_binary_on_path("maildev").is_none() {
            eprintln!("skipping: maildev not found on $PATH");
            return;
        }

        // Ensure a clean slot regardless of test execution order.
        if status().running {
            stop().expect("stop pre-existing instance");
        }

        let status = start().expect("maildev should start");
        assert!(status.running);
        let pid = status.pid.expect("running status carries a pid");

        let mut found = false;
        for _ in 0..25 {
            if pid_alive(pid) {
                found = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        assert!(found, "spawned maildev process should be alive by pid");

        let stopped = stop().expect("stop should succeed");
        assert!(!stopped.running);
        assert!(!pid_alive(pid), "spawned maildev process should be gone after stop");
    }

    #[cfg(unix)]
    fn pid_alive(pid: u32) -> bool {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}
