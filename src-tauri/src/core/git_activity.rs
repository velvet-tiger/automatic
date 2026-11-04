//! Recent git history for a project folder, shown on the project Summary.
//!
//! Read-only: Automatic runs `git status` and `git log` in the project's
//! directory and never writes to the repository. Parsing is pure and kept
//! apart from the subprocess calls so it can be tested without git.
//!
//! The folder is often a repository the user cloned or downloaded, so its
//! `.git/config` is not trusted. Every call turns off the settings that make a
//! read run another program: `core.fsmonitor`, external diff drivers,
//! textconv filters and signature checks. `--no-optional-locks` stops
//! `git status` from rewriting the index, so a refresh never races the user's
//! own git commands. Clean and smudge filters named in that config can still
//! run during `git status`. Git has no switch that turns them all off.

use std::path::{Path, PathBuf};
use std::process::Command;
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

/// Field separator inside a commit header.
const FIELD_SEP: char = '\u{1f}';
/// Marks the start of each commit in `git log` output.
const RECORD_SEP: char = '\u{1e}';

/// One commit that touched the project folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitCommit {
    pub hash: String,
    /// Author date, ISO 8601 with the author's UTC offset.
    pub timestamp: String,
    pub author: String,
    pub subject: String,
    /// Lines added inside the project folder. Binary files count as 0.
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
    /// `None` when the branch has no upstream.
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
    /// True when [`MAX_COMMITS`] cut the history short. The oldest commit
    /// then marks where the history starts.
    pub truncated: bool,
    pub history_days: u32,
}

/// A commit as `git log` printed it, with the paths it touched.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedCommit {
    hash: String,
    timestamp: String,
    author: String,
    subject: String,
    insertions: u64,
    deletions: u64,
    paths: Vec<String>,
}

// ── Parsing ───────────────────────────────────────────────────────────────────

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

/// Parse `git log --numstat` output written with [`log_format`].
fn parse_log(output: &str) -> Vec<ParsedCommit> {
    output
        .split(RECORD_SEP)
        .filter_map(parse_commit_record)
        .collect()
}

fn parse_commit_record(record: &str) -> Option<ParsedCommit> {
    let mut lines = record.lines();
    let header = lines.next()?;
    let mut fields = header.splitn(4, FIELD_SEP);
    let hash = fields.next()?.trim();
    let timestamp = fields.next()?;
    let author = fields.next()?;
    let subject = fields.next().unwrap_or("");
    if hash.is_empty() || timestamp.is_empty() {
        return None;
    }

    let mut commit = ParsedCommit {
        hash: hash.to_string(),
        timestamp: timestamp.to_string(),
        author: author.to_string(),
        subject: subject.to_string(),
        insertions: 0,
        deletions: 0,
        paths: Vec::new(),
    };
    for line in lines {
        let mut parts = line.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        // A binary file prints `-` for both counts.
        commit.insertions += added.parse::<u64>().unwrap_or(0);
        commit.deletions += removed.parse::<u64>().unwrap_or(0);
        commit.paths.push(path.to_string());
    }
    Some(commit)
}

// ── Agent config paths ────────────────────────────────────────────────────────

/// Project-relative paths Automatic writes, used to tag commits.
#[derive(Debug, Clone, Default)]
struct AgentConfigPaths {
    files: Vec<String>,
    /// Directory paths with a trailing `/`.
    dirs: Vec<String>,
}

impl AgentConfigPaths {
    /// The same ownership the managed `.gitignore` block uses, so the two
    /// never disagree about which files are Automatic's.
    fn for_project(project_dir: &Path, project: &Project) -> Self {
        let mut paths = Self {
            files: vec![".automatic.json".to_string()],
            dirs: vec![".automatic/".to_string(), ".agents/".to_string()],
        };
        let managed = project
            .agents
            .iter()
            .filter_map(|id| crate::agent::from_id(id))
            .flat_map(|agent| agent.managed_gitignore_paths(project_dir));
        for path in managed {
            let Some(rel) = relative_slash_path(project_dir, &path.path) else {
                continue;
            };
            if path.is_dir {
                paths.dirs.push(format!("{rel}/"));
            } else {
                paths.files.push(rel);
            }
        }
        paths
    }

    fn matches(&self, rel: &str) -> bool {
        self.files.iter().any(|f| f == rel) || self.dirs.iter().any(|d| rel.starts_with(d.as_str()))
    }
}

/// `path` relative to `root`, with `/` separators as git prints them.
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

fn into_commit(parsed: ParsedCommit, config: &AgentConfigPaths) -> GitCommit {
    let touches_agent_config = parsed.paths.iter().any(|p| config.matches(p));
    GitCommit {
        hash: parsed.hash,
        timestamp: parsed.timestamp,
        author: parsed.author,
        subject: parsed.subject,
        insertions: parsed.insertions,
        deletions: parsed.deletions,
        touches_agent_config,
    }
}

// ── Running git ───────────────────────────────────────────────────────────────

fn log_format() -> String {
    format!("--format={RECORD_SEP}%H{FIELD_SEP}%aI{FIELD_SEP}%aN{FIELD_SEP}%s")
}

/// A `git` command for `dir` with every repository setting that could run
/// another program turned off.
fn git_command(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .arg("--no-pager")
        .arg("--no-optional-locks")
        .args(["-c", "core.fsmonitor=false"])
        .args(["-c", "core.quotePath=false"])
        .args(["-c", "color.ui=false"])
        .args(["-c", "log.showSignature=false"])
        .arg("-C")
        .arg(dir)
        // Error text is matched below, so it must not be translated.
        .env("LC_ALL", "C")
        .env("GIT_OPTIONAL_LOCKS", "0");
    command
}

