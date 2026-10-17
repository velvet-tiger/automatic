//! Find folders under a root that look like agent projects, for the
//! "Find projects" dialog.
//!
//! A folder is a candidate when any agent's [`Agent::detect_in`] matches it,
//! or when it holds an `AGENTS.md`. `AGENTS.md` is shared by many agents, so
//! no agent claims it on its own, but it still marks a project.
//!
//! The walk only reads. Adding the chosen folders goes through the normal
//! project-creation path in `commands::projects`.

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::agent::Agent;

use super::DirectoryStatus;

/// Default depth below the root, counted in folder levels.
pub const DEFAULT_MAX_DEPTH: usize = 6;
/// Default cap on folders opened in one scan, so a huge tree cannot hang the
/// dialog. Reaching it marks the report as truncated.
pub const DEFAULT_MAX_VISITED: usize = 50_000;

/// Folder names never descended into, at any depth. These hold build output,
/// dependencies or virtual environments, not projects of their own.
const SKIPPED_DIR_NAMES: &[&str] = &[
    "node_modules",
    "target",
    "vendor",
    "dist",
    "build",
    "out",
    "__pycache__",
    "venv",
    "Pods",
    "DerivedData",
    "bower_components",
];

/// Folders directly under the user's home that are never descended into.
/// `Library` holds app data, and the media folders trigger macOS privacy
/// prompts (Photos, Music) without ever holding code.
/// `AppData` is the Windows equivalent of `Library`; it is hidden by a file
/// attribute, not a leading dot, so the dot rule misses it.
const SKIPPED_HOME_DIR_NAMES: &[&str] = &[
    "Library",
    "Applications",
    "Pictures",
    "Music",
    "Movies",
    "AppData",
];

/// Limits for one scan.
#[derive(Debug, Clone, Copy)]
pub struct DiscoveryLimits {
    pub max_depth: usize,
    pub max_visited: usize,
}

impl Default for DiscoveryLimits {
    fn default() -> Self {
        Self {
            max_depth: DEFAULT_MAX_DEPTH,
            max_visited: DEFAULT_MAX_VISITED,
        }
    }
}

/// An agent detected in a candidate folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DetectedAgent {
    pub id: String,
    pub label: String,
}

/// A folder the walk matched, before its registry status is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub directory: PathBuf,
    pub agents: Vec<DetectedAgent>,
    pub has_agents_md: bool,
}

/// What the walk found, with counters the dialog reports.
#[derive(Debug, Default)]
pub struct WalkResult {
    pub candidates: Vec<Candidate>,
    /// Folders checked for project files. Each one counts once, whether or
    /// not the walk then opens it.
    pub visited: usize,
    /// Folders that could not be listed (permissions, races).
    pub unreadable: usize,
    /// True when the walk stopped at `max_visited` before finishing.
    pub truncated: bool,
}

/// One candidate as the dialog shows it.
#[derive(Debug, Serialize)]
pub struct DiscoveredProject {
    /// Absolute path to the folder.
    pub directory: String,
    /// Folder name, used as the new project's name.
    pub name: String,
    /// Path relative to the scan root. Empty for the root itself.
    pub relative_path: String,
    pub agents: Vec<DetectedAgent>,
    pub has_agents_md: bool,
    /// The folder's registry status. `None` when it could not be checked;
    /// `check_error` then says why, and the folder cannot be added.
    pub status: Option<DirectoryStatus>,
    pub check_error: Option<String>,
}

/// The result of `discover_projects`.
#[derive(Debug, Serialize)]
pub struct DiscoveryReport {
    pub root: String,
    pub projects: Vec<DiscoveredProject>,
    pub visited: usize,
    pub unreadable: usize,
    pub truncated: bool,
}

