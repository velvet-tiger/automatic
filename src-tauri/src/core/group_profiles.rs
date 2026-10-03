use super::*;

// ── Group Profiles ───────────────────────────────────────────────────────────
//
// A project group can list profiles. Every member project receives them in
// its own `profiles` list, and `Project::group_profile_contributions` records
// which group provides which name. From there the ordinary profile reconcile
// (`reconcile_project_profiles`) and sync do the work, so a group profile
// behaves exactly like one attached by hand, except that it can only be
// removed at the group.
//
// Order matters wherever a project is reconciled: group profiles first, then
// `reconcile_project_profiles`. The first decides which profiles are
// attached. The second turns that into skills, rules and the rest.

/// Bring a project's `profiles` in step with the groups it belongs to.
///
/// `member_groups` must be exactly the groups that list the project. The
/// rules are those of `reconcile_group_contexts`: a departed group releases
/// what it recorded, a member group records every profile it lists (added
/// when missing, adopted when the project already had it), and a released
/// profile another member group still lists stays and its record moves.
///
/// Only edits `profiles` and `group_profile_contributions`. A released
/// profile keeps its `profile_contributions` record, so the
/// `reconcile_project_profiles` call that must follow removes its entries.
/// Returns `true` when anything changed.
pub fn reconcile_group_profiles(project: &mut Project, member_groups: &[ProjectGroup]) -> bool {
    reconcile_group_provided(
        &mut project.profiles,
        &mut project.group_profile_contributions,
        member_groups,
        group_profiles,
    )
}

fn group_profiles(group: &ProjectGroup) -> &[String] {
    &group.profiles
}

/// The group that provides `profile` to the project, or `None` when the
/// profile is the project's own. The first group by name wins when a stale
/// record names more than one.
pub fn group_providing_profile(project: &Project, profile: &str) -> Option<String> {
    // BTreeMap iterates in key order, so the first match is the lowest name.
    project
        .group_profile_contributions
        .iter()
        .find(|(_, profiles)| profiles.iter().any(|p| p == profile))
        .map(|(group, _)| group.clone())
}

/// Add `profile_name` to a group's `profiles` and save the group. Fails when
/// the profile or the group does not exist. Returns `false` when the group
/// already listed it. Does not touch member projects: the caller reconciles
/// and syncs them.
pub fn add_profile_to_group(group_name: &str, profile_name: &str) -> Result<bool, String> {
    read_project_profile_parsed(profile_name)?;
    edit_group_profiles(group_name, |profiles| {
        if profiles.iter().any(|p| p == profile_name) {
            return false;
        }
        profiles.push(profile_name.to_string());
        true
    })
}

/// Remove `profile_name` from a group's `profiles` and save the group. Fails
/// when the group does not exist. Returns `false` when the group did not
/// list it. Does not touch member projects: the caller reconciles and syncs
/// them.
pub fn remove_profile_from_group(group_name: &str, profile_name: &str) -> Result<bool, String> {
    edit_group_profiles(group_name, |profiles| {
        let before = profiles.len();
        profiles.retain(|p| p != profile_name);
        profiles.len() != before
    })
}

/// Apply `edit` to one group's `profiles` and save the group when it
/// reports a change.
fn edit_group_profiles(
    group_name: &str,
    edit: impl FnOnce(&mut Vec<String>) -> bool,
) -> Result<bool, String> {
    let mut group = read_group_parsed(group_name)?;
    if !edit(&mut group.profiles) {
        return Ok(false);
    }
    write_group(group_name, &mut group)?;
    Ok(true)
}

/// Names of the groups that list `profile_name`, sorted. Unreadable group
/// files are skipped.
pub fn groups_referencing_profile(profile_name: &str) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    for name in list_groups()? {
        match read_group_parsed(&name) {
            Ok(group) if group.profiles.iter().any(|p| p == profile_name) => names.push(name),
            Ok(_) => {}
            Err(e) => eprintln!("group profiles: skipping unreadable group '{}': {}", name, e),
        }
    }
    names.sort();
    Ok(names)
}

/// Remove `profile_name` from every group's `profiles`. Used when a profile
/// is deleted, so no group keeps a dangling reference. Returns the groups
/// that changed. Per-group failures are logged and skipped.
pub(crate) fn prune_profile_from_groups(profile_name: &str) -> Vec<String> {
    rewrite_groups_listing_profile(profile_name, |profiles| {
        profiles.retain(|p| p != profile_name);
    })
}

/// Rename `old` to `new` in every group's `profiles`. When a group already
/// lists `new`, the `old` entry is dropped. Returns the groups that changed.
/// Per-group failures are logged and skipped.
pub(crate) fn rename_profile_in_groups(old: &str, new: &str) -> Vec<String> {
    rewrite_groups_listing_profile(old, |profiles| rename_profile_entry(profiles, old, new))
}

