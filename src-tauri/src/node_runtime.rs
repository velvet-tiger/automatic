//! Picks the Node.js version a project asks for, using nvm's installs.
//!
//! Automatic launches project commands (e.g. Dev Servers) directly, with the
//! `PATH` captured once at startup by `path_env`. That `PATH` reflects the
//! user's login shell in their home directory. It never reflects a project's
//! `.nvmrc`, because nvm only switches versions through its shell `cd` hook
//! or an explicit `nvm use`. A project built with one Node version and run
//! with another fails as soon as it loads a native add-on.
//!
//! This module reads the project's version file and resolves it against the
//! folders nvm keeps on disk. It never runs the `nvm` shell function, so it
//! adds no startup delay and needs no shell. Callers prepend the returned
//! `bin` folder to the child's `PATH`.
//!
//! Supported version file contents follow nvm: exact (`24.19.0`, `v24.19.0`),
//! partial (`24`, `24.19`), `node` / `stable` (newest installed), `system`
//! (leave `PATH` alone), and alias names such as `default`, `lts/*` or
//! `lts/jod`, which nvm stores as files under `$NVM_DIR/alias`.

use std::fs;
use std::path::{Path, PathBuf};

use semver::Version;

/// Version files checked in each directory, in priority order. nvm reads
/// `.nvmrc`; `.node-version` is the cross-manager convention.
const VERSION_FILES: [&str; 2] = [".nvmrc", ".node-version"];

/// Guards against alias files that point at each other.
const MAX_ALIAS_DEPTH: usize = 10;

/// Where a requested version came from and what it said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionRequest {
    pub file: PathBuf,
    pub spec: String,
}

/// An nvm-installed Node version selected for a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvmNode {
    pub version: Version,
    pub bin_dir: PathBuf,
    pub request: VersionRequest,
}

/// Outcome of selecting Node for a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeSelection {
    /// No version file between the working directory and the project root.
    NotRequested,
    /// The version file says `system`: use whatever Node is already on `PATH`.
    System(VersionRequest),
    Nvm(NvmNode),
}

