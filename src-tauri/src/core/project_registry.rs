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
//!
//! **Identifiers (stage 3b, step 1).** Commands accept a project
//! *identifier*: a `local_key` or a name. [`resolve_entry_index`] tries the
//! `local_key` first, as an exact match against each entry's pointer field
//! (a keyed entry's stem is its `local_key`, and a legacy entry may already
//! carry one). Only then does it fall back to the case-insensitive name
//! lookup. A string that is one project's `local_key` and another project's
//! name therefore resolves to the key's project. Keys are UUID v4 strings,
//! so this does not collide with human-chosen names in practice.

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
    /// The `local_key` the entry's file carries, if any. `None` for an
    /// entry that could not be parsed or has no key yet.
    pub fn local_key(&self) -> Option<&str> {
        self.string_field("local_key")
    }

    /// The `id` cached in the entry's file, if any.
    pub fn cached_id(&self) -> Option<&str> {
        self.string_field("id")
    }

    fn string_field(&self, key: &str) -> Option<&str> {
        self.contents
            .as_ref()
            .ok()
            .and_then(|v| string_field(v, key))
            .filter(|s| !s.is_empty())
    }

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

/// Index of the entry that `ident` identifies: the entry whose `local_key`
/// equals `ident` exactly, else the entry named `ident` (ignoring case). See
/// the module docs for why a key match wins. Two entries carrying the same
/// `local_key`, or two entries with the name, are an error naming each file.
/// Does no I/O.
pub(crate) fn resolve_entry_index(
    entries: &[RegistryEntry],
    ident: &str,
) -> Result<Option<usize>, String> {
    let key_matches: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.local_key() == Some(ident))
        .map(|(i, _)| i)
        .collect();
    match key_matches.as_slice() {
        [] => find_entry_index(entries, ident),
        [only] => Ok(Some(*only)),
        many => {
            let files: Vec<String> = many
                .iter()
                .map(|&i| entries[i].path.display().to_string())
                .collect();
            Err(format!(
                "More than one registry file carries the local key '{}': {}",
                ident,
                files.join(", ")
            ))
        }
    }
}

/// The registry entry that `ident` (a `local_key` or a name) identifies in
/// `projects_dir`, if any.
pub(crate) fn resolve_registry_entry_in(
    projects_dir: &Path,
    ident: &str,
) -> Result<Option<RegistryEntry>, String> {
    let mut entries = scan_registry_in(projects_dir)?;
    Ok(resolve_entry_index(&entries, ident)?.map(|i| entries.swap_remove(i)))
}

/// The registry entry that `ident` (a `local_key` or a name) identifies in
/// the user's projects directory, if any.
pub(crate) fn resolve_registry_entry(ident: &str) -> Result<Option<RegistryEntry>, String> {
    resolve_registry_entry_in(&get_projects_dir()?, ident)
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
    fn resolve_matches_a_local_key_exactly() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "k1.json", &json!({"name": "site", "local_key": "k1"}));
        let entry = resolve_registry_entry_in(tmp.path(), "k1").unwrap().expect("key match");
        assert_eq!(entry.name, "site");
        assert!(
            resolve_registry_entry_in(tmp.path(), "K1").unwrap().is_none(),
            "keys compare exactly"
        );
    }

    #[test]
    fn resolve_matches_a_legacy_pointer_key() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "legacy.json", &json!({"name": "legacy", "local_key": "k9"}));
        let entry = resolve_registry_entry_in(tmp.path(), "k9").unwrap().expect("pointer key");
        assert_eq!(entry.name, "legacy");
        assert!(!entry.keyed);
    }

    #[test]
    fn resolve_falls_back_to_a_case_insensitive_name() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "k1.json", &json!({"name": "Website", "local_key": "k1"}));
        let entry = resolve_registry_entry_in(tmp.path(), "website").unwrap().expect("name");
        assert_eq!(entry.name, "Website");
    }

    #[test]
    fn resolve_prefers_a_key_over_a_name() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "k1.json", &json!({"name": "first", "local_key": "k1"}));
        write(tmp.path(), "k2.json", &json!({"name": "k1", "local_key": "k2"}));
        let entry = resolve_registry_entry_in(tmp.path(), "k1").unwrap().expect("match");
        assert_eq!(entry.name, "first", "the key wins over the other project's name");
        assert_eq!(entry.path, tmp.path().join("k1.json"));
    }

    #[test]
    fn resolve_of_an_unknown_identifier_is_none() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "k1.json", &json!({"name": "site", "local_key": "k1"}));
        assert!(resolve_registry_entry_in(tmp.path(), "nope").unwrap().is_none());
        assert!(resolve_registry_entry_in(tmp.path(), "").unwrap().is_none());
    }

    #[test]
    fn resolve_refuses_a_key_carried_by_two_files() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "k1.json", &json!({"name": "a", "local_key": "k1"}));
        write(tmp.path(), "b.json", &json!({"name": "b", "local_key": "k1"}));
        let err = resolve_registry_entry_in(tmp.path(), "k1").unwrap_err();
        assert!(err.contains("k1.json") && err.contains("b.json"), "{err}");
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
