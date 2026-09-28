//! Which project an MCP tool call acts on.
//!
//! Stage 5 of the project identity plan
//! (`automatic-meta/general/plans/projects/project-identity.md`).
//!
//! `mcp-serve` works out a *current project* once at startup with
//! [`resolve_current_project`]. Tools that take a `project` argument then
//! call [`resolve_project_target`]: an explicit argument wins, and an
//! omitted one falls back to the current project.
//!
//! Sync writes the project `id` into each agent config's
//! `AUTOMATIC_PROJECT`. The id is committed, so it is right on every
//! machine and in every clone. Several checkouts can share an id (git
//! worktrees, a second clone), so the server's working directory picks
//! between them. Older builds wrote the project name there, and a
//! `local_key` is accepted too.
//!
//! Both resolvers are pure functions over the registry summaries. The only
//! I/O is in [`detect_current_project`], which reads the environment, the
//! working directory and the registry.

use std::path::{Path, PathBuf};

use super::*;

/// The project a `mcp-serve` process defaults to.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum CurrentProject {
    /// Nothing identified a project. Tools need an explicit `project`.
    #[default]
    None,
    /// One checkout.
    Checkout(ProjectSummary),
    /// A project `id` with several checkouts, none of which contains the
    /// working directory. Memory and feature tools can still use the id.
    /// Tools that act on one checkout need an explicit `project`.
    Project {
        id: String,
        checkouts: Vec<ProjectSummary>,
    },
}

impl CurrentProject {
    /// Whether `summary` is the current project, or one of its checkouts
    /// when the checkout is not known.
    pub fn includes(&self, summary: &ProjectSummary) -> bool {
        match self {
            CurrentProject::None => false,
            CurrentProject::Checkout(current) => same_checkout(current, summary),
            CurrentProject::Project { id, .. } => !id.is_empty() && summary.id == *id,
        }
    }

    /// One line for the startup log.
    pub fn describe(&self) -> String {
        match self {
            CurrentProject::None => "none".to_string(),
            CurrentProject::Checkout(s) => format!("{}", CandidateLine(s)),
            CurrentProject::Project { id, checkouts } => format!(
                "project id {} with {} checkouts, none containing the working directory",
                id,
                checkouts.len()
            ),
        }
    }
}

/// What a tool call acts on after resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectTarget {
    /// One checkout.
    Checkout(ProjectSummary),
    /// Every checkout of one project `id`. Enough for id-keyed stores
    /// (memory, features). Not enough for a tool that reads or writes one
    /// checkout's configuration.
    Project {
        id: String,
        checkouts: Vec<ProjectSummary>,
    },
}

impl ProjectTarget {
    /// The single checkout this target names, or an error listing every
    /// candidate so the agent can pass one `local_key`.
    pub fn into_checkout(self) -> Result<ProjectSummary, String> {
        match self {
            ProjectTarget::Checkout(summary) => Ok(summary),
            ProjectTarget::Project { id, checkouts } => Err(format!(
                "Project id '{}' has {} checkouts on this machine, and the working directory \
                 is in none of them. Pass `project` with the local_key of the one you mean:\n{}",
                id,
                checkouts.len(),
                candidate_lines(&checkouts)
            )),
        }
    }

    /// A checkout to read project settings from. For a project with
    /// several checkouts this is the first in registry order, which is the
    /// choice `project_store_keys` makes for an `id`.
    pub fn representative(&self) -> &ProjectSummary {
        match self {
            ProjectTarget::Checkout(summary) => summary,
            // `resolve_project_target` never builds an empty list.
            ProjectTarget::Project { checkouts, .. } => &checkouts[0],
        }
    }

    /// Keys for the per-project stores. For a project with several
    /// checkouts, `id` is shared and `name`/`local_key` come from
    /// [`ProjectTarget::representative`].
    pub fn store_keys(&self) -> ProjectStoreKeys {
        ProjectStoreKeys::of_summary(self.representative())
    }
}

/// The identifier to pass to registry functions (`read_project`,
/// `save_project`) for one checkout: its `local_key`, or its name while it
/// has no key. A `local_key` stays unique when names repeat.
pub fn checkout_ident(summary: &ProjectSummary) -> &str {
    if summary.local_key.is_empty() {
        &summary.name
    } else {
        &summary.local_key
    }
}

