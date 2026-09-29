//! Removing one agent from a project.
//!
//! The user picks one of two modes when they remove an agent:
//!
//! - [`RemovalMode::Remove`] deletes every trace of the agent's config from
//!   the project directory.
//! - [`RemovalMode::Keep`] only takes the agent off the project's agent list,
//!   so Automatic stops syncing it.  No file on disk changes.
//!
//! Remove works from each agent's declaration of what it writes:
//!
//! - [`Agent::owned_dirs`]: directories the agent owns outright.  They are
//!   deleted whole, including files the user added.
//! - The instruction file, skill, sub-agent and command directories,
//!   [`Agent::owned_config_paths`], an owned hook file and
//!   [`Agent::owned_extra_files`]: deleted whole.
//! - [`Agent::mcp_merge_inputs`] the agent does not own, and a merged hook
//!   file: only Automatic's entries are stripped.
//!
//! A path is shared when another agent that stays in the project claims it,
//! or claims something inside it.  Shared paths are left alone, so the root
//! `AGENTS.md` and the `.agents/skills` hub survive until the last agent that
//! uses them goes.
//!
//! [`plan_agent_removal`] is read-only and feeds the confirmation dialog.
//! [`apply_removal_plan`] carries out exactly that plan, so the dialog always
//! lists what removal does.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::Serialize;

use super::{from_id, Agent, HookConfigTarget};

/// What happens to the agent's files when it leaves a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovalMode {
    /// Delete the agent's config from the project directory.
    Remove,
    /// Stop syncing the agent and leave every file in place.
    Keep,
}

impl FromStr for RemovalMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "remove" => Ok(Self::Remove),
            "keep" => Ok(Self::Keep),
            other => Err(format!(
                "Unknown removal mode '{}': expected 'remove' or 'keep'",
                other
            )),
        }
    }
}

/// What removal does to one path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RemovalAction {
    /// Deleted.  A directory goes with everything inside it.
    Delete,
    /// A directory left empty by the deletions, removed afterwards.
    RemoveEmptyDir,
    /// Only Automatic's entries are removed.  The rest of the file stays.
    Strip,
    /// Left in place because another agent in the project still uses it.
    KeepShared,
}

/// One line of the removal preview, and of the removal result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemovalEntry {
    /// Absolute path.
    pub path: String,
    /// Path relative to the project directory, for display.
    pub relative_path: String,
    pub action: RemovalAction,
    pub is_dir: bool,
    /// Labels of the remaining agents that use the path.  Only set for
    /// [`RemovalAction::KeepShared`].
    pub shared_with: Vec<String>,
}

/// Which strip operation clears Automatic's entries from a merged file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StripKind {
    /// [`Agent::cleanup_mcp_config`].
    McpConfig,
    /// [`Agent::sync_hooks`] with no hooks.
    Hooks,
}

#[derive(Debug, Clone)]
struct PlannedEntry {
    entry: RemovalEntry,
    strip: Option<StripKind>,
    /// True for a `Delete` on an owned directory that also holds a strip
    /// target — apply must first strip the file inside, then delete the
    /// directory only if it ends up empty.  A strip that keeps user content
    /// leaves the file behind, and the directory survives with it.
    soft_delete: bool,
}

/// The full effect of removing one agent in [`RemovalMode::Remove`].
#[derive(Debug, Clone, Default)]
pub struct RemovalPlan {
    items: Vec<PlannedEntry>,
    /// Names of MCP servers Automatic wrote for this project.  Threaded into
    /// each `cleanup_mcp_config` call so the strip touches only the entries
    /// Automatic owns; user-added servers in the same file survive.  Recorded
    /// on the plan so `apply_removal_plan` acts on the same list the preview
    /// used.
    managed_mcp_servers: Vec<String>,
}

impl RemovalPlan {
    pub fn entries(&self) -> Vec<RemovalEntry> {
        self.items.iter().map(|item| item.entry.clone()).collect()
    }
}

/// A path that Remove deletes, or strips when `strip` is set.
struct Candidate {
    path: PathBuf,
    strip: Option<StripKind>,
}

