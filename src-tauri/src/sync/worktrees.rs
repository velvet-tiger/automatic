//! Linked git worktrees of a registered project.
//!
//! `git worktree add` checks out tracked files only. Automatic's agent config
//! is usually ignored (see `core::gitignore`), so a new worktree starts with
//! no MCP config, skills or agent folders. A worktree is short-lived, so it is
//! never registered as a project of its own. It receives a copy of its parent
//! project's config through [`sync_to_directory`], which leaves the registry
//! alone.
//!
//! Two things start that copy: [`sync_linked_worktree`], which
//! `automatic projects sync-worktree` calls from inside a worktree, and
//! [`sync_project_worktrees`], which runs after every sync of the parent.
//!
//! The copy uses the parent's config, not the `.automatic.json` committed on
//! the worktree's branch. The worktree's MCP server also resolves to the
//! parent (see `core::project_resolution`), so both agree.
//!
//! A worktree's tracked files belong to its branch. Sync must not change them,
//! so the tracked files among sync's targets are read before the sync, and the
//! ones sync wrote are put back afterwards. See [`TrackedSnapshot`].
//!
//! Putting back a tracked `.gitignore` drops sync's managed ignore block, and
//! an older branch may not have one. So the block also goes into the
//! repository's `info/exclude`, which git applies to every worktree and never
//! commits. Without it, an agent's `git add -A` could commit `.mcp.json` and
//! the env values it holds.
//!
//! The repository can be one the user downloaded with its `.git` folder, so
//! its config is not trusted. Every git call goes through [`WorktreeGit`],
//! which blanks every git filter and turns off the other settings that run a
//! program. See `core::git_guard`.

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

use crate::core::git_guard::{self, GitInstall, RepoConfig};
use crate::core::{self, Project, ProjectMode, ProjectSummary};

use super::engine::{fixed_sync_paths, sync_to_directory};
use super::helpers::project_write_refusal;

/// The outcome of syncing one worktree.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct WorktreeSync {
    /// The worktree folder.
    pub worktree: String,
    /// Display name of the parent project whose config was copied.
    pub project: String,
    /// Files the sync wrote, as the sync engine reports them.
    pub written: Vec<String>,
    /// Tracked files the sync changed, which were put back as the branch
    /// has them. Paths are relative to the worktree.
    pub kept_tracked: Vec<String>,
    /// Tracked files that changed during the sync and could not be put back.
    /// Paths are relative to the worktree. Non-empty means the worktree now
    /// shows changes the user did not make.
    pub changed_tracked: Vec<String>,
    /// Written files git does not ignore in the worktree, relative to it.
    /// Empty when the project does not manage `.gitignore`, because that
    /// project chose to let its config be committed.
    pub unignored: Vec<String>,
    /// Checks that could not run after the files were written.
    pub warnings: Vec<String>,
}

impl WorktreeSync {
    /// One line per thing the user should look at, for logs and the CLI.
    pub fn problems(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if !self.changed_tracked.is_empty() {
            lines.push(format!(
                "these tracked files changed and could not be put back: {}",
                self.changed_tracked.join(", ")
            ));
        }
        if !self.unignored.is_empty() {
            lines.push(format!(
                "git does not ignore these synced files, so they could be committed: {}",
                self.unignored.join(", ")
            ));
        }
        lines.extend(self.warnings.iter().cloned());
        lines
    }
}

/// Sync the parent project's config into the linked worktree at `worktree`.
///
/// Refused when `worktree` is not a linked worktree that git knows about,
/// when its main checkout is not a registered project, when the worktree is
/// registered as a project of its own, and when the parent is read-only.
pub fn sync_linked_worktree(worktree: &Path) -> Result<WorktreeSync, String> {
    let worktree = fs::canonicalize(worktree)
        .map_err(|e| format!("Cannot resolve {}: {}", worktree.display(), e))?;
    let main = main_checkout_of(&worktree)?;
    let summaries = core::get_project_summaries()?;
    if let Some(own) = registered_at(&worktree, &summaries) {
        return Err(format!(
            "{} is registered as the project '{}'. Sync it with `automatic projects sync {}`.",
            worktree.display(),
            own.name,
            core::checkout_ident(own)
        ));
    }
    let parent = parent_summary(&main, &summaries)?;
    let raw = core::read_project(core::checkout_ident(parent))?;
    let project: Project = serde_json::from_str(&raw)
        .map_err(|e| format!("Invalid project data for '{}': {}", parent.name, e))?;
    core::ensure_project_writable(&project)?;
    sync_worktree_for(&project, &worktree, git_guard::git_install())
}

/// Sync every linked worktree of `project`'s folder that is not registered as
/// a project of its own. Called after the parent's own sync succeeded, so a
/// worktree that cannot be synced is logged and skipped: the parent's sync
/// already happened and still reports success.
pub(crate) fn sync_project_worktrees(project: &Project) {
    if !project.manage_gitignore {
        if let Err(e) = remove_excluded_paths(Path::new(&project.directory)) {
            eprintln!(
                "[automatic] worktree sync: could not remove Automatic's block from the git \
                 exclude file of '{}': {}",
                project.name, e
            );
        }
    }
    let worktrees = linked_worktrees(Path::new(&project.directory));
    if worktrees.is_empty() {
        return;
    }
    let summaries = match core::get_project_summaries() {
        Ok(summaries) => summaries,
        Err(e) => {
            eprintln!(
                "[automatic] worktree sync: could not read the project registry, so no \
                 worktree of '{}' was synced: {}",
                project.name, e
            );
            return;
        }
    };
    for worktree in worktrees {
        if registered_at(&worktree, &summaries).is_some() {
            continue;
        }
        match sync_worktree_for(project, &worktree, git_guard::git_install()) {
            Ok(result) => {
                for problem in result.problems() {
                    eprintln!(
                        "[automatic] worktree sync: {}: {}",
                        worktree.display(),
                        problem
                    );
                }
            }
            Err(e) => eprintln!(
                "[automatic] worktree sync: {} was not synced: {}",
                worktree.display(),
                e
            ),
        }
    }
}

/// The linked worktrees of the repository whose main checkout is
/// `project_dir`, as canonical paths. Empty when `project_dir` has no `.git`
/// folder of its own (it is not a main checkout) or git records no worktrees.
/// A record whose folder is gone, or that does not point back at its folder,
/// is skipped.
pub(crate) fn linked_worktrees(project_dir: &Path) -> Vec<PathBuf> {
    if !project_dir.join(".git").is_dir() {
        return Vec::new();
    }
    let Some(common) = core::git_common_dir(project_dir) else {
        return Vec::new();
    };
    let records = match fs::read_dir(common.join("worktrees")) {
        Ok(records) => records,
        Err(e) if e.kind() == ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            eprintln!(
                "[automatic] worktree sync: cannot list the worktrees of {}: {}",
                project_dir.display(),
                e
            );
            return Vec::new();
        }
    };
    let mut found: Vec<PathBuf> = records
        .filter_map(Result::ok)
        .filter_map(|record| worktree_of_record(&record.path()))
        .filter(|dir| core::is_linked_worktree(dir, Some(&common)))
        .collect();
    found.sort();
    found.dedup();
    found
}