/// Apply `edit` to the `profiles` of every group that lists `profile_name`
/// and save each one.
fn rewrite_groups_listing_profile(
    profile_name: &str,
    edit: impl Fn(&mut Vec<String>),
) -> Vec<String> {
    let names = match groups_referencing_profile(profile_name) {
        Ok(names) => names,
        Err(e) => {
            eprintln!("group profiles: could not list groups: {}", e);
            return Vec::new();
        }
    };
    let mut changed = Vec::new();
    for name in names {
        let saved = read_group_parsed(&name).and_then(|mut group| {
            edit(&mut group.profiles);
            write_group(&name, &mut group)
        });
        match saved {
            Ok(()) => changed.push(name),
            Err(e) => eprintln!("group profiles: failed to update group '{}': {}", name, e),
        }
    }
    changed
}

/// Forget `profile_name` in every group record on `project`. Used when a
/// profile is deleted. Returns `true` when anything changed.
pub(crate) fn strip_group_profile_contribution(project: &mut Project, profile_name: &str) -> bool {
    let mut changed = false;
    project.group_profile_contributions.retain(|_, profiles| {
        let before = profiles.len();
        profiles.retain(|p| p != profile_name);
        changed |= profiles.len() != before;
        !profiles.is_empty()
    });
    changed
}

/// Rename `old` to `new` in every group record on `project`. Returns `true`
/// when anything changed.
pub(crate) fn rename_group_profile_contribution(project: &mut Project, old: &str, new: &str) -> bool {
    let mut changed = false;
    for profiles in project.group_profile_contributions.values_mut() {
        if profiles.iter().any(|p| p == old) {
            rename_profile_entry(profiles, old, new);
            changed = true;
        }
    }
    changed
}

/// Replace `old` with `new` in `profiles`, or drop `old` when `new` is
/// already there.
fn rename_profile_entry(profiles: &mut Vec<String>, old: &str, new: &str) {
    if profiles.iter().any(|p| p == new) {
        profiles.retain(|p| p != old);
        return;
    }
    for entry in profiles.iter_mut() {
        if entry == old {
            *entry = new.to_string();
        }
    }
}

/// Read a group for editing. Members come back as display keys, which
/// `save_group` turns into stored ids again.
fn read_group_parsed(name: &str) -> Result<ProjectGroup, String> {
    let raw = read_group(name)?;
    serde_json::from_str(&raw).map_err(|e| format!("Invalid group '{}': {}", name, e))
}

