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
use std::ffi::OsString;
use std::io::{BufRead, BufReader};
use std::net::IpAddr;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use once_cell::sync::Lazy;
use regex::Regex;

use super::registry;
use crate::node_runtime::NodeSelection;
use super::types::{DevServerStatus, LogLine, LogStream, PackageManager, ServerConfig};

/// Cap on captured log lines per server, so a long-running dev server cannot
/// grow memory usage without bound.
const MAX_LOG_LINES: usize = 1000;

/// How long `start` watches a new server's output before handing back a
/// "running" status. Long enough for a package manager plus a TypeScript
/// loader to boot and fail a `listen` (well under a second in practice),
/// short enough that a server which prints neither a URL nor a crash does
/// not leave the Start button spinning for long.
const START_GRACE: Duration = Duration::from_secs(4);
const START_POLL: Duration = Duration::from_millis(100);

/// Matches ANSI SGR escape sequences (e.g. `\x1b[32m`), which dev server
/// tooling commonly wraps around the URL in its startup banner for coloring.
/// Stripped before URL matching so the trailing escape isn't swallowed into
/// the match.
static ANSI_ESCAPE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\x1b\[[0-9;]*m").expect("ansi escape regex"));

/// Matches an http(s) URL such as the ones dev servers print on startup
/// (`http://localhost:5173/`, `https://192.168.1.5:3000`).
static URL_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"https?://[^\s]+").expect("url regex"));

/// Matches output lines that mean the server itself died even though its
/// supervisor (`tsx watch`, nodemon, vite, ...) is still alive waiting for a
/// file change — the case `Child::try_wait` alone cannot see. Kept short and
/// specific: each entry is a phrase a crash reliably prints. Bare `EACCES` is
/// deliberately not here because file watchers print it as a non-fatal
/// warning; `listen EACCES` is the fatal form.
static FATAL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)address already in use|EADDRINUSE|listen EACCES|app crashed|Unhandled 'error' event|Emitted 'error' event",
    )
    .expect("fatal pattern regex")
});

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
    /// Most recent output line matching `FATAL_RE`. Held until the server
    /// prints a local URL again, which only happens once it has actually
    /// bound its port. See `record_line`.
    last_error: Arc<Mutex<Option<String>>>,
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

fn is_fatal_line(line: &str) -> bool {
    FATAL_RE.is_match(&ANSI_ESCAPE_RE.replace_all(line, ""))
}

/// True if something on this machine accepts TCP connections on `port`.
/// A connect probe is used rather than a trial bind: std's listener sets
/// SO_REUSEADDR, which lets a trial bind on 127.0.0.1 succeed alongside a
/// wildcard-bound (0.0.0.0) listener, so a bind would miss the common case.
/// A timeout is treated as "free" so a misbehaving firewall cannot block
/// starting servers.
fn port_accepts_connections(port: u16) -> bool {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(200)).is_ok()
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

/// The app's `PATH` with `dir` placed first.
fn path_with_prefix(dir: &std::path::Path) -> Result<OsString, String> {
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    let dirs = std::iter::once(dir.to_path_buf()).chain(std::env::split_paths(&inherited));
    std::env::join_paths(dirs).map_err(|e| format!("Could not add '{}' to PATH: {}", dir.display(), e))
}

fn spawn_log_reader<R: std::io::Read + Send + 'static>(
    stream: R,
    kind: LogStream,
    log: Arc<Mutex<VecDeque<LogLine>>>,
    urls: Arc<Mutex<Vec<String>>>,
    last_error: Arc<Mutex<Option<String>>>,
) {
    // Piped stdout/stderr must be drained continuously — once the OS pipe
    // buffer fills, the child blocks on its next write() and appears to hang.
    std::thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines() {
            let Ok(text) = line else { break };
            record_line(text, kind, &log, &urls, &last_error);
        }
    });
}