/// The worktree folder a `.git/worktrees/<name>` record names. Its `gitdir`
/// file holds the path of the worktree's `.git` file, which newer git can
/// write relative to the record.
fn worktree_of_record(record: &Path) -> Option<PathBuf> {
    let raw = fs::read_to_string(record.join("gitdir")).ok()?;
    let dot_git = record.join(raw.trim());
    fs::canonicalize(dot_git.parent()?).ok()
}

/// The main checkout of the repository that `worktree` is a linked worktree
/// of. A bare repository or a separate git folder has no main checkout to take
/// the config from, so it is refused.
pub(crate) fn main_checkout_of(worktree: &Path) -> Result<PathBuf, String> {
    let not_a_worktree = || format!("{} is not a linked git worktree", worktree.display());
    let common = core::git_common_dir(worktree).ok_or_else(not_a_worktree)?;
    if !core::is_linked_worktree(worktree, Some(&common)) {
        return Err(not_a_worktree());
    }
    match (common.file_name(), common.parent()) {
        (Some(name), Some(main)) if name == ".git" && main.join(".git").is_dir() => {
            Ok(main.to_path_buf())
        }
        _ => Err(format!(
            "The repository of {} keeps its git folder at {}, outside a main checkout, so \
             there is no project to copy the config from",
            worktree.display(),
            common.display()
        )),
    }
}

/// The registered project whose folder is exactly `dir`.
fn registered_at<'a>(dir: &Path, summaries: &'a [ProjectSummary]) -> Option<&'a ProjectSummary> {
    summaries.iter().find(|s| {
        !s.directory.is_empty() && fs::canonicalize(&s.directory).ok().as_deref() == Some(dir)
    })
}

/// The one registered project whose folder is the main checkout `main`.
fn parent_summary<'a>(
    main: &Path,
    summaries: &'a [ProjectSummary],
) -> Result<&'a ProjectSummary, String> {
    let matches: Vec<&ProjectSummary> = summaries
        .iter()
        .filter(|s| {
            !s.directory.is_empty() && fs::canonicalize(&s.directory).ok().as_deref() == Some(main)
        })
        .collect();
    match matches.as_slice() {
        [one] => Ok(one),
        [] => Err(format!(
            "The main checkout {} is not a registered Automatic project",
            main.display()
        )),
        _ => Err(core::ambiguous_name_error(
            &main.display().to_string(),
            &matches.into_iter().cloned().collect::<Vec<_>>(),
        )),
    }
}

/// Copy `project`'s config into `worktree` and put back any tracked file the
/// copy changed. `install` is the git to read the worktree with, `None` when
/// there is no usable git.
fn sync_worktree_for(
    project: &Project,
    worktree: &Path,
    install: Option<&GitInstall>,
) -> Result<WorktreeSync, String> {
    let mut copy = project.clone();
    copy.directory = worktree
        .to_str()
        .ok_or_else(|| format!("{} is not a UTF-8 path", worktree.display()))?
        .to_string();
    let mut result = WorktreeSync {
        worktree: copy.directory.clone(),
        project: project.name.clone(),
        ..Default::default()
    };
    // The same no-op as `sync_project` for a project with no agents.
    if copy.agents.is_empty() {
        return Ok(result);
    }

    let git = WorktreeGit::open(worktree, install)?;

    let effective_dir = match copy.mode {
        ProjectMode::Silent => worktree.join(".automatic").join("silent"),
        ProjectMode::Normal => worktree.to_path_buf(),
    };
    let mut targets = fixed_sync_paths(&copy, worktree, &effective_dir);
    let gitignore = worktree.join(".gitignore");
    targets.push(gitignore.clone());
    let pathspecs = literal_pathspecs(worktree, &targets);
    let snapshot = TrackedSnapshot::take(&git, &pathspecs)?;
    let modified_before = modified_tracked_files(&git, &pathspecs)?;

    let synced = sync_to_directory(&mut copy);
    // Only what sync wrote is put back, so an edit someone else makes during
    // the sync is left alone. Sync does not list `.gitignore` among its
    // writes. After a failed sync the written list is unknown, so every
    // changed entry is put back.
    let wrote: Option<Vec<PathBuf>> = synced
        .as_ref()
        .ok()
        .map(|written| written.iter().map(PathBuf::from).collect());
    let restore = snapshot.restore(|path| may_restore(path, &gitignore, wrote.as_deref()));
    let written = match synced {
        Ok(written) => written,
        Err(e) if restore.failed.is_empty() => return Err(e),
        Err(e) => {
            return Err(format!(
                "{e}. These tracked files changed and could not be put back: {}",
                restore.failed.join(", ")
            ))
        }
    };
    let kept: HashSet<PathBuf> = restore.kept.iter().map(|rel| worktree.join(rel)).collect();
    result.written = written
        .into_iter()
        .filter(|path| !kept.contains(Path::new(path)))
        .collect();

    if copy.manage_gitignore {
        if let Err(e) = exclude_managed_paths(worktree, &copy) {
            result.warnings.push(e);
        }
        match unignored_files(&git, &result.written) {
            Ok(unignored) => result.unignored = unignored,
            Err(e) => result.warnings.push(format!(
                "Could not check which synced files git ignores: {e}"
            )),
        }
    } else if let Some(common) = core::git_common_dir(worktree) {
        if let Err(e) = update_exclude(&common, None) {
            result.warnings.push(e);
        }
    }

    let mut changed: BTreeSet<String> = restore.failed.into_iter().collect();
    match modified_tracked_files(&git, &pathspecs) {
        Ok(after) => changed.extend(
            after
                .difference(&modified_before)
                .filter(|rel| !restore.kept.contains(rel))
                .cloned(),
        ),
        Err(e) => result
            .warnings
            .push(format!("Could not check tracked files after the sync: {e}")),
    }
    result.kept_tracked = restore.kept;
    result.changed_tracked = changed.into_iter().collect();
    Ok(result)
}

/// Whether the restore may put `path` back. Sync reports some writes as a
/// folder (a skill), which covers every file under it. Sync does not report
/// `.gitignore`. After a failed sync, `wrote` is `None` and everything may be
/// put back.
fn may_restore(path: &Path, gitignore: &Path, wrote: Option<&[PathBuf]>) -> bool {
    path == gitignore || wrote.map_or(true, |wrote| wrote.iter().any(|w| path.starts_with(w)))
}

/// Write `project`'s managed ignore patterns into the `info/exclude` of the
/// repository `worktree` belongs to, replacing an earlier Automatic block.
/// The patterns are rooted, so they apply at the top of every worktree.
fn exclude_managed_paths(worktree: &Path, project: &Project) -> Result<(), String> {
    let common = core::git_common_dir(worktree)
        .ok_or_else(|| format!("Cannot find the git folder of {}", worktree.display()))?;
    let block =
        core::gitignore::render_block(&core::gitignore::project_patterns(worktree, project));
    update_exclude(&common, Some(&block))
}

/// Remove Automatic's block from the `info/exclude` of the repository whose
/// main checkout is `project_dir`. The block is only written while the
/// project manages `.gitignore`, so once the user turns that off it must go,
/// or the files stay hidden in every checkout.
fn remove_excluded_paths(project_dir: &Path) -> Result<(), String> {
    if !project_dir.join(".git").is_dir() {
        return Ok(());
    }
    match core::git_common_dir(project_dir) {
        Some(common) => update_exclude(&common, None),
        None => Ok(()),
    }
}

