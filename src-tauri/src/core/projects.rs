use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use super::*;

// ── Projects ─────────────────────────────────────────────────────────────────
//
// A project with a directory stores its config in `.automatic.json` and its
// machine-local state in `.automatic/project.json` (see `project_layout`).
// A lightweight registry entry in `~/.automatic/projects/` maps each project
// name to its directory so we can enumerate them. The entry's file is named
// by the project's `local_key`; older entries still use `{name}.json` until
// the startup backfill renames them (see `project_registry`). When a project
// has no directory yet, the full config lives in the registry file.
//
// Callers address a project by an identifier: its `local_key` or its name
// (stage 3b, step 1). The functions here resolve an identifier to its
// registry file through `resolve_registry_entry`. Stores outside the
// registry (groups, memory, features, activity, recommendations, dev
// servers) are still keyed by name, so callers turn an identifier into the
// canonical name with `canonical_project_name` or `project_store_name`
// before they reach one.

/// Names of every registered project, sorted.
pub fn list_projects() -> Result<Vec<String>, String> {
    Ok(registry_names(&scan_registry()?))
}

// ── Project identifiers ──────────────────────────────────────────────────────
//
// Three ways to turn an identifier (a `local_key` or a name) into the name
// that name-keyed stores use. Pick by what the caller did before stage 3b:
//
// - Commands that needed the project to exist use `canonical_project_name`.
//   An unknown identifier is an error, exactly as `read_project` was.
// - Commands that create or tolerate a missing entry (save, delete) use
//   `resolve_project_name` and keep the identifier when it is `None`.
// - Store-only commands (memory, features, activity, recommendations, dev
//   servers, group lookups) never consulted the registry, and accept any
//   string so data for a deleted or never-registered project stays
//   reachable. They use `project_store_name`, which canonicalises an
//   identifier that resolves and passes any other string through unchanged.
//   A key therefore never reaches a store unless no project has it.

/// The registry name of the project `ident` identifies, or `None` when no
/// entry matches. An invalid identifier matches nothing.
pub fn resolve_project_name(ident: &str) -> Result<Option<String>, String> {
    if !is_valid_name(ident) {
        return Ok(None);
    }
    Ok(resolve_registry_entry(ident)?.map(|e| e.name))
}

/// The registry name of the project `ident` identifies. An unknown or
/// invalid identifier is an error with the same text `read_project` uses.
pub fn canonical_project_name(ident: &str) -> Result<String, String> {
    if !is_valid_name(ident) {
        return Err("Invalid project name".into());
    }
    resolve_project_name(ident)?.ok_or_else(|| format!("Project '{}' not found", ident))
}

/// The name a name-keyed store should use for `ident`. See the section
/// comment above. An identifier that matches two projects by name is passed
/// through too, because such a store never looked at the registry and so
/// never failed on it. A registry that cannot be read is still an error.
pub fn project_store_name(ident: &str) -> Result<String, String> {
    if !is_valid_name(ident) {
        return Ok(ident.to_string());
    }
    let entries = scan_registry()?;
    Ok(match resolve_entry_index(&entries, ident) {
        Ok(Some(i)) => entries[i].name.clone(),
        Ok(None) | Err(_) => ident.to_string(),
    })
}

/// One registered project as a list shows it. See [`get_project_summaries`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProjectSummary {
    /// Empty until the startup backfill has minted it.
    pub local_key: String,
    /// Empty until the startup backfill has minted it.
    pub id: String,
    pub name: String,
    /// Empty for a project with no folder yet.
    pub directory: String,
}

/// Every registered project with its keys, sorted by name.
///
/// Unlike `read_project` this writes nothing: it reads the registry entry
/// and the folder's config and state files only. Keys follow the stage 2
/// precedence (`resolve_stored_keys`): `id` from the folder's config, then
/// the registry cache; `local_key` from the registry, then the state file.
/// An entry that cannot be parsed is listed by name with no keys, as
/// `list_projects` lists it. A folder whose files cannot be read falls back
/// to the keys the registry caches, and the failure is logged.
pub fn get_project_summaries() -> Result<Vec<ProjectSummary>, String> {
    get_project_summaries_in(&get_projects_dir()?)
}

pub(crate) fn get_project_summaries_in(
    projects_dir: &std::path::Path,
) -> Result<Vec<ProjectSummary>, String> {
    // `scan_registry_in` sorts by name, then path.
    let entries = scan_registry_in(projects_dir)?;
    Ok(entries.iter().map(project_summary).collect())
}

fn project_summary(entry: &RegistryEntry) -> ProjectSummary {
    let directory = match entry.directory() {
        Ok(dir) => dir.unwrap_or_default(),
        Err(e) => {
            eprintln!("get_project_summaries: skipping keys of '{}': {}", entry.name, e);
            return ProjectSummary {
                local_key: String::new(),
                id: String::new(),
                name: entry.name.clone(),
                directory: String::new(),
            };
        }
    };
    let cached = ProjectKeys {
        id: entry.cached_id().unwrap_or_default().to_string(),
        local_key: entry.local_key().unwrap_or_default().to_string(),
    };
    let files = if directory.is_empty() {
        ProjectKeys::default()
    } else {
        match read_project_files(&directory, &entry.name) {
            Ok(project) => project.map(|p| ProjectKeys::of(&p)).unwrap_or_default(),
            Err(e) => {
                eprintln!(
                    "get_project_summaries: could not read the files of '{}', using the registry's keys: {}",
                    entry.name, e
                );
                ProjectKeys::default()
            }
        }
    };
    let keys = resolve_stored_keys(&files, Some(&cached));
    ProjectSummary {
        local_key: keys.local_key,
        id: keys.id,
        name: entry.name.clone(),
        directory,
    }
}

/// A project's `local_key` for a row that names it, or `None` while the
/// project has no key yet.
pub fn local_key_of(project: &Project) -> Option<String> {
    Some(project.local_key.clone()).filter(|k| !k.is_empty())
}

/// Map from project name to `local_key` for every registered project that
/// has one. Built once per call by the commands that tag rows naming a
/// project with its key.
pub fn project_local_keys_by_name() -> Result<std::collections::HashMap<String, String>, String> {
    Ok(get_project_summaries()?
        .into_iter()
        .filter(|s| !s.local_key.is_empty())
        .map(|s| (s.name, s.local_key))
        .collect())
}

/// Read a project by identifier (a `local_key` or a name). The returned
/// project's `name` is always the registry name, never the identifier.
pub fn read_project(name: &str) -> Result<String, String> {
    if !is_valid_name(name) {
        return Err("Invalid project name".into());
    }
    let entry =
        resolve_registry_entry(name)?.ok_or_else(|| format!("Project '{}' not found", name))?;
    // The registry's name, not the argument: a lookup ignores case and the
    // argument may be a `local_key`.
    let name = entry.name.as_str();
    let registry_path = &entry.path;

    let raw = fs::read_to_string(registry_path).map_err(|e| e.to_string())?;
    let registry_project = match serde_json::from_str::<Project>(&raw) {
        Ok(p) => p,
        Err(_) => Project {
            name: name.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            ..Default::default()
        },
    };
    let registry_keys = ProjectKeys::of(&registry_project);

    // If directory is set, read the full project from the project directory.
    // A malformed file there is an error rather than a fallback to registry
    // data: the write-back below would otherwise replace it.
    let mut project = if !registry_project.directory.is_empty() {
        read_project_files(&registry_project.directory, name)?.unwrap_or(registry_project)
    } else {
        registry_project
    };
    // `id` from the folder's config, else the pointer's cache; `local_key`
    // from the pointer, else the state file (see `resolve_stored_keys`). A
    // copied state file therefore cannot hand this checkout another
    // checkout's key, and the write-back below corrects it. No key is minted
    // here: minting belongs to the create paths and the startup backfill.
    let resolved = resolve_stored_keys(&ProjectKeys::of(&project), Some(&registry_keys));
    project.id = resolved.id;
    project.local_key = resolved.local_key;

    // Mark the project if its directory no longer exists on disk.
    if !project.directory.is_empty() && !std::path::Path::new(&project.directory).exists() {
        project.directory_missing = true;
    }

    // Restore: write project-level metadata back into user-level registries
    // for any entries that are missing locally.  This is the key portability
    // mechanism — when a project arrives on a new machine, its resolved data
    // seeds the local registries so skills regain their provenance.
    restore_to_user_registries(&project);

    // Enrich with current user-level metadata and persist so the project
    // config stays up-to-date on disk (important for portability).
    // Skip the write-back when the directory is missing — we cannot write to
    // a path that does not exist, and doing so would recreate the folder.
    // The write-back also carries keys resolved from the registry cache into
    // the folder, which is how a folder that returns gets its keys back.
    enrich_project(&mut project);
    if !project.directory.is_empty() && !project.directory_missing {
        if let Err(e) = write_project_files(&project) {
            eprintln!("read_project: could not refresh files for '{}': {}", name, e);
        }
    }

    // The name comes from the registry entry, never from the committed
    // config, which can differ (for example after a teammate renamed the
    // project and this machine pulled). Callers save back with
    // `project.name`, so it must resolve to this same entry. It is never the
    // entry's `local_key`: see `registry_entry_name`. Set after the
    // write-back so a read never rewrites the committed name on its own.
    project.name = name.to_string();

    let formatted = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;
    Ok(formatted)
}

// ── Restore from project to user-level registries ───────────────────────────
//
// When a project is opened on a machine that doesn't have the original
// user-level registry entries (e.g. cloned repo, different Automatic
// instance), this function seeds the local registries from the resolved
// metadata stored in project.json.  Only missing entries are written —
// existing local entries are never overwritten, so local state wins.

fn restore_to_user_registries(project: &Project) {
    restore_skill_sources(project);
    restore_skill_collections(project);
}

fn restore_skill_sources(project: &Project) {
    if project.skill_sources.is_empty() {
        return;
    }
    let Ok(existing) = read_skill_sources() else {
        return;
    };
    for (name, source) in &project.skill_sources {
        if !existing.contains_key(name) {
            let _ = record_skill_source(name, &source.source, &source.id, &source.kind);
        }
    }
}

fn restore_skill_collections(project: &Project) {
    if project.skill_collections.is_empty() {
        return;
    }
    let Ok(existing) = read_skill_collections() else {
        return;
    };
    for (name, collection) in &project.skill_collections {
        if !existing.contains_key(name) {
            let _ = set_skill_collection(name, collection);
        }
    }
}

// ── Project enrichment ───────────────────────────────────────────────────────
//
// Snapshots user-level registry data into the project config so it is
// self-contained when the project is opened on another machine.  Runs on
// every save — fields are overwritten with the current user-level state.
// Failures in individual lookups are silently skipped so that a missing
// registry never prevents a project from being saved.

fn enrich_project(project: &mut Project) {
    // Collapse case-variant MCP server duplicates before deriving anything from
    // the list. The global registry is keyed by filename on a case-insensitive
    // filesystem (macOS APFS, Windows), so `Sentry` and `sentry` resolve to the
    // same server there and the registry only ever shows one — but the project's
    // `Vec<String>` is case-sensitive, so without this it accumulates both and
    // each renders as its own row on the MCP tab. Running this on every
    // enrich_project means both save_project (persists the fix) and read_project
    // (cleans the display immediately) heal the duplicate.
    dedupe_ignore_ascii_case(&mut project.mcp_servers);
    dedupe_ignore_ascii_case(&mut project.disabled_mcp_servers);
    enrich_skill_sources(project);
    enrich_skill_collections(project);
    enrich_mcp_server_specs(project);
    enrich_resolved_rules(project);
    enrich_resolved_agents(project);
    enrich_resolved_commands(project);
}

