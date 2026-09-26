use std::fs;
use std::path::PathBuf;

use super::types::ServerConfig;

/// Directory holding one JSON file per project, each containing that
/// project's list of configured dev servers. Self-contained under this
/// plugin's own module, mirroring `memory.rs`'s per-project storage.
fn dev_servers_dir() -> Result<PathBuf, String> {
    Ok(crate::core::get_automatic_dir()?.join("dev-servers"))
}

fn config_path(project: &str) -> Result<PathBuf, String> {
    if !crate::core::is_valid_name(project) {
        return Err("Invalid project name".into());
    }
    Ok(dev_servers_dir()?.join(format!("{}.json", project)))
}

/// Read all configured servers for a project. Returns an empty list if the
/// project has none configured yet.
pub fn list_configs(project: &str) -> Result<Vec<ServerConfig>, String> {
    let path = config_path(project)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| format!("Corrupt dev server config for '{}': {}", project, e))
}

fn write_configs(project: &str, configs: &[ServerConfig]) -> Result<(), String> {
    let path = config_path(project)?;
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let pretty = serde_json::to_string_pretty(configs).map_err(|e| e.to_string())?;
    fs::write(&path, pretty).map_err(|e| e.to_string())
}

/// Find a single server config by id.
pub fn find_config(project: &str, id: &str) -> Result<ServerConfig, String> {
    list_configs(project)?
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| format!("Dev server '{}' not found for project '{}'", id, project))
}

/// Create or update a server config. When `config.id` is empty, a new id is
/// generated and the config is appended; otherwise the existing entry with
/// a matching id is replaced.
pub fn save_config(project: &str, mut config: ServerConfig) -> Result<ServerConfig, String> {
    if config.name.trim().is_empty() {
        return Err("Server name cannot be empty".into());
    }
    if config.script.trim().is_empty() {
        return Err("Script cannot be empty".into());
    }

    let mut configs = list_configs(project)?;

    if config.id.is_empty() {
        config.id = uuid::Uuid::new_v4().to_string();
        config.created_at = chrono::Utc::now().to_rfc3339();
        configs.push(config.clone());
    } else if let Some(existing) = configs.iter_mut().find(|c| c.id == config.id) {
        config.created_at = existing.created_at.clone();
        *existing = config.clone();
    } else {
        return Err(format!("Dev server '{}' not found for project '{}'", config.id, project));
    }

    write_configs(project, &configs)?;
    Ok(config)
}

/// Remove a server config. Does not stop a running process — callers must
/// stop the server first if it is running.
pub fn delete_config(project: &str, id: &str) -> Result<(), String> {
    let mut configs = list_configs(project)?;
    let before = configs.len();
    configs.retain(|c| c.id != id);
    if configs.len() == before {
        return Err(format!("Dev server '{}' not found for project '{}'", id, project));
    }
    write_configs(project, &configs)
}

/// Project names that own a dev-server registry file, read from the file
/// stems in the registry directory. Read-only: this does not check whether
/// each project still exists. Pair it with `classify_config_projects` to
/// separate live owners from orphans.
///
/// Stems that are not valid project names are skipped. No project can own
/// them, and `config_path` would refuse to read them.
pub fn list_config_projects() -> Result<Vec<String>, String> {
    let dir = dev_servers_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in fs::read_dir(&dir)
        .map_err(|e| format!("Could not read dev-server registry '{}': {}", dir.display(), e))?
        .flatten()
    {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if crate::core::is_valid_name(stem) {
            names.push(stem.to_string());
        }
    }
    names.sort();
    Ok(names)
}

/// Registry file owners split by whether their project still exists.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ConfigOwners {
    /// Owners that match a registered project, as the registered name.
    pub live: Vec<String>,
    /// Owners with no registered project. Their files are orphans (VEL-160).
    pub orphans: Vec<String>,
}

/// Split registry file owners into live projects and orphans. Pure.
///
/// Matching ignores case. On macOS the default filesystem is
/// case-insensitive, so `Foo.json` still serves project `foo` after a
/// case-only rename. Treating it as an orphan would delete a live
/// project's servers. A live match is reported under the registered name
/// so the Servers view links rows to the right project.
pub fn classify_config_projects(config_projects: &[String], live_projects: &[String]) -> ConfigOwners {
    let mut owners = ConfigOwners::default();
    for name in config_projects {
        let registered = live_projects
            .iter()
            .find(|p| *p == name)
            .or_else(|| live_projects.iter().find(|p| p.eq_ignore_ascii_case(name)));
        match registered {
            Some(project) => {
                if !owners.live.contains(project) {
                    owners.live.push(project.clone());
                }
            }
            None => owners.orphans.push(name.clone()),
        }
    }
    owners
}

