use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use super::*;

// ── Project Profiles ─────────────────────────────────────────────────────────
//
// A profile is the live counterpart of a project template. It holds the same
// library references a template holds (skills, MCP servers, providers, agents,
// sub-agents, commands, hooks, rules) but stays attached: saving it brings
// every attached project back in step. Stored as JSON files in
// `~/.automatic/library/profiles/{name}.json`.
//
// Profiles write their references into a project's own lists in
// `project.json`, so every existing mechanism (save → sync, drift detection,
// the `sync_projects_referencing_*` sweeps, autodetect) keeps working
// unchanged. `Project::profile_contributions` records what each profile added
// so a later reconcile can remove exactly that and nothing the project
// defined itself.

/// The canonical `file_rules` key. Profile rules are attached here, the same
/// key the Rules tab and `apply_templates_to_project` use.
const PROJECT_RULES_KEY: &str = "_project";

/// A profile: a named, live bundle of library references.
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct ProjectProfile {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub mcp_servers: Vec<String>,
    #[serde(default)]
    pub providers: Vec<String>,
    #[serde(default)]
    pub agents: Vec<String>,
    /// Workspace sub-agent machine names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_agents: Vec<String>,
    /// Workspace command machine names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_commands: Vec<String>,
    /// Hook machine names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<String>,
    /// Rule machine names, attached to `file_rules["_project"]`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<String>,
    /// Author/provider metadata for profiles installed from a remote source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _author: Option<serde_json::Value>,
}

// ── Storage ──────────────────────────────────────────────────────────────────

pub fn get_profiles_dir() -> Result<PathBuf, String> {
    Ok(super::paths::get_library_dir()?.join("profiles"))
}

fn profile_path(name: &str) -> Result<PathBuf, String> {
    Ok(get_profiles_dir()?.join(format!("{}.json", name)))
}

pub fn list_project_profiles() -> Result<Vec<String>, String> {
    let dir = get_profiles_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut names = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten() {
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

/// Read a profile as JSON text. The `name` field always reflects the file
/// stem, which is the name projects reference.
pub fn read_project_profile(name: &str) -> Result<String, String> {
    let profile = read_project_profile_parsed(name)?;
    serde_json::to_string(&profile).map_err(|e| e.to_string())
}

/// Read and parse a profile. Fails when the file does not exist.
pub fn read_project_profile_parsed(name: &str) -> Result<ProjectProfile, String> {
    if !is_valid_name(name) {
        return Err("Invalid profile name".into());
    }
    let path = profile_path(name)?;
    if !path.exists() {
        return Err(format!("Profile '{}' not found", name));
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut profile: ProjectProfile =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid profile data: {}", e))?;
    profile.name = name.to_string();
    if profile._author.is_none() {
        profile._author = super::remote_sources::get_provenance_author("profile", name)?;
    }
    Ok(profile)
}

pub fn save_project_profile(name: &str, data: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid profile name".into());
    }

    let mut profile: ProjectProfile =
        serde_json::from_str(data).map_err(|e| format!("Invalid profile data: {}", e))?;
    profile.name = name.to_string();
    let pretty = serde_json::to_string_pretty(&profile).map_err(|e| e.to_string())?;

    let dir = get_profiles_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    let path = dir.join(format!("{}.json", name));
    let is_new = !path.exists();
    fs::write(&path, pretty).map_err(|e| e.to_string())?;

    if is_new {
        record_recently_added("profiles", name);
    }

    Ok(())
}

pub fn delete_project_profile(name: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid profile name".into());
    }
    let path = profile_path(name)?;
    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    remove_recently_added("profiles", name);
    Ok(())
}

pub fn rename_project_profile(old_name: &str, new_name: &str) -> Result<(), String> {
    if !is_valid_name(old_name) {
        return Err("Invalid current profile name".into());
    }
    if !is_valid_name(new_name) {
        return Err("Invalid new profile name".into());
    }
    if old_name == new_name {
        return Ok(());
    }

    let old_path = profile_path(old_name)?;
    let new_path = profile_path(new_name)?;
    if !old_path.exists() {
        return Err(format!("Profile '{}' not found", old_name));
    }
    if new_path.exists() {
        return Err(format!("A profile named '{}' already exists", new_name));
    }

    let raw = fs::read_to_string(&old_path).map_err(|e| e.to_string())?;
    let mut profile: ProjectProfile =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid profile data: {}", e))?;
    profile.name = new_name.to_string();
    let pretty = serde_json::to_string_pretty(&profile).map_err(|e| e.to_string())?;
    fs::write(&new_path, pretty).map_err(|e| e.to_string())?;
    fs::remove_file(&old_path).map_err(|e| e.to_string())?;

    Ok(())
}

// ── Resource kinds ───────────────────────────────────────────────────────────

/// The kinds of library reference a profile can carry. Each maps to one list
/// on `ProjectProfile`, one on `ProfileContribution`, and one on `Project`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileResourceKind {
    Skill,
    McpServer,
    Provider,
    Agent,
    UserAgent,
    UserCommand,
    Hook,
    Rule,
}

