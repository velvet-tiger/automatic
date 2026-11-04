//! Recent git history for a project folder, shown on the project Summary.
//!
//! Read-only: Automatic runs `git config`, `git status` and `git log` in the
//! project's directory and never writes to the repository. Parsing is pure and
//! kept apart from the subprocess calls so it can be tested without git.
//!
//! The folder can be a repository the user downloaded with its `.git`
//! directory, so its `.git/config` is not trusted. Every call turns off the
//! settings that make a read run another program:
//!
//! - `core.fsmonitor`, external diff drivers, textconv and signature display,
//!   with `-c` and command options.
//! - Clean, smudge and process filters defined in the repository's own config
//!   (local or worktree scope, includes counted), which `git status` runs when
//!   it re-checks a file. Their names come from `git config`, which runs
//!   nothing, and are blanked through `GIT_CONFIG_*` variables, so a name
//!   holding `=` cannot escape the override. Filters from the user's global
//!   config, such as Git LFS, stay on: they are the user's own, and turning
//!   them off makes files look changed.
//! - Lazy fetches in a partial clone, which would run `core.sshCommand` and
//!   write objects. `GIT_NO_LAZY_FETCH` stops them on git 2.44 and later.
//!   For older git, a partial clone also skips the line counts, the only
//!   step that needs file contents.
//!
//! `--no-optional-locks` stops `git status` from rewriting the index, so a
//! refresh never races the user's own git commands. Every call also sets
//! `protocol.allow=never` and a no-op `core.sshCommand`, so no transport can
//! run whatever else asks for one.
//!
//! The filter override needs git 2.31 and `GIT_NO_LAZY_FETCH` needs git 2.44.
//! Older git ignores both silently, so a repository that needs one of them
//! is refused on older git rather than read unprotected.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

use serde::Serialize;

use super::remote_sources::run_git_limited;
use super::Project;

/// History covers 14 weeks, so the 13-week calendar's first Sunday is always
/// inside it.
const HISTORY_DAYS: u32 = 98;
/// A cap on commits read, so a busy monorepo cannot flood the Summary.
pub const MAX_COMMITS: usize = 2000;
/// A local read. A repository that takes longer than this is reported as
/// unavailable rather than holding a worker.
const GIT_READ_LIMIT: Duration = Duration::from_secs(20);
/// The first git that reads `GIT_CONFIG_COUNT`.
const MIN_VERSION_CONFIG_ENV: GitVersion = GitVersion(2, 31);
/// The first git that reads `GIT_NO_LAZY_FETCH`.
const MIN_VERSION_NO_LAZY_FETCH: GitVersion = GitVersion(2, 44);

/// A git release as major and minor numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct GitVersion(u32, u32);

impl std::fmt::Display for GitVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.0, self.1)
    }
}

/// The git Automatic runs, found once per run.
#[derive(Debug, Clone)]
struct GitInstall {
    path: PathBuf,
    version: GitVersion,
}

/// One commit that touched the project folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitCommit {
    pub hash: String,
    /// Committer date, ISO 8601 with the committer's UTC offset. The same
    /// date git orders and limits history by, so a rebased commit sits where
    /// it landed.
    pub timestamp: String,
    pub author: String,
    pub subject: String,
    /// Lines added inside the project folder. Binary files count as 0, and
    /// so does every commit when `GitActivity::line_stats` is false.
    pub insertions: u64,
    pub deletions: u64,
    /// True when the commit changed a file Automatic writes for this project.
    pub touches_agent_config: bool,
}

/// The working tree's state when it was read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct GitRepoState {
    /// `None` on a detached HEAD.
    pub branch: Option<String>,
    /// Changed, staged, conflicted and untracked entries inside the project
    /// folder. An untracked directory counts once.
    pub uncommitted_files: usize,
    /// The upstream branch, such as `origin/main`. Set even when that branch
    /// no longer exists, in which case `ahead` and `behind` are `None`.
    pub upstream: Option<String>,
    /// `None` when there is no upstream or it is gone.
    pub ahead: Option<u64>,
    pub behind: Option<u64>,
    /// False for a repository with no commits yet.
    pub has_commits: bool,
}

/// Everything the Summary shows about a project's repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitActivity {
    pub repo: GitRepoState,
    /// Newest first, from the last [`HISTORY_DAYS`] days.
    pub commits: Vec<GitCommit>,
    /// True when [`MAX_COMMITS`] cut the history short. The last commit
    /// then marks where the history starts.
    pub truncated: bool,
    pub history_days: u32,
    /// False in a partial clone, where counting lines would fetch file
    /// contents from the remote.
    pub line_stats: bool,
}

