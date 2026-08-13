//! Runtime process management for dev servers.
//!
//! State here is in-memory only (one process registry per running app
//! instance) — persisted server *configuration* lives in `registry.rs`.
//! A server that is running always has an entry keyed by its config id; a
//! server that has never been started, or whose most recent run has exited,
//! is represented purely by its persisted config. Entries are kept after
//! exit (not removed) so the captured log remains available for the user to
//! review why a server stopped or crashed — they are only dropped when the
//! server is started again (replacing the entry) or explicitly forgotten
//! via `forget`, which the delete-config command path calls.

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader};
use std::net::IpAddr;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use once_cell::sync::Lazy;
use regex::Regex;

use super::registry;
use super::types::{DevServerStatus, LogLine, LogStream, PackageManager, ServerConfig};

/// Cap on captured log lines per server, so a long-running dev server cannot
/// grow memory usage without bound.
const MAX_LOG_LINES: usize = 1000;

/// Matches ANSI SGR escape sequences (e.g. `\x1b[32m`), which dev server
/// tooling commonly wraps around the URL in its startup banner for coloring.
/// Stripped before URL matching so the trailing escape isn't swallowed into
/// the match.
static ANSI_ESCAPE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\x1b\[[0-9;]*m").expect("ansi escape regex"));

/// Matches an http(s) URL such as the ones dev servers print on startup
/// (`http://localhost:5173/`, `https://192.168.1.5:3000`).
static URL_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"https?://[^\s]+").expect("url regex"));

struct RunningServer {
    project: String,
    /// Snapshot of the config at the moment `start` was called, so a running
    /// server's displayed script/directory/port stays accurate even if the
    /// persisted config is edited afterwards.
    config: ServerConfig,
    child: Child,
    pid: u32,
    started_at: String,
    log: Arc<Mutex<VecDeque<LogLine>>>,
    /// URLs the server has printed to stdout/stderr that point at this
    /// machine, in first-seen order. See `detect_local_urls`.
    urls: Arc<Mutex<Vec<String>>>,
}

/// Extracts URLs from a line of dev-server output, keeping only ones that
/// point at this machine. Dev servers only ever bind to loopback or private
/// addresses, so restricting to those filters out unrelated links a tool
/// might print (e.g. its own docs site) without needing to recognise every
/// framework's specific banner wording.
fn detect_local_urls(line: &str) -> Vec<String> {
    let cleaned = ANSI_ESCAPE_RE.replace_all(line, "");
    URL_RE
        .find_iter(&cleaned)
        .filter_map(|m| normalize_local_url(m.as_str()))
        .collect()
}

/// Parses a matched URL, trims trailing punctuation a line of prose would
/// leave attached (e.g. a closing parenthesis), and returns it with the host
/// rewritten to `localhost` if it was a bind-all address like `0.0.0.0`,
/// which browsers can't navigate to directly. Returns `None` for URLs that
/// don't point at this machine.
fn normalize_local_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim_end_matches([')', ']', '}', ',', '.', '\'', '"', ';']);
    let mut parsed = url::Url::parse(trimmed).ok()?;
    let host = parsed.host_str()?;
    if !is_local_host(host) {
        return None;
    }
    if host == "0.0.0.0" || host == "::" {
        parsed.set_host(Some("localhost")).ok()?;
    }
    Some(parsed.to_string())
}

fn is_local_host(host: &str) -> bool {
    match host {
        "localhost" | "0.0.0.0" | "::" | "::1" => true,
        _ => host
            .parse::<IpAddr>()
            .map(|ip| match ip {
                IpAddr::V4(v4) => v4.is_loopback() || v4.is_private(),
                IpAddr::V6(v6) => v6.is_loopback(),
            })
            .unwrap_or(false),
    }
}

fn processes() -> &'static Mutex<HashMap<String, RunningServer>> {
    static PROCESSES: OnceLock<Mutex<HashMap<String, RunningServer>>> = OnceLock::new();
    PROCESSES.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(windows)]
fn build_command(pm: PackageManager, script: &str) -> Command {
    // `Command::new(pm.binary())` cannot find npm/pnpm/yarn on Windows —
    // they are `.cmd` shims, and `CreateProcess` (unlike cmd.exe's own PATH
    // search) does not probe for that extension. Routing through `cmd /C`
    // gets the same shim resolution cmd.exe itself would apply.
    let mut cmd = Command::new("cmd");
    cmd.args(["/C", pm.binary(), "run", script]);
    cmd
}

