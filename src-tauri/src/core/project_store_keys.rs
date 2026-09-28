//! Store keys: the key each per-project store uses, and how a project
//! identifier becomes that key.
//!
//! Stage 3b step 2 of the project identity plan
//! (`automatic-meta/general/plans/projects/project-identity.md`):
//!
//! | Store | Key |
//! |---|---|
//! | Memory, features, group membership | project `id` (shared by every checkout) |
//! | Activity, recommendations, dev servers | `local_key` (one checkout) |
//!
//! Commands and MCP tools accept an identifier: a `local_key`, a name, or
//! (for id-keyed stores) an `id`. [`project_store_keys`] turns it into all
//! three store-facing values at once.
//!
//! **Pass-through policy.** This mirrors `project_store_name` from step 1.
//! An identifier that does not resolve to a registered project is used
//! unchanged for every store, so data left under a deleted, never-registered
//! or not-yet-migrated project stays reachable. A registered project that
//! has not been given a key yet (its startup backfill failed) uses its name
//! for that key, which is where its data still lives.
//!
//! **Display.** Rows returned to the UI keep naming the project by its
//! display name. [`ProjectKeyIndex`] is built once per call from the
//! registry and translates stored keys back to names, so a list costs one
//! registry scan, not one per row.

use super::*;

/// The three values a per-project store can be keyed by, for one project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectStoreKeys {
    /// Display name, for messages and for rows returned to the UI.
    pub name: String,
    /// Key for id-keyed stores: memory, features, group membership.
    pub id: String,
    /// Key for checkout-keyed stores: activity, recommendations, dev servers.
    pub local_key: String,
}

impl ProjectStoreKeys {
    /// Keys for an identifier that names no registered project: the
    /// identifier itself, for every store.
    pub fn unregistered(ident: &str) -> Self {
        Self {
            name: ident.to_string(),
            id: ident.to_string(),
            local_key: ident.to_string(),
        }
    }

    /// Keys for a registered project. A key the project does not have yet
    /// falls back to its name, where its data still lives.
    pub fn of_summary(summary: &ProjectSummary) -> Self {
        let or_name = |key: &str| {
            if key.is_empty() {
                summary.name.clone()
            } else {
                key.to_string()
            }
        };
        Self {
            name: summary.name.clone(),
            id: or_name(&summary.id),
            local_key: or_name(&summary.local_key),
        }
    }
}

/// Whether `value` has the shape of a project key: a hyphenated UUID. Keys
/// are minted as lowercase UUID v4 strings, so a human-chosen name never
/// looks like one.
pub fn is_project_key(value: &str) -> bool {
    value.len() == 36 && uuid::Uuid::try_parse(value).is_ok()
}

/// Resolve `ident` (a `local_key`, a name, or a project `id`) to the keys of
/// the project it names. See the module docs for the pass-through policy.
///
/// Resolution order: `local_key`, then name (ignoring case), then `id`. An
/// `id` shared by several checkouts resolves to the first checkout in
/// registry order (by name, then file); only its `name` and `local_key`
/// depend on that choice. A name two entries share passes through, as in
/// `project_store_name`. A registry that cannot be read is an error.
pub fn project_store_keys(ident: &str) -> Result<ProjectStoreKeys, String> {
    if !is_valid_name(ident) {
        return Ok(ProjectStoreKeys::unregistered(ident));
    }
    let entries = scan_registry()?;
    match resolve_entry_index(&entries, ident) {
        Ok(Some(i)) => return Ok(ProjectStoreKeys::of_summary(&project_summary(&entries[i]))),
        Err(_) => return Ok(ProjectStoreKeys::unregistered(ident)),
        Ok(None) => {}
    }
    if is_project_key(ident) {
        // `scan_registry` sorts entries, so the first match is stable.
        for entry in &entries {
            let summary = project_summary(entry);
            if summary.id == ident {
                return Ok(ProjectStoreKeys::of_summary(&summary));
            }
        }
    }
    Ok(ProjectStoreKeys::unregistered(ident))
}

/// The key id-keyed stores (memory, features, groups) use for `ident`.
pub fn project_store_id(ident: &str) -> Result<String, String> {
    Ok(project_store_keys(ident)?.id)
}

/// The key checkout-keyed stores (activity, recommendations, dev servers)
/// use for `ident`.
pub fn project_store_local_key(ident: &str) -> Result<String, String> {
    Ok(project_store_keys(ident)?.local_key)
}

/// Every registered project's keys, for translating stored keys back to
/// names. Build it once per command with [`ProjectKeyIndex::load`].
#[derive(Debug, Clone, Default)]
pub struct ProjectKeyIndex {
    summaries: Vec<ProjectSummary>,
}

