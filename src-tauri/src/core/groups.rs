use std::fs;
use std::path::PathBuf;

use super::*;

// ── Project Groups ────────────────────────────────────────────────────────────
//
// Group configs are stored as individual JSON files at:
//   ~/.automatic/groups/{name}.json
//
// Each file contains a full `ProjectGroup` value.  The group name is the
// file stem; it must pass `is_valid_name`.

fn group_path(groups_dir: &PathBuf, name: &str) -> PathBuf {
    groups_dir.join(format!("{}.json", name))
}

pub fn list_groups() -> Result<Vec<String>, String> {
    let groups_dir = get_groups_dir()?;

    if !groups_dir.exists() {
        return Ok(Vec::new());
    }

    let mut groups = Vec::new();
    let entries = fs::read_dir(&groups_dir).map_err(|e| e.to_string())?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if is_valid_name(stem) {
                    groups.push(stem.to_string());
                }
            }
        }
    }

    groups.sort();
    Ok(groups)
}

pub fn read_group(name: &str) -> Result<String, String> {
    if !is_valid_name(name) {
        return Err("Invalid group name".into());
    }
    let groups_dir = get_groups_dir()?;
    let path = group_path(&groups_dir, name);

    if !path.exists() {
        return Err(format!("Group '{}' not found", name));
    }

    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    // Round-trip through the struct to ensure forward-compatibility: unknown
    // fields are silently dropped and defaults are applied.
    let group = serde_json::from_str::<ProjectGroup>(&raw).unwrap_or_else(|_| ProjectGroup {
        name: name.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
        ..Default::default()
    });
    serde_json::to_string_pretty(&group).map_err(|e| e.to_string())
}

pub fn save_group(name: &str, data: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid group name".into());
    }

    let group: ProjectGroup =
        serde_json::from_str(data).map_err(|e| format!("Invalid group data: {}", e))?;
    let pretty = serde_json::to_string_pretty(&group).map_err(|e| e.to_string())?;

    let groups_dir = get_groups_dir()?;
    if !groups_dir.exists() {
        fs::create_dir_all(&groups_dir).map_err(|e| e.to_string())?;
    }

    fs::write(group_path(&groups_dir, name), &pretty).map_err(|e| e.to_string())
}

pub fn delete_group(name: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid group name".into());
    }
    let groups_dir = get_groups_dir()?;
    let path = group_path(&groups_dir, name);

    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Return all groups that contain the given project name.
pub fn groups_for_project(project_name: &str) -> Vec<ProjectGroup> {
    let names = match list_groups() {
        Ok(n) => n,
        Err(_) => return Vec::new(),
    };

    let mut result = Vec::new();
    for name in names {
        if let Ok(raw) = read_group(&name) {
            if let Ok(group) = serde_json::from_str::<ProjectGroup>(&raw) {
                if group.projects.iter().any(|p| p == project_name) {
                    result.push(group);
                }
            }
        }
    }
    result
}

/// Remove the project name from every group's `projects` list and persist
/// the changes. Returns the names of groups that were modified.
///
/// Per-group failures are logged and skipped rather than aborting the whole
/// pass — callers (typically `delete_project`) treat this as best-effort
/// cleanup since the project file is already gone.
pub fn remove_project_from_all_groups(project_name: &str) -> Result<Vec<String>, String> {
    let groups_dir = get_groups_dir()?;
    remove_project_from_all_groups_in_dir(&groups_dir, project_name)
}

/// Replace `old_name` with `new_name` in every group's `projects` list. If a
/// group already contains `new_name`, the stale `old_name` entry is dropped
/// (deduplication) rather than producing a duplicate. Returns the names of
/// groups that were modified.
pub fn rename_project_in_all_groups(
    old_name: &str,
    new_name: &str,
) -> Result<Vec<String>, String> {
    let groups_dir = get_groups_dir()?;
    rename_project_in_all_groups_in_dir(&groups_dir, old_name, new_name)
}

/// Drop every reference to a project name that does not appear in
/// `live_projects` from any group's `projects` list. Returns the names of
/// groups that were modified.
///
/// Intended as a one-shot startup migration to heal pre-existing stale
/// references left over from before `delete_project` and `rename_project`
/// started cleaning up their own group entries. Idempotent.
pub fn scrub_orphan_project_references(
    live_projects: &[String],
) -> Result<Vec<String>, String> {
    let groups_dir = get_groups_dir()?;
    scrub_orphan_project_references_in_dir(&groups_dir, live_projects)
}

// ── Path-injectable internals (used by the public API and tests) ──────────────

fn read_group_from_dir(groups_dir: &PathBuf, name: &str) -> Result<ProjectGroup, String> {
    let path = group_path(groups_dir, name);
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str::<ProjectGroup>(&raw).map_err(|e| e.to_string())
}

