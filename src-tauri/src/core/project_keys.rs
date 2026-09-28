//! Project identity keys: the committed project `id` and this machine's
//! `local_key`.
//!
//! Stage 2 of the project identity plan
//! (`automatic-meta/general/plans/projects/project-identity.md`). Keys are
//! minted only by the create paths and by the startup backfill. Reads and
//! ordinary saves resolve and preserve them but never mint.
//!
//! Resolution rule, used by both `read_project` and `save_project`
//! ([`resolve_stored_keys`]):
//! - `id` comes from the in-directory config when it has one. The registry
//!   pointer only caches it, so after a `git pull` changes the committed id
//!   the file wins.
//! - `local_key` comes from the registry pointer, which is authoritative for
//!   this checkout. The state file only mirrors it. A folder copied with
//!   `cp -r` carries the source checkout's state file, so the state file is
//!   consulted only when the pointer has no key yet.
//!
//! Keys are system-managed. On save a stored key always beats the incoming
//! one, so an editor holding a stale `id` cannot write it back.

use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::*;

/// A project's two identity keys. Either may be empty when not yet minted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectKeys {
    pub id: String,
    pub local_key: String,
}

impl ProjectKeys {
    pub fn of(project: &Project) -> Self {
        Self {
            id: project.id.clone(),
            local_key: project.local_key.clone(),
        }
    }

    pub fn is_complete(&self) -> bool {
        !self.id.is_empty() && !self.local_key.is_empty()
    }
}

/// Mint a fresh key (UUID v4).
pub fn new_project_key() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Which keys [`fill_missing_project_keys`] minted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MintedKeys {
    pub id: bool,
    pub local_key: bool,
}

impl MintedKeys {
    pub fn any(&self) -> bool {
        self.id || self.local_key
    }
}

/// Mint a key for each of `project`'s empty keys. Keys already set are kept.
/// Does no I/O.
pub fn fill_missing_project_keys(project: &mut Project) -> MintedKeys {
    let mut minted = MintedKeys::default();
    if project.id.is_empty() {
        project.id = new_project_key();
        minted.id = true;
    }
    if project.local_key.is_empty() {
        project.local_key = new_project_key();
        minted.local_key = true;
    }
    minted
}

/// Resolve the stored keys from what the folder's files hold (`files`) and
/// what the registry entry caches (`registry`, `None` when there is no
/// entry). `id`: files first, then registry. `local_key`: registry first,
/// then files, and only when a registry entry exists, because without one
/// the folder's state file belongs to some other registration. Does no I/O.
pub(crate) fn resolve_stored_keys(
    files: &ProjectKeys,
    registry: Option<&ProjectKeys>,
) -> ProjectKeys {
    let id = match registry {
        _ if !files.id.is_empty() => files.id.clone(),
        Some(r) => r.id.clone(),
        None => String::new(),
    };
    let local_key = match registry {
        Some(r) if !r.local_key.is_empty() => r.local_key.clone(),
        Some(_) => files.local_key.clone(),
        None => String::new(),
    };
    ProjectKeys { id, local_key }
}

/// Apply stored keys to a project about to be saved. A stored key wins over
/// the incoming one; the incoming key is kept only where nothing is stored,
/// which is the case for the create paths and the backfill. Does no I/O.
pub(crate) fn apply_stored_keys(project: &mut Project, stored: &ProjectKeys) {
    if !stored.id.is_empty() {
        project.id = stored.id.clone();
    }
    if !stored.local_key.is_empty() {
        project.local_key = stored.local_key.clone();
    }
}

/// Give a project being created its keys. A new registration is always a new
/// checkout, so `local_key` is always fresh. `committed_id` is the `id`
/// already committed in the project's folder: keeping it makes a second
/// checkout of a known repo the same project. Does no I/O.
pub fn assign_new_project_keys(project: &mut Project, committed_id: Option<String>) {
    project.id = committed_id
        .filter(|id| !id.is_empty())
        .unwrap_or_else(new_project_key);
    project.local_key = new_project_key();
}