/// The repository settings that change how Automatic reads it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct RepoConfig {
    /// Filter drivers defined at local or worktree scope.
    local_filters: Vec<String>,
    partial_clone: bool,
}

// ── Parsing ───────────────────────────────────────────────────────────────────

/// Parse `git config --show-scope -z --get-regexp` output: records of
/// `scope NUL key LF value NUL`.
fn parse_repo_config(output: &[u8]) -> RepoConfig {
    let text = String::from_utf8_lossy(output);
    let mut config = RepoConfig::default();
    let mut fields = text.split('\0');
    while let (Some(scope), Some(entry)) = (fields.next(), fields.next()) {
        let (key, value) = entry.split_once('\n').unwrap_or((entry, ""));
        let repo_scope = scope == "local" || scope == "worktree";
        if !repo_scope {
            continue;
        }
        if let Some(name) = filter_name(key) {
            if !config.local_filters.iter().any(|n| n == name) {
                config.local_filters.push(name.to_string());
            }
        } else if is_partial_clone_key(key) && !value.trim().is_empty() && value.trim() != "false" {
            config.partial_clone = true;
        }
    }
    config
}

/// The driver name in `filter.<name>.<variable>`. The name may hold dots.
fn filter_name(key: &str) -> Option<&str> {
    let rest = key.strip_prefix("filter.")?;
    let (name, _variable) = rest.rsplit_once('.')?;
    (!name.is_empty()).then_some(name)
}

fn is_partial_clone_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key == "extensions.partialclone" || (key.starts_with("remote.") && key.ends_with(".promisor"))
}

/// Parse `git status --porcelain=v2 --branch -z` output.
fn parse_status(output: &[u8]) -> GitRepoState {
    let text = String::from_utf8_lossy(output);
    let mut state = GitRepoState {
        has_commits: true,
        ..GitRepoState::default()
    };
    let mut records = text.split('\0').filter(|r| !r.is_empty());
    while let Some(record) = records.next() {
        if let Some(header) = record.strip_prefix("# ") {
            apply_branch_header(&mut state, header);
            continue;
        }
        match record.as_bytes().first() {
            Some(b'1') | Some(b'u') | Some(b'?') => state.uncommitted_files += 1,
            Some(b'2') => {
                state.uncommitted_files += 1;
                // A rename or copy is followed by a record holding the
                // original path, which is not a separate change.
                records.next();
            }
            // `!` (ignored) is only printed on request and is not a change.
            _ => {}
        }
    }
    state
}

fn apply_branch_header(state: &mut GitRepoState, header: &str) {
    let Some((key, value)) = header.split_once(' ') else {
        return;
    };
    match key {
        "branch.oid" => state.has_commits = value != "(initial)",
        "branch.head" => {
            state.branch = (value != "(detached)").then(|| value.to_string());
        }
        "branch.upstream" => state.upstream = Some(value.to_string()),
        "branch.ab" => {
            let mut parts = value.split(' ');
            let ahead = parts.next().and_then(|p| p.strip_prefix('+'));
            let behind = parts.next().and_then(|p| p.strip_prefix('-'));
            state.ahead = ahead.and_then(|n| n.parse().ok());
            state.behind = behind.and_then(|n| n.parse().ok());
        }
        _ => {}
    }
}

/// The `--format` for [`parse_log`]. Fields are NUL-separated, so commit text
/// cannot forge a field or a record: NUL cannot appear in commit metadata.
const LOG_FORMAT: &str = "--format=%H%x00%cI%x00%aN%x00%s";

/// A commit as `git log` printed it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedCommit {
    hash: String,
    timestamp: String,
    author: String,
    subject: String,
    insertions: u64,
    deletions: u64,
}