fn write_group_to_dir(groups_dir: &PathBuf, group: &ProjectGroup) -> Result<(), String> {
    if !groups_dir.exists() {
        fs::create_dir_all(groups_dir).map_err(|e| e.to_string())?;
    }
    let pretty = serde_json::to_string_pretty(group).map_err(|e| e.to_string())?;
    fs::write(group_path(groups_dir, &group.name), &pretty).map_err(|e| e.to_string())
}

fn list_group_names_in_dir(groups_dir: &PathBuf) -> Result<Vec<String>, String> {
    if !groups_dir.exists() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    let entries = fs::read_dir(groups_dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if is_valid_name(stem) {
                    names.push(stem.to_string());
                }
            }
        }
    }
    names.sort();
    Ok(names)
}

fn remove_project_from_all_groups_in_dir(
    groups_dir: &PathBuf,
    project_name: &str,
) -> Result<Vec<String>, String> {
    let names = list_group_names_in_dir(groups_dir)?;
    let mut affected = Vec::new();
    for name in names {
        let mut group = match read_group_from_dir(groups_dir, &name) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("groups cleanup: skipping unreadable group '{}': {}", name, e);
                continue;
            }
        };
        let before = group.projects.len();
        group.projects.retain(|p| p != project_name);
        if group.projects.len() == before {
            continue;
        }
        group.updated_at = chrono::Utc::now().to_rfc3339();
        if let Err(e) = write_group_to_dir(groups_dir, &group) {
            eprintln!("groups cleanup: failed to save group '{}': {}", name, e);
            continue;
        }
        affected.push(name);
    }
    Ok(affected)
}

fn scrub_orphan_project_references_in_dir(
    groups_dir: &PathBuf,
    live_projects: &[String],
) -> Result<Vec<String>, String> {
    use std::collections::HashSet;
    let live: HashSet<&str> = live_projects.iter().map(|s| s.as_str()).collect();
    let names = list_group_names_in_dir(groups_dir)?;
    let mut affected = Vec::new();
    for name in names {
        let mut group = match read_group_from_dir(groups_dir, &name) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("groups scrub: skipping unreadable group '{}': {}", name, e);
                continue;
            }
        };
        let before = group.projects.len();
        group.projects.retain(|p| live.contains(p.as_str()));
        if group.projects.len() == before {
            continue;
        }
        group.updated_at = chrono::Utc::now().to_rfc3339();
        if let Err(e) = write_group_to_dir(groups_dir, &group) {
            eprintln!("groups scrub: failed to save group '{}': {}", name, e);
            continue;
        }
        affected.push(name);
    }
    Ok(affected)
}

