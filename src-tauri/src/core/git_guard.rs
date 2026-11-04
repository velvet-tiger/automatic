//! Running git on a repository Automatic does not trust.
//!
//! A project folder can be a repository the user downloaded with its `.git`
//! directory, so its `.git/config` is not trusted. [`git_command`] turns off
//! the settings that make a read run another program:
//!
//! - `core.fsmonitor`, with `-c`. Callers also pass the command options that
//!   turn off external diff drivers, textconv and signature display.
//! - Clean, smudge and process filters defined in the repository's own config
//!   (local or worktree scope, includes counted), which git runs when it
//!   re-checks a file's content. Their names come from `git config`, which
//!   runs nothing, and are blanked through `GIT_CONFIG_*` variables, so a name
//!   holding `=` cannot escape the override. Filters from the user's global
//!   config, such as Git LFS, stay on: they are the user's own, and turning
//!   them off makes files look changed.
//! - Lazy fetches in a partial clone, which would run `core.sshCommand` and
//!   write objects. `GIT_NO_LAZY_FETCH` stops them on git 2.44 and later.
//!
//! `--no-optional-locks` stops a read such as `git status` from rewriting the
//! index, so it never races the user's own git commands. Every call also sets
//! `protocol.allow=never` and a no-op `core.sshCommand`, so no transport can
//! run whatever else asks for one.
//!
//! The filter override needs git 2.31 and `GIT_NO_LAZY_FETCH` needs git 2.44.
//! Older git ignores both silently, so [`check_git_supports`] refuses a
//! repository that needs one of them on older git, rather than let it be read
//! unprotected.
//!
//! A caller reads the config with [`REPO_CONFIG_ARGS`] and
//! [`parse_repo_config`], checks it, and then runs every later command through
//! the same builder with that config.
//!
//! This module uses only `std`, so the same file serves the desktop app and
//! the CLI.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// The first git that reads `GIT_CONFIG_COUNT`.
const MIN_VERSION_CONFIG_ENV: GitVersion = GitVersion(2, 31);
/// The first git that reads `GIT_NO_LAZY_FETCH`.
const MIN_VERSION_NO_LAZY_FETCH: GitVersion = GitVersion(2, 44);

/// Variables a calling git process, such as one running a hook, sets to point
/// git at its own repository or to hand it config.
const CALLER_VARIABLES: [&str; 7] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_NAMESPACE",
    "GIT_CONFIG_PARAMETERS",
];

/// A git release as major and minor numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct GitVersion(u32, u32);

impl std::fmt::Display for GitVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.0, self.1)
    }
}

/// The git Automatic runs, found once per run.
#[derive(Debug, Clone)]
pub(crate) struct GitInstall {
    path: PathBuf,
    pub(crate) version: GitVersion,
}

/// The repository settings that change how Automatic reads it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RepoConfig {
    /// Filter drivers defined at local or worktree scope.
    local_filters: Vec<String>,
    pub(crate) partial_clone: bool,
}

/// The `git config` arguments that list every setting [`parse_repo_config`]
/// reads. Run them through [`git_command`] with a default [`RepoConfig`].
/// `git config --get-regexp` exits 1 when nothing matches, which is not an
/// error.
pub(crate) const REPO_CONFIG_ARGS: [&str; 5] = [
    "config",
    "--show-scope",
    "-z",
    "--get-regexp",
    r"^(filter\..*|extensions\.partialclone|remote\..*\.promisor)$",
];

// ── Parsing ───────────────────────────────────────────────────────────────────