fn is_commit_hash(text: &str) -> bool {
    matches!(text.len(), 40 | 64) && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Parse `git log -z [--shortstat]` output written with [`LOG_FORMAT`].
///
/// The layout is `hash NUL date NUL author NUL subject NUL`, and git puts a
/// commit's `--shortstat` line, when it has one, in front of the next hash:
/// `LF stat LF hash NUL ...`. A token that is not a hash ends the parse,
/// because the output is not what was asked for.
fn parse_log(output: &str) -> Vec<ParsedCommit> {
    let mut commits: Vec<ParsedCommit> = Vec::new();
    let mut tokens = output.split('\0');
    while let Some(token) = tokens.next() {
        let (stat, rest) = token.rsplit_once('\n').unwrap_or(("", token));
        if let Some(previous) = commits.last_mut() {
            let (insertions, deletions) = parse_shortstat(stat);
            previous.insertions = insertions;
            previous.deletions = deletions;
        }
        let hash = rest.trim();
        if hash.is_empty() || !is_commit_hash(hash) {
            break;
        }
        let (Some(timestamp), Some(author), Some(subject)) =
            (tokens.next(), tokens.next(), tokens.next())
        else {
            break;
        };
        commits.push(ParsedCommit {
            hash: hash.to_string(),
            timestamp: timestamp.to_string(),
            author: author.to_string(),
            subject: subject.to_string(),
            insertions: 0,
            deletions: 0,
        });
    }
    commits
}

/// Lines added and removed in a `--shortstat` line such as
/// ` 3 files changed, 10 insertions(+), 2 deletions(-)`.
fn parse_shortstat(stat: &str) -> (u64, u64) {
    let mut insertions = 0;
    let mut deletions = 0;
    for part in stat.trim().split(", ") {
        let Some((number, label)) = part.split_once(' ') else {
            continue;
        };
        let Ok(number) = number.parse::<u64>() else {
            continue;
        };
        if label.starts_with("insertion") {
            insertions = number;
        } else if label.starts_with("deletion") {
            deletions = number;
        }
    }
    (insertions, deletions)
}

// ── Agent config paths ────────────────────────────────────────────────────────

/// Pathspecs for the files Automatic writes for this project, relative to
/// the project folder.
///
/// The same ownership the managed `.gitignore` block uses, so the two never
/// disagree about which files are Automatic's. `literal` magic stops a name
/// from being read as a glob or as further pathspec magic.
fn agent_config_pathspecs(project_dir: &Path, project: &Project) -> Vec<String> {
    let mut rel_paths: Vec<String> = vec![
        ".automatic.json".to_string(),
        ".automatic".to_string(),
        ".agents".to_string(),
    ];
    let managed = project
        .agents
        .iter()
        .filter_map(|id| crate::agent::from_id(id))
        .flat_map(|agent| agent.managed_gitignore_paths(project_dir));
    for path in managed {
        if let Some(rel) = relative_slash_path(project_dir, &path.path) {
            rel_paths.push(rel);
        }
    }
    rel_paths.sort();
    rel_paths.dedup();
    rel_paths
        .into_iter()
        .map(|rel| format!(":(literal){rel}"))
        .collect()
}

/// `path` relative to `root`, with `/` separators as git expects.
fn relative_slash_path(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let parts: Vec<&str> = rel
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join("/"))
}

fn parse_hashes(output: &[u8]) -> HashSet<String> {
    String::from_utf8_lossy(output)
        .split(['\n', '\0'])
        .map(str::trim)
        .filter(|h| is_commit_hash(h))
        .map(str::to_string)
        .collect()
}

// ── Running git ───────────────────────────────────────────────────────────────

/// A `git` command for `dir` with every setting that could run another
/// program or reach the network turned off. `config` names the repository's
/// own filters to blank.
fn git_command(git: &GitInstall, dir: &Path, config: &RepoConfig) -> Command {
    let mut command = Command::new(&git.path);
    command
        .arg("--no-pager")
        .arg("--no-optional-locks")
        .args(["-c", "protocol.allow=never"])
        .args(["-c", "core.sshCommand=false"])
        .args(["-c", "core.fsmonitor=false"])
        .args(["-c", "core.quotePath=false"])
        .args(["-c", "color.ui=false"])
        .args(["-c", "log.showSignature=false"])
        .arg("-C")
        .arg(dir)
        // Error text is matched below, so it must not be translated.
        .env("LC_ALL", "C")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_NO_LAZY_FETCH", "1");

    let mut overrides: Vec<(String, &str)> = Vec::new();
    for name in &config.local_filters {
        for variable in ["clean", "smudge", "process"] {
            overrides.push((format!("filter.{name}.{variable}"), ""));
        }
        overrides.push((format!("filter.{name}.required"), "false"));
    }
    // Settings the user exported this way are kept: ours follow them, and
    // a later entry wins.
    let inherited = std::env::var("GIT_CONFIG_COUNT")
        .ok()
        .and_then(|n| n.trim().parse::<usize>().ok())
        .unwrap_or(0);
    command.env("GIT_CONFIG_COUNT", (inherited + overrides.len()).to_string());
    for (offset, (key, value)) in overrides.iter().enumerate() {
        let index = inherited + offset;
        command
            .env(format!("GIT_CONFIG_KEY_{index}"), key)
            .env(format!("GIT_CONFIG_VALUE_{index}"), value);
    }
    command
}