#[cfg(not(windows))]
fn build_command(pm: PackageManager, script: &str) -> Command {
    let mut cmd = Command::new(pm.binary());
    cmd.args(["run", script]);
    cmd
}

fn spawn_log_reader<R: std::io::Read + Send + 'static>(
    stream: R,
    kind: LogStream,
    log: Arc<Mutex<VecDeque<LogLine>>>,
    urls: Arc<Mutex<Vec<String>>>,
) {
    // Piped stdout/stderr must be drained continuously — once the OS pipe
    // buffer fills, the child blocks on its next write() and appears to hang.
    std::thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines() {
            let Ok(text) = line else { break };
            for found in detect_local_urls(&text) {
                let mut list = urls.lock().unwrap();
                if !list.contains(&found) {
                    list.push(found);
                }
            }
            let mut buf = log.lock().unwrap();
            if buf.len() >= MAX_LOG_LINES {
                buf.pop_front();
            }
            buf.push_back(LogLine { stream: kind, text });
        }
    });
}

fn status_from_config(project: &str, config: &ServerConfig) -> DevServerStatus {
    DevServerStatus {
        id: config.id.clone(),
        project: project.to_string(),
        name: config.name.clone(),
        package_manager: config.package_manager,
        script: config.script.clone(),
        subdirectory: config.subdirectory.clone(),
        port: config.port,
        running: false,
        pid: None,
        started_at: None,
        exit_code: None,
        urls: Vec::new(),
    }
}

/// Refreshes and returns the status of a tracked entry. Calling `try_wait`
/// here (rather than trusting a cached flag) is what lets a crash detected
/// on the next poll surface as "stopped" without a dedicated watcher thread.
fn status_from_running(id: &str, running: &mut RunningServer) -> DevServerStatus {
    let exit_status = running.child.try_wait().ok().flatten();
    let is_running = exit_status.is_none();
    DevServerStatus {
        id: id.to_string(),
        project: running.project.clone(),
        name: running.config.name.clone(),
        package_manager: running.config.package_manager,
        script: running.config.script.clone(),
        subdirectory: running.config.subdirectory.clone(),
        port: running.config.port,
        running: is_running,
        pid: if is_running { Some(running.pid) } else { None },
        started_at: Some(running.started_at.clone()),
        exit_code: exit_status.and_then(|s| s.code()),
        urls: running.urls.lock().unwrap().clone(),
    }
}