impl ProfileResourceKind {
    pub const ALL: [ProfileResourceKind; 8] = [
        ProfileResourceKind::Skill,
        ProfileResourceKind::McpServer,
        ProfileResourceKind::Provider,
        ProfileResourceKind::Agent,
        ProfileResourceKind::UserAgent,
        ProfileResourceKind::UserCommand,
        ProfileResourceKind::Hook,
        ProfileResourceKind::Rule,
    ];

    fn profile_list<'a>(&self, profile: &'a ProjectProfile) -> &'a Vec<String> {
        match self {
            Self::Skill => &profile.skills,
            Self::McpServer => &profile.mcp_servers,
            Self::Provider => &profile.providers,
            Self::Agent => &profile.agents,
            Self::UserAgent => &profile.user_agents,
            Self::UserCommand => &profile.user_commands,
            Self::Hook => &profile.hooks,
            Self::Rule => &profile.rules,
        }
    }

    fn profile_list_mut<'a>(&self, profile: &'a mut ProjectProfile) -> &'a mut Vec<String> {
        match self {
            Self::Skill => &mut profile.skills,
            Self::McpServer => &mut profile.mcp_servers,
            Self::Provider => &mut profile.providers,
            Self::Agent => &mut profile.agents,
            Self::UserAgent => &mut profile.user_agents,
            Self::UserCommand => &mut profile.user_commands,
            Self::Hook => &mut profile.hooks,
            Self::Rule => &mut profile.rules,
        }
    }

    fn contribution<'a>(&self, c: &'a ProfileContribution) -> &'a Vec<String> {
        match self {
            Self::Skill => &c.skills,
            Self::McpServer => &c.mcp_servers,
            Self::Provider => &c.providers,
            Self::Agent => &c.agents,
            Self::UserAgent => &c.user_agents,
            Self::UserCommand => &c.user_commands,
            Self::Hook => &c.hooks,
            Self::Rule => &c.rules,
        }
    }

    fn contribution_mut<'a>(&self, c: &'a mut ProfileContribution) -> &'a mut Vec<String> {
        match self {
            Self::Skill => &mut c.skills,
            Self::McpServer => &mut c.mcp_servers,
            Self::Provider => &mut c.providers,
            Self::Agent => &mut c.agents,
            Self::UserAgent => &mut c.user_agents,
            Self::UserCommand => &mut c.user_commands,
            Self::Hook => &mut c.hooks,
            Self::Rule => &mut c.rules,
        }
    }

    /// The project list this kind lives in. Rules live under
    /// `file_rules["_project"]`, created on demand.
    fn project_list_mut<'a>(&self, project: &'a mut Project) -> &'a mut Vec<String> {
        match self {
            Self::Skill => &mut project.skills,
            Self::McpServer => &mut project.mcp_servers,
            Self::Provider => &mut project.providers,
            Self::Agent => &mut project.agents,
            Self::UserAgent => &mut project.user_agents,
            Self::UserCommand => &mut project.user_commands,
            Self::Hook => &mut project.hooks,
            Self::Rule => project
                .file_rules
                .entry(PROJECT_RULES_KEY.to_string())
                .or_default(),
        }
    }

    /// Name equality for this kind. MCP server names are matched without
    /// case, mirroring `enrich_project` (the registry is keyed by filename on
    /// a case-insensitive filesystem).
    fn same(&self, a: &str, b: &str) -> bool {
        match self {
            Self::McpServer => a.eq_ignore_ascii_case(b),
            _ => a == b,
        }
    }

    fn contains(&self, list: &[String], item: &str) -> bool {
        list.iter().any(|x| self.same(x, item))
    }
}