/// The agents whose project markers are present in `dir`, and whether it
/// holds an `AGENTS.md`. `None` when nothing matches.
fn detect_candidate(dir: &Path, agents: &[&'static dyn Agent]) -> Option<Candidate> {
    let detected: Vec<DetectedAgent> = agents
        .iter()
        .filter(|agent| agent.detect_in(dir))
        .map(|agent| DetectedAgent {
            id: agent.id().to_string(),
            label: agent.label().to_string(),
        })
        .collect();
    let has_agents_md = dir.join("AGENTS.md").is_file();
    if detected.is_empty() && !has_agents_md {
        return None;
    }
    Some(Candidate {
        directory: dir.to_path_buf(),
        agents: detected,
        has_agents_md,
    })
}

/// Whether the walk may enter a child folder named `name` inside `parent`.
/// `parent_is_home` adds the home-only skip list.
fn should_descend(name: &str, parent: &Path, parent_is_home: bool) -> bool {
    if name.starts_with('.') || SKIPPED_DIR_NAMES.contains(&name) {
        return false;
    }
    // Go's module cache (`~/go/pkg/mod`) holds a copy of every dependency,
    // many of which ship their own CLAUDE.md or AGENTS.md.
    if name == "mod" && parent.file_name().is_some_and(|p| p == "pkg") {
        return false;
    }
    !(parent_is_home && SKIPPED_HOME_DIR_NAMES.contains(&name))
}

/// Walk `root` and collect candidate folders.
///
/// - Symlinks are never followed, so a link loop cannot trap the walk.
/// - The walk stops at a candidate and does not look inside it, so a monorepo
///   shows up once rather than once per package.
/// - The root is the exception: it is listed when it matches, but the walk
///   still descends below it. A folder of projects may carry its own
///   `CLAUDE.md`.
/// - `home` is never listed. It holds each agent's global config (`~/.claude`,
///   `~/.gemini`), which would otherwise look like a project.
///
/// Reads only; never writes.
pub fn walk_for_projects(
    root: &Path,
    home: Option<&Path>,
    agents: &[&'static dyn Agent],
    limits: DiscoveryLimits,
) -> WalkResult {
    let mut result = WalkResult::default();
    // Callers pass canonical paths, so plain equality is enough here.
    let is_home = |p: &Path| home.is_some_and(|h| h == p);

    result.visited = 1;
    if !is_home(root) {
        if let Some(candidate) = detect_candidate(root, agents) {
            result.candidates.push(candidate);
        }
    }

    // Breadth-first: every shallow folder is checked before any deep one.
    // When the visit cap cuts the walk short, it drops the deep tail of the
    // tree, not whole top-level folders such as `~/code`.
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::from([(root.to_path_buf(), 0)]);
    'walk: while let Some((dir, depth)) = queue.pop_front() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => {
                // Unreadable folders are normal under a home folder
                // (permission-protected app data). Count them and move on.
                result.unreadable += 1;
                continue;
            }
        };

        let parent_is_home = is_home(&dir);
        let child_depth = depth + 1;
        let mut children: Vec<PathBuf> = Vec::new();
        for entry in entries.flatten() {
            // `DirEntry::file_type` does not follow symlinks, so a link to a
            // folder reports as a symlink here and is skipped.
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if !should_descend(name, &dir, parent_is_home) {
                continue;
            }
            children.push(entry.path());
        }
        children.sort_unstable();

        for child in children {
            // Every child costs a round of marker checks, so each one counts
            // against the budget. A folder with 100k subfolders cannot run
            // past the cap in a single step.
            if result.visited >= limits.max_visited {
                result.truncated = true;
                break 'walk;
            }
            result.visited += 1;
            if !is_home(&child) {
                if let Some(candidate) = detect_candidate(&child, agents) {
                    result.candidates.push(candidate);
                    continue;
                }
            }
            if child_depth < limits.max_depth {
                queue.push_back((child, child_depth));
            }
        }
    }

    result
        .candidates
        .sort_by(|a, b| a.directory.cmp(&b.directory));
    result
}