/// The `id` committed in `directory`'s project files, if any. An unreadable
/// or malformed config is an error: guessing would mint a second identity for
/// a project that already has one.
pub fn committed_project_id(directory: &str) -> Result<Option<String>, String> {
    if directory.is_empty() {
        return Ok(None);
    }
    let project = read_project_files(directory, "").map_err(|e| {
        format!(
            "Could not read the existing project id in {}: {}",
            directory, e
        )
    })?;
    Ok(project.map(|p| p.id).filter(|id| !id.is_empty()))
}

/// Assign keys to a project that a create path (Add Project wizard, MCP
/// register) is about to save for the first time. Shared so both surfaces
/// mint the same way.
pub fn prepare_new_project_keys(project: &mut Project) -> Result<(), String> {
    let committed = committed_project_id(&project.directory)?;
    assign_new_project_keys(project, committed);
    Ok(())
}

/// Registry pointer for a project with a directory: `{name, directory}` plus
/// whichever keys are set. The pointer caches the keys so a project whose
/// folder is missing, or was re-cloned without a state file, keeps them.
pub(crate) fn registry_pointer(project: &Project) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert("name".into(), project.name.clone().into());
    map.insert("directory".into(), project.directory.clone().into());
    if !project.id.is_empty() {
        map.insert("id".into(), project.id.clone().into());
    }
    if !project.local_key.is_empty() {
        map.insert("local_key".into(), project.local_key.clone().into());
    }
    serde_json::Value::Object(map)
}

/// Write the registry pointer for `project` to `registry_path`.
pub(crate) fn write_registry_pointer(registry_path: &Path, project: &Project) -> Result<(), String> {
    let pretty = serde_json::to_string_pretty(&registry_pointer(project))
        .map_err(|e| format!("Failed to serialize registry entry for '{}': {}", project.name, e))?;
    fs::write(registry_path, pretty)
        .map_err(|e| format!("Failed to write {}: {}", registry_path.display(), e))
}

/// Keys cached in the registry entry at `registry_path`, or `None` when
/// there is no entry. An entry that does not parse is an error, so a save
/// cannot drop keys it failed to read.
pub(crate) fn read_registry_keys(registry_path: &Path) -> Result<Option<ProjectKeys>, String> {
    if !registry_path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(registry_path)
        .map_err(|e| format!("Failed to read {}: {}", registry_path.display(), e))?;
    let value: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| format!("Failed to parse {}: {}", registry_path.display(), e))?;
    let field = |key: &str| {
        value
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };
    Ok(Some(ProjectKeys {
        id: field("id"),
        local_key: field("local_key"),
    }))
}

/// Keys already stored for a project, resolved by [`resolve_stored_keys`]
/// from its registry entry and its folder. `registry_path` is `None` for a
/// project with no entry yet. `directory` may be empty or missing.
pub(crate) fn stored_project_keys(
    registry_path: Option<&Path>,
    directory: &str,
) -> Result<ProjectKeys, String> {
    let registry = match registry_path {
        Some(path) => read_registry_keys(path)?,
        None => None,
    };
    let files = if directory.is_empty() {
        ProjectKeys::default()
    } else {
        read_project_files(directory, "")?
            .map(|p| ProjectKeys::of(&p))
            .unwrap_or_default()
    };
    Ok(resolve_stored_keys(&files, registry.as_ref()))
}

// ── Startup backfill ────────────────────────────────────────────────────────

/// File name of the backfill lock inside the Automatic data directory.
const LOCK_FILE_NAME: &str = ".project-keys.lock";

/// A lock older than this is left over from a process that died mid-run. A
/// full backfill touches each project once and the store migration each
/// store once, so a live run finishes well within it.
const LOCK_STALE_AFTER: Duration = Duration::from_secs(120);

/// Held backfill lock. Dropping it removes the lock file.
#[derive(Debug)]
pub(crate) struct KeysLock {
    path: PathBuf,
}