/// Work out the current project of a `mcp-serve` process.
///
/// - `env_value` is `AUTOMATIC_PROJECT`. It is tried as a project `id`,
///   then a `local_key`, then a name (legacy configs hold the name).
/// - `cwd` is the process working directory, already canonicalised. It
///   picks between checkouts that share an id or a name. When
///   `env_value` is unset, empty or matches nothing, the deepest
///   registered directory containing `cwd` is used.
/// - `canonical` turns a registered directory into the form `cwd` is in.
///   Pass the identity in tests.
pub fn resolve_current_project<F>(
    env_value: Option<&str>,
    cwd: Option<&Path>,
    summaries: &[ProjectSummary],
    canonical: F,
) -> CurrentProject
where
    F: Fn(&str) -> PathBuf,
{
    if let Some(value) = env_value.map(str::trim).filter(|v| !v.is_empty()) {
        if let Some(found) = resolve_env_value(value, cwd, summaries, &canonical) {
            return found;
        }
    }
    match cwd.and_then(|cwd| deepest_containing(cwd, summaries.iter(), &canonical)) {
        Some(summary) => CurrentProject::Checkout(summary.clone()),
        None => CurrentProject::None,
    }
}

fn resolve_env_value<F>(
    value: &str,
    cwd: Option<&Path>,
    summaries: &[ProjectSummary],
    canonical: &F,
) -> Option<CurrentProject>
where
    F: Fn(&str) -> PathBuf,
{
    let by_id: Vec<&ProjectSummary> = summaries.iter().filter(|s| s.id == value).collect();
    if !by_id.is_empty() {
        return Some(pick_checkout(value, &by_id, cwd, canonical));
    }
    if let Some(summary) = summaries.iter().find(|s| s.local_key == value) {
        return Some(CurrentProject::Checkout(summary.clone()));
    }
    let named: Vec<&ProjectSummary> = summaries
        .iter()
        .filter(|s| same_project_name(&s.name, value))
        .collect();
    match named.as_slice() {
        [] => None,
        [only] => Some(CurrentProject::Checkout((*only).clone())),
        many => match shared_id(many) {
            Some(id) => Some(pick_checkout(id, many, cwd, canonical)),
            // Different projects share the name. Only the working directory
            // can tell them apart; without it there is no safe default.
            None => Some(
                cwd.and_then(|cwd| deepest_containing(cwd, many.iter().copied(), canonical))
                    .map_or(CurrentProject::None, |s| CurrentProject::Checkout(s.clone())),
            ),
        },
    }
}

/// One checkout of `id` from `checkouts` (never empty): the only one, or
/// the one containing `cwd`, or the whole project when neither applies.
fn pick_checkout<F>(
    id: &str,
    checkouts: &[&ProjectSummary],
    cwd: Option<&Path>,
    canonical: &F,
) -> CurrentProject
where
    F: Fn(&str) -> PathBuf,
{
    if let [only] = checkouts {
        return CurrentProject::Checkout((*only).clone());
    }
    match cwd.and_then(|cwd| deepest_containing(cwd, checkouts.iter().copied(), canonical)) {
        Some(summary) => CurrentProject::Checkout(summary.clone()),
        None => CurrentProject::Project {
            id: id.to_string(),
            checkouts: checkouts.iter().map(|s| (*s).clone()).collect(),
        },
    }
}

/// The candidate whose directory contains `cwd` with the most path
/// components. `Path::starts_with` compares whole components, so `/a/web`
/// does not contain `/a/website`. The first candidate wins a tie.
fn deepest_containing<'a, F>(
    cwd: &Path,
    candidates: impl Iterator<Item = &'a ProjectSummary>,
    canonical: &F,
) -> Option<&'a ProjectSummary>
where
    F: Fn(&str) -> PathBuf,
{
    let mut best: Option<(usize, &ProjectSummary)> = None;
    for summary in candidates {
        if summary.directory.is_empty() {
            continue;
        }
        let dir = canonical(&summary.directory);
        if !cwd.starts_with(&dir) {
            continue;
        }
        let depth = dir.components().count();
        if best.is_none_or(|(best_depth, _)| depth > best_depth) {
            best = Some((depth, summary));
        }
    }
    best.map(|(_, summary)| summary)
}