// ── Reconcile ────────────────────────────────────────────────────────────────

/// What `reconcile_project_profiles` did.
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct ProfileReconcileReport {
    /// `true` when any project list or contribution record changed.
    pub changed: bool,
    /// Names in `project.profiles` whose file could not be read. Their
    /// recorded contributions are left untouched.
    pub missing_profiles: Vec<String>,
}

/// Bring a project's lists in step with its attached profiles.
///
/// - Profiles whose record is still present but which are no longer in
///   `project.profiles` are detached: the items they added are removed.
/// - For each attached profile, items it no longer lists are removed, items
///   it lists but the project lacks are added and recorded, and items the
///   project already had on its own are left alone and stay unrecorded.
/// - An item another attached profile still lists is never removed; its
///   record is handed to that profile instead.
///
/// Only touches the in-memory project. Callers persist and sync.
pub fn reconcile_project_profiles(project: &mut Project) -> ProfileReconcileReport {
    let mut report = ProfileReconcileReport::default();

    // Defensive: a duplicated name would make a profile contribute twice.
    let mut seen = std::collections::HashSet::new();
    let before = project.profiles.len();
    project.profiles.retain(|name| seen.insert(name.clone()));
    if project.profiles.len() != before {
        report.changed = true;
    }

    let mut loaded: Vec<ProjectProfile> = Vec::new();
    for name in &project.profiles {
        match read_project_profile_parsed(name) {
            Ok(profile) => loaded.push(profile),
            Err(_) => report.missing_profiles.push(name.clone()),
        }
    }

    // Detached profiles: recorded, but no longer attached.
    let detached: Vec<String> = project
        .profile_contributions
        .keys()
        .filter(|name| !project.profiles.contains(name))
        .cloned()
        .collect();
    for name in detached {
        let Some(contribution) = project.profile_contributions.remove(&name) else {
            continue;
        };
        for kind in ProfileResourceKind::ALL {
            for item in kind.contribution(&contribution) {
                release_item(project, &loaded, kind, item, &name);
            }
        }
        report.changed = true;
    }

    // Attached profiles, in attach order.
    for profile in &loaded {
        let prev = project
            .profile_contributions
            .get(&profile.name)
            .cloned()
            .unwrap_or_default();
        let mut next = ProfileContribution::default();

        for kind in ProfileResourceKind::ALL {
            let wanted = kind.profile_list(profile);

            for item in kind.contribution(&prev) {
                if !kind.contains(wanted, item) {
                    release_item(project, &loaded, kind, item, &profile.name);
                    report.changed = true;
                }
            }

            for item in wanted {
                let list = kind.project_list_mut(project);
                if !kind.contains(list, item) {
                    list.push(item.clone());
                    kind.contribution_mut(&mut next).push(item.clone());
                    report.changed = true;
                } else if kind.contains(kind.contribution(&prev), item) {
                    kind.contribution_mut(&mut next).push(item.clone());
                }
                // Otherwise the project defined it itself: leave it unrecorded
                // so it survives a later detach.
            }
        }

        if next != prev {
            report.changed = true;
        }
        if next.is_empty() {
            project.profile_contributions.remove(&profile.name);
        } else {
            project
                .profile_contributions
                .insert(profile.name.clone(), next);
        }
    }

    // Keep `file_rules` tidy: the Rules tab drops an empty `_project` key too.
    if project
        .file_rules
        .get(PROJECT_RULES_KEY)
        .is_some_and(|rules| rules.is_empty())
    {
        project.file_rules.remove(PROJECT_RULES_KEY);
    }

    report
}

/// Drop `item` from the project because `from` no longer provides it. When
/// another attached profile still lists the item it stays, and the record
/// moves to that profile.
fn release_item(
    project: &mut Project,
    loaded: &[ProjectProfile],
    kind: ProfileResourceKind,
    item: &str,
    from: &str,
) {
    let other = loaded
        .iter()
        .find(|p| p.name != from && kind.contains(kind.profile_list(p), item));

    if let Some(other) = other {
        let entry = project
            .profile_contributions
            .entry(other.name.clone())
            .or_default();
        if !kind.contains(kind.contribution(entry), item) {
            kind.contribution_mut(entry).push(item.to_string());
        }
        return;
    }

    let list = kind.project_list_mut(project);
    list.retain(|x| !kind.same(x, item));
}

