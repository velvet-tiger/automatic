//! Folders Claude Code may edit outside a project's own folder.
//!
//! Claude Code's `permissions.blockWritesOutsideWorkingDirectories` stops its
//! file tools at the session's working folders. Entries in
//! `permissions.additionalDirectories` count as working folders, but only in
//! user settings, managed settings, and a `.claude/settings.local.json` that
//! git does not track. Automatic adds entries for a project's related folders
//! and never turns the block off. The plan of record is
//! `automatic-meta/general/plans/projects/writable-folders.md`.
//!
//! A project wants three kinds of folder:
//!
//! - the other members of each group it joined whose "Let agents edit across
//!   this group" switch is on (`ProjectGroup::agents_edit_across_group`), when
//!   that member joined the group on this machine too;
//! - the extra folders in its registry pointer (`local_writable_folders`);
//! - for a linked worktree, its main checkout.
//!
//! The entries go into the project's local settings file when Claude Code
//! honours it there ([`choose_target`]), and into user settings otherwise.
//! User settings apply to every session on the machine, so nothing is written
//! there until the user agrees (`Settings::writable_folders_user_consent`).
//!
//! Automatic changes only the entries it wrote. Its record lives in
//! `~/.automatic/claude-directories.json`, keyed by the checkout's
//! `local_key` and folder. A path two checkouts want stays in user settings
//! until neither wants it. An entry the user deleted is not added back while
//! it stays wanted.
//!
//! The pure parts ([`merge_directories`], [`choose_target`],
//! [`resolve_folders`], [`assess`]) take gathered facts. The functions below
//! them gather the facts and do the writes.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::claude_settings_file::{
    followed, hold_settings_writes, load, parse_root, replace_text_unless_changed,
    user_settings_path, write_root, Permissions,
};
use super::git_guard;
use super::*;

/// The agent id of Claude Code.
const CLAUDE_AGENT: &str = "claude";
const PERMISSIONS_KEY: &str = "permissions";
const DIRECTORIES_KEY: &str = "additionalDirectories";
const BLOCK_WRITES_KEY: &str = "blockWritesOutsideWorkingDirectories";
const RECORD_FILE: &str = "claude-directories.json";
/// Ends the "Update git to ..." sentence of a git refusal.
const GIT_PURPOSE: &str = "let Claude Code use this project's local settings";

// ── Records ───────────────────────────────────────────────────────────────────

/// What Automatic did with one checkout's entries in one settings file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct OwnedEntries {
    /// Paths Automatic wrote, or found there and shares with another record.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub written: BTreeSet<String>,
    /// Wanted paths the user deleted after Automatic wrote them. They are not
    /// added back until they stop being wanted.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub declined: BTreeSet<String>,
}

impl OwnedEntries {
    fn is_empty(&self) -> bool {
        self.written.is_empty() && self.declined.is_empty()
    }
}

/// Records by checkout folder, for one `local_key`. A linked worktree syncs
/// with its parent's `local_key`, so the folder tells the two apart.
type FolderRecords = BTreeMap<String, OwnedEntries>;

/// Every record, as stored in `~/.automatic/claude-directories.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DirectoryRecords {
    /// Entries in `~/.claude/settings.json`, by `local_key`, then folder.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub user_settings: BTreeMap<String, FolderRecords>,
    /// Entries in each folder's `.claude/settings.local.json`, by
    /// `local_key`, then folder.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub local_settings: BTreeMap<String, FolderRecords>,
    /// Writes the last sync could not make, by `local_key`, then folder, so
    /// the problems check can show them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub last_errors: BTreeMap<String, BTreeMap<String, Vec<String>>>,
}

fn set_last_errors(records: &mut DirectoryRecords, key: &str, folder: &str, errors: &[String]) {
    if errors.is_empty() {
        if let Some(folders) = records.last_errors.get_mut(key) {
            folders.remove(folder);
            if folders.is_empty() {
                records.last_errors.remove(key);
            }
        }
    } else {
        records
            .last_errors
            .entry(key.to_string())
            .or_default()
            .insert(folder.to_string(), errors.to_vec());
    }
}

fn record_of(map: &BTreeMap<String, FolderRecords>, key: &str, folder: &str) -> OwnedEntries {
    map.get(key)
        .and_then(|folders| folders.get(folder))
        .cloned()
        .unwrap_or_default()
}

fn set_record(
    map: &mut BTreeMap<String, FolderRecords>,
    key: &str,
    folder: &str,
    entries: OwnedEntries,
) {
    if entries.is_empty() {
        if let Some(folders) = map.get_mut(key) {
            folders.remove(folder);
            if folders.is_empty() {
                map.remove(key);
            }
        }
    } else {
        map.entry(key.to_string())
            .or_default()
            .insert(folder.to_string(), entries);
    }
}

/// Paths in user settings that a record other than (`key`, `folder`) holds.
fn shared_user_paths(records: &DirectoryRecords, key: &str, folder: &str) -> BTreeSet<String> {
    records
        .user_settings
        .iter()
        .flat_map(|(k, folders)| {
            folders
                .iter()
                .filter(move |(f, _)| !(k == key && f.as_str() == folder))
                .flat_map(|(_, entries)| entries.written.iter().cloned())
        })
        .collect()
}

fn records_path() -> Result<PathBuf, String> {
    Ok(get_automatic_dir()?.join(RECORD_FILE))
}

/// Read the records, and the text they were read from (`None`: no file). A
/// file that cannot be parsed is an error: without the record, Automatic
/// cannot tell its entries from the user's.
fn read_records_seen() -> Result<(DirectoryRecords, Option<String>), String> {
    let path = records_path()?;
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok((DirectoryRecords::default(), None))
        }
        Err(e) => return Err(format!("Could not read {}: {}", path.display(), e)),
    };
    let records = serde_json::from_str(&raw).map_err(|e| {
        format!(
            "{} could not be parsed (line {}). Nothing was changed.",
            path.display(),
            e.line()
        )
    })?;
    Ok((records, Some(raw)))
}

fn read_records() -> Result<DirectoryRecords, String> {
    read_records_seen().map(|(records, _)| records)
}

/// How many times [`commit_records`] reads again after another process
/// changed the record file between its read and its write.
const RECORD_WRITE_TRIES: usize = 3;

/// Which records a commit owns.
pub(crate) enum RecordScope<'a> {
    /// These folders of the checkout. A linked worktree shares its main
    /// checkout's `local_key` and syncs on its own, so a sync owns only the
    /// folders it touched.
    Folders(&'a [String]),
    /// Every folder of the checkout, for forgetting it whole.
    Checkout,
}

/// Copy the records `scope` names, for the checkout `key`, from `ours` into
/// `base`. A key left with no folders is dropped.
fn with_checkout_records(
    mut base: DirectoryRecords,
    ours: &DirectoryRecords,
    key: &str,
    scope: &RecordScope,
) -> DirectoryRecords {
    fn copy<T: Clone>(
        to: &mut BTreeMap<String, BTreeMap<String, T>>,
        from: &BTreeMap<String, BTreeMap<String, T>>,
        key: &str,
        scope: &RecordScope,
    ) {
        let RecordScope::Folders(folders) = scope else {
            match from.get(key) {
                Some(value) => to.insert(key.to_string(), value.clone()),
                None => to.remove(key),
            };
            return;
        };
        let target = to.entry(key.to_string()).or_default();
        for folder in *folders {
            match from.get(key).and_then(|f| f.get(folder)) {
                Some(value) => {
                    target.insert(folder.clone(), value.clone());
                }
                None => {
                    target.remove(folder);
                }
            }
        }
        if target.is_empty() {
            to.remove(key);
        }
    }
    copy(&mut base.user_settings, &ours.user_settings, key, scope);
    copy(&mut base.local_settings, &ours.local_settings, key, scope);
    copy(&mut base.last_errors, &ours.last_errors, key, scope);
    base
}