/// Every path Automatic writes for `agent` under `dir`, in the order Remove
/// considers them.  Another agent's removal must leave these alone while
/// `agent` stays in the project.
fn candidates(agent: &dyn Agent, dir: &Path) -> Vec<Candidate> {
    let mut deletes: Vec<PathBuf> = agent.owned_dirs(dir);

    let instruction_file = agent.project_file_name();
    deletes.push(dir.join(instruction_file));
    deletes.push(crate::core::instruction_snapshot_path(
        dir,
        instruction_file,
    ));
    deletes.extend(agent.skill_dirs(dir));
    deletes.extend(agent.agents_dir(dir));
    deletes.extend(agent.commands_dir(dir));
    let owned_config = agent.owned_config_paths(dir);
    deletes.extend(owned_config.iter().cloned());
    let hook_target = agent.hook_config_target(dir);
    if let Some(HookConfigTarget::Owned { path }) = &hook_target {
        deletes.push(path.clone());
    }
    deletes.extend(agent.owned_extra_files(dir));

    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut out: Vec<Candidate> = Vec::new();
    for path in deletes {
        if seen.insert(path.clone()) {
            out.push(Candidate { path, strip: None });
        }
    }

    // A file the agent both owns and merges into (OpenCode's `opencode.json`)
    // is owned: it was added above and `seen` skips it here.
    for path in agent.mcp_merge_inputs(dir) {
        if seen.insert(path.clone()) {
            out.push(Candidate {
                path,
                strip: Some(StripKind::McpConfig),
            });
        }
    }
    if let Some(HookConfigTarget::Merged { path, .. }) = hook_target {
        if seen.insert(path.clone()) {
            out.push(Candidate {
                path,
                strip: Some(StripKind::Hooks),
            });
        }
    }
    out
}

fn is_home_dir(dir: &Path) -> bool {
    let Some(home) = super::home_dir() else {
        return false;
    };
    match (fs::canonicalize(dir), fs::canonicalize(&home)) {
        (Ok(a), Ok(b)) => a == b,
        _ => dir == home.as_path(),
    }
}

/// True when one path is the other, or contains it.
fn paths_overlap(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}

/// True when `path` is inside (not equal to) one of `roots`.
fn inside_any(path: &Path, roots: &[PathBuf]) -> bool {
    roots
        .iter()
        .any(|root| path != root.as_path() && path.starts_with(root))
}

/// Metadata without following a symlink, so a symlinked skill or directory
/// is treated as the link itself.
fn path_kind(path: &Path) -> Option<bool> {
    fs::symlink_metadata(path).ok().map(|meta| meta.is_dir())
}

fn relative_display(path: &Path, dir: &Path) -> String {
    path.strip_prefix(dir).unwrap_or(path).display().to_string()
}

fn make_entry(
    path: &Path,
    dir: &Path,
    action: RemovalAction,
    is_dir: bool,
    shared_with: Vec<String>,
) -> RemovalEntry {
    RemovalEntry {
        path: path.display().to_string(),
        relative_path: relative_display(path, dir),
        action,
        is_dir,
        shared_with,
    }
}

