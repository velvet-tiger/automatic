//! Read-only projects.
//!
//! A read-only project is one this machine reads but never writes into. Its
//! settings still load into Automatic, and the MCP service still serves it,
//! but nothing Automatic does changes a file inside its folder. The use case
//! is a repository someone else configured: this checkout reads the
//! committed settings without producing changes in the working tree.
//!
//! The state in effect is per machine. It lives in the registry pointer
//! (`local_read_only` in `~/.automatic/projects/<local_key>.json`), so
//! turning it on or off writes nothing into the folder. The committed
//! `read_only` field in `.automatic.json` is only a hint: a checkout that
//! adds the project from its existing files starts read-only when it is set.

use std::path::{Component, Path, PathBuf};

use super::*;

/// Why a write to a read-only project was refused, and how to allow it.
pub fn read_only_error(project_name: &str) -> String {
    format!(
        "Project '{}' is read only, so Automatic will not change its files. \
         Turn off read-only mode in Configuration > Settings to allow changes.",
        project_name
    )
}

/// Why a write into a folder inside a read-only project was refused.
fn nested_read_only_error(directory: &str, owner_name: &str) -> String {
    format!(
        "{} is inside project '{}', which is read only, so Automatic will not change files in it. \
         Turn off read-only mode for '{}' in Configuration > Settings to allow changes.",
        directory, owner_name, owner_name
    )
}

/// Whether the project with identifier `ident` (a `local_key` or a name) is
/// read-only on this machine, as its registry pointer stores it. An unknown
/// project is not read-only. A pointer that cannot be parsed is an error, so
/// a damaged entry never lets a write through.
pub fn is_project_read_only(ident: &str) -> Result<bool, String> {
    let Some(entry) = resolve_registry_entry(ident)? else {
        return Ok(false);
    };
    let contents = entry.contents.as_ref().map_err(|e| {
        format!(
            "Could not tell whether project '{}' is read only: {}",
            entry.name, e
        )
    })?;
    Ok(pointer_is_read_only(contents))
}

/// Whether the registry file at `path` currently marks its project
/// read-only. Read afresh, so a writer can see a change another process made
/// after it read the project. A missing or unparseable file reports `false`.
pub(crate) fn pointer_file_is_read_only(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .is_some_and(|value| pointer_is_read_only(&value))
}

/// Symlinks [`normalised`] follows in one call before it stops resolving,
/// matching the usual operating-system limit. The count covers the whole
/// call, not one chain, so a link whose target names itself several times
/// cannot fan out into exponential work.
const MAX_SYMLINK_HOPS: usize = 40;

/// Where `path` lands on disk, so a symlinked or `/private`-prefixed form
/// of a folder compares equal to the plain one, even when the folder itself
/// does not exist yet.
///
/// Resolves one component at a time. Each part that exists is resolved
/// before the next `..` applies, because a `..` after a symlink climbs out
/// of the link's target, not out of the folder holding the link. A `..`
/// after a missing part only drops that part, since a missing part cannot
/// be a link. A dangling symlink is followed to its target, so a write that
/// would create the target is still seen.
pub(crate) fn normalised(path: &str) -> PathBuf {
    let mut hops = 0;
    normalise_from(PathBuf::new(), Path::new(path), &mut hops)
}

fn normalise_from(mut resolved: PathBuf, path: &Path, hops: &mut usize) -> PathBuf {
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            Component::Normal(part) => {
                resolved.push(part);
                resolved = resolve_existing(resolved, hops);
            }
            other => resolved.push(other),
        }
    }
    resolved
}

/// `path` with its last part resolved when it exists or is a dangling
/// symlink. Everything before the last part is already resolved. `hops`
/// counts the links followed so far in this call.
fn resolve_existing(path: PathBuf, hops: &mut usize) -> PathBuf {
    if let Ok(real) = path.canonicalize() {
        return real;
    }
    if *hops >= MAX_SYMLINK_HOPS {
        return path;
    }
    match std::fs::read_link(&path) {
        // A target is relative to the folder holding the link.
        Ok(target) => {
            *hops += 1;
            let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
            normalise_from(parent, &target, hops)
        }
        // Missing, or not a link: nothing on disk to resolve.
        Err(_) => path,
    }
}