/// Write the records `scope` names, for the checkout `key`, from `ours`.
/// `base` and `seen` are the records and text this process read. When another
/// process changed the file since, it is read again and only those records
/// are put on top of the new version, so that process's records survive.
fn commit_records(
    key: &str,
    scope: &RecordScope,
    ours: &DirectoryRecords,
    mut base: DirectoryRecords,
    mut seen: Option<String>,
) -> Result<(), String> {
    let path = records_path()?;
    for _ in 0..RECORD_WRITE_TRIES {
        let next = with_checkout_records(base.clone(), ours, key, scope);
        if seen.is_some() && next == base {
            return Ok(());
        }
        let text = serde_json::to_string_pretty(&next)
            .map_err(|e| format!("Could not serialise {}: {}", path.display(), e))?;
        if replace_text_unless_changed(&path, &text, seen.as_deref())? {
            return Ok(());
        }
        (base, seen) = read_records_seen()?;
    }
    Err(format!(
        "{} kept changing while Automatic was writing it",
        path.display()
    ))
}

// ── Merge (pure) ──────────────────────────────────────────────────────────────

/// The result of [`merge_directories`].
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DirsMerge {
    pub root: Map<String, Value>,
    pub entries: OwnedEntries,
    /// Whether `root` differs from the input.
    pub changed: bool,
}

/// The `additionalDirectories` list in `root`, or `None` when there is none. A
/// `permissions` that is not an object, or a list that is not an array, is an
/// error, so the file is left as it is.
fn directory_list(root: &Map<String, Value>, path: &Path) -> Result<Option<Vec<Value>>, String> {
    let not_shaped = |what: &str| {
        format!(
            "{} in {} is not {}. Fix the file, then sync again. Nothing was changed.",
            what,
            path.display(),
            if what == PERMISSIONS_KEY {
                "a JSON object"
            } else {
                "a JSON array"
            }
        )
    };
    match root.get(PERMISSIONS_KEY) {
        None => Ok(None),
        Some(Value::Object(permissions)) => match permissions.get(DIRECTORIES_KEY) {
            None => Ok(None),
            Some(Value::Array(list)) => Ok(Some(list.clone())),
            Some(_) => Err(not_shaped(&format!("{PERMISSIONS_KEY}.{DIRECTORIES_KEY}"))),
        },
        Some(_) => Err(not_shaped(PERMISSIONS_KEY)),
    }
}

fn string_entries(list: &[Value]) -> BTreeSet<String> {
    list.iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

/// Bring the `additionalDirectories` entries one record owns in `root` in line
/// with `desired`.
///
/// - `before` is the record from the last write. A path it lists that the
///   list no longer holds was deleted by the user. While it stays wanted it is
///   declined, and not added back. When the list itself is gone (the file was
///   deleted, or a fresh clone sits at the same path) nothing was declined:
///   the merge starts as if there were no record.
/// - `shared` holds the paths other records own in the same file. They are
///   never removed here, and a wanted path one of them already wrote is
///   shared rather than taken for the user's.
/// - A wanted path the file already holds, that no record owns, is the
///   user's. It is left alone and never recorded, so it is never removed.
///
/// Only `permissions.additionalDirectories` changes. A list this empties is
/// removed, and so is a `permissions` object this empties.
pub(crate) fn merge_directories(
    mut root: Map<String, Value>,
    before: &OwnedEntries,
    shared: &BTreeSet<String>,
    desired: &BTreeSet<String>,
    path: &Path,
) -> Result<DirsMerge, String> {
    let (current, before) = match directory_list(&root, path)? {
        Some(list) => (list, before.clone()),
        None => (Vec::new(), OwnedEntries::default()),
    };
    let before = &before;
    let present = string_entries(&current);

    let mut declined: BTreeSet<String> = before.declined.intersection(desired).cloned().collect();
    for gone in before.written.iter().filter(|p| !present.contains(*p)) {
        if desired.contains(gone) {
            declined.insert(gone.clone());
        }
    }

    let mut list = current.clone();
    let mut written = BTreeSet::new();
    for path in desired.iter().filter(|p| !declined.contains(*p)) {
        if present.contains(path) {
            if before.written.contains(path) || shared.contains(path) {
                written.insert(path.clone());
            }
        } else {
            list.push(Value::String(path.clone()));
            written.insert(path.clone());
        }
    }
    let released: BTreeSet<&String> = before
        .written
        .iter()
        .filter(|p| !written.contains(*p) && !shared.contains(*p))
        .collect();
    list.retain(|entry| !entry.as_str().is_some_and(|p| released.contains(&p.to_string())));

    let changed = list != current;
    if changed {
        set_directory_list(&mut root, list);
    }
    Ok(DirsMerge {
        root,
        entries: OwnedEntries { written, declined },
        changed,
    })
}

/// Put `list` in `root`, removing the list and an emptied `permissions` when
/// `list` is empty. [`directory_list`] has already checked the shapes.
fn set_directory_list(root: &mut Map<String, Value>, list: Vec<Value>) {
    let permissions = root
        .entry(PERMISSIONS_KEY.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(permissions) = permissions else {
        return;
    };
    if list.is_empty() {
        permissions.remove(DIRECTORIES_KEY);
    } else {
        permissions.insert(DIRECTORIES_KEY.to_string(), Value::Array(list));
    }
    if permissions.is_empty() {
        root.remove(PERMISSIONS_KEY);
    }
}

// ── Target (pure) ─────────────────────────────────────────────────────────────

/// What git says about `.claude/settings.local.json` in the project folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalFileState {
    /// The folder is a repository root and git does not track the file.
    Untracked,
    /// Git tracks the file, so Claude Code ignores its directories.
    Tracked,
    /// The folder is not the root of a git repository. Claude Code reads the
    /// local file from the repository root, and treats it as shared settings
    /// outside a repository.
    NotRepositoryRoot,
    /// Git could not answer.
    Unknown,
}

/// What `~/.claude.json` records about the project folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrustFlag {
    /// Nothing stops Claude Code from reading the local file's directories.
    Clear,
    /// Claude Code once saw the local file tracked and the folder is not
    /// trusted, so it ignores the file's directories even when untracked.
    SeenTracked,
    /// `~/.claude.json` could not be read, so the flag is unknown. Treated
    /// as flagged.
    Unknown,
}

/// Why the entries need user settings rather than the local file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserSettingsReason {
    /// The app setting says to always use user settings.
    Preferred,
    /// Silent mode never writes into the project folder.
    Silent,
    /// The project is read-only on this machine.
    ReadOnly,
    /// See [`LocalFileState::NotRepositoryRoot`].
    NotRepositoryRoot,
    /// See [`LocalFileState::Tracked`].
    Tracked,
    /// See [`TrustFlag::SeenTracked`].
    SeenTracked,
    /// See [`LocalFileState::Unknown`].
    GitUnavailable,
    /// See [`TrustFlag::Unknown`].
    TrustUnknown,
}

/// Where the entries go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "reason", rename_all = "snake_case")]
pub enum DirsTarget {
    /// The project's `.claude/settings.local.json`.
    Local,
    /// `~/.claude/settings.json`.
    User(UserSettingsReason),
    /// User settings are needed, and the user has not agreed to them.
    NeedsConsent(UserSettingsReason),
}

/// The facts [`choose_target`] decides from.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TargetFacts {
    pub silent: bool,
    pub read_only: bool,
    /// `None` when it was not needed, because an earlier fact already rules
    /// the local file out.
    pub local_file: Option<LocalFileState>,
    pub trust: TrustFlag,
    pub preference: WritableFoldersTarget,
    pub user_consent: bool,
}

