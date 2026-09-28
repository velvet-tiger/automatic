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
//
// Membership is stored by project `id` (stage 3b step 2 of the project
// identity plan), so a rename touches no group file and every checkout of a
// project shares its groups. The public API still speaks names: `read_group`
// and `groups_for_project` return member names, and `save_group` accepts
// them. `project_store_keys::group_members_for_display` and
// `group_members_for_storage` do the translation. A member no registered
// project claims is kept as stored in both directions, so a legacy name
// that the startup migration has not converted, or the id of a deleted
// project, never silently disappears. The raw helpers below
// (`*_in_dir`) work on stored values.

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

    let group = read_group_for_display(&ProjectKeyIndex::load()?, &path, name)?;
    serde_json::to_string_pretty(&group).map_err(|e| e.to_string())
}

/// Read the group file at `path` with its members translated to names.
fn read_group_for_display(
    index: &ProjectKeyIndex,
    path: &std::path::Path,
    name: &str,
) -> Result<ProjectGroup, String> {
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    // Round-trip through the struct to ensure forward-compatibility: unknown
    // fields are silently dropped and defaults are applied.
    let mut group = serde_json::from_str::<ProjectGroup>(&raw).unwrap_or_else(|_| ProjectGroup {
        name: name.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
        ..Default::default()
    });
    group.projects = group_members_for_display(index, &group.projects);
    Ok(group)
}

pub fn save_group(name: &str, data: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid group name".into());
    }

    let mut group: ProjectGroup =
        serde_json::from_str(data).map_err(|e| format!("Invalid group data: {}", e))?;
    // Members arrive as names; store them by project id.
    group.projects = group_members_for_storage(&ProjectKeyIndex::load()?, &group.projects);
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

/// Return all groups that contain the given project name, with members as
/// names. The registry is read once for the whole pass.
pub fn groups_for_project(project_name: &str) -> Vec<ProjectGroup> {
    let names = match list_groups() {
        Ok(n) => n,
        Err(_) => return Vec::new(),
    };
    let index = match ProjectKeyIndex::load() {
        Ok(index) => index,
        Err(e) => {
            eprintln!("groups_for_project: could not read the project registry: {}", e);
            return Vec::new();
        }
    };
    let groups_dir = match get_groups_dir() {
        Ok(dir) => dir,
        Err(_) => return Vec::new(),
    };

    let mut result = Vec::new();
    for name in names {
        if let Ok(group) = read_group_for_display(&index, &group_path(&groups_dir, &name), &name) {
            if group.projects.iter().any(|p| p == project_name) {
                result.push(group);
            }
        }
    }
    result
}

/// Remove each of `stored_values` (a project `id`, or a legacy name) from
/// every group's stored `projects` list and persist the changes. Returns the
/// names of groups that were modified.
///
/// Per-group failures are logged and skipped rather than aborting the whole
/// pass — callers (typically `delete_project`) treat this as best-effort
/// cleanup since the project file is already gone.
pub fn remove_project_from_all_groups(stored_values: &[String]) -> Result<Vec<String>, String> {
    let groups_dir = get_groups_dir()?;
    remove_project_from_all_groups_in_dir(&groups_dir, stored_values)
}

/// Replace the stored member `old_value` with `new_value` in every group's
/// `projects` list. If a group already contains `new_value`, the stale
/// `old_value` entry is dropped (deduplication) rather than producing a
/// duplicate. Returns the names of groups that were modified.
///
/// Members are stored by `id`, which a rename does not change. A rename
/// uses this only to convert a legacy name member the startup migration
/// has not reached yet: `old_value` is the old name and `new_value` the
/// project's `id` (or its new name when it has no `id` yet).
pub fn rename_project_in_all_groups(old_value: &str, new_value: &str) -> Result<Vec<String>, String> {
    let groups_dir = get_groups_dir()?;
    rename_project_in_all_groups_in_dir(&groups_dir, old_value, new_value)
}