/// Delete the registry file of each orphaned owner and log each removal.
/// Callers must pass only names that `classify_config_projects` reported as
/// orphans. Keeps going past a failed delete so one bad file does not block
/// the rest. Returns the names whose files were removed.
pub fn remove_orphaned_configs(orphans: &[String]) -> Vec<String> {
    let mut removed = Vec::new();
    for name in orphans {
        let path = match config_path(name) {
            Ok(path) => path,
            Err(e) => {
                eprintln!("[dev-servers] skipped orphaned config '{}': {}", name, e);
                continue;
            }
        };
        if !path.exists() {
            continue;
        }
        match fs::remove_file(&path) {
            Ok(()) => {
                eprintln!(
                    "[dev-servers] removed orphaned config for missing project '{}': {}",
                    name,
                    path.display()
                );
                removed.push(name.clone());
            }
            Err(e) => eprintln!(
                "[dev-servers] could not remove orphaned config '{}': {}",
                path.display(),
                e
            ),
        }
    }
    removed
}

/// Rename this project's dev-server registry file from `old` to `new`.
/// No-op when the source file does not exist (the common case — most
/// projects never configure a dev server). Called from the project
/// rename command so the global "Servers" view stays anchored to the
/// live project name.
///
/// A case-only rename (`Foo` to `foo`) on a case-insensitive filesystem
/// resolves both paths to the same file. That is not a clash, so the file
/// is renamed in place to pick up the new casing.
pub fn rename_project(old: &str, new: &str) -> Result<(), String> {
    if old == new {
        return Ok(());
    }
    if !crate::core::is_valid_name(old) || !crate::core::is_valid_name(new) {
        return Err("Invalid project name".into());
    }
    let dir = dev_servers_dir()?;
    let old_path = dir.join(format!("{}.json", old));
    if !old_path.exists() {
        return Ok(());
    }
    let new_path = dir.join(format!("{}.json", new));
    if new_path.exists() && !same_file(&old_path, &new_path) {
        return Err(format!(
            "Dev server config for '{}' already exists",
            new
        ));
    }
    fs::rename(&old_path, &new_path).map_err(|e| {
        format!(
            "Could not rename dev-server config '{}' to '{}': {}",
            old_path.display(),
            new_path.display(),
            e
        )
    })
}