impl NodeSelection {
    /// One line for a server log explaining which Node was chosen, or `None`
    /// when nothing changed.
    pub fn describe(&self) -> Option<String> {
        match self {
            NodeSelection::NotRequested => None,
            NodeSelection::System(req) => Some(format!(
                "Using the system Node on PATH ({} requests 'system')",
                file_label(&req.file)
            )),
            NodeSelection::Nvm(node) => Some(format!(
                "Using Node v{} from nvm ({})",
                node.version,
                file_label(&node.request.file)
            )),
        }
    }
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// nvm's install root: `$NVM_DIR` when set, otherwise `~/.nvm`.
pub fn default_nvm_dir() -> Option<PathBuf> {
    match std::env::var_os("NVM_DIR") {
        Some(dir) if !dir.is_empty() => Some(PathBuf::from(dir)),
        _ => dirs::home_dir().map(|home| home.join(".nvm")),
    }
}

/// Select Node for a command run in `working_dir`, a folder inside
/// `project_root`. Version files are searched from `working_dir` upwards and
/// never above `project_root`, so a monorepo subpackage inherits the root's
/// `.nvmrc`.
///
/// Errors when a version file exists but cannot be read, is empty, or asks
/// for a version nvm does not have installed. Falling back to another Node
/// in that case would reproduce the mismatch this exists to prevent.
pub fn select_node(working_dir: &Path, project_root: &Path, nvm_dir: &Path) -> Result<NodeSelection, String> {
    let Some(request) = find_version_request(working_dir, project_root)? else {
        return Ok(NodeSelection::NotRequested);
    };
    if request.spec == "system" {
        return Ok(NodeSelection::System(request));
    }
    let version = resolve_spec(&request, nvm_dir)?;
    let bin_dir = nvm_dir
        .join("versions")
        .join("node")
        .join(format!("v{version}"))
        .join("bin");
    Ok(NodeSelection::Nvm(NvmNode {
        version,
        bin_dir,
        request,
    }))
}

/// Find the nearest version file, walking from `working_dir` up to and
/// including `project_root`.
pub fn find_version_request(working_dir: &Path, project_root: &Path) -> Result<Option<VersionRequest>, String> {
    let mut dir = Some(working_dir);
    while let Some(current) = dir {
        for name in VERSION_FILES {
            let file = current.join(name);
            if file.is_file() {
                let raw = fs::read_to_string(&file)
                    .map_err(|e| format!("Could not read {}: {}", file.display(), e))?;
                let spec = parse_version_file(&raw)
                    .ok_or_else(|| format!("{} is empty. Put a Node version in it, e.g. 24.", file.display()))?;
                return Ok(Some(VersionRequest { file, spec }));
            }
        }
        if current == project_root {
            break;
        }
        dir = current.parent();
    }
    Ok(None)
}

/// The first non-blank, non-comment token in a version file.
fn parse_version_file(raw: &str) -> Option<String> {
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .find_map(|line| line.split_whitespace().next())
        .map(str::to_string)
}

/// Resolve a request against nvm's installed versions and alias files.
fn resolve_spec(request: &VersionRequest, nvm_dir: &Path) -> Result<Version, String> {
    let installed = installed_versions(nvm_dir)?;
    let mut spec = request.spec.clone();
    for _ in 0..MAX_ALIAS_DEPTH {
        match spec.as_str() {
            "node" | "stable" => {
                return installed.iter().max().cloned().ok_or_else(|| {
                    format!(
                        "{} requests '{}', but nvm has no Node versions installed in {}. Run `nvm install node`.",
                        request.file.display(),
                        request.spec,
                        nvm_dir.display()
                    )
                });
            }
            "system" => {
                return Err(format!(
                    "{} requests '{}', which resolves to 'system'. Put 'system' directly in the file instead.",
                    request.file.display(),
                    request.spec
                ));
            }
            _ => {}
        }
        if let Some(prefix) = parse_numeric_spec(&spec) {
            return highest_match(&installed, &prefix).ok_or_else(|| {
                format!(
                    "{} requests Node '{}', but nvm has no matching version installed in {}. Run `nvm install {}`.",
                    request.file.display(),
                    request.spec,
                    nvm_dir.display(),
                    request.spec
                )
            });
        }
        spec = read_alias(nvm_dir, &spec).map_err(|reason| {
            format!(
                "{} requests Node '{}': {}",
                request.file.display(),
                request.spec,
                reason
            )
        })?;
    }
    Err(format!(
        "{} requests Node '{}', but nvm's aliases for it loop or nest more than {} levels deep.",
        request.file.display(),
        request.spec,
        MAX_ALIAS_DEPTH
    ))
}

/// Versions under `$NVM_DIR/versions/node/vX.Y.Z` that have a `bin` folder.
fn installed_versions(nvm_dir: &Path) -> Result<Vec<Version>, String> {
    let root = nvm_dir.join("versions").join("node");
    if !root.is_dir() {
        return Err(format!(
            "nvm was not found: {} does not exist. Install nvm, or set NVM_DIR before starting Automatic.",
            root.display()
        ));
    }
    let entries = fs::read_dir(&root).map_err(|e| format!("Could not list {}: {}", root.display(), e))?;
    Ok(entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("bin").is_dir())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            Version::parse(name.strip_prefix('v')?).ok()
        })
        .collect())
}

/// `24`, `v24.19` or `24.19.0` as one to three numeric components.
fn parse_numeric_spec(spec: &str) -> Option<Vec<u64>> {
    let trimmed = spec.strip_prefix('v').unwrap_or(spec);
    let parts: Vec<u64> = trimmed
        .split('.')
        .map(|part| part.parse::<u64>().ok())
        .collect::<Option<_>>()?;
    (1..=3).contains(&parts.len()).then_some(parts)
}

fn highest_match(installed: &[Version], prefix: &[u64]) -> Option<Version> {
    installed
        .iter()
        .filter(|v| v.pre.is_empty())
        .filter(|v| {
            let parts = [v.major, v.minor, v.patch];
            prefix.iter().zip(parts.iter()).all(|(want, have)| want == have)
        })
        .max()
        .cloned()
}