impl TargetFacts {
    /// Whether the local file's state matters, so git must be asked.
    fn local_file_matters(silent: bool, read_only: bool, preference: WritableFoldersTarget) -> bool {
        !silent && !read_only && preference == WritableFoldersTarget::PreferLocal
    }
}

/// Pick where a project's entries go. The local file is used only for a
/// Normal-mode project that is writable here, whose folder is a repository
/// root where git does not track the file, and that Claude Code has not
/// flagged. A Silent or read-only project never gets the local file.
pub(crate) fn choose_target(facts: &TargetFacts) -> DirsTarget {
    let reason = if facts.preference == WritableFoldersTarget::AlwaysUser {
        UserSettingsReason::Preferred
    } else if facts.silent {
        UserSettingsReason::Silent
    } else if facts.read_only {
        UserSettingsReason::ReadOnly
    } else {
        match facts.local_file {
            Some(LocalFileState::Untracked) => match facts.trust {
                TrustFlag::Clear => return DirsTarget::Local,
                TrustFlag::SeenTracked => UserSettingsReason::SeenTracked,
                TrustFlag::Unknown => UserSettingsReason::TrustUnknown,
            },
            Some(LocalFileState::Tracked) => UserSettingsReason::Tracked,
            Some(LocalFileState::NotRepositoryRoot) => UserSettingsReason::NotRepositoryRoot,
            Some(LocalFileState::Unknown) | None => UserSettingsReason::GitUnavailable,
        }
    };
    if facts.user_consent {
        DirsTarget::User(reason)
    } else {
        DirsTarget::NeedsConsent(reason)
    }
}

// ── Folders (pure) ────────────────────────────────────────────────────────────

/// Where a wanted folder comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FolderSource {
    /// Another member of a group with the switch on.
    GroupMember { group: String, project: String },
    /// The project's own list of extra folders.
    Extra,
    /// The main checkout of the linked worktree being synced.
    MainCheckout,
}

/// A folder the project might want, with where it really is on disk.
#[derive(Debug, Clone)]
pub(crate) struct Candidate {
    pub source: FolderSource,
    /// The path as stored or found.
    pub path: String,
    /// The canonical path, or `None` when the folder does not exist.
    pub resolved: Option<PathBuf>,
    /// The canonical path stored when the folder was chosen. A folder that
    /// now resolves elsewhere (a link was swapped) is refused.
    pub expected: Option<PathBuf>,
}

/// A folder Automatic will not let Claude Code edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RefusedFolder {
    pub path: String,
    pub source: FolderSource,
    /// Why, as the end of a sentence.
    pub reason: String,
}

/// Folders too broad or too sensitive to hand to an agent. See
/// [`refusal_reason`].
#[derive(Debug, Clone, Default)]
pub(crate) struct DeniedPaths {
    /// The home folder, normalised.
    pub home: PathBuf,
    /// Every guarded path, normalised: `~/.claude.json`, and the folders in
    /// `sensitive_folders`. A folder that holds one is refused.
    pub sensitive: Vec<PathBuf>,
    /// The guarded folders: Claude Code's settings folder and Automatic's
    /// data folders. Nothing inside them may be granted either.
    pub sensitive_folders: Vec<PathBuf>,
    /// The same paths by file identity, so another name for one of them is
    /// caught too (a macOS firmlink such as `/System/Volumes/Data/Users`, a
    /// bind mount, a hard-linked folder).
    #[cfg(unix)]
    ids: DeniedIds,
}

/// A file's identity: its device and inode numbers.
#[cfg(unix)]
type FileId = (u64, u64);

#[cfg(unix)]
fn file_id(path: &Path) -> Option<FileId> {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(path).ok().map(|meta| (meta.dev(), meta.ino()))
}

/// [`DeniedPaths`] by identity. Paths that do not exist have none, and are
/// guarded by the string checks alone.
#[cfg(unix)]
#[derive(Debug, Clone, Default)]
struct DeniedIds {
    root: Option<FileId>,
    home: Option<FileId>,
    /// The home folder and every folder above it.
    home_chain: Vec<FileId>,
    /// Each guarded path, and every folder above it, with the guarded path.
    holds: Vec<(FileId, PathBuf)>,
    /// The guarded folders.
    inside: Vec<(FileId, PathBuf)>,
}

impl DeniedPaths {
    /// Guard `home`, the paths in `sensitive` and the folders in
    /// `sensitive_folders` (which `sensitive` need not repeat).
    pub(crate) fn new(home: PathBuf, sensitive: Vec<PathBuf>, sensitive_folders: Vec<PathBuf>) -> Self {
        let mut all = sensitive;
        for folder in &sensitive_folders {
            if !all.contains(folder) {
                all.push(folder.clone());
            }
        }
        #[cfg(unix)]
        let ids = DeniedIds {
            root: file_id(Path::new("/")),
            home: file_id(&home),
            home_chain: home.ancestors().filter_map(file_id).collect(),
            holds: all
                .iter()
                .flat_map(|path| {
                    path.ancestors()
                        .filter_map(file_id)
                        .map(move |id| (id, path.clone()))
                })
                .collect(),
            inside: sensitive_folders
                .iter()
                .filter_map(|folder| file_id(folder).map(|id| (id, folder.clone())))
                .collect(),
        };
        Self {
            home,
            sensitive: all,
            sensitive_folders,
            #[cfg(unix)]
            ids,
        }
    }
}

/// Why `folder` (canonical) must not become a working folder, or `None`. The
/// filesystem root, the home folder and its ancestors are refused. So is any
/// folder that is or holds Claude Code's settings or Automatic's data, and
/// any folder inside Claude Code's settings folder or Automatic's data
/// folders: an agent that can edit those can rewrite its own grants (the
/// registry pointers hold them) or lift Claude Code's limits.
///
/// The folder must also be inside the home folder: an allow-list, so a new
/// alias for a disk or a system folder is refused without being named here.
///
/// Both guard checks run: by file identity on unix, which catches other names
/// for the same folder, and by path, which covers paths that do not exist.
pub(crate) fn refusal_reason(folder: &Path, denied: &DeniedPaths) -> Option<String> {
    if let Some(reason) = outside_home(folder, denied) {
        return Some(reason);
    }
    #[cfg(unix)]
    if let Some(reason) = identity_refusal(folder, denied) {
        return Some(reason);
    }
    path_refusal(folder, denied)
}

/// Why `folder` (canonical) is not strictly inside the home folder, or `None`.
///
/// The path must start with the canonical home path, and on unix one of the
/// folder's own ancestors must also be the home folder by identity. Both are
/// needed: macOS gives one folder several paths (`/System/Volumes/Data/Users`,
/// `/.nofollow/Users`), which pass an identity check but not the path check,
/// and a path check alone would trust a home path that leads elsewhere.
fn outside_home(folder: &Path, denied: &DeniedPaths) -> Option<String> {
    const MUST_BE_INSIDE: &str = "the folder must be inside your home folder";
    #[cfg(windows)]
    let inside = windows_inside(folder, &denied.home);
    #[cfg(not(windows))]
    let inside = folder != denied.home && folder.starts_with(&denied.home);
    if folder == denied.home {
        return Some(format!("it is your home folder, and {MUST_BE_INSIDE}"));
    }
    if !inside {
        return Some(format!("it is outside your home folder, and {MUST_BE_INSIDE}"));
    }
    #[cfg(unix)]
    {
        let Some(home_id) = denied.ids.home else {
            return Some(format!("your home folder cannot be found, and {MUST_BE_INSIDE}"));
        };
        let found = folder
            .ancestors()
            .skip(1)
            .any(|ancestor| file_id(ancestor) == Some(home_id));
        if !found {
            return Some(format!(
                "it does not lead to a folder inside your home folder, and {MUST_BE_INSIDE}"
            ));
        }
    }
    None
}