/// The `id` every summary carries, when they all carry the same non-empty
/// one.
fn shared_id<'a>(summaries: &[&'a ProjectSummary]) -> Option<&'a str> {
    let first = summaries.first()?.id.as_str();
    (!first.is_empty() && summaries.iter().all(|s| s.id == first)).then_some(first)
}

fn same_checkout(a: &ProjectSummary, b: &ProjectSummary) -> bool {
    if a.local_key.is_empty() || b.local_key.is_empty() {
        a.name == b.name && a.directory == b.directory
    } else {
        a.local_key == b.local_key
    }
}

/// Resolve a tool's `project` argument.
///
/// An explicit value is matched as a `local_key`, then a name (ignoring
/// case), then a project `id`. An omitted or blank value uses `current`.
/// A name or id shared by several checkouts of one project resolves to the
/// current checkout when it is one of them, else to the whole project.
/// A name shared by different projects is an error listing each one.
pub fn resolve_project_target(
    explicit: Option<&str>,
    current: &CurrentProject,
    summaries: &[ProjectSummary],
) -> Result<ProjectTarget, String> {
    if let Some(ident) = explicit.map(str::trim).filter(|v| !v.is_empty()) {
        return resolve_ident(ident, current, summaries)?
            .ok_or_else(|| unknown_project_error(ident, summaries));
    }
    let ident = match current {
        CurrentProject::None => return Err(NO_CURRENT_PROJECT.to_string()),
        CurrentProject::Checkout(summary) => checkout_ident(summary),
        CurrentProject::Project { id, .. } => id.as_str(),
    };
    // The registry is read again on every call, so a rename or a removed
    // checkout since startup is picked up here.
    resolve_ident(ident, current, summaries)?.ok_or_else(|| {
        format!(
            "The current project ({}) is no longer registered in Automatic. Pass `project`, \
             and call automatic_list_projects to see the registered projects.",
            current.describe()
        )
    })
}

const NO_CURRENT_PROJECT: &str = "No current project: Automatic could not tell which project \
     this agent is working in. Pass `project` (a name, local_key or id). Call \
     automatic_list_projects to see the registered projects.";

fn resolve_ident(
    ident: &str,
    current: &CurrentProject,
    summaries: &[ProjectSummary],
) -> Result<Option<ProjectTarget>, String> {
    if let Some(summary) = summaries.iter().find(|s| s.local_key == ident) {
        return Ok(Some(ProjectTarget::Checkout(summary.clone())));
    }
    let named: Vec<&ProjectSummary> = summaries
        .iter()
        .filter(|s| same_project_name(&s.name, ident))
        .collect();
    match named.as_slice() {
        [] => {}
        [only] => return Ok(Some(ProjectTarget::Checkout((*only).clone()))),
        many => {
            return match shared_id(many) {
                Some(id) => Ok(Some(target_for_checkouts(id, many, current))),
                None => Err(ambiguous_name_error(
                    ident,
                    &many.iter().map(|s| (*s).clone()).collect::<Vec<_>>(),
                )),
            }
        }
    }
    let by_id: Vec<&ProjectSummary> = summaries.iter().filter(|s| s.id == ident).collect();
    if by_id.is_empty() {
        return Ok(None);
    }
    Ok(Some(target_for_checkouts(ident, &by_id, current)))
}

fn target_for_checkouts(
    id: &str,
    checkouts: &[&ProjectSummary],
    current: &CurrentProject,
) -> ProjectTarget {
    if let [only] = checkouts {
        return ProjectTarget::Checkout((*only).clone());
    }
    if let CurrentProject::Checkout(current) = current {
        if let Some(found) = checkouts.iter().find(|s| same_checkout(current, s)) {
            return ProjectTarget::Checkout((*found).clone());
        }
    }
    ProjectTarget::Project {
        id: id.to_string(),
        checkouts: checkouts.iter().map(|s| (*s).clone()).collect(),
    }
}

fn unknown_project_error(ident: &str, summaries: &[ProjectSummary]) -> String {
    let mut names: Vec<&str> = summaries.iter().map(|s| s.name.as_str()).collect();
    names.dedup();
    let list = if names.is_empty() {
        "no projects registered yet".to_string()
    } else {
        names.join(", ")
    };
    format!(
        "Unknown project '{}'. Valid project names are: {}. \
         Call automatic_list_projects to confirm the correct name before retrying.",
        ident, list
    )
}