/// Plan what [`RemovalMode::Remove`] does when `agent` leaves the project in
/// `dir` and `remaining_agent_ids` stay.  Read-only.
///
/// `managed_mcp_servers` names the entries Automatic wrote to this project's
/// shared MCP config files (e.g. `.vscode/mcp.json` for Copilot).  Strips
/// touch only these names; user-added entries in the same file survive.
///
/// Refuses a project whose directory is the home directory.  There
/// `.claude/` and the other owned directories hold the agents' global config.
pub fn plan_agent_removal(
    agent: &dyn Agent,
    dir: &Path,
    remaining_agent_ids: &[String],
    managed_mcp_servers: &[String],
) -> Result<RemovalPlan, String> {
    if is_home_dir(dir) {
        return Err(format!(
            "'{}' is your home folder, so {}'s files there are its global settings. \
             Choose Keep to stop syncing it without deleting anything.",
            dir.display(),
            agent.label()
        ));
    }
    let remaining: Vec<(&'static str, Vec<PathBuf>)> = remaining_agent_ids
        .iter()
        .filter(|id| id.as_str() != agent.id())
        .filter_map(|id| from_id(id))
        .map(|other| {
            let claims = candidates(other, dir).into_iter().map(|c| c.path);
            (other.label(), claims.collect())
        })
        .collect();
    let shared_with = |path: &Path| -> Vec<String> {
        remaining
            .iter()
            .filter(|(_, claims)| claims.iter().any(|claim| paths_overlap(claim, path)))
            .map(|(label, _)| label.to_string())
            .collect()
    };

    let all_candidates = candidates(agent, dir);
    // Owned dirs that hold a merged MCP strip target on disk cannot be wiped
    // whole — the target file may carry user hand-edits alongside Automatic's
    // entries.  Apply runs the strip first (see VEL-174), which preserves the
    // user's content, and then deletes the dir only if the strip left it
    // empty.  Hook-only strip targets don't trigger this: those owned dirs
    // are wiped whole as before, keeping the pre-VEL-174 behaviour for
    // agents such as Claude Code.
    let soft_delete_dirs = compute_soft_delete_dirs(agent, dir, &all_candidates);

    let mut items: Vec<PlannedEntry> = Vec::new();
    let mut deleted: Vec<PathBuf> = Vec::new();
    let mut strip_changes: Option<StripChanges> = None;

    for candidate in all_candidates {
        let path = candidate.path;
        let Some(is_dir) = path_kind(&path) else {
            continue;
        };
        if inside_any(&path, &deleted) {
            // A hard delete on a parent will remove this subtree; skip
            // planning it separately.  Soft-delete parents are not in
            // `deleted`, so their subpaths reach this loop and get their
            // own entries.
            continue;
        }

        let users = shared_with(&path);
        if !users.is_empty() {
            items.push(PlannedEntry {
                entry: make_entry(&path, dir, RemovalAction::KeepShared, is_dir, users),
                strip: None,
                soft_delete: false,
            });
            continue;
        }

        match candidate.strip {
            None => {
                let soft = is_dir && soft_delete_dirs.contains(&path);
                items.push(PlannedEntry {
                    entry: make_entry(&path, dir, RemovalAction::Delete, is_dir, vec![]),
                    strip: None,
                    soft_delete: soft,
                });
                if !soft {
                    // Only a hard delete subsumes subpaths.  A soft-delete
                    // parent must let its Automatic-owned subpaths (e.g.
                    // `.codex/agents/`) plan their own Delete entries so
                    // they still get removed when the parent survives.
                    deleted.push(path);
                }
            }
            Some(kind) => {
                let changes = strip_changes
                    .get_or_insert_with(|| dry_run_strips(agent, dir, managed_mcp_servers));
                if changes.changes(&path, kind) {
                    items.push(PlannedEntry {
                        entry: make_entry(&path, dir, RemovalAction::Strip, is_dir, vec![]),
                        strip: Some(kind),
                        soft_delete: false,
                    });
                }
            }
        }
    }

    items.extend(plan_empty_dirs(dir, &deleted));
    Ok(RemovalPlan {
        items,
        managed_mcp_servers: managed_mcp_servers.to_vec(),
    })
}

/// Owned directories that host an MCP-strip candidate present on disk.  A
/// soft-delete parent must delete only when the strip empties it, so a user
/// hand-edited `.codex/config.toml`, `.gemini/settings.json` or
/// `.zed/settings.json` survives Remove alongside the parent directory.
fn compute_soft_delete_dirs(
    agent: &dyn Agent,
    _dir: &Path,
    all_candidates: &[Candidate],
) -> HashSet<PathBuf> {
    let owned: Vec<PathBuf> = agent.owned_dirs(_dir);
    let mut soft: HashSet<PathBuf> = HashSet::new();
    for cand in all_candidates {
        if cand.strip != Some(StripKind::McpConfig) {
            continue;
        }
        if path_kind(&cand.path).is_none() {
            continue;
        }
        for od in &owned {
            if cand.path.starts_with(od) && cand.path != *od {
                soft.insert(od.clone());
            }
        }
    }
    soft
}

/// Parent directories that hold nothing once `deleted` is gone, deepest
/// first.  The project directory itself is never removed.
fn plan_empty_dirs(dir: &Path, deleted: &[PathBuf]) -> Vec<PlannedEntry> {
    let mut gone: HashSet<PathBuf> = deleted.iter().cloned().collect();
    let mut out = Vec::new();
    for path in deleted {
        let mut parent = path.parent();
        while let Some(candidate) = parent {
            if candidate == dir || !candidate.starts_with(dir) || gone.contains(candidate) {
                break;
            }
            if !dir_empty_after_removal(candidate, &gone) {
                break;
            }
            gone.insert(candidate.to_path_buf());
            out.push(PlannedEntry {
                entry: make_entry(candidate, dir, RemovalAction::RemoveEmptyDir, true, vec![]),
                strip: None,
                soft_delete: false,
            });
            parent = candidate.parent();
        }
    }
    out
}

/// Whether `dir` would be empty once every path in `removed` is gone.
fn dir_empty_after_removal(dir: &Path, removed: &HashSet<PathBuf>) -> bool {
    if !dir.is_dir() {
        return false;
    }
    match fs::read_dir(dir) {
        Ok(entries) => entries
            .map(|entry| entry.map(|e| removed.contains(&e.path())))
            .all(|gone| gone.unwrap_or(false)),
        // Removal would fail to inspect it too; do not promise to remove it.
        Err(_) => false,
    }
}

/// Per-kind dry-run result: the paths each strip kind would actually
/// change.  A file is only emitted as a Strip for a given kind when THAT
/// kind's strip touches it — a cosmetic hook rewrite must not trip a
/// Strip on a file whose MCP entries have nothing to remove, and vice
/// versa.
struct StripChanges {
    mcp: HashSet<PathBuf>,
    hooks: HashSet<PathBuf>,
}

impl StripChanges {
    fn changes(&self, path: &Path, kind: StripKind) -> bool {
        match kind {
            StripKind::McpConfig => self.mcp.contains(path),
            StripKind::Hooks => self.hooks.contains(path),
        }
    }
}

/// Run the agent's strip operations against copies of its merged files and
/// report, per kind, which real paths each strip would change.  The copies
/// keep the same relative layout so the strip code sees the same shape.
fn dry_run_strips(agent: &dyn Agent, dir: &Path, managed_mcp_servers: &[String]) -> StripChanges {
    let mut merged: Vec<PathBuf> = agent.mcp_merge_inputs(dir);
    if let Some(HookConfigTarget::Merged { path, .. }) = agent.hook_config_target(dir) {
        if !merged.contains(&path) {
            merged.push(path);
        }
    }

    let mcp = dry_run_one_kind(dir, &merged, |root| {
        agent.cleanup_mcp_config(root, managed_mcp_servers);
    });
    let hooks = dry_run_one_kind(dir, &merged, |root| {
        if let Err(e) = agent.sync_hooks(root, &[]) {
            eprintln!("[automatic] removal preview: hook strip failed: {}", e);
        }
    });
    StripChanges { mcp, hooks }
}

/// Copy each `merged` file into a fresh temp tree at the same relative
/// path, run `apply_one`, and return the real paths whose bytes changed
/// (a deletion counts as a change).
fn dry_run_one_kind<F>(dir: &Path, merged: &[PathBuf], apply_one: F) -> HashSet<PathBuf>
where
    F: FnOnce(&Path),
{
    let mut changed = HashSet::new();
    let tmp = match tempfile::tempdir() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[automatic] removal preview: cannot create tempdir: {}", e);
            return changed;
        }
    };

    let mut copies: Vec<(PathBuf, PathBuf, Vec<u8>)> = Vec::new();
    for real in merged {
        let Ok(relative) = real.strip_prefix(dir) else {
            continue;
        };
        let Ok(bytes) = fs::read(real) else {
            continue;
        };
        let copy = tmp.path().join(relative);
        if let Some(parent) = copy.parent() {
            if fs::create_dir_all(parent).is_err() {
                continue;
            }
        }
        if fs::write(&copy, &bytes).is_ok() {
            copies.push((real.clone(), copy, bytes));
        }
    }
    if copies.is_empty() {
        return changed;
    }

    apply_one(tmp.path());

    for (real, copy, before) in copies {
        let after = fs::read(&copy).ok();
        if after.as_deref() != Some(before.as_slice()) {
            changed.insert(real);
        }
    }
    changed
}

