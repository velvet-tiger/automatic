//! Project registry resolver: maps a project name to its registry file.
//!
//! Stage 3a of the project identity plan
//! (`automatic-meta/general/plans/projects/project-identity.md`). Registry
//! files live in `~/.automatic/projects/`. Two layouts exist side by side
//! until the startup migration has renamed every file:
//!
//! - **Keyed:** `<local_key>.json`. The entry's `name` field is the name.
//! - **Legacy:** `<name>.json`. The file stem is the name, as it always was.
//!   A legacy pointer's `name` field can disagree with its stem, and callers
//!   have only ever seen the stem, so the stem wins. The migration writes the
//!   stem into the `name` field before it renames the file.
//!
//! An entry counts as keyed only when its `local_key` field equals its stem.
//! An entry whose JSON cannot be read or parsed is named by its stem, so a
//! damaged legacy file still resolves as it did before. A damaged keyed file
//! therefore shows up under its key until it is repaired.
//!
//! Names compare case-insensitively (Unicode lowercase), matching the Add
//! Project wizard and the default macOS filesystem. Two entries that claim
//! the same name are an error that names both files: the resolver never
//! guesses between them.
//!
//! Cost: every lookup reads and parses every registry file. Entries are
//! small and a user has tens of projects, not thousands.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::paths::{get_projects_dir, is_valid_name};

/// One registry file and the project name it holds.
#[derive(Debug)]
pub(crate) struct RegistryEntry {
    pub name: String,
    pub path: PathBuf,
    /// True when the file is named by the entry's `local_key`.
    pub keyed: bool,
    /// The parsed file, or why it could not be read or parsed.
    pub contents: Result<serde_json::Value, String>,
}

impl RegistryEntry {
    /// The entry's `directory`, or `None` when it has none. An entry that
    /// could not be parsed is an error, so a caller cannot mistake a damaged
    /// entry for one without a directory.
    pub fn directory(&self) -> Result<Option<String>, String> {
        let value = self
            .contents
            .as_ref()
            .map_err(|e| format!("Invalid project registry data: {}", e))?;
        Ok(value
            .get("directory")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string))
    }
}

/// Whether two project names are the same name. Ignores case.
pub(crate) fn same_project_name(a: &str, b: &str) -> bool {
    a == b || a.to_lowercase() == b.to_lowercase()
}

fn string_field<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(|v| v.as_str())
}

/// Whether a registry file with this stem and contents uses the keyed
/// layout. Does no I/O.
pub(crate) fn is_keyed_entry(stem: &str, contents: Option<&serde_json::Value>) -> bool {
    contents
        .and_then(|v| string_field(v, "local_key"))
        .is_some_and(|key| !key.is_empty() && key == stem)
}

/// The project name a registry file holds. See the module docs for the
/// rules. Does no I/O.
pub(crate) fn registry_entry_name(stem: &str, contents: Option<&serde_json::Value>) -> String {
    if is_keyed_entry(stem, contents) {
        if let Some(name) = contents
            .and_then(|v| string_field(v, "name"))
            .filter(|n| is_valid_name(n))
        {
            return name.to_string();
        }
    }
    stem.to_string()
}

/// Read every registry file in `projects_dir`. A missing directory holds no
/// entries. A file removed while the scan runs (another process renaming it)
/// is skipped.
pub(crate) fn scan_registry_in(projects_dir: &Path) -> Result<Vec<RegistryEntry>, String> {
    if !projects_dir.exists() {
        return Ok(Vec::new());
    }
    let dir_entries = fs::read_dir(projects_dir)
        .map_err(|e| format!("Failed to read {}: {}", projects_dir.display(), e))?;

    let mut entries = Vec::new();
    for dir_entry in dir_entries.flatten() {
        let path = dir_entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()).map(str::to_string) else {
            continue;
        };
        if !is_valid_name(&stem) {
            continue;
        }
        let contents = match fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str::<serde_json::Value>(&raw)
                .map_err(|e| format!("Failed to parse {}: {}", path.display(), e)),
            Err(e) if e.kind() == ErrorKind::NotFound => continue,
            Err(e) => Err(format!("Failed to read {}: {}", path.display(), e)),
        };
        let name = registry_entry_name(&stem, contents.as_ref().ok());
        let keyed = is_keyed_entry(&stem, contents.as_ref().ok());
        entries.push(RegistryEntry {
            name,
            path,
            keyed,
            contents,
        });
    }
    // Directory order is arbitrary. Sorting keeps errors and lists stable.
    entries.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.path.cmp(&b.path)));
    Ok(entries)
}

/// Every project name in `entries`, sorted, each name once.
pub(crate) fn registry_names(entries: &[RegistryEntry]) -> Vec<String> {
    let mut names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
    names.sort();
    names.dedup();
    names
}

/// Index of the entry named `name` in `entries`, ignoring case. Two or more
/// matches are an error naming each file. Does no I/O.
pub(crate) fn find_entry_index(entries: &[RegistryEntry], name: &str) -> Result<Option<usize>, String> {
    let matches: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| same_project_name(&e.name, name))
        .map(|(i, _)| i)
        .collect();
    match matches.as_slice() {
        [] => Ok(None),
        [only] => Ok(Some(*only)),
        many => {
            let files: Vec<String> = many
                .iter()
                .map(|&i| entries[i].path.display().to_string())
                .collect();
            Err(format!(
                "More than one project is named '{}'. Rename or remove one of these registry files: {}",
                name,
                files.join(", ")
            ))
        }
    }
}