/// Drop every member that is a well-formed project key but belongs to no
/// registered project. Returns the names of groups that were modified.
///
/// Heals references to deleted projects. It never removes a member it
/// cannot classify: any other string (a legacy name the migration has not
/// converted, or a hand-edited value) is left alone. When some registered
/// project has no `id`, an unknown key might be that project's, so the
/// scrub is skipped and an empty list returned. Idempotent.
pub fn scrub_orphan_project_references() -> Result<Vec<String>, String> {
    let index = ProjectKeyIndex::load()?;
    if !index.every_project_has_an_id() {
        eprintln!(
            "[automatic] group scrub skipped: a registered project has no id yet, so orphaned members cannot be told apart"
        );
        return Ok(Vec::new());
    }
    let groups_dir = get_groups_dir()?;
    scrub_orphan_project_references_in_dir(&groups_dir, &|member| index.is_known_key(member))
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
    stored_values: &[String],
) -> Result<Vec<String>, String> {
    let names = list_group_names_in_dir(groups_dir)?;
    let mut affected = Vec::new();
    for name in names {
        let mut group = match read_group_from_dir(groups_dir, &name) {
            Ok(g) => g,
            Err(e) => {
                eprintln!(
                    "groups cleanup: skipping unreadable group '{}': {}",
                    name, e
                );
                continue;
            }
        };
        let before = group.projects.len();
        group.projects.retain(|p| !stored_values.contains(p));
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
    is_live_key: &dyn Fn(&str) -> bool,
) -> Result<Vec<String>, String> {
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
        group.projects.retain(|p| !is_project_key(p) || is_live_key(p));
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
            contexts: Vec::new(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        write_group_to_dir(groups_dir, &group).expect("write group");
    }

    fn read_projects(groups_dir: &PathBuf, name: &str) -> Vec<String> {
        read_group_from_dir(groups_dir, name)
            .expect("read")
            .projects
    }

    // ── remove_project_from_all_groups ───────────────────────────────────────

    #[test]
    fn remove_strips_project_from_every_group_that_lists_it() {
        let (_tmp, groups_dir) = setup();
        write_group(&groups_dir, "foo", &["a", "b"]);
        write_group(&groups_dir, "bar", &["a"]);
        write_group(&groups_dir, "baz", &["b"]);

        let mut affected = remove_project_from_all_groups_in_dir(&groups_dir, &["a".to_string()]).expect("clean");
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
        let affected = remove_project_from_all_groups_in_dir(&groups_dir, &["ghost".to_string()]).expect("clean");
        assert!(affected.is_empty());
        assert_eq!(read_projects(&groups_dir, "foo"), vec!["a"]);
    }

    #[test]
    fn remove_handles_missing_groups_dir() {
        let (_tmp, groups_dir) = setup();
        let affected = remove_project_from_all_groups_in_dir(&groups_dir, &["a".to_string()]).expect("clean");
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
        let affected =
            rename_project_in_all_groups_in_dir(&groups_dir, "ghost", "z").expect("rename");
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

    const LIVE: &str = "11111111-1111-4111-8111-111111111111";
    const DEAD: &str = "22222222-2222-4222-8222-222222222222";

    #[test]
    fn scrub_drops_only_unknown_project_keys() {
        let (_tmp, groups_dir) = setup();
        write_group(&groups_dir, "foo", &[LIVE, DEAD, "legacy-name"]);
        write_group(&groups_dir, "bar", &[LIVE]);
        write_group(&groups_dir, "baz", &[DEAD]);

        let mut affected =
            scrub_orphan_project_references_in_dir(&groups_dir, &|m| m == LIVE).expect("scrub");
        affected.sort();
        assert_eq!(affected, vec!["baz", "foo"]);
        assert_eq!(
            read_projects(&groups_dir, "foo"),
            vec![LIVE, "legacy-name"],
            "a member that is not a key cannot be classified and stays"
        );
        assert_eq!(read_projects(&groups_dir, "bar"), vec![LIVE]);
        assert!(read_projects(&groups_dir, "baz").is_empty());
    }

    #[test]
    fn scrub_never_removes_names_even_when_nothing_is_live() {
        let (_tmp, groups_dir) = setup();
        write_group(&groups_dir, "foo", &["ghost", "Other Name", "not-a-uuid-1234"]);
        let affected = scrub_orphan_project_references_in_dir(&groups_dir, &|_| false).expect("scrub");
        assert!(affected.is_empty());
        assert_eq!(read_projects(&groups_dir, "foo"), vec!["ghost", "Other Name", "not-a-uuid-1234"]);
    }

    #[test]
    fn scrub_is_idempotent_when_no_orphans() {
        let (_tmp, groups_dir) = setup();
        write_group(&groups_dir, "foo", &[LIVE]);
        let affected = scrub_orphan_project_references_in_dir(&groups_dir, &|m| m == LIVE).expect("scrub");
        assert!(affected.is_empty());
        assert_eq!(read_projects(&groups_dir, "foo"), vec![LIVE]);
    }

    #[test]
    fn scrub_handles_missing_groups_dir() {
        let (_tmp, groups_dir) = setup();
        let affected = scrub_orphan_project_references_in_dir(&groups_dir, &|_| false).expect("scrub");
        assert!(affected.is_empty());
    }

    #[test]
    fn scrub_is_skipped_while_a_project_has_no_id() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            crate::core::save_project("unkeyed", r#"{"name":"unkeyed"}"#).unwrap();
            let groups_dir = get_groups_dir().unwrap();
            write_group(&groups_dir, "g", &[DEAD]);
            assert!(scrub_orphan_project_references().unwrap().is_empty());
            assert_eq!(read_projects(&groups_dir, "g"), vec![DEAD], "an unclassifiable key stays");
        });
    }
}