fn remove_path(path: &Path, is_dir: bool) -> Result<(), String> {
    let result = if is_dir {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    result.map_err(|e| format!("{}: {}", path.display(), e))
}

/// Carry out `plan`.  Strips run first so user hand-edits inside a
/// to-be-deleted owned directory survive; then the deletes, with a
/// soft-delete parent (see [`PlannedEntry::soft_delete`]) only removed when
/// the strip left it empty; finally the emptied parent directories.  Returns
/// the entries that actually took effect, in plan order.  A soft-delete
/// parent whose strip preserved user content silently drops off the returned
/// list — the parent survives with the file the user cares about.  When any
/// step fails the error lists every failure; the steps that succeeded stay
/// done.
pub fn apply_removal_plan(
    agent: &dyn Agent,
    dir: &Path,
    plan: &RemovalPlan,
) -> Result<Vec<RemovalEntry>, String> {
    let mut failed: HashSet<usize> = HashSet::new();
    let mut skipped: HashSet<usize> = HashSet::new();
    let mut errors: Vec<String> = Vec::new();

    // Phase 1 — strips.  Run before any delete so files inside a soft-delete
    // parent (e.g. `.codex/config.toml` inside `.codex/`) get a chance to
    // preserve user content before the parent decides whether to disappear.
    let strips = |kind: StripKind| {
        plan.items
            .iter()
            .enumerate()
            .filter(move |(_, item)| item.strip == Some(kind))
            .map(|(idx, _)| idx)
            .collect::<Vec<usize>>()
    };
    let mcp_strips = strips(StripKind::McpConfig);
    if !mcp_strips.is_empty() {
        let touched: HashSet<String> = agent
            .cleanup_mcp_config(dir, &plan.managed_mcp_servers)
            .into_iter()
            .collect();
        for idx in mcp_strips {
            let path = &plan.items[idx].entry.path;
            if !touched.contains(path) {
                failed.insert(idx);
                errors.push(format!(
                    "{}: Automatic's MCP entries were not removed",
                    path
                ));
            }
        }
    }
    let hook_strips = strips(StripKind::Hooks);
    if !hook_strips.is_empty() {
        if let Err(e) = agent.sync_hooks(dir, &[]) {
            failed.extend(hook_strips);
            errors.push(format!("hooks: {}", e));
        }
    }

    // Phase 2 — deletes.  A soft-delete directory is removed only if strips
    // left it empty; otherwise the delete is a silent no-op so user content
    // survives (VEL-174).  Hard deletes run as before.
    for (idx, item) in plan.items.iter().enumerate() {
        if item.entry.action != RemovalAction::Delete {
            continue;
        }
        let path = Path::new(&item.entry.path);
        if item.soft_delete {
            match dir_is_empty(path) {
                Ok(true) => {
                    if let Err(e) = fs::remove_dir(path) {
                        failed.insert(idx);
                        errors.push(format!("{}: {}", item.entry.path, e));
                    }
                }
                Ok(false) => {
                    // Strip preserved user content; parent stays.
                    skipped.insert(idx);
                }
                Err(_) => {
                    // The directory already disappeared (e.g. strip removed
                    // the last file and something else pruned it).  Treat as
                    // "nothing to do" rather than an error.
                    skipped.insert(idx);
                }
            }
        } else if let Err(e) = remove_path(path, item.entry.is_dir) {
            failed.insert(idx);
            errors.push(e);
        }
    }

    // Phase 3 — parent directories left empty by the deletes.
    for (idx, item) in plan.items.iter().enumerate() {
        if item.entry.action != RemovalAction::RemoveEmptyDir {
            continue;
        }
        // `remove_dir` refuses a directory that is not empty, so a file that
        // appeared since the plan was made keeps its directory.
        if let Err(e) = fs::remove_dir(&item.entry.path) {
            failed.insert(idx);
            errors.push(format!("{}: {}", item.entry.path, e));
        }
    }

    if !errors.is_empty() {
        return Err(format!(
            "Could not finish removing {}'s files: {}",
            agent.label(),
            errors.join("; ")
        ));
    }
    Ok(plan
        .items
        .iter()
        .enumerate()
        .filter(|(idx, _)| !failed.contains(idx) && !skipped.contains(idx))
        .map(|(_, item)| item.entry.clone())
        .collect())
}

/// Whether the directory holds no entries — a symlinked or vanished
/// directory returns an error so the caller can decide what to do.
fn dir_is_empty(path: &Path) -> std::io::Result<bool> {
    let mut entries = fs::read_dir(path)?;
    Ok(entries.next().is_none())
}

#[cfg(test)]
#[path = "removal_tests.rs"]
mod tests;