impl ProjectKeyIndex {
    /// Read the registry once.
    pub fn load() -> Result<Self, String> {
        Ok(Self::new(get_project_summaries()?))
    }

    pub fn new(summaries: Vec<ProjectSummary>) -> Self {
        Self { summaries }
    }

    pub fn summaries(&self) -> &[ProjectSummary] {
        &self.summaries
    }

    /// The project `ident` names: a `local_key` (exact), else a name that
    /// exactly one project has (ignoring case). Mirrors
    /// `resolve_entry_index`, without the error for a shared name.
    pub fn resolve(&self, ident: &str) -> Option<&ProjectSummary> {
        if ident.is_empty() {
            return None;
        }
        if let Some(s) = self.summaries.iter().find(|s| s.local_key == ident) {
            return Some(s);
        }
        let mut named = self
            .summaries
            .iter()
            .filter(|s| same_project_name(&s.name, ident));
        match (named.next(), named.next()) {
            (Some(only), None) => Some(only),
            _ => None,
        }
    }

    /// Every checkout whose project `id` is `id`, in registry order.
    pub fn checkouts_of_id<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a ProjectSummary> + 'a {
        self.summaries
            .iter()
            .filter(move |s| !id.is_empty() && s.id == id)
    }

    /// The name and `local_key` to show for a row a checkout-keyed store
    /// holds under `stored`. A `local_key` shows its project's name. A
    /// legacy row still stored under a name keeps it and gains the key. A
    /// value no project claims is returned unchanged with no key.
    pub fn display_checkout_row(&self, stored: &str) -> (String, Option<String>) {
        if let Some(s) = self.summaries.iter().find(|s| !s.local_key.is_empty() && s.local_key == stored) {
            return (s.name.clone(), Some(s.local_key.clone()));
        }
        match self.resolve(stored) {
            Some(s) => (stored.to_string(), Some(s.local_key.clone()).filter(|k| !k.is_empty())),
            None => (stored.to_string(), None),
        }
    }

    /// Names to show for one group member stored as `stored`. An `id` shows
    /// every checkout of that project. Any other value (a legacy name, or an
    /// `id` no registered project has) is shown unchanged, so nothing
    /// silently disappears.
    pub fn member_names(&self, stored: &str) -> Vec<String> {
        let names: Vec<String> = self.checkouts_of_id(stored).map(|s| s.name.clone()).collect();
        if names.is_empty() {
            vec![stored.to_string()]
        } else {
            names
        }
    }

    /// The value to store for a group member given as `member` (a name, a
    /// `local_key`, or an `id`). A project with an `id` is stored by it. A
    /// value no project claims, or a project with no `id` yet, is stored
    /// unchanged.
    pub fn member_id(&self, member: &str) -> String {
        match self.resolve(member) {
            Some(s) if !s.id.is_empty() => s.id.clone(),
            _ => member.to_string(),
        }
    }

    /// Whether `value` is the `id` or `local_key` of a registered project.
    pub fn is_known_key(&self, value: &str) -> bool {
        !value.is_empty()
            && self
                .summaries
                .iter()
                .any(|s| s.id == value || s.local_key == value)
    }

    /// Whether every registered project has an `id`. When one does not (its
    /// entry is damaged, or its backfill failed), a group member that looks
    /// like an `id` cannot be classified as orphaned.
    pub fn every_project_has_an_id(&self) -> bool {
        self.summaries.iter().all(|s| !s.id.is_empty())
    }
}