/// Start a configured server. Fails if it is already running, if the
/// package manager binary cannot be found on `$PATH`, or if the resolved
/// working directory does not exist.
pub fn start(project: &str, project_dir: &str, config: &ServerConfig) -> Result<DevServerStatus, String> {
    if project_dir.trim().is_empty() {
        return Err("This project has no directory set".into());
    }

    let working_dir = if config.subdirectory.trim().is_empty() {
        PathBuf::from(project_dir)
    } else {
        PathBuf::from(project_dir).join(&config.subdirectory)
    };
    if !working_dir.is_dir() {
        return Err(format!("Directory '{}' does not exist", working_dir.display()));
    }

    let binary = config.package_manager.binary();
    if crate::core::tools::find_binary_on_path(binary).is_none() {
        return Err(format!("'{}' was not found on $PATH", binary));
    }

    let mut map = processes().lock().unwrap();
    if let Some(existing) = map.get_mut(&config.id) {
        if matches!(existing.child.try_wait(), Ok(None)) {
            return Err(format!("'{}' is already running", config.name));
        }
    }

    let mut command = build_command(config.package_manager, &config.script);
    command
        .current_dir(&working_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // New process group so `stop` can signal the whole tree (npm/pnpm/yarn
    // spawn the actual dev server as a child process, not exec into it).
    #[cfg(unix)]
    command.process_group(0);

    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to start '{}': {}", config.name, e))?;
    let pid = child.id();

    let log: Arc<Mutex<VecDeque<LogLine>>> = Arc::new(Mutex::new(VecDeque::new()));
    let urls: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    if let Some(stdout) = child.stdout.take() {
        spawn_log_reader(stdout, LogStream::Stdout, Arc::clone(&log), Arc::clone(&urls));
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_log_reader(stderr, LogStream::Stderr, Arc::clone(&log), Arc::clone(&urls));
    }

    let mut running = RunningServer {
        project: project.to_string(),
        config: config.clone(),
        child,
        pid,
        started_at: chrono::Utc::now().to_rfc3339(),
        log,
        urls,
    };
    let status = status_from_running(&config.id, &mut running);
    map.insert(config.id.clone(), running);
    Ok(status)
}

#[cfg(unix)]
fn terminate_and_wait(child: &mut Child, pid: u32) {
    let pgid = pid as i32;
    // Negative pid targets the whole process group, reaching children the
    // package manager spawned (the actual dev server), not just its own pid.
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
    // /T kills the process tree rooted at pid (cmd.exe -> npm/pnpm/yarn ->
    // node), /F forces termination without prompting.
    let _ = Command::new("taskkill")
        .args(["/T", "/F", "/PID", &pid.to_string()])
        .status();
    let _ = child.wait();
}

/// Stop a running server, killing the whole process tree it spawned.
/// A no-op (returning the current, already-stopped status) if it already
/// exited on its own. Errors if the server was never started.
///
/// Holds the process-registry lock for the duration of the kill (up to ~2s
/// on Unix if the process ignores SIGTERM before being force-killed) — an
/// acceptable, deliberate tradeoff for a user-initiated, infrequent action.
pub fn stop(id: &str) -> Result<DevServerStatus, String> {
    let mut map = processes().lock().unwrap();
    let running = map
        .get_mut(id)
        .ok_or("This dev server has not been started")?;

    if matches!(running.child.try_wait(), Ok(None)) {
        terminate_and_wait(&mut running.child, running.pid);
    }

    Ok(status_from_running(id, running))
}

/// Statuses for a set of configs known to belong to `project`, merging live
/// process state where a server is (or was) running.
pub fn list_statuses(project: &str, configs: &[ServerConfig]) -> Vec<DevServerStatus> {
    let mut map = processes().lock().unwrap();
    configs
        .iter()
        .map(|config| match map.get_mut(&config.id) {
            Some(running) if running.project == project => status_from_running(&config.id, running),
            _ => status_from_config(project, config),
        })
        .collect()
}

/// Statuses across every project that has at least one dev server
/// configured. Used by the global Tools "Servers" view.
pub fn list_all_statuses() -> Result<Vec<DevServerStatus>, String> {
    let mut all = Vec::new();
    for project in registry::list_projects_with_configs()? {
        let configs = registry::list_configs(&project)?;
        all.extend(list_statuses(&project, &configs));
    }
    Ok(all)
}

/// Captured stdout/stderr lines for a server, oldest first. Empty if the
/// server has never been started this session, or was `forget`-ten.
pub fn get_log(id: &str) -> Vec<LogLine> {
    let map = processes().lock().unwrap();
    match map.get(id) {
        Some(running) => running.log.lock().unwrap().iter().cloned().collect(),
        None => Vec::new(),
    }
}

/// Drop a server's tracked process/log entry. Used when its config is
/// deleted. Refuses while the server is still running.
pub fn forget(id: &str) -> Result<(), String> {
    let mut map = processes().lock().unwrap();
    if let Some(running) = map.get_mut(id) {
        if matches!(running.child.try_wait(), Ok(None)) {
            return Err("This dev server is running — stop it before deleting".into());
        }
    }
    map.remove(id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn detects_vite_style_local_url() {
        let found = detect_local_urls("  \u{1b}[32m➜\u{1b}[39m  Local:   \u{1b}[36mhttp://localhost:5173/\u{1b}[39m");
        assert_eq!(found, vec!["http://localhost:5173/"]);
    }

    #[test]
    fn detects_multiple_urls_on_one_line_and_rewrites_bind_all_host() {
        let found = detect_local_urls("Local: http://0.0.0.0:3000, Network: http://192.168.1.5:3000");
        assert_eq!(found, vec!["http://localhost:3000/", "http://192.168.1.5:3000/"]);
    }

    #[test]
    fn ignores_urls_that_do_not_point_at_this_machine() {
        let found = detect_local_urls("See https://vitejs.dev/guide/ for docs, or https://example.com/api");
        assert!(found.is_empty());
    }

    #[test]
    fn trims_trailing_prose_punctuation() {
        let found = detect_local_urls("Server ready (http://localhost:8080).");
        assert_eq!(found, vec!["http://localhost:8080/"]);
    }

    /// Exercises the real spawn/kill path end to end against actual npm and
    /// node binaries — this is the property the whole module exists to get
    /// right, and it is not meaningfully testable any other way: `stop` must
    /// reach the node process npm forks, not just npm itself. Skips (rather
    /// than fails) when npm/node are not on `$PATH`, since this is the one
    /// test in the suite that depends on the host environment.
    #[cfg(unix)]
    #[test]
    fn start_runs_and_stop_terminates_the_whole_tree() {
        if crate::core::tools::find_binary_on_path("npm").is_none()
            || crate::core::tools::find_binary_on_path("node").is_none()
        {
            eprintln!("skipping: npm/node not found on $PATH");
            return;
        }

        let tmp = TempDir::new().unwrap();
        let marker = "automatic_dev_server_test_marker_18f2";
        let package_json = format!(
            r#"{{"name":"fixture","scripts":{{"dev":"node -e \"/*{marker}*/ setInterval(function(){{}}, 1000)\""}}}}"#
        );
        std::fs::write(tmp.path().join("package.json"), package_json).unwrap();

        let config = ServerConfig {
            id: format!("test-{}", marker),
            name: "test".to_string(),
            package_manager: PackageManager::Npm,
            script: "dev".to_string(),
            subdirectory: String::new(),
            port: None,
            created_at: String::new(),
        };

        let status = start("test-project", tmp.path().to_str().unwrap(), &config)
            .expect("server should start");
        assert!(status.running);

        // npm needs a moment to fork+exec node.
        let mut found = false;
        for _ in 0..25 {
            if pgrep_matches(marker) {
                found = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        assert!(found, "node process spawned by npm should be discoverable via pgrep");

        let stopped = stop(&config.id).expect("stop should succeed");
        assert!(!stopped.running);
        assert!(
            !pgrep_matches(marker),
            "node process should be gone after stop killed the process group"
        );
    }

    /// Exercises the full pipeline from a real child process's stdout through
    /// to `list_statuses`: spawn a node script that prints a Vite-style
    /// banner, then poll status until the reader thread has picked it up.
    /// Skips (rather than fails) when npm/node are not on `$PATH`.
    #[cfg(unix)]
    #[test]
    fn start_captures_a_url_printed_by_the_server() {
        if crate::core::tools::find_binary_on_path("npm").is_none()
            || crate::core::tools::find_binary_on_path("node").is_none()
        {
            eprintln!("skipping: npm/node not found on $PATH");
            return;
        }

        let tmp = TempDir::new().unwrap();
        let marker = "automatic_dev_server_test_marker_url_9c3a";
        // Plain, uncolored output here — ANSI stripping is already covered
        // directly by `detects_vite_style_local_url` above. This test's job
        // is only to prove the real spawn -> capture -> status pipeline.
        let package_json = format!(
            r#"{{"name":"fixture","scripts":{{"dev":"node -e \"/*{marker}*/ console.log('Local: http://localhost:4321/'); setInterval(function(){{}}, 1000)\""}}}}"#
        );
        std::fs::write(tmp.path().join("package.json"), package_json).unwrap();

        let config = ServerConfig {
            id: format!("test-{}", marker),
            name: "test".to_string(),
            package_manager: PackageManager::Npm,
            script: "dev".to_string(),
            subdirectory: String::new(),
            port: None,
            created_at: String::new(),
        };

        start("test-project", tmp.path().to_str().unwrap(), &config).expect("server should start");

        let mut captured = Vec::new();
        for _ in 0..25 {
            let statuses = list_statuses("test-project", std::slice::from_ref(&config));
            if let Some(status) = statuses.first() {
                if !status.urls.is_empty() {
                    captured = status.urls.clone();
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }

        stop(&config.id).expect("stop should succeed");
        assert_eq!(captured, vec!["http://localhost:4321/".to_string()]);
    }

    #[cfg(unix)]
    fn pgrep_matches(marker: &str) -> bool {
        Command::new("pgrep")
            .args(["-f", marker])
            .output()
            .map(|o| !o.stdout.is_empty())
            .unwrap_or(false)
    }
}