/// The registry entry named `name` in `projects_dir`, if any.
pub(crate) fn find_registry_entry_in(
    projects_dir: &Path,
    name: &str,
) -> Result<Option<RegistryEntry>, String> {
    let mut entries = scan_registry_in(projects_dir)?;
    Ok(find_entry_index(&entries, name)?.map(|i| entries.swap_remove(i)))
}

/// Every registry entry in the user's projects directory.
pub(crate) fn scan_registry() -> Result<Vec<RegistryEntry>, String> {
    scan_registry_in(&get_projects_dir()?)
}

/// The registry entry named `name` in the user's projects directory, if any.
pub(crate) fn find_registry_entry(name: &str) -> Result<Option<RegistryEntry>, String> {
    find_registry_entry_in(&get_projects_dir()?, name)
}

/// Path for a registry entry that does not exist yet. A project with a
/// `local_key` gets `<local_key>.json`. One without keeps the legacy
/// `<name>.json` until the startup backfill mints its keys and renames it:
/// ordinary saves never mint keys. A file already at the target belongs to
/// another project, since no entry holds `name`, so it is an error rather
/// than an overwrite.
pub(crate) fn new_registry_path(
    projects_dir: &Path,
    name: &str,
    local_key: &str,
) -> Result<PathBuf, String> {
    let stem = if local_key.is_empty() { name } else { local_key };
    if !is_valid_name(stem) {
        return Err(format!(
            "Cannot create a registry entry for project '{}': '{}' is not a valid file name",
            name, stem
        ));
    }
    let path = projects_dir.join(format!("{}.json", stem));
    if path.exists() {
        return Err(format!(
            "Cannot create a registry entry for project '{}': {} already belongs to another project",
            name,
            path.display()
        ));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn write(dir: &Path, file: &str, value: &serde_json::Value) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(file), value.to_string()).unwrap();
    }

    #[test]
    fn keyed_entry_is_named_by_its_name_field() {
        let value = json!({"name": "site", "local_key": "k1"});
        assert_eq!(registry_entry_name("k1", Some(&value)), "site");
        assert!(is_keyed_entry("k1", Some(&value)));
    }

    #[test]
    fn legacy_entry_is_named_by_its_stem() {
        let divergent = json!({"name": "other", "directory": "/d"});
        assert_eq!(registry_entry_name("site", Some(&divergent)), "site");
        let keyed_elsewhere = json!({"name": "other", "local_key": "k1"});
        assert_eq!(
            registry_entry_name("site", Some(&keyed_elsewhere)),
            "site",
            "a key that is not the stem means the file was not renamed yet"
        );
        assert!(!is_keyed_entry("site", Some(&keyed_elsewhere)));
    }

    #[test]
    fn unparseable_or_nameless_entry_falls_back_to_its_stem() {
        assert_eq!(registry_entry_name("broken", None), "broken");
        let nameless = json!({"local_key": "k1"});
        assert_eq!(registry_entry_name("k1", Some(&nameless)), "k1");
        let bad_name = json!({"name": "../x", "local_key": "k1"});
        assert_eq!(registry_entry_name("k1", Some(&bad_name)), "k1");
    }

    #[test]
    fn scan_names_a_mix_of_layouts() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write(dir, "legacy.json", &json!({"name": "legacy", "directory": "/a"}));
        write(dir, "k1.json", &json!({"name": "keyed", "directory": "/b", "local_key": "k1"}));
        fs::write(dir.join("broken.json"), "{ not json").unwrap();
        fs::write(dir.join("notes.txt"), "ignored").unwrap();

        let entries = scan_registry_in(dir).unwrap();
        assert_eq!(registry_names(&entries), vec!["broken", "keyed", "legacy"]);
        let keyed = &entries[find_entry_index(&entries, "keyed").unwrap().unwrap()];
        assert!(keyed.keyed);
        assert_eq!(keyed.path, dir.join("k1.json"));
        assert_eq!(keyed.directory().unwrap().as_deref(), Some("/b"));
        let broken = &entries[find_entry_index(&entries, "broken").unwrap().unwrap()];
        assert!(broken.directory().is_err(), "a damaged entry is not directory-less");
    }

    #[test]
    fn scan_of_a_missing_directory_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(scan_registry_in(&tmp.path().join("nope")).unwrap().is_empty());
    }

    #[test]
    fn find_ignores_case() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "k1.json", &json!({"name": "Website", "local_key": "k1"}));
        let entry = find_registry_entry_in(tmp.path(), "website")
            .unwrap()
            .expect("case-insensitive match");
        assert_eq!(entry.name, "Website");
        assert!(find_registry_entry_in(tmp.path(), "other").unwrap().is_none());
    }

    #[test]
    fn two_files_with_one_name_are_an_error_naming_both() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "k1.json", &json!({"name": "site", "local_key": "k1"}));
        write(tmp.path(), "k2.json", &json!({"name": "SITE", "local_key": "k2"}));

        let err = find_registry_entry_in(tmp.path(), "site").unwrap_err();
        assert!(err.contains("k1.json") && err.contains("k2.json"), "{err}");
        assert_eq!(
            registry_names(&scan_registry_in(tmp.path()).unwrap()),
            vec!["SITE", "site"],
            "listing still shows both"
        );
    }

    #[test]
    fn new_path_prefers_the_local_key_and_never_overwrites() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        assert_eq!(new_registry_path(dir, "site", "k1").unwrap(), dir.join("k1.json"));
        assert_eq!(new_registry_path(dir, "site", "").unwrap(), dir.join("site.json"));

        write(dir, "k1.json", &json!({"name": "taken", "local_key": "k1"}));
        assert!(new_registry_path(dir, "site", "k1").is_err());
        assert!(new_registry_path(dir, "site", "../escape").is_err());
    }
}
