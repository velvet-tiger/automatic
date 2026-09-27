use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use super::*;

// ── Projects ─────────────────────────────────────────────────────────────────
//
// A project with a directory stores its config in `.automatic.json` and its
// machine-local state in `.automatic/project.json` (see `project_layout`).
// A lightweight registry entry at `~/.automatic/projects/{name}.json` maps project
// names to their directories so we can enumerate them.  When a project has no
// directory set yet, the full config lives in the registry file as a fallback.

pub fn list_projects() -> Result<Vec<String>, String> {
    let projects_dir = get_projects_dir()?;

    if !projects_dir.exists() {
        return Ok(Vec::new());
    }

    let mut projects = Vec::new();
    let entries = fs::read_dir(&projects_dir).map_err(|e| e.to_string())?;

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if is_valid_name(stem) {
                        projects.push(stem.to_string());
                    }
                }
            }
        }
    }

    Ok(projects)
}

pub fn read_project(name: &str) -> Result<String, String> {
    if !is_valid_name(name) {
        return Err("Invalid project name".into());
    }
    let projects_dir = get_projects_dir()?;
    let registry_path = projects_dir.join(format!("{}.json", name));

    if !registry_path.exists() {
        return Err(format!("Project '{}' not found", name));
    }

    let raw = fs::read_to_string(&registry_path).map_err(|e| e.to_string())?;
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

    // The committed config can carry a different name from the registry key,
    // for example after a teammate renamed the project and this machine
    // pulled. Callers save back with `project.name`, so returning the config
    // name would write a second registry entry. Set after the write-back so
    // a read never rewrites the committed name on its own.
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

/// Read only the `directory` field from a registry entry, without loading or
/// enriching the full project config (avoids write-back side effects).
pub(crate) fn registry_directory_for(name: &str) -> Result<Option<String>, String> {
    let projects_dir = get_projects_dir()?;
    let registry_path = projects_dir.join(format!("{}.json", name));
    if !registry_path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&registry_path).map_err(|e| e.to_string())?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project registry data: {}", e))?;
    Ok(value
        .get("directory")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty()))
}

