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
//! so the tracked files among sync's targets are read before the sync and put
//! back afterwards. See [`TrackedSnapshot`].

use std::collections::BTreeSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

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
    sync_worktree_for(&project, &worktree)
}

/// Sync every linked worktree of `project`'s folder that is not registered as
/// a project of its own. Called after the parent's own sync succeeded, so a
/// worktree that cannot be synced is logged and skipped: the parent's sync
/// already happened and still reports success.
pub(crate) fn sync_project_worktrees(project: &Project) {
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
        match sync_worktree_for(project, &worktree) {
            Ok(result) if !result.changed_tracked.is_empty() => eprintln!(
                "[automatic] worktree sync: {} now shows changes to tracked files: {}",
                worktree.display(),
                result.changed_tracked.join(", ")
            ),
            Ok(_) => {}
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
fn main_checkout_of(worktree: &Path) -> Result<PathBuf, String> {
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
/// copy changed.
fn sync_worktree_for(project: &Project, worktree: &Path) -> Result<WorktreeSync, String> {
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

    let effective_dir = match copy.mode {
        ProjectMode::Silent => worktree.join(".automatic").join("silent"),
        ProjectMode::Normal => worktree.to_path_buf(),
    };
    let mut targets = fixed_sync_paths(&copy, worktree, &effective_dir);
    targets.push(worktree.join(".gitignore"));
    let snapshot = TrackedSnapshot::take(worktree, &targets)?;
    let modified_before = modified_tracked_files(worktree)?;

    let synced = sync_to_directory(&mut copy);
    // Put the branch's files back even when the sync stopped part way.
    let restore = snapshot.restore();
    result.written = synced?;
    result.kept_tracked = restore.kept;

    let modified_after = modified_tracked_files(worktree)?;
    let kept: BTreeSet<&String> = result.kept_tracked.iter().collect();
    result.changed_tracked = restore
        .failed
        .into_iter()
        .chain(
            modified_after
                .difference(&modified_before)
                .filter(|path| !kept.contains(path))
                .cloned(),
        )
        .collect::<BTreeSet<String>>()
        .into_iter()
        .collect();
    Ok(result)
}

/// A `git` command run in `worktree`. Variables that point git at another
/// repository are removed, since this may run inside a git hook that sets
/// them. The fsmonitor hook is turned off so a read never starts one.
fn git_in(worktree: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(worktree)
        .args(["-c", "core.fsmonitor=false"]);
    for var in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_NAMESPACE",
    ] {
        cmd.env_remove(var);
    }
    cmd
}

/// Run `git ls-files -z` in `worktree` with `args`, and return the listed
/// paths, relative to the worktree.
fn ls_files(worktree: &Path, args: &[String]) -> Result<Vec<String>, String> {
    let output = git_in(worktree)
        .args(["ls-files", "-z"])
        .args(args)
        .output()
        .map_err(|e| format!("Cannot run git in {}: {}", worktree.display(), e))?;
    if !output.status.success() {
        return Err(format!(
            "git ls-files failed in {}: {}",
            worktree.display(),
            String::from_utf8_lossy(&output.stderr).trim()
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

/// Tracked files whose working copy differs from the index, or is missing.
fn modified_tracked_files(worktree: &Path) -> Result<BTreeSet<String>, String> {
    Ok(ls_files(worktree, &["--modified".to_string()])?
        .into_iter()
        .collect())
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
    /// Read every tracked file at or under one of `targets`. A target outside
    /// `root` is ignored.
    fn take(root: &Path, targets: &[PathBuf]) -> Result<Self, String> {
        let mut pathspecs: Vec<String> = targets
            .iter()
            .filter_map(|target| target.strip_prefix(root).ok())
            .filter(|rel| !rel.as_os_str().is_empty())
            .filter_map(|rel| rel.to_str())
            .map(|rel| format!(":(literal){}", rel.replace('\\', "/")))
            .collect();
        pathspecs.sort();
        pathspecs.dedup();
        if pathspecs.is_empty() {
            return Ok(Self {
                root: root.to_path_buf(),
                entries: Vec::new(),
            });
        }
        let mut args = vec!["--".to_string()];
        args.extend(pathspecs);
        let entries = ls_files(root, &args)?
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

    /// Put back every entry that no longer looks as it did. Each path is
    /// checked with [`project_write_refusal`] first, so a restore never writes
    /// through a link out of the worktree.
    fn restore(self) -> Restored {
        let mut restored = Restored {
            kept: Vec::new(),
            failed: Vec::new(),
        };
        for (rel, before) in self.entries {
            let path = self.root.join(&rel);
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

/// Make `path` look as `state` describes. Whatever is at `path` now is
/// removed first, unless it is a folder: a folder may hold files the user
/// wrote, so it is never removed here.
fn write_state(path: &Path, state: &TrackedState) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => {
            return Err(format!("{} is now a folder", path.display()));
        }
        Ok(_) => {
            fs::remove_file(path).map_err(|e| format!("Cannot remove {}: {}", path.display(), e))?
        }
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(format!("Cannot read {}: {}", path.display(), e)),
    }
    let ensure_parent = || -> Result<(), String> {
        match path.parent() {
            Some(parent) => fs::create_dir_all(parent)
                .map_err(|e| format!("Cannot create {}: {}", parent.display(), e)),
            None => Ok(()),
        }
    };
    match state {
        TrackedState::Missing => Ok(()),
        TrackedState::File { bytes, permissions } => {
            ensure_parent()?;
            fs::write(path, bytes)
                .map_err(|e| format!("Cannot write {}: {}", path.display(), e))?;
            fs::set_permissions(path, permissions.clone())
                .map_err(|e| format!("Cannot set permissions on {}: {}", path.display(), e))
        }
        TrackedState::Link(target) => {
            ensure_parent()?;
            make_link(target, path)
        }
    }
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
        let tmp = tempfile::tempdir().expect("tempdir");
        let base = fs::canonicalize(tmp.path()).expect("canonical");
        let main = base.join("main");
        fs::create_dir_all(&main).expect("mkdir");
        git(&main, &["init", "-q"]);
        fs::write(main.join("CLAUDE.md"), BRANCH_CLAUDE_MD).expect("write");
        fs::write(main.join(".gitignore"), BRANCH_GITIGNORE).expect("write");
        git(&main, &["add", "CLAUDE.md", ".gitignore"]);
        git(&main, &["commit", "-q", "-m", "init"]);
        git(&main, &["worktree", "add", "-q", "-b", "feature", "../wt"]);
        let worktree = base.join("wt");

        let project = Project {
            name: name.into(),
            directory: main.to_str().unwrap().into(),
            agents: vec!["claude".into()],
            manage_gitignore: true,
            ..Default::default()
        };
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
        assert!(modified_tracked_files(&f.worktree).unwrap().is_empty());
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
        assert!(modified_tracked_files(&f.worktree).unwrap().is_empty());
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
}