/// Folds one line of output into the captured state. A fatal line marks the
/// server as crashed even while the supervisor process is still alive; a
/// later line carrying a local URL clears that, because a server only prints
/// its URL once it has actually bound the port. URLs on a fatal line are not
/// recorded — an "Open" link to a port the server failed to bind is exactly
/// the misleading state this exists to prevent.
fn record_line(
    text: String,
    kind: LogStream,
    log: &Mutex<VecDeque<LogLine>>,
    urls: &Mutex<Vec<String>>,
    last_error: &Mutex<Option<String>>,
) {
    if is_fatal_line(&text) {
        let cleaned = ANSI_ESCAPE_RE.replace_all(&text, "").trim().to_string();
        *last_error.lock().unwrap() = Some(cleaned);
    } else {
        let found = detect_local_urls(&text);
        if !found.is_empty() {
            *last_error.lock().unwrap() = None;
            let mut list = urls.lock().unwrap();
            for url in found {
                if !list.contains(&url) {
                    list.push(url);
                }
            }
        }
    }
    let mut buf = log.lock().unwrap();
    if buf.len() >= MAX_LOG_LINES {
        buf.pop_front();
    }
    buf.push_back(LogLine { stream: kind, text });
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
        last_error: None,
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
        last_error: running.last_error.lock().unwrap().clone(),
    }
}

/// Start a configured server, using the Node chosen in `node` (see
/// `node_runtime`). Fails if it is already running, if the
/// package manager binary cannot be found on `$PATH`, if the resolved
/// working directory does not exist, or if a port is configured and
/// something already answers on it (the server would only die with
/// EADDRINUSE inside its supervisor, which `running` cannot see).
///
/// Also fails, after killing the tree, if the server prints a crash line
/// within `START_GRACE` of spawning. Blocks for up to that long when the
/// server prints neither a crash nor a URL, so callers must not be on the
/// UI thread.
pub fn start(
    project: &str,
    project_dir: &str,
    config: &ServerConfig,
    node: &NodeSelection,
) -> Result<DevServerStatus, String> {
    start_with_grace(project, project_dir, config, node, START_GRACE)
}

