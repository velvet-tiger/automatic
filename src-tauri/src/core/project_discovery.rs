//! Find folders under a root that look like agent projects, for the
//! "Find projects" dialog.
//!
//! A folder is a candidate when any agent's [`Agent::detect_in`] matches it,
//! or when it holds an `AGENTS.md`. `AGENTS.md` is shared by many agents, so
//! no agent claims it on its own, but it still marks a project.
//!
//! The walk only reads. Adding the chosen folders goes through the normal
//! project-creation path in `commands::projects`.

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
const SKIPPED_HOME_DIR_NAMES: &[&str] =
    &["Library", "Applications", "Pictures", "Music", "Movies"];

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
    /// Folders opened.
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
    pub status: DirectoryStatus,
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

/// Whether the walk may descend into a child folder named `name`. `parent_is_home`
/// adds the home-only skip list.
fn should_descend(name: &str, parent_is_home: bool) -> bool {
    if name.starts_with('.') || SKIPPED_DIR_NAMES.contains(&name) {
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
    let is_home = |p: &Path| home.is_some_and(|h| h == p);

    if !is_home(root) {
        if let Some(candidate) = detect_candidate(root, agents) {
            result.candidates.push(candidate);
        }
    }

    // Depth-first with an explicit stack: deep trees cannot overflow the
    // thread stack.
    let mut stack: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        if result.visited >= limits.max_visited {
            result.truncated = true;
            break;
        }
        result.visited += 1;

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
            if !should_descend(name, parent_is_home) {
                continue;
            }
            children.push(entry.path());
        }
        // Sorted in reverse so the stack pops them in name order.
        children.sort_unstable_by(|a, b| b.cmp(a));

        for child in children {
            if !is_home(&child) {
                if let Some(candidate) = detect_candidate(&child, agents) {
                    result.candidates.push(candidate);
                    continue;
                }
            }
            if child_depth < limits.max_depth {
                stack.push((child, child_depth));
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
    let root_path = PathBuf::from(root.trim());
    if !root_path.is_absolute() {
        return Err(format!(
            "The folder to scan must be an absolute path (got '{}').",
            root
        ));
    }
    if !root_path.is_dir() {
        return Err(format!("Folder '{}' does not exist.", root_path.display()));
    }

    let home = crate::agent::home_dir();
    let agents = crate::agent::all();
    let walk = walk_for_projects(&root_path, home.as_deref(), &agents, limits);

    let mut projects = Vec::with_capacity(walk.candidates.len());
    for candidate in walk.candidates {
        let Some(directory) = candidate.directory.to_str().map(str::to_string) else {
            // A non-UTF-8 path cannot be stored in a project; skip it.
            continue;
        };
        let status = super::inspect_project_directory(&directory)
            .map_err(|e| format!("Failed to check '{}': {}", directory, e))?;
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
            let registered = root.path().join("registered");
            touch(&registered.join("CLAUDE.md"));
            touch(&root.path().join("fresh/CLAUDE.md"));
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
            assert!(matches!(by_name("fresh").status, DirectoryStatus::Available));
            assert!(matches!(
                by_name("registered").status,
                DirectoryStatus::RegisteredHere { .. }
            ));
            assert_eq!(by_name("fresh").relative_path, "fresh");
        });
    }

    #[test]
    fn discover_rejects_relative_and_missing_roots() {
        assert!(discover_projects("relative/path", DiscoveryLimits::default()).is_err());
        let gone = tempdir().expect("tempdir").path().join("missing");
        assert!(discover_projects(gone.to_str().unwrap(), DiscoveryLimits::default()).is_err());
    }
}
