use std::fs;
use std::path::{Path, PathBuf};

use super::project_registry::RegistryEntry;
use super::remote_sources::fold_name;
use super::*;

// ── Profile confirmation ─────────────────────────────────────────────────────
//
// `profiles` arrives in a committed `.automatic.json`, so a repository can
// name any profile in this machine's library. A name is applied only once this
// checkout has confirmed it, or a group this checkout joined provides it.
// Until then it is pending, and reconcile treats it like a missing profile:
// nothing is added, removed or rewritten.
//
// The decisions live only in the registry pointer, like `local_read_only`, so
// no file in a repository can grant one. Group members are stored by project
// `id`, and a repository chooses its committed `id`, so listing a checkout is
// not enough: it must have joined on this machine. A group record is no proof
// either, because a repository can commit `.automatic/project.json`.

const CONFIRMED_FIELD: &str = "local_confirmed_profiles";
const IGNORED_FIELD: &str = "local_ignored_profiles";
const GROUPS_FIELD: &str = "local_confirmed_groups";
const DEPARTED_FIELD: &str = "local_departed_groups";
const RELEASING_FIELD: &str = "local_releasing_profiles";
const CONFIRMED_CONTEXTS_FIELD: &str = "local_confirmed_contexts";
const IGNORED_CONTEXTS_FIELD: &str = "local_ignored_contexts";
const MIGRATION_MARKER: &str = "profiles_confirmed_migrated";
const CONTEXT_MIGRATION_MARKER: &str = "contexts_confirmed_migrated";

/// What this checkout has decided about the profile names it was given.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ProfileDecisions {
    confirmed: Vec<String>,
    ignored: Vec<String>,
    groups: Vec<String>,
    departed: Vec<String>,
    releasing: Vec<String>,
    confirmed_contexts: Vec<String>,
    ignored_contexts: Vec<String>,
}

impl ProfileDecisions {
    fn of_pointer(pointer: &serde_json::Value) -> Self {
        Self {
            confirmed: pointer_names(pointer, CONFIRMED_FIELD),
            ignored: pointer_names(pointer, IGNORED_FIELD),
            groups: pointer_names(pointer, GROUPS_FIELD),
            departed: pointer_names(pointer, DEPARTED_FIELD),
            releasing: pointer_names(pointer, RELEASING_FIELD),
            confirmed_contexts: pointer_names(pointer, CONFIRMED_CONTEXTS_FIELD),
            ignored_contexts: pointer_names(pointer, IGNORED_CONTEXTS_FIELD),
        }
    }

    /// The decisions in a pointer file, read afresh so a writer keeps a change
    /// another process made after it read the project. `None` for a missing
    /// or unparseable file.
    fn of_file(path: &Path) -> Option<Self> {
        let raw = fs::read_to_string(path).ok()?;
        let pointer: serde_json::Value = serde_json::from_str(&raw).ok()?;
        Some(Self::of_pointer(&pointer))
    }

    fn of_entry(entry: &RegistryEntry) -> Self {
        entry.contents.as_ref().map(Self::of_pointer).unwrap_or_default()
    }

    pub(crate) fn of_project(project: &Project) -> Self {
        Self {
            confirmed: project.local_confirmed_profiles.clone(),
            ignored: project.local_ignored_profiles.clone(),
            groups: project.local_confirmed_groups.clone(),
            departed: project.local_departed_groups.clone(),
            releasing: project.local_releasing_profiles.clone(),
            confirmed_contexts: project.local_confirmed_contexts.clone(),
            ignored_contexts: project.local_ignored_contexts.clone(),
        }
    }

    pub(crate) fn apply_to(self, project: &mut Project) {
        project.local_confirmed_profiles = self.confirmed;
        project.local_ignored_profiles = self.ignored;
        project.local_confirmed_groups = self.groups;
        project.local_departed_groups = self.departed;
        project.local_releasing_profiles = self.releasing;
        project.local_confirmed_contexts = self.confirmed_contexts;
        project.local_ignored_contexts = self.ignored_contexts;
    }

    fn write_to(&self, map: &mut serde_json::Map<String, serde_json::Value>) {
        for (field, names) in [
            (CONFIRMED_FIELD, &self.confirmed),
            (IGNORED_FIELD, &self.ignored),
            (GROUPS_FIELD, &self.groups),
            (DEPARTED_FIELD, &self.departed),
            (RELEASING_FIELD, &self.releasing),
            (CONFIRMED_CONTEXTS_FIELD, &self.confirmed_contexts),
            (IGNORED_CONTEXTS_FIELD, &self.ignored_contexts),
        ] {
            if names.is_empty() {
                map.remove(field);
            } else {
                map.insert(field.into(), names.clone().into());
            }
        }
    }
}