/// Remove case-insensitive duplicate entries in place, keeping the first
/// occurrence and preserving order. Returns `true` if any duplicate was
/// dropped. See [`enrich_project`] for why the project's MCP server lists must
/// mirror the registry's case-insensitivity.
fn dedupe_ignore_ascii_case(items: &mut Vec<String>) -> bool {
    let before = items.len();
    let mut seen: Vec<String> = Vec::with_capacity(before);
    items.retain(|item| {
        let key = item.to_ascii_lowercase();
        if seen.contains(&key) {
            false
        } else {
            seen.push(key);
            true
        }
    });
    items.len() != before
}

/// Case-insensitive membership test used by the sync/discovery merge paths so
/// re-discovering a server under a different casing (e.g. `Sentry` when the
/// project already has `sentry`) does not push a duplicate. Matches the
/// registry's case-insensitivity — see [`enrich_project`].
pub(crate) fn contains_ignore_ascii_case(items: &[String], value: &str) -> bool {
    items.iter().any(|item| item.eq_ignore_ascii_case(value))
}

fn enrich_skill_sources(project: &mut Project) {
    let Ok(all_sources) = read_skill_sources() else {
        return;
    };
    let mut sources = BTreeMap::new();
    for name in &project.skills {
        if let Some(src) = all_sources.get(name) {
            sources.insert(name.clone(), src.clone());
        }
    }
    project.skill_sources = sources;
}

fn enrich_skill_collections(project: &mut Project) {
    let Ok(all_collections) = read_skill_collections() else {
        return;
    };
    let mut collections = BTreeMap::new();
    for name in &project.skills {
        if let Some(coll) = all_collections.get(name) {
            collections.insert(name.clone(), coll.clone());
        }
    }
    project.skill_collections = collections;
}

fn enrich_mcp_server_specs(project: &mut Project) {
    let mut specs = BTreeMap::new();
    for name in &project.mcp_servers {
        let Ok(raw) = read_mcp_server_config(name) else {
            continue;
        };
        let Ok(config) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        let server_type = config
            .get("type")
            .and_then(|v| v.as_str())
            .map(String::from);
        let command = config
            .get("command")
            .and_then(|v| v.as_str())
            .map(String::from);
        let args = config
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let env_keys = config
            .get("env")
            .and_then(|v| v.as_object())
            .map(|obj| obj.keys().cloned().collect())
            .unwrap_or_default();
        let url = config.get("url").and_then(|v| v.as_str()).map(String::from);

        specs.insert(
            name.clone(),
            McpServerSpec {
                server_type,
                command,
                args,
                env_keys,
                url,
            },
        );
    }
    project.mcp_server_specs = specs;
}

fn enrich_resolved_rules(project: &mut Project) {
    let mut resolved = BTreeMap::new();
    // Collect all unique rule machine names from file_rules values.
    let rule_names: std::collections::HashSet<&String> =
        project.file_rules.values().flatten().collect();
    for machine_name in rule_names {
        let Ok(raw) = read_rule(machine_name) else {
            continue;
        };
        let Ok(rule) = serde_json::from_str::<Rule>(&raw) else {
            continue;
        };
        resolved.insert(
            machine_name.clone(),
            ResolvedRule {
                name: rule.name,
                content: rule.content,
            },
        );
    }
    project.resolved_rules = resolved;
}

fn enrich_resolved_agents(project: &mut Project) {
    let mut resolved = BTreeMap::new();
    for machine_name in &project.user_agents {
        let Ok(content) = read_subagent(machine_name) else {
            continue;
        };
        // Extract display name from frontmatter, fall back to machine name.
        let name = extract_frontmatter_name(&content).unwrap_or_else(|| machine_name.clone());
        resolved.insert(machine_name.clone(), CustomAgent { name, content });
    }
    project.resolved_agents = resolved;
}

fn enrich_resolved_commands(project: &mut Project) {
    let mut resolved = BTreeMap::new();
    for machine_name in &project.user_commands {
        let Ok(content) = read_user_command(machine_name) else {
            continue;
        };
        resolved.insert(
            machine_name.clone(),
            CustomCommand {
                name: machine_name.clone(),
                content,
            },
        );
    }
    project.resolved_commands = resolved;
}

