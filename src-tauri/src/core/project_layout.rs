//! On-disk layout of a project's Automatic files.
//!
//! A project directory holds two files:
//!
//! - `.automatic.json` holds the project's configuration: what the project
//!   uses and how it syncs. It is meant to be committed, so it holds nothing
//!   specific to one machine, and every write sorts its keys at every level.
//! - `.automatic/project.json` holds this machine's state: the directory path,
//!   timestamps, the hashes drift detection compares against, and snapshots of
//!   this machine's registries. The managed `.gitignore` block ignores
//!   `.automatic/`, so this file is never committed.
//!
//! Silent mode leaves the project tree untouched, so a silent project keeps its
//! config at `.automatic/silent/.automatic.json` instead.
//!
//! Before VEL-115 both halves lived in `.automatic/project.json`. That legacy
//! file is still read as a whole. The next write copies it once to
//! `.automatic/project.legacy.json`, then splits it.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::{Project, ProjectMode};

/// File name of the committed project configuration.
pub const CONFIG_FILE_NAME: &str = ".automatic.json";

/// A project's top-level fields as JSON, keyed by field name.
type ProjectFields = Map<String, Value>;

/// Top-level `Project` fields stored in the state file. Every other persisted
/// field is configuration.
const STATE_FIELDS: &[&str] = &[
    "directory",
    // Names this checkout on this machine. Committing it would give every
    // clone the same key, and a copied folder could not be told apart from
    // the original. The project `id` stays in the config because it must
    // travel with the repo.
    "local_key",
    "created_at",
    "updated_at",
    "last_activity",
    "created_by",
    "instruction_file_hashes",
    // Groups live only in this machine's `~/.automatic/groups/`. If this record
    // were committed, a machine without the group would treat it as departed
    // on its next reconcile and strip the group's contexts from the config.
    "group_context_contributions",
    // Snapshots of this machine's registries. Nothing reads them yet. They
    // hold machine-specific command paths and full rule bodies, which would
    // churn a committed file on every library edit.
    "mcp_server_specs",
    "resolved_rules",
    "resolved_agents",
    "resolved_commands",
];

/// Runtime flags that neither file stores.
const TRANSIENT_FIELDS: &[&str] = &["directory_missing"];

fn is_state_field(key: &str) -> bool {
    STATE_FIELDS.contains(&key)
}

fn automatic_dir(directory: &str) -> PathBuf {
    PathBuf::from(directory).join(".automatic")
}

/// Path of the state file: `<directory>/.automatic/project.json`. A
/// pre-VEL-115 project keeps its whole config in this same file.
pub fn project_state_path(directory: &str) -> PathBuf {
    automatic_dir(directory).join("project.json")
}

/// Path of the one-time copy of a legacy combined `project.json`, taken
/// before the first split. Lives in the gitignored `.automatic/`.
pub fn project_legacy_backup_path(directory: &str) -> PathBuf {
    automatic_dir(directory).join("project.legacy.json")
}

/// Path the config file is written to for `mode`.
fn config_path_for_mode(directory: &str, mode: &ProjectMode) -> PathBuf {
    match mode {
        ProjectMode::Normal => PathBuf::from(directory).join(CONFIG_FILE_NAME),
        ProjectMode::Silent => automatic_dir(directory)
            .join("silent")
            .join(CONFIG_FILE_NAME),
    }
}

/// Both places a config file can live, normal location first.
fn config_paths(directory: &str) -> [PathBuf; 2] {
    [
        config_path_for_mode(directory, &ProjectMode::Normal),
        config_path_for_mode(directory, &ProjectMode::Silent),
    ]
}

/// The config file on disk, checking the normal location before the silent one.
fn existing_config_path(directory: &str) -> Option<PathBuf> {
    config_paths(directory).into_iter().find(|p| p.is_file())
}