fn rename_project_in_all_groups_in_dir(
    groups_dir: &PathBuf,
    old_name: &str,
    new_name: &str,
) -> Result<Vec<String>, String> {
    if old_name == new_name {
        return Ok(Vec::new());
    }
    let names = list_group_names_in_dir(groups_dir)?;
    let mut affected = Vec::new();
    for name in names {
        let mut group = match read_group_from_dir(groups_dir, &name) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("groups rename: skipping unreadable group '{}': {}", name, e);
                continue;
            }
        };
        if !group.projects.iter().any(|p| p == old_name) {
            continue;
        }
        let already_has_new = group.projects.iter().any(|p| p == new_name);
        if already_has_new {
            // Collision: drop the stale old_name to avoid duplicates.
            group.projects.retain(|p| p != old_name);
        } else {
            for project in group.projects.iter_mut() {
                if project == old_name {
                    *project = new_name.to_string();
                }
            }
        }
        group.updated_at = chrono::Utc::now().to_rfc3339();
        if let Err(e) = write_group_to_dir(groups_dir, &group) {
            eprintln!("groups rename: failed to save group '{}': {}", name, e);
            continue;
        }
        affected.push(name);
    }
    Ok(affected)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, PathBuf) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let groups_dir = tmp.path().join("groups");
        (tmp, groups_dir)
    }

    fn write_group(groups_dir: &PathBuf, name: &str, projects: &[&str]) {
        let group = ProjectGroup {
            name: name.to_string(),
            description: String::new(),
            projects: projects.iter().map(|p| p.to_string()).collect(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        write_group_to_dir(groups_dir, &group).expect("write group");
    }

    fn read_projects(groups_dir: &PathBuf, name: &str) -> Vec<String> {
        read_group_from_dir(groups_dir, name).expect("read").projects
    }

    // ── remove_project_from_all_groups ───────────────────────────────────────

    #[test]
    fn remove_strips_project_from_every_group_that_lists_it() {
        let (_tmp, groups_dir) = setup();
        write_group(&groups_dir, "foo", &["a", "b"]);
        write_group(&groups_dir, "bar", &["a"]);
        write_group(&groups_dir, "baz", &["b"]);

        let mut affected = remove_project_from_all_groups_in_dir(&groups_dir, "a").expect("clean");
        affected.sort();
        assert_eq!(affected, vec!["bar", "foo"]);
        assert_eq!(read_projects(&groups_dir, "foo"), vec!["b"]);
        assert!(read_projects(&groups_dir, "bar").is_empty());
        assert_eq!(read_projects(&groups_dir, "baz"), vec!["b"]);
    }

    #[test]
    fn remove_is_no_op_when_project_absent() {
        let (_tmp, groups_dir) = setup();
        write_group(&groups_dir, "foo", &["a"]);
        let affected = remove_project_from_all_groups_in_dir(&groups_dir, "ghost").expect("clean");
        assert!(affected.is_empty());
        assert_eq!(read_projects(&groups_dir, "foo"), vec!["a"]);
    }

    #[test]
    fn remove_handles_missing_groups_dir() {
        let (_tmp, groups_dir) = setup();
        let affected = remove_project_from_all_groups_in_dir(&groups_dir, "a").expect("clean");
        assert!(affected.is_empty());
    }

    // ── rename_project_in_all_groups ─────────────────────────────────────────

    #[test]
    fn rename_replaces_old_name_with_new() {
        let (_tmp, groups_dir) = setup();
        write_group(&groups_dir, "foo", &["a", "c"]);
        write_group(&groups_dir, "bar", &["a"]);

        let mut affected =
            rename_project_in_all_groups_in_dir(&groups_dir, "a", "b").expect("rename");
        affected.sort();
        assert_eq!(affected, vec!["bar", "foo"]);
        assert_eq!(read_projects(&groups_dir, "foo"), vec!["b", "c"]);
        assert_eq!(read_projects(&groups_dir, "bar"), vec!["b"]);
    }

    #[test]
    fn rename_drops_old_name_on_collision_to_avoid_duplicate() {
        let (_tmp, groups_dir) = setup();
        write_group(&groups_dir, "foo", &["a", "b"]);

        let affected = rename_project_in_all_groups_in_dir(&groups_dir, "a", "b").expect("rename");
        assert_eq!(affected, vec!["foo"]);
        assert_eq!(read_projects(&groups_dir, "foo"), vec!["b"]);
    }

    #[test]
    fn rename_is_no_op_when_old_name_absent() {
        let (_tmp, groups_dir) = setup();
        write_group(&groups_dir, "foo", &["a"]);
        let affected = rename_project_in_all_groups_in_dir(&groups_dir, "ghost", "z").expect("rename");
        assert!(affected.is_empty());
        assert_eq!(read_projects(&groups_dir, "foo"), vec!["a"]);
    }

    #[test]
    fn rename_is_no_op_when_old_equals_new() {
        let (_tmp, groups_dir) = setup();
        write_group(&groups_dir, "foo", &["a"]);
        let affected = rename_project_in_all_groups_in_dir(&groups_dir, "a", "a").expect("rename");
        assert!(affected.is_empty());
        assert_eq!(read_projects(&groups_dir, "foo"), vec!["a"]);
    }

    // ── scrub_orphan_project_references ──────────────────────────────────────

    #[test]
    fn scrub_drops_only_orphan_references() {
        let (_tmp, groups_dir) = setup();
        write_group(&groups_dir, "foo", &["a", "ghost", "b"]);
        write_group(&groups_dir, "bar", &["b"]);
        write_group(&groups_dir, "baz", &["only-ghosts"]);

        let live = vec!["a".to_string(), "b".to_string()];
        let mut affected =
            scrub_orphan_project_references_in_dir(&groups_dir, &live).expect("scrub");
        affected.sort();
        assert_eq!(affected, vec!["baz", "foo"]);
        assert_eq!(read_projects(&groups_dir, "foo"), vec!["a", "b"]);
        assert_eq!(read_projects(&groups_dir, "bar"), vec!["b"]);
        assert!(read_projects(&groups_dir, "baz").is_empty());
    }

    #[test]
    fn scrub_is_idempotent_when_no_orphans() {
        let (_tmp, groups_dir) = setup();
        write_group(&groups_dir, "foo", &["a"]);
        let live = vec!["a".to_string()];
        let affected = scrub_orphan_project_references_in_dir(&groups_dir, &live).expect("scrub");
        assert!(affected.is_empty());
        assert_eq!(read_projects(&groups_dir, "foo"), vec!["a"]);
    }

    #[test]
    fn scrub_handles_missing_groups_dir() {
        let (_tmp, groups_dir) = setup();
        let affected = scrub_orphan_project_references_in_dir(&groups_dir, &[]).expect("scrub");
        assert!(affected.is_empty());
    }
}