/// The error for a name that several different projects share. Lists each
/// candidate as `name — directory (local_key)` so the caller can name the
/// one it means by its `local_key`. Shared by every by-name lookup: the
/// registry resolver, the store-key resolver and MCP tool arguments.
pub(crate) fn ambiguous_name_error(name: &str, candidates: &[ProjectSummary]) -> String {
    format!(
        "More than one project is named '{}'. Use the local_key of the one you mean:\n{}",
        name,
        candidate_lines(candidates)
    )
}

/// One candidate per line: `name — directory (local_key)`.
fn candidate_lines(summaries: &[ProjectSummary]) -> String {
    summaries
        .iter()
        .map(|s| format!("- {}", CandidateLine(s)))
        .collect::<Vec<_>>()
        .join("\n")
}

struct CandidateLine<'a>(&'a ProjectSummary);

impl std::fmt::Display for CandidateLine<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self.0;
        let directory = if s.directory.is_empty() {
            "no folder"
        } else {
            s.directory.as_str()
        };
        let key = if s.local_key.is_empty() {
            "no local_key"
        } else {
            s.local_key.as_str()
        };
        write!(f, "{} — {} ({})", s.name, directory, key)
    }
}

/// A registered directory in canonical form, or as written when it cannot
/// be canonicalised (it was moved or deleted).
fn canonical_or_raw(directory: &str) -> PathBuf {
    std::fs::canonicalize(directory).unwrap_or_else(|_| PathBuf::from(directory))
}