/// The file that holds a project's configuration fields: the config file, or
/// a legacy combined `project.json`. `None` when the directory holds neither.
/// Raw JSON edits of configuration fields (rule renames, tool scrubs) target
/// this file.
pub fn project_config_source_path(directory: &str) -> Option<PathBuf> {
    existing_config_path(directory).or_else(|| {
        let state = project_state_path(directory);
        state.is_file().then_some(state)
    })
}

/// Whether `directory` holds any Automatic project file.
pub fn has_project_files(directory: &str) -> bool {
    project_config_source_path(directory).is_some()
}

fn read_json_object(path: &Path) -> Result<ProjectFields, String> {
    let raw = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    match serde_json::from_str::<Value>(&raw) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err(format!("{} does not contain a JSON object", path.display())),
        Err(e) => Err(format!("Failed to parse {}: {}", path.display(), e)),
    }
}

/// Combine the two files' fields into one project object.
///
/// With a config file, configuration comes only from it and state only from
/// the state file, so a stale legacy `project.json` cannot override a newer
/// committed config. Without one, the state file is read whole, which is how a
/// legacy combined file loads.
fn merge_project_fields(
    config: Option<ProjectFields>,
    state: Option<ProjectFields>,
) -> ProjectFields {
    let Some(config) = config else {
        return state.unwrap_or_default();
    };
    let mut merged: ProjectFields = config
        .into_iter()
        .filter(|(key, _)| !is_state_field(key))
        .collect();
    for (key, value) in state.unwrap_or_default() {
        if is_state_field(&key) {
            merged.insert(key, value);
        }
    }
    merged
}

/// Read the project stored in `directory`. Returns `Ok(None)` when the
/// directory holds no project files.
///
/// `directory` is always taken from the argument, so a fresh clone with no
/// state file still knows where it lives. `fallback_name` fills `name` when no
/// file provides one (for example, the config file was deleted).
///
/// A file that exists but cannot be read or parsed is an error. Falling back
/// to other data would let the next write replace the file, and a committed
/// config holding merge-conflict markers is exactly that case.
pub fn read_project_files(
    directory: &str,
    fallback_name: &str,
) -> Result<Option<Project>, String> {
    let config = existing_config_path(directory)
        .map(|path| read_json_object(&path))
        .transpose()?;
    let state_path = project_state_path(directory);
    let state = if state_path.is_file() {
        Some(read_json_object(&state_path)?)
    } else {
        None
    };
    if config.is_none() && state.is_none() {
        return Ok(None);
    }

    let mut fields = merge_project_fields(config, state);
    fields.insert("directory".into(), Value::String(directory.to_string()));
    if !fields.contains_key("name") {
        fields.insert("name".into(), Value::String(fallback_name.to_string()));
    }
    serde_json::from_value(Value::Object(fields))
        .map(Some)
        .map_err(|e| format!("Invalid project config in {}: {}", directory, e))
}

/// Split a project into its configuration and state fields.
pub fn split_project(project: &Project) -> Result<(ProjectFields, ProjectFields), String> {
    let Value::Object(fields) = serde_json::to_value(project)
        .map_err(|e| format!("Failed to serialize project '{}': {}", project.name, e))?
    else {
        return Err(format!(
            "Project '{}' did not serialize to a JSON object",
            project.name
        ));
    };

    let mut config = Map::new();
    let mut state = Map::new();
    for (key, value) in fields {
        if TRANSIENT_FIELDS.contains(&key.as_str()) {
            continue;
        }
        if is_state_field(&key) {
            state.insert(key, value);
        } else {
            config.insert(key, value);
        }
    }
    Ok((config, state))
}

/// Copy of `value` with every object's keys in sorted order.
///
/// `serde_json` keeps insertion order when any crate in the build enables its
/// `preserve_order` feature, and sorts otherwise. Sorting here makes the
/// written order independent of that feature, so a dependency change cannot
/// reorder every committed config.
fn sort_keys(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key.clone(), sort_keys(value)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.iter().map(sort_keys).collect()),
        other => other.clone(),
    }
}