/// Replace Automatic's block in `<common>/info/exclude` with `block`, or
/// remove it when `block` is `None`. Writes nothing when the file would not
/// change.
fn update_exclude(common: &Path, block: Option<&str>) -> Result<(), String> {
    let info = common.join("info");
    let path = info.join("exclude");
    for candidate in [&info, &path] {
        if fs::symlink_metadata(candidate).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(format!(
                "{} is a link, so Automatic will not write its ignore block there",
                candidate.display()
            ));
        }
    }
    let existing = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("Cannot read {}: {}", path.display(), e)),
    };
    let mut content = without_managed_block(&existing)
        .trim_end_matches('\n')
        .to_string();
    if let Some(block) = block {
        if !content.is_empty() {
            content.push_str("\n\n");
        }
        content.push_str(block);
    } else if !content.is_empty() {
        content.push('\n');
    }
    if content == existing || (block.is_none() && existing.is_empty()) {
        return Ok(());
    }
    fs::create_dir_all(&info).map_err(|e| format!("Cannot create {}: {}", info.display(), e))?;
    fs::write(&path, content).map_err(|e| format!("Cannot write {}: {}", path.display(), e))
}

/// `text` with every Automatic-managed block removed. An unterminated block
/// runs to the end of the text.
fn without_managed_block(text: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in text.lines() {
        if line.trim_end() == core::gitignore::BEGIN_MARKER {
            inside = true;
        } else if inside && line.trim_end() == core::gitignore::END_MARKER {
            inside = false;
        } else if !inside {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// The files among `written` that are neither tracked nor ignored.
fn unignored_files(git: &WorktreeGit, written: &[String]) -> Result<Vec<String>, String> {
    let targets: Vec<PathBuf> = written.iter().map(PathBuf::from).collect();
    let pathspecs = literal_pathspecs(git.worktree, &targets);
    if pathspecs.is_empty() {
        return Ok(Vec::new());
    }
    let mut args = vec![
        "--others".to_string(),
        "--exclude-standard".to_string(),
        "--".to_string(),
    ];
    args.extend(pathspecs);
    ls_files(git, &args)
}

/// `targets` as literal git pathspecs relative to `root`. A target outside
/// `root`, or `root` itself, is left out.
fn literal_pathspecs(root: &Path, targets: &[PathBuf]) -> Vec<String> {
    let mut pathspecs: Vec<String> = targets
        .iter()
        .filter_map(|target| target.strip_prefix(root).ok())
        .filter(|rel| !rel.as_os_str().is_empty())
        .filter_map(|rel| rel.to_str())
        .map(|rel| format!(":(literal){}", rel.replace('\\', "/")))
        .collect();
    pathspecs.sort();
    pathspecs.dedup();
    pathspecs
}

/// The git that reads one worktree, with its repository's config already
/// read. Made once per worktree sync.
///
/// `git ls-files --modified` reads a file's content when stat data cannot
/// settle whether it changed: the file has the indexed size but a new time
/// or inode, or its index entry is racy. Reading content runs the clean
/// filter git's config names for that file. So every call goes
/// through [`git_guard::isolated_git_command`], which blanks those filters.
/// It also drops the variables a calling git sets, since this can run inside
/// a git hook.
struct WorktreeGit<'a> {
    worktree: &'a Path,
    install: &'a GitInstall,
    config: RepoConfig,
}

/// Ends the "Update git to ..." sentence of a refusal.
const GIT_PURPOSE: &str = "sync its worktrees";

impl<'a> WorktreeGit<'a> {
    /// Read the repository config in `worktree` with `install`, so config at
    /// worktree scope counts. Fails when there is no usable git (`install` is
    /// `None`), or when it would ignore or cannot apply a protection this
    /// repository needs.
    fn open(worktree: &'a Path, install: Option<&'a GitInstall>) -> Result<Self, String> {
        let install = install.ok_or_else(|| {
            format!(
                "No usable git was found, so the tracked files of {} cannot be protected",
                worktree.display()
            )
        })?;
        let config = git_guard::read_checked_repo_config(install, worktree, GIT_PURPOSE)?;
        Ok(Self {
            worktree,
            install,
            config,
        })
    }

    /// A `git` command run in the worktree.
    fn command(&self) -> Result<Command, String> {
        git_guard::isolated_git_command(self.install, self.worktree, &self.config)
    }
}

/// Run `git ls-files -z` in the worktree with `args`, and return the listed
/// paths, relative to the worktree.
fn ls_files(git: &WorktreeGit, args: &[String]) -> Result<Vec<String>, String> {
    let worktree = git.worktree;
    let output = git
        .command()?
        .args(["ls-files", "-z"])
        .args(args)
        .output()
        .map_err(|e| format!("Cannot run git in {}: {}", worktree.display(), e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // `git config` in `WorktreeGit::open` runs even when git refuses the
        // folder's owner, reading only the user's config, so the refusal
        // shows here first.
        if let Some(message) = git_guard::unsafe_ownership_message(&stderr, worktree) {
            return Err(message);
        }
        return Err(format!(
            "git ls-files failed in {}: {}",
            worktree.display(),
            stderr.trim()
        ));
    }
    output
        .stdout
        .split(|b| *b == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            String::from_utf8(path.to_vec()).map_err(|_| {
                format!(
                    "{} tracks a file whose name is not UTF-8, so its tracked files cannot \
                     be protected",
                    worktree.display()
                )
            })
        })
        .collect()
}

/// Tracked files under `pathspecs` whose working copy differs from the
/// index, or is missing.
fn modified_tracked_files(
    git: &WorktreeGit,
    pathspecs: &[String],
) -> Result<BTreeSet<String>, String> {
    if pathspecs.is_empty() {
        return Ok(BTreeSet::new());
    }
    let mut args = vec!["--modified".to_string(), "--".to_string()];
    args.extend(pathspecs.iter().cloned());
    Ok(ls_files(git, &args)?.into_iter().collect())
}

/// How one tracked path looked before the sync.
#[derive(Debug, PartialEq, Eq)]
enum TrackedState {
    File {
        bytes: Vec<u8>,
        permissions: fs::Permissions,
    },
    Link(PathBuf),
    Missing,
}

/// The tracked files under sync's target paths, read before the sync so they
/// can be put back afterwards. Restoring from the bytes on disk, rather than
/// from git, keeps edits the user had not committed yet.
struct TrackedSnapshot {
    root: PathBuf,
    entries: Vec<(String, TrackedState)>,
}

/// What [`TrackedSnapshot::restore`] did.
struct Restored {
    kept: Vec<String>,
    failed: Vec<String>,
}

impl TrackedSnapshot {
    /// Read every tracked file in the worktree that `pathspecs` match.
    fn take(git: &WorktreeGit, pathspecs: &[String]) -> Result<Self, String> {
        let root = git.worktree;
        if pathspecs.is_empty() {
            return Ok(Self {
                root: root.to_path_buf(),
                entries: Vec::new(),
            });
        }
        let mut args = vec!["--".to_string()];
        args.extend(pathspecs.iter().cloned());
        let entries = ls_files(git, &args)?
            .into_iter()
            .map(|rel| {
                let state = read_state(&root.join(&rel))?;
                Ok((rel, state))
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self {
            root: root.to_path_buf(),
            entries,
        })
    }

    /// Put back every entry that `may_restore` accepts and that no longer
    /// looks as it did. Each path is checked with [`project_write_refusal`]
    /// first, so a restore never writes through a link out of the worktree.
    fn restore(self, may_restore: impl Fn(&Path) -> bool) -> Restored {
        let mut restored = Restored {
            kept: Vec::new(),
            failed: Vec::new(),
        };
        for (rel, before) in self.entries {
            let path = self.root.join(&rel);
            if !may_restore(&path) {
                continue;
            }
            match read_state(&path) {
                Ok(now) if now == before => continue,
                _ => {}
            }
            let outcome = match project_write_refusal(&self.root, &path) {
                Some(reason) => Err(reason),
                None => write_state(&path, &before),
            };
            match outcome {
                Ok(()) => restored.kept.push(rel),
                Err(e) => {
                    eprintln!(
                        "[automatic] worktree sync: could not put back {}: {}",
                        path.display(),
                        e
                    );
                    restored.failed.push(rel);
                }
            }
        }
        restored
    }
}

fn read_state(path: &Path) -> Result<TrackedState, String> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(TrackedState::Missing),
        Err(e) => return Err(format!("Cannot read {}: {}", path.display(), e)),
    };
    if meta.file_type().is_symlink() {
        return fs::read_link(path)
            .map(TrackedState::Link)
            .map_err(|e| format!("Cannot read the link {}: {}", path.display(), e));
    }
    if !meta.is_file() {
        return Err(format!(
            "{} is tracked by git but is not a file or a link",
            path.display()
        ));
    }
    let bytes = fs::read(path).map_err(|e| format!("Cannot read {}: {}", path.display(), e))?;
    Ok(TrackedState::File {
        bytes,
        permissions: meta.permissions(),
    })
}

/// Make `path` look as `state` describes. A file or link is written beside
/// `path` and renamed over it, so a crash never leaves the path empty. A
/// folder at `path` may hold files the user wrote, so it is never replaced.
fn write_state(path: &Path, state: &TrackedState) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => return Err(format!("{} is now a folder", path.display())),
        Ok(_) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(format!("Cannot read {}: {}", path.display(), e)),
    }
    let temp = match state {
        TrackedState::Missing => {
            return match fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
                Err(e) => Err(format!("Cannot remove {}: {}", path.display(), e)),
            };
        }
        TrackedState::File { .. } | TrackedState::Link(_) => temp_path_beside(path)?,
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create {}: {}", parent.display(), e))?;
    }
    let _ = fs::remove_file(&temp);
    let staged = match state {
        TrackedState::File { bytes, permissions } => fs::write(&temp, bytes)
            .and_then(|()| fs::set_permissions(&temp, permissions.clone()))
            .map_err(|e| format!("Cannot write {}: {}", temp.display(), e)),
        TrackedState::Link(target) => make_link(target, &temp),
        TrackedState::Missing => unreachable!("handled above"),
    };
    let renamed = staged.and_then(|()| {
        fs::rename(&temp, path)
            .map_err(|e| format!("Cannot move {} into place: {}", temp.display(), e))
    });
    if renamed.is_err() {
        let _ = fs::remove_file(&temp);
    }
    renamed
}