/// Parse `git config --show-scope -z --get-regexp` output: records of
/// `scope NUL key LF value NUL`.
pub(crate) fn parse_repo_config(output: &[u8]) -> RepoConfig {
    let text = String::from_utf8_lossy(output);
    let mut config = RepoConfig::default();
    let mut fields = text.split('\0');
    while let (Some(scope), Some(entry)) = (fields.next(), fields.next()) {
        // A key written with no value prints with no line feed.
        let key = entry.split_once('\n').map_or(entry, |(key, _value)| key);
        let repo_scope = scope == "local" || scope == "worktree";
        if !repo_scope {
            continue;
        }
        if let Some(name) = filter_name(key) {
            if !config.local_filters.iter().any(|n| n == name) {
                config.local_filters.push(name.to_string());
            }
        } else if is_partial_clone_key(key) {
            // Any such key counts, whatever its value. A bare key is a
            // boolean true to git, and guessing wrong here would let a fetch
            // through on old git, while guessing wrong the other way only
            // hides line counts.
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

// ── Building commands ─────────────────────────────────────────────────────────

/// A `git` command for `dir` with every setting that could run another
/// program or reach the network turned off. `config` names the repository's
/// own filters to blank. Config the user exported through `GIT_CONFIG_COUNT`
/// is kept.
pub(crate) fn git_command(git: &GitInstall, dir: &Path, config: &RepoConfig) -> Command {
    // Settings the user exported this way are kept: ours follow them, and
    // a later entry wins.
    let inherited = std::env::var("GIT_CONFIG_COUNT")
        .ok()
        .and_then(|n| n.trim().parse::<usize>().ok())
        .unwrap_or(0);
    build_command(git, dir, config, inherited)
}

/// [`git_command`], cut off from a calling git process. For code that can
/// run inside a git hook, where git sets variables that point at the hook's
/// repository and passes its own config down.
///
/// Those variables are removed. Inherited `GIT_CONFIG_*` entries are dropped
/// by numbering ours from 0 and setting `GIT_CONFIG_COUNT` to our count, so
/// git reads ours and nothing past them. Removing `GIT_CONFIG_COUNT` instead
/// would also drop the filter overrides.
pub(crate) fn isolated_git_command(git: &GitInstall, dir: &Path, config: &RepoConfig) -> Command {
    let mut command = build_command(git, dir, config, 0);
    for variable in CALLER_VARIABLES {
        command.env_remove(variable);
    }
    command
}

/// The shared builder. Our `GIT_CONFIG_*` entries start at `first_index`.
fn build_command(git: &GitInstall, dir: &Path, config: &RepoConfig, first_index: usize) -> Command {
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
        // Callers match error text, so it must not be translated.
        .env("LC_ALL", "C")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
        // An allow-list that overrides every `protocol.*` setting, including
        // a per-protocol `protocol.<name>.allow` in the repository's config,
        // which `protocol.allow=never` does not. Works on old git too.
        .env("GIT_ALLOW_PROTOCOL", "none");

    let mut overrides: Vec<(String, &str)> = Vec::new();
    for name in &config.local_filters {
        for variable in ["clean", "smudge", "process"] {
            overrides.push((format!("filter.{name}.{variable}"), ""));
        }
        overrides.push((format!("filter.{name}.required"), "false"));
    }
    command.env("GIT_CONFIG_COUNT", (first_index + overrides.len()).to_string());
    for (offset, (key, value)) in overrides.iter().enumerate() {
        let index = first_index + offset;
        command
            .env(format!("GIT_CONFIG_KEY_{index}"), key)
            .env(format!("GIT_CONFIG_VALUE_{index}"), value);
    }
    command
}

/// Refuse a repository whose protection this git would silently ignore.
/// `purpose` ends the message's last sentence, "Update git to ...".
pub(crate) fn check_git_supports(
    config: &RepoConfig,
    version: GitVersion,
    purpose: &str,
) -> Result<(), String> {
    if !config.local_filters.is_empty() && version < MIN_VERSION_CONFIG_ENV {
        return Err(format!(
            "This repository defines its own git filters. Reading it safely needs git \
             {MIN_VERSION_CONFIG_ENV} or later, and git {version} is installed. Update git to {purpose}."
        ));
    }
    if config.partial_clone && version < MIN_VERSION_NO_LAZY_FETCH {
        return Err(format!(
            "This repository is a partial clone. Reading it without fetching needs git \
             {MIN_VERSION_NO_LAZY_FETCH} or later, and git {version} is installed. Update git to {purpose}."
        ));
    }
    Ok(())
}

// ── Finding git ───────────────────────────────────────────────────────────────

/// The installed git, checked once per run. `None` when there is no usable
/// git.
///
/// On macOS, `/usr/bin/git` is a stub when the Command Line Tools are
/// missing, and running it opens the system's install dialog. The Summary
/// reloads on every window focus, so that dialog would keep returning. The
/// stub is used only when `xcode-select -p` finds a developer directory,
/// which opens nothing. Installing git while the app runs takes effect on
/// the next launch.
pub(crate) fn git_install() -> Option<&'static GitInstall> {
    static INSTALL: OnceLock<Option<GitInstall>> = OnceLock::new();
    INSTALL.get_or_init(detect_git).as_ref()
}

#[cfg(test)]
pub(crate) fn git_available() -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

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
        // Any value counts, failing safe: the cost is only hidden line counts.
        let config = parse_repo_config(b"local\0remote.origin.promisor\nfalse\0");
        assert!(config.partial_clone);
        // A key with no value is printed with no line feed after it.
        let config = parse_repo_config(b"local\0remote.origin.promisor\0");
        assert!(config.partial_clone);
        let config = parse_repo_config(b"global\0remote.origin.promisor\ntrue\0");
        assert!(!config.partial_clone);
    }

    #[test]
    fn every_call_blocks_all_transports() {
        let git = GitInstall { path: PathBuf::from("git"), version: GitVersion(2, 56) };
        let command = git_command(&git, Path::new("."), &RepoConfig::default());
        let envs: Vec<_> = command.get_envs().collect();
        assert!(envs.contains(&(OsStr::new("GIT_ALLOW_PROTOCOL"), Some(OsStr::new("none")))));
        assert!(envs.contains(&(OsStr::new("GIT_NO_LAZY_FETCH"), Some(OsStr::new("1")))));
        let args: Vec<_> = command.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert!(args.windows(2).any(|w| w == ["-c", "protocol.allow=never"]));
        assert!(args.windows(2).any(|w| w == ["-c", "core.sshCommand=false"]));
    }

    #[test]
    fn inherited_config_entries_come_before_the_overrides() {
        let git = GitInstall { path: PathBuf::from("git"), version: GitVersion(2, 56) };
        let config = parse_repo_config(b"local\0filter.evil.clean\nx\0");
        let command = build_command(&git, Path::new("."), &config, 3);
        let env = |key: &str| {
            command
                .get_envs()
                .find(|(k, _)| *k == OsStr::new(key))
                .and_then(|(_, v)| v.map(|v| v.to_string_lossy().into_owned()))
        };
        assert_eq!(env("GIT_CONFIG_COUNT").as_deref(), Some("7"));
        assert_eq!(env("GIT_CONFIG_KEY_3").as_deref(), Some("filter.evil.clean"));
        assert_eq!(env("GIT_CONFIG_VALUE_3").as_deref(), Some(""));
        assert_eq!(env("GIT_CONFIG_KEY_6").as_deref(), Some("filter.evil.required"));
        assert_eq!(env("GIT_CONFIG_VALUE_6").as_deref(), Some("false"));
        assert_eq!(env("GIT_CONFIG_KEY_0"), None);
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
        let purpose = "see its history";
        assert!(check_git_supports(&plain, GitVersion(2, 20), purpose).is_ok());
        assert!(check_git_supports(&filters, GitVersion(2, 30), purpose).is_err());
        assert!(check_git_supports(&filters, GitVersion(2, 31), purpose).is_ok());
        assert!(check_git_supports(&partial, GitVersion(2, 43), purpose).is_err());
        assert!(check_git_supports(&partial, GitVersion(2, 44), purpose).is_ok());
        let err = check_git_supports(&filters, GitVersion(2, 30), purpose).unwrap_err();
        assert!(err.ends_with("git 2.30 is installed. Update git to see its history."), "{err}");
    }
}