/// `start` with an explicit watch window, so tests can exercise the
/// early-crash and late-crash paths without waiting the full default.
fn start_with_grace(
    project: &str,
    project_dir: &str,
    config: &ServerConfig,
    node: &NodeSelection,
    grace: Duration,
) -> Result<DevServerStatus, String> {
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
    let nvm_bin = match node {
        NodeSelection::Nvm(selected) => Some(selected.bin_dir.as_path()),
        NodeSelection::NotRequested | NodeSelection::System(_) => None,
    };
    let in_nvm_bin = nvm_bin.is_some_and(|dir| dir.join(binary).is_file());
    if !in_nvm_bin && crate::core::tools::find_binary_on_path(binary).is_none() {
        return Err(format!("'{}' was not found on $PATH", binary));
    }

    let mut map = processes().lock().unwrap();
    if let Some(existing) = map.get_mut(&config.id) {
        if matches!(existing.child.try_wait(), Ok(None)) {
            return Err(format!("'{}' is already running", config.name));
        }
    }

    // Checked after the already-running test so our own live instance
    // reports as such rather than as an anonymous port conflict. Runs under
    // the registry lock (at most the 200ms probe timeout), which is in line
    // with `stop` holding it through a kill.
    if let Some(port) = config.port {
        if port_accepts_connections(port) {
            return Err(format!(
                "Port {} is already in use. Stop whatever is listening on it before starting '{}'.",
                port, config.name
            ));
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
    // `Command` searches the child's PATH when one is set, so the package
    // manager itself also resolves from the selected Node's bin folder.
    if let Some(dir) = nvm_bin {
        command.env("PATH", path_with_prefix(dir)?);
    }

    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to start '{}': {}", config.name, e))?;
    let pid = child.id();

    let log: Arc<Mutex<VecDeque<LogLine>>> = Arc::new(Mutex::new(VecDeque::new()));
    if let Some(text) = node.describe() {
        log.lock().unwrap().push_back(LogLine {
            stream: LogStream::Stdout,
            text,
        });
    }
    let urls: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let last_error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    if let Some(stdout) = child.stdout.take() {
        spawn_log_reader(
            stdout,
            LogStream::Stdout,
            Arc::clone(&log),
            Arc::clone(&urls),
            Arc::clone(&last_error),
        );
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_log_reader(
            stderr,
            LogStream::Stderr,
            Arc::clone(&log),
            Arc::clone(&urls),
            Arc::clone(&last_error),
        );
    }

    let watched_urls = Arc::clone(&urls);
    let watched_error = Arc::clone(&last_error);
    let running = RunningServer {
        project: project.to_string(),
        config: config.clone(),
        child,
        pid,
        started_at: chrono::Utc::now().to_rfc3339(),
        log,
        urls,
        last_error,
    };
    map.insert(config.id.clone(), running);
    // Released before the watch loop so status polls keep working while we
    // wait, and so `stop` below can take it.
    drop(map);

    // Watch the first moments of output. A crash here is a failed start,
    // not a server that is "running" with the reason buried in its log. A
    // printed URL means the server is up, so return without waiting out
    // the window. The entry is kept after a kill so the row can still show
    // the crash line and the captured log.
    let deadline = Instant::now() + grace;
    loop {
        let crash = watched_error.lock().unwrap().clone();
        if let Some(line) = crash {
            let _ = stop(&config.id);
            return Err(format!("'{}' failed to start: {}", config.name, line));
        }
        if !watched_urls.lock().unwrap().is_empty() || Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(START_POLL);
    }

    let mut map = processes().lock().unwrap();
    let running = map
        .get_mut(&config.id)
        .ok_or_else(|| format!("'{}' was forgotten while starting", config.name))?;
    Ok(status_from_running(&config.id, running))
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

    #[test]
    fn recognises_crash_lines_but_not_watcher_warnings() {
        assert!(is_fatal_line(
            "Error: listen EADDRINUSE: address already in use 127.0.0.1:3900"
        ));
        assert!(is_fatal_line(
            "[nodemon] app crashed - waiting for file changes before starting..."
        ));
        assert!(is_fatal_line(
            "\u{1b}[31mError: listen EACCES: permission denied 0.0.0.0:80\u{1b}[39m"
        ));
        assert!(is_fatal_line("Emitted 'error' event on Server instance at:"));
        assert!(!is_fatal_line("  ➜  Local:   http://localhost:5173/"));
        // chokidar/vite print this for unreadable directories without dying.
        assert!(!is_fatal_line("EACCES: permission denied, watch '/private/var'"));
    }

    #[test]
    fn fatal_line_sets_last_error_and_a_later_url_clears_it() {
        let log = Mutex::new(VecDeque::new());
        let urls = Mutex::new(Vec::new());
        let last_error = Mutex::new(None);

        record_line(
            "Error: listen EADDRINUSE: address already in use 127.0.0.1:3900".to_string(),
            LogStream::Stderr,
            &log,
            &urls,
            &last_error,
        );
        assert_eq!(
            last_error.lock().unwrap().as_deref(),
            Some("Error: listen EADDRINUSE: address already in use 127.0.0.1:3900")
        );
        assert!(urls.lock().unwrap().is_empty());

        record_line(
            "[waiforge] http://127.0.0.1:3900".to_string(),
            LogStream::Stdout,
            &log,
            &urls,
            &last_error,
        );
        assert!(last_error.lock().unwrap().is_none());
        assert_eq!(*urls.lock().unwrap(), vec!["http://127.0.0.1:3900/".to_string()]);
        assert_eq!(log.lock().unwrap().len(), 2);
    }

    #[test]
    fn port_probe_sees_a_live_listener_and_not_a_closed_port() {
        let live = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        assert!(port_accepts_connections(live.local_addr().unwrap().port()));

        // Closing a listener is not instantaneous on macOS: under load (the
        // spawn tests in this module) the port can still complete a
        // handshake for a few milliseconds afterwards, so the closed-port
        // check retries briefly instead of asserting on a single probe.
        let closed_port = {
            let never_connected = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            never_connected.local_addr().unwrap().port()
        };
        let deadline = Instant::now() + Duration::from_secs(1);
        while port_accepts_connections(closed_port) {
            assert!(
                Instant::now() < deadline,
                "port {closed_port} still accepting a second after its listener closed"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
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

        // Short window: this script prints nothing, so the default would
        // only add four seconds of waiting.
        let status = start_with_grace(
            "test-project",
            tmp.path().to_str().unwrap(),
            &config,
            &NodeSelection::NotRequested,
            Duration::from_millis(300),
        )
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

        start("test-project", tmp.path().to_str().unwrap(), &config, &NodeSelection::NotRequested).expect("server should start");

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

    /// The reported bug: a watch-mode supervisor keeps the tree alive after
    /// the real server dies. When that happens inside the grace window,
    /// `start` must fail, kill the tree, and keep the crash line on the
    /// (now stopped) entry. The node script stands in for `tsx watch`: it
    /// prints the crash and then idles.
    #[cfg(unix)]
    #[test]
    fn start_fails_when_the_server_crashes_within_the_grace_window() {
        if crate::core::tools::find_binary_on_path("npm").is_none()
            || crate::core::tools::find_binary_on_path("node").is_none()
        {
            eprintln!("skipping: npm/node not found on $PATH");
            return;
        }

        let tmp = TempDir::new().unwrap();
        let marker = "automatic_dev_server_test_marker_crash_7b1e";
        // The crash text is assembled at runtime: npm echoes the script
        // command to stderr before running it, so a literal here would match
        // the echo and pass this test before node has even started.
        let package_json = format!(
            r#"{{"name":"fixture","scripts":{{"dev":"node -e \"/*{marker}*/ console.error('Error: listen ' + 'EADDR' + 'INUSE: address already ' + 'in use 127.0.0.1:3900'); setInterval(function(){{}}, 1000)\""}}}}"#
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

        let err = start("test-project", tmp.path().to_str().unwrap(), &config, &NodeSelection::NotRequested)
            .expect_err("a crash inside the window should fail the start");
        assert!(err.contains("failed to start"), "got: {}", err);
        assert!(err.contains("EADDRINUSE"), "got: {}", err);

        let status = list_statuses("test-project", std::slice::from_ref(&config))
            .into_iter()
            .next()
            .expect("entry is kept after the kill");
        assert!(!status.running, "the tree must have been killed");
        assert!(status.last_error.unwrap().contains("EADDRINUSE"));
        assert!(
            !pgrep_matches(marker),
            "node process should be gone after the failed start killed the process group"
        );
    }

    /// A crash after the grace window cannot fail the start any more, so it
    /// must surface as `last_error` on a status that is still `running`,
    /// which is what the UI renders as "Crashed".
    #[cfg(unix)]
    #[test]
    fn late_crash_shows_as_crashed_while_supervisor_stays_alive() {
        if crate::core::tools::find_binary_on_path("npm").is_none()
            || crate::core::tools::find_binary_on_path("node").is_none()
        {
            eprintln!("skipping: npm/node not found on $PATH");
            return;
        }

        let tmp = TempDir::new().unwrap();
        let marker = "automatic_dev_server_test_marker_latecrash_4e6d";
        // Crash text assembled at runtime for the same reason as above: the
        // npm echo of the command must not match before the delay elapses.
        let package_json = format!(
            r#"{{"name":"fixture","scripts":{{"dev":"node -e \"/*{marker}*/ setTimeout(function(){{ console.error('Error: listen ' + 'EADDR' + 'INUSE: address already ' + 'in use 127.0.0.1:3900'); }}, 800); setInterval(function(){{}}, 1000)\""}}}}"#
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

        // A window shorter than the script's delay, so the crash lands
        // after `start` has already returned.
        let status = start_with_grace(
            "test-project",
            tmp.path().to_str().unwrap(),
            &config,
            &NodeSelection::NotRequested,
            Duration::from_millis(200),
        )
        .expect("server should start");
        assert!(status.running);
        assert!(status.last_error.is_none());

        let mut observed: Option<DevServerStatus> = None;
        for _ in 0..40 {
            let statuses = list_statuses("test-project", std::slice::from_ref(&config));
            if let Some(status) = statuses.into_iter().next() {
                if status.last_error.is_some() {
                    observed = Some(status);
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }

        stop(&config.id).expect("stop should succeed");

        let observed = observed.expect("crash line should surface as last_error");
        assert!(observed.running, "supervisor tree is still alive, so running stays true");
        assert!(observed.last_error.unwrap().contains("EADDRINUSE"));
    }

    #[cfg(unix)]
    #[test]
    fn start_refuses_when_configured_port_is_already_in_use() {
        if crate::core::tools::find_binary_on_path("npm").is_none() {
            eprintln!("skipping: npm not found on $PATH");
            return;
        }

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("package.json"),
            r#"{"name":"fixture","scripts":{"dev":"node -e \"setInterval(function(){}, 1000)\""}}"#,
        )
        .unwrap();

        let config = ServerConfig {
            id: "test-automatic_dev_server_test_marker_port_2d9c".to_string(),
            name: "test".to_string(),
            package_manager: PackageManager::Npm,
            script: "dev".to_string(),
            subdirectory: String::new(),
            port: Some(port),
            created_at: String::new(),
        };

        let err = start("test-project", tmp.path().to_str().unwrap(), &config, &NodeSelection::NotRequested)
            .expect_err("start should refuse while the port is held");
        assert!(err.contains(&format!("Port {} is already in use", port)), "got: {}", err);
        drop(listener);
    }

    /// With an nvm selection, the package manager must come from the
    /// selected version's bin folder, ahead of anything on the app's PATH.
    /// A fake `npm` script stands in for the real one, so this needs no
    /// Node install.
    #[cfg(unix)]
    #[test]
    fn start_runs_the_package_manager_from_the_selected_nvm_bin() {
        use std::os::unix::fs::PermissionsExt;

        let nvm = TempDir::new().unwrap();
        let bin_dir = nvm.path().join("versions/node/v99.0.0/bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let fake_npm = bin_dir.join("npm");
        std::fs::write(
            &fake_npm,
            "#!/bin/sh\necho \"fake-npm-from-nvm Local: http://localhost:4999/\"\nexec sleep 30\n",
        )
        .unwrap();
        std::fs::set_permissions(&fake_npm, std::fs::Permissions::from_mode(0o755)).unwrap();

        let project = TempDir::new().unwrap();
        std::fs::write(project.path().join(".nvmrc"), "99\n").unwrap();
        let node = crate::node_runtime::select_node(project.path(), project.path(), nvm.path())
            .expect("fake nvm install should resolve");

        let config = ServerConfig {
            id: "test-automatic_dev_server_nvm_bin_7c41".to_string(),
            name: "test".to_string(),
            package_manager: PackageManager::Npm,
            script: "dev".to_string(),
            subdirectory: String::new(),
            port: None,
            created_at: String::new(),
        };

        let status = start_with_grace(
            "test-project",
            project.path().to_str().unwrap(),
            &config,
            &node,
            Duration::from_secs(3),
        )
        .expect("server should start");
        let log = get_log(&config.id);
        stop(&config.id).expect("stop should succeed");

        assert_eq!(status.urls, vec!["http://localhost:4999/".to_string()]);
        assert_eq!(log[0].text, "Using Node v99.0.0 from nvm (.nvmrc)");
        assert!(
            log.iter().any(|line| line.text.contains("fake-npm-from-nvm")),
            "log should show the fake npm ran: {:?}",
            log
        );
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