/// Whether `folder` is strictly inside `home` on Windows. Both are compared
/// as a drive letter and lower-cased components, with any `\\?\` prefix
/// removed. A UNC share, a device path (`\\.\`), `\\?\GLOBALROOT` or any
/// other verbatim form is never inside.
#[cfg(windows)]
fn windows_inside(folder: &Path, home: &Path) -> bool {
    fn key(path: &Path) -> Option<(u8, Vec<String>)> {
        use std::path::{Component, Prefix};
        let mut components = path.components();
        let drive = match components.next()? {
            Component::Prefix(prefix) => match prefix.kind() {
                Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => drive.to_ascii_uppercase(),
                _ => return None,
            },
            _ => return None,
        };
        let mut parts = Vec::new();
        for component in components {
            match component {
                Component::RootDir => {}
                Component::Normal(part) => parts.push(part.to_string_lossy().to_lowercase()),
                _ => return None,
            }
        }
        Some((drive, parts))
    }
    match (key(folder), key(home)) {
        (Some((drive, parts)), Some((home_drive, home_parts))) => {
            drive == home_drive && parts.len() > home_parts.len() && parts.starts_with(&home_parts)
        }
        _ => false,
    }
}

/// [`refusal_reason`] by file identity.
#[cfg(unix)]
fn identity_refusal(folder: &Path, denied: &DeniedPaths) -> Option<String> {
    let ids = &denied.ids;
    for ancestor in folder.ancestors() {
        let Some(id) = file_id(ancestor) else {
            continue;
        };
        if let Some((_, inside)) = ids.inside.iter().find(|(guarded, _)| *guarded == id) {
            return Some(format!("it is inside {}", inside.display()));
        }
    }
    let id = file_id(folder)?;
    if ids.root == Some(id) {
        return Some("it is the root of the file system".to_string());
    }
    if ids.home == Some(id) {
        return Some("it is your home folder".to_string());
    }
    if ids.home_chain.contains(&id) {
        return Some("it holds your home folder".to_string());
    }
    ids.holds
        .iter()
        .find(|(guarded, _)| *guarded == id)
        .map(|(_, path)| format!("it holds {}", path.display()))
}

/// [`refusal_reason`] by comparing paths.
fn path_refusal(folder: &Path, denied: &DeniedPaths) -> Option<String> {
    if let Some(inside) = denied
        .sensitive_folders
        .iter()
        .find(|sensitive| folder.starts_with(sensitive))
    {
        return Some(format!("it is inside {}", inside.display()));
    }
    if folder.parent().is_none() {
        return Some("it is the root of the file system".to_string());
    }
    if folder == denied.home {
        return Some("it is your home folder".to_string());
    }
    if denied.home.starts_with(folder) {
        return Some("it holds your home folder".to_string());
    }
    denied
        .sensitive
        .iter()
        .find(|sensitive| sensitive.starts_with(folder))
        .map(|sensitive| format!("it holds {}", sensitive.display()))
}

/// A wanted folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WantedFolder {
    pub path: String,
    pub source: FolderSource,
}

/// A folder that is not on this machine, so it is left out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MissingFolder {
    pub path: String,
    pub source: FolderSource,
}

/// The result of [`resolve_folders`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ResolvedFolders {
    /// Canonical paths, first source kept, in path order.
    pub wanted: Vec<WantedFolder>,
    pub missing: Vec<MissingFolder>,
    pub refused: Vec<RefusedFolder>,
}

impl ResolvedFolders {
    fn paths(&self) -> BTreeSet<String> {
        self.wanted.iter().map(|w| w.path.clone()).collect()
    }
}

/// The folders a project at the canonical folder `own` wants. A folder that
/// does not exist is reported as missing. A folder [`refusal_reason`] refuses,
/// or one that no longer resolves to the path stored for it, is reported as
/// refused, whatever its source. The project's own folder, and any folder
/// inside it, is already a working folder and is left out. A path that is not
/// UTF-8 cannot be written to JSON and is left out too.
pub(crate) fn resolve_folders(
    own: &Path,
    candidates: &[Candidate],
    denied: &DeniedPaths,
) -> ResolvedFolders {
    let mut by_path: BTreeMap<String, FolderSource> = BTreeMap::new();
    let mut missing = Vec::new();
    let mut refused = Vec::new();
    for candidate in candidates {
        let Some(resolved) = &candidate.resolved else {
            missing.push(MissingFolder {
                path: candidate.path.clone(),
                source: candidate.source.clone(),
            });
            continue;
        };
        let reason = match &candidate.expected {
            Some(expected) if expected != resolved => Some(format!(
                "it now leads to {}. Choose the folder again if that is right",
                resolved.display()
            )),
            _ => refusal_reason(resolved, denied),
        };
        if let Some(reason) = reason {
            refused.push(RefusedFolder {
                path: candidate.path.clone(),
                source: candidate.source.clone(),
                reason,
            });
            continue;
        }
        if resolved.starts_with(own) {
            continue;
        }
        let Some(path) = resolved.to_str() else {
            eprintln!(
                "[automatic] writable folders: skipping {}, whose path is not UTF-8",
                resolved.display()
            );
            continue;
        };
        by_path
            .entry(path.to_string())
            .or_insert_with(|| candidate.source.clone());
    }
    ResolvedFolders {
        wanted: by_path
            .into_iter()
            .map(|(path, source)| WantedFolder { path, source })
            .collect(),
        missing,
        refused,
    }
}

// ── Findings (pure) ───────────────────────────────────────────────────────────

/// The `additionalDirectories` entries and the block setting of one settings
/// file Claude Code loads.
#[derive(Debug, Clone, Default)]
pub(crate) struct SettingsFileFacts {
    /// How the file is shown to the user.
    pub shown: String,
    pub directories: BTreeSet<String>,
    pub blocks_writes: bool,
    /// Why the file could not be read, when it could not.
    pub unreadable: Option<String>,
}

/// Everything [`assess`] reports from.
#[derive(Debug, Clone)]
pub(crate) struct AssessFacts {
    pub folders: ResolvedFolders,
    pub target: DirsTarget,
    /// User, managed, project and local settings, in that order.
    pub user: SettingsFileFacts,
    pub managed: SettingsFileFacts,
    pub project: SettingsFileFacts,
    pub local: SettingsFileFacts,
    /// Whether Claude Code honours the local file's directories.
    pub local_counts: bool,
    pub notice_hidden: bool,
    /// Joined groups whose switch is off.
    pub groups_switched_off: Vec<String>,
    pub last_sync_errors: Vec<String>,
    /// Why `~/.claude.json` could not be read, when it could not.
    pub trust_unreadable: Option<String>,
    /// Entries Automatic wrote to the local file before git began tracking
    /// it. Automatic leaves a tracked file alone, so they stay there.
    pub left_in_tracked_local: Vec<String>,
}

/// What the problems check reports about a project's writable folders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WritableFoldersFindings {
    /// Canonical folders the project wants.
    pub wanted: Vec<WantedFolder>,
    /// Wanted folders no settings file Claude Code honours lists.
    pub uncovered: Vec<String>,
    pub missing: Vec<MissingFolder>,
    pub target: DirsTarget,
    /// The file that switches the block on, when one does.
    pub blocked_by: Option<String>,
    /// Whether to ask the user to allow user settings (the
    /// `writable_folders_target` notice).
    pub ask_for_user_settings: bool,
    /// The JSON to add to `~/.claude/settings.json` by hand for the uncovered
    /// folders.
    pub paste_json: String,
    /// Settings files that could not be read.
    pub unreadable: Vec<String>,
    pub groups_switched_off: Vec<String>,
    /// Writes the last sync could not make.
    pub last_sync_errors: Vec<String>,
    /// Folders refused because they are too broad, too sensitive, or changed.
    pub refused: Vec<RefusedFolder>,
    /// See [`AssessFacts::left_in_tracked_local`].
    pub left_in_tracked_local: Vec<String>,
}