/// Refuse a repository whose protection this git would silently ignore.
fn check_git_supports(config: &RepoConfig, version: GitVersion) -> Result<(), String> {
    if !config.local_filters.is_empty() && version < MIN_VERSION_CONFIG_ENV {
        return Err(format!(
            "This repository defines its own git filters. Reading it safely needs git \
             {MIN_VERSION_CONFIG_ENV} or later, and git {version} is installed. Update git to see its history."
        ));
    }
    if config.partial_clone && version < MIN_VERSION_NO_LAZY_FETCH {
        return Err(format!(
            "This repository is a partial clone. Reading it without fetching needs git \
             {MIN_VERSION_NO_LAZY_FETCH} or later, and git {version} is installed. Update git to see its history."
        ));
    }
    Ok(())
}

/// Outcome of a git call that may find no repository.
enum GitRun {
    Output(Vec<u8>),
    NotARepository,
}

fn run_git(
    git: &GitInstall,
    dir: &Path,
    config: &RepoConfig,
    args: &[&str],
    what: &str,
    allowed_exit: Option<i32>,
) -> Result<GitRun, String> {
    let mut command = git_command(git, dir, config);
    command.args(args);
    let label = dir.display().to_string();
    let output = run_git_limited(&mut command, GIT_READ_LIMIT, what, &label)?;
    if output.status.success() || (allowed_exit.is_some() && output.status.code() == allowed_exit)
    {
        return Ok(GitRun::Output(output.stdout));
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("not a git repository") {
        return Ok(GitRun::NotARepository);
    }
    Err(format!(
        "git {} failed in {}: {}",
        what,
        label,
        stderr.trim()
    ))
}

/// The installed git, checked once per run. `None` when there is no usable
/// git.
///
/// On macOS, `/usr/bin/git` is a stub when the Command Line Tools are
/// missing, and running it opens the system's install dialog. The Summary
/// reloads on every window focus, so that dialog would keep returning. The
/// stub is used only when `xcode-select -p` finds a developer directory,
/// which opens nothing. Installing git while the app runs takes effect on
/// the next launch.
fn git_install() -> Option<&'static GitInstall> {
    static INSTALL: OnceLock<Option<GitInstall>> = OnceLock::new();
    INSTALL.get_or_init(detect_git).as_ref()
}

#[cfg(test)]
fn git_available() -> bool {
    git_install().is_some()
}

fn detect_git() -> Option<GitInstall> {
    let name = if cfg!(windows) { "git.exe" } else { "git" };
    let path = find_on_path(name)?;
    if cfg!(target_os = "macos") && path == Path::new("/usr/bin/git") && !xcode_tools_installed() {
        return None;
    }
    let output = Command::new(&path)
        .arg("--version")
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let version = parse_git_version(&String::from_utf8_lossy(&output.stdout))?;
    Some(GitInstall { path, version })
}

/// The version in `git --version` output, such as
/// `git version 2.39.5 (Apple Git-154)` or `git version 2.45.1.windows.1`.
fn parse_git_version(text: &str) -> Option<GitVersion> {
    let number = text.trim().strip_prefix("git version ")?;
    let mut parts = number.split(|c: char| !c.is_ascii_digit());
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some(GitVersion(major, minor))
}

fn xcode_tools_installed() -> bool {
    Command::new("/usr/bin/xcode-select")
        .arg("-p")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// The first `name` on PATH that can be run, as `Command::new(name)` finds it.
fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable_file(candidate))
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