/// Return the registry name of the project that owns `directory`, if any.
pub fn find_project_by_directory(directory: &str) -> Result<Option<String>, String> {
    if directory.is_empty() {
        return Ok(None);
    }
    for name in list_projects()? {
        let Some(existing_dir) = registry_directory_for(&name)? else {
            continue;
        };
        if directories_equivalent(&existing_dir, directory) {
            return Ok(Some(name));
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

    let projects_dir = get_projects_dir()?;
    let registry_path = projects_dir.join(format!("{}.json", name));
    if registry_path.exists() {
        // Name the other project's directory so the user can tell an
        // unrelated project with the same folder name from this one.
        return Err(match registry_directory_for(name)? {
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
/// `~/.automatic/projects/{name}.json`. Returns the adopted project name.
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
    let registry_path = projects_dir.join(format!("{}.json", name));
    if registry_path.exists() {
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

    let registry_path = projects_dir.join(format!("{}.json", name));

    // Keys are system-managed. Callers may hold none (sync paths, tests) or
    // stale ones (an editor opened before a `git pull` changed the committed
    // id). A stored key always wins; the incoming key is used only where
    // nothing is stored, which is how the create paths and the backfill
    // write keys they just minted.
    match stored_project_keys(&registry_path, &project.directory) {
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

    if !project.directory.is_empty() {
        write_project_files(&project)?;
        write_registry_pointer(&registry_path, &project)?;
    } else {
        // No directory yet — write full config to registry
        let pretty = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;
        fs::write(&registry_path, &pretty).map_err(|e| e.to_string())?;
    }

    Ok(())
}

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

    let projects_dir = get_projects_dir()?;
    let old_registry = projects_dir.join(format!("{}.json", old_name));
    let new_registry = projects_dir.join(format!("{}.json", new_name));

    if !old_registry.exists() {
        return Err(format!("Project '{}' not found", old_name));
    }
    // Only block if the target file exists and is a genuinely different project
    // (not just a case change on a case-insensitive filesystem like macOS APFS).
    if new_registry.exists() {
        // Compare canonical paths: on a case-insensitive FS, a case-only rename
        // will resolve both paths to the same inode.
        let old_canon = old_registry.canonicalize().map_err(|e| e.to_string())?;
        let new_canon = new_registry.canonicalize().map_err(|e| e.to_string())?;
        if old_canon != new_canon {
            return Err(format!("A project named '{}' already exists", new_name));
        }
    }

    // Read the full project (via read_project which resolves directory-based configs)
    let raw = read_project(old_name)?;
    let mut project: Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    // Update the name field
    project.name = new_name.to_string();
    project.updated_at = chrono::Utc::now().to_rfc3339();

    let pretty = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;

    // Write the in-directory config with the updated name
    if !project.directory.is_empty() {
        if has_project_files(&project.directory)
            || PathBuf::from(&project.directory)
                .join(".automatic")
                .exists()
        {
            write_project_files(&project)?;
        }

        // Write new registry entry (lightweight pointer)
        write_registry_pointer(&new_registry, &project)?;
    } else {
        // No directory — write full config to new registry entry
        fs::write(&new_registry, &pretty).map_err(|e| e.to_string())?;
    }

    // On a case-insensitive filesystem (macOS APFS/HFS+), a case-only rename
    // means old_registry and new_registry point to the same inode.  In that
    // case fs::write already updated the content above, so we just need to
    // rename the file to get the new casing on disk.  A plain remove would
    // delete the only copy.
    let same_file = old_registry.canonicalize().ok() == new_registry.canonicalize().ok();
    if same_file {
        // fs::rename handles case-only renames correctly on APFS/HFS+
        fs::rename(&old_registry, &new_registry).map_err(|e| e.to_string())?;
    } else {
        fs::remove_file(&old_registry).map_err(|e| e.to_string())?;
    }

    // Update group membership lists so references follow the rename. Best-
    // effort — see delete_project for the rationale.
    if let Err(e) = rename_project_in_all_groups(old_name, new_name) {
        eprintln!(
            "rename_project: could not update group references '{}' -> '{}': {}",
            old_name, new_name, e
        );
    }

    Ok(())
}

pub fn delete_project(name: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid project name".into());
    }
    let projects_dir = get_projects_dir()?;
    let registry_path = projects_dir.join(format!("{}.json", name));

    // Try to read the project to clean up the project-directory config
    if registry_path.exists() {
        if let Ok(raw) = fs::read_to_string(&registry_path) {
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

        fs::remove_file(&registry_path).map_err(|e| e.to_string())?;
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

        let registry_path = projects_dir.join(format!("{}.json", name));

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
        let registry_path = projects_dir.join(format!("{}.json", name));
        if !registry_path.exists() {
            return Err(format!("Project '{}' not found", name));
        }

        let raw = fs::read_to_string(&registry_path).map_err(|e| e.to_string())?;
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
        if !projects_dir.exists() {
            return Ok(Vec::new());
        }
        let mut projects = Vec::new();
        let entries = fs::read_dir(projects_dir).map_err(|e| e.to_string())?;
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        if is_valid_name(stem) {
                            projects.push(stem.to_string());
                        }
                    }
                }
            }
        }
        Ok(projects)
    }

    /// Delete a project using an explicit projects dir.
    pub fn delete_project_at(projects_dir: &PathBuf, name: &str) -> Result<(), String> {
        if !is_valid_name(name) {
            return Err("Invalid project name".into());
        }
        let registry_path = projects_dir.join(format!("{}.json", name));
        if registry_path.exists() {
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

            // The registry pointer must now exist and point at this dir.
            let projects_dir = get_projects_dir().expect("projects dir");
            let pointer = projects_dir.join("adopted.json");
            assert!(pointer.exists(), "registry pointer must be written");
            let raw = fs::read_to_string(&pointer).expect("read pointer");
            let value: serde_json::Value =
                serde_json::from_str(&raw).expect("parse pointer");
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
            let pointer = read_json(&get_projects_dir().unwrap().join("keyed.json"));

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
            let pointer = read_json(&get_projects_dir().unwrap().join("keep.json"));
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
    fn read_returns_the_registry_key_as_name() {
        use crate::core::paths::with_test_home;

        let home = tempfile::tempdir().expect("tempdir");
        with_test_home(home.path().to_path_buf(), || {
            let dir = home.path().join("repo");
            fs::create_dir_all(&dir).expect("mkdir");
            let project = project_with_dir("local-name", &dir);
            save_project("local-name", &serde_json::to_string(&project).unwrap()).expect("save");

            let config_path = dir.join(CONFIG_FILE_NAME);
            let mut config = read_json(&config_path);
            config["name"] = serde_json::json!("renamed-upstream");
            fs::write(&config_path, config.to_string()).expect("write config");

            let loaded = load("local-name");
            assert_eq!(loaded.name, "local-name", "name follows the registry key");
            assert_eq!(
                read_json(&config_path)["name"],
                "renamed-upstream",
                "a read alone does not rewrite the committed name"
            );

            // Saving what was read writes back under the same registry entry.
            save_project(&loaded.name, &serde_json::to_string(&loaded).unwrap()).expect("save");
            let mut names = list_projects().expect("list");
            names.sort();
            assert_eq!(names, vec!["local-name".to_string()], "no second registry entry");
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
            let pointer = read_json(&get_projects_dir().unwrap().join(format!("{name}.json")));
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
            let pointer = read_json(&get_projects_dir().unwrap().join(format!("{name}.json")));
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
            let pointer = read_json(&get_projects_dir().unwrap().join("stale.json"));
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
            let pointer = read_json(&get_projects_dir().unwrap().join("old-repo.json"));
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
            assert_eq!(
                first,
                ProjectKeyBackfill::Completed {
                    updated: vec!["no-dir".to_string(), "with-dir".to_string()],
                    failed: vec![],
                }
            );
            let with_dir = load("with-dir");
            let no_dir = load("no-dir");
            for p in [&with_dir, &no_dir] {
                assert!(uuid::Uuid::parse_str(&p.id).is_ok(), "{} has an id", p.name);
                assert!(uuid::Uuid::parse_str(&p.local_key).is_ok());
            }
            assert_eq!(read_json(&dir.join(CONFIG_FILE_NAME))["id"], with_dir.id.as_str());

            let projects_dir = get_projects_dir().unwrap();
            let files = [
                dir.join(CONFIG_FILE_NAME),
                dir.join(".automatic").join("project.json"),
                projects_dir.join("with-dir.json"),
                projects_dir.join("no-dir.json"),
            ];
            let before: Vec<String> =
                files.iter().map(|f| fs::read_to_string(f).unwrap()).collect();

            let second = ensure_project_keys().expect("second backfill");
            assert_eq!(
                second,
                ProjectKeyBackfill::Completed {
                    updated: vec![],
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
                    failed: vec![],
                }
            );
            assert!(!dir.exists(), "the missing folder is not recreated");
            let pointer = read_json(&get_projects_dir().unwrap().join("gone.json"));
            assert_eq!(pointer["directory"], dir.to_str().unwrap());
            let id = pointer["id"].as_str().expect("id cached").to_string();
            let local_key = pointer["local_key"].as_str().expect("local_key cached").to_string();

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
                ProjectKeyBackfill::Completed { updated, failed } => {
                    assert_eq!(updated, vec!["fine".to_string()]);
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
}