fn pointer_names(pointer: &serde_json::Value, field: &str) -> Vec<String> {
    pointer
        .get(field)
        .and_then(|v| v.as_array())
        .map(|names| {
            names
                .iter()
                .filter_map(|name| name.as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Put `project`'s decisions into a pointer being built.
pub(crate) fn write_profile_decisions(project: &Project, pointer: &mut serde_json::Value) {
    if let Some(map) = pointer.as_object_mut() {
        ProfileDecisions::of_project(project).write_to(map);
    }
}

/// Keep the decisions already in the pointer file at `path`. `pointer` was
/// built from a project that may have been read before they changed, and only
/// the functions in this file change them.
pub(crate) fn keep_stored_profile_decisions(path: &Path, pointer: &mut serde_json::Value) {
    let Some(stored) = ProfileDecisions::of_file(path) else {
        return;
    };
    if let Some(map) = pointer.as_object_mut() {
        stored.write_to(map);
    }
}

/// Set `project`'s decisions and `local_key` from the stored entry for
/// `ident`, so stale or edited data cannot change them. The editor's copy can
/// lack the key, and its name may belong to another checkout.
///
/// With no entry, only a project being created keeps its names: the user chose
/// them in the app.
pub fn load_profile_decisions(
    ident: &str,
    project: &mut Project,
    creating: bool,
) -> Result<(), String> {
    match resolve_registry_entry(ident)? {
        Some(entry) => {
            ProfileDecisions::of_entry(&entry).apply_to(project);
            project.local_key = entry.local_key().unwrap_or_default().to_string();
        }
        None => {
            ProfileDecisions::default().apply_to(project);
            if creating {
                project.local_confirmed_profiles =
                    project.profiles.iter().map(|name| canonical_profile_name(name)).collect();
                project.local_confirmed_contexts = project.contexts.clone();
            }
        }
    }
    Ok(())
}

/// Set `project`'s decisions from `entry`. A new project has no entry, and
/// keeps what its creator gave it.
pub(crate) fn keep_entry_profile_decisions(entry: Option<&RegistryEntry>, project: &mut Project) {
    if let Some(entry) = entry {
        ProfileDecisions::of_entry(entry).apply_to(project);
    }
}

// ── Pending names ────────────────────────────────────────────────────────────

/// The groups `project` has joined on this machine that still list it. Group
/// files are read only for a checkout that joined one. The checkout is found
/// by its `local_key`, never by its name or committed `id`.
pub(crate) fn joined_groups(project: &Project) -> Vec<ProjectGroup> {
    if project.local_confirmed_groups.is_empty() || project.local_key.is_empty() {
        return Vec::new();
    }
    let mut groups = groups_for_project(&project.local_key);
    groups.retain(|group| project.local_confirmed_groups.contains(&group.name));
    groups
}

fn same_profile(a: &str, b: &str) -> bool {
    fold_name(a) == fold_name(b)
}

/// The library's spelling of `name`. Profiles are files, so on a
/// case-insensitive filesystem `Base` opens `base.json`.
fn canonical_profile_name(name: &str) -> String {
    let Ok(names) = list_project_profiles() else {
        return name.to_string();
    };
    if names.iter().any(|n| n == name) {
        return name.to_string();
    }
    names
        .into_iter()
        .find(|n| same_profile(n, name))
        .unwrap_or_else(|| name.to_string())
}

pub(crate) fn is_confirmed(confirmed: &[String], name: &str) -> bool {
    confirmed.iter().any(|n| n == name) || confirmed.contains(&canonical_profile_name(name))
}

/// Names in `project.profiles` that this checkout has not confirmed and that
/// no group it joined provides.
pub(crate) fn unconfirmed_profiles(project: &Project) -> Vec<String> {
    let mut names: Vec<String> = project
        .profiles
        .iter()
        .filter(|name| !is_confirmed(&project.local_confirmed_profiles, name))
        .cloned()
        .collect();
    if names.is_empty() {
        return names;
    }
    let groups = joined_groups(project);
    names.retain(|name| !groups.iter().any(|group| group.profiles.contains(name)));
    names
}

/// Accept `profile` on the in-memory `project`, for the reconcile of an
/// attach. [`confirm_project_profile`] records it once the attach is saved, so
/// a failure in between leaves the name pending.
pub fn accept_profile(project: &mut Project, profile: &str) {
    add_name(&mut project.local_confirmed_profiles, &canonical_profile_name(profile));
}

/// Whether `name` is listed by `project` but not applied on this checkout.
pub fn profile_is_unconfirmed(project: &Project, name: &str) -> bool {
    project.profiles.iter().any(|p| p == name)
        && unconfirmed_profiles(project).iter().any(|p| p == name)
}

/// Unconfirmed names this machine has a profile for. A name with no profile
/// is left out: it is reported as missing, and there is nothing to attach.
pub(crate) fn pending_profile_names(project: &Project) -> Vec<String> {
    unconfirmed_profiles(project)
        .into_iter()
        .filter(|name| read_project_profile_parsed(name).is_ok())
        .collect()
}

/// Slugs in `project.contexts` that this checkout has not confirmed and that
/// no group it joined lists or records. Like [`unconfirmed_profiles`], this
/// reads the groups leniently, for a notice. What agents are offered comes
/// from `project_context_entries`, which fails closed.
pub(crate) fn pending_context_slugs(project: &Project) -> Vec<String> {
    let mut slugs: Vec<String> = project
        .contexts
        .iter()
        .filter(|slug| !project.local_confirmed_contexts.contains(slug))
        .cloned()
        .collect();
    if slugs.is_empty() {
        return slugs;
    }
    let groups = joined_groups(project);
    slugs.retain(|slug| {
        !groups.iter().any(|group| {
            group.contexts.contains(slug)
                || project
                    .group_context_contributions
                    .get(&group.name)
                    .is_some_and(|recorded| recorded.contains(slug))
        })
    });
    slugs.retain(|slug| read_context(slug).is_ok());
    slugs
}

// ── Decisions ────────────────────────────────────────────────────────────────

/// Edit the pointer at `path`, read afresh so a change another process made
/// since a scan is kept.
fn edit_pointer_file(
    path: &Path,
    name: &str,
    edit: impl FnOnce(&mut ProfileDecisions),
) -> Result<(), String> {
    let raw = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    let mut value: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| format!("Invalid project registry data for '{}': {}", name, e))?;
    let before = ProfileDecisions::of_pointer(&value);
    let mut after = before.clone();
    edit(&mut after);
    if after == before {
        return Ok(());
    }
    let map = value
        .as_object_mut()
        .ok_or_else(|| format!("{} does not contain a JSON object", path.display()))?;
    after.write_to(map);
    let pretty = serde_json::to_string_pretty(&value)
        .map_err(|e| format!("Failed to serialize registry entry for '{}': {}", name, e))?;
    write_registry_file(path, &pretty)
}

/// Edit the decisions of the project with identifier `ident`. Writes only the
/// registry pointer, so it works while the project is read-only.
fn edit_decisions(
    ident: &str,
    edit: impl FnOnce(&mut ProfileDecisions),
) -> Result<(), String> {
    if !is_valid_name(ident) {
        return Err("Invalid project name".into());
    }
    let _writes = super::hold_library_writes();
    let entry =
        resolve_registry_entry(ident)?.ok_or_else(|| format!("Project '{}' not found", ident))?;
    edit_pointer_file(&entry.path, &entry.name, edit)
}

fn add_name(names: &mut Vec<String>, name: &str) {
    if !names.iter().any(|n| n == name) {
        names.push(name.to_string());
    }
}

fn rename_in(names: &mut Vec<String>, old: &str, new: &str) {
    if names.iter().any(|name| same_profile(name, old)) {
        names.retain(|name| !same_profile(name, old));
        add_name(names, new);
    }
}

fn forget_profile(decisions: &mut ProfileDecisions, profile: &str) {
    decisions.confirmed.retain(|name| !same_profile(name, profile));
    decisions.ignored.retain(|name| !same_profile(name, profile));
}

/// Record that this checkout accepts `profile`, in the library's spelling.
/// Clears any Ignore.
pub fn confirm_project_profile(ident: &str, profile: &str) -> Result<(), String> {
    let canonical = canonical_profile_name(profile);
    edit_decisions(ident, |decisions| {
        add_name(&mut decisions.confirmed, &canonical);
        decisions.ignored.retain(|name| !same_profile(name, profile));
    })
}

/// Hide the notice for `profile` on this checkout. The name stays pending, so
/// the committed config is left as it is.
pub fn ignore_project_profile(ident: &str, profile: &str) -> Result<(), String> {
    if !is_valid_name(profile) {
        return Err("Invalid profile name".into());
    }
    edit_decisions(ident, |decisions| add_name(&mut decisions.ignored, profile))
}

/// Drop this checkout's decisions about `profile`, on its way to a detach. It
/// goes by the case folding the library lookup uses, so no spelling survives.
pub fn forget_project_profile(ident: &str, profile: &str) -> Result<(), String> {
    edit_decisions(ident, |decisions| forget_profile(decisions, profile))
}

/// Record that this checkout accepts the context `slug`. Clears any Ignore.
pub fn confirm_project_context(ident: &str, slug: &str) -> Result<(), String> {
    edit_decisions(ident, |decisions| {
        add_name(&mut decisions.confirmed_contexts, slug);
        decisions.ignored_contexts.retain(|name| name != slug);
    })
}

/// Hide the notice for the pending context `slug` on this checkout. It stays
/// pending, so the committed config is left as it is.
pub fn ignore_project_context(ident: &str, slug: &str) -> Result<(), String> {
    if !is_valid_context_slug(slug) {
        return Err(format!("Invalid context slug '{}'", slug));
    }
    edit_decisions(ident, |decisions| add_name(&mut decisions.ignored_contexts, slug))
}

/// Drop this checkout's decisions about the context `slug`.
pub fn forget_project_context(ident: &str, slug: &str) -> Result<(), String> {
    edit_decisions(ident, |decisions| forget_context(decisions, slug))
}

fn forget_context(decisions: &mut ProfileDecisions, slug: &str) {
    decisions.confirmed_contexts.retain(|name| name != slug);
    decisions.ignored_contexts.retain(|name| name != slug);
}

/// What an editor save changed in `contexts`, against the stored list. Only a
/// difference counts: a save carries the whole list, so a slug the committed
/// config already holds is not confirmed by being sent back.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ContextEdits {
    added: Vec<String>,
    removed: Vec<String>,
}

/// Compare `requested` with the list stored for `ident`. A project with nothing
/// stored has no edits: the create path confirms what its creator chose.
pub fn context_edits(ident: &str, requested: &[String]) -> ContextEdits {
    let Some(stored) = read_project(ident)
        .ok()
        .and_then(|raw| serde_json::from_str::<Project>(&raw).ok())
    else {
        return ContextEdits::default();
    };
    ContextEdits {
        added: requested.iter().filter(|s| !stored.contexts.contains(s)).cloned().collect(),
        removed: stored.contexts.iter().filter(|s| !requested.contains(s)).cloned().collect(),
    }
}

/// Record `edits` once the save is done, so a failed save decides nothing. A
/// slug a group provides is left out: leaving the group must remove it.
pub fn record_context_edits(ident: &str, edits: &ContextEdits) -> Result<(), String> {
    if edits == &ContextEdits::default() {
        return Ok(());
    }
    let raw = read_project(ident)?;
    let project: Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project '{}': {}", ident, e))?;
    edit_decisions(ident, |decisions| {
        for slug in edits.added.iter().filter(|s| group_providing(&project, s).is_none()) {
            add_name(&mut decisions.confirmed_contexts, slug);
            decisions.ignored_contexts.retain(|name| name != slug);
        }
        for slug in &edits.removed {
            forget_context(decisions, slug);
        }
    })
}

/// Record that this checkout joined `group` on this machine.
pub(crate) fn confirm_project_group(ident: &str, group: &str) -> Result<(), String> {
    edit_decisions(ident, |decisions| add_name(&mut decisions.groups, group))
}

/// Record that this checkout, which `group` already lists through a shared
/// `id`, joins it. Refused for a group that does not list it.
pub fn join_listed_group(ident: &str, group: &str) -> Result<(), String> {
    let _writes = super::hold_library_writes();
    let entry =
        resolve_registry_entry(ident)?.ok_or_else(|| format!("Project '{}' not found", ident))?;
    let key = entry.local_key().unwrap_or(ident);
    if !groups_for_project(key).iter().any(|g| g.name == group) {
        return Err(format!("Group '{}' does not list project '{}'", group, entry.name));
    }
    confirm_project_group(key, group)
}

/// Mark `profile` as released by `group` on every checkout that joined it, so
/// the reconcile after the group drops the name can tell this from a forged
/// record. Call before the group changes.
pub fn mark_group_profile_releasing(group: &str, profile: &str) -> Result<(), String> {
    let _writes = super::hold_library_writes();
    for member in group_member_idents(group) {
        let Some(entry) = resolve_registry_entry(&member)? else {
            continue;
        };
        edit_pointer_file(&entry.path, &entry.name, |decisions| {
            if decisions.groups.iter().any(|g| g == group) {
                add_name(&mut decisions.releasing, profile);
            }
        })?;
    }
    Ok(())
}

/// Tidy `ident`'s joins after its reconcile. A join of a group that no longer
/// lists it is dropped, so a later group of that name is not trusted. While
/// the project still records what that group provided, the reconcile failed
/// or was skipped, so the group stays marked as departed and its next
/// reconcile can release those names.
pub(crate) fn settle_group_joins(ident: &str) -> Result<(), String> {
    let _writes = super::hold_library_writes();
    let Some(entry) = resolve_registry_entry(ident)? else {
        return Ok(());
    };
    let stored = ProfileDecisions::of_entry(&entry);
    if stored.groups.is_empty() && stored.departed.is_empty() && stored.releasing.is_empty() {
        return Ok(());
    }
    let key = entry.local_key().unwrap_or(ident).to_string();
    let listing = match group_names_listing(&key) {
        Ok(listing) => listing,
        Err(e) => {
            eprintln!("Could not settle the group joins of '{}': {}", entry.name, e);
            return Ok(());
        }
    };
    // An unreadable project may still hold the records, so keep the marks.
    let project: Option<Project> = read_project(&key)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok());
    let recorded = |group: &str| {
        project
            .as_ref()
            .is_none_or(|p| p.group_profile_contributions.contains_key(group))
    };
    // A mark waits for the record of a group that stopped listing the name.
    let held = |name: &str| {
        project.as_ref().is_none_or(|p| {
            p.group_profile_contributions.iter().any(|(group, names)| {
                names.iter().any(|n| n == name) && !group_lists_profile(group, name)
            })
        })
    };
    edit_pointer_file(&entry.path, &entry.name, |decisions| {
        let (kept, left): (Vec<String>, Vec<String>) = decisions
            .groups
            .drain(..)
            .partition(|group| listing.contains(group));
        decisions.groups = kept;
        for group in left.iter().filter(|group| recorded(group)) {
            add_name(&mut decisions.departed, group);
        }
        let joined = decisions.groups.clone();
        decisions.departed.retain(|group| recorded(group) && !joined.contains(group));
        decisions.releasing.retain(|name| held(name));
    })
}

/// Run [`settle_group_joins`] on every project with something to settle.
/// Read-only projects are included. Returns the name and error of each one
/// that could not be settled.
pub fn settle_all_group_joins() -> Vec<(String, String)> {
    let _writes = super::hold_library_writes();
    let entries = match scan_registry() {
        Ok(entries) => entries,
        Err(e) => return vec![("Project list".to_string(), e)],
    };
    entries
        .iter()
        .filter(|entry| entry.contents.is_ok())
        .filter_map(|entry| {
            let ident = entry.local_key().unwrap_or(&entry.name);
            settle_group_joins(ident).err().map(|e| (entry.name.clone(), e))
        })
        .collect()
}

/// Edit the decisions of every project, one pointer at a time. Read-only
/// projects are included, because this writes only pointers. Returns the name
/// and error of each entry that could not be changed.
fn edit_all_decisions(edit: impl Fn(&mut ProfileDecisions)) -> Vec<(String, String)> {
    let _writes = super::hold_library_writes();
    let entries = match scan_registry() {
        Ok(entries) => entries,
        Err(e) => return vec![("Project list".to_string(), e)],
    };
    entries
        .iter()
        .filter(|entry| entry.contents.is_ok())
        .filter_map(|entry| {
            edit_pointer_file(&entry.path, &entry.name, &edit)
                .err()
                .map(|e| (entry.name.clone(), e))
        })
        .collect()
}

/// Drop every project's decisions about a deleted profile, so a later profile
/// with the same name is not trusted.
pub fn forget_profile_everywhere(profile: &str) -> Vec<(String, String)> {
    edit_all_decisions(|decisions| forget_profile(decisions, profile))
}

/// Drop every project's decisions about a deleted context, so a later context
/// with the same slug is not trusted.
pub fn forget_context_everywhere(slug: &str) -> Vec<(String, String)> {
    edit_all_decisions(|decisions| forget_context(decisions, slug))
}

/// Carry every project's decisions over to a renamed context.
pub fn rename_context_decisions(old: &str, new: &str) -> Vec<(String, String)> {
    let rename = |names: &mut Vec<String>| {
        if names.iter().any(|name| name == old) {
            names.retain(|name| name != old);
            add_name(names, new);
        }
    };
    edit_all_decisions(|decisions| {
        rename(&mut decisions.confirmed_contexts);
        rename(&mut decisions.ignored_contexts);
    })
}

/// Carry every project's decisions over to a renamed profile.
pub fn rename_profile_decisions(old: &str, new: &str) -> Vec<(String, String)> {
    edit_all_decisions(|decisions| {
        rename_in(&mut decisions.confirmed, old, new);
        rename_in(&mut decisions.ignored, old, new);
        // Added, not moved: a project the rename skipped still records the old
        // name, and its next reconcile must release that too.
        if decisions.releasing.iter().any(|name| same_profile(name, old)) {
            add_name(&mut decisions.releasing, new);
        }
    })
}

// ── Migration ────────────────────────────────────────────────────────────────

fn migration_marker_path() -> Result<PathBuf, String> {
    Ok(get_automatic_dir()?.join(MIGRATION_MARKER))
}

fn attached_profile_names(entry: &RegistryEntry) -> Result<Vec<String>, String> {
    match entry.directory()? {
        Some(directory) => Ok(read_project_files(&directory, &entry.name)?
            .map(|project| project.profiles)
            .unwrap_or_default()),
        None => Ok(entry
            .contents
            .as_ref()
            .map(|value| pointer_names(value, "profiles"))
            .unwrap_or_default()),
    }
}

/// Confirm every name attached to every registered project and record every
/// group that lists it, once. Without this, projects from before confirmation
/// existed would stop tracking their profiles. Writes only pointers, so
/// read-only projects are covered.
///
/// A project whose folder cannot be read is skipped. Its names show as pending
/// with a notice, which loses nothing. A pointer that cannot be written is an
/// error and leaves the marker unset, so the next start tries again.
pub fn confirm_attached_profiles_once() -> Result<(), String> {
    let marker = migration_marker_path()?;
    if marker.exists() {
        return Ok(());
    }
    let _writes = super::hold_library_writes();
    let mut failed = Vec::new();
    for entry in scan_registry()? {
        let ident = entry.local_key().unwrap_or(&entry.name);
        let groups: Vec<String> = groups_for_project(ident).into_iter().map(|g| g.name).collect();
        let confirmed = attached_profile_names(&entry).unwrap_or_else(|e| {
            eprintln!("Could not read the profiles of '{}': {}", entry.name, e);
            Vec::new()
        });
        let confirmed = edit_pointer_file(&entry.path, &entry.name, |decisions| {
            for name in &confirmed {
                add_name(&mut decisions.confirmed, &canonical_profile_name(name));
            }
            for group in &groups {
                add_name(&mut decisions.groups, group);
            }
        });
        if let Err(e) = confirmed {
            failed.push(format!("'{}': {}", entry.name, e));
        }
    }
    if !failed.is_empty() {
        return Err(format!("Could not confirm the profiles of {}", failed.join("; ")));
    }
    match fs::OpenOptions::new().write(true).create_new(true).open(&marker) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(format!("Could not write {}: {}", marker.display(), e)),
    }
}