/// The settings fragment that adds `paths` as working folders.
pub fn paste_json(paths: &[String]) -> String {
    serde_json::json!({ PERMISSIONS_KEY: { DIRECTORIES_KEY: paths } }).to_string()
}

/// Work out the findings from gathered facts.
pub(crate) fn assess(facts: &AssessFacts) -> WritableFoldersFindings {
    let files = [&facts.user, &facts.managed, &facts.project, &facts.local];
    let blocked_by = files
        .iter()
        .find(|file| file.blocks_writes)
        .map(|file| file.shown.clone());
    let uncovered: Vec<String> = facts
        .folders
        .wanted
        .iter()
        .map(|w| w.path.clone())
        .filter(|path| {
            !facts.user.directories.contains(path)
                && !facts.managed.directories.contains(path)
                && !(facts.local_counts && facts.local.directories.contains(path))
        })
        .collect();
    let ask_for_user_settings = matches!(facts.target, DirsTarget::NeedsConsent(_))
        && !facts.notice_hidden
        && !facts.folders.wanted.is_empty();
    let mut unreadable: Vec<String> = files
        .iter()
        .filter_map(|file| {
            file.unreadable
                .as_ref()
                .map(|why| format!("{}: {}", file.shown, why))
        })
        .collect();
    unreadable.extend(facts.trust_unreadable.iter().map(|why| format!("~/.claude.json: {why}")));
    WritableFoldersFindings {
        wanted: facts.folders.wanted.clone(),
        paste_json: paste_json(&uncovered),
        uncovered,
        missing: facts.folders.missing.clone(),
        target: facts.target,
        blocked_by,
        ask_for_user_settings,
        unreadable,
        groups_switched_off: facts.groups_switched_off.clone(),
        last_sync_errors: facts.last_sync_errors.clone(),
        refused: facts.folders.refused.clone(),
        left_in_tracked_local: facts.left_in_tracked_local.clone(),
    }
}

// ── Gathering facts ───────────────────────────────────────────────────────────

/// Managed settings, which only an administrator can change.
fn managed_settings_path() -> Result<PathBuf, String> {
    #[cfg(test)]
    return Ok(super::paths::home_dir()?.join("managed-claude").join("managed-settings.json"));
    #[cfg(all(not(test), target_os = "macos"))]
    return Ok(PathBuf::from(
        "/Library/Application Support/ClaudeCode/managed-settings.json",
    ));
    #[cfg(all(not(test), target_os = "windows"))]
    return Ok(PathBuf::from(r"C:\Program Files\ClaudeCode\managed-settings.json"));
    #[cfg(all(not(test), not(target_os = "macos"), not(target_os = "windows")))]
    return Ok(PathBuf::from("/etc/claude-code/managed-settings.json"));
}

/// `~/.claude.json`, which Claude Code writes. Automatic only reads it.
fn claude_state_path() -> Result<PathBuf, String> {
    super::paths::claude_state_file()
}

fn local_settings_path(dir: &Path) -> PathBuf {
    dir.join(".claude").join("settings.local.json")
}

/// Read a settings file Claude Code loads. `contained_in` is the project
/// folder for a file inside it, which is read without following a link out.
fn settings_file_facts(path: &Path, shown: &str, contained_in: Option<&Path>) -> SettingsFileFacts {
    let mut facts = SettingsFileFacts {
        shown: shown.to_string(),
        ..Default::default()
    };
    let text = match contained_in {
        Some(root) => crate::sync::read_project_text(root, path),
        None => match fs::read_to_string(path) {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("could not be read: {e}")),
        },
    };
    let root = match text {
        Ok(Some(text)) => parse_root(&text, path),
        Ok(None) => return facts,
        Err(e) => Err(e),
    };
    match root {
        Ok(root) => {
            facts.blocks_writes = root
                .get(PERMISSIONS_KEY)
                .and_then(|p| p.get(BLOCK_WRITES_KEY))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            match directory_list(&root, path) {
                Ok(list) => facts.directories = string_entries(&list.unwrap_or_default()),
                Err(e) => facts.unreadable = Some(e),
            }
        }
        Err(e) => facts.unreadable = Some(e),
    }
    facts
}

/// Ask git, through `git_guard`, whether it tracks the local settings file in
/// the folder `dir`. The folder must be the repository root, because that is
/// where Claude Code reads the file from.
fn local_file_state(dir: &Path) -> LocalFileState {
    if fs::symlink_metadata(dir.join(".git")).is_err() {
        return LocalFileState::NotRepositoryRoot;
    }
    let Some(install) = git_guard::git_install() else {
        return LocalFileState::Unknown;
    };
    let tracked = git_guard::read_checked_repo_config(install, dir, GIT_PURPOSE).and_then(|config| {
        git_guard::isolated_git_command(install, dir, &config)?
            .args(["ls-files", "--error-unmatch", "--", ".claude/settings.local.json"])
            .output()
            .map_err(|e| format!("Cannot run git in {}: {}", dir.display(), e))
    });
    match tracked {
        // `--error-unmatch` exits 1 for a path git does not track. Other
        // failures exit 128.
        Ok(output) if output.status.success() => LocalFileState::Tracked,
        Ok(output) if output.status.code() == Some(1) => LocalFileState::Untracked,
        Ok(output) => {
            eprintln!(
                "[automatic] writable folders: git ls-files failed in {}: {}",
                dir.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            );
            LocalFileState::Unknown
        }
        Err(e) => {
            eprintln!("[automatic] writable folders: {e}");
            LocalFileState::Unknown
        }
    }
}