/// Read the repository state and recent commits for `project_dir`.
///
/// Returns `Ok(None)` when the folder is not inside a git repository, or when
/// git is not installed: in both cases there is no history to show, and the
/// Summary leaves the git cards out. History is limited to the project
/// folder, so a project inside a larger repository shows only its own
/// commits.
pub fn read_git_activity(
    project_dir: &Path,
    project: &Project,
) -> Result<Option<GitActivity>, String> {
    if !project_dir.is_dir() {
        return Err(format!(
            "Project folder {} does not exist",
            project_dir.display()
        ));
    }
    let Some(git) = git_install() else {
        return Ok(None);
    };

    // `git config --get-regexp` exits 1 when nothing matches.
    let config_args = [
        "config",
        "--show-scope",
        "-z",
        "--get-regexp",
        r"^(filter\..*|extensions\.partialclone|remote\..*\.promisor)$",
    ];
    let config = match run_git(git, project_dir, &RepoConfig::default(), &config_args, "config", Some(1))? {
        GitRun::NotARepository => return Ok(None),
        GitRun::Output(out) => parse_repo_config(&out),
    };
    check_git_supports(&config, git.version)?;

    let status_args = [
        "status",
        "--porcelain=v2",
        "--branch",
        "-z",
        "--ignore-submodules=all",
        "--",
        ".",
    ];
    let repo = match run_git(git, project_dir, &config, &status_args, "status", None)? {
        GitRun::NotARepository => return Ok(None),
        GitRun::Output(out) => parse_status(&out),
    };
    let line_stats = !config.partial_clone;
    if !repo.has_commits {
        return Ok(Some(GitActivity {
            repo,
            commits: Vec::new(),
            truncated: false,
            history_days: HISTORY_DAYS,
            line_stats,
        }));
    }

    let since = format!("--since={HISTORY_DAYS}.days.ago");
    let max_count = format!("--max-count={}", MAX_COMMITS + 1);
    let mut log_args = vec![
        "log",
        "-z",
        "--no-color",
        "--no-renames",
        "--no-textconv",
        "--no-ext-diff",
        "--no-show-signature",
        since.as_str(),
        max_count.as_str(),
        LOG_FORMAT,
    ];
    if line_stats {
        log_args.push("--shortstat");
    }
    log_args.extend(["HEAD", "--", "."]);
    let raw = match run_git(git, project_dir, &config, &log_args, "log", None)? {
        GitRun::NotARepository => return Ok(None),
        GitRun::Output(out) => out,
    };
    let mut parsed = parse_log(&String::from_utf8_lossy(&raw));
    let truncated = parsed.len() > MAX_COMMITS;
    parsed.truncate(MAX_COMMITS);

    // Which commits touched agent config: a tree comparison, so it needs no
    // file contents and works in a partial clone.
    let pathspecs = agent_config_pathspecs(project_dir, project);
    let mut tag_args: Vec<&str> = vec!["log", "--format=%H", since.as_str(), max_count.as_str(), "HEAD", "--"];
    tag_args.extend(pathspecs.iter().map(String::as_str));
    let tagged = match run_git(git, project_dir, &config, &tag_args, "log", None)? {
        GitRun::NotARepository => return Ok(None),
        GitRun::Output(out) => parse_hashes(&out),
    };

    let commits = parsed
        .into_iter()
        .map(|c| GitCommit {
            touches_agent_config: tagged.contains(&c.hash),
            hash: c.hash,
            timestamp: c.timestamp,
            author: c.author,
            subject: c.subject,
            insertions: c.insertions,
            deletions: c.deletions,
        })
        .collect();
    Ok(Some(GitActivity {
        repo,
        commits,
        truncated,
        history_days: HISTORY_DAYS,
        line_stats,
    }))
}