/// Read `AUTOMATIC_PROJECT`, the working directory and the registry, and
/// resolve the current project. Logs the result to stderr: stdout carries
/// the MCP protocol.
pub fn detect_current_project() -> CurrentProject {
    let env_value = std::env::var("AUTOMATIC_PROJECT").ok();
    let cwd = std::env::current_dir()
        .ok()
        .map(|dir| std::fs::canonicalize(&dir).unwrap_or(dir));
    let summaries = match get_project_summaries() {
        Ok(summaries) => summaries,
        Err(e) => {
            eprintln!(
                "[automatic] mcp-serve: could not read the project registry, so there is no \
                 current project: {}",
                e
            );
            return CurrentProject::None;
        }
    };
    let current =
        resolve_current_project(env_value.as_deref(), cwd.as_deref(), &summaries, canonical_or_raw);
    eprintln!(
        "[automatic] mcp-serve: current project: {} (AUTOMATIC_PROJECT={:?}, cwd={})",
        current.describe(),
        env_value.unwrap_or_default(),
        cwd.map_or_else(|| "unknown".to_string(), |d| d.display().to_string())
    );
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID_A: &str = "11111111-1111-4111-8111-111111111111";
    const ID_B: &str = "22222222-2222-4222-8222-222222222222";
    const KEY_A1: &str = "aaaaaaaa-0000-4000-8000-000000000001";
    const KEY_A2: &str = "aaaaaaaa-0000-4000-8000-000000000002";
    const KEY_B: &str = "bbbbbbbb-0000-4000-8000-000000000001";

    fn summary(name: &str, id: &str, local_key: &str, directory: &str) -> ProjectSummary {
        ProjectSummary {
            local_key: local_key.into(),
            id: id.into(),
            name: name.into(),
            directory: directory.into(),
        }
    }

    /// `site` has two checkouts (a main folder and a worktree); `api` has one.
    fn registry() -> Vec<ProjectSummary> {
        vec![
            summary("api", ID_B, KEY_B, "/work/api"),
            summary("site", ID_A, KEY_A1, "/work/site"),
            summary("site-wt", ID_A, KEY_A2, "/work/trees/site-wt"),
        ]
    }

    fn identity(dir: &str) -> PathBuf {
        PathBuf::from(dir)
    }

    fn current(env: Option<&str>, cwd: Option<&str>) -> CurrentProject {
        resolve_current_project(env, cwd.map(Path::new), &registry(), identity)
    }

    fn checkout_key(current: &CurrentProject) -> Option<&str> {
        match current {
            CurrentProject::Checkout(s) => Some(s.local_key.as_str()),
            _ => None,
        }
    }

    #[test]
    fn an_id_with_one_checkout_needs_no_cwd() {
        assert_eq!(checkout_key(&current(Some(ID_B), None)), Some(KEY_B));
        assert_eq!(checkout_key(&current(Some(ID_B), Some("/elsewhere"))), Some(KEY_B));
    }

    #[test]
    fn a_shared_id_picks_the_checkout_containing_cwd() {
        assert_eq!(checkout_key(&current(Some(ID_A), Some("/work/site/src"))), Some(KEY_A1));
        assert_eq!(checkout_key(&current(Some(ID_A), Some("/work/trees/site-wt"))), Some(KEY_A2));
    }

    #[test]
    fn a_shared_id_picks_the_deepest_checkout() {
        let nested = vec![
            summary("outer", ID_A, KEY_A1, "/work/site"),
            summary("inner", ID_A, KEY_A2, "/work/site/.worktrees/feature"),
        ];
        let found = resolve_current_project(
            Some(ID_A),
            Some(Path::new("/work/site/.worktrees/feature/src")),
            &nested,
            identity,
        );
        assert_eq!(checkout_key(&found), Some(KEY_A2));
    }

    #[test]
    fn a_shared_id_outside_every_checkout_keeps_only_the_id() {
        match current(Some(ID_A), Some("/tmp")) {
            CurrentProject::Project { id, checkouts } => {
                assert_eq!(id, ID_A);
                assert_eq!(checkouts.len(), 2);
            }
            other => panic!("expected the id alone, got {other:?}"),
        }
        assert!(matches!(current(Some(ID_A), None), CurrentProject::Project { .. }));
    }

    #[test]
    fn a_legacy_name_resolves() {
        assert_eq!(checkout_key(&current(Some("API"), None)), Some(KEY_B), "ignoring case");
    }

    #[test]
    fn a_local_key_resolves() {
        assert_eq!(checkout_key(&current(Some(KEY_A2), Some("/work/site"))), Some(KEY_A2));
    }

    #[test]
    fn no_env_falls_back_to_cwd() {
        assert_eq!(checkout_key(&current(None, Some("/work/api/src/lib"))), Some(KEY_B));
        assert_eq!(checkout_key(&current(Some("  "), Some("/work/api"))), Some(KEY_B));
    }

    #[test]
    fn an_unknown_env_value_falls_back_to_cwd() {
        assert_eq!(checkout_key(&current(Some("ghost"), Some("/work/api"))), Some(KEY_B));
    }

    #[test]
    fn nothing_matching_is_no_current_project() {
        assert_eq!(current(None, Some("/tmp")), CurrentProject::None);
        assert_eq!(current(Some("ghost"), None), CurrentProject::None);
        assert_eq!(current(None, None), CurrentProject::None);
    }

    #[test]
    fn a_directory_prefix_only_matches_whole_components() {
        let summaries = vec![summary("web", ID_B, KEY_B, "/a/web")];
        let found = resolve_current_project(None, Some(Path::new("/a/website")), &summaries, identity);
        assert_eq!(found, CurrentProject::None);
        let found =
            resolve_current_project(None, Some(Path::new("/a/web/src")), &summaries, identity);
        assert_eq!(checkout_key(&found), Some(KEY_B));
    }

    #[test]
    fn a_name_shared_by_different_projects_needs_cwd() {
        let summaries = vec![
            summary("site", ID_A, KEY_A1, "/one/site"),
            summary("site", ID_B, KEY_B, "/two/site"),
        ];
        let none = resolve_current_project(Some("site"), None, &summaries, identity);
        assert_eq!(none, CurrentProject::None);
        let found =
            resolve_current_project(Some("site"), Some(Path::new("/two/site")), &summaries, identity);
        assert_eq!(checkout_key(&found), Some(KEY_B));
    }

    #[test]
    fn a_symlinked_directory_matches_its_canonical_cwd() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let real = tmp.path().join("real");
        std::fs::create_dir_all(real.join("src")).expect("mkdir");
        let link = tmp.path().join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, &link).expect("symlink");
        #[cfg(not(unix))]
        return;

        let summaries = vec![summary("site", ID_A, KEY_A1, link.to_str().unwrap())];
        let cwd = std::fs::canonicalize(real.join("src")).expect("canonicalize");
        let found = resolve_current_project(None, Some(&cwd), &summaries, canonical_or_raw);
        assert_eq!(checkout_key(&found), Some(KEY_A1));
        let raw = resolve_current_project(None, Some(&cwd), &summaries, identity);
        assert_eq!(raw, CurrentProject::None, "without canonicalising, the link does not match");
    }

    // ── tool targets ────────────────────────────────────────────────────

    fn target(explicit: Option<&str>, current: &CurrentProject) -> Result<ProjectTarget, String> {
        resolve_project_target(explicit, current, &registry())
    }

    fn target_key(target: &ProjectTarget) -> Option<&str> {
        match target {
            ProjectTarget::Checkout(s) => Some(s.local_key.as_str()),
            ProjectTarget::Project { .. } => None,
        }
    }

    #[test]
    fn an_omitted_project_uses_the_current_one() {
        let api = current(Some(ID_B), None);
        assert_eq!(target_key(&target(None, &api).unwrap()), Some(KEY_B));
        assert_eq!(target_key(&target(Some(""), &api).unwrap()), Some(KEY_B));
    }

    #[test]
    fn an_explicit_key_name_or_id_wins() {
        let api = current(Some(ID_B), None);
        assert_eq!(target_key(&target(Some(KEY_A2), &api).unwrap()), Some(KEY_A2));
        assert_eq!(target_key(&target(Some("Site"), &api).unwrap()), Some(KEY_A1));
        assert_eq!(target_key(&target(Some(ID_B), &CurrentProject::None).unwrap()), Some(KEY_B));
    }

    #[test]
    fn a_shared_id_prefers_the_current_checkout() {
        let worktree = current(Some(KEY_A2), None);
        assert_eq!(target_key(&target(Some(ID_A), &worktree).unwrap()), Some(KEY_A2));
        let found = target(Some(ID_A), &CurrentProject::None).unwrap();
        assert!(matches!(found, ProjectTarget::Project { .. }));
        assert_eq!(found.store_keys().id, ID_A, "id-keyed stores still work");
    }

    #[test]
    fn a_checkout_tool_on_an_ambiguous_id_lists_the_candidates() {
        let id_only = current(Some(ID_A), Some("/tmp"));
        let err = target(None, &id_only).unwrap().into_checkout().unwrap_err();
        assert!(err.contains(&format!("site — /work/site ({})", KEY_A1)), "{err}");
        assert!(err.contains(&format!("site-wt — /work/trees/site-wt ({})", KEY_A2)), "{err}");
    }

    #[test]
    fn a_name_shared_by_different_projects_is_an_error() {
        let summaries = vec![
            summary("site", ID_A, KEY_A1, "/one/site"),
            summary("site", ID_B, KEY_B, "/two/site"),
        ];
        let err =
            resolve_project_target(Some("site"), &CurrentProject::None, &summaries).unwrap_err();
        assert!(err.contains("More than one project is named 'site'"), "{err}");
        assert!(err.contains(KEY_A1) && err.contains(KEY_B), "{err}");
    }

    #[test]
    fn no_current_project_and_no_argument_explains_what_to_do() {
        let err = target(None, &CurrentProject::None).unwrap_err();
        assert!(err.contains("Pass `project`"), "{err}");
        assert!(err.contains("automatic_list_projects"), "{err}");
    }

    #[test]
    fn an_unknown_project_lists_the_names() {
        let err = target(Some("ghost"), &CurrentProject::None).unwrap_err();
        assert!(err.contains("Unknown project 'ghost'"), "{err}");
        assert!(err.contains("api, site, site-wt"), "{err}");
    }

    #[test]
    fn a_current_project_removed_since_startup_is_reported() {
        let gone = CurrentProject::Checkout(summary("old", ID_A, "cccccccc-0000-4000-8000-000000000001", "/x"));
        let err = target(None, &gone).unwrap_err();
        assert!(err.contains("no longer registered"), "{err}");
    }

    #[test]
    fn includes_marks_the_checkout_or_every_checkout_of_the_id() {
        let summaries = registry();
        let worktree = current(Some(KEY_A2), None);
        let marked: Vec<bool> = summaries.iter().map(|s| worktree.includes(s)).collect();
        assert_eq!(marked, vec![false, false, true]);
        let id_only = current(Some(ID_A), None);
        let marked: Vec<bool> = summaries.iter().map(|s| id_only.includes(s)).collect();
        assert_eq!(marked, vec![false, true, true]);
    }
}