/// Whether `~/.claude.json` flags any of `folders` as having had a tracked
/// local file, without the trust that lifts it. A file that cannot be read
/// or parsed gives [`TrustFlag::Unknown`] and the reason. A missing file
/// flags nothing.
fn trust_flag(folders: &[String]) -> (TrustFlag, Option<String>) {
    let path = match claude_state_path() {
        Ok(path) => path,
        Err(e) => return (TrustFlag::Unknown, Some(e)),
    };
    let state: Value = match fs::read_to_string(&path) {
        Ok(raw) => match serde_json::from_str(&raw) {
            Ok(state) => state,
            Err(e) => {
                return (
                    TrustFlag::Unknown,
                    Some(format!("could not be parsed (line {})", e.line())),
                )
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (TrustFlag::Clear, None),
        Err(e) => return (TrustFlag::Unknown, Some(format!("could not be read: {e}"))),
    };
    let flagged = folders.iter().any(|folder| {
        let Some(entry) = state.get("projects").and_then(|p| p.get(folder)) else {
            return false;
        };
        let seen = entry.get("localSettingsSeenGitTracked").and_then(Value::as_bool) == Some(true);
        let trusted = entry.get("hasTrustDialogAccepted").and_then(Value::as_bool) == Some(true);
        seen && !trusted
    });
    if flagged {
        (TrustFlag::SeenTracked, None)
    } else {
        (TrustFlag::Clear, None)
    }
}

/// The paths `~/.claude.json` may key the folder under, plus the main
/// checkout for a linked worktree, which probably shares its flag.
fn trust_keys(project: &Project, own: &Path, main: Option<&Path>) -> Vec<String> {
    let mut keys = vec![project.directory.clone()];
    keys.extend(own.to_str().map(str::to_string));
    keys.extend(main.and_then(Path::to_str).map(str::to_string));
    keys.dedup();
    keys
}

/// The main checkout when `own` is a linked worktree.
fn main_checkout(own: &Path) -> Option<PathBuf> {
    let common = git_common_dir(own)?;
    if !is_linked_worktree(own, Some(&common)) {
        return None;
    }
    crate::sync::main_checkout_of(own).ok()
}

/// The joined groups of `project`, split into those with the switch on and
/// the names of those with it off. An unreadable group is an error: a group
/// left out would release its folders.
fn joined_groups_by_switch(project: &Project) -> Result<(Vec<ProjectGroup>, Vec<String>), String> {
    let (on, off): (Vec<ProjectGroup>, Vec<ProjectGroup>) = joined_groups(project)?
        .into_iter()
        .partition(|group| group.agents_edit_across_group);
    Ok((on, off.into_iter().map(|g| g.name).collect()))
}

/// Every folder `project` might want.
fn gather_candidates(
    project: &Project,
    groups_on: &[ProjectGroup],
    main: Option<&Path>,
) -> Result<Vec<Candidate>, String> {
    let resolve = |path: &str| fs::canonicalize(path).ok();
    let mut candidates = Vec::new();
    if !groups_on.is_empty() {
        let entries = scan_registry()?;
        for group in groups_on {
            for entry in &entries {
                if entry.local_key() == Some(project.local_key.as_str())
                    || entry.awaiting_confirmation()
                    || !is_group_member(entry, group)
                    || !entry_joined_group(entry, &group.name)
                {
                    continue;
                }
                let Some(directory) = entry.directory().ok().flatten() else {
                    continue;
                };
                candidates.push(Candidate {
                    source: FolderSource::GroupMember {
                        group: group.name.clone(),
                        project: entry.name.clone(),
                    },
                    resolved: resolve(&directory),
                    expected: None,
                    path: directory,
                });
            }
        }
    }
    for folder in &project.local_writable_folders {
        candidates.push(Candidate {
            source: FolderSource::Extra,
            resolved: resolve(folder),
            // Stored canonical by `set_project_writable_folders`.
            expected: Some(PathBuf::from(folder)),
            path: folder.clone(),
        });
    }
    if let Some(main) = main {
        candidates.push(Candidate {
            source: FolderSource::MainCheckout,
            resolved: Some(main.to_path_buf()),
            expected: None,
            path: main.display().to_string(),
        });
    }
    Ok(candidates)
}

/// The folders [`refusal_reason`] guards, on this machine: the home folder,
/// `~/.claude.json`, `~/.claude`, the folder `CLAUDE_CONFIG_DIR` names when
/// set, and the data folders of both the release and the development build,
/// whichever one is running.
fn denied_paths() -> Result<DeniedPaths, String> {
    let home = super::paths::home_dir()?;
    let normal = |path: &Path| normalised(&path.to_string_lossy());
    // `get_automatic_dir()` is one of the two data folders, whichever build
    // runs, so it is not listed again.
    let mut folders = vec![
        normal(&home.join(".claude")),
        normal(&home.join(".automatic")),
        normal(&home.join(".automatic-dev")),
    ];
    if let Some(config) = super::paths::claude_config_dir_override() {
        let config = normal(&config);
        if !folders.contains(&config) {
            folders.push(config);
        }
    }
    let mut files = vec![normal(&home.join(".claude.json"))];
    let state = normal(&super::paths::claude_state_file()?);
    if !files.contains(&state) {
        files.push(state);
    }
    Ok(DeniedPaths::new(normal(&home), files, folders))
}

/// The folders whose records belong to a checkout that is at `own` now: its
/// main checkout (`own`, or the main checkout of the worktree `own`) and that
/// checkout's live linked worktrees. A record for any other folder is left
/// from a folder that moved or a worktree that was removed.
fn live_folders(own: &Path) -> BTreeSet<String> {
    let base = main_checkout(own).unwrap_or_else(|| own.to_path_buf());
    std::iter::once(base.clone())
        .chain(crate::sync::linked_worktrees(&base))
        .chain(std::iter::once(own.to_path_buf()))
        .filter_map(|path| path.to_str().map(str::to_string))
        .collect()
}

/// Whether the group stores the entry's project as a member, by its `id` or,
/// for an entry without one, its `local_key`.
fn is_group_member(entry: &RegistryEntry, group: &ProjectGroup) -> bool {
    group.projects.iter().any(|stored| {
        entry.cached_id() == Some(stored.as_str()) || entry.local_key() == Some(stored.as_str())
    })
}

/// Whether the project folder may be written: Normal mode and not read-only.
/// A failed read-only check counts as read-only.
fn folder_writable(project: &Project) -> bool {
    project.mode == ProjectMode::Normal
        && !project.is_read_only_here()
        && matches!(project_is_read_only(project), Ok(false))
}

/// Everything the sync step and the problems check share.
struct Gathered {
    own: PathBuf,
    folder_key: String,
    folders: ResolvedFolders,
    target: DirsTarget,
    local_state: Option<LocalFileState>,
    trust: TrustFlag,
    trust_unreadable: Option<String>,
    groups_off: Vec<String>,
}

fn gather(project: &Project) -> Result<Gathered, String> {
    let own = fs::canonicalize(&project.directory)
        .map_err(|e| format!("Cannot resolve {}: {}", project.directory, e))?;
    let folder_key = own
        .to_str()
        .ok_or_else(|| format!("{} is not a UTF-8 path", own.display()))?
        .to_string();
    let main = main_checkout(&own);
    let (groups_on, groups_off) = joined_groups_by_switch(project)?;
    let candidates = gather_candidates(project, &groups_on, main.as_deref())?;
    let folders = resolve_folders(&own, &candidates, &denied_paths()?);

    let settings = read_settings()?;
    let silent = project.mode == ProjectMode::Silent;
    let read_only = !folder_writable(project) && !silent;
    // Git runs only when something is wanted. Then the state is read whatever
    // the target, so the problems check can tell whether entries the user put
    // in the local file count.
    let local_state = (!folders.wanted.is_empty()).then(|| local_file_state(&own));
    let (trust, trust_unreadable) = trust_flag(&trust_keys(project, &own, main.as_deref()));
    let facts = TargetFacts {
        silent,
        read_only,
        local_file: if TargetFacts::local_file_matters(silent, read_only, settings.writable_folders_target) {
            local_state
        } else {
            None
        },
        trust,
        preference: settings.writable_folders_target,
        user_consent: settings.writable_folders_user_consent,
    };
    Ok(Gathered {
        own,
        folder_key,
        folders,
        target: choose_target(&facts),
        local_state,
        trust,
        trust_unreadable,
        groups_off,
    })
}

fn uses_claude(project: &Project) -> bool {
    project.agents.iter().any(|agent| agent == CLAUDE_AGENT)
}

// ── Problems check ────────────────────────────────────────────────────────────

/// The writable-folder findings for `project`, or `None` when it does not use
/// Claude Code or has no folder. Reads only.
pub fn writable_folders_findings(project: &Project) -> Result<Option<WritableFoldersFindings>, String> {
    if !uses_claude(project)
        || project.directory.is_empty()
        || project.local_key.is_empty()
        || project.local_awaiting_confirmation
    {
        return Ok(None);
    }
    if !Path::new(&project.directory).is_dir() {
        return Ok(None);
    }
    let gathered = gather(project)?;
    let own = &gathered.own;
    let (last_sync_errors, local_record) = match read_records() {
        Ok(records) => (
            records
                .last_errors
                .get(&project.local_key)
                .and_then(|folders| folders.get(&gathered.folder_key))
                .cloned()
                .unwrap_or_default(),
            record_of(&records.local_settings, &project.local_key, &gathered.folder_key),
        ),
        Err(e) => (vec![e], OwnedEntries::default()),
    };
    let local_tracked = !local_record.written.is_empty()
        && gathered
            .local_state
            .unwrap_or_else(|| local_file_state(own))
            == LocalFileState::Tracked;
    let facts = AssessFacts {
        last_sync_errors,
        trust_unreadable: gathered.trust_unreadable.clone(),
        left_in_tracked_local: if local_tracked {
            local_record.written.into_iter().collect()
        } else {
            Vec::new()
        },
        user: settings_file_facts(&user_settings_path()?, "~/.claude/settings.json", None),
        managed: {
            let path = managed_settings_path()?;
            let shown = path.display().to_string();
            settings_file_facts(&path, &shown, None)
        },
        project: settings_file_facts(
            &own.join(".claude").join("settings.json"),
            ".claude/settings.json",
            Some(own),
        ),
        local: settings_file_facts(
            &local_settings_path(own),
            ".claude/settings.local.json",
            Some(own),
        ),
        local_counts: gathered.local_state == Some(LocalFileState::Untracked)
            && gathered.trust == TrustFlag::Clear,
        folders: gathered.folders,
        target: gathered.target,
        notice_hidden: project.local_writable_folders_notice_hidden,
        groups_switched_off: gathered.groups_off,
    };
    Ok(Some(assess(&facts)))
}

// ── Sync ──────────────────────────────────────────────────────────────────────

/// What one sync of a project's writable folders did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct WritableFoldersSync {
    /// Where the entries went, or `None` when the step did nothing.
    pub target: Option<DirsTarget>,
    /// Canonical folders the project wants.
    pub wanted: Vec<String>,
    /// Writes that failed. The rest of the step still ran.
    pub errors: Vec<String>,
}

/// Add or remove one record's entries in the settings file at `path`, and
/// return the new record. `contained_in` is the project folder for its local
/// file: the write is refused if the path leads out of it.
fn apply_to_file(
    path: &Path,
    contained_in: Option<&Path>,
    before: &OwnedEntries,
    shared: &BTreeSet<String>,
    desired: &BTreeSet<String>,
) -> Result<OwnedEntries, String> {
    if let Some(root) = contained_in {
        if let Some(reason) = crate::sync::project_write_refusal(root, path) {
            return Err(format!("Refused to write {}: {}", path.display(), reason));
        }
    }
    let target = followed(path)?;
    let loaded = load(&target)?;
    if loaded.text.is_none() && desired.is_empty() {
        return Ok(OwnedEntries::default());
    }
    let merge = merge_directories(loaded.root, before, shared, desired, &target)?;
    if merge.changed {
        write_root(&target, &merge.root, loaded.text.as_deref(), Permissions::Keep)?;
    }
    Ok(merge.entries)
}

/// Bring the project's entries in Claude Code's settings in line with the
/// folders it wants. Runs for a project that lists Claude Code, and for one
/// that stopped listing it while records remain, so its entries are removed.
///
/// A Silent or read-only project never gets a write into its folder, and a
/// local file git tracks is left as it is. A project an agent registered,
/// which the user has not kept, is left alone. A failed write is reported in
/// `errors` and the other target still runs. An error is returned only when
/// nothing could be decided, and then nothing was changed.
pub fn sync_writable_folders(project: &Project) -> Result<WritableFoldersSync, String> {
    if project.local_key.is_empty()
        || project.directory.is_empty()
        || project.local_awaiting_confirmation
    {
        return Ok(WritableFoldersSync::default());
    }
    let _writes = hold_settings_writes();
    let (base, seen) = read_records_seen()?;
    let mut records = base.clone();
    let key = project.local_key.as_str();
    let gathered = if uses_claude(project) {
        Some(gather(project)?)
    } else {
        None
    };
    let folder_key = match &gathered {
        Some(g) => g.folder_key.clone(),
        None => fs::canonicalize(&project.directory)
            .ok()
            .and_then(|p| p.to_str().map(str::to_string))
            .unwrap_or_else(|| project.directory.clone()),
    };
    if gathered.is_none() && !records.user_settings.contains_key(key) && !records.local_settings.contains_key(key) {
        return Ok(WritableFoldersSync::default());
    }

    let wanted = gathered.as_ref().map(|g| g.folders.paths()).unwrap_or_default();
    let target = gathered.as_ref().map(|g| g.target);
    let empty = BTreeSet::new();
    let (local_desired, user_desired) = match target {
        Some(DirsTarget::Local) => (&wanted, &empty),
        Some(DirsTarget::User(_)) => (&empty, &wanted),
        Some(DirsTarget::NeedsConsent(_)) | None => (&empty, &empty),
    };
    let mut outcome = WritableFoldersSync {
        target,
        wanted: wanted.iter().cloned().collect(),
        errors: Vec::new(),
    };

    let own = PathBuf::from(&folder_key);
    let live = live_folders(&own);
    let moved_from = adopt_moved_local_record(&mut records, key, &own, &live);
    let known_state = gathered.as_ref().and_then(|g| g.local_state);
    if let Err(e) = apply_local(project, &own, &mut records, local_desired, known_state) {
        outcome.errors.push(e);
    }

    let user_path = user_settings_path()?;
    let mut user_jobs: Vec<(String, BTreeSet<String>)> = vec![(folder_key.clone(), user_desired.clone())];
    // A folder the checkout left (it moved, or a worktree was removed) keeps
    // its record until its user-settings entries are removed.
    let stale: Vec<String> = records
        .user_settings
        .get(key)
        .into_iter()
        .chain(records.local_settings.get(key))
        .flat_map(|folders| folders.keys().cloned())
        .filter(|folder| !live.contains(folder))
        .collect();
    user_jobs.extend(stale.iter().map(|folder| (folder.clone(), BTreeSet::new())));
    for (folder, desired) in user_jobs {
        let before = record_of(&records.user_settings, key, &folder);
        if desired.is_empty() && before.written.is_empty() {
            set_record(&mut records.user_settings, key, &folder, OwnedEntries::default());
            continue;
        }
        let shared = shared_user_paths(&records, key, &folder);
        match apply_to_file(&user_path, None, &before, &shared, &desired) {
            Ok(entries) => set_record(&mut records.user_settings, key, &folder, entries),
            Err(e) => outcome.errors.push(e),
        }
    }
    for folder in &stale {
        set_record(&mut records.local_settings, key, folder, OwnedEntries::default());
        set_last_errors(&mut records, key, folder, &[]);
    }

    set_last_errors(&mut records, key, &folder_key, &outcome.errors);
    let touched: Vec<String> = std::iter::once(folder_key.clone())
        .chain(stale)
        .chain(moved_from)
        .collect();
    if let Err(e) = commit_records(key, &RecordScope::Folders(&touched), &records, base, seen) {
        outcome.errors.push(format!(
            "Automatic changed Claude Code's settings but could not record it: {e}"
        ));
    }
    Ok(outcome)
}

/// When the checkout's folder was renamed, its local-file record is still
/// keyed by the old folder, and the entries it lists moved with the file.
/// Move that record to `own`, so the entries stay Automatic's rather than
/// look like the user's. Only a record whose old folder is gone, is not a
/// live folder of the checkout, and is not the main checkout of `own` (a
/// worktree), is moved, and only while `own` has no record. It claims only
/// the entries it recorded that `own`'s local file still holds. Returns the
/// old folder, whose record the commit must now remove.
fn adopt_moved_local_record(
    records: &mut DirectoryRecords,
    key: &str,
    own: &Path,
    live: &BTreeSet<String>,
) -> Option<String> {
    let folder_key = own.to_str()?;
    let folders = records.local_settings.get(key)?;
    if folders.contains_key(folder_key) {
        return None;
    }
    let present = settings_file_facts(&local_settings_path(own), "", Some(own)).directories;
    let main = main_checkout(own);
    let (old, claimed) = folders.iter().find_map(|(folder, entries)| {
        if live.contains(folder)
            || Path::new(folder).exists()
            || main.as_deref() == Some(Path::new(folder))
        {
            return None;
        }
        // Only what Automatic recorded, and only what the moved file holds.
        // An entry the file holds that the record does not list stays the
        // user's.
        let claimed: BTreeSet<String> = entries.written.intersection(&present).cloned().collect();
        (!claimed.is_empty()).then(|| (folder.clone(), claimed))
    })?;
    set_record(&mut records.local_settings, key, &old, OwnedEntries::default());
    set_record(
        &mut records.local_settings,
        key,
        folder_key,
        OwnedEntries {
            written: claimed,
            declined: BTreeSet::new(),
        },
    );
    Some(old)
}

/// Bring the checkout's entries in its own local settings file at `own` in
/// line with `desired`, and update `records`. Writes nothing when the folder
/// must not be written (Silent, read-only) or git tracks the file: a tracked
/// file belongs to the repository, so Automatic leaves its entries there and
/// keeps the record, and the problems check names them. `known` is the
/// file's state when it was already read.
fn apply_local(
    project: &Project,
    own: &Path,
    records: &mut DirectoryRecords,
    desired: &BTreeSet<String>,
    known: Option<LocalFileState>,
) -> Result<(), String> {
    let key = project.local_key.as_str();
    let folder_key = own.to_str().unwrap_or_default();
    let before = record_of(&records.local_settings, key, folder_key);
    if !folder_writable(project) || (desired.is_empty() && before.written.is_empty()) {
        return Ok(());
    }
    if known.unwrap_or_else(|| local_file_state(own)) == LocalFileState::Tracked {
        return Ok(());
    }
    let path = local_settings_path(own);
    let entries = apply_to_file(&path, Some(own), &before, &BTreeSet::new(), desired)?;
    set_record(&mut records.local_settings, key, folder_key, entries);
    Ok(())
}

/// Whether `project` has anything for [`sync_writable_folders`] to do: it
/// uses Claude Code, or Automatic still records entries for it.
pub fn writable_folders_apply(project: &Project) -> bool {
    if uses_claude(project) {
        return true;
    }
    !project.local_key.is_empty()
        && read_records().is_ok_and(|records| {
            records.user_settings.contains_key(&project.local_key)
                || records.local_settings.contains_key(&project.local_key)
        })
}

/// Remove the entries Automatic wrote for `project`, which is about to be
/// deleted: from its own local file while that folder may still be written,
/// and from user settings. Then drop its records.
pub fn release_writable_folders(project: &Project) -> Result<(), String> {
    if project.local_key.is_empty() {
        return Ok(());
    }
    let mut errors = Vec::new();
    if let Some(own) = fs::canonicalize(&project.directory).ok().filter(|_| !project.directory.is_empty()) {
        let _writes = hold_settings_writes();
        let (base, seen) = read_records_seen()?;
        let mut records = base.clone();
        match apply_local(project, &own, &mut records, &BTreeSet::new(), None) {
            Ok(()) => {
                let touched = [own.to_string_lossy().to_string()];
                commit_records(&project.local_key, &RecordScope::Folders(&touched), &records, base, seen)?
            }
            Err(e) => errors.push(e),
        }
    }
    if let Err(e) = forget_writable_folders(&project.local_key) {
        errors.push(e);
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

/// Remove every entry Automatic wrote to user settings for the checkout
/// `local_key`, and drop its records. Its local files are left: use
/// [`release_writable_folders`] while the project is still registered.
pub fn forget_writable_folders(local_key: &str) -> Result<(), String> {
    if local_key.is_empty() {
        return Ok(());
    }
    let _writes = hold_settings_writes();
    let (base, seen) = read_records_seen()?;
    let mut records = base.clone();
    let folders: Vec<String> = records
        .user_settings
        .get(local_key)
        .map(|f| f.keys().cloned().collect())
        .unwrap_or_default();
    let user_path = user_settings_path()?;
    let mut errors = Vec::new();
    for folder in folders {
        let before = record_of(&records.user_settings, local_key, &folder);
        let shared = shared_user_paths(&records, local_key, &folder);
        match apply_to_file(&user_path, None, &before, &shared, &BTreeSet::new()) {
            Ok(entries) => set_record(&mut records.user_settings, local_key, &folder, entries),
            Err(e) => errors.push(e),
        }
    }
    records.local_settings.remove(local_key);
    records.last_errors.remove(local_key);
    commit_records(local_key, &RecordScope::Checkout, &records, base, seen)?;
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

/// Replace the checkout's "Other folders Claude can edit". Each new folder
/// must exist, and is stored as its canonical path, so a link swapped later is
/// noticed. A folder [`refusal_reason`] refuses is an error, and nothing is
/// stored.
pub fn set_project_writable_folders(ident: &str, folders: &[String]) -> Result<(), String> {
    let denied = denied_paths()?;
    // An entry already stored was checked when it was set. It is kept as it is,
    // so a folder on a drive that is not mounted now does not block the edit.
    // Sync checks every entry again before writing it.
    let stored: Vec<String> = serde_json::from_str::<Project>(&read_project(ident)?)
        .map_err(|e| format!("Invalid project data: {}", e))?
        .local_writable_folders;
    let mut canonical = Vec::new();
    for folder in folders {
        if stored.contains(folder) {
            canonical.push(folder.clone());
            continue;
        }
        if !Path::new(folder).is_absolute() {
            return Err(format!("'{}' is not an absolute folder path", folder));
        }
        let real = fs::canonicalize(folder)
            .map_err(|e| format!("'{}' cannot be used: {}", folder, e))?;
        if !real.is_dir() {
            return Err(format!("'{}' is not a folder", folder));
        }
        if let Some(reason) = refusal_reason(&real, &denied) {
            return Err(format!(
                "Claude Code cannot be allowed to edit '{}', because {}.",
                folder, reason
            ));
        }
        let real = real
            .to_str()
            .ok_or_else(|| format!("'{}' is not a UTF-8 path", folder))?
            .to_string();
        canonical.push(real);
    }
    store_project_writable_folders(ident, &canonical)
}

// ── Settings ──────────────────────────────────────────────────────────────────

/// Set where entries go. Choosing user settings is the user's agreement to
/// them, so it records consent too.
pub fn set_writable_folders_target(target: WritableFoldersTarget) -> Result<(), String> {
    update_settings(SettingsRead::Strict, |mut settings| {
        settings.writable_folders_target = target;
        if target == WritableFoldersTarget::AlwaysUser {
            settings.writable_folders_user_consent = true;
        }
        settings
    })
}

/// Record whether user settings may hold project folders. Withdrawing it
/// makes the next sync of each project remove the entries Automatic wrote
/// there, and moves the target back to the local file.
pub fn set_writable_folders_user_consent(consent: bool) -> Result<(), String> {
    update_settings(SettingsRead::Strict, |mut settings| {
        settings.writable_folders_user_consent = consent;
        if !consent {
            settings.writable_folders_target = WritableFoldersTarget::PreferLocal;
        }
        settings
    })
}

/// Turn "Let agents edit across this group" on or off. Returns whether the
/// group changed.
pub fn set_group_agents_edit(name: &str, enabled: bool) -> Result<bool, String> {
    if !is_valid_name(name) {
        return Err("Invalid group name".into());
    }
    let mut group: ProjectGroup = serde_json::from_str(&read_group(name)?)
        .map_err(|e| format!("Group '{}' could not be parsed: {}", name, e))?;
    if group.agents_edit_across_group == enabled {
        return Ok(false);
    }
    group.agents_edit_across_group = enabled;
    let data = serde_json::to_string(&group).map_err(|e| e.to_string())?;
    save_group(name, &data)?;
    Ok(true)
}

#[cfg(test)]
#[path = "claude_dirs_tests.rs"]
mod tests;