/// Scan `root` for agent projects and report each one with its registry
/// status. Reads only.
pub fn discover_projects(root: &str, limits: DiscoveryLimits) -> Result<DiscoveryReport, String> {
    let given = PathBuf::from(root.trim());
    if !given.is_absolute() {
        return Err(format!(
            "The folder to scan must be an absolute path (got '{}').",
            root
        ));
    }
    if !given.is_dir() {
        return Err(format!("Folder '{}' does not exist.", given.display()));
    }
    // Canonical paths make the home check reliable when the folder was
    // reached through a symlink (macOS aliases, `/home` -> `/var/home`).
    // Every path the walk builds is then canonical too, because it joins
    // real names and never follows a link. The add command relies on that.
    let root_path = given
        .canonicalize()
        .map_err(|e| format!("Cannot open folder '{}': {}", given.display(), e))?;

    let home = crate::agent::home_dir().and_then(|h| h.canonicalize().ok());
    let agents = crate::agent::all();
    let walk = walk_for_projects(&root_path, home.as_deref(), &agents, limits);

    let mut projects = Vec::with_capacity(walk.candidates.len());
    for candidate in walk.candidates {
        let Some(directory) = candidate.directory.to_str().map(str::to_string) else {
            // A non-UTF-8 path cannot be stored in a project; skip it.
            continue;
        };
        // One unreadable registry entry must not sink the whole scan.
        let (status, check_error) = match super::inspect_project_directory(&directory) {
            Ok(status) => (Some(status), None),
            Err(e) => (None, Some(e)),
        };
        let name = candidate
            .directory
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        let relative_path = candidate
            .directory
            .strip_prefix(&root_path)
            .ok()
            .and_then(|p| p.to_str())
            .unwrap_or_default()
            .to_string();
        projects.push(DiscoveredProject {
            directory,
            name,
            relative_path,
            agents: candidate.agents,
            has_agents_md: candidate.has_agents_md,
            status,
            check_error,
        });
    }

    Ok(DiscoveryReport {
        root: root_path.display().to_string(),
        projects,
        visited: walk.visited,
        unreadable: walk.unreadable,
        truncated: walk.truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn agents() -> Vec<&'static dyn Agent> {
        crate::agent::all()
    }

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, "x").expect("write");
    }

    fn found(result: &WalkResult, root: &Path) -> Vec<String> {
        result
            .candidates
            .iter()
            .map(|c| {
                c.directory
                    .strip_prefix(root)
                    .expect("under root")
                    .to_string_lossy()
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn finds_folders_by_agent_files_and_agents_md() {
        let root = tempdir().expect("tempdir");
        touch(&root.path().join("alpha/CLAUDE.md"));
        touch(&root.path().join("work/beta/.cursor/mcp.json"));
        touch(&root.path().join("gamma/AGENTS.md"));
        fs::create_dir_all(root.path().join("plain/src")).expect("mkdir");

        let result = walk_for_projects(root.path(), None, &agents(), DiscoveryLimits::default());

        assert_eq!(found(&result, root.path()), vec!["alpha", "gamma", "work/beta"]);
        let alpha = &result.candidates[0];
        assert_eq!(alpha.agents.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), vec!["claude"]);
        assert!(!alpha.has_agents_md);
        let gamma = &result.candidates[1];
        assert!(gamma.agents.is_empty());
        assert!(gamma.has_agents_md);
    }

    #[test]
    fn does_not_look_inside_a_found_project() {
        let root = tempdir().expect("tempdir");
        touch(&root.path().join("mono/CLAUDE.md"));
        touch(&root.path().join("mono/packages/web/CLAUDE.md"));

        let result = walk_for_projects(root.path(), None, &agents(), DiscoveryLimits::default());

        assert_eq!(found(&result, root.path()), vec!["mono"]);
    }

    #[test]
    fn lists_the_root_and_still_descends_below_it() {
        let root = tempdir().expect("tempdir");
        touch(&root.path().join("CLAUDE.md"));
        touch(&root.path().join("child/CLAUDE.md"));

        let result = walk_for_projects(root.path(), None, &agents(), DiscoveryLimits::default());

        assert_eq!(found(&result, root.path()), vec!["", "child"]);
    }

    #[test]
    fn never_lists_the_home_folder() {
        let home = tempdir().expect("tempdir");
        touch(&home.path().join(".claude/settings.json"));
        touch(&home.path().join("code/app/CLAUDE.md"));

        let result = walk_for_projects(
            home.path(),
            Some(home.path()),
            &agents(),
            DiscoveryLimits::default(),
        );

        assert_eq!(found(&result, home.path()), vec!["code/app"]);
    }

    #[test]
    fn skips_dependency_hidden_and_home_media_folders() {
        let home = tempdir().expect("tempdir");
        touch(&home.path().join("node_modules/pkg/AGENTS.md"));
        touch(&home.path().join("app/target/debug/AGENTS.md"));
        touch(&home.path().join(".config/tool/AGENTS.md"));
        touch(&home.path().join("Library/Thing/CLAUDE.md"));
        touch(&home.path().join("Pictures/x/CLAUDE.md"));
        // The home-only list applies directly under home, not deeper.
        touch(&home.path().join("code/Library/CLAUDE.md"));

        let result = walk_for_projects(
            home.path(),
            Some(home.path()),
            &agents(),
            DiscoveryLimits::default(),
        );

        assert_eq!(found(&result, home.path()), vec!["code/Library"]);
    }

    #[test]
    fn skips_the_go_module_cache() {
        let root = tempdir().expect("tempdir");
        touch(&root.path().join("go/pkg/mod/github.com/o/r@v1/AGENTS.md"));
        touch(&root.path().join("go/src/app/AGENTS.md"));

        let result = walk_for_projects(root.path(), None, &agents(), DiscoveryLimits::default());

        assert_eq!(found(&result, root.path()), vec!["go/src/app"]);
    }

    /// A deep folder that sorts first must not use up the cap before a
    /// shallow project in a later folder is checked.
    #[test]
    fn the_cap_drops_deep_folders_before_shallow_ones() {
        let root = tempdir().expect("tempdir");
        for i in 0..30 {
            fs::create_dir_all(root.path().join(format!("Downloads/a/b{i:02}/c"))).expect("mkdir");
        }
        touch(&root.path().join("code/app/CLAUDE.md"));

        let limits = DiscoveryLimits {
            max_visited: 10,
            ..DiscoveryLimits::default()
        };
        let result = walk_for_projects(root.path(), None, &agents(), limits);

        assert!(result.truncated);
        assert_eq!(found(&result, root.path()), vec!["code/app"]);
    }

    #[cfg(unix)]
    #[test]
    fn counts_unreadable_folders_and_carries_on() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempdir().expect("tempdir");
        let locked = root.path().join("locked");
        fs::create_dir_all(locked.join("inner")).expect("mkdir");
        touch(&root.path().join("open/app/CLAUDE.md"));
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("chmod");

        let result = walk_for_projects(root.path(), None, &agents(), DiscoveryLimits::default());
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("restore");

        assert_eq!(result.unreadable, 1);
        assert_eq!(found(&result, root.path()), vec!["open/app"]);
    }

    #[test]
    fn respects_the_depth_limit() {
        let root = tempdir().expect("tempdir");
        touch(&root.path().join("a/b/c/CLAUDE.md"));
        touch(&root.path().join("a/near/CLAUDE.md"));

        let limits = DiscoveryLimits {
            max_depth: 2,
            ..DiscoveryLimits::default()
        };
        let result = walk_for_projects(root.path(), None, &agents(), limits);

        assert_eq!(found(&result, root.path()), vec!["a/near"]);
    }

    #[test]
    fn stops_at_the_visit_cap_and_says_so() {
        let root = tempdir().expect("tempdir");
        for i in 0..5 {
            fs::create_dir_all(root.path().join(format!("d{i}/inner"))).expect("mkdir");
        }

        let limits = DiscoveryLimits {
            max_visited: 3,
            ..DiscoveryLimits::default()
        };
        let result = walk_for_projects(root.path(), None, &agents(), limits);

        assert!(result.truncated);
        assert_eq!(result.visited, 3);
    }

    #[test]
    fn a_wide_folder_counts_every_child_against_the_cap() {
        let root = tempdir().expect("tempdir");
        for i in 0..20 {
            fs::create_dir_all(root.path().join(format!("d{i:02}"))).expect("mkdir");
        }

        let limits = DiscoveryLimits {
            max_visited: 5,
            ..DiscoveryLimits::default()
        };
        let result = walk_for_projects(root.path(), None, &agents(), limits);

        assert!(result.truncated);
        assert_eq!(result.visited, 5);
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_symlinks() {
        let root = tempdir().expect("tempdir");
        let outside = tempdir().expect("outside");
        touch(&outside.path().join("linked/CLAUDE.md"));
        std::os::unix::fs::symlink(outside.path(), root.path().join("link")).expect("symlink");
        // A loop back to the root must not trap the walk.
        std::os::unix::fs::symlink(root.path(), root.path().join("loop")).expect("symlink");

        let result = walk_for_projects(root.path(), None, &agents(), DiscoveryLimits::default());

        assert!(result.candidates.is_empty(), "{:?}", found(&result, root.path()));
    }

    #[test]
    fn discover_reports_registry_status() {
        use crate::core::paths::with_test_home;

        let home = tempdir().expect("home");
        with_test_home(home.path().to_path_buf(), || {
            let root = tempdir().expect("root");
            let root_path = root.path().canonicalize().expect("canonical root");
            let registered = root_path.join("registered");
            touch(&registered.join("CLAUDE.md"));
            touch(&root_path.join("fresh/CLAUDE.md"));
            touch(&root_path.join("orphan/CLAUDE.md"));
            touch(&root_path.join("orphan/.automatic/project.json"));
            let project = crate::core::Project {
                name: "registered".to_string(),
                directory: registered.to_str().unwrap().to_string(),
                ..Default::default()
            };
            crate::core::save_project("registered", &serde_json::to_string(&project).unwrap())
                .expect("save");

            let report = discover_projects(root.path().to_str().unwrap(), DiscoveryLimits::default())
                .expect("discover");

            let by_name = |n: &str| report.projects.iter().find(|p| p.name == n).expect(n);
            assert!(matches!(by_name("fresh").status, Some(DirectoryStatus::Available)));
            assert!(matches!(
                by_name("registered").status,
                Some(DirectoryStatus::RegisteredHere { .. })
            ));
            assert!(matches!(
                by_name("orphan").status,
                Some(DirectoryStatus::OrphanConfig { .. })
            ));
            assert_eq!(by_name("fresh").relative_path, "fresh");
        });
    }

    /// A root reached through a symlink is reported by its real path, so a
    /// home folder reached that way is still recognised as home.
    #[cfg(unix)]
    #[test]
    fn discover_reports_canonical_paths_for_a_symlinked_root() {
        let real = tempdir().expect("real");
        touch(&real.path().join("app/CLAUDE.md"));
        let links = tempdir().expect("links");
        let link = links.path().join("alias");
        std::os::unix::fs::symlink(real.path(), &link).expect("symlink");

        let report = discover_projects(link.to_str().unwrap(), DiscoveryLimits::default())
            .expect("discover");

        let canonical = real.path().canonicalize().expect("canonical");
        assert_eq!(report.root, canonical.display().to_string());
        assert_eq!(
            report.projects[0].directory,
            canonical.join("app").display().to_string()
        );
    }

    #[test]
    fn discover_rejects_relative_and_missing_roots() {
        assert!(discover_projects("relative/path", DiscoveryLimits::default()).is_err());
        let gone = tempdir().expect("tempdir").path().join("missing");
        assert!(discover_projects(gone.to_str().unwrap(), DiscoveryLimits::default()).is_err());
    }
}