impl Drop for KeysLock {
    fn drop(&mut self) {
        if let Err(e) = fs::remove_file(&self.path) {
            if e.kind() != ErrorKind::NotFound {
                eprintln!(
                    "[automatic] could not remove project key lock {}: {}",
                    self.path.display(),
                    e
                );
            }
        }
    }
}

/// Take the lock at `path`, or return `None` while another process holds it.
///
/// The GUI and every `mcp-serve` process run startup housekeeping, so several
/// can start at once. `create_new` makes taking the lock atomic. A lock whose
/// mtime is at least `stale_after` older than `now` is removed and taken once
/// more; if another process wins that retry, this one skips.
pub(crate) fn try_acquire_lock(
    path: &Path,
    now: SystemTime,
    stale_after: Duration,
) -> Result<Option<KeysLock>, String> {
    for attempt in 0..2 {
        match OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(_) => {
                return Ok(Some(KeysLock {
                    path: path.to_path_buf(),
                }))
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                if attempt > 0 {
                    return Ok(None);
                }
                let modified = match fs::metadata(path).and_then(|m| m.modified()) {
                    Ok(time) => time,
                    // Released between our open and this check: try again.
                    Err(e) if e.kind() == ErrorKind::NotFound => continue,
                    Err(e) => {
                        return Err(format!(
                            "Failed to inspect lock {}: {}",
                            path.display(),
                            e
                        ))
                    }
                };
                // A clock that runs backwards reads as a fresh lock, so a
                // live holder is never pre-empted.
                let age = now.duration_since(modified).unwrap_or(Duration::ZERO);
                if age < stale_after {
                    return Ok(None);
                }
                match fs::remove_file(path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == ErrorKind::NotFound => {}
                    Err(e) => {
                        return Err(format!(
                            "Failed to remove stale lock {}: {}",
                            path.display(),
                            e
                        ))
                    }
                }
            }
            Err(e) => {
                return Err(format!("Failed to create lock {}: {}", path.display(), e));
            }
        }
    }
    Ok(None)
}

/// Outcome of [`ensure_project_keys`].
#[derive(Debug, PartialEq, Eq)]
pub enum ProjectKeyBackfill {
    /// Another process is running the backfill. Nothing was changed.
    LockHeld,
    /// The backfill ran over every registry entry.
    Completed {
        /// Registry names whose keys were minted.
        updated: Vec<String>,
        /// Registry names whose file was renamed to `<local_key>.json`.
        renamed: Vec<String>,
        /// Registry names that could not be backfilled, with the reason.
        failed: Vec<(String, String)>,
        /// What re-keying the per-project stores did (stage 3b step 2).
        stores: StoreMigrationReport,
    },
}

/// Give every registered project an `id` and a `local_key`, name its
/// registry file by the `local_key`, then move the per-project stores
/// (memory, features, groups, activity, recommendations, dev servers) from
/// name keys to project keys. Runs at startup, under one lock.
///
/// Idempotent: a project that already has both keys is not written, a
/// file already named by its key is not moved, and store data already keyed
/// is left alone (see `store_migration`). One project's failure does not
/// stop the rest; it is reported in `failed`, and store problems in
/// `stores.problems`.
pub fn ensure_project_keys() -> Result<ProjectKeyBackfill, String> {
    let automatic_dir = get_automatic_dir()?;
    fs::create_dir_all(&automatic_dir)
        .map_err(|e| format!("Failed to create {}: {}", automatic_dir.display(), e))?;
    let lock_path = automatic_dir.join(LOCK_FILE_NAME);
    let Some(_lock) = try_acquire_lock(&lock_path, SystemTime::now(), LOCK_STALE_AFTER)? else {
        return Ok(ProjectKeyBackfill::LockHeld);
    };

    let names = list_projects()?;
    let mut updated = Vec::new();
    let mut renamed = Vec::new();
    let mut failed = Vec::new();
    for name in names {
        match ensure_keys_for_project(&name) {
            Ok(true) => updated.push(name.clone()),
            Ok(false) => {}
            Err(e) => {
                failed.push((name, e));
                continue;
            }
        }
        match rename_registry_file_to_key(&name) {
            Ok(true) => renamed.push(name),
            Ok(false) => {}
            Err(e) => failed.push((name, e)),
        }
    }

    // Stores move only for projects that now have both keys, so this runs
    // after the loop above. A registry that cannot be read is reported and
    // leaves every store untouched.
    let stores = match get_project_summaries() {
        Ok(summaries) => super::store_migration::migrate_project_stores(summaries),
        Err(e) => StoreMigrationReport {
            problems: vec![format!("Could not read the project registry: {}", e)],
            ..Default::default()
        },
    };
    Ok(ProjectKeyBackfill::Completed {
        updated,
        renamed,
        failed,
        stores,
    })
}