/// Stamp `updated_at` and save the group.
fn write_group(name: &str, group: &mut ProjectGroup) -> Result<(), String> {
    group.updated_at = chrono::Utc::now().to_rfc3339();
    let data = serde_json::to_string_pretty(group).map_err(|e| e.to_string())?;
    save_group(name, &data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::paths::with_test_home;

    fn group(name: &str, profiles: &[&str]) -> ProjectGroup {
        ProjectGroup {
            name: name.to_string(),
            profiles: profiles.iter().map(|p| p.to_string()).collect(),
            ..Default::default()
        }
    }

    fn project(profiles: &[&str]) -> Project {
        Project {
            name: "app".to_string(),
            profiles: profiles.iter().map(|p| p.to_string()).collect(),
            ..Default::default()
        }
    }

    fn record(project: &Project, group: &str) -> Vec<String> {
        project
            .group_profile_contributions
            .get(group)
            .cloned()
            .unwrap_or_default()
    }

    fn with_home<T>(test: impl FnOnce() -> T) -> T {
        let tmp = tempfile::tempdir().expect("tempdir");
        with_test_home(tmp.path().to_path_buf(), test)
    }

    fn save_profile(name: &str) {
        save_project_profile(name, &serde_json::json!({ "name": name }).to_string())
            .expect("save profile");
    }

    fn save_group_value(group: &ProjectGroup) {
        save_group(&group.name, &serde_json::to_string(group).unwrap()).expect("save group");
    }

    #[test]
    fn reconcile_adds_and_records_a_group_profile() {
        let mut p = project(&["own"]);
        assert!(reconcile_group_profiles(&mut p, &[group("team", &["base"])]));
        assert_eq!(p.profiles, vec!["own", "base"]);
        assert_eq!(record(&p, "team"), vec!["base"]);
        assert_eq!(group_providing_profile(&p, "base").as_deref(), Some("team"));
        assert_eq!(group_providing_profile(&p, "own"), None);

        assert!(!reconcile_group_profiles(&mut p, &[group("team", &["base"])]), "settles");
    }

    #[test]
    fn reconcile_adopts_a_profile_the_project_already_had() {
        let mut p = project(&["base"]);
        assert!(reconcile_group_profiles(&mut p, &[group("team", &["base"])]));
        assert_eq!(p.profiles, vec!["base"]);
        assert_eq!(record(&p, "team"), vec!["base"]);

        // Adopted means owned: the group dropping it removes it.
        assert!(reconcile_group_profiles(&mut p, &[group("team", &[])]));
        assert!(p.profiles.is_empty());
        assert!(p.group_profile_contributions.is_empty());
    }

    #[test]
    fn reconcile_releases_a_profile_the_group_stops_listing() {
        let mut p = project(&["own"]);
        reconcile_group_profiles(&mut p, &[group("team", &["a", "b"])]);
        assert!(reconcile_group_profiles(&mut p, &[group("team", &["b"])]));
        assert_eq!(p.profiles, vec!["own", "b"]);
        assert_eq!(record(&p, "team"), vec!["b"]);
    }

    #[test]
    fn released_profile_moves_to_another_group_that_lists_it() {
        let mut p = project(&[]);
        let both = [group("alpha", &["base"]), group("beta", &["base"])];
        reconcile_group_profiles(&mut p, &both);
        assert_eq!(record(&p, "alpha"), vec!["base"]);
        assert!(record(&p, "beta").is_empty(), "one group records it");

        // `alpha` drops it while `beta` still lists it.
        let after = [group("alpha", &[]), group("beta", &["base"])];
        assert!(reconcile_group_profiles(&mut p, &after));
        assert_eq!(p.profiles, vec!["base"]);
        assert_eq!(record(&p, "beta"), vec!["base"]);
        assert!(!p.group_profile_contributions.contains_key("alpha"));
    }

    #[test]
    fn departed_group_releases_everything_it_recorded() {
        let mut p = project(&["own"]);
        let both = [group("alpha", &["a", "shared"]), group("beta", &["shared"])];
        reconcile_group_profiles(&mut p, &both);
        assert_eq!(p.profiles, vec!["own", "a", "shared"]);

        // The project leaves `alpha`. `shared` stays through `beta`.
        assert!(reconcile_group_profiles(&mut p, &[group("beta", &["shared"])]));
        assert_eq!(p.profiles, vec!["own", "shared"]);
        assert_eq!(record(&p, "beta"), vec!["shared"]);
        assert!(!p.group_profile_contributions.contains_key("alpha"));

        // It leaves every group.
        assert!(reconcile_group_profiles(&mut p, &[]));
        assert_eq!(p.profiles, vec!["own"]);
        assert!(p.group_profile_contributions.is_empty());
    }

    #[test]
    fn released_profile_entries_go_on_the_following_profile_reconcile() {
        with_home(|| {
            save_project_profile(
                "base",
                &serde_json::json!({ "name": "base", "skills": ["react"] }).to_string(),
            )
            .expect("save profile");

            let mut p = project(&[]);
            reconcile_group_profiles(&mut p, &[group("team", &["base"])]);
            reconcile_project_profiles(&mut p);
            assert_eq!(p.skills, vec!["react"]);

            reconcile_group_profiles(&mut p, &[]);
            assert!(p.profiles.is_empty());
            assert_eq!(p.skills, vec!["react"], "group reconcile leaves entries alone");
            reconcile_project_profiles(&mut p);
            assert!(p.skills.is_empty());
            assert!(p.profile_contributions.is_empty());
        });
    }

    #[test]
    fn add_and_remove_edit_the_group_file() {
        with_home(|| {
            save_profile("base");
            save_group_value(&group("team", &[]));

            assert!(add_profile_to_group("team", "ghost").is_err(), "unknown profile");
            assert!(add_profile_to_group("nope", "base").is_err(), "unknown group");
            assert!(remove_profile_from_group("nope", "base").is_err(), "unknown group");

            assert!(add_profile_to_group("team", "base").unwrap());
            assert!(!add_profile_to_group("team", "base").unwrap(), "idempotent");
            let saved = read_group_parsed("team").unwrap();
            assert_eq!(saved.profiles, vec!["base"]);
            assert!(!saved.updated_at.is_empty());
            assert_eq!(groups_referencing_profile("base").unwrap(), vec!["team"]);

            assert!(remove_profile_from_group("team", "base").unwrap());
            assert!(!remove_profile_from_group("team", "base").unwrap(), "idempotent");
            assert!(groups_referencing_profile("base").unwrap().is_empty());
        });
    }

    #[test]
    fn prune_and_rename_rewrite_group_files_and_records() {
        with_home(|| {
            save_group_value(&group("alpha", &["old", "keep"]));
            save_group_value(&group("beta", &["old", "new"]));
            save_group_value(&group("gamma", &["keep"]));

            assert_eq!(rename_profile_in_groups("old", "new"), vec!["alpha", "beta"]);
            assert_eq!(read_group_parsed("alpha").unwrap().profiles, vec!["new", "keep"]);
            assert_eq!(read_group_parsed("beta").unwrap().profiles, vec!["new"], "no duplicate");

            assert_eq!(prune_profile_from_groups("keep"), vec!["alpha", "gamma"]);
            assert_eq!(read_group_parsed("alpha").unwrap().profiles, vec!["new"]);
            assert!(read_group_parsed("gamma").unwrap().profiles.is_empty());
        });

        let mut p = project(&["old", "keep"]);
        p.group_profile_contributions
            .insert("alpha".into(), vec!["old".into(), "keep".into()]);
        assert!(rename_group_profile_contribution(&mut p, "old", "new"));
        assert!(!rename_group_profile_contribution(&mut p, "old", "new"));
        assert_eq!(record(&p, "alpha"), vec!["new", "keep"]);
        assert!(strip_group_profile_contribution(&mut p, "new"));
        assert!(strip_group_profile_contribution(&mut p, "keep"));
        assert!(p.group_profile_contributions.is_empty());
        assert!(!strip_group_profile_contribution(&mut p, "keep"));
    }
}