/// The slugs `entry` attached itself. A slug a group recorded is left out: a
/// joined group already provides it, and confirming it would outlive the join.
fn attached_context_slugs(entry: &RegistryEntry) -> Result<Vec<String>, String> {
    let Some(directory) = entry.directory()? else {
        return Ok(entry
            .contents
            .as_ref()
            .map(|value| pointer_names(value, "contexts"))
            .unwrap_or_default());
    };
    let Some(project) = read_project_files(&directory, &entry.name)? else {
        return Ok(Vec::new());
    };
    let provided = |slug: &String| {
        project.group_context_contributions.values().any(|slugs| slugs.contains(slug))
    };
    Ok(project.contexts.iter().filter(|slug| !provided(slug)).cloned().collect())
}

/// Confirm every slug attached to every registered checkout, once. Without
/// this, projects from before confirmation existed would lose their contexts.
/// Writes only pointers, so read-only projects are covered. A project whose
/// folder cannot be read is skipped, and its slugs show as pending. A pointer
/// that cannot be written is an error and leaves the marker unset.
pub fn confirm_attached_contexts_once() -> Result<(), String> {
    let marker = get_automatic_dir()?.join(CONTEXT_MIGRATION_MARKER);
    if marker.exists() {
        return Ok(());
    }
    let _writes = super::hold_library_writes();
    let mut failed = Vec::new();
    for entry in scan_registry()? {
        let slugs = attached_context_slugs(&entry).unwrap_or_else(|e| {
            eprintln!("Could not read the contexts of '{}': {}", entry.name, e);
            Vec::new()
        });
        let confirmed = edit_pointer_file(&entry.path, &entry.name, |decisions| {
            for slug in &slugs {
                add_name(&mut decisions.confirmed_contexts, slug);
            }
        });
        if let Err(e) = confirmed {
            failed.push(format!("'{}': {}", entry.name, e));
        }
    }
    if !failed.is_empty() {
        return Err(format!("Could not confirm the contexts of {}", failed.join("; ")));
    }
    match fs::OpenOptions::new().write(true).create_new(true).open(&marker) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(format!("Could not write {}: {}", marker.display(), e)),
    }
}