fn same_file(a: &std::path::Path, b: &std::path::Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Remove this project's dev-server registry file. No-op if not present.
/// Called from the project delete command so no orphan is left behind.
pub fn remove_project(project: &str) -> Result<(), String> {
    if !crate::core::is_valid_name(project) {
        return Err("Invalid project name".into());
    }
    let dir = dev_servers_dir()?;
    let path = dir.join(format!("{}.json", project));
    if !path.exists() {
        return Ok(());
    }
    fs::remove_file(&path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::with_test_home;
    use tempfile::TempDir;

    fn tmp() -> TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn sample() -> ServerConfig {
        ServerConfig {
            id: String::new(),
            name: "web".into(),
            package_manager: super::super::types::PackageManager::Npm,
            script: "dev".into(),
            subdirectory: String::new(),
            port: Some(3000),
            created_at: String::new(),
        }
    }

    #[test]
    fn save_creates_new_config_with_generated_id() {
        let tmp = tmp();
        with_test_home(tmp.path().to_path_buf(), || {
            let saved = save_config("demo", sample()).unwrap();
            assert!(!saved.id.is_empty());
            assert_eq!(saved.name, "web");

            let listed = list_configs("demo").unwrap();
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0].id, saved.id);
        });
    }

    #[test]
    fn save_updates_existing_config_by_id() {
        let tmp = tmp();
        with_test_home(tmp.path().to_path_buf(), || {
            let saved = save_config("demo", sample()).unwrap();

            let mut updated = saved.clone();
            updated.script = "start".into();
            let resaved = save_config("demo", updated).unwrap();

            let listed = list_configs("demo").unwrap();
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0].script, "start");
            assert_eq!(listed[0].created_at, saved.created_at);
            assert_eq!(resaved.created_at, saved.created_at);
        });
    }

    #[test]
    fn delete_removes_config() {
        let tmp = tmp();
        with_test_home(tmp.path().to_path_buf(), || {
            let saved = save_config("demo", sample()).unwrap();
            delete_config("demo", &saved.id).unwrap();
            assert!(list_configs("demo").unwrap().is_empty());
        });
    }

    #[test]
    fn delete_missing_config_errors() {
        let tmp = tmp();
        with_test_home(tmp.path().to_path_buf(), || {
            let err = delete_config("demo", "missing").unwrap_err();
            assert!(err.contains("not found"));
        });
    }

    /// Register `name` as a project so `crate::core::list_projects` sees
    /// it. Empty-directory registry entries are the simplest possible
    /// project record and match how a wizard-in-progress project looks
    /// on disk before its directory is picked.
    fn register_project(name: &str) {
        let raw = format!("{{\"name\":\"{}\"}}", name);
        crate::core::save_project(name, &raw).expect("register project");
    }

    #[test]
    fn list_config_projects_reads_file_stems() {
        let tmp = tmp();
        with_test_home(tmp.path().to_path_buf(), || {
            save_config("demo-b", sample()).unwrap();
            save_config("demo-a", sample()).unwrap();

            let names = list_config_projects().unwrap();
            assert_eq!(names, vec!["demo-a".to_string(), "demo-b".to_string()]);
        });
    }

    #[test]
    fn list_config_projects_does_not_delete_anything() {
        let tmp = tmp();
        with_test_home(tmp.path().to_path_buf(), || {
            save_config("ghost-project", sample()).unwrap();
            list_config_projects().unwrap();
            assert!(dev_servers_dir().unwrap().join("ghost-project.json").exists());
        });
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn classify_separates_live_owners_from_orphans() {
        // VEL-160: a file left behind by a rename or delete has no project.
        let owners = classify_config_projects(
            &names(&["ghost-project", "live-project"]),
            &names(&["live-project", "no-servers"]),
        );
        assert_eq!(owners.live, names(&["live-project"]));
        assert_eq!(owners.orphans, names(&["ghost-project"]));
    }

    #[test]
    fn classify_matches_a_case_only_rename_to_the_live_project() {
        // `Demo.json` still serves project `demo` on a case-insensitive
        // filesystem. It must not be classed as an orphan and deleted.
        let owners = classify_config_projects(&names(&["Demo"]), &names(&["demo"]));
        assert_eq!(owners.live, names(&["demo"]));
        assert!(owners.orphans.is_empty());
    }

    #[test]
    fn classify_prefers_an_exact_match_over_a_case_insensitive_one() {
        let owners = classify_config_projects(&names(&["demo"]), &names(&["Demo", "demo"]));
        assert_eq!(owners.live, names(&["demo"]));
    }

    #[test]
    fn classify_treats_everything_as_orphaned_when_no_projects_exist() {
        let owners = classify_config_projects(&names(&["a", "b"]), &[]);
        assert!(owners.live.is_empty());
        assert_eq!(owners.orphans, names(&["a", "b"]));
    }

    #[test]
    fn remove_orphaned_configs_deletes_only_the_named_files() {
        let tmp = tmp();
        with_test_home(tmp.path().to_path_buf(), || {
            register_project("live-project");
            save_config("live-project", sample()).unwrap();
            save_config("ghost-project", sample()).unwrap();

            let removed = remove_orphaned_configs(&names(&["ghost-project", "never-existed"]));
            assert_eq!(removed, names(&["ghost-project"]));

            let dir = dev_servers_dir().unwrap();
            assert!(!dir.join("ghost-project.json").exists());
            assert!(dir.join("live-project.json").exists());
        });
    }

    #[test]
    fn rename_project_moves_config_file() {
        let tmp = tmp();
        with_test_home(tmp.path().to_path_buf(), || {
            save_config("old", sample()).unwrap();
            rename_project("old", "renamed").unwrap();

            let dir = dev_servers_dir().unwrap();
            assert!(!dir.join("old.json").exists());
            assert!(dir.join("renamed.json").exists());

            let listed = list_configs("renamed").unwrap();
            assert_eq!(listed.len(), 1);
        });
    }

    #[test]
    fn rename_project_is_noop_when_source_missing() {
        let tmp = tmp();
        with_test_home(tmp.path().to_path_buf(), || {
            // Most projects never touch this plugin. A rename must still
            // succeed for them or the whole rename_project command fails.
            rename_project("never-configured", "renamed").unwrap();
        });
    }

    #[test]
    fn rename_project_refuses_to_overwrite_existing_target() {
        let tmp = tmp();
        with_test_home(tmp.path().to_path_buf(), || {
            save_config("a", sample()).unwrap();
            save_config("b", sample()).unwrap();
            let err = rename_project("a", "b").unwrap_err();
            assert!(err.contains("already exists"), "unexpected error: {err}");
        });
    }

    #[test]
    fn rename_project_handles_a_case_only_rename() {
        // On macOS `Demo.json` and `demo.json` are the same file. Treating
        // that as a clash left `Demo.json` in place under the old name.
        let tmp = tmp();
        with_test_home(tmp.path().to_path_buf(), || {
            save_config("Demo", sample()).unwrap();
            rename_project("Demo", "demo").unwrap();

            assert_eq!(list_config_projects().unwrap(), names(&["demo"]));
            assert_eq!(list_configs("demo").unwrap().len(), 1);
        });
    }

    #[test]
    fn remove_project_deletes_config_file() {
        let tmp = tmp();
        with_test_home(tmp.path().to_path_buf(), || {
            save_config("goner", sample()).unwrap();
            remove_project("goner").unwrap();

            let dir = dev_servers_dir().unwrap();
            assert!(!dir.join("goner.json").exists());
        });
    }

    #[test]
    fn remove_project_is_noop_when_file_missing() {
        let tmp = tmp();
        with_test_home(tmp.path().to_path_buf(), || {
            remove_project("never-configured").unwrap();
        });
    }
}