/// Read an nvm alias file such as `$NVM_DIR/alias/default` or
/// `$NVM_DIR/alias/lts/*`. The name comes from a project file, so it must
/// stay inside the alias folder.
fn read_alias(nvm_dir: &Path, name: &str) -> Result<String, String> {
    let unsafe_name = name.is_empty()
        || name.starts_with('/')
        || name.starts_with('\\')
        || name.split(['/', '\\']).any(|part| part == ".." || part == ".");
    if unsafe_name {
        return Err(format!("'{name}' is not a valid nvm version or alias name"));
    }
    let path = nvm_dir.join("alias").join(name);
    if !path.is_file() {
        return Err(format!(
            "it is not a version number and nvm has no alias with that name ({} does not exist)",
            path.display()
        ));
    }
    let raw = fs::read_to_string(&path).map_err(|e| format!("could not read {}: {}", path.display(), e))?;
    parse_version_file(&raw).ok_or_else(|| format!("nvm alias file {} is empty", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fake_nvm(versions: &[&str]) -> TempDir {
        let nvm = TempDir::new().unwrap();
        for v in versions {
            fs::create_dir_all(nvm.path().join("versions/node").join(v).join("bin")).unwrap();
        }
        nvm
    }

    fn alias(nvm: &TempDir, name: &str, target: &str) {
        let path = nvm.path().join("alias").join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, format!("{target}\n")).unwrap();
    }

    fn project_with(file: &str, contents: &str) -> TempDir {
        let project = TempDir::new().unwrap();
        fs::write(project.path().join(file), contents).unwrap();
        project
    }

    fn select(project: &TempDir, nvm: &TempDir) -> Result<NodeSelection, String> {
        select_node(project.path(), project.path(), nvm.path())
    }

    fn selected_version(result: Result<NodeSelection, String>) -> String {
        match result.unwrap() {
            NodeSelection::Nvm(node) => node.version.to_string(),
            other => panic!("expected an nvm selection, got {other:?}"),
        }
    }

    #[test]
    fn no_version_file_means_not_requested() {
        let project = TempDir::new().unwrap();
        let nvm = fake_nvm(&["v24.19.0"]);
        assert_eq!(select(&project, &nvm).unwrap(), NodeSelection::NotRequested);
    }

    #[test]
    fn exact_version_with_and_without_v_prefix() {
        let nvm = fake_nvm(&["v24.14.1", "v24.19.0"]);
        assert_eq!(selected_version(select(&project_with(".nvmrc", "24.14.1\n"), &nvm)), "24.14.1");
        assert_eq!(selected_version(select(&project_with(".nvmrc", "v24.19.0"), &nvm)), "24.19.0");
    }

    #[test]
    fn partial_version_picks_the_highest_match() {
        let nvm = fake_nvm(&["v22.20.0", "v24.14.1", "v24.19.0", "v26.7.0"]);
        assert_eq!(selected_version(select(&project_with(".nvmrc", "24"), &nvm)), "24.19.0");
        assert_eq!(selected_version(select(&project_with(".nvmrc", "24.14"), &nvm)), "24.14.1");
    }

    #[test]
    fn bin_dir_points_into_the_selected_install() {
        let nvm = fake_nvm(&["v24.19.0"]);
        let NodeSelection::Nvm(node) = select(&project_with(".nvmrc", "24"), &nvm).unwrap() else {
            panic!("expected an nvm selection");
        };
        assert_eq!(node.bin_dir, nvm.path().join("versions/node/v24.19.0/bin"));
        assert!(node.bin_dir.is_dir());
    }

    #[test]
    fn node_and_stable_pick_the_newest_install() {
        let nvm = fake_nvm(&["v20.16.0", "v26.7.0", "v24.19.0"]);
        assert_eq!(selected_version(select(&project_with(".nvmrc", "node"), &nvm)), "26.7.0");
        assert_eq!(selected_version(select(&project_with(".nvmrc", "stable"), &nvm)), "26.7.0");
    }

    #[test]
    fn default_and_lts_aliases_resolve_through_alias_files() {
        let nvm = fake_nvm(&["v22.20.0", "v24.19.0"]);
        alias(&nvm, "default", "24");
        alias(&nvm, "lts/*", "lts/jod");
        alias(&nvm, "lts/jod", "v22.20.0");
        assert_eq!(selected_version(select(&project_with(".nvmrc", "default"), &nvm)), "24.19.0");
        assert_eq!(selected_version(select(&project_with(".nvmrc", "lts/*"), &nvm)), "22.20.0");
        assert_eq!(selected_version(select(&project_with(".nvmrc", "lts/jod"), &nvm)), "22.20.0");
    }

    #[test]
    fn system_leaves_path_alone() {
        let nvm = fake_nvm(&["v24.19.0"]);
        let result = select(&project_with(".nvmrc", "system"), &nvm).unwrap();
        assert!(matches!(result, NodeSelection::System(_)));
    }

    #[test]
    fn missing_version_is_an_error_naming_the_fix() {
        let nvm = fake_nvm(&["v22.20.0"]);
        let err = select(&project_with(".nvmrc", "24"), &nvm).unwrap_err();
        assert!(err.contains(".nvmrc"), "{err}");
        assert!(err.contains("nvm install 24"), "{err}");
    }

    #[test]
    fn missing_nvm_install_is_an_error() {
        let empty = TempDir::new().unwrap();
        let err = select(&project_with(".nvmrc", "24"), &empty).unwrap_err();
        assert!(err.contains("nvm was not found"), "{err}");
    }

    #[test]
    fn empty_version_file_is_an_error() {
        let nvm = fake_nvm(&["v24.19.0"]);
        let err = select(&project_with(".nvmrc", "\n# comment only\n"), &nvm).unwrap_err();
        assert!(err.contains("is empty"), "{err}");
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let nvm = fake_nvm(&["v24.19.0"]);
        let project = project_with(".nvmrc", "# pinned for better-sqlite3\n\n  24  \n");
        assert_eq!(selected_version(select(&project, &nvm)), "24.19.0");
    }

    #[test]
    fn nvmrc_wins_over_node_version() {
        let nvm = fake_nvm(&["v22.20.0", "v24.19.0"]);
        let project = project_with(".nvmrc", "24");
        fs::write(project.path().join(".node-version"), "22").unwrap();
        assert_eq!(selected_version(select(&project, &nvm)), "24.19.0");
    }

    #[test]
    fn node_version_file_is_used_when_there_is_no_nvmrc() {
        let nvm = fake_nvm(&["v22.20.0"]);
        assert_eq!(selected_version(select(&project_with(".node-version", "22"), &nvm)), "22.20.0");
    }

    #[test]
    fn subdirectory_inherits_the_project_root_file() {
        let nvm = fake_nvm(&["v24.19.0"]);
        let project = project_with(".nvmrc", "24");
        let sub = project.path().join("apps/api");
        fs::create_dir_all(&sub).unwrap();
        let result = select_node(&sub, project.path(), nvm.path());
        assert_eq!(selected_version(result), "24.19.0");
    }

    #[test]
    fn search_stops_at_the_project_root() {
        let nvm = fake_nvm(&["v24.19.0"]);
        let outer = project_with(".nvmrc", "24");
        let project = outer.path().join("project");
        fs::create_dir_all(&project).unwrap();
        let result = select_node(&project, &project, nvm.path()).unwrap();
        assert_eq!(result, NodeSelection::NotRequested);
    }

    #[test]
    fn alias_names_cannot_escape_the_alias_folder() {
        let nvm = fake_nvm(&["v24.19.0"]);
        fs::write(nvm.path().join("secret"), "24").unwrap();
        let err = select(&project_with(".nvmrc", "../secret"), &nvm).unwrap_err();
        assert!(err.contains("not a valid"), "{err}");
    }

    #[test]
    fn alias_loops_are_reported() {
        let nvm = fake_nvm(&["v24.19.0"]);
        alias(&nvm, "a", "b");
        alias(&nvm, "b", "a");
        let err = select(&project_with(".nvmrc", "a"), &nvm).unwrap_err();
        assert!(err.contains("loop"), "{err}");
    }

    #[test]
    fn describe_names_the_version_and_file() {
        let nvm = fake_nvm(&["v24.19.0"]);
        let selection = select(&project_with(".nvmrc", "24"), &nvm).unwrap();
        assert_eq!(selection.describe().unwrap(), "Using Node v24.19.0 from nvm (.nvmrc)");
        assert_eq!(NodeSelection::NotRequested.describe(), None);
    }
}