/// A temporary name in the same folder as `path`, so a rename stays on one
/// filesystem.
fn temp_path_beside(path: &Path) -> Result<PathBuf, String> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("{} has no file name", path.display()))?;
    Ok(path.with_file_name(format!(".{name}.automatic-restore-{}", std::process::id())))
}

#[cfg(unix)]
fn make_link(target: &Path, path: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(target, path)
        .map_err(|e| format!("Cannot create the link {}: {}", path.display(), e))
}

#[cfg(windows)]
fn make_link(target: &Path, path: &Path) -> Result<(), String> {
    std::os::windows::fs::symlink_file(target, path)
        .map_err(|e| format!("Cannot create the link {}: {}", path.display(), e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::git_guard::GitVersion;

    fn git(dir: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
            .args([
                "-c",
                "commit.gpgsign=false",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(args)
            .current_dir(dir)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {:?}: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// A registered Claude project at `<tmp>/main` with a committed
    /// `.gitignore` and `CLAUDE.md`, and a linked worktree of it at
    /// `<tmp>/wt`, outside the main checkout.
    struct Fixture {
        _tmp: tempfile::TempDir,
        main: PathBuf,
        worktree: PathBuf,
    }

    const BRANCH_CLAUDE_MD: &str = "# The branch's own instructions\n";
    const BRANCH_GITIGNORE: &str = "target/\n";

    fn fixture(name: &str) -> Fixture {
        fixture_with(name, &[], |_| {})
    }

    /// [`fixture`], with extra `git worktree add` flags (before `add`) and a
    /// change to the registered project.
    fn fixture_with(
        name: &str,
        worktree_flags: &[&str],
        change: impl FnOnce(&mut Project),
    ) -> Fixture {
        let tmp = tempfile::tempdir().expect("tempdir");
        let base = fs::canonicalize(tmp.path()).expect("canonical");
        let main = base.join("main");
        fs::create_dir_all(&main).expect("mkdir");
        git(&main, &["init", "-q"]);
        fs::write(main.join("CLAUDE.md"), BRANCH_CLAUDE_MD).expect("write");
        fs::write(main.join(".gitignore"), BRANCH_GITIGNORE).expect("write");
        git(&main, &["add", "CLAUDE.md", ".gitignore"]);
        git(&main, &["commit", "-q", "-m", "init"]);
        let mut add: Vec<&str> = worktree_flags.to_vec();
        add.extend(["worktree", "add", "-q", "-b", "feature", "../wt"]);
        git(&main, &add);
        let worktree = base.join("wt");

        let mut project = Project {
            name: name.into(),
            directory: main.to_str().unwrap().into(),
            agents: vec!["claude".into()],
            manage_gitignore: true,
            ..Default::default()
        };
        change(&mut project);
        core::save_project(name, &serde_json::to_string(&project).unwrap()).expect("register");
        Fixture {
            _tmp: tmp,
            main,
            worktree,
        }
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
    }

    #[test]
    fn a_worktree_gets_the_parents_config_and_keeps_its_tracked_files() {
        let _home = core::temp_test_home();
        let f = fixture("wt-config");

        let result = sync_linked_worktree(&f.worktree).expect("sync worktree");

        assert_eq!(result.project, "wt-config");
        assert!(!result.written.is_empty(), "the sync wrote nothing");
        assert!(
            f.worktree.join(".mcp.json").is_file(),
            "no MCP config in the worktree"
        );
        assert_eq!(read(&f.worktree.join("CLAUDE.md")), BRANCH_CLAUDE_MD);
        assert_eq!(read(&f.worktree.join(".gitignore")), BRANCH_GITIGNORE);
        assert!(
            result.kept_tracked.contains(&".gitignore".to_string()),
            "{result:?}"
        );
        assert!(result.changed_tracked.is_empty(), "{result:?}");
        assert_eq!(status(&f.worktree), "");
        // No registry entry for the worktree.
        let summaries = core::get_project_summaries().unwrap();
        assert!(registered_at(&f.worktree, &summaries).is_none());
    }

    #[test]
    fn uncommitted_edits_to_tracked_files_survive_the_sync() {
        let _home = core::temp_test_home();
        let f = fixture("wt-edits");
        let edited = "# Edited in the worktree, not committed\n";
        fs::write(f.worktree.join("CLAUDE.md"), edited).unwrap();

        let result = sync_linked_worktree(&f.worktree).expect("sync worktree");

        assert_eq!(read(&f.worktree.join("CLAUDE.md")), edited);
        assert!(result.changed_tracked.is_empty(), "{result:?}");
    }

    #[test]
    fn a_parent_sync_also_syncs_its_worktrees() {
        let _home = core::temp_test_home();
        let f = fixture("wt-fanout");
        let raw = core::read_project("wt-fanout").unwrap();
        let mut project: Project = serde_json::from_str(&raw).unwrap();

        super::super::sync_project_without_autodetect(&mut project).expect("parent sync");

        assert!(f.main.join(".mcp.json").is_file());
        assert!(
            f.worktree.join(".mcp.json").is_file(),
            "the worktree was not synced"
        );
        assert_eq!(read(&f.worktree.join("CLAUDE.md")), BRANCH_CLAUDE_MD);
        assert_eq!(status(&f.worktree), "");
    }

    #[test]
    fn a_broken_worktree_does_not_fail_the_parent_sync() {
        let _home = core::temp_test_home();
        let f = fixture("wt-broken");
        // A folder where git expects a tracked file cannot be snapshotted.
        fs::remove_file(f.worktree.join(".gitignore")).unwrap();
        fs::create_dir(f.worktree.join(".gitignore")).unwrap();
        let raw = core::read_project("wt-broken").unwrap();
        let mut project: Project = serde_json::from_str(&raw).unwrap();

        super::super::sync_project_without_autodetect(&mut project).expect("parent sync");

        assert!(f.main.join(".mcp.json").is_file());
        assert!(
            !f.worktree.join(".mcp.json").exists(),
            "the broken worktree was written"
        );
        assert!(sync_linked_worktree(&f.worktree).is_err());
    }

    #[test]
    fn a_folder_that_is_not_a_linked_worktree_is_refused() {
        let _home = core::temp_test_home();
        let f = fixture("wt-plain");
        let plain = tempfile::tempdir().unwrap();

        let err = sync_linked_worktree(plain.path()).unwrap_err();
        assert!(err.contains("not a linked git worktree"), "{err}");
        // The main checkout is a repository, not a linked worktree.
        let err = sync_linked_worktree(&f.main).unwrap_err();
        assert!(err.contains("not a linked git worktree"), "{err}");
    }

    #[test]
    fn a_worktree_whose_main_checkout_is_not_registered_is_refused() {
        let _home = core::temp_test_home();
        let f = fixture("wt-unregistered");
        core::delete_project("wt-unregistered").expect("unregister");

        let err = sync_linked_worktree(&f.worktree).unwrap_err();
        assert!(
            err.contains("is not a registered Automatic project"),
            "{err}"
        );
        assert!(!f.worktree.join(".mcp.json").exists());
    }

    #[test]
    fn a_worktree_registered_as_its_own_project_is_refused_and_skipped() {
        let _home = core::temp_test_home();
        let f = fixture("wt-parent");
        let own = Project {
            name: "wt-own".into(),
            directory: f.worktree.to_str().unwrap().into(),
            ..Default::default()
        };
        core::save_project("wt-own", &serde_json::to_string(&own).unwrap()).unwrap();

        let err = sync_linked_worktree(&f.worktree).unwrap_err();
        assert!(err.contains("registered as the project 'wt-own'"), "{err}");

        let raw = core::read_project("wt-parent").unwrap();
        let mut parent: Project = serde_json::from_str(&raw).unwrap();
        super::super::sync_project_without_autodetect(&mut parent).expect("parent sync");
        assert!(
            !f.worktree.join(".mcp.json").exists(),
            "the parent wrote into a registered worktree"
        );
    }

    #[test]
    fn a_read_only_parent_is_refused() {
        let _home = core::temp_test_home();
        let f = fixture("wt-read-only");
        core::set_project_read_only("wt-read-only", true).expect("read-only");

        assert!(sync_linked_worktree(&f.worktree).is_err());
        assert!(!f.worktree.join(".mcp.json").exists());
    }

    #[test]
    fn linked_worktrees_lists_only_worktrees_git_knows() {
        let _home = core::temp_test_home();
        let f = fixture("wt-list");

        assert_eq!(linked_worktrees(&f.main), vec![f.worktree.clone()]);
        // A worktree is not a main checkout, so it lists nothing.
        assert!(linked_worktrees(&f.worktree).is_empty());

        fs::remove_dir_all(&f.worktree).unwrap();
        assert!(
            linked_worktrees(&f.main).is_empty(),
            "a removed worktree is still listed"
        );
    }

    fn status(worktree: &Path) -> String {
        let output = Command::new("git")
            .args(["status", "--porcelain", "--untracked-files=all"])
            .current_dir(worktree)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .output()
            .expect("git status");
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    #[test]
    fn synced_files_are_ignored_even_when_the_branch_gitignore_has_no_block() {
        let _home = core::temp_test_home();
        let f = fixture("wt-ignored");

        let result = sync_linked_worktree(&f.worktree).expect("sync worktree");

        assert_eq!(read(&f.worktree.join(".gitignore")), BRANCH_GITIGNORE);
        assert!(result.unignored.is_empty(), "{result:?}");
        assert!(result.warnings.is_empty(), "{result:?}");
        assert_eq!(status(&f.worktree), "", "the worktree shows changes");
        let exclude = read(&f.main.join(".git/info/exclude"));
        assert!(exclude.contains(core::gitignore::BEGIN_MARKER), "{exclude}");
        assert!(exclude.contains("/.mcp.json"), "{exclude}");

        // A second sync replaces the block instead of adding another.
        sync_linked_worktree(&f.worktree).expect("sync again");
        let again = read(&f.main.join(".git/info/exclude"));
        assert_eq!(
            again.matches(core::gitignore::BEGIN_MARKER).count(),
            1,
            "{again}"
        );
    }

    #[test]
    fn a_tracked_mcp_config_on_the_branch_is_left_alone() {
        let _home = core::temp_test_home();
        let f = fixture("wt-tracked-mcp");
        let branch_mcp = "{\"mcpServers\":{\"branch\":{\"command\":\"x\"}}}\n";
        fs::write(f.worktree.join(".mcp.json"), branch_mcp).unwrap();
        git(&f.worktree, &["add", ".mcp.json"]);
        git(&f.worktree, &["commit", "-q", "-m", "branch mcp"]);

        let result = sync_linked_worktree(&f.worktree).expect("sync worktree");

        assert_eq!(read(&f.worktree.join(".mcp.json")), branch_mcp);
        assert!(
            result.kept_tracked.contains(&".mcp.json".to_string()),
            "{result:?}"
        );
        assert!(
            !result.written.iter().any(|w| w.ends_with(".mcp.json")),
            "{result:?}"
        );
        assert_eq!(status(&f.worktree), "");
    }

    #[test]
    fn a_worktree_with_relative_git_records_is_found() {
        let _home = core::temp_test_home();
        let f = fixture_with(
            "wt-relative",
            &["-c", "worktree.useRelativePaths=true"],
            |_| {},
        );

        assert_eq!(linked_worktrees(&f.main), vec![f.worktree.clone()]);
        sync_linked_worktree(&f.worktree).expect("sync worktree");
        assert!(f.worktree.join(".mcp.json").is_file());
    }

    #[test]
    fn a_silent_project_writes_only_under_the_silent_folder() {
        let _home = core::temp_test_home();
        let f = fixture_with("wt-silent", &[], |p| p.mode = ProjectMode::Silent);

        let result = sync_linked_worktree(&f.worktree).expect("sync worktree");

        let silent = f.worktree.join(".automatic").join("silent");
        assert!(!result.written.is_empty());
        assert!(
            result
                .written
                .iter()
                .all(|w| Path::new(w).starts_with(&silent)),
            "{result:?}"
        );
        assert!(!f.worktree.join(".mcp.json").exists());
        assert_eq!(read(&f.worktree.join(".gitignore")), BRANCH_GITIGNORE);
        assert_eq!(status(&f.worktree), "");
    }

    #[test]
    fn a_written_folder_covers_the_tracked_files_under_it() {
        let root = Path::new("/wt");
        let gitignore = root.join(".gitignore");
        let skill = root.join(".claude/skills/foo");
        let wrote = vec![skill.clone(), root.join(".mcp.json")];

        assert!(may_restore(
            &skill.join("SKILL.md"),
            &gitignore,
            Some(&wrote)
        ));
        assert!(may_restore(
            &root.join(".mcp.json"),
            &gitignore,
            Some(&wrote)
        ));
        assert!(may_restore(&gitignore, &gitignore, Some(&wrote)));
        assert!(!may_restore(
            &root.join("CLAUDE.md"),
            &gitignore,
            Some(&wrote)
        ));
        assert!(!may_restore(
            &root.join(".claude/skills/foobar/SKILL.md"),
            &gitignore,
            Some(&wrote)
        ));
        assert!(may_restore(&root.join("CLAUDE.md"), &gitignore, None));
    }

    /// A shell script that creates `marker` and passes its input through, as
    /// a clean filter does.
    #[cfg(unix)]
    fn marker_script(dir: &Path, marker: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let script = dir.join("payload.sh");
        fs::write(
            &script,
            format!("#!/bin/sh\ntouch '{}'\ncat\n", marker.display()),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        script
    }

    #[cfg(unix)]
    fn set_mtime(path: &Path, time: std::time::SystemTime) {
        fs::File::options()
            .write(true)
            .open(path)
            .and_then(|file| file.set_modified(time))
            .unwrap_or_else(|e| panic!("set the time of {}: {e}", path.display()));
    }

    #[test]
    fn a_worktree_command_drops_the_callers_git_settings_and_keeps_the_overrides() {
        let git = WorktreeGit {
            worktree: Path::new("/wt"),
            install: git_guard::git_install().expect("git is installed"),
            config: git_guard::parse_repo_config(b"local\0filter.evil.clean\nx\0"),
        };
        let command = git.command().expect("command");
        // `Some(None)` is a removed variable, `None` one left as inherited.
        let env = |key: &str| {
            command
                .get_envs()
                .find(|(k, _)| *k == std::ffi::OsStr::new(key))
                .map(|(_, v)| v.map(|v| v.to_string_lossy().into_owned()))
        };
        // Every variable git itself names as pointing at a repository.
        let tmp = tempfile::tempdir().unwrap();
        git_init(tmp.path());
        let listed = Command::new("git")
            .args(["rev-parse", "--local-env-vars"])
            .current_dir(tmp.path())
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .output()
            .expect("git rev-parse");
        assert!(listed.status.success(), "{listed:?}");
        let listed = String::from_utf8(listed.stdout).unwrap();
        let mut removed: Vec<&str> = listed
            .lines()
            .filter(|v| *v != "GIT_CONFIG_COUNT")
            .collect();
        assert!(removed.contains(&"GIT_CONFIG"), "{listed}");
        removed.push("GIT_NAMESPACE");
        for variable in removed {
            assert_eq!(env(variable), Some(None), "{variable} is not removed");
        }
        // Our four overrides, numbered from 0, whatever the caller exported.
        assert_eq!(env("GIT_CONFIG_COUNT"), Some(Some("4".into())));
        assert_eq!(
            env("GIT_CONFIG_KEY_0"),
            Some(Some("filter.evil.clean".into()))
        );
        assert_eq!(env("GIT_CONFIG_VALUE_0"), Some(Some(String::new())));
        assert_eq!(
            env("GIT_CONFIG_KEY_3"),
            Some(Some("filter.evil.required".into()))
        );
        assert_eq!(env("GIT_CONFIG_VALUE_3"), Some(Some("false".into())));
        let args: Vec<String> = command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.windows(2).any(|w| w == ["-C", "/wt"]), "{args:?}");
        assert!(
            args.windows(2).any(|w| w == ["-c", "core.fsmonitor="]),
            "{args:?}"
        );
    }

    fn git_init(dir: &Path) {
        git(dir, &["init", "-q"]);
    }

    #[test]
    fn a_worktree_sync_without_a_usable_git_is_refused() {
        let err = WorktreeGit::open(Path::new("/wt"), None)
            .err()
            .expect("refused");
        assert!(err.contains("No usable git was found"), "{err}");
    }

    #[test]
    fn git_too_old_for_the_repositorys_filters_is_refused() {
        let _home = core::temp_test_home();
        let f = fixture("wt-old-git");
        git(&f.main, &["config", "filter.evil.clean", "false"]);
        let installed = git_guard::git_install().expect("git is installed");

        let old = installed.with_version(GitVersion::new(2, 30));
        let err = WorktreeGit::open(&f.worktree, Some(&old))
            .err()
            .expect("refused");
        assert!(
            err.contains("Git filters are set in your git config or in this repository's"),
            "{err}"
        );
        assert!(err.ends_with("Update git to sync its worktrees."), "{err}");

        let new_enough = installed.with_version(GitVersion::new(2, 31));
        assert!(WorktreeGit::open(&f.worktree, Some(&new_enough)).is_ok());
    }

    #[test]
    fn git_without_show_scope_is_refused_with_a_clear_message() {
        let installed = git_guard::git_install().expect("git is installed");
        let old = installed.with_version(GitVersion::new(2, 25));
        let err = WorktreeGit::open(Path::new("/wt"), Some(&old))
            .err()
            .expect("refused");
        assert!(
            err.contains("needs git 2.26 or later, and git 2.25 is installed"),
            "{err}"
        );
        assert!(err.ends_with("Update git to sync its worktrees."), "{err}");
    }

    /// A tracked file whose change only a content read shows, and the
    /// script a filter on it runs.
    #[cfg(unix)]
    struct ArmedFilter {
        /// Holds the script and the marker, outside the repository.
        outside: tempfile::TempDir,
        script: PathBuf,
        /// Created when the script runs.
        marker: PathBuf,
        /// The worktree's `CLAUDE.md` as edited.
        edited: String,
    }

    /// Make `CLAUDE.md` in `f`'s worktree a racy, same-size edit, and give it
    /// the filter whose attribute value is `attr` in the repository's
    /// `info/attributes`. `define` defines that filter.
    #[cfg(unix)]
    fn arm_filter(f: &Fixture, attr: &[u8], define: impl FnOnce(&ArmedFilter)) -> ArmedFilter {
        let target = f.worktree.join("CLAUDE.md");
        // 2099-01-01. An index entry whose time is not older than the index
        // file is racy, so `git ls-files --modified` must read the file.
        let future = std::time::UNIX_EPOCH + std::time::Duration::from_secs(4_070_908_800);
        set_mtime(&target, future);
        // Record that time in the index before any filter exists.
        git(&f.worktree, &["add", "CLAUDE.md"]);

        let outside = tempfile::tempdir().unwrap();
        let marker = outside.path().join("filter-ran");
        let script = marker_script(outside.path(), &marker);
        let edited = BRANCH_CLAUDE_MD.replace("instructions", "instructionz");
        assert_eq!(edited.len(), BRANCH_CLAUDE_MD.len());
        let armed = ArmedFilter {
            outside,
            script,
            marker,
            edited,
        };
        define(&armed);
        git(&f.main, &["config", "core.trustctime", "false"]);
        fs::create_dir_all(f.main.join(".git/info")).unwrap();
        let mut attributes = b"CLAUDE.md filter=".to_vec();
        attributes.extend_from_slice(attr);
        attributes.push(b'\n');
        fs::write(f.main.join(".git/info/attributes"), attributes).unwrap();
        // Same size and time, other content: only reading the file shows it.
        fs::write(&target, &armed.edited).unwrap();
        set_mtime(&target, future);
        armed
    }

    /// Append a filter section to the repository's config as raw bytes, so
    /// its name can hold any byte. `header` is the `[filter "..."]` line.
    #[cfg(unix)]
    fn append_filter_section(f: &Fixture, header: &[u8], script: &Path) {
        use std::io::Write;
        let mut config = fs::File::options()
            .append(true)
            .open(f.main.join(".git/config"))
            .unwrap();
        config.write_all(header).unwrap();
        writeln!(config, "\n\tclean = {}", script.display()).unwrap();
    }

    /// Show that the setup reaches the filter: git without the override runs
    /// it. A filter that does not speak git's protocol can make git fail, so
    /// only the marker counts.
    #[cfg(unix)]
    fn assert_plain_git_runs_the_filter(f: &Fixture, armed: &ArmedFilter) {
        let _ = Command::new("git")
            .args(["ls-files", "--modified", "--", "CLAUDE.md"])
            .current_dir(&f.worktree)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .output()
            .expect("run git");
        assert!(
            armed.marker.exists(),
            "the test no longer reaches the filter"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_repository_clean_filter_is_not_run_by_worktree_sync() {
        let _home = core::temp_test_home();
        let f = fixture("wt-filter");
        let armed = arm_filter(&f, b"evil", |a| {
            let script = a.script.to_str().unwrap();
            git(&f.main, &["config", "filter.evil.clean", script]);
        });

        let result = sync_linked_worktree(&f.worktree).expect("sync worktree");

        assert!(!armed.marker.exists(), "the repository's clean filter ran");
        assert!(f.worktree.join(".mcp.json").is_file(), "{result:?}");
        assert_eq!(read(&f.worktree.join("CLAUDE.md")), armed.edited);
        assert!(result.changed_tracked.is_empty(), "{result:?}");
        assert_plain_git_runs_the_filter(&f, &armed);
    }

    /// git refuses a worktree another user owns. Its `git config` read
    /// still succeeds, so the refusal must be explained where it does fail.
    #[cfg(unix)]
    #[test]
    fn a_worktree_another_user_owns_gets_the_safe_directory_command() {
        let _home = core::temp_test_home();
        let f = fixture("wt-other-owner");
        let outside = tempfile::tempdir().unwrap();
        let installed = git_guard::git_install().expect("git is installed");
        let Some(git) = installed.assuming_another_owner(outside.path(), &f.worktree) else {
            return;
        };
        let raw = core::read_project("wt-other-owner").unwrap();
        let project: Project = serde_json::from_str(&raw).unwrap();

        let err = sync_worktree_for(&project, &f.worktree, Some(&git)).unwrap_err();

        let shown = f.worktree.display().to_string();
        assert!(err.starts_with(&format!("Git will not read the repository at {shown},")), "{err}");
        // git quotes the path only when it needs to.
        let last = err.lines().last().unwrap();
        assert!(
            last == format!("git config --global --add safe.directory {shown}")
                || last == format!("git config --global --add safe.directory '{shown}'"),
            "{err}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_clean_filter_from_the_global_config_is_not_run_by_worktree_sync() {
        let _home = core::temp_test_home();
        let f = fixture("wt-filter-global");
        let armed = arm_filter(&f, b"glob", |a| {
            fs::write(
                a.outside.path().join("global.cfg"),
                format!("[filter \"glob\"]\n\tclean = {}\n", a.script.display()),
            )
            .unwrap();
        });
        let global = armed.outside.path().join("global.cfg");
        let installed = git_guard::git_install().expect("git is installed");
        let Some(git) =
            installed.with_global_config(armed.outside.path(), &global, "filter.glob.clean")
        else {
            return;
        };
        let raw = core::read_project("wt-filter-global").unwrap();
        let project: Project = serde_json::from_str(&raw).unwrap();

        let result = sync_worktree_for(&project, &f.worktree, Some(&git)).expect("sync worktree");

        assert!(
            !armed.marker.exists(),
            "the clean filter from the global config ran"
        );
        assert_eq!(read(&f.worktree.join("CLAUDE.md")), armed.edited);
        assert!(result.changed_tracked.is_empty(), "{result:?}");
        // The setup does reach the filter: git with that global config runs it.
        let _ = Command::new("git")
            .args(["ls-files", "--modified", "--", "CLAUDE.md"])
            .current_dir(&f.worktree)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_CONFIG_GLOBAL", &global)
            .output()
            .expect("run git");
        assert!(
            armed.marker.exists(),
            "the test no longer reaches the filter"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_required_clean_filter_at_worktree_scope_is_not_run_by_worktree_sync() {
        let _home = core::temp_test_home();
        let f = fixture("wt-filter-worktree-scope");
        let armed = arm_filter(&f, b"wtf", |a| {
            git(&f.main, &["config", "extensions.worktreeConfig", "true"]);
            let script = a.script.to_str().unwrap();
            git(
                &f.worktree,
                &["config", "--worktree", "filter.wtf.clean", script],
            );
            git(
                &f.worktree,
                &["config", "--worktree", "filter.wtf.required", "true"],
            );
        });

        let result = sync_linked_worktree(&f.worktree).expect("sync worktree");

        assert!(
            !armed.marker.exists(),
            "the worktree-scope clean filter ran"
        );
        assert_eq!(read(&f.worktree.join("CLAUDE.md")), armed.edited);
        assert!(result.changed_tracked.is_empty(), "{result:?}");
        assert!(result.warnings.is_empty(), "{result:?}");
        assert_plain_git_runs_the_filter(&f, &armed);
    }

    #[cfg(unix)]
    #[test]
    fn an_included_filter_with_dots_in_its_name_is_not_run_by_worktree_sync() {
        let _home = core::temp_test_home();
        let f = fixture("wt-filter-include");
        let armed = arm_filter(&f, b"a.b", |a| {
            let included = a.outside.path().join("included.cfg");
            fs::write(
                &included,
                format!("[filter \"a.b\"]\n\tclean = {}\n", a.script.display()),
            )
            .unwrap();
            git(
                &f.main,
                &["config", "include.path", included.to_str().unwrap()],
            );
        });

        let result = sync_linked_worktree(&f.worktree).expect("sync worktree");

        assert!(!armed.marker.exists(), "the included clean filter ran");
        assert!(result.changed_tracked.is_empty(), "{result:?}");
        assert_plain_git_runs_the_filter(&f, &armed);
    }

    #[cfg(unix)]
    #[test]
    fn a_process_filter_is_not_run_by_worktree_sync() {
        let _home = core::temp_test_home();
        let f = fixture("wt-filter-process");
        let armed = arm_filter(&f, b"proc", |a| {
            let script = a.script.to_str().unwrap();
            git(&f.main, &["config", "filter.proc.process", script]);
        });

        let result = sync_linked_worktree(&f.worktree).expect("sync worktree");

        assert!(!armed.marker.exists(), "the process filter ran");
        assert!(result.changed_tracked.is_empty(), "{result:?}");
        assert_plain_git_runs_the_filter(&f, &armed);
    }

    #[cfg(unix)]
    #[test]
    fn a_mixed_case_filter_name_with_dots_and_equals_is_not_run_by_worktree_sync() {
        let _home = core::temp_test_home();
        let f = fixture("wt-filter-mixed");
        let armed = arm_filter(&f, b"A.b=C", |a| {
            append_filter_section(&f, b"[filter \"A.b=C\"]", &a.script);
        });

        let result = sync_linked_worktree(&f.worktree).expect("sync worktree");

        assert!(!armed.marker.exists(), "the mixed-case filter ran");
        assert!(result.changed_tracked.is_empty(), "{result:?}");
        assert_plain_git_runs_the_filter(&f, &armed);
    }

    /// `[filter ""]` prints as `filter..clean`, and git applies it to a file
    /// whose `filter` attribute is empty.
    #[cfg(unix)]
    #[test]
    fn a_filter_with_an_empty_name_is_not_run_by_worktree_sync() {
        let _home = core::temp_test_home();
        let f = fixture("wt-filter-empty");
        let armed = arm_filter(&f, b"", |a| {
            append_filter_section(&f, b"[filter \"\"]", &a.script);
        });

        let result = sync_linked_worktree(&f.worktree);

        assert!(
            !armed.marker.exists(),
            "the filter with an empty name ran: {result:?}"
        );
        let result = result.expect("sync worktree");
        assert!(result.changed_tracked.is_empty(), "{result:?}");
        assert_plain_git_runs_the_filter(&f, &armed);
    }

    /// A name that is not UTF-8 cannot be blanked on every platform, so the
    /// repository is refused.
    #[cfg(unix)]
    #[test]
    fn a_filter_with_a_non_utf8_name_is_refused_by_worktree_sync() {
        let _home = core::temp_test_home();
        let f = fixture("wt-filter-non-utf8");
        let armed = arm_filter(&f, b"ev\xffil", |a| {
            append_filter_section(&f, b"[filter \"ev\xffil\"]", &a.script);
        });

        let result = sync_linked_worktree(&f.worktree);

        assert!(
            !armed.marker.exists(),
            "the filter with a non-UTF-8 name ran: {result:?}"
        );
        let err = result.unwrap_err();
        assert!(err.contains("not valid UTF-8"), "{err}");
        assert!(!f.worktree.join(".mcp.json").exists());
        assert_plain_git_runs_the_filter(&f, &armed);
    }

    #[test]
    fn filters_from_includes_include_ifs_and_worktree_scope_are_all_read() {
        let _home = core::temp_test_home();
        let f = fixture("wt-filter-scopes");
        let common = f.main.join(".git");
        fs::write(
            common.join("inc.cfg"),
            "[filter \"viaInclude\"]\n\tclean = x\n",
        )
        .unwrap();
        fs::write(
            common.join("incif.cfg"),
            "[filter \"viaIncludeIf\"]\n\tclean = x\n",
        )
        .unwrap();
        git(&f.main, &["config", "include.path", "inc.cfg"]);
        let record = fs::read_dir(common.join("worktrees"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let wt_gitdir = fs::canonicalize(record).unwrap();
        let include_if = format!("includeIf.gitdir:{}.path", wt_gitdir.display());
        git(&f.main, &["config", &include_if, "incif.cfg"]);
        git(&f.main, &["config", "extensions.worktreeConfig", "true"]);
        git(
            &f.worktree,
            &["config", "--worktree", "filter.viaWorktree.process", "x"],
        );

        let wt = WorktreeGit::open(&f.worktree, git_guard::git_install()).expect("open");

        let seen = format!("{:?}", wt.config);
        for name in ["viaInclude", "viaIncludeIf", "viaWorktree"] {
            assert!(
                seen.contains(&format!("\"{name}\"")),
                "{name} missing from {seen}"
            );
        }
    }

    /// The empty-name override relies on git reading `filter..clean`.
    #[test]
    fn git_accepts_an_override_for_the_empty_filter_name() {
        let tmp = tempfile::tempdir().unwrap();
        git_init(tmp.path());
        let out = Command::new("git")
            .arg("-C")
            .arg(tmp.path())
            .args(["config", "--show-scope", "--get-regexp", "^filter\\."])
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_CONFIG_COUNT", "2")
            .env("GIT_CONFIG_KEY_0", "filter..clean")
            .env("GIT_CONFIG_VALUE_0", "")
            .env("GIT_CONFIG_KEY_1", "filter..required")
            .env("GIT_CONFIG_VALUE_1", "false")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{out:?}");
        assert!(stdout.contains("command\tfilter..clean"), "{stdout}");
    }

    #[test]
    fn turning_off_gitignore_management_removes_the_exclude_block() {
        let _home = core::temp_test_home();
        let f = fixture("wt-unmanage");
        let exclude = f.main.join(".git/info/exclude");
        let before = read(&exclude);
        sync_linked_worktree(&f.worktree).expect("sync worktree");
        assert!(read(&exclude).contains(core::gitignore::BEGIN_MARKER));

        let raw = core::read_project("wt-unmanage").unwrap();
        let mut project: Project = serde_json::from_str(&raw).unwrap();
        project.manage_gitignore = false;
        core::save_project("wt-unmanage", &serde_json::to_string(&project).unwrap()).unwrap();
        super::super::sync_project_without_autodetect(&mut project).expect("parent sync");

        let after = read(&exclude);
        assert!(!after.contains(core::gitignore::BEGIN_MARKER), "{after}");
        assert_eq!(
            after.trim_end(),
            before.trim_end(),
            "git's own lines were changed"
        );
    }
}