// ── Library maintenance ──────────────────────────────────────────────────────
//
// Profiles reference library assets by name, so they need the same hygiene
// projects get when an asset is deleted or renamed.

/// Remove `name` from every profile's list for `kind`. Returns the profiles
/// that changed.
pub(crate) fn prune_asset_from_profiles(kind: ProfileResourceKind, name: &str) -> Vec<String> {
    rewrite_profiles(|profile| {
        let list = kind.profile_list_mut(profile);
        let before = list.len();
        list.retain(|x| !kind.same(x, name));
        list.len() != before
    })
}

/// Rename `old` to `new` in every profile's list for `kind`. When `new` is
/// already present the `old` entry is dropped instead. Returns the profiles
/// that changed.
pub(crate) fn rename_asset_in_profiles(
    kind: ProfileResourceKind,
    old: &str,
    new: &str,
) -> Vec<String> {
    rewrite_profiles(|profile| rename_in_list(kind, kind.profile_list_mut(profile), old, new))
}

fn rewrite_profiles(mut edit: impl FnMut(&mut ProjectProfile) -> bool) -> Vec<String> {
    let names = match list_project_profiles() {
        Ok(names) => names,
        Err(e) => {
            eprintln!("Failed to list profiles for library maintenance: {}", e);
            return Vec::new();
        }
    };

    let mut changed = Vec::new();
    for name in names {
        let mut profile = match read_project_profile_parsed(&name) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Failed to read profile '{}': {}", name, e);
                continue;
            }
        };
        if !edit(&mut profile) {
            continue;
        }
        match serde_json::to_string_pretty(&profile) {
            Ok(data) => match save_project_profile(&name, &data) {
                Ok(()) => changed.push(name),
                Err(e) => eprintln!("Failed to save profile '{}': {}", name, e),
            },
            Err(e) => eprintln!("Failed to serialise profile '{}': {}", name, e),
        }
    }
    changed
}

fn rename_in_list(kind: ProfileResourceKind, list: &mut Vec<String>, old: &str, new: &str) -> bool {
    if !kind.contains(list, old) {
        return false;
    }
    if kind.contains(list, new) {
        list.retain(|x| !kind.same(x, old));
    } else {
        for entry in list.iter_mut() {
            if kind.same(entry, old) {
                *entry = new.to_string();
            }
        }
    }
    true
}

/// Forget `name` in every contribution record on `project`. Used alongside
/// the `prune_*_from_projects` helpers so a deleted asset leaves no stale
/// provenance behind. Returns `true` when anything changed.
pub(crate) fn strip_contribution(
    project: &mut Project,
    kind: ProfileResourceKind,
    name: &str,
) -> bool {
    let mut changed = false;
    project.profile_contributions.retain(|_, contribution| {
        let list = kind.contribution_mut(contribution);
        let before = list.len();
        list.retain(|x| !kind.same(x, name));
        if list.len() != before {
            changed = true;
        }
        !contribution.is_empty()
    });
    changed
}

/// Rename `old` to `new` in every contribution record on `project`.
pub(crate) fn rename_contribution(
    project: &mut Project,
    kind: ProfileResourceKind,
    old: &str,
    new: &str,
) -> bool {
    let mut changed = false;
    for contribution in project.profile_contributions.values_mut() {
        if rename_in_list(kind, kind.contribution_mut(contribution), old, new) {
            changed = true;
        }
    }
    changed
}