/// Extract the `name` field from YAML frontmatter in markdown content.
fn extract_frontmatter_name(content: &str) -> Option<String> {
    if !content.starts_with("---\n") {
        return None;
    }
    let end = content[4..].find("\n---")?;
    let yaml = &content[4..end + 4];
    for line in yaml.lines() {
        let line = line.trim();
        if let Some(name_val) = line.strip_prefix("name:") {
            let name = name_val.trim().trim_matches('"').trim_matches('\'');
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// Compare two directory paths for equality, preferring canonical forms when
/// both paths exist on disk so symlinks and trailing-slash variants match.
fn directories_equivalent(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let path_a = PathBuf::from(a);
    let path_b = PathBuf::from(b);
    match (path_a.canonicalize(), path_b.canonicalize()) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => {
            let na = a.trim_end_matches(['/', '\\']);
            let nb = b.trim_end_matches(['/', '\\']);
            na == nb
        }
    }
}

/// Return the registry name of the project that owns `directory`, if any.
/// A registry entry that cannot be parsed is an error, as it may own the
/// directory.
pub fn find_project_by_directory(directory: &str) -> Result<Option<String>, String> {
    if directory.is_empty() {
        return Ok(None);
    }
    for entry in scan_registry()? {
        let Some(existing_dir) = entry.directory()? else {
            continue;
        };
        if directories_equivalent(&existing_dir, directory) {
            return Ok(Some(entry.name));
        }
    }
    Ok(None)
}

/// Refuse to create a project that would overwrite an existing registry entry.
/// Used by the Add Project wizard. Orphan on-disk configs — a
/// `.automatic/project.json` with no registry entry — are handled upstream in
/// the wizard via [`inspect_project_directory`], so this check does not
/// consult the disk.
pub fn assert_can_create_project(name: &str, directory: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid project name".into());
    }

    if let Some(existing) = find_registry_entry(name)? {
        // Name the other project's directory so the user can tell an
        // unrelated project with the same folder name from this one. The
        // lookup ignores case, so `Website` clashes with `website`.
        return Err(match existing.directory()? {
            Some(dir) => format!(
                "The name '{}' is already used by the project at {}. Choose a different name.",
                name, dir
            ),
            None => format!(
                "The name '{}' is already used by another project. Choose a different name.",
                name
            ),
        });
    }

    if directory.is_empty() {
        return Ok(());
    }

    if let Some(existing) = find_project_by_directory(directory)? {
        return Err(format!(
            "This directory is already registered as project '{}'. Open that project instead of creating it again.",
            existing
        ));
    }

    Ok(())
}

/// Result of the wizard's pre-check on a candidate project directory. The
/// frontend branches on `kind` to decide whether to proceed straight to
/// create, redirect the user to an existing project, or prompt about an
/// orphan on-disk config.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind")]
pub enum DirectoryStatus {
    /// No on-disk config, no registry hit — safe to create.
    Available,
    /// A registered project already points at this directory.
    RegisteredHere { name: String },
    /// `.automatic/project.json` exists on disk but no registry entry
    /// references this directory. `name` is a best-effort read from the
    /// on-disk config's `name` field (falls back to the directory basename
    /// when the field is missing or the JSON is malformed).
    OrphanConfig { name: String },
}

/// Classify a candidate directory for the Add Project wizard. Distinguishes
/// registered projects from orphan on-disk configs so the frontend can offer
/// a real choice instead of a dead-end refusal.
pub fn inspect_project_directory(directory: &str) -> Result<DirectoryStatus, String> {
    if directory.is_empty() {
        return Ok(DirectoryStatus::Available);
    }

    if let Some(existing) = find_project_by_directory(directory)? {
        return Ok(DirectoryStatus::RegisteredHere { name: existing });
    }

    let Some(config_path) = project_config_source_path(directory) else {
        return Ok(DirectoryStatus::Available);
    };

    let name = read_orphan_config_name(&config_path).unwrap_or_else(|| {
        PathBuf::from(directory)
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .unwrap_or_default()
    });
    Ok(DirectoryStatus::OrphanConfig { name })
}

fn read_orphan_config_name(config_path: &std::path::Path) -> Option<String> {
    let raw = fs::read_to_string(config_path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    value
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

/// Adopt an on-disk project config as a new registry entry. Reads the
/// project files in `<directory>` (`.automatic.json`, or a legacy
/// `.automatic/project.json`), refuses on invalid or colliding names, and
/// writes the lightweight `{name, directory, id, local_key}` pointer to
/// `~/.automatic/projects/{local_key}.json`. Returns the adopted project name.
/// The on-disk config is left untouched here; the first `read_project`
/// writes the keys into it.
pub fn import_existing_project(directory: &str) -> Result<String, String> {
    if directory.is_empty() {
        return Err("A project directory is required to import.".into());
    }

    let project = read_project_files(directory, "")
        .map_err(|e| format!("The existing project config could not be read: {}", e))?
        .ok_or_else(|| format!("No Automatic configuration was found in {}.", directory))?;

    let name = project.name.trim();
    if name.is_empty() {
        return Err("The existing project config has no name and cannot be imported.".into());
    }
    if !is_valid_name(name) {
        return Err(format!(
            "The existing project config has an invalid name ('{}') and cannot be imported.",
            name
        ));
    }

    let projects_dir = get_projects_dir()?;
    if !projects_dir.exists() {
        fs::create_dir_all(&projects_dir).map_err(|e| e.to_string())?;
    }
    if find_registry_entry(name)?.is_some() {
        return Err(format!(
            "A project named '{}' is already registered. Rename or remove it before importing.",
            name
        ));
    }

    // Importing registers a new checkout, so it gets a fresh `local_key`. A
    // committed `id` means the repo already has an identity, which is kept.
    let name = name.to_string();
    let committed_id = Some(project.id.clone());
    let mut pointer = Project {
        name: name.clone(),
        directory: directory.to_string(),
        ..Default::default()
    };
    assign_new_project_keys(&mut pointer, committed_id);
    let registry_path = new_registry_path(&projects_dir, &name, &pointer.local_key)?;
    write_registry_pointer(&registry_path, &pointer)?;

    Ok(name)
}

/// Delete the project config and state files in `<directory>` and, if the
/// enclosing `.automatic/` directory is empty afterwards, remove it too.
/// Idempotent — succeeds when the files are already absent.
pub fn delete_project_config(directory: &str) -> Result<(), String> {
    if directory.is_empty() {
        return Err("A project directory is required.".into());
    }

    remove_project_files(directory)?;

    Ok(())
}

/// Save a project. `name` is an identifier: an existing entry is found by
/// its `local_key` or its name and keeps its registry name. An identifier
/// that resolves to nothing creates a new entry named `name`.
pub fn save_project(name: &str, data: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid project name".into());
    }

    let mut project: Project =
        serde_json::from_str(data).map_err(|e| format!("Invalid project data: {}", e))?;
    // directory_missing is a transient runtime flag — never persist it.
    project.directory_missing = false;

    let projects_dir = get_projects_dir()?;
    if !projects_dir.exists() {
        fs::create_dir_all(&projects_dir).map_err(|e| e.to_string())?;
    }

    // An existing entry is written where it is and keeps its registry name:
    // saving never renames (see `rename_project`). The lookup accepts a
    // `local_key` and ignores the case of a name.
    let existing = resolve_registry_entry(name)?;
    let registry_name = existing
        .as_ref()
        .map_or_else(|| name.to_string(), |e| e.name.clone());
    // Keys are resolved against the existing entry only. A new entry has no
    // stored keys, so its path comes from the incoming `local_key` below.
    let existing_path = existing.map(|e| e.path);

    // Keys are system-managed. Callers may hold none (sync paths, tests) or
    // stale ones (an editor opened before a `git pull` changed the committed
    // id). A stored key always wins; the incoming key is used only where
    // nothing is stored, which is how the create paths and the backfill
    // write keys they just minted.
    match stored_project_keys(existing_path.as_deref(), &project.directory) {
        Ok(stored) => apply_stored_keys(&mut project, &stored),
        // The unreadable file is about to be replaced by this save, which
        // was already the case before keys existed. Only proceed when the
        // incoming project carries both keys, so nothing is dropped.
        Err(e) if ProjectKeys::of(&project).is_complete() => eprintln!(
            "save_project: could not read the stored keys of '{}', keeping the incoming keys: {}",
            name, e
        ),
        Err(e) => {
            return Err(format!(
                "Could not read the stored keys of project '{}': {}",
                name, e
            ))
        }
    }
    enrich_project(&mut project);

    let registry_path = match existing_path {
        Some(path) => path,
        None => new_registry_path(&projects_dir, name, &project.local_key)?,
    };

    if !project.directory.is_empty() {
        // The committed config keeps the incoming name. The pointer's name is
        // the registry name, which is what makes the project findable.
        write_project_files(&project)?;
        let pointer = Project {
            name: registry_name,
            ..project
        };
        write_registry_pointer(&registry_path, &pointer)?;
    } else {
        // No directory yet: the registry file is the whole config, so its
        // name must be the registry name.
        project.name = registry_name;
        let pretty = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;
        fs::write(&registry_path, &pretty).map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Rename a project. A keyed registry entry is edited in place: only its
/// `name` field changes, and the file keeps its `local_key` name. The
/// in-directory config is rewritten with the new name too. Refuses a name
/// that another entry holds, ignoring case; a case-only rename of the same
/// entry is allowed. `old_name` is an identifier (a `local_key` or a name);
/// `new_name` is always a name.
pub fn rename_project(old_name: &str, new_name: &str) -> Result<(), String> {
    if !is_valid_name(old_name) {
        return Err("Invalid current project name".into());
    }
    if !is_valid_name(new_name) {
        return Err("Invalid new project name".into());
    }
    if old_name == new_name {
        return Ok(());
    }

    let entry = resolve_registry_entry(old_name)?
        .ok_or_else(|| format!("Project '{}' not found", old_name))?;
    if let Some(other) = find_registry_entry(new_name)? {
        if other.path != entry.path {
            return Err(format!("A project named '{}' already exists", new_name));
        }
    }
    // A legacy entry is named by its file stem, so it has to move to take
    // the new name. Check the target before anything is written.
    let legacy_target = if entry.keyed {
        None
    } else {
        let target = entry.path.with_file_name(format!("{}.json", new_name));
        if target.exists() && !same_file(&entry.path, &target)? {
            return Err(format!("A project named '{}' already exists", new_name));
        }
        Some(target)
    };

    // Read the full project (via read_project which resolves directory-based configs)
    let raw = read_project(&entry.name)?;
    let mut project: Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    project.name = new_name.to_string();
    project.updated_at = chrono::Utc::now().to_rfc3339();

    if !project.directory.is_empty() {
        if has_project_files(&project.directory)
            || PathBuf::from(&project.directory)
                .join(".automatic")
                .exists()
        {
            write_project_files(&project)?;
        }
        write_registry_pointer(&entry.path, &project)?;
    } else {
        // No directory: the registry file holds the full config.
        let pretty = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;
        fs::write(&entry.path, &pretty).map_err(|e| e.to_string())?;
    }

    // `fs::rename` also handles a case-only rename on a case-insensitive
    // filesystem (macOS APFS/HFS+), where both paths are the same file.
    if let Some(target) = legacy_target {
        fs::rename(&entry.path, &target).map_err(|e| {
            format!(
                "Failed to move {} to {}: {}",
                entry.path.display(),
                target.display(),
                e
            )
        })?;
    }

    // Update group membership lists so references follow the rename. Best-
    // effort — see delete_project for the rationale. Groups store the name
    // exactly as the registry held it.
    if let Err(e) = rename_project_in_all_groups(&entry.name, new_name) {
        eprintln!(
            "rename_project: could not update group references '{}' -> '{}': {}",
            entry.name, new_name, e
        );
    }

    Ok(())
}

/// Whether two existing paths are the same file. On a case-insensitive
/// filesystem `Foo.json` and `foo.json` are one file.
fn same_file(a: &std::path::Path, b: &std::path::Path) -> Result<bool, String> {
    let ca = a
        .canonicalize()
        .map_err(|e| format!("Failed to resolve {}: {}", a.display(), e))?;
    let cb = b
        .canonicalize()
        .map_err(|e| format!("Failed to resolve {}: {}", b.display(), e))?;
    Ok(ca == cb)
}

/// Delete a project by identifier (a `local_key` or a name). An identifier
/// that resolves to nothing still clears group references to it.
pub fn delete_project(name: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid project name".into());
    }
    let entry = resolve_registry_entry(name)?;
    // Group references use the registry's spelling of the name.
    let name = entry.as_ref().map_or(name, |e| e.name.as_str());

    // Try to read the project to clean up the project-directory config
    if let Some(registry_path) = entry.as_ref().map(|e| &e.path) {
        if let Ok(raw) = fs::read_to_string(registry_path) {
            if let Ok(project) = serde_json::from_str::<Project>(&raw) {
                if !project.directory.is_empty() {
                    // Best-effort: the registry entry is what makes the
                    // project exist, so a leftover file must not block delete.
                    if let Err(e) = remove_project_files(&project.directory) {
                        eprintln!("delete_project: could not remove files for '{}': {}", name, e);
                    }
                }
            }
        }

        fs::remove_file(registry_path).map_err(|e| e.to_string())?;
    }

    // Strip the deleted project from any group that still references it. The
    // project file is already gone, so a failure here cannot be recovered by
    // re-running the delete — log and continue so the UI still sees a clean
    // delete result.
    if let Err(e) = remove_project_from_all_groups(name) {
        eprintln!(
            "delete_project: could not clean up group references for '{}': {}",
            name, e
        );
    }

    Ok(())
}

// ── Test helpers (path-injectable versions of CRUD operations) ────────────────

#[cfg(test)]
mod test_helpers {
    use super::*;

    /// Save a project using an explicit projects dir (bypasses get_projects_dir).
    pub fn save_project_at(projects_dir: &PathBuf, name: &str, data: &str) -> Result<(), String> {
        if !is_valid_name(name) {
            return Err("Invalid project name".into());
        }
        let project: Project =
            serde_json::from_str(data).map_err(|e| format!("Invalid project data: {}", e))?;
        let pretty = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;

        if !projects_dir.exists() {
            fs::create_dir_all(projects_dir).map_err(|e| e.to_string())?;
        }

        let registry_path = match resolve_registry_entry_in(projects_dir, name)? {
            Some(entry) => entry.path,
            None => new_registry_path(projects_dir, name, &project.local_key)?,
        };

        if !project.directory.is_empty() {
            write_project_files(&project)?;
            write_registry_pointer(&registry_path, &project)?;
        } else {
            fs::write(&registry_path, &pretty).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Read a project using an explicit projects dir.
    pub fn read_project_at(projects_dir: &PathBuf, name: &str) -> Result<String, String> {
        if !is_valid_name(name) {
            return Err("Invalid project name".into());
        }
        let entry = resolve_registry_entry_in(projects_dir, name)?
            .ok_or_else(|| format!("Project '{}' not found", name))?;
        let name = entry.name.as_str();

        let raw = fs::read_to_string(&entry.path).map_err(|e| e.to_string())?;
        let registry_project = match serde_json::from_str::<Project>(&raw) {
            Ok(p) => p,
            Err(_) => Project {
                name: name.to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
                updated_at: chrono::Utc::now().to_rfc3339(),
                ..Default::default()
            },
        };

        if !registry_project.directory.is_empty() {
            if let Some(full_project) = read_project_files(&registry_project.directory, name)? {
                return serde_json::to_string_pretty(&full_project).map_err(|e| e.to_string());
            }
        }

        serde_json::to_string_pretty(&registry_project).map_err(|e| e.to_string())
    }

    /// List project names using an explicit projects dir.
    pub fn list_projects_at(projects_dir: &PathBuf) -> Result<Vec<String>, String> {
        Ok(registry_names(&scan_registry_in(projects_dir)?))
    }

    /// Delete a project using an explicit projects dir.
    pub fn delete_project_at(projects_dir: &PathBuf, name: &str) -> Result<(), String> {
        if !is_valid_name(name) {
            return Err("Invalid project name".into());
        }
        if let Some(entry) = resolve_registry_entry_in(projects_dir, name)? {
            let registry_path = entry.path;
            if let Ok(raw) = fs::read_to_string(&registry_path) {
                if let Ok(project) = serde_json::from_str::<Project>(&raw) {
                    if !project.directory.is_empty() {
                        remove_project_files(&project.directory)?;
                    }
                }
            }
            fs::remove_file(&registry_path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::test_helpers::*;
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, PathBuf) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let projects_dir = tmp.path().join("projects");
        (tmp, projects_dir)
    }

    fn minimal_project(name: &str) -> String {
        serde_json::to_string(&Project {
            name: name.to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            ..Default::default()
        })
        .expect("serialize")
    }

    // ── list ─────────────────────────────────────────────────────────────────

    #[test]
    fn list_returns_empty_when_dir_missing() {
        let (_tmp, projects_dir) = setup();
        let names = list_projects_at(&projects_dir).expect("list");
        assert!(names.is_empty());
    }

    #[test]
    fn list_returns_project_names() {
        let (_tmp, projects_dir) = setup();
        save_project_at(&projects_dir, "alpha", &minimal_project("alpha")).expect("save");
        save_project_at(&projects_dir, "beta", &minimal_project("beta")).expect("save");

        let mut names = list_projects_at(&projects_dir).expect("list");
        names.sort();
        assert_eq!(names, vec!["alpha", "beta"]);
    }

    // ── save + read ──────────────────────────────────────────────────────────

    #[test]
    fn save_and_read_roundtrip_no_directory() {
        let (_tmp, projects_dir) = setup();
        let data = minimal_project("my-project");
        save_project_at(&projects_dir, "my-project", &data).expect("save");

        let raw = read_project_at(&projects_dir, "my-project").expect("read");
        let project: Project = serde_json::from_str(&raw).expect("parse");
        assert_eq!(project.name, "my-project");
    }

    #[test]
    fn save_with_directory_writes_config_to_project_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let projects_dir = tmp.path().join("projects");
        let project_dir = tmp.path().join("my-workspace");
        fs::create_dir_all(&project_dir).expect("create project dir");

        let project = Project {
            name: "with-dir".to_string(),
            directory: project_dir.to_str().unwrap().to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            ..Default::default()
        };
        let data = serde_json::to_string(&project).expect("serialize");
        save_project_at(&projects_dir, "with-dir", &data).expect("save");

        // Config should be in the project directory.
        let config_path = project_dir.join(".automatic").join("project.json");
        assert!(
            config_path.exists(),
            "project config missing in project dir"
        );

        // Registry entry should be a lightweight pointer.
        let registry_path = projects_dir.join("with-dir.json");
        let raw = fs::read_to_string(&registry_path).expect("read registry");
        let val: serde_json::Value = serde_json::from_str(&raw).expect("parse");
        assert!(val.get("name").is_some());
        assert!(val.get("directory").is_some());
    }

    #[test]
    fn read_falls_back_to_registry_when_no_project_dir_config() {
        let (_tmp, projects_dir) = setup();
        let data = minimal_project("fallback");
        save_project_at(&projects_dir, "fallback", &data).expect("save");

        let raw = read_project_at(&projects_dir, "fallback").expect("read");
        let project: Project = serde_json::from_str(&raw).expect("parse");
        assert_eq!(project.name, "fallback");
    }

    #[test]
    fn read_returns_error_for_missing_project() {
        let (_tmp, projects_dir) = setup();
        let result = read_project_at(&projects_dir, "does-not-exist");
        assert!(result.is_err());
    }

    // ── delete ───────────────────────────────────────────────────────────────

    #[test]
    fn delete_removes_registry_entry() {
        let (_tmp, projects_dir) = setup();
        save_project_at(&projects_dir, "doomed", &minimal_project("doomed")).expect("save");

        let path = projects_dir.join("doomed.json");
        assert!(path.exists());

        delete_project_at(&projects_dir, "doomed").expect("delete");
        assert!(!path.exists());
    }

    #[test]
    fn delete_is_idempotent_when_project_missing() {
        let (_tmp, projects_dir) = setup();
        // Deleting a non-existent project should not error.
        delete_project_at(&projects_dir, "ghost").expect("delete non-existent");
    }

    // ── invalid name handling ────────────────────────────────────────────────

    #[test]
    fn save_with_invalid_name_returns_error() {
        let (_tmp, projects_dir) = setup();
        let result = save_project_at(&projects_dir, "", &minimal_project(""));
        assert!(result.is_err());
    }

    #[test]
    fn save_with_path_traversal_name_returns_error() {
        let (_tmp, projects_dir) = setup();
        let result = save_project_at(&projects_dir, "../escape", &minimal_project("x"));
        assert!(result.is_err());
    }

    // ── overwrite ────────────────────────────────────────────────────────────

    // ── extract_frontmatter_name ──────────────────────────────────────────

    #[test]
    fn extract_frontmatter_name_returns_name() {
        let content = "---\nname: My Agent\ndescription: does stuff\n---\n\nBody here.";
        assert_eq!(
            super::extract_frontmatter_name(content),
            Some("My Agent".into())
        );
    }

    #[test]
    fn extract_frontmatter_name_strips_quotes() {
        let content = "---\nname: \"Quoted Name\"\n---\n\nBody.";
        assert_eq!(
            super::extract_frontmatter_name(content),
            Some("Quoted Name".into())
        );
    }

    #[test]
    fn extract_frontmatter_name_returns_none_without_frontmatter() {
        let content = "# Just a heading\n\nNo frontmatter.";
        assert_eq!(super::extract_frontmatter_name(content), None);
    }

    #[test]
    fn extract_frontmatter_name_returns_none_for_empty_name() {
        let content = "---\nname: \n---\n\nBody.";
        assert_eq!(super::extract_frontmatter_name(content), None);
    }

    // ── resolved fields roundtrip through save/read ─────────────────────

    #[test]
    fn resolved_fields_survive_save_and_read() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let projects_dir = tmp.path().join("projects");
        let project_dir = tmp.path().join("my-project");
        fs::create_dir_all(&project_dir).expect("mkdir");

        let mut project = Project {
            name: "portable".into(),
            directory: project_dir.to_str().unwrap().into(),
            skills: vec!["my-skill".into()],
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            ..Default::default()
        };
        project.skill_sources.insert(
            "my-skill".into(),
            SkillSource {
                source: "owner/repo".into(),
                id: "owner/repo/my-skill".into(),
                kind: "github".into(),
                installed_sha: None,
                installed_version: None,
                installed_at: None,
            },
        );
        project
            .skill_collections
            .insert("my-skill".into(), "test-collection".into());
        project.mcp_server_specs.insert(
            "github".into(),
            McpServerSpec {
                server_type: Some("stdio".into()),
                command: Some("docker".into()),
                args: vec!["run".into(), "ghcr.io/github/server".into()],
                env_keys: vec!["GITHUB_TOKEN".into()],
                url: None,
            },
        );
        project.resolved_rules.insert(
            "my-rule".into(),
            ResolvedRule {
                name: "My Rule".into(),
                content: "Do the thing.".into(),
            },
        );

        let data = serde_json::to_string(&project).expect("serialize");
        save_project_at(&projects_dir, "portable", &data).expect("save");

        let raw = read_project_at(&projects_dir, "portable").expect("read");
        let loaded: Project = serde_json::from_str(&raw).expect("parse");

        // skill_sources preserved
        let src = loaded
            .skill_sources
            .get("my-skill")
            .expect("skill source should survive roundtrip");
        assert_eq!(src.source, "owner/repo");
        assert_eq!(src.kind, "github");

        // skill_collections preserved
        assert_eq!(
            loaded.skill_collections.get("my-skill").map(|s| s.as_str()),
            Some("test-collection")
        );

        // mcp_server_specs preserved
        let spec = loaded
            .mcp_server_specs
            .get("github")
            .expect("server spec should survive roundtrip");
        assert_eq!(spec.command.as_deref(), Some("docker"));
        assert_eq!(spec.env_keys, vec!["GITHUB_TOKEN"]);

        // resolved_rules preserved
        let rule = loaded
            .resolved_rules
            .get("my-rule")
            .expect("rule should survive roundtrip");
        assert_eq!(rule.name, "My Rule");
        assert_eq!(rule.content, "Do the thing.");
    }

    // ── overwrite ────────────────────────────────────────────────────────

    #[test]
    fn save_overwrites_existing_project() {
        let (_tmp, projects_dir) = setup();
        let v1 = serde_json::to_string(&Project {
            name: "proj".to_string(),
            description: "v1".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            ..Default::default()
        })
        .expect("serialize v1");

        let v2 = serde_json::to_string(&Project {
            name: "proj".to_string(),
            description: "v2".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-02T00:00:00Z".to_string(),
            ..Default::default()
        })
        .expect("serialize v2");

        save_project_at(&projects_dir, "proj", &v1).expect("save v1");
        save_project_at(&projects_dir, "proj", &v2).expect("save v2");

        let raw = read_project_at(&projects_dir, "proj").expect("read");
        let project: Project = serde_json::from_str(&raw).expect("parse");
        assert_eq!(project.description, "v2");
    }

    // ── MCP server case-insensitive de-duplication ───────────────────────────

    #[test]
    fn dedupe_ignore_ascii_case_keeps_first_occurrence() {
        let mut items = vec![
            "Sentry".to_string(),
            "linear".to_string(),
            "sentry".to_string(),
            "SENTRY".to_string(),
            "Linear".to_string(),
        ];
        let changed = dedupe_ignore_ascii_case(&mut items);
        assert!(changed, "duplicates should have been dropped");
        assert_eq!(
            items,
            vec!["Sentry".to_string(), "linear".to_string()],
            "first occurrence of each case-insensitive name wins, order preserved"
        );
    }

    #[test]
    fn dedupe_ignore_ascii_case_reports_no_change_when_unique() {
        let mut items = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert!(!dedupe_ignore_ascii_case(&mut items));
        assert_eq!(items, vec!["a", "b", "c"]);
    }

    #[test]
    fn contains_ignore_ascii_case_matches_across_case() {
        let items = vec!["sentry".to_string(), "linear".to_string()];
        assert!(contains_ignore_ascii_case(&items, "Sentry"));
        assert!(contains_ignore_ascii_case(&items, "LINEAR"));
        assert!(!contains_ignore_ascii_case(&items, "github"));
    }

    #[test]
    fn enrich_project_drops_case_variant_mcp_servers() {
        // Regression: a project that accumulated both `Sentry` (from autodetect,
        // verbatim casing) and `sentry` (from Add-from-Library, lowercased) must
        // collapse to a single entry — the registry only ever shows one.
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let mut project = Project {
                name: "demo".to_string(),
                mcp_servers: vec![
                    "Sentry".to_string(),
                    "linear".to_string(),
                    "sentry".to_string(),
                ],
                disabled_mcp_servers: vec!["Foo".to_string(), "foo".to_string()],
                ..Default::default()
            };

            enrich_project(&mut project);

            assert_eq!(
                project.mcp_servers,
                vec!["Sentry".to_string(), "linear".to_string()],
                "case-variant MCP server duplicates should collapse to the first occurrence"
            );
            assert_eq!(
                project.disabled_mcp_servers,
                vec!["Foo".to_string()],
                "disabled list is de-duplicated the same way"
            );
        });
    }

    // ── create guards (Add Project must not overwrite) ───────────────────

    #[test]
    fn assert_can_create_rejects_existing_name() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let data = minimal_project("alpha");
            save_project("alpha", &data).expect("save");

            let err = assert_can_create_project("alpha", "")
                .expect_err("duplicate name should be rejected");
            assert_eq!(
                err,
                "The name 'alpha' is already used by another project. Choose a different name."
            );
        });
    }

    #[test]
    fn assert_can_create_existing_name_error_names_the_other_directory() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let existing_dir = home.path().join("work").join("website");
            fs::create_dir_all(&existing_dir).expect("mkdir");
            let existing = existing_dir.to_str().unwrap().to_string();

            let project = Project {
                name: "website".to_string(),
                directory: existing.clone(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
                ..Default::default()
            };
            let data = serde_json::to_string(&project).expect("serialize");
            save_project("website", &data).expect("save");

            let other_dir = home.path().join("personal").join("website");
            fs::create_dir_all(&other_dir).expect("mkdir");

            let err = assert_can_create_project("website", other_dir.to_str().unwrap())
                .expect_err("duplicate name should be rejected");
            assert_eq!(
                err,
                format!(
                    "The name 'website' is already used by the project at {}. Choose a different name.",
                    existing
                )
            );
        });
    }

    #[test]
    fn assert_can_create_rejects_directory_already_registered() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let project_dir = home.path().join("workspace");
            fs::create_dir_all(&project_dir).expect("mkdir");
            let dir = project_dir.to_str().unwrap().to_string();

            let project = Project {
                name: "alpha".to_string(),
                directory: dir.clone(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
                ..Default::default()
            };
            let data = serde_json::to_string(&project).expect("serialize");
            save_project("alpha", &data).expect("save");

            let err = assert_can_create_project("beta", &dir)
                .expect_err("directory claimed by another project should be rejected");
            assert!(
                err.contains("already registered as project 'alpha'"),
                "unexpected error: {err}"
            );
        });
    }

    #[test]
    fn assert_can_create_allows_orphan_on_disk_config() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let project_dir = home.path().join("orphan-workspace");
            let automatic_dir = project_dir.join(".automatic");
            fs::create_dir_all(&automatic_dir).expect("mkdir");
            fs::write(
                automatic_dir.join("project.json"),
                r#"{"name":"orphan","directory":"x"}"#,
            )
            .expect("write config");
            let dir = project_dir.to_str().unwrap().to_string();

            // An orphan on-disk config is no longer a blocker — the wizard
            // handles it via inspect_project_directory + user choice.
            assert_can_create_project("fresh", &dir)
                .expect("orphan on-disk config must not block creation");
        });
    }

    #[test]
    fn inspect_project_directory_reports_available_for_empty_dir() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let project_dir = home.path().join("blank");
            fs::create_dir_all(&project_dir).expect("mkdir");
            let dir = project_dir.to_str().unwrap().to_string();

            let status = inspect_project_directory(&dir).expect("inspect");
            assert!(matches!(status, DirectoryStatus::Available), "unexpected status: {:?}", status);
        });
    }

    #[test]
    fn inspect_project_directory_reports_registered_here() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let project_dir = home.path().join("registered");
            fs::create_dir_all(&project_dir).expect("mkdir");
            let dir = project_dir.to_str().unwrap().to_string();

            let project = Project {
                name: "alpha".to_string(),
                directory: dir.clone(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
                ..Default::default()
            };
            let data = serde_json::to_string(&project).expect("serialize");
            save_project("alpha", &data).expect("save");

            let status = inspect_project_directory(&dir).expect("inspect");
            match status {
                DirectoryStatus::RegisteredHere { name } => assert_eq!(name, "alpha"),
                other => panic!("unexpected status: {:?}", other),
            }
        });
    }

    #[test]
    fn inspect_project_directory_reports_orphan_config_with_name() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let project_dir = home.path().join("orphan");
            let automatic_dir = project_dir.join(".automatic");
            fs::create_dir_all(&automatic_dir).expect("mkdir");
            fs::write(
                automatic_dir.join("project.json"),
                r#"{"name":"legacy-name","directory":"whatever"}"#,
            )
            .expect("write config");
            let dir = project_dir.to_str().unwrap().to_string();

            let status = inspect_project_directory(&dir).expect("inspect");
            match status {
                DirectoryStatus::OrphanConfig { name } => assert_eq!(name, "legacy-name"),
                other => panic!("unexpected status: {:?}", other),
            }
        });
    }

    #[test]
    fn inspect_project_directory_falls_back_to_folder_name_on_malformed_json() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let project_dir = home.path().join("orphan-broken");
            let automatic_dir = project_dir.join(".automatic");
            fs::create_dir_all(&automatic_dir).expect("mkdir");
            fs::write(automatic_dir.join("project.json"), "{ not json").expect("write");
            let dir = project_dir.to_str().unwrap().to_string();

            let status = inspect_project_directory(&dir).expect("inspect");
            match status {
                DirectoryStatus::OrphanConfig { name } => assert_eq!(name, "orphan-broken"),
                other => panic!("unexpected status: {:?}", other),
            }
        });
    }

    #[test]
    fn import_existing_project_registers_the_on_disk_name() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let project_dir = home.path().join("adopt-me");
            let automatic_dir = project_dir.join(".automatic");
            fs::create_dir_all(&automatic_dir).expect("mkdir");
            let dir = project_dir.to_str().unwrap().to_string();
            let full = Project {
                name: "adopted".to_string(),
                directory: dir.clone(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
                ..Default::default()
            };
            fs::write(
                automatic_dir.join("project.json"),
                serde_json::to_string(&full).expect("serialize"),
            )
            .expect("write config");

            let name = import_existing_project(&dir).expect("import");
            assert_eq!(name, "adopted");

            // The registry pointer must now exist, be named by its
            // local_key, and point at this dir.
            let pointer = registry_file("adopted");
            let raw = fs::read_to_string(&pointer).expect("read pointer");
            let value: serde_json::Value =
                serde_json::from_str(&raw).expect("parse pointer");
            assert_eq!(
                pointer.file_stem().and_then(|s| s.to_str()),
                value["local_key"].as_str()
            );
            assert_eq!(value["name"].as_str(), Some("adopted"));
            assert_eq!(value["directory"].as_str(), Some(dir.as_str()));
        });
    }

    #[test]
    fn import_existing_project_rejects_name_collision() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            // Pre-register a project with the same name.
            let existing_dir = home.path().join("existing");
            fs::create_dir_all(&existing_dir).expect("mkdir");
            let existing = Project {
                name: "clash".to_string(),
                directory: existing_dir.to_str().unwrap().to_string(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
                ..Default::default()
            };
            save_project(
                "clash",
                &serde_json::to_string(&existing).expect("serialize"),
            )
            .expect("save existing");

            // Now try to import an orphan whose name collides.
            let project_dir = home.path().join("orphan-clash");
            let automatic_dir = project_dir.join(".automatic");
            fs::create_dir_all(&automatic_dir).expect("mkdir");
            fs::write(
                automatic_dir.join("project.json"),
                r#"{"name":"clash","directory":"whatever"}"#,
            )
            .expect("write");

            let dir = project_dir.to_str().unwrap().to_string();
            let err = import_existing_project(&dir).expect_err("collision should fail");
            assert!(err.contains("already registered"), "unexpected error: {err}");
        });
    }

    #[test]
    fn delete_project_config_removes_file_and_empty_dir() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let project_dir = home.path().join("wipe-me");
            let automatic_dir = project_dir.join(".automatic");
            fs::create_dir_all(&automatic_dir).expect("mkdir");
            fs::write(automatic_dir.join("project.json"), "{}").expect("write");
            let dir = project_dir.to_str().unwrap().to_string();

            delete_project_config(&dir).expect("delete");
            assert!(
                !automatic_dir.join("project.json").exists(),
                "project.json must be gone"
            );
            assert!(
                !automatic_dir.exists(),
                ".automatic must be removed when empty"
            );
        });
    }

    #[test]
    fn delete_project_config_is_idempotent() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let project_dir = home.path().join("nothing-here");
            fs::create_dir_all(&project_dir).expect("mkdir");
            let dir = project_dir.to_str().unwrap().to_string();
            delete_project_config(&dir).expect("no-op delete should succeed");
        });
    }

    #[test]
    fn assert_can_create_allows_new_name_and_directory() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let project_dir = home.path().join("new-workspace");
            fs::create_dir_all(&project_dir).expect("mkdir");
            let dir = project_dir.to_str().unwrap().to_string();

            assert_can_create_project("brand-new", &dir)
                .expect("new name and empty directory should be allowed");
        });
    }

    // ── identity keys (project identity plan, stage 2) ──────────────────

    fn read_json(path: &std::path::Path) -> serde_json::Value {
        let raw = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {}: {}", path.display(), e))
    }

    fn project_with_dir(name: &str, dir: &std::path::Path) -> Project {
        Project {
            name: name.to_string(),
            directory: dir.to_str().unwrap().to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            ..Default::default()
        }
    }

    fn load(name: &str) -> Project {
        serde_json::from_str(&read_project(name).expect("read")).expect("parse")
    }

    /// Registry file of the project named `name`, via the resolver.
    fn registry_file(name: &str) -> PathBuf {
        find_registry_entry(name)
            .expect("resolve")
            .unwrap_or_else(|| panic!("no registry entry for '{name}'"))
            .path
    }

    /// Registry file named `<stem>.json`.
    fn registry_stem_file(stem: &str) -> PathBuf {
        get_projects_dir().unwrap().join(format!("{stem}.json"))
    }

    #[test]
    fn new_project_keys_land_in_the_right_files() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("repo");
            fs::create_dir_all(&dir).expect("mkdir");
            let mut project = project_with_dir("keyed", &dir);
            prepare_new_project_keys(&mut project).expect("prepare");
            save_project("keyed", &serde_json::to_string(&project).unwrap()).expect("save");

            let config = read_json(&dir.join(CONFIG_FILE_NAME));
            let state = read_json(&dir.join(".automatic").join("project.json"));
            // A new project with keys is written as `<local_key>.json`.
            let pointer = read_json(&registry_stem_file(&project.local_key));

            assert_eq!(config["id"].as_str(), Some(project.id.as_str()));
            assert!(config.get("local_key").is_none(), "local_key is never committed");
            assert_eq!(state["local_key"].as_str(), Some(project.local_key.as_str()));
            assert!(state.get("id").is_none(), "id is config, not state");
            assert_eq!(
                pointer,
                serde_json::json!({
                    "name": "keyed",
                    "directory": dir.to_str().unwrap(),
                    "id": project.id,
                    "local_key": project.local_key,
                })
            );
        });
    }

    #[test]
    fn save_without_keys_preserves_stored_keys() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("repo");
            fs::create_dir_all(&dir).expect("mkdir");
            let mut project = project_with_dir("keep", &dir);
            prepare_new_project_keys(&mut project).expect("prepare");
            save_project("keep", &serde_json::to_string(&project).unwrap()).expect("save");

            let mut keyless = project_with_dir("keep", &dir);
            keyless.description = "edited".into();
            save_project("keep", &serde_json::to_string(&keyless).unwrap()).expect("keyless save");

            let loaded = load("keep");
            assert_eq!(loaded.description, "edited");
            assert_eq!(loaded.id, project.id);
            assert_eq!(loaded.local_key, project.local_key);
            let pointer = read_json(&registry_stem_file(&project.local_key));
            assert_eq!(pointer["id"].as_str(), Some(project.id.as_str()));
            assert_eq!(pointer["local_key"].as_str(), Some(project.local_key.as_str()));

            // A directory-less project keeps its keys in the registry file.
            let mut bare = Project {
                name: "bare".into(),
                ..Default::default()
            };
            fill_missing_project_keys(&mut bare);
            save_project("bare", &serde_json::to_string(&bare).unwrap()).expect("save bare");
            let keyless_bare = Project {
                name: "bare".into(),
                description: "edited".into(),
                ..Default::default()
            };
            save_project("bare", &serde_json::to_string(&keyless_bare).unwrap())
                .expect("keyless bare save");
            let loaded_bare = load("bare");
            assert_eq!(loaded_bare.id, bare.id);
            assert_eq!(loaded_bare.local_key, bare.local_key);
        });
    }

    #[test]
    fn committed_id_beats_the_registry_cache() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("repo");
            fs::create_dir_all(&dir).expect("mkdir");
            let mut project = project_with_dir("pulled", &dir);
            prepare_new_project_keys(&mut project).expect("prepare");
            save_project("pulled", &serde_json::to_string(&project).unwrap()).expect("save");

            // A `git pull` brings in a config with a different committed id.
            let config_path = dir.join(CONFIG_FILE_NAME);
            let mut config = read_json(&config_path);
            config["id"] = serde_json::json!("id-from-upstream");
            fs::write(&config_path, config.to_string()).expect("write config");

            let loaded = load("pulled");
            assert_eq!(loaded.id, "id-from-upstream", "the committed id wins");
            assert_eq!(loaded.local_key, project.local_key);
        });
    }

    #[test]
    fn registry_cached_keys_fill_a_folder_without_them() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("repo");
            fs::create_dir_all(&dir).expect("mkdir");
            let project = project_with_dir("cached", &dir);
            save_project("cached", &serde_json::to_string(&project).unwrap()).expect("save");
            // Only the registry holds the keys, as after a backfill while the
            // folder was away.
            let registry_path = get_projects_dir().unwrap().join("cached.json");
            let mut pointer = read_json(&registry_path);
            pointer["id"] = serde_json::json!("cached-id");
            pointer["local_key"] = serde_json::json!("cached-key");
            fs::write(&registry_path, pointer.to_string()).expect("write pointer");

            let loaded = load("cached");
            assert_eq!(loaded.id, "cached-id");
            assert_eq!(loaded.local_key, "cached-key");
            // The read's write-back carries them into the folder.
            assert_eq!(read_json(&dir.join(CONFIG_FILE_NAME))["id"], "cached-id");
            assert_eq!(
                read_json(&dir.join(".automatic").join("project.json"))["local_key"],
                "cached-key"
            );
        });
    }

    #[test]
    fn read_returns_the_registry_entry_name() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("repo");
            fs::create_dir_all(&dir).expect("mkdir");
            let mut project = project_with_dir("local-name", &dir);
            prepare_new_project_keys(&mut project).expect("prepare");
            save_project("local-name", &serde_json::to_string(&project).unwrap()).expect("save");
            let registry_path = registry_stem_file(&project.local_key);

            let config_path = dir.join(CONFIG_FILE_NAME);
            let mut config = read_json(&config_path);
            config["name"] = serde_json::json!("renamed-upstream");
            fs::write(&config_path, config.to_string()).expect("write config");

            let loaded = load("local-name");
            assert_eq!(loaded.name, "local-name", "name follows the registry entry");
            assert_ne!(loaded.name, project.local_key, "never the key");
            assert_eq!(
                read_json(&config_path)["name"],
                "renamed-upstream",
                "a read alone does not rewrite the committed name"
            );
            assert_eq!(
                load("LOCAL-NAME").name,
                "local-name",
                "a lookup ignores case and returns the registry's spelling"
            );

            // Saving what was read writes back under the same registry entry.
            save_project(&loaded.name, &serde_json::to_string(&loaded).unwrap()).expect("save");
            assert_eq!(list_projects().expect("list"), vec!["local-name".to_string()]);
            assert_eq!(registry_file("local-name"), registry_path, "same file");
            assert_eq!(read_json(&registry_path)["name"], "local-name");
        });
    }

    #[test]
    fn import_keeps_committed_id_and_mints_a_new_local_key() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("clone");
            fs::create_dir_all(dir.join(".automatic")).expect("mkdir");
            fs::write(
                dir.join(CONFIG_FILE_NAME),
                r#"{"id":"committed-id","name":"cloned"}"#,
            )
            .expect("write config");
            // A state file copied along with the folder.
            fs::write(
                dir.join(".automatic").join("project.json"),
                r#"{"local_key":"other-checkout"}"#,
            )
            .expect("write state");

            let name = import_existing_project(dir.to_str().unwrap()).expect("import");
            let pointer = read_json(&registry_file(&name));
            assert_eq!(pointer["id"], "committed-id");
            let local_key = pointer["local_key"].as_str().expect("local_key").to_string();
            assert_ne!(local_key, "other-checkout");
            assert!(uuid::Uuid::parse_str(&local_key).is_ok());

            let loaded = load(&name);
            assert_eq!(loaded.id, "committed-id");
            assert_eq!(loaded.local_key, local_key, "read returns the pointer's local_key");
        });
    }

    #[test]
    fn copied_folder_takes_the_pointer_local_key_not_the_copied_one() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let source_dir = home.path().join("source");
            fs::create_dir_all(&source_dir).expect("mkdir");
            let mut source = project_with_dir("source", &source_dir);
            prepare_new_project_keys(&mut source).expect("prepare");
            save_project("source", &serde_json::to_string(&source).unwrap()).expect("save");

            // `cp -r source copy`, then give the copy its own name so the
            // import does not collide.
            let copy_dir = home.path().join("copy");
            fs::create_dir_all(copy_dir.join(".automatic")).expect("mkdir");
            let state_rel = std::path::Path::new(".automatic").join("project.json");
            for rel in [std::path::Path::new(CONFIG_FILE_NAME), state_rel.as_path()] {
                fs::copy(source_dir.join(rel), copy_dir.join(rel)).expect("copy");
            }
            let copy_config = copy_dir.join(CONFIG_FILE_NAME);
            let mut config = read_json(&copy_config);
            config["name"] = serde_json::json!("copy");
            fs::write(&copy_config, config.to_string()).expect("rename copy");
            assert_eq!(
                read_json(&copy_dir.join(&state_rel))["local_key"],
                source.local_key.as_str(),
                "the copy starts with the source checkout's local_key"
            );

            let name = import_existing_project(copy_dir.to_str().unwrap()).expect("import");
            let pointer = read_json(&registry_file(&name));
            let pointer_key = pointer["local_key"].as_str().expect("local_key").to_string();
            assert_ne!(pointer_key, source.local_key);

            let loaded = load(&name);
            assert_eq!(loaded.local_key, pointer_key, "the pointer is authoritative");
            assert_eq!(loaded.id, source.id, "a copy shares the committed id");
            assert_eq!(
                read_json(&copy_dir.join(&state_rel))["local_key"],
                pointer_key.as_str(),
                "the write-back corrects the copied state file"
            );
            assert_eq!(load("source").local_key, source.local_key);
        });
    }

    #[test]
    fn save_keeps_the_stored_id_over_a_stale_incoming_id() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("repo");
            fs::create_dir_all(&dir).expect("mkdir");
            let mut project = project_with_dir("stale", &dir);
            project.id = "id-a".into();
            project.local_key = "key-a".into();
            save_project("stale", &serde_json::to_string(&project).unwrap()).expect("save");

            // An editor still holding other keys saves.
            let mut incoming = project.clone();
            incoming.id = "id-b".into();
            incoming.local_key = "key-b".into();
            incoming.description = "edited".into();
            save_project("stale", &serde_json::to_string(&incoming).unwrap()).expect("save");

            assert_eq!(read_json(&dir.join(CONFIG_FILE_NAME))["id"], "id-a");
            let loaded = load("stale");
            assert_eq!(loaded.description, "edited");
            assert_eq!(loaded.id, "id-a");
            assert_eq!(loaded.local_key, "key-a");
            let pointer = read_json(&registry_stem_file("key-a"));
            assert_eq!(pointer["id"], "id-a");
            assert_eq!(pointer["local_key"], "key-a");
        });
    }

    #[test]
    fn save_with_unreadable_registry_needs_incoming_keys() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let projects_dir = get_projects_dir().unwrap();
            fs::create_dir_all(&projects_dir).expect("mkdir");
            fs::write(projects_dir.join("damaged.json"), "{ not json").expect("write");

            let keyless = minimal_project("damaged");
            let err = save_project("damaged", &keyless).expect_err("keys cannot be resolved");
            assert!(err.contains("stored keys"), "unexpected error: {err}");

            let mut keyed: Project = serde_json::from_str(&keyless).unwrap();
            fill_missing_project_keys(&mut keyed);
            save_project("damaged", &serde_json::to_string(&keyed).unwrap())
                .expect("a save carrying both keys replaces the damaged entry");
            assert_eq!(load("damaged").id, keyed.id);
        });
    }

    #[test]
    fn import_mints_an_id_when_none_is_committed() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("old-repo");
            fs::create_dir_all(&dir).expect("mkdir");
            fs::write(dir.join(CONFIG_FILE_NAME), r#"{"name":"old-repo"}"#).expect("write");

            import_existing_project(dir.to_str().unwrap()).expect("import");
            let pointer = read_json(&registry_file("old-repo"));
            assert!(uuid::Uuid::parse_str(pointer["id"].as_str().unwrap()).is_ok());
        });
    }

    #[test]
    fn backfill_mints_missing_keys_once() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("repo");
            fs::create_dir_all(&dir).expect("mkdir");
            save_project(
                "with-dir",
                &serde_json::to_string(&project_with_dir("with-dir", &dir)).unwrap(),
            )
            .expect("save");
            save_project("no-dir", &minimal_project("no-dir")).expect("save");

            let first = ensure_project_keys().expect("backfill");
            let both = vec!["no-dir".to_string(), "with-dir".to_string()];
            assert_eq!(
                first,
                ProjectKeyBackfill::Completed {
                    updated: both.clone(),
                    renamed: both,
                    failed: vec![],
                }
            );
            let with_dir = load("with-dir");
            let no_dir = load("no-dir");
            for p in [&with_dir, &no_dir] {
                assert!(uuid::Uuid::parse_str(&p.id).is_ok(), "{} has an id", p.name);
                assert!(uuid::Uuid::parse_str(&p.local_key).is_ok());
                assert_eq!(registry_file(&p.name), registry_stem_file(&p.local_key));
                assert!(!registry_stem_file(&p.name).exists(), "the legacy file moved");
            }
            assert_eq!(read_json(&dir.join(CONFIG_FILE_NAME))["id"], with_dir.id.as_str());

            let files = [
                dir.join(CONFIG_FILE_NAME),
                dir.join(".automatic").join("project.json"),
                registry_stem_file(&with_dir.local_key),
                registry_stem_file(&no_dir.local_key),
            ];
            let before: Vec<String> =
                files.iter().map(|f| fs::read_to_string(f).unwrap()).collect();

            let second = ensure_project_keys().expect("second backfill");
            assert_eq!(
                second,
                ProjectKeyBackfill::Completed {
                    updated: vec![],
                    renamed: vec![],
                    failed: vec![],
                }
            );
            let after: Vec<String> =
                files.iter().map(|f| fs::read_to_string(f).unwrap()).collect();
            assert_eq!(before, after, "a second run changes nothing");
            assert_eq!(load("with-dir").id, with_dir.id);
        });
    }

    #[test]
    fn backfill_keeps_an_id_already_committed() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("repo");
            fs::create_dir_all(&dir).expect("mkdir");
            save_project(
                "half",
                &serde_json::to_string(&project_with_dir("half", &dir)).unwrap(),
            )
            .expect("save");
            let config_path = dir.join(CONFIG_FILE_NAME);
            let mut config = read_json(&config_path);
            config["id"] = serde_json::json!("from-teammate");
            fs::write(&config_path, config.to_string()).expect("write config");

            ensure_project_keys().expect("backfill");
            let loaded = load("half");
            assert_eq!(loaded.id, "from-teammate");
            assert!(uuid::Uuid::parse_str(&loaded.local_key).is_ok());
        });
    }

    #[test]
    fn backfill_with_missing_directory_writes_only_the_registry() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("gone");
            fs::create_dir_all(&dir).expect("mkdir");
            save_project(
                "gone",
                &serde_json::to_string(&project_with_dir("gone", &dir)).unwrap(),
            )
            .expect("save");
            fs::remove_dir_all(&dir).expect("remove folder");

            let result = ensure_project_keys().expect("backfill");
            assert_eq!(
                result,
                ProjectKeyBackfill::Completed {
                    updated: vec!["gone".to_string()],
                    renamed: vec!["gone".to_string()],
                    failed: vec![],
                }
            );
            assert!(!dir.exists(), "the missing folder is not recreated");
            assert!(!registry_stem_file("gone").exists(), "the pointer moved too");
            let pointer = read_json(&registry_file("gone"));
            assert_eq!(pointer["name"], "gone");
            assert_eq!(pointer["directory"], dir.to_str().unwrap());
            let id = pointer["id"].as_str().expect("id cached").to_string();
            let local_key = pointer["local_key"].as_str().expect("local_key cached").to_string();
            assert_eq!(registry_file("gone"), registry_stem_file(&local_key));

            // When the folder returns, the next read writes the cached keys in.
            fs::create_dir_all(&dir).expect("restore folder");
            let loaded = load("gone");
            assert_eq!(loaded.id, id);
            assert_eq!(loaded.local_key, local_key);
            assert_eq!(read_json(&dir.join(CONFIG_FILE_NAME))["id"], id.as_str());
        });
    }

    #[test]
    fn backfill_reports_a_broken_project_and_continues() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let broken_dir = home.path().join("broken");
            fs::create_dir_all(&broken_dir).expect("mkdir");
            save_project(
                "broken",
                &serde_json::to_string(&project_with_dir("broken", &broken_dir)).unwrap(),
            )
            .expect("save");
            fs::write(broken_dir.join(CONFIG_FILE_NAME), "<<<<<<< HEAD\n").expect("conflict");
            save_project("fine", &minimal_project("fine")).expect("save");

            match ensure_project_keys().expect("backfill") {
                ProjectKeyBackfill::Completed {
                    updated,
                    renamed,
                    failed,
                } => {
                    assert_eq!(updated, vec!["fine".to_string()]);
                    assert_eq!(renamed, vec!["fine".to_string()]);
                    assert_eq!(failed.len(), 1);
                    assert_eq!(failed[0].0, "broken");
                }
                other => panic!("unexpected outcome: {:?}", other),
            }
            assert_eq!(
                fs::read_to_string(broken_dir.join(CONFIG_FILE_NAME)).unwrap(),
                "<<<<<<< HEAD\n",
                "a conflicted config is left for the user to fix"
            );
        });
    }

    // ── registry keyed by local_key (project identity plan, stage 3a) ──

    fn keyed_project(name: &str, dir: &std::path::Path) -> Project {
        let mut project = project_with_dir(name, dir);
        prepare_new_project_keys(&mut project).expect("prepare");
        save_project(name, &serde_json::to_string(&project).unwrap()).expect("save");
        project
    }

    fn write_registry(stem: &str, value: serde_json::Value) {
        let projects_dir = get_projects_dir().unwrap();
        fs::create_dir_all(&projects_dir).expect("mkdir");
        fs::write(projects_dir.join(format!("{stem}.json")), value.to_string()).expect("write");
    }

    fn group_members(group: &str) -> Vec<String> {
        let raw = read_group(group).expect("read group");
        serde_json::from_str::<ProjectGroup>(&raw).expect("parse group").projects
    }

    fn save_members(group: &str, members: &[&str]) {
        let data = serde_json::json!({"name": group, "projects": members});
        save_group(group, &data.to_string()).expect("save group");
    }

    #[test]
    fn list_returns_names_for_legacy_and_keyed_files() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("repo");
            fs::create_dir_all(&dir).expect("mkdir");
            let keyed = keyed_project("keyed", &dir);
            save_project("legacy-bare", &minimal_project("legacy-bare")).expect("save");
            // A legacy pointer whose name field disagrees with its stem is
            // listed under its stem, as it always was.
            write_registry("legacy-pointer", serde_json::json!({"name": "drifted", "directory": ""}));

            assert!(registry_stem_file(&keyed.local_key).exists());
            assert!(registry_stem_file("legacy-bare").exists());
            let names = list_projects().expect("list");
            assert_eq!(names, vec!["keyed", "legacy-bare", "legacy-pointer"]);
            assert!(!names.contains(&keyed.local_key), "no key leaks as a name");
        });
    }

    #[test]
    fn keyed_entry_reads_saves_and_deletes_by_name() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("repo");
            fs::create_dir_all(&dir).expect("mkdir");
            let project = keyed_project("site", &dir);
            let registry_path = registry_stem_file(&project.local_key);
            assert!(!registry_stem_file("site").exists(), "no name-keyed file");

            let mut loaded = load("site");
            assert_eq!(loaded.name, "site");
            loaded.description = "edited".into();
            save_project("site", &serde_json::to_string(&loaded).unwrap()).expect("save");
            assert_eq!(load("site").description, "edited");
            assert_eq!(fs::read_dir(get_projects_dir().unwrap()).unwrap().count(), 1);
            assert_eq!(registry_file("site"), registry_path);

            delete_project("site").expect("delete");
            assert!(!registry_path.exists());
            assert!(list_projects().expect("list").is_empty());
        });
    }

    #[test]
    fn directoryless_project_is_saved_under_its_key_and_save_never_renames() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let mut bare = Project {
                name: "bare".into(),
                ..Default::default()
            };
            fill_missing_project_keys(&mut bare);
            save_project("bare", &serde_json::to_string(&bare).unwrap()).expect("save");
            let file = registry_stem_file(&bare.local_key);
            assert_eq!(read_json(&file)["name"], "bare");

            // A save carrying a different name never renames the entry.
            let mut other = load("bare");
            other.name = "something-else".into();
            save_project("bare", &serde_json::to_string(&other).unwrap()).expect("save");
            assert_eq!(read_json(&file)["name"], "bare");
            assert_eq!(list_projects().expect("list"), vec!["bare".to_string()]);
        });
    }

    #[test]
    fn new_project_without_keys_uses_the_legacy_file_name() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            save_project("plain", &minimal_project("plain")).expect("save");
            assert!(registry_stem_file("plain").exists());
            assert!(load("plain").local_key.is_empty(), "a save does not mint keys");
        });
    }

    #[test]
    fn new_entry_never_overwrites_a_file_at_its_key() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            write_registry("k1", serde_json::json!({"name": "owner", "local_key": "k1"}));
            let mut intruder = Project {
                name: "intruder".into(),
                id: "some-id".into(),
                local_key: "k1".into(),
                ..Default::default()
            };
            intruder.description = "would clobber".into();
            let err = save_project("intruder", &serde_json::to_string(&intruder).unwrap())
                .expect_err("the key's file belongs to another project");
            assert!(err.contains("another project"), "unexpected error: {err}");
            assert_eq!(read_json(&registry_stem_file("k1"))["name"], "owner");
        });
    }

    #[test]
    fn two_entries_with_one_name_are_an_error() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            write_registry("k1", serde_json::json!({"name": "twin", "local_key": "k1"}));
            write_registry("k2", serde_json::json!({"name": "Twin", "local_key": "k2"}));

            for err in [
                read_project("twin").expect_err("read"),
                save_project("twin", &minimal_project("twin")).expect_err("save"),
                delete_project("twin").expect_err("delete"),
                rename_project("twin", "single").expect_err("rename"),
                assert_can_create_project("TWIN", "").expect_err("create"),
            ] {
                assert!(err.contains("k1.json") && err.contains("k2.json"), "{err}");
            }
            assert!(registry_stem_file("k1").exists() && registry_stem_file("k2").exists());
        });
    }

    #[test]
    fn create_refuses_a_case_variant_of_an_existing_name() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("website");
            fs::create_dir_all(&dir).expect("mkdir");
            keyed_project("website", &dir);
            let other = home.path().join("other");
            fs::create_dir_all(&other).expect("mkdir");

            let err = assert_can_create_project("WebSite", other.to_str().unwrap())
                .expect_err("case variant clashes");
            assert!(err.contains("already used by the project at"), "{err}");
            assert!(err.contains(dir.to_str().unwrap()), "{err}");
        });
    }

    #[test]
    fn find_by_directory_returns_the_name_not_the_key() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("repo");
            fs::create_dir_all(&dir).expect("mkdir");
            keyed_project("owner", &dir);
            assert_eq!(
                find_project_by_directory(dir.to_str().unwrap()).expect("find"),
                Some("owner".to_string())
            );
        });
    }

    #[test]
    fn rename_edits_a_keyed_entry_in_place() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("repo");
            fs::create_dir_all(&dir).expect("mkdir");
            let project = keyed_project("before", &dir);
            let registry_path = registry_stem_file(&project.local_key);
            save_members("g", &["before", "bystander"]);

            rename_project("before", "after").expect("rename");
            assert_eq!(registry_file("after"), registry_path, "the file did not move");
            assert_eq!(read_json(&registry_path)["name"], "after");
            assert_eq!(read_json(&dir.join(CONFIG_FILE_NAME))["name"], "after");
            assert_eq!(list_projects().expect("list"), vec!["after".to_string()]);
            let loaded = load("after");
            assert_eq!((loaded.id, loaded.local_key), (project.id, project.local_key));
            assert_eq!(group_members("g"), vec!["after", "bystander"]);

            // A case-only rename of the same entry is allowed.
            rename_project("after", "After").expect("case-only rename");
            assert_eq!(registry_file("After"), registry_path);
            assert_eq!(list_projects().expect("list"), vec!["After".to_string()]);
            assert_eq!(group_members("g"), vec!["After", "bystander"]);
        });
    }

    #[test]
    fn rename_refuses_a_name_another_entry_holds() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let a = home.path().join("a");
            let b = home.path().join("b");
            fs::create_dir_all(&a).expect("mkdir");
            fs::create_dir_all(&b).expect("mkdir");
            keyed_project("alpha", &a);
            keyed_project("beta", &b);

            let err = rename_project("alpha", "BETA").expect_err("clash");
            assert!(err.contains("already exists"), "{err}");
            assert_eq!(list_projects().expect("list"), vec!["alpha", "beta"]);
        });
    }

    #[test]
    fn rename_moves_a_legacy_entry_to_the_new_stem() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            save_project("old", &minimal_project("old")).expect("save");
            rename_project("old", "new").expect("rename");
            assert!(!registry_stem_file("old").exists());
            assert_eq!(read_json(&registry_stem_file("new"))["name"], "new");
            assert_eq!(list_projects().expect("list"), vec!["new".to_string()]);

            rename_project("new", "New").expect("case-only rename");
            assert_eq!(list_projects().expect("list"), vec!["New".to_string()]);
            assert_eq!(load("New").name, "New");
        });
    }

    #[test]
    fn migration_renames_legacy_files_once() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("repo");
            fs::create_dir_all(&dir).expect("mkdir");
            save_project(
                "legacy",
                &serde_json::to_string(&project_with_dir("legacy", &dir)).unwrap(),
            )
            .expect("save");
            // A drifted name field: the stem is the name callers know.
            let legacy_path = registry_stem_file("legacy");
            let mut pointer = read_json(&legacy_path);
            pointer["name"] = serde_json::json!("drifted");
            fs::write(&legacy_path, pointer.to_string()).expect("write");

            let first = ensure_project_keys().expect("backfill");
            assert_eq!(
                first,
                ProjectKeyBackfill::Completed {
                    updated: vec!["legacy".to_string()],
                    renamed: vec!["legacy".to_string()],
                    failed: vec![],
                }
            );
            let loaded = load("legacy");
            let keyed_path = registry_stem_file(&loaded.local_key);
            assert!(!legacy_path.exists());
            assert_eq!(read_json(&keyed_path)["name"], "legacy", "the stem became the name");
            assert_eq!(list_projects().expect("list"), vec!["legacy".to_string()]);

            let before = fs::read_to_string(&keyed_path).unwrap();
            let second = ensure_project_keys().expect("second run");
            assert_eq!(
                second,
                ProjectKeyBackfill::Completed {
                    updated: vec![],
                    renamed: vec![],
                    failed: vec![],
                }
            );
            assert_eq!(fs::read_to_string(&keyed_path).unwrap(), before);
        });
    }

    #[test]
    fn migration_adds_a_local_key_the_pointer_lacks() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("repo");
            fs::create_dir_all(&dir).expect("mkdir");
            let project = keyed_project("half", &dir);
            // An old pointer without keys, while the folder holds them.
            fs::remove_file(registry_stem_file(&project.local_key)).expect("remove");
            write_registry(
                "half",
                serde_json::json!({"name": "half", "directory": dir.to_str().unwrap()}),
            );

            match ensure_project_keys().expect("backfill") {
                ProjectKeyBackfill::Completed {
                    updated,
                    renamed,
                    failed,
                } => {
                    assert!(updated.is_empty(), "the folder's keys are kept");
                    assert_eq!(renamed, vec!["half".to_string()]);
                    assert!(failed.is_empty(), "{failed:?}");
                }
                other => panic!("unexpected outcome: {other:?}"),
            }
            let pointer = read_json(&registry_stem_file(&project.local_key));
            assert_eq!(pointer["local_key"], project.local_key.as_str());
            assert_eq!(pointer["id"], project.id.as_str());
            assert_eq!(pointer["name"], "half");
        });
    }

    #[test]
    fn migration_reports_an_occupied_target_and_changes_nothing() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let legacy = serde_json::json!({
                "name": "site", "directory": "", "id": "id-1", "local_key": "k1"
            });
            let occupant = serde_json::json!({"name": "other", "id": "id-2", "local_key": "k1"});
            write_registry("site", legacy);
            write_registry("k1", occupant);
            let site_before = fs::read_to_string(registry_stem_file("site")).unwrap();
            let k1_before = fs::read_to_string(registry_stem_file("k1")).unwrap();

            match ensure_project_keys().expect("backfill") {
                ProjectKeyBackfill::Completed {
                    renamed, failed, ..
                } => {
                    assert!(renamed.is_empty());
                    assert_eq!(failed.len(), 1, "{failed:?}");
                    assert_eq!(failed[0].0, "site");
                    assert!(failed[0].1.contains("already exists"), "{}", failed[0].1);
                }
                other => panic!("unexpected outcome: {other:?}"),
            }
            assert_eq!(fs::read_to_string(registry_stem_file("site")).unwrap(), site_before);
            assert_eq!(fs::read_to_string(registry_stem_file("k1")).unwrap(), k1_before);
            assert_eq!(list_projects().expect("list"), vec!["other", "site"]);
        });
    }

    #[test]
    fn group_scrub_after_migration_keeps_every_member() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("repo");
            fs::create_dir_all(&dir).expect("mkdir");
            save_project(
                "with-dir",
                &serde_json::to_string(&project_with_dir("with-dir", &dir)).unwrap(),
            )
            .expect("save");
            save_project("no-dir", &minimal_project("no-dir")).expect("save");
            save_members("g", &["with-dir", "no-dir", "deleted-long-ago"]);

            ensure_project_keys().expect("backfill");
            let live = list_projects().expect("list");
            assert_eq!(live, vec!["no-dir", "with-dir"]);
            scrub_orphan_project_references(&live).expect("scrub");
            assert_eq!(group_members("g"), vec!["with-dir", "no-dir"]);
        });
    }

    // ── identifiers (project identity plan, stage 3b step 1) ────────────

    #[test]
    fn read_save_rename_delete_by_local_key_hit_the_named_entry() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("repo");
            fs::create_dir_all(&dir).expect("mkdir");
            let project = keyed_project("site", &dir);
            let key = project.local_key.clone();
            let registry_path = registry_stem_file(&key);
            save_members("g", &["site"]);

            let by_key = load(&key);
            assert_eq!(by_key.name, "site", "a read by key returns the name");
            assert_eq!(by_key.local_key, key);

            let mut edited = by_key;
            edited.description = "via key".into();
            save_project(&key, &serde_json::to_string(&edited).unwrap()).expect("save by key");
            assert_eq!(load("site").description, "via key");
            assert_eq!(fs::read_dir(get_projects_dir().unwrap()).unwrap().count(), 1);
            assert_eq!(read_json(&registry_path)["name"], "site", "no key became a name");

            rename_project(&key, "renamed").expect("rename by key");
            assert_eq!(registry_file("renamed"), registry_path);
            assert_eq!(group_members("g"), vec!["renamed"], "groups got the name, not the key");

            delete_project(&key).expect("delete by key");
            assert!(!registry_path.exists());
            assert!(group_members("g").is_empty());
        });
    }

    #[test]
    fn identifier_helpers_resolve_keys_and_names() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("repo");
            fs::create_dir_all(&dir).expect("mkdir");
            let project = keyed_project("Website", &dir);

            assert_eq!(canonical_project_name(&project.local_key).unwrap(), "Website");
            assert_eq!(canonical_project_name("website").unwrap(), "Website");
            assert_eq!(
                canonical_project_name("nope").unwrap_err(),
                "Project 'nope' not found",
                "same text read_project uses"
            );
            assert_eq!(resolve_project_name("nope").unwrap(), None);
            assert_eq!(resolve_project_name("../x").unwrap(), None);

            assert_eq!(project_store_name(&project.local_key).unwrap(), "Website");
            assert_eq!(project_store_name("website").unwrap(), "Website");
            assert_eq!(
                project_store_name("orphan").unwrap(),
                "orphan",
                "an unknown identifier passes through unchanged"
            );
            assert_eq!(project_store_name("").unwrap(), "");
        });
    }

    #[test]
    fn store_name_passes_through_a_name_two_entries_share() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            write_registry("k1", serde_json::json!({"name": "site", "local_key": "k1"}));
            write_registry("k2", serde_json::json!({"name": "SITE", "local_key": "k2"}));
            assert_eq!(project_store_name("site").unwrap(), "site");
            assert_eq!(project_store_name("k2").unwrap(), "SITE", "a key is still unique");
            assert!(canonical_project_name("site").is_err());
        });
    }

    #[test]
    fn summaries_list_every_entry_sorted_with_keys() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("repo");
            fs::create_dir_all(&dir).expect("mkdir");
            let keyed = keyed_project("zeta", &dir);
            save_project("alpha", &minimal_project("alpha")).expect("save");
            fs::write(get_projects_dir().unwrap().join("broken.json"), "{ nope").unwrap();

            let summaries = get_project_summaries().expect("summaries");
            assert_eq!(
                summaries,
                vec![
                    ProjectSummary {
                        local_key: String::new(),
                        id: String::new(),
                        name: "alpha".into(),
                        directory: String::new(),
                    },
                    ProjectSummary {
                        local_key: String::new(),
                        id: String::new(),
                        name: "broken".into(),
                        directory: String::new(),
                    },
                    ProjectSummary {
                        local_key: keyed.local_key.clone(),
                        id: keyed.id.clone(),
                        name: "zeta".into(),
                        directory: dir.to_str().unwrap().to_string(),
                    },
                ]
            );
            let json = serde_json::to_value(&summaries[2]).unwrap();
            assert_eq!(
                json,
                serde_json::json!({
                    "local_key": keyed.local_key,
                    "id": keyed.id,
                    "name": "zeta",
                    "directory": dir.to_str().unwrap(),
                })
            );
        });
    }

    #[test]
    fn summaries_never_write() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("repo");
            fs::create_dir_all(&dir).expect("mkdir");
            let project = keyed_project("site", &dir);
            // A pointer and folder files that `read_project` would rewrite:
            // the pointer lacks `id`, and the config lacks enrichment.
            let pointer = registry_stem_file(&project.local_key);
            fs::write(
                &pointer,
                serde_json::json!({
                    "name": "site",
                    "directory": dir.to_str().unwrap(),
                    "local_key": project.local_key,
                })
                .to_string(),
            )
            .unwrap();
            let snapshot = |path: &std::path::Path| fs::read(path).unwrap();
            let files = [
                pointer.clone(),
                dir.join(CONFIG_FILE_NAME),
                dir.join(".automatic").join("project.json"),
            ];
            let before: Vec<Vec<u8>> = files.iter().map(|f| snapshot(f)).collect();

            let summaries = get_project_summaries().expect("summaries");
            assert_eq!(summaries[0].id, project.id, "id comes from the folder's config");

            let after: Vec<Vec<u8>> = files.iter().map(|f| snapshot(f)).collect();
            assert_eq!(before, after, "no file was rewritten");
        });
    }

    #[test]
    fn summaries_follow_the_stage_2_key_precedence() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("repo");
            fs::create_dir_all(&dir).expect("mkdir");
            let project = keyed_project("site", &dir);
            let pointer = registry_stem_file(&project.local_key);
            let mut config = read_json(&dir.join(CONFIG_FILE_NAME));
            let state_path = dir.join(".automatic").join("project.json");
            let mut state = read_json(&state_path);

            // id: the folder's config beats the registry cache.
            let mut value = read_json(&pointer);
            value["id"] = "cached-id".into();
            fs::write(&pointer, value.to_string()).unwrap();
            config["id"] = "config-id".into();
            fs::write(dir.join(CONFIG_FILE_NAME), config.to_string()).unwrap();
            // local_key: the registry beats the state file.
            state["local_key"] = "copied-key".into();
            fs::write(&state_path, state.to_string()).unwrap();

            let summary = &get_project_summaries().expect("summaries")[0];
            assert_eq!(summary.id, "config-id");
            assert_eq!(summary.local_key, project.local_key);

            // Without a committed id, the registry's cached id is used.
            config.as_object_mut().unwrap().remove("id");
            fs::write(dir.join(CONFIG_FILE_NAME), config.to_string()).unwrap();
            assert_eq!(get_project_summaries().unwrap()[0].id, "cached-id");

            // Without a pointer key, the state file's key is used.
            let mut value = read_json(&pointer);
            value.as_object_mut().unwrap().remove("local_key");
            let legacy = get_projects_dir().unwrap().join("site.json");
            fs::write(&legacy, value.to_string()).unwrap();
            fs::remove_file(&pointer).unwrap();
            assert_eq!(get_project_summaries().unwrap()[0].local_key, "copied-key");
        });
    }

    #[test]
    fn local_keys_by_name_skips_projects_without_a_key() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("repo");
            fs::create_dir_all(&dir).expect("mkdir");
            let keyed = keyed_project("site", &dir);
            save_project("plain", &minimal_project("plain")).expect("save");

            let keys = project_local_keys_by_name().expect("keys");
            assert_eq!(keys.get("site"), Some(&keyed.local_key));
            assert_eq!(keys.get("plain"), None);
            assert_eq!(keys.len(), 1);
        });
    }
}