/// Write `value` as pretty JSON with sorted keys and a trailing newline.
/// Skips the write when the file already holds exactly that text, so saving an
/// unchanged project leaves a committed config's mtime and git status alone.
/// Creates missing parent directories.
pub fn write_json_file(path: &Path, value: &Value) -> Result<(), String> {
    let mut text = serde_json::to_string_pretty(&sort_keys(value))
        .map_err(|e| format!("Failed to serialize {}: {}", path.display(), e))?;
    text.push('\n');
    if fs::read_to_string(path).is_ok_and(|existing| existing == text) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
    }
    fs::write(path, text).map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}

/// Whether `project.json` in `directory` is a pre-VEL-115 file that the next
/// write would split. That is the case when no config file exists yet and the
/// state file holds anything besides state: config fields, or content that
/// does not parse, which a write would otherwise replace.
fn needs_legacy_backup(directory: &str) -> bool {
    let state_path = project_state_path(directory);
    if existing_config_path(directory).is_some() || !state_path.is_file() {
        return false;
    }
    match read_json_object(&state_path) {
        Ok(fields) => fields
            .keys()
            .any(|key| !is_state_field(key) && !TRANSIENT_FIELDS.contains(&key.as_str())),
        Err(_) => true,
    }
}

/// Copy a legacy combined `project.json` to `project.legacy.json`, byte for
/// byte, before the split replaces it. An existing backup is never
/// overwritten, so it always holds the file as it was before migration.
fn back_up_legacy_file(directory: &str) -> Result<(), String> {
    let backup = project_legacy_backup_path(directory);
    if backup.exists() || !needs_legacy_backup(directory) {
        return Ok(());
    }
    let source = project_state_path(directory);
    fs::copy(&source, &backup).map(|_| ()).map_err(|e| {
        format!(
            "Failed to back up {} to {} before splitting it: {}",
            source.display(),
            backup.display(),
            e
        )
    })
}

fn remove_file_if_present(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Failed to delete {}: {}", path.display(), e)),
    }
}

/// Write `project` into its directory as a config file and a state file.
///
/// The config file goes to the location for the project's mode. A copy left
/// at the other location by an earlier mode is removed, so switching modes
/// moves the file. The first write over a legacy combined `project.json`
/// backs it up first and fails, writing nothing, if the backup fails.
pub fn write_project_files(project: &Project) -> Result<(), String> {
    let directory = project.directory.as_str();
    if directory.is_empty() {
        return Err(format!(
            "Project '{}' has no directory to write its config to",
            project.name
        ));
    }

    back_up_legacy_file(directory)?;
    let (config, state) = split_project(project)?;
    let config_path = config_path_for_mode(directory, &project.mode);
    write_json_file(&config_path, &Value::Object(config))?;
    for other in config_paths(directory) {
        if other != config_path {
            remove_file_if_present(&other)?;
        }
    }
    write_json_file(&project_state_path(directory), &Value::Object(state))
}