/// Translate a group's stored members to display names. See
/// [`ProjectKeyIndex::member_names`]. Order follows the stored list; a name
/// appears once.
pub fn group_members_for_display(index: &ProjectKeyIndex, stored: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for member in stored {
        for name in index.member_names(member) {
            if !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

/// Translate a group's members, as the UI sends them, to stored values. See
/// [`ProjectKeyIndex::member_id`]. Order follows the first occurrence; a
/// value appears once, so two checkouts of one project collapse to one `id`.
pub fn group_members_for_storage(index: &ProjectKeyIndex, members: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for member in members {
        let stored = index.member_id(member);
        if !out.contains(&stored) {
            out.push(stored);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID_A: &str = "11111111-1111-4111-8111-111111111111";
    const KEY_A1: &str = "aaaaaaaa-0000-4000-8000-000000000001";
    const KEY_A2: &str = "aaaaaaaa-0000-4000-8000-000000000002";
    const ID_B: &str = "22222222-2222-4222-8222-222222222222";
    const KEY_B: &str = "bbbbbbbb-0000-4000-8000-000000000001";

    fn summary(name: &str, id: &str, local_key: &str) -> ProjectSummary {
        ProjectSummary {
            local_key: local_key.into(),
            id: id.into(),
            name: name.into(),
            directory: String::new(),
        }
    }

    fn index() -> ProjectKeyIndex {
        ProjectKeyIndex::new(vec![
            summary("site", ID_A, KEY_A1),
            summary("site-worktree", ID_A, KEY_A2),
            summary("api", ID_B, KEY_B),
            summary("unkeyed", "", ""),
        ])
    }

    fn strings(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn project_key_shape_is_a_hyphenated_uuid() {
        assert!(is_project_key(ID_A));
        assert!(!is_project_key("site"));
        assert!(!is_project_key("11111111111141118111111111111111"), "simple form is not a key");
        assert!(!is_project_key(""));
    }

    #[test]
    fn keys_of_a_summary_fall_back_to_the_name() {
        let keys = ProjectStoreKeys::of_summary(&summary("unkeyed", "", ""));
        assert_eq!(keys, ProjectStoreKeys::unregistered("unkeyed"));
        let keys = ProjectStoreKeys::of_summary(&summary("site", ID_A, KEY_A1));
        assert_eq!((keys.id.as_str(), keys.local_key.as_str()), (ID_A, KEY_A1));
    }

    #[test]
    fn checkout_rows_show_the_name_and_keep_orphans() {
        let index = index();
        assert_eq!(index.display_checkout_row(KEY_A2), ("site-worktree".into(), Some(KEY_A2.into())));
        assert_eq!(index.display_checkout_row("api"), ("api".into(), Some(KEY_B.into())), "legacy name row");
        assert_eq!(index.display_checkout_row("ghost"), ("ghost".into(), None));
        assert_eq!(index.display_checkout_row("unkeyed"), ("unkeyed".into(), None));
    }

    #[test]
    fn group_members_round_trip_between_names_and_ids() {
        let index = index();
        let shown = group_members_for_display(&index, &strings(&[ID_A, "legacy", ID_B, "99999999-9999-4999-8999-999999999999"]));
        assert_eq!(
            shown,
            strings(&["site", "site-worktree", "legacy", "api", "99999999-9999-4999-8999-999999999999"]),
            "an id shows every checkout; unknown values stay"
        );
        let stored = group_members_for_storage(&index, &shown);
        assert_eq!(
            stored,
            strings(&[ID_A, "legacy", ID_B, "99999999-9999-4999-8999-999999999999"]),
            "checkouts collapse to one id; unknown values stay"
        );
        assert_eq!(
            group_members_for_storage(&index, &strings(&["SITE", KEY_B, "unkeyed"])),
            strings(&[ID_A, ID_B, "unkeyed"]),
            "a name ignores case, a key maps to its id, a project without an id keeps its name"
        );
    }

    #[test]
    fn known_keys_and_id_coverage() {
        let index = index();
        assert!(index.is_known_key(ID_A) && index.is_known_key(KEY_B));
        assert!(!index.is_known_key("site"));
        assert!(!index.every_project_has_an_id());
        assert!(ProjectKeyIndex::new(vec![summary("a", ID_A, KEY_A1)]).every_project_has_an_id());
    }

    #[test]
    fn store_keys_resolve_key_name_and_id_and_pass_orphans_through() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let projects = get_projects_dir().unwrap();
            std::fs::create_dir_all(&projects).unwrap();
            let write = |key: &str, name: &str, id: &str| {
                let value = serde_json::json!({"name": name, "local_key": key, "id": id});
                std::fs::write(projects.join(format!("{}.json", key)), value.to_string()).unwrap();
            };
            write(KEY_A1, "site", ID_A);
            write(KEY_A2, "site-worktree", ID_A);

            let by_key = project_store_keys(KEY_A2).unwrap();
            assert_eq!(by_key.name, "site-worktree");
            assert_eq!(by_key.id, ID_A);
            assert_eq!(by_key.local_key, KEY_A2);

            let by_name = project_store_keys("SITE").unwrap();
            assert_eq!((by_name.name.as_str(), by_name.local_key.as_str()), ("site", KEY_A1));

            let by_id = project_store_keys(ID_A).unwrap();
            assert_eq!(by_id.id, ID_A);
            assert_eq!(by_id.name, "site", "the first checkout in registry order");

            assert_eq!(project_store_keys("orphan").unwrap(), ProjectStoreKeys::unregistered("orphan"));
            assert_eq!(project_store_id("site-worktree").unwrap(), ID_A);
            assert_eq!(project_store_local_key("site").unwrap(), KEY_A1);
            assert_eq!(project_store_keys("../x").unwrap(), ProjectStoreKeys::unregistered("../x"));
        });
    }
}