/// Outcome of a git call that may find no repository.
enum GitRun {
    Output(Vec<u8>),
    NotARepository,
}

fn run_git(dir: &Path, args: &[&str], what: &str) -> Result<GitRun, String> {
    let mut command = git_command(dir);
    command.args(args);
    let label = dir.display().to_string();
    let output = run_git_limited(&mut command, GIT_READ_LIMIT, what, &label)?;
    if output.status.success() {
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

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
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
    if !git_available() {
        return Ok(None);
    }

    let status_args = [
        "status",
        "--porcelain=v2",
        "--branch",
        "-z",
        "--ignore-submodules=all",
        "--",
        ".",
    ];
    let repo = match run_git(project_dir, &status_args, "status")? {
        GitRun::NotARepository => return Ok(None),
        GitRun::Output(out) => parse_status(&out),
    };
    if !repo.has_commits {
        return Ok(Some(GitActivity {
            repo,
            commits: Vec::new(),
            truncated: false,
            history_days: HISTORY_DAYS,
        }));
    }

    let since = format!("--since={HISTORY_DAYS}.days.ago");
    let max_count = format!("--max-count={}", MAX_COMMITS + 1);
    let format = log_format();
    let log_args = [
        "log",
        "--no-color",
        "--no-renames",
        "--no-textconv",
        "--no-ext-diff",
        "--no-show-signature",
        "--numstat",
        "--relative",
        since.as_str(),
        max_count.as_str(),
        format.as_str(),
        "HEAD",
        "--",
        ".",
    ];
    let raw = match run_git(project_dir, &log_args, "log")? {
        GitRun::NotARepository => return Ok(None),
        GitRun::Output(out) => out,
    };

    let config = AgentConfigPaths::for_project(project_dir, project);
    let mut parsed = parse_log(&String::from_utf8_lossy(&raw));
    let truncated = parsed.len() > MAX_COMMITS;
    parsed.truncate(MAX_COMMITS);
    let commits = parsed
        .into_iter()
        .map(|c| into_commit(c, &config))
        .collect();
    Ok(Some(GitActivity {
        repo,
        commits,
        truncated,
        history_days: HISTORY_DAYS,
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

    fn header(hash: &str, ts: &str, author: &str, subject: &str) -> String {
        format!("{RECORD_SEP}{hash}{FIELD_SEP}{ts}{FIELD_SEP}{author}{FIELD_SEP}{subject}\n")
    }

    #[test]
    fn parses_commits_with_numstat() {
        let out = format!(
            "{}\n3\t1\tsrc/a.rs\n-\t-\tlogo.png\n10\t0\tCLAUDE.md\n{}",
            header("aaa", "2026-11-01T10:00:00+11:00", "Sam Lee", "fix: thing"),
            header("bbb", "2026-10-30T09:00:00+00:00", "Alex", "merge"),
        );
        let commits = parse_log(&out);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].hash, "aaa");
        assert_eq!(commits[0].author, "Sam Lee");
        assert_eq!(commits[0].subject, "fix: thing");
        assert_eq!(commits[0].insertions, 13);
        assert_eq!(commits[0].deletions, 1);
        assert_eq!(commits[0].paths, vec!["src/a.rs", "logo.png", "CLAUDE.md"]);
        assert_eq!(commits[1].insertions, 0);
        assert!(commits[1].paths.is_empty());
    }

    #[test]
    fn subject_keeps_tabs_and_separators_after_the_third_field() {
        let out = header("ccc", "2026-11-01T10:00:00Z", "A", "feat:\tx\u{1f}y");
        let commits = parse_log(&out);
        assert_eq!(commits[0].subject, "feat:\tx\u{1f}y");
    }

    #[test]
    fn empty_log_parses_to_nothing() {
        assert!(parse_log("").is_empty());
        assert!(parse_log("\n").is_empty());
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
        assert_eq!(state.ahead, Some(2));
        assert_eq!(state.behind, Some(5));
        assert_eq!(state.uncommitted_files, 4);
        assert!(state.has_commits);
    }

    #[test]
    fn status_without_upstream_or_commits() {
        let out = b"# branch.oid (initial)\0# branch.head main\0";
        let state = parse_status(out);
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
    fn agent_config_paths_match_files_and_dirs() {
        let config = AgentConfigPaths {
            files: vec!["CLAUDE.md".into(), ".github/copilot-instructions.md".into()],
            dirs: vec![".claude/".into()],
        };
        assert!(config.matches("CLAUDE.md"));
        assert!(config.matches(".claude/rules/x.md"));
        assert!(config.matches(".github/copilot-instructions.md"));
        assert!(!config.matches(".github/workflows/ci.yml"));
        assert!(!config.matches("docs/CLAUDE.md"));
        assert!(!config.matches(".claudex/file"));
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
        assert_eq!(activity.repo.ahead, None);
        assert!(!activity.truncated);
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
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", "outside"]);
        std::fs::write(sub.join("CLAUDE.md"), "x").unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", "inside"]);

        let project = project_with_agents(&sub, &["claude"]);
        let activity = read_git_activity(&sub, &project).unwrap().expect("a repo");
        assert_eq!(activity.commits.len(), 1);
        assert_eq!(activity.commits[0].subject, "inside");
        // `--relative` makes paths relative to the project folder.
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
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        git(root, &["init", "-q", "-b", "main"]);
        std::fs::write(root.join("a.txt"), "x").unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", "first"]);
        let marker = root.join("fsmonitor-ran");
        let hook = root.join("hook.sh");
        std::fs::write(&hook, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        git(root, &["config", "core.fsmonitor", hook.to_str().unwrap()]);

        let project = project_with_agents(root, &[]);
        read_git_activity(root, &project).unwrap().expect("a repo");
        assert!(!marker.exists(), "core.fsmonitor from the repository ran");
    }
}