/// Delete the config file from either location, the state file and any
/// legacy backup, then remove `.automatic/` if that left it empty. Succeeds
/// when the files are already gone.
pub fn remove_project_files(directory: &str) -> Result<(), String> {
    for path in config_paths(directory) {
        remove_file_if_present(&path)?;
    }
    remove_file_if_present(&project_state_path(directory))?;
    remove_file_if_present(&project_legacy_backup_path(directory))?;

    let dir = automatic_dir(directory);
    if fs::read_dir(&dir).is_ok_and(|mut entries| entries.next().is_none()) {
        fs::remove_dir(&dir).map_err(|e| format!("Failed to delete {}: {}", dir.display(), e))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn project_in(dir: &Path) -> Project {
        Project {
            name: "demo".into(),
            description: "A demo".into(),
            directory: dir.to_str().unwrap().to_string(),
            skills: vec!["b-skill".into(), "a-skill".into()],
            agents: vec!["claude".into()],
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-02T00:00:00Z".into(),
            last_activity: Some("2026-01-03T00:00:00Z".into()),
            created_by: Some("user_123".into()),
            instruction_file_hashes: BTreeMap::from([
                ("CLAUDE.md".into(), "abc".into()),
                ("AGENTS.md".into(), "def".into()),
            ]),
            file_rules: BTreeMap::from([
                ("_project".into(), vec!["rule-b".into(), "rule-a".into()]),
                ("CLAUDE.md".into(), vec!["rule-c".into()]),
            ]),
            group_context_contributions: BTreeMap::from([(
                "Group".into(),
                vec!["ctx".into()],
            )]),
            contexts: vec!["ctx".into()],
            ..Default::default()
        }
    }

    fn read_object(path: &Path) -> Map<String, Value> {
        read_json_object(path).expect("read json object")
    }

    #[test]
    fn write_splits_config_and_state() {
        let tmp = tempfile::tempdir().unwrap();
        write_project_files(&project_in(tmp.path())).expect("write");

        let config = read_object(&tmp.path().join(CONFIG_FILE_NAME));
        let state = read_object(&tmp.path().join(".automatic").join("project.json"));

        for key in ["name", "description", "skills", "agents", "file_rules", "contexts"] {
            assert!(config.contains_key(key), "config is missing {key}");
            assert!(!state.contains_key(key), "state must not hold {key}");
        }
        for key in [
            "directory",
            "created_at",
            "updated_at",
            "last_activity",
            "created_by",
            "instruction_file_hashes",
            "group_context_contributions",
        ] {
            assert!(state.contains_key(key), "state is missing {key}");
            assert!(!config.contains_key(key), "config must not hold {key}");
        }
    }

    #[test]
    fn config_output_is_byte_identical_across_writes() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join(CONFIG_FILE_NAME);
        let mut project = project_in(tmp.path());

        write_project_files(&project).expect("first write");
        let first = fs::read_to_string(&config_path).unwrap();

        // Rebuild the maps in a different insertion order. A HashMap-backed
        // field would be free to serialize these in a different order.
        project.file_rules = BTreeMap::from([
            ("CLAUDE.md".into(), vec!["rule-c".into()]),
            ("_project".into(), vec!["rule-b".into(), "rule-a".into()]),
        ]);
        project.updated_at = "2026-02-01T00:00:00Z".into();
        write_project_files(&project).expect("second write");
        let second = fs::read_to_string(&config_path).unwrap();

        assert_eq!(first, second, "state-only changes must not touch the config file");
        assert!(first.ends_with('\n'));
        assert!(
            first.find("\"CLAUDE.md\"").unwrap() < first.find("\"_project\"").unwrap(),
            "map keys are written in sorted order"
        );
        let top_level: Vec<usize> = ["\"agents\"", "\"description\"", "\"name\"", "\"skills\""]
            .iter()
            .map(|key| first.find(key).unwrap())
            .collect();
        assert!(
            top_level.windows(2).all(|w| w[0] < w[1]),
            "top-level keys are written in sorted order"
        );
    }

    #[test]
    fn unchanged_config_is_not_rewritten() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join(CONFIG_FILE_NAME);
        let project = project_in(tmp.path());
        write_project_files(&project).expect("first write");

        // Make the file read-only: a second write of the same content must
        // skip the write rather than fail on it.
        let mut perms = fs::metadata(&config_path).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&config_path, perms.clone()).unwrap();
        let result = write_project_files(&project);
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        fs::set_permissions(&config_path, perms).unwrap();

        result.expect("an unchanged config must not be rewritten");
    }

    #[test]
    fn read_merges_both_files() {
        let tmp = tempfile::tempdir().unwrap();
        let original = project_in(tmp.path());
        write_project_files(&original).expect("write");

        let loaded = read_project_files(tmp.path().to_str().unwrap(), "fallback")
            .expect("read")
            .expect("project present");
        assert_eq!(loaded.name, "demo");
        assert_eq!(loaded.skills, original.skills);
        assert_eq!(loaded.file_rules, original.file_rules);
        assert_eq!(loaded.instruction_file_hashes, original.instruction_file_hashes);
        assert_eq!(loaded.last_activity, original.last_activity);
        assert_eq!(loaded.group_context_contributions, original.group_context_contributions);
    }

    #[test]
    fn read_without_state_file_uses_argument_directory() {
        // A fresh clone has the committed config but no state file.
        let tmp = tempfile::tempdir().unwrap();
        write_project_files(&project_in(tmp.path())).expect("write");
        fs::remove_file(tmp.path().join(".automatic").join("project.json")).unwrap();

        let dir = tmp.path().to_str().unwrap();
        let loaded = read_project_files(dir, "fallback").unwrap().unwrap();
        assert_eq!(loaded.name, "demo");
        assert_eq!(loaded.directory, dir);
        assert!(loaded.instruction_file_hashes.is_empty());
    }

    #[test]
    fn read_returns_none_without_files() {
        let tmp = tempfile::tempdir().unwrap();
        let loaded = read_project_files(tmp.path().to_str().unwrap(), "x").unwrap();
        assert!(loaded.is_none());
        assert!(!has_project_files(tmp.path().to_str().unwrap()));
    }

    #[test]
    fn malformed_config_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join(CONFIG_FILE_NAME),
            "<<<<<<< HEAD\n{}\n=======\n{}\n>>>>>>> branch\n",
        )
        .unwrap();

        let err = read_project_files(tmp.path().to_str().unwrap(), "x")
            .expect_err("a conflicted config must not load");
        assert!(err.contains(CONFIG_FILE_NAME), "error names the file: {err}");
    }

    #[test]
    fn legacy_combined_file_loads_and_is_split_on_write() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap();
        let original = project_in(tmp.path());
        let legacy = serde_json::to_string_pretty(&original).unwrap();
        fs::create_dir_all(tmp.path().join(".automatic")).unwrap();
        fs::write(project_state_path(dir), legacy).unwrap();
        assert!(has_project_files(dir));
        assert_eq!(project_config_source_path(dir), Some(project_state_path(dir)));

        let loaded = read_project_files(dir, "fallback").unwrap().unwrap();
        assert_eq!(loaded.skills, original.skills);
        assert_eq!(loaded.file_rules, original.file_rules);

        write_project_files(&loaded).expect("write");
        let state = read_object(&project_state_path(dir));
        assert!(!state.contains_key("skills"), "config fields leave project.json");
        assert!(state.contains_key("instruction_file_hashes"));
        let config = read_object(&tmp.path().join(CONFIG_FILE_NAME));
        assert_eq!(config["skills"], serde_json::json!(["b-skill", "a-skill"]));
    }

    /// Write `project` as a pre-VEL-115 combined file and return its bytes.
    fn write_legacy(dir: &str, project: &Project) -> Vec<u8> {
        let legacy = serde_json::to_string_pretty(project).unwrap();
        fs::create_dir_all(automatic_dir(dir)).unwrap();
        fs::write(project_state_path(dir), &legacy).unwrap();
        legacy.into_bytes()
    }

    #[test]
    fn split_backs_up_legacy_file_byte_for_byte() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap();
        let original = write_legacy(dir, &project_in(tmp.path()));

        let loaded = read_project_files(dir, "fallback").unwrap().unwrap();
        write_project_files(&loaded).expect("write");

        let backup = fs::read(project_legacy_backup_path(dir)).expect("backup exists");
        assert_eq!(backup, original, "backup matches the pre-split file exactly");
    }

    #[test]
    fn legacy_backup_is_never_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap();
        let original = write_legacy(dir, &project_in(tmp.path()));

        let mut loaded = read_project_files(dir, "fallback").unwrap().unwrap();
        write_project_files(&loaded).expect("first write");
        loaded.skills.push("later-skill".into());
        write_project_files(&loaded).expect("second write");

        // A second legacy file appearing later (for example, restored from an
        // old copy) must not replace the first backup either.
        fs::remove_file(tmp.path().join(CONFIG_FILE_NAME)).unwrap();
        let mut other = project_in(tmp.path());
        other.skills = vec!["other".into()];
        write_legacy(dir, &other);
        write_project_files(&other).expect("third write");

        let backup = fs::read(project_legacy_backup_path(dir)).unwrap();
        assert_eq!(backup, original);
    }

    #[test]
    fn no_backup_for_projects_already_split() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap();
        let project = project_in(tmp.path());
        write_project_files(&project).expect("first write");

        // A fresh clone: committed config, no state file.
        fs::remove_file(project_state_path(dir)).unwrap();
        write_project_files(&project).expect("second write");

        // State-only file with no config file (config deleted by hand).
        fs::remove_file(tmp.path().join(CONFIG_FILE_NAME)).unwrap();
        write_project_files(&project).expect("third write");

        assert!(!project_legacy_backup_path(dir).exists());
    }

    #[test]
    fn unparseable_legacy_file_is_backed_up_before_replacement() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap();
        fs::create_dir_all(automatic_dir(dir)).unwrap();
        fs::write(project_state_path(dir), "{ not json").unwrap();

        write_project_files(&project_in(tmp.path())).expect("write");

        assert_eq!(
            fs::read_to_string(project_legacy_backup_path(dir)).unwrap(),
            "{ not json"
        );
    }

    #[test]
    fn config_file_wins_over_stale_legacy_fields() {
        // Another machine committed a new config while this machine still has
        // a legacy combined project.json.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap();
        let mut stale = project_in(tmp.path());
        stale.skills = vec!["old-skill".into()];
        fs::create_dir_all(tmp.path().join(".automatic")).unwrap();
        fs::write(project_state_path(dir), serde_json::to_string(&stale).unwrap()).unwrap();
        fs::write(
            tmp.path().join(CONFIG_FILE_NAME),
            r#"{"name":"demo","skills":["new-skill"],"directory":"/elsewhere"}"#,
        )
        .unwrap();

        let loaded = read_project_files(dir, "fallback").unwrap().unwrap();
        assert_eq!(loaded.skills, vec!["new-skill".to_string()]);
        assert_eq!(loaded.directory, dir, "a directory in the config file is ignored");
        assert_eq!(loaded.instruction_file_hashes, stale.instruction_file_hashes);
    }

    #[test]
    fn silent_mode_writes_config_under_silent_dir_and_moves_on_switch() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap();
        let root_config = tmp.path().join(CONFIG_FILE_NAME);
        let silent_config = tmp.path().join(".automatic").join("silent").join(CONFIG_FILE_NAME);

        let mut project = project_in(tmp.path());
        write_project_files(&project).expect("normal write");
        assert!(root_config.is_file());

        project.mode = ProjectMode::Silent;
        write_project_files(&project).expect("silent write");
        assert!(!root_config.exists(), "silent mode leaves the project root untouched");
        assert!(silent_config.is_file());
        let loaded = read_project_files(dir, "fallback").unwrap().unwrap();
        assert_eq!(loaded.mode, ProjectMode::Silent);

        project.mode = ProjectMode::Normal;
        write_project_files(&project).expect("back to normal");
        assert!(root_config.is_file());
        assert!(!silent_config.exists());
    }

    #[test]
    fn missing_name_uses_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap();
        fs::create_dir_all(tmp.path().join(".automatic")).unwrap();
        fs::write(project_state_path(dir), r#"{"created_at":"2026-01-01T00:00:00Z"}"#).unwrap();

        let loaded = read_project_files(dir, "from-registry").unwrap().unwrap();
        assert_eq!(loaded.name, "from-registry");
    }

    #[test]
    fn remove_deletes_every_file_and_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap();
        write_legacy(dir, &project_in(tmp.path()));
        write_project_files(&project_in(tmp.path())).expect("write");
        assert!(project_legacy_backup_path(dir).exists());

        remove_project_files(dir).expect("remove");
        assert!(!tmp.path().join(CONFIG_FILE_NAME).exists());
        assert!(!tmp.path().join(".automatic").exists());
        remove_project_files(dir).expect("idempotent");
    }
}