/// The name of the read-only project whose folder is `directory` or contains
/// it, if any. A folder inside a read-only project is part of that
/// repository, so it is read-only too: registering it as its own project
/// must not open a way to write into the tree. Registry entries that cannot
/// be parsed are skipped and logged, so one damaged file does not stop every
/// write on this machine; the project's own entry is checked strictly by
/// [`is_project_read_only`].
pub fn read_only_owner_of(directory: &str) -> Result<Option<String>, String> {
    if directory.is_empty() {
        return Ok(None);
    }
    Ok(owner_containing(&read_only_owners()?, directory))
}

/// [`read_only_owner_of`] for display only, such as the `read_only_owner`
/// field `read_project` fills in. Reuses the owner list while the registry
/// folder is unchanged, so reading every project does not rescan the
/// registry once per project. Never use it to decide whether to write: the
/// folder's modified time may be coarse, so the list can lag a change.
pub(crate) fn read_only_owner_for_display(directory: &str) -> Result<Option<String>, String> {
    if directory.is_empty() {
        return Ok(None);
    }
    let projects_dir = get_projects_dir()?;
    let modified = std::fs::metadata(&projects_dir).and_then(|m| m.modified()).ok();
    let mut cache = OWNER_CACHE
        .lock()
        .map_err(|_| "The read-only owner cache is unavailable".to_string())?;
    let fresh = matches!(
        (&*cache, modified),
        (Some(c), Some(m)) if c.projects_dir == projects_dir && c.modified == m
    );
    if !fresh {
        let owners = read_only_owners()?;
        *cache = modified.map(|modified| OwnerCache {
            projects_dir: projects_dir.clone(),
            modified,
            owners: owners.clone(),
        });
        return Ok(owner_containing(&owners, directory));
    }
    Ok(cache
        .as_ref()
        .and_then(|c| owner_containing(&c.owners, directory)))
}

/// Read-only projects as (name, normalised folder), from the registry.
type Owners = Vec<(String, PathBuf)>;

struct OwnerCache {
    projects_dir: PathBuf,
    modified: std::time::SystemTime,
    owners: Owners,
}

static OWNER_CACHE: std::sync::Mutex<Option<OwnerCache>> = std::sync::Mutex::new(None);

/// Every read-only project with a folder, read from the registry now.
fn read_only_owners() -> Result<Owners, String> {
    let mut owners = Vec::new();
    for entry in scan_registry()? {
        let contents = match &entry.contents {
            Ok(contents) => contents,
            Err(e) => {
                eprintln!(
                    "read-only check: skipping unreadable registry entry {}: {}",
                    entry.path.display(),
                    e
                );
                continue;
            }
        };
        if !pointer_is_read_only(contents) {
            continue;
        }
        if let Some(owner_dir) = entry.directory()? {
            owners.push((entry.name, normalised(&owner_dir)));
        }
    }
    Ok(owners)
}

/// The first owner whose folder is `directory` or contains it.
fn owner_containing(owners: &Owners, directory: &str) -> Option<String> {
    let target = normalised(directory);
    owners
        .iter()
        .find(|(_, owner_dir)| target.starts_with(owner_dir))
        .map(|(name, _)| name.clone())
}

/// Whether `directory` is the folder of a read-only project or sits inside
/// one. Used by paths that write to a folder without a project record, such
/// as `automatic init` and discarding an orphan config.
pub fn is_directory_read_only(directory: &str) -> Result<bool, String> {
    Ok(read_only_owner_of(directory)?.is_some())
}

/// The folder of the project with identifier `ident`, or `None` for an
/// unknown project or one without a folder.
pub fn project_directory(ident: &str) -> Result<Option<String>, String> {
    match resolve_registry_entry(ident)? {
        Some(entry) => entry.directory(),
        None => Ok(None),
    }
}