/// The project folder for `project`, or an error naming why there is none.
pub fn project_git_dir(project: &Project) -> Result<PathBuf, String> {
    let dir = project.directory.trim();
    if dir.is_empty() {
        return Err(format!("Project '{}' has no folder set", project.name));
    }
    Ok(PathBuf::from(dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    const H1: &str = "1111111111111111111111111111111111111111";
    const H2: &str = "2222222222222222222222222222222222222222";
    const H3: &str = "3333333333333333333333333333333333333333";

    #[test]
    fn parses_commits_with_shortstat() {
        let out = format!(
            "{H1}\02026-11-01T10:00:00+11:00\0Sam Lee\0fix: thing\0\n 3 files changed, 13 insertions(+), 1 deletion(-)\n\
             {H2}\02026-10-30T09:00:00+00:00\0Alex\0merge\0\
             {H3}\02026-10-29T09:00:00+00:00\0Alex\0bin only\0\n 1 file changed, 2 deletions(-)\n"
        );
        let commits = parse_log(&out);
        assert_eq!(commits.len(), 3);
        assert_eq!(commits[0].hash, H1);
        assert_eq!(commits[0].author, "Sam Lee");
        assert_eq!(commits[0].subject, "fix: thing");
        assert_eq!((commits[0].insertions, commits[0].deletions), (13, 1));
        assert_eq!((commits[1].insertions, commits[1].deletions), (0, 0));
        assert_eq!((commits[2].insertions, commits[2].deletions), (0, 2));
    }

    #[test]
    fn commit_text_cannot_forge_fields_or_records() {
        let subject = format!("evil\u{1e}{H2}\u{1f}x\n{H3}");
        let out = format!("{H1}\02026-11-01T10:00:00Z\0A\u{1f}B\0{subject}\0");
        let commits = parse_log(&out);
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].author, "A\u{1f}B");
        assert_eq!(commits[0].subject, subject);
    }

    #[test]
    fn unexpected_output_ends_the_parse() {
        assert!(parse_log("").is_empty());
        assert!(parse_log("\n").is_empty());
        assert!(parse_log("not-a-hash\0x\0y\0z\0").is_empty());
    }

    #[test]
    fn parses_status_branch_and_entries() {
        let out = b"# branch.oid 0123\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -5\0\
1 .M N... 100644 100644 100644 a a src/a.rs\0\
2 R. N... 100644 100644 100644 a a R100 new.rs\0old.rs\0\
u UU N... 1 2 3 4 a b c conflict.rs\0\
? scratch/\0";
        let state = parse_status(out);
        assert_eq!(state.branch.as_deref(), Some("main"));
        assert_eq!(state.upstream.as_deref(), Some("origin/main"));
        assert_eq!(state.ahead, Some(2));
        assert_eq!(state.behind, Some(5));
        assert_eq!(state.uncommitted_files, 4);
        assert!(state.has_commits);
    }

    #[test]
    fn a_gone_upstream_keeps_its_name_without_counts() {
        let state = parse_status(b"# branch.oid abc\0# branch.head main\0# branch.upstream origin/old\0");
        assert_eq!(state.upstream.as_deref(), Some("origin/old"));
        assert_eq!(state.ahead, None);
    }

    #[test]
    fn status_without_upstream_or_commits() {
        let out = b"# branch.oid (initial)\0# branch.head main\0";
        let state = parse_status(out);
        assert_eq!(state.upstream, None);
        assert_eq!(state.ahead, None);
        assert_eq!(state.behind, None);
        assert!(!state.has_commits);
        assert_eq!(state.uncommitted_files, 0);
    }

    #[test]
    fn detached_head_has_no_branch() {
        let state = parse_status(b"# branch.oid abc\0# branch.head (detached)\0");
        assert_eq!(state.branch, None);
    }

    #[test]
    fn repo_config_keeps_only_repository_filters() {
        let out = b"global\0filter.lfs.clean\ngit-lfs clean -- %f\0\
local\0filter.evil.clean\ntouch x\0\
local\0filter.evil.process\ntouch y\0\
local\0filter.inc.luded=x.smudge\ntouch z\0\
worktree\0filter.wt.clean\ntouch w\0";
        let config = parse_repo_config(out);
        assert_eq!(config.local_filters, vec!["evil", "inc.luded=x", "wt"]);
        assert!(!config.partial_clone);
    }

    #[test]
    fn repo_config_detects_a_partial_clone() {
        let config = parse_repo_config(b"local\0remote.origin.promisor\ntrue\0");
        assert!(config.partial_clone);
        let config = parse_repo_config(b"local\0extensions.partialclone\norigin\0");
        assert!(config.partial_clone);
        let config = parse_repo_config(b"local\0remote.origin.promisor\nfalse\0");
        assert!(!config.partial_clone);
    }

    #[test]
    fn parses_git_versions() {
        assert_eq!(parse_git_version("git version 2.39.5 (Apple Git-154)\n"), Some(GitVersion(2, 39)));
        assert_eq!(parse_git_version("git version 2.45.1.windows.1"), Some(GitVersion(2, 45)));
        assert_eq!(parse_git_version("git version 3.0.0-rc1"), Some(GitVersion(3, 0)));
        assert_eq!(parse_git_version("not git"), None);
    }

    #[test]
    fn older_git_is_refused_only_when_a_protection_needs_it() {
        let plain = RepoConfig::default();
        let filters = RepoConfig { local_filters: vec!["evil".into()], partial_clone: false };
        let partial = RepoConfig { local_filters: vec![], partial_clone: true };
        assert!(check_git_supports(&plain, GitVersion(2, 20)).is_ok());
        assert!(check_git_supports(&filters, GitVersion(2, 30)).is_err());
        assert!(check_git_supports(&filters, GitVersion(2, 31)).is_ok());
        assert!(check_git_supports(&partial, GitVersion(2, 43)).is_err());
        assert!(check_git_supports(&partial, GitVersion(2, 44)).is_ok());
    }

    #[test]
    fn shortstat_parses_each_part() {
        assert_eq!(parse_shortstat(" 1 file changed, 1 insertion(+)"), (1, 0));
        assert_eq!(parse_shortstat(" 2 files changed, 5 insertions(+), 3 deletions(-)"), (5, 3));
        assert_eq!(parse_shortstat(""), (0, 0));
    }

    // ── Against a real repository ─────────────────────────────────────────

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(["-c", "user.name=Test", "-c", "user.email=t@example.com"])
            .args(["-c", "commit.gpgsign=false"])
            .arg("-C")
            .arg(dir)
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("run git");
        assert!(status.success(), "git {:?} failed", args);
    }

    fn project_with_agents(dir: &Path, agents: &[&str]) -> Project {
        Project {
            name: "test".into(),
            directory: dir.to_string_lossy().into_owned(),
            agents: agents.iter().map(|a| a.to_string()).collect(),
            ..Project::default()
        }
    }

    fn committed_repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        git(tmp.path(), &["init", "-q", "-b", "main"]);
        std::fs::write(tmp.path().join("a.txt"), "x\n").unwrap();
        git(tmp.path(), &["add", "."]);
        git(tmp.path(), &["commit", "-q", "-m", "first"]);
        tmp
    }

    /// A shell script that creates `marker` when run.
    fn marker_script(dir: &Path, marker: &Path) -> PathBuf {
        let script = dir.join("payload.sh");
        std::fs::write(&script, format!("#!/bin/sh\ntouch '{}'\ncat\n", marker.display())).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        script
    }

    #[test]
    fn folder_outside_a_repository_reads_as_none() {
        if !git_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        // The test means nothing if the temp dir itself sits inside a repository.
        let inside = Command::new("git")
            .arg("-C")
            .arg(tmp.path())
            .args(["rev-parse", "--is-inside-work-tree"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if inside {
            return;
        }
        let project = project_with_agents(tmp.path(), &[]);
        let result = read_git_activity(tmp.path(), &project).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn reads_commits_and_state_from_a_repository() {
        if !git_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        git(root, &["init", "-q", "-b", "main"]);
        std::fs::write(root.join("a.txt"), "one\ntwo\n").unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", "first"]);
        std::fs::write(root.join("CLAUDE.md"), "rules\n").unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", "add instructions"]);
        std::fs::write(root.join("untracked.txt"), "x").unwrap();

        let project = project_with_agents(root, &["claude"]);
        let activity = read_git_activity(root, &project).unwrap().expect("a repo");
        assert_eq!(activity.repo.branch.as_deref(), Some("main"));
        assert_eq!(activity.repo.uncommitted_files, 1);
        assert_eq!(activity.repo.upstream, None);
        assert!(!activity.truncated);
        assert!(activity.line_stats);
        assert_eq!(activity.commits.len(), 2);
        assert_eq!(activity.commits[0].subject, "add instructions");
        assert!(activity.commits[0].touches_agent_config);
        assert_eq!(activity.commits[1].insertions, 2);
        assert!(!activity.commits[1].touches_agent_config);
    }

    #[test]
    fn a_subfolder_shows_only_its_own_commits() {
        if !git_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let sub = root.join("app");
        std::fs::create_dir(&sub).unwrap();
        git(root, &["init", "-q", "-b", "main"]);
        std::fs::write(root.join("other.txt"), "x").unwrap();
        std::fs::write(root.join("CLAUDE.md"), "root rules").unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", "outside"]);
        std::fs::write(sub.join("CLAUDE.md"), "x").unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", "inside"]);

        let project = project_with_agents(&sub, &["claude"]);
        let activity = read_git_activity(&sub, &project).unwrap().expect("a repo");
        assert_eq!(activity.commits.len(), 1);
        assert_eq!(activity.commits[0].subject, "inside");
        // Pathspecs are relative to the project folder, not the repository root.
        assert!(activity.commits[0].touches_agent_config);
    }

    #[test]
    fn a_repository_with_no_commits_has_empty_history() {
        if !git_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        git(tmp.path(), &["init", "-q", "-b", "main"]);
        let project = project_with_agents(tmp.path(), &[]);
        let activity = read_git_activity(tmp.path(), &project).unwrap().expect("a repo");
        assert!(!activity.repo.has_commits);
        assert!(activity.commits.is_empty());
    }

    #[test]
    fn repository_fsmonitor_setting_is_not_run() {
        if !git_available() {
            return;
        }
        let tmp = committed_repo();
        let root = tmp.path();
        let marker = root.join("fsmonitor-ran");
        let script = marker_script(root, &marker);
        git(root, &["config", "core.fsmonitor", script.to_str().unwrap()]);

        let project = project_with_agents(root, &[]);
        read_git_activity(root, &project).unwrap().expect("a repo");
        assert!(!marker.exists(), "core.fsmonitor from the repository ran");
    }

    #[test]
    fn repository_filters_are_not_run_including_included_ones() {
        if !git_available() {
            return;
        }
        let tmp = committed_repo();
        let root = tmp.path();
        let outside = tempfile::tempdir().unwrap();
        let clean_marker = outside.path().join("clean-ran");
        let process_marker = outside.path().join("process-ran");
        let included_marker = outside.path().join("included-ran");
        let clean = marker_script(outside.path(), &clean_marker);
        let process_dir = outside.path().join("p");
        std::fs::create_dir(&process_dir).unwrap();
        let process = marker_script(&process_dir, &process_marker);
        let included_dir = outside.path().join("i");
        std::fs::create_dir(&included_dir).unwrap();
        let included = marker_script(&included_dir, &included_marker);

        // An included file, with a name holding `=` to test the override.
        let include = outside.path().join("include.cfg");
        std::fs::write(
            &include,
            format!("[filter \"inc=x\"]\n\tclean = {}\n", included.display()),
        )
        .unwrap();
        git(root, &["config", "filter.evil.clean", clean.to_str().unwrap()]);
        git(root, &["config", "filter.proc.process", process.to_str().unwrap()]);
        git(root, &["config", "include.path", include.to_str().unwrap()]);
        std::fs::write(
            root.join(".git/info/attributes"),
            "a.txt filter=evil\nb.txt filter=proc\nc.txt filter=inc=x\n",
        )
        .unwrap();
        for name in ["b.txt", "c.txt"] {
            std::fs::write(root.join(name), "x\n").unwrap();
        }
        // Track the new files without running the filters ourselves.
        let blank = ["-c", "filter.evil.clean=", "-c", "filter.proc.process=", "-c", "filter.inc=x.clean="];
        let mut add: Vec<&str> = blank.to_vec();
        add.extend(["add", "b.txt", "c.txt"]);
        git(root, &add);
        // Rewrite every file with the same content and a newer time. The size
        // matches the index, so status must hash each file, through its filter.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        for name in ["a.txt", "b.txt", "c.txt"] {
            std::fs::write(root.join(name), "x\n").unwrap();
        }

        // Setup's own `git add` can run a filter whose name holds `=`, since
        // `-c` splits at the first `=`. Only runs from here on count.
        for marker in [&clean_marker, &process_marker, &included_marker] {
            let _ = std::fs::remove_file(marker);
        }

        let project = project_with_agents(root, &[]);
        read_git_activity(root, &project).unwrap().expect("a repo");
        assert!(!clean_marker.exists(), "clean filter from the repository ran");
        assert!(!process_marker.exists(), "process filter from the repository ran");
        assert!(!included_marker.exists(), "included filter ran");
    }

    #[test]
    fn a_partial_clone_skips_line_counts_and_never_fetches() {
        if !git_available() {
            return;
        }
        // A blobless clone whose older blobs are missing locally, so counting
        // lines in the first commit would need a fetch.
        let source = committed_repo();
        std::fs::write(source.path().join("a.txt"), "x\ny\n").unwrap();
        git(source.path(), &["commit", "-q", "-am", "second"]);
        git(source.path(), &["config", "uploadpack.allowFilter", "true"]);
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("clone");
        let url = format!("file://{}", source.path().display());
        git(parent.path(), &["clone", "-q", "--filter=blob:none", &url, root.to_str().unwrap()]);

        let outside = tempfile::tempdir().unwrap();
        let marker = outside.path().join("ssh-ran");
        let ssh = marker_script(outside.path(), &marker);
        git(&root, &["config", "remote.origin.url", "ssh://example.invalid/repo.git"]);
        git(&root, &["config", "core.sshCommand", ssh.to_str().unwrap()]);

        let project = project_with_agents(&root, &[]);
        let activity = read_git_activity(&root, &project).unwrap().expect("a repo");
        assert!(!activity.line_stats);
        assert_eq!(activity.commits.len(), 2);
        assert!(!marker.exists(), "core.sshCommand from the repository ran");
    }
}