/// A project's registry pointer as stored.
#[cfg(test)]
pub(crate) fn resolve_project_pointer_for_test(ident: &str) -> serde_json::Value {
    resolve_registry_entry(ident)
        .expect("resolve")
        .expect("entry")
        .contents
        .expect("contents")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::paths::with_test_home;
    use std::collections::BTreeMap;

    fn with_home(test: impl FnOnce(&Path)) {
        let home = tempfile::tempdir().expect("home");
        with_test_home(home.path().to_path_buf(), || test(home.path()));
    }

    fn save_profile(name: &str, skills: &[&str]) {
        let profile = ProjectProfile {
            name: name.to_string(),
            skills: skills.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        };
        save_project_profile(name, &serde_json::to_string(&profile).expect("json"))
            .expect("save profile");
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_pointer_edit_leaves_the_migration_to_run_again() {
        use std::os::unix::fs::PermissionsExt;
        with_home(|_| {
            let local = Project { name: "local".into(), ..Default::default() };
            save_project("local", &serde_json::to_string(&local).expect("json")).expect("save");
            save_group("team", r#"{"name":"team","projects":["local"]}"#).expect("group");
            let pointers = get_projects_dir().expect("dir");
            let chmod = |mode| {
                for entry in fs::read_dir(&pointers).expect("read").flatten() {
                    fs::set_permissions(entry.path(), fs::Permissions::from_mode(mode)).expect("chmod");
                }
                let dir = fs::Permissions::from_mode(mode | 0o111);
                fs::set_permissions(&pointers, dir).expect("chmod");
            };

            chmod(0o444);
            let failed = confirm_attached_profiles_once();
            chmod(0o644);

            assert!(failed.is_err());
            assert!(!migration_marker_path().unwrap().exists());
            confirm_attached_profiles_once().expect("retry");
            assert!(migration_marker_path().unwrap().exists());
        });
    }

    /// A folder as a clone leaves it, imported the way the app imports it.
    fn import_clone(home: &Path, config: serde_json::Value) -> (PathBuf, String) {
        let dir = home.join("clone");
        fs::create_dir_all(&dir).expect("mkdir");
        fs::write(dir.join(".automatic.json"), config.to_string()).expect("write config");
        let imported =
            import_existing_project(dir.to_str().expect("utf-8"), None).expect("import");
        (dir, imported.local_key)
    }

    fn read(ident: &str) -> Project {
        serde_json::from_str(&read_project(ident).expect("read")).expect("parse")
    }

    fn registry_json(ident: &str) -> serde_json::Value {
        resolve_project_pointer_for_test(ident)
    }

    #[test]
    fn a_committed_confirmation_is_ignored() {
        with_home(|home| {
            save_profile("baseline", &["react"]);
            let (dir, key) = import_clone(
                home,
                serde_json::json!({
                    "name": "demo",
                    "profiles": ["baseline"],
                    "local_confirmed_profiles": ["baseline"],
                }),
            );
            // A repository can commit the state file too.
            fs::create_dir_all(dir.join(".automatic")).expect("mkdir");
            fs::write(
                dir.join(".automatic/project.json"),
                r#"{"local_confirmed_profiles":["baseline"]}"#,
            )
            .expect("write state");

            let project = read(&key);
            assert!(project.local_confirmed_profiles.is_empty());
            assert_eq!(project.pending_profiles, vec!["baseline"]);

            let mut reconciled = project.clone();
            let report = reconcile_project_profiles(&mut reconciled);
            assert_eq!(report.pending_profiles, vec!["baseline"]);
            assert!(reconciled.skills.is_empty());
        });
    }

    #[test]
    fn a_pending_profile_changes_nothing_and_reports_its_name() {
        with_home(|_| {
            save_profile("baseline", &["react"]);
            let mut project = Project {
                name: "demo".into(),
                skills: vec!["theirs".into()],
                profiles: vec!["baseline".into()],
                // The committed record disagrees with the local profile.
                profile_contributions: BTreeMap::from([(
                    "baseline".to_string(),
                    ProfileContribution {
                        skills: vec!["theirs".into()],
                        ..Default::default()
                    },
                )]),
                ..Default::default()
            };
            let as_json = |p: &Project| serde_json::to_value(p).expect("json");
            let before = as_json(&project);

            let report = reconcile_project_profiles(&mut project);

            assert_eq!(as_json(&project), before);
            assert!(!report.changed);
            assert_eq!(report.pending_profiles, vec!["baseline"]);
            assert!(report.missing_profiles.is_empty());
        });
    }

    #[test]
    fn a_name_with_no_local_profile_is_missing_and_not_pending() {
        with_home(|_| {
            let mut project = Project {
                name: "demo".into(),
                profiles: vec!["ghost".into()],
                ..Default::default()
            };
            let report = reconcile_project_profiles(&mut project);
            assert_eq!(report.missing_profiles, vec!["ghost"]);
            assert!(report.pending_profiles.is_empty());
            assert!(pending_profile_names(&project).is_empty());
        });
    }

    #[test]
    fn a_forged_group_record_does_not_confirm_a_profile() {
        with_home(|_| {
            save_profile("baseline", &["react"]);
            let mut project = Project {
                name: "demo".into(),
                profiles: vec!["baseline".into()],
                group_profile_contributions: BTreeMap::from([(
                    "forged".to_string(),
                    vec!["baseline".to_string()],
                )]),
                ..Default::default()
            };

            let report = reconcile_project_profiles(&mut project);

            assert_eq!(report.pending_profiles, vec!["baseline"]);
            assert!(project.skills.is_empty());
        });
    }

    #[test]
    fn a_profile_a_group_provides_needs_no_confirmation() {
        with_home(|home| {
            save_profile("baseline", &["react"]);
            let (_dir, key) = import_clone(
                home,
                serde_json::json!({ "name": "demo", "profiles": ["baseline"] }),
            );
            assert_eq!(read(&key).pending_profiles, vec!["baseline"]);

            let group = serde_json::json!({
                "name": "team",
                "projects": [key],
                "profiles": ["baseline"],
            });
            save_group("team", &group.to_string()).expect("save group");

            confirm_project_group(&key, "team").expect("join");

            let mut project = read(&key);
            assert_eq!(project.local_confirmed_groups, vec!["team"]);
            assert!(project.pending_profiles.is_empty());
            assert!(reconcile_project_profiles(&mut project).changed);
            assert_eq!(project.skills, vec!["react"]);
        });
    }

    #[test]
    fn a_pull_that_adds_a_name_leaves_it_pending() {
        with_home(|home| {
            save_profile("baseline", &["react"]);
            save_profile("extra", &["vitest"]);
            let (dir, key) = import_clone(
                home,
                serde_json::json!({ "name": "demo", "profiles": ["baseline"] }),
            );
            confirm_project_profile(&key, "baseline").expect("confirm");

            fs::write(
                dir.join(".automatic.json"),
                r#"{"name":"demo","profiles":["baseline","extra"]}"#,
            )
            .expect("pull");

            let mut project = read(&key);
            assert_eq!(project.pending_profiles, vec!["extra"]);
            reconcile_project_profiles(&mut project);
            assert_eq!(project.skills, vec!["react"]);
        });
    }

    #[test]
    fn a_save_keeps_the_stored_decisions() {
        with_home(|home| {
            save_profile("baseline", &["react"]);
            let (_dir, key) = import_clone(
                home,
                serde_json::json!({ "name": "demo", "profiles": ["baseline"] }),
            );

            let mut forged = read(&key);
            forged.local_confirmed_profiles = vec!["baseline".into()];
            forged.local_ignored_profiles = vec!["other".into()];
            save_project(&key, &serde_json::to_string(&forged).expect("json")).expect("save");

            let pointer = registry_json(&key);
            assert!(pointer.get(CONFIRMED_FIELD).is_none());
            assert!(pointer.get(IGNORED_FIELD).is_none());

            ignore_project_profile(&key, "baseline").expect("ignore");
            confirm_project_profile(&key, "baseline").expect("confirm");
            let mut stale = read(&key);
            stale.local_confirmed_profiles.clear();
            save_project(&key, &serde_json::to_string(&stale).expect("json")).expect("save");
            assert_eq!(read(&key).local_confirmed_profiles, vec!["baseline"]);
        });
    }

    #[test]
    fn rewriting_the_pointer_keeps_the_decisions() {
        with_home(|home| {
            save_profile("baseline", &["react"]);
            let (dir, key) = import_clone(
                home,
                serde_json::json!({ "name": "demo", "profiles": ["baseline"] }),
            );
            confirm_project_profile(&key, "baseline").expect("confirm");
            ignore_project_profile(&key, "other").expect("ignore");
            let path = resolve_registry_entry(&key).unwrap().unwrap().path;

            write_registry_pointer(
                &path,
                &Project {
                    name: "demo".into(),
                    directory: dir.to_str().unwrap().to_string(),
                    local_key: key.clone(),
                    ..Default::default()
                },
            )
            .expect("write");

            let project = read(&key);
            assert_eq!(project.local_confirmed_profiles, vec!["baseline"]);
            assert_eq!(project.local_ignored_profiles, vec!["other"]);

            rename_project(&key, "renamed").expect("rename");
            assert_eq!(read(&key).local_confirmed_profiles, vec!["baseline"]);
        });
    }

    #[test]
    fn decisions_stay_out_of_the_folder() {
        with_home(|home| {
            save_profile("baseline", &["react"]);
            let (dir, key) = import_clone(
                home,
                serde_json::json!({ "name": "demo", "profiles": ["baseline", "other"] }),
            );
            confirm_project_profile(&key, "baseline").expect("confirm");
            ignore_project_profile(&key, "other").expect("ignore");

            save_project(&key, &read_project(&key).expect("read")).expect("save");

            for file in [".automatic.json", ".automatic/project.json"] {
                let text = fs::read_to_string(dir.join(file)).expect("read file");
                assert!(!text.contains("local_confirmed"), "{file}: {text}");
                assert!(!text.contains("local_ignored"), "{file}: {text}");
                assert!(!text.contains("local_confirmed_groups"), "{file}: {text}");
                assert!(!text.contains("pending_profiles"), "{file}: {text}");
            }
        });
    }

    #[test]
    fn ignore_leaves_a_name_pending_and_confirm_clears_it() {
        with_home(|home| {
            save_profile("baseline", &["react"]);
            let (_dir, key) = import_clone(
                home,
                serde_json::json!({ "name": "demo", "profiles": ["baseline"] }),
            );

            ignore_project_profile(&key, "baseline").expect("ignore");
            let project = read(&key);
            assert_eq!(project.pending_profiles, vec!["baseline"]);
            assert_eq!(project.local_ignored_profiles, vec!["baseline"]);

            confirm_project_profile(&key, "baseline").expect("confirm");
            let project = read(&key);
            assert!(project.pending_profiles.is_empty());
            assert!(project.local_ignored_profiles.is_empty());
        });
    }

    #[test]
    fn a_rename_carries_decisions_and_a_delete_forgets_them() {
        with_home(|home| {
            save_profile("old", &["react"]);
            let (_dir, key) = import_clone(
                home,
                serde_json::json!({ "name": "demo", "profiles": ["old"] }),
            );
            confirm_project_profile(&key, "old").expect("confirm");

            assert!(rename_profile_decisions("old", "new").is_empty());
            assert_eq!(read(&key).local_confirmed_profiles, vec!["new"]);

            assert!(forget_profile_everywhere("new").is_empty());
            assert!(read(&key).local_confirmed_profiles.is_empty());
        });
    }

    #[test]
    fn the_migration_keeps_existing_projects_working_and_runs_once() {
        with_home(|home| {
            save_profile("baseline", &["react"]);
            save_profile("later", &["vitest"]);
            let (dir, key) = import_clone(
                home,
                serde_json::json!({ "name": "demo", "profiles": ["baseline"] }),
            );
            set_project_read_only(&key, true).expect("read only");
            let folder_before = fs::read(dir.join(".automatic.json")).expect("read config");

            // No folder: the registry file is the whole config.
            let local = Project {
                name: "local".into(),
                profiles: vec!["baseline".into()],
                ..Default::default()
            };
            save_project("local", &serde_json::to_string(&local).expect("json")).expect("save");
            assert!(read("local").local_confirmed_profiles.is_empty());

            confirm_attached_profiles_once().expect("migrate");

            assert_eq!(read(&key).local_confirmed_profiles, vec!["baseline"]);
            assert!(read(&key).pending_profiles.is_empty());
            assert_eq!(read("local").local_confirmed_profiles, vec!["baseline"]);
            assert_eq!(
                fs::read(dir.join(".automatic.json")).expect("read config"),
                folder_before,
                "the migration writes only pointers"
            );

            // A name added later is not swept in by a second run.
            fs::write(
                dir.join(".automatic.json"),
                r#"{"name":"demo","profiles":["baseline","later"]}"#,
            )
            .expect("pull");
            confirm_attached_profiles_once().expect("again");
            assert_eq!(read(&key).pending_profiles, vec!["later"]);
        });
    }

    #[test]
    fn the_migration_records_group_memberships() {
        with_home(|home| {
            let (_dir, key) = import_clone(home, serde_json::json!({ "name": "demo" }));
            let id = read(&key).id;
            // Written directly, as a group from before joins were recorded.
            let groups = get_groups_dir().expect("dir");
            fs::create_dir_all(&groups).expect("mkdir");
            fs::write(
                groups.join("team.json"),
                serde_json::json!({ "name": "team", "projects": [id] }).to_string(),
            )
            .expect("write group");
            assert!(read(&key).local_confirmed_groups.is_empty());

            confirm_attached_profiles_once().expect("migrate");

            assert_eq!(read(&key).local_confirmed_groups, vec!["team"]);
        });
    }

    #[test]
    fn a_confirmation_uses_the_library_spelling_and_forgetting_folds_case() {
        with_home(|home| {
            save_profile("baseline", &["react"]);
            let (_dir, key) = import_clone(
                home,
                serde_json::json!({ "name": "demo", "profiles": ["Baseline"] }),
            );

            confirm_project_profile(&key, "Baseline").expect("confirm");
            assert_eq!(read(&key).local_confirmed_profiles, vec!["baseline"]);
            assert!(read(&key).pending_profiles.is_empty());

            // NFD and `ſ` reach the same file on APFS.
            forget_project_profile(&key, "baſeline").expect("forget");
            assert!(read(&key).local_confirmed_profiles.is_empty());

            confirm_project_profile(&key, "Baseline").expect("confirm");
            assert!(rename_profile_decisions("BASELINE", "renamed").is_empty());
            assert_eq!(read(&key).local_confirmed_profiles, vec!["renamed"]);
        });
    }

    #[test]
    fn only_a_created_project_keeps_its_names_when_it_has_no_entry() {
        with_home(|_| {
            let mut project = Project {
                profiles: vec!["baseline".into()],
                local_confirmed_groups: vec!["team".into()],
                ..Default::default()
            };

            load_profile_decisions("unknown", &mut project, false).expect("load");
            assert!(project.local_confirmed_profiles.is_empty());
            assert!(project.local_confirmed_groups.is_empty());

            load_profile_decisions("unknown", &mut project, true).expect("load");
            assert_eq!(project.local_confirmed_profiles, vec!["baseline"]);
        });
    }

    #[test]
    fn the_migration_skips_a_folder_it_cannot_read() {
        with_home(|home| {
            save_profile("baseline", &["react"]);
            let (dir, key) = import_clone(
                home,
                serde_json::json!({ "name": "demo", "profiles": ["baseline"] }),
            );
            fs::write(dir.join(".automatic.json"), "<<<<<<< conflict").expect("break");

            confirm_attached_profiles_once().expect("migrate");

            assert!(migration_marker_path().expect("marker").exists());
            assert!(registry_json(&key).get(CONFIRMED_FIELD).is_none());
        });
    }
}