/// Whether a parsed registry pointer marks its project read-only. For the
/// raw JSON migrations that walk registry files without building a
/// [`Project`].
pub(crate) fn pointer_is_read_only(pointer: &serde_json::Value) -> bool {
    pointer
        .get("local_read_only")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Why `project` may not be written, or `None` when it may. Checks its own
/// flag, then its stored registry pointer (`project` may have been read
/// before read-only mode was turned on), then whether its folder sits inside
/// a read-only project.
fn read_only_reason(project: &Project) -> Result<Option<String>, String> {
    if project.local_read_only
        || (!project.local_key.is_empty() && is_project_read_only(&project.local_key)?)
    {
        return Ok(Some(read_only_error(&project.name)));
    }
    Ok(read_only_owner_of(&project.directory)?.map(|owner| {
        if owner == project.name {
            read_only_error(&owner)
        } else {
            nested_read_only_error(&project.directory, &owner)
        }
    }))
}

/// Whether Automatic must not write into `project`'s folder. See
/// [`ensure_project_writable`].
pub fn project_is_read_only(project: &Project) -> Result<bool, String> {
    Ok(read_only_reason(project)?.is_some())
}

/// `Ok` when Automatic may write into `project`'s folder, or the read-only
/// error. Every writer that touches a project folder calls this first.
pub fn ensure_project_writable(project: &Project) -> Result<(), String> {
    match read_only_reason(project)? {
        Some(reason) => Err(reason),
        None => Ok(()),
    }
}

/// `Ok` when Automatic may write into `directory`, or the error naming the
/// read-only project that holds it.
pub fn ensure_directory_writable(directory: &str) -> Result<(), String> {
    match read_only_owner_of(directory)? {
        Some(owner) => Err(nested_read_only_error(directory, &owner)),
        None => Ok(()),
    }
}

/// Turn read-only mode on or off for the project with identifier `ident` on
/// this machine. Edits only the registry pointer, so it is allowed while the
/// project is read-only and never touches the folder. Idempotent.
pub fn set_project_read_only(ident: &str, read_only: bool) -> Result<(), String> {
    if !is_valid_name(ident) {
        return Err("Invalid project name".into());
    }
    let entry =
        resolve_registry_entry(ident)?.ok_or_else(|| format!("Project '{}' not found", ident))?;
    // Read-only mode protects a folder. A project without one has nothing
    // to protect, and its settings live only on this machine.
    if read_only && entry.directory()?.is_none() {
        return Err(format!(
            "Project '{}' has no folder, so it cannot be made read only.",
            entry.name
        ));
    }
    let mut value = entry
        .contents
        .clone()
        .map_err(|e| format!("Invalid project registry data for '{}': {}", entry.name, e))?;
    let map = value.as_object_mut().ok_or_else(|| {
        format!(
            "{} does not contain a JSON object",
            entry.path.display()
        )
    })?;
    if read_only {
        map.insert("local_read_only".into(), true.into());
    } else {
        map.remove("local_read_only");
    }
    let pretty = serde_json::to_string_pretty(&value).map_err(|e| {
        format!(
            "Failed to serialize registry entry for '{}': {}",
            entry.name, e
        )
    })?;
    write_registry_file(&entry.path, &pretty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::paths::with_test_home;
    use std::fs;

    fn read_json(path: &std::path::Path) -> serde_json::Value {
        serde_json::from_str(&fs::read_to_string(path).expect("read")).expect("parse")
    }

    /// Save a keyed project in `<home>/repo` and return its `local_key`.
    fn keyed_project(home: &std::path::Path, name: &str) -> (String, std::path::PathBuf) {
        let dir = home.join(name);
        fs::create_dir_all(&dir).expect("mkdir");
        let mut project = Project {
            name: name.into(),
            directory: dir.display().to_string(),
            ..Default::default()
        };
        prepare_new_project_keys(&mut project).expect("keys");
        save_project(name, &serde_json::to_string(&project).unwrap()).expect("save");
        (project.local_key, dir)
    }

    fn load(ident: &str) -> Project {
        serde_json::from_str(&read_project(ident).expect("read")).expect("parse")
    }

    #[test]
    fn setting_read_only_edits_only_the_pointer() {
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let (key, dir) = keyed_project(home.path(), "repo");
            let config_before = fs::read(dir.join(CONFIG_FILE_NAME)).unwrap();
            let state_path = dir.join(".automatic").join("project.json");
            let state_before = fs::read(&state_path).unwrap();

            set_project_read_only(&key, true).expect("on");
            assert!(is_project_read_only(&key).unwrap());
            assert!(load(&key).local_read_only);
            assert_eq!(fs::read(dir.join(CONFIG_FILE_NAME)).unwrap(), config_before);
            assert_eq!(fs::read(&state_path).unwrap(), state_before);
            assert!(read_json(&dir.join(CONFIG_FILE_NAME)).get("local_read_only").is_none());

            let summary = get_project_summaries()
                .unwrap()
                .into_iter()
                .find(|s| s.local_key == key)
                .unwrap();
            assert!(summary.read_only, "the summary reports read-only mode");

            set_project_read_only(&key, false).expect("off");
            assert!(!is_project_read_only(&key).unwrap());
            set_project_read_only(&key, false).expect("idempotent");
        });
    }

    #[test]
    fn save_refuses_a_read_only_project_and_keeps_the_flag() {
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let (key, dir) = keyed_project(home.path(), "repo");
            set_project_read_only(&key, true).expect("on");
            let config_before = fs::read(dir.join(CONFIG_FILE_NAME)).unwrap();

            // Stale data from an editor that still thinks it is writable.
            let mut stale = load(&key);
            stale.local_read_only = false;
            stale.description = "edited".into();
            let err = save_project(&key, &serde_json::to_string(&stale).unwrap())
                .expect_err("a read-only project is not saved");
            assert!(err.contains("read only"), "{}", err);
            assert_eq!(fs::read(dir.join(CONFIG_FILE_NAME)).unwrap(), config_before);
            assert!(is_project_read_only(&key).unwrap(), "the save cannot turn it off");

            let mut project = load(&key);
            assert!(ensure_project_writable(&project).is_err());
            // A project read before read-only mode was turned on still hits
            // the stored flag.
            project.local_read_only = false;
            assert!(ensure_project_writable(&project).is_err());
        });
    }

    #[test]
    fn import_starts_read_only_when_the_config_asks_and_writes_nothing() {
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("clone");
            fs::create_dir_all(&dir).expect("mkdir");
            let config_path = dir.join(CONFIG_FILE_NAME);
            let config = r#"{"id":"committed-id","name":"cloned","read_only":true}"#;
            fs::write(&config_path, config).expect("write config");

            let imported = import_existing_project(dir.to_str().unwrap()).expect("import");
            assert!(imported.read_only);
            assert!(is_project_read_only(&imported.local_key).unwrap());

            refresh_project_files(&imported.local_key).expect("refresh");
            assert_eq!(fs::read_to_string(&config_path).unwrap(), config);
            assert!(!dir.join(".automatic").exists(), "no state file is written");

            let loaded = load(&imported.local_key);
            assert!(loaded.read_only, "the committed hint is read");
            assert!(loaded.local_read_only);
        });
    }

    #[test]
    fn import_without_the_hint_is_writable() {
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("clone");
            fs::create_dir_all(&dir).expect("mkdir");
            fs::write(
                dir.join(CONFIG_FILE_NAME),
                r#"{"id":"committed-id","name":"cloned"}"#,
            )
            .expect("write config");

            let imported = import_existing_project(dir.to_str().unwrap()).expect("import");
            assert!(!imported.read_only);
            assert!(!is_project_read_only(&imported.local_key).unwrap());
        });
    }

    #[test]
    fn the_hint_is_committed_but_does_not_make_this_machine_read_only() {
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let (key, dir) = keyed_project(home.path(), "repo");
            let mut project = load(&key);
            project.read_only = true;
            save_project(&key, &serde_json::to_string(&project).unwrap()).expect("save hint");

            assert_eq!(read_json(&dir.join(CONFIG_FILE_NAME))["read_only"], true);
            assert!(!is_project_read_only(&key).unwrap());
            assert!(ensure_project_writable(&load(&key)).is_ok());
        });
    }

    #[test]
    fn a_folder_inside_a_read_only_project_is_read_only() {
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let (key, dir) = keyed_project(home.path(), "repo");
            set_project_read_only(&key, true).expect("on");
            let sub = dir.join("packages").join("api");
            let sub_str = sub.display().to_string();

            // The subfolder does not exist yet, and is still covered.
            assert!(is_directory_read_only(&sub_str).unwrap());
            let err = assert_can_create_project("api", &sub_str).expect_err("create refused");
            assert!(err.contains("inside project 'repo'"), "{}", err);

            fs::create_dir_all(&sub).expect("mkdir");
            let nested = Project {
                name: "api".into(),
                directory: sub_str.clone(),
                agents: vec!["claude".into()],
                ..Default::default()
            };
            let err = save_project("api", &serde_json::to_string(&nested).unwrap())
                .expect_err("save refused");
            assert!(err.contains("read only"), "{}", err);
            assert!(ensure_project_writable(&nested).is_err());
            let mut init = nested.clone();
            assert!(crate::sync::sync_to_directory(&mut init).is_err());
            fs::write(sub.join(CONFIG_FILE_NAME), "{}").expect("plant config");
            assert!(delete_project_config(&sub_str).is_err());
            assert!(sub.join(CONFIG_FILE_NAME).exists(), "the planted file is left alone");
            assert_eq!(
                fs::read_dir(&sub).unwrap().count(),
                1,
                "nothing else was written into the subfolder"
            );

            // A sibling folder outside the project is not affected.
            let sibling = home.path().join("repo-other");
            assert!(!is_directory_read_only(&sibling.display().to_string()).unwrap());
        });
    }

    #[test]
    fn a_stale_pointer_write_keeps_read_only_on() {
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let (key, _dir) = keyed_project(home.path(), "repo");
            // Read before another process turns read-only mode on.
            let stale = load(&key);
            set_project_read_only(&key, true).expect("on");

            let entry = resolve_registry_entry(&key).unwrap().unwrap();
            write_registry_pointer(&entry.path, &stale).expect("stale pointer write");
            assert!(is_project_read_only(&key).unwrap(), "the flag survives");

            rename_project(&key, "renamed").expect("rename");
            assert!(is_project_read_only(&key).unwrap(), "a rename keeps it");
        });
    }

    #[test]
    fn a_damaged_pointer_is_not_treated_as_writable() {
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let (key, _dir) = keyed_project(home.path(), "repo");
            let path = resolve_registry_entry(&key).unwrap().unwrap().path;
            // Still found by its file name, which is the key, but unparseable.
            fs::write(&path, "{ not json").expect("damage");
            let err = is_project_read_only(&key).expect_err("a damaged pointer is an error");
            assert!(err.contains("Could not tell whether project"), "{}", err);
            let stale = Project {
                name: "repo".into(),
                local_key: key.clone(),
                ..Default::default()
            };
            assert!(ensure_project_writable(&stale).is_err(), "and the write is refused");
        });
    }


    #[test]
    fn a_path_that_climbs_out_of_a_missing_folder_is_still_caught() {
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let (key, dir) = keyed_project(home.path(), "repo");
            set_project_read_only(&key, true).expect("on");
            let sneaky = home.path().join("missing").join("..").join("repo").join("sub");
            assert!(is_directory_read_only(&sneaky.display().to_string()).unwrap());
            assert!(dir.exists());
        });
    }

    #[test]
    fn a_project_inside_a_folder_made_read_only_later_is_refused() {
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let (outer, dir) = keyed_project(home.path(), "repo");
            let sub = dir.join("api");
            fs::create_dir_all(&sub).expect("mkdir");
            let mut inner = Project {
                name: "api".into(),
                directory: sub.display().to_string(),
                ..Default::default()
            };
            prepare_new_project_keys(&mut inner).expect("keys");
            save_project("api", &serde_json::to_string(&inner).unwrap()).expect("save before");

            set_project_read_only(&outer, true).expect("outer on");
            let err = crate::commands::save_doc_note(&inner.local_key, "note", "text")
                .expect_err("a nested project's direct writes are refused");
            assert!(err.contains("inside project 'repo'"), "{}", err);
        });
    }


    #[cfg(unix)]
    #[test]
    fn a_symlink_followed_by_parent_dir_is_resolved_like_the_os_does() {
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let (key, dir) = keyed_project(home.path(), "repo");
            set_project_read_only(&key, true).expect("on");
            let inside = dir.join("pkg").join("api");
            fs::create_dir_all(&inside).expect("mkdir");
            let other = home.path().join("other");
            fs::create_dir_all(&other).expect("mkdir other");
            std::os::unix::fs::symlink(&inside, other.join("link")).expect("symlink");

            // The OS follows `link` first, so `link/..` is `<repo>/pkg`.
            let sneaky = other.join("link").join("..").join("new");
            assert!(
                is_directory_read_only(&sneaky.display().to_string()).unwrap(),
                "{} lands inside the read-only project",
                sneaky.display()
            );
        });
    }

    /// A `..` after a symlink climbs out of the link's target. Checking the
    /// path as written would put each of these outside the read-only
    /// project, while a write through it lands inside.
    #[cfg(unix)]
    #[test]
    fn a_path_through_a_symlink_is_checked_where_it_lands() {
        use std::os::unix::fs::symlink;
        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let (key, dir) = keyed_project(home.path(), "repo");
            set_project_read_only(&key, true).expect("on");
            let outside = home.path().join("outside");
            fs::create_dir_all(dir.join("deep")).expect("mkdir deep");
            fs::create_dir_all(&outside).expect("mkdir outside");
            symlink(dir.join("deep"), outside.join("link")).expect("link");
            symlink(dir.join("new"), outside.join("dangling")).expect("dangling link");
            let read_only = |path: std::path::PathBuf| {
                is_directory_read_only(&path.display().to_string()).unwrap()
            };

            // `outside/link/..` is `repo`, not `outside`.
            assert!(read_only(outside.join("link").join("..").join("x")));
            // The same link reached after climbing out of a missing folder.
            assert!(read_only(
                outside.join("missing").join("..").join("link").join("..").join("x")
            ));
            // A dangling link whose target would be created inside the project.
            assert!(read_only(outside.join("dangling").join("x")));
            assert!(!dir.join("new").exists());

            // The plain folder outside the project stays writable.
            assert!(!read_only(outside.join("x")));
            assert!(!read_only(outside.join("missing").join("..").join("x")));
        });
    }

    /// A symlink cycle stops resolving instead of looping.
    #[cfg(unix)]
    #[test]
    fn a_symlink_cycle_does_not_hang() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().expect("tempdir");
        symlink(dir.path().join("b"), dir.path().join("a")).expect("a");
        symlink(dir.path().join("a"), dir.path().join("b")).expect("b");
        let path = dir.path().join("a").join("x");
        let _ = normalised(&path.display().to_string());
    }

    /// A dangling link whose target names itself several times would branch
    /// at every hop if the limit counted depth: each `x/../..` climbs back
    /// out to the folder holding `l`, so every `l` follows the link again.
    /// The limit counts every link followed, so this returns at once.
    #[cfg(unix)]
    #[test]
    fn a_self_referencing_dangling_link_does_not_fan_out() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().expect("tempdir");
        symlink("l/x/../../l/x/../../l/x/../../l", dir.path().join("l")).expect("l");
        let started = std::time::Instant::now();
        let _ = normalised(&dir.path().join("l").join("x").display().to_string());
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "resolving took {:?}",
            started.elapsed()
        );
    }
}