/// Mint and persist any missing key for one registry entry. Returns whether
/// anything was written.
fn ensure_keys_for_project(name: &str) -> Result<bool, String> {
    let entry = find_registry_entry(name)?
        .ok_or_else(|| format!("Project '{}' not found", name))?;
    let directory = entry.directory()?.unwrap_or_default();
    // Probe without `read_project`, whose enrichment write-back would touch
    // every project on every start even when nothing is missing.
    if stored_project_keys(Some(&entry.path), &directory)?.is_complete() {
        return Ok(false);
    }

    let raw = read_project(name)?;
    let mut project: Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    if !fill_missing_project_keys(&mut project).any() {
        return Ok(false);
    }

    if project.directory_missing {
        // Saving would recreate the folder. Cache the keys in the pointer;
        // the next read after the folder returns writes them into it.
        write_registry_pointer(&entry.path, &project)?;
    } else {
        let data = serde_json::to_string(&project)
            .map_err(|e| format!("Failed to serialize project '{}': {}", name, e))?;
        save_project(name, &data)?;
    }
    Ok(true)
}

/// Move one legacy registry file, `<name>.json`, to `<local_key>.json`.
/// Returns whether the file moved.
///
/// The stem was the legacy entry's name, so it is written into the `name`
/// field first, in place, along with any key the pointer lacks. Then `fs::rename` moves the file in one step. A
/// crash between the two leaves a legacy file whose `name` field matches
/// its stem, which the next run picks up. A file already at the target is
/// an error and both files are left alone.
fn rename_registry_file_to_key(name: &str) -> Result<bool, String> {
    let entry = find_registry_entry(name)?
        .ok_or_else(|| format!("Project '{}' not found", name))?;
    if entry.keyed {
        return Ok(false);
    }
    let mut value = entry.contents.clone()?;
    // The pointer may lack a key the folder's state file holds; the pointer
    // is about to be named by it, so it must carry it.
    let directory = entry.directory()?.unwrap_or_default();
    let keys = stored_project_keys(Some(&entry.path), &directory)?;
    let local_key = keys.local_key;
    if local_key.is_empty() {
        return Err(format!(
            "{} has no local_key, so it cannot be renamed",
            entry.path.display()
        ));
    }
    if !is_valid_name(&local_key) {
        return Err(format!(
            "{} has an invalid local_key '{}'",
            entry.path.display(),
            local_key
        ));
    }
    let target = entry.path.with_file_name(format!("{}.json", local_key));
    if target.exists() {
        return Err(format!(
            "Cannot rename {} to {}: the target already exists. Both files were left unchanged.",
            entry.path.display(),
            target.display()
        ));
    }

    let field = |value: &serde_json::Value, key: &str| {
        value.get(key).and_then(|v| v.as_str()).map(str::to_string)
    };
    let name_stale = field(&value, "name").as_deref() != Some(entry.name.as_str());
    let key_missing = field(&value, "local_key").as_deref() != Some(local_key.as_str());
    let id_missing = field(&value, "id").unwrap_or_default().is_empty() && !keys.id.is_empty();
    if name_stale || key_missing || id_missing {
        let object = value.as_object_mut().ok_or_else(|| {
            format!("{} is not a JSON object", entry.path.display())
        })?;
        object.insert("name".into(), entry.name.clone().into());
        object.insert("local_key".into(), local_key.clone().into());
        if id_missing {
            object.insert("id".into(), keys.id.clone().into());
        }
        let pretty = serde_json::to_string_pretty(&value)
            .map_err(|e| format!("Failed to serialize {}: {}", entry.path.display(), e))?;
        fs::write(&entry.path, pretty)
            .map_err(|e| format!("Failed to write {}: {}", entry.path.display(), e))?;
    }
    fs::rename(&entry.path, &target).map_err(|e| {
        format!(
            "Failed to rename {} to {}: {}",
            entry.path.display(),
            target.display(),
            e
        )
    })?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_missing_mints_only_empty_keys() {
        let mut project = Project {
            id: "kept-id".into(),
            ..Default::default()
        };
        let minted = fill_missing_project_keys(&mut project);
        assert_eq!(
            minted,
            MintedKeys {
                id: false,
                local_key: true
            }
        );
        assert_eq!(project.id, "kept-id");
        assert!(uuid::Uuid::parse_str(&project.local_key).is_ok());

        let again = fill_missing_project_keys(&mut project);
        assert!(!again.any(), "a second fill mints nothing");
    }

    #[test]
    fn assign_new_keys_keeps_committed_id_and_always_mints_local_key() {
        let mut project = Project {
            id: "ignored".into(),
            local_key: "old-checkout".into(),
            ..Default::default()
        };
        assign_new_project_keys(&mut project, Some("committed".into()));
        assert_eq!(project.id, "committed");
        assert_ne!(project.local_key, "old-checkout");

        assign_new_project_keys(&mut project, None);
        assert!(uuid::Uuid::parse_str(&project.id).is_ok());
    }

    fn keys(id: &str, local_key: &str) -> ProjectKeys {
        ProjectKeys {
            id: id.into(),
            local_key: local_key.into(),
        }
    }

    #[test]
    fn resolve_prefers_file_id_and_registry_local_key() {
        let files = keys("file-id", "copied-key");
        let registry = keys("cached-id", "pointer-key");
        assert_eq!(
            resolve_stored_keys(&files, Some(&registry)),
            keys("file-id", "pointer-key")
        );
        assert_eq!(
            resolve_stored_keys(&keys("", "state-key"), Some(&keys("cached-id", ""))),
            keys("cached-id", "state-key"),
            "each source fills the other's gaps"
        );
        assert_eq!(
            resolve_stored_keys(&files, None),
            keys("file-id", ""),
            "without a registry entry the state file's local_key is foreign"
        );
    }

    #[test]
    fn stored_keys_beat_incoming_keys() {
        let mut project = Project {
            id: "stale".into(),
            local_key: "incoming-key".into(),
            ..Default::default()
        };
        apply_stored_keys(&mut project, &keys("stored-id", ""));
        assert_eq!(project.id, "stored-id");
        assert_eq!(project.local_key, "incoming-key", "incoming fills an empty stored key");
    }

    #[test]
    fn registry_pointer_omits_empty_keys() {
        let project = Project {
            name: "p".into(),
            directory: "/d".into(),
            ..Default::default()
        };
        assert_eq!(
            registry_pointer(&project),
            serde_json::json!({"name": "p", "directory": "/d"})
        );
    }

    #[test]
    fn lock_is_taken_and_released_on_drop() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(LOCK_FILE_NAME);
        let lock = try_acquire_lock(&path, SystemTime::now(), LOCK_STALE_AFTER)
            .unwrap()
            .expect("free lock is taken");
        assert!(path.exists());
        drop(lock);
        assert!(!path.exists(), "dropping the guard removes the lock file");
    }

    #[test]
    fn fresh_lock_held_by_another_process_skips() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(LOCK_FILE_NAME);
        fs::write(&path, "").unwrap();

        let result = try_acquire_lock(&path, SystemTime::now(), LOCK_STALE_AFTER).unwrap();
        assert!(result.is_none(), "a fresh lock must not be taken");
        assert!(path.exists(), "another holder's lock is left alone");
    }

    #[test]
    fn stale_lock_is_replaced() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(LOCK_FILE_NAME);
        fs::write(&path, "").unwrap();

        // Judge the lock from a point past the stale threshold rather than
        // backdating its mtime.
        let later = SystemTime::now() + LOCK_STALE_AFTER + Duration::from_secs(1);
        let lock = try_acquire_lock(&path, later, LOCK_STALE_AFTER)
            .unwrap()
            .expect("stale lock is taken over");
        assert!(path.exists());
        drop(lock);
        assert!(!path.exists());
    }

    #[test]
    fn backfill_moves_name_keyed_stores_to_the_new_keys() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let project = Project {
                name: "legacy".into(),
                ..Default::default()
            };
            save_project("legacy", &serde_json::to_string(&project).unwrap()).unwrap();
            // Data written by the previous release, keyed by name.
            let unregistered = ProjectStoreKeys::unregistered("legacy");
            crate::memory::store_memory(&unregistered, "k", "v", None).unwrap();
            crate::features::create_feature(&unregistered, "f", "", "medium", None, &[], &[], None, None, None)
                .unwrap();
            let groups_dir = get_groups_dir().unwrap();
            fs::create_dir_all(&groups_dir).unwrap();
            fs::write(groups_dir.join("g.json"), r#"{"name":"g","projects":["legacy","ghost"]}"#).unwrap();
            let dev_dir = get_automatic_dir().unwrap().join("dev-servers");
            fs::create_dir_all(&dev_dir).unwrap();
            fs::write(dev_dir.join("legacy.json"), "[]").unwrap();

            let ProjectKeyBackfill::Completed { stores, failed, .. } = ensure_project_keys().unwrap() else {
                panic!("lock unexpectedly held");
            };
            assert!(failed.is_empty(), "{failed:?}");
            assert!(stores.problems.is_empty(), "{:?}", stores.problems);
            assert_eq!(stores.memory_files, 1);
            assert_eq!(stores.feature_rows, 1);
            assert!(stores.activity_rows >= 2, "memory and feature activity: {stores:?}");
            assert_eq!(stores.group_files, 1);
            assert_eq!(stores.dev_server_files, 1);
            let backup = stores.backup_dir.expect("backed up");
            assert!(backup.join("memory/legacy.json").is_file());
            assert!(backup.join("features.db").is_file());
            assert!(backup.join("activity.db").is_file());

            let keys = project_store_keys("legacy").unwrap();
            assert!(crate::memory::get_all_memories(&keys.id).unwrap().contains_key("k"));
            assert_eq!(crate::features::list_features(&keys, None, false).unwrap().len(), 1);
            assert!(crate::activity::get_project_activity(&keys.local_key, 10).unwrap().len() >= 2);
            assert!(dev_dir.join(format!("{}.json", keys.local_key)).is_file());
            let group: ProjectGroup = serde_json::from_str(&read_group("g").unwrap()).unwrap();
            assert_eq!(group.projects, vec!["legacy", "ghost"], "shown by name");

            let ProjectKeyBackfill::Completed { stores, .. } = ensure_project_keys().unwrap() else {
                panic!("lock unexpectedly held");
            };
            assert_eq!(stores, StoreMigrationReport::default(), "a second start changes nothing");
        });
    }

    #[test]
    fn backfill_skips_while_lock_is_held() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let project = Project {
                name: "locked".into(),
                ..Default::default()
            };
            save_project("locked", &serde_json::to_string(&project).unwrap()).unwrap();
            let lock_path = get_automatic_dir().unwrap().join(LOCK_FILE_NAME);
            fs::write(&lock_path, "").unwrap();

            assert_eq!(ensure_project_keys().unwrap(), ProjectKeyBackfill::LockHeld);
            let raw = read_project("locked").unwrap();
            let loaded: Project = serde_json::from_str(&raw).unwrap();
            assert!(loaded.id.is_empty(), "nothing is minted while locked");
            assert!(lock_path.exists(), "the other holder's lock is not removed");
        });
    }
}