/// Rename a profile inside a project's `profiles` list and contribution map.
pub(crate) fn rename_profile_in_project(project: &mut Project, old: &str, new: &str) -> bool {
    let mut changed = false;
    for entry in project.profiles.iter_mut() {
        if entry == old {
            *entry = new.to_string();
            changed = true;
        }
    }
    if let Some(contribution) = project.profile_contributions.remove(old) {
        project
            .profile_contributions
            .insert(new.to_string(), contribution);
        changed = true;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::paths::with_test_home;

    fn empty_project() -> Project {
        Project {
            name: "test-project".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            ..Default::default()
        }
    }

    fn write_profile(profile: &ProjectProfile) {
        let data = serde_json::to_string(profile).expect("serialise profile");
        save_project_profile(&profile.name, &data).expect("save profile");
    }

    fn profile(name: &str) -> ProjectProfile {
        ProjectProfile {
            name: name.into(),
            ..Default::default()
        }
    }

    fn rules(project: &Project) -> Vec<String> {
        project
            .file_rules
            .get(PROJECT_RULES_KEY)
            .cloned()
            .unwrap_or_default()
    }

    #[test]
    fn crud_round_trip_rename_and_delete() {
        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), || {
            assert!(list_project_profiles().expect("list").is_empty());

            let mut p = profile("baseline");
            p.description = "Shared baseline".into();
            p.skills = vec!["react".into()];
            p.rules = vec!["automatic-process".into()];
            write_profile(&p);

            assert_eq!(list_project_profiles().expect("list"), vec!["baseline"]);
            let read = read_project_profile_parsed("baseline").expect("read");
            assert_eq!(read.description, "Shared baseline");
            assert_eq!(read.skills, vec!["react"]);
            assert_eq!(read.rules, vec!["automatic-process"]);

            // The stored `name` field never overrides the file stem.
            let mut mismatched = profile("wrong-name");
            mismatched.skills = vec!["x".into()];
            let data = serde_json::to_string(&mismatched).unwrap();
            save_project_profile("stem-wins", &data).expect("save");
            assert_eq!(read_project_profile_parsed("stem-wins").unwrap().name, "stem-wins");

            rename_project_profile("baseline", "base").expect("rename");
            assert!(read_project_profile_parsed("baseline").is_err());
            assert_eq!(read_project_profile_parsed("base").unwrap().name, "base");
            assert!(rename_project_profile("base", "stem-wins").is_err());

            delete_project_profile("base").expect("delete");
            assert_eq!(list_project_profiles().expect("list"), vec!["stem-wins"]);
            assert!(save_project_profile("bad name!", "{}").is_err());
        });
    }

    #[test]
    fn read_hydrates_author_from_remote_provenance() {
        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), || {
            write_profile(&profile("remote"));
            super::super::remote_sources::record_provenance("profile", "remote", "octocat/profiles")
                .expect("record provenance");
            let read = read_project_profile_parsed("remote").expect("read");
            let author = read._author.expect("author");
            assert_eq!(author["repo"].as_str(), Some("octocat/profiles"));
        });
    }

    #[test]
    fn reconcile_adds_missing_items_and_records_them() {
        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), || {
            let mut p = profile("baseline");
            p.skills = vec!["react".into()];
            p.mcp_servers = vec!["github".into()];
            p.providers = vec!["anthropic".into()];
            p.agents = vec!["claude".into()];
            p.user_agents = vec!["reviewer".into()];
            p.user_commands = vec!["deploy".into()];
            p.hooks = vec!["lint".into()];
            p.rules = vec!["automatic-process".into()];
            write_profile(&p);

            let mut project = empty_project();
            project.profiles = vec!["baseline".into()];
            let report = reconcile_project_profiles(&mut project);

            assert!(report.changed);
            assert!(report.missing_profiles.is_empty());
            assert_eq!(project.skills, vec!["react"]);
            assert_eq!(project.mcp_servers, vec!["github"]);
            assert_eq!(project.providers, vec!["anthropic"]);
            assert_eq!(project.agents, vec!["claude"]);
            assert_eq!(project.user_agents, vec!["reviewer"]);
            assert_eq!(project.user_commands, vec!["deploy"]);
            assert_eq!(project.hooks, vec!["lint"]);
            assert_eq!(rules(&project), vec!["automatic-process"]);

            let record = &project.profile_contributions["baseline"];
            assert_eq!(record.skills, vec!["react"]);
            assert_eq!(record.mcp_servers, vec!["github"]);
            assert_eq!(record.rules, vec!["automatic-process"]);
            assert_eq!(record.hooks, vec!["lint"]);
        });
    }

    #[test]
    fn reconcile_leaves_project_own_items_unrecorded() {
        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), || {
            let mut p = profile("baseline");
            p.skills = vec!["react".into(), "vitest".into()];
            write_profile(&p);

            let mut project = empty_project();
            project.skills = vec!["react".into()];
            project.profiles = vec!["baseline".into()];
            reconcile_project_profiles(&mut project);

            assert_eq!(project.skills, vec!["react", "vitest"]);
            assert_eq!(project.profile_contributions["baseline"].skills, vec!["vitest"]);
        });
    }

    #[test]
    fn reconcile_removes_items_the_profile_dropped() {
        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), || {
            let mut p = profile("baseline");
            p.skills = vec!["react".into(), "vitest".into()];
            p.hooks = vec!["lint".into()];
            write_profile(&p);

            let mut project = empty_project();
            project.profiles = vec!["baseline".into()];
            reconcile_project_profiles(&mut project);
            assert_eq!(project.skills, vec!["react", "vitest"]);

            p.skills = vec!["react".into()];
            p.hooks.clear();
            write_profile(&p);
            let report = reconcile_project_profiles(&mut project);

            assert!(report.changed);
            assert_eq!(project.skills, vec!["react"]);
            assert!(project.hooks.is_empty());
            let record = &project.profile_contributions["baseline"];
            assert_eq!(record.skills, vec!["react"]);
            assert!(record.hooks.is_empty());
        });
    }

    #[test]
    fn detach_removes_only_what_the_profile_added() {
        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), || {
            let mut p = profile("baseline");
            p.skills = vec!["react".into(), "vitest".into()];
            p.rules = vec!["automatic-process".into()];
            write_profile(&p);

            let mut project = empty_project();
            project.skills = vec!["react".into()];
            project.profiles = vec!["baseline".into()];
            reconcile_project_profiles(&mut project);
            assert_eq!(rules(&project), vec!["automatic-process"]);

            project.profiles.clear();
            let report = reconcile_project_profiles(&mut project);

            assert!(report.changed);
            assert_eq!(project.skills, vec!["react"]);
            assert!(rules(&project).is_empty());
            assert!(!project.file_rules.contains_key(PROJECT_RULES_KEY));
            assert!(project.profile_contributions.is_empty());
        });
    }

    #[test]
    fn item_listed_by_a_second_profile_is_handed_over_not_removed() {
        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), || {
            let mut first = profile("first");
            first.skills = vec!["react".into()];
            write_profile(&first);
            let mut second = profile("second");
            second.skills = vec!["react".into()];
            write_profile(&second);

            // `first` is attached earlier, so it owns the record.
            let mut project = empty_project();
            project.profiles = vec!["first".into(), "second".into()];
            reconcile_project_profiles(&mut project);
            assert_eq!(project.profile_contributions["first"].skills, vec!["react"]);
            assert!(!project.profile_contributions.contains_key("second"));

            // Detach `first`: the skill stays and `second` takes the record.
            project.profiles = vec!["second".into()];
            reconcile_project_profiles(&mut project);
            assert_eq!(project.skills, vec!["react"]);
            assert_eq!(project.profile_contributions["second"].skills, vec!["react"]);
            assert!(!project.profile_contributions.contains_key("first"));

            // Same when the owner is attached later and drops the item.
            let mut project = empty_project();
            project.profiles = vec!["second".into(), "first".into()];
            reconcile_project_profiles(&mut project);
            assert_eq!(project.profile_contributions["second"].skills, vec!["react"]);
            second.skills.clear();
            write_profile(&second);
            reconcile_project_profiles(&mut project);
            assert_eq!(project.skills, vec!["react"]);
            assert_eq!(project.profile_contributions["first"].skills, vec!["react"]);
            assert!(!project.profile_contributions.contains_key("second"));
        });
    }

    #[test]
    fn missing_profile_is_reported_and_leaves_state_alone() {
        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), || {
            let mut p = profile("baseline");
            p.skills = vec!["react".into()];
            write_profile(&p);

            let mut project = empty_project();
            project.profiles = vec!["baseline".into()];
            reconcile_project_profiles(&mut project);

            delete_project_profile("baseline").expect("delete");
            let report = reconcile_project_profiles(&mut project);

            assert_eq!(report.missing_profiles, vec!["baseline"]);
            assert!(!report.changed);
            assert_eq!(project.skills, vec!["react"]);
            assert_eq!(project.profile_contributions["baseline"].skills, vec!["react"]);
        });
    }

    #[test]
    fn mcp_server_membership_ignores_case() {
        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), || {
            let mut p = profile("baseline");
            p.mcp_servers = vec!["Sentry".into()];
            write_profile(&p);

            let mut project = empty_project();
            project.mcp_servers = vec!["sentry".into()];
            project.profiles = vec!["baseline".into()];
            reconcile_project_profiles(&mut project);

            assert_eq!(project.mcp_servers, vec!["sentry"]);
            assert!(!project.profile_contributions.contains_key("baseline"));
        });
    }

    #[test]
    fn reconcile_is_idempotent_and_dedupes_the_profiles_list() {
        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), || {
            let mut p = profile("baseline");
            p.skills = vec!["react".into()];
            write_profile(&p);

            let mut project = empty_project();
            project.profiles = vec!["baseline".into(), "baseline".into()];
            let first = reconcile_project_profiles(&mut project);
            assert!(first.changed);
            assert_eq!(project.profiles, vec!["baseline"]);
            assert_eq!(project.skills, vec!["react"]);

            let snapshot = project.clone();
            let second = reconcile_project_profiles(&mut project);
            assert!(!second.changed);
            assert_eq!(project.skills, snapshot.skills);
            assert_eq!(project.profile_contributions, snapshot.profile_contributions);
        });
    }

    #[test]
    fn prune_and_rename_rewrite_profile_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), || {
            let mut a = profile("a");
            a.skills = vec!["react".into(), "vitest".into()];
            a.mcp_servers = vec!["GitHub".into(), "sentry".into()];
            write_profile(&a);
            let mut b = profile("b");
            b.skills = vec!["python".into()];
            write_profile(&b);

            let changed = prune_asset_from_profiles(ProfileResourceKind::Skill, "react");
            assert_eq!(changed, vec!["a"]);
            assert_eq!(read_project_profile_parsed("a").unwrap().skills, vec!["vitest"]);
            assert_eq!(read_project_profile_parsed("b").unwrap().skills, vec!["python"]);

            let changed = rename_asset_in_profiles(ProfileResourceKind::McpServer, "github", "gh");
            assert_eq!(changed, vec!["a"]);
            assert_eq!(
                read_project_profile_parsed("a").unwrap().mcp_servers,
                vec!["gh", "sentry"]
            );

            // Renaming onto an existing entry collapses the old one.
            let changed = rename_asset_in_profiles(ProfileResourceKind::McpServer, "gh", "sentry");
            assert_eq!(changed, vec!["a"]);
            assert_eq!(read_project_profile_parsed("a").unwrap().mcp_servers, vec!["sentry"]);

            assert!(prune_asset_from_profiles(ProfileResourceKind::Hook, "nothing").is_empty());
        });
    }

    #[test]
    fn contribution_helpers_strip_rename_and_move_records() {
        let mut project = empty_project();
        project.profiles = vec!["baseline".into()];
        project.profile_contributions.insert(
            "baseline".into(),
            ProfileContribution {
                skills: vec!["react".into()],
                mcp_servers: vec!["github".into()],
                ..Default::default()
            },
        );

        assert!(rename_contribution(&mut project, ProfileResourceKind::McpServer, "github", "gh"));
        assert_eq!(project.profile_contributions["baseline"].mcp_servers, vec!["gh"]);
        assert!(!rename_contribution(&mut project, ProfileResourceKind::Skill, "none", "x"));

        assert!(strip_contribution(&mut project, ProfileResourceKind::Skill, "react"));
        assert_eq!(project.profile_contributions["baseline"].mcp_servers, vec!["gh"]);
        assert!(strip_contribution(&mut project, ProfileResourceKind::McpServer, "gh"));
        assert!(project.profile_contributions.is_empty());

        project.profile_contributions.insert("baseline".into(), ProfileContribution::default());
        assert!(rename_profile_in_project(&mut project, "baseline", "base"));
        assert_eq!(project.profiles, vec!["base"]);
        assert!(project.profile_contributions.contains_key("base"));
        assert!(!rename_profile_in_project(&mut project, "missing", "x"));
    }
}
