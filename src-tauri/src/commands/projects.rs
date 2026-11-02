use crate::activity::{self, ActivityEvent};
use crate::context;
use crate::core;
use crate::sync;
use serde::Serialize;
use std::collections::HashMap;

use super::project_sweeps::{hold_library_writes, SweepProgress};

// ── Projects ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_projects() -> Result<Vec<String>, String> {
    core::list_projects()
}

/// Every registered project as `{local_key, id, name, directory}`, sorted by
/// name. Reads only, without the enrichment `read_project` does.
#[tauri::command]
pub fn get_project_summaries() -> Result<Vec<core::ProjectSummary>, String> {
    core::get_project_summaries()
}

/// Read a project by identifier: its `local_key` or its name.
#[tauri::command]
pub fn read_project(name: &str) -> Result<String, String> {
    core::read_project(name)
}

#[derive(Serialize)]
struct RebuildPreviewCategory {
    key: &'static str,
    label: &'static str,
    automatic: Vec<String>,
    disk: Vec<String>,
    added: Vec<String>,
    removed: Vec<String>,
}

#[derive(Serialize)]
struct RebuildPreview {
    project_name: String,
    categories: Vec<RebuildPreviewCategory>,
    changed: bool,
    /// Sub-agent and command files rebuild could not read, each with the
    /// reason.  They stay on disk, and any stored version of them stays in
    /// the project.
    refused_files: Vec<String>,
}

#[tauri::command]
pub fn preview_rebuild_project(name: &str) -> Result<String, String> {
    let name = &crate::core::canonical_project_ident(name)?;
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let (rebuilt, refused_files) = sync::rebuild_project_with_refusals(&project)?;

    let categories = vec![
        diff_category("agents", "Agent Tools", &project.agents, &rebuilt.agents),
        diff_category(
            "instruction_files",
            "Instruction Files",
            &instruction_files_for(&project),
            &instruction_files_for(&rebuilt),
        ),
        diff_category("skills", "Skills", &project.skills, &rebuilt.skills),
        diff_category(
            "custom_skills",
            "Project Skills",
            &custom_skill_names(&project),
            &custom_skill_names(&rebuilt),
        ),
        diff_category(
            "mcp_servers",
            "MCP Servers",
            &project.mcp_servers,
            &rebuilt.mcp_servers,
        ),
        diff_category("tools", "Tools", &project.tools, &rebuilt.tools),
        diff_category(
            "user_agents",
            "Workspace Sub-Agents",
            &project.user_agents,
            &rebuilt.user_agents,
        ),
        diff_category(
            "custom_agents",
            "Project Sub-Agents",
            &custom_agent_names(&project),
            &custom_agent_names(&rebuilt),
        ),
        diff_category(
            "user_commands",
            "Workspace Commands",
            &project.user_commands,
            &rebuilt.user_commands,
        ),
        diff_category(
            "custom_commands",
            "Project Commands",
            &custom_command_names(&project),
            &custom_command_names(&rebuilt),
        ),
    ];

    let preview = RebuildPreview {
        project_name: project.name.clone(),
        changed: categories
            .iter()
            .any(|category| !category.added.is_empty() || !category.removed.is_empty()),
        categories,
        refused_files,
    };

    serde_json::to_string_pretty(&preview).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn autodetect_project_dependencies(name: &str) -> Result<String, String> {
    let name = &crate::core::canonical_project_ident(name)?;
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let updated = sync::autodetect_project_dependencies(&project)?;
    serde_json::to_string_pretty(&updated).map_err(|e| e.to_string())
}

/// Scan `root` for folders that look like agent projects. Runs off the main
/// thread: a scan of a home folder can open tens of thousands of folders.
#[tauri::command]
pub async fn discover_projects(
    root: String,
) -> Result<core::project_discovery::DiscoveryReport, String> {
    tokio::task::spawn_blocking(move || {
        core::project_discovery::discover_projects(
            &root,
            core::project_discovery::DiscoveryLimits::default(),
        )
    })
    .await
    .map_err(|e| format!("Project scan failed: {}", e))?
}

/// The outcome of adding one folder in [`add_discovered_projects`].
#[derive(Debug, Serialize)]
pub struct AddedDiscoveredProject {
    pub directory: String,
    /// The new project's `local_key`, when it was added.
    pub local_key: Option<String>,
    /// The project's name. An imported config keeps its own name, which may
    /// differ from the folder name.
    pub name: Option<String>,
    /// True when an imported config asked for read-only mode.
    pub read_only: bool,
    pub error: Option<String>,
}

/// Add each folder in `directories` as a project. A new folder goes through
/// the Add Project wizard's create path, which names it after the folder,
/// adopts the agents found on disk and runs the first sync. A folder with an
/// unregistered Automatic config is imported and keeps that config. One
/// failure does not stop the rest; each folder reports its own result.
/// `import_read_only` holds the user's mode choice for folders that are
/// imported. A folder it omits is imported read only: nobody reviewed it, and
/// its committed `read_only` hint cannot be trusted to ask for that.
#[tauri::command]
pub async fn add_discovered_projects(
    directories: Vec<String>,
    created_by: Option<String>,
    import_read_only: HashMap<String, bool>,
) -> Result<Vec<AddedDiscoveredProject>, String> {
    tokio::task::spawn_blocking(move || {
        Ok(add_discovered_folders(
            directories,
            created_by.as_deref(),
            &import_read_only,
        ))
    })
    .await
    .map_err(|e| format!("Adding projects failed: {}", e))?
}

/// Synchronous body of [`add_discovered_projects`].
fn add_discovered_folders(
    directories: Vec<String>,
    created_by: Option<&str>,
    import_read_only: &HashMap<String, bool>,
) -> Vec<AddedDiscoveredProject> {
    directories
        .into_iter()
        .map(|directory| {
            let read_only = import_read_only.get(&directory).copied();
            match add_discovered_project(&directory, created_by, read_only) {
                Ok(added) => AddedDiscoveredProject {
                    directory,
                    local_key: Some(added.local_key),
                    name: Some(added.name),
                    read_only: added.read_only,
                    error: None,
                },
                Err(e) => AddedDiscoveredProject {
                    directory,
                    local_key: None,
                    name: None,
                    read_only: false,
                    error: Some(e),
                },
            }
        })
        .collect()
}

/// Validate one discovered folder and add it as a project. A folder with an
/// unregistered Automatic config is imported, keeping that config, as the
/// wizard's Import choice does. Any other folder gets a new project.
fn add_discovered_project(
    directory: &str,
    created_by: Option<&str>,
    read_only: Option<bool>,
) -> Result<core::ImportedProject, String> {
    let path = std::path::Path::new(directory);
    if !path.is_absolute() {
        return Err(format!("'{}' is not an absolute folder path.", directory));
    }
    // The scan reports canonical paths and never follows symlinks. A path
    // that differs from its canonical form holds a symlink or `..`, so it did
    // not come from the scan.
    let canonical = core::project_discovery::canonical_path(path)
        .map_err(|e| format!("Cannot read folder '{}': {}", directory, e))?;
    if canonical != path {
        return Err(format!(
            "'{}' goes through a symlink or '..'. Add it with Add Project instead.",
            directory
        ));
    }
    if !canonical.is_dir() {
        return Err(format!("'{}' is not a folder.", directory));
    }
    // Home holds each agent's global config. Syncing it as a project would
    // rewrite that config.
    let home =
        crate::agent::home_dir().and_then(|h| core::project_discovery::canonical_path(&h).ok());
    if home.as_deref() == Some(canonical.as_path()) {
        return Err("Your home folder cannot be added as a project.".to_string());
    }

    // The folder may have been added, or gained a config, since the scan.
    match core::inspect_project_directory(directory)? {
        core::DirectoryStatus::Available => {}
        core::DirectoryStatus::RegisteredHere { name, .. } => {
            return Err(format!("Already added as project '{}'.", name));
        }
        core::DirectoryStatus::OrphanConfig { .. } => {
            return import_existing_project(directory, Some(read_only.unwrap_or(true)));
        }
    }

    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| core::is_valid_name(s))
        .ok_or_else(|| format!("Cannot name a project after '{}'.", directory))?;

    let now = chrono::Utc::now().to_rfc3339();
    // Built as JSON so serde fills every other field with its default, the
    // same as the wizard's stub.
    let mut stub = serde_json::json!({
        "name": name,
        "directory": directory,
        "created_at": now,
        "updated_at": now,
    });
    if let Some(user) = created_by.filter(|u| !u.is_empty()) {
        stub["created_by"] = serde_json::Value::String(user.to_string());
    }
    // A bulk add would otherwise fire one forced AI call per folder at once.
    // Each project gets its recommendations on its next save instead, under
    // the normal once-a-day throttle.
    let local_key =
        save_project_with(name, &stub.to_string(), true, NewProjectRecommendations::Skip)?;
    Ok(core::ImportedProject {
        name: name.to_string(),
        local_key,
        read_only: false,
    })
}

/// Whether creating a project runs the forced first AI recommendations call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NewProjectRecommendations {
    Run,
    Skip,
}

/// Save a project and sync it. `name` is an identifier (a `local_key`, or a
/// name). Returns the saved project's `local_key` (its name while it has no
/// key), which the Add Project wizard uses to address the project it just
/// created: the name may be shared with another project.
#[tauri::command(async)]
pub fn save_project(name: &str, data: &str, creating: Option<bool>) -> Result<String, String> {
    save_project_with(
        name,
        data,
        creating.unwrap_or(false),
        NewProjectRecommendations::Run,
    )
}

/// Body of [`save_project`]. `recommendations` decides whether a new
/// project gets the forced first AI recommendations call.
fn save_project_with(
    name: &str,
    data: &str,
    creating: bool,
    recommendations: NewProjectRecommendations,
) -> Result<String, String> {
    let _writes = hold_library_writes();
    let mut incoming: core::Project =
        serde_json::from_str(data).map_err(|e| format!("Invalid project data: {}", e))?;
    // Add Project wizard: refuse a directory another project owns, then give
    // the new project its identity keys before the first write. The name is
    // not looked up: another project may already use it. From here on the
    // new project is addressed by its fresh `local_key`.
    let ident = if creating {
        core::assert_can_create_project(name, &incoming.directory)?;
        core::prepare_new_project_keys(&mut incoming)?;
        core::project_ident(&incoming).to_string()
    } else {
        // An identifier that resolves to nothing is the name of a new project.
        core::resolve_project_ident(name)?.unwrap_or_else(|| name.to_string())
    };
    let name = &ident;

    // Profiles: bring the project's lists in step with its attached profiles
    // before anything is persisted or synced. This one hook covers the
    // editor, the create wizard, and attach/detach done by editing
    // `profiles`. See `core::reconcile_project_profiles`.
    //
    // Group profiles go first: they decide which profiles are attached, and
    // the profile reconcile then turns that into entries. Membership lives
    // in the group files, so a stale editor copy cannot drop a profile a
    // group provides.
    //
    // An unreadable group leaves membership unknown, so the group reconciles
    // are skipped: they would release everything that group provides. The
    // save still goes ahead, because the bad file may have nothing to do
    // with this project and failing would block every project's autosave.
    let member_groups = core::groups_listing(name);
    if let Err(e) = &member_groups {
        eprintln!("Skipping the group reconcile for '{}': {}", name, e);
    }
    if let Ok(groups) = &member_groups {
        core::reconcile_group_profiles(&mut incoming, groups);
    }
    core::reconcile_project_profiles(&mut incoming);
    // Groups: same idea for contexts. Membership lives in the group files,
    // so a stale editor copy cannot drop or duplicate a group's contexts.
    if let Ok(groups) = &member_groups {
        core::reconcile_group_contexts(&mut incoming, groups);
    }

    let reconciled = serde_json::to_string_pretty(&incoming).map_err(|e| e.to_string())?;
    let data: &str = &reconciled;

    // No directory configured yet -- just persist to the registry and return.
    // There is nothing to sync until the user has pointed us at a real directory.
    if incoming.directory.is_empty() {
        core::save_project(name, data)?;
        return Ok(name.clone());
    }

    // Detect whether this is a brand-new project (no existing registry entry).
    let is_new = core::read_project(name).is_err();

    if is_new {
        // ── Case 1: Project is being added for the first time ─────────────
        //
        // Save the initial state so the project exists in the registry even if
        // the subsequent autodetect fails for any reason.  Then run full
        // autodetect to discover the skills and MCP servers already present
        // in the directory.  This is the only path that adopts agents found
        // on disk, and only when the creator chose none.  The enriched
        // project config is written back to the registry by the sync.
        //
        // Nothing is deleted during this step -- autodetect only adds findings.
        core::save_project(name, data)?;

        // Log project creation.
        activity::log(name, ActivityEvent::ProjectCreated, "Project created", &incoming.name);

        // Errors are intentionally swallowed: partial success (project saved
        // but no agent configs written because the directory has no AI tools)
        // is better than returning a hard error to the frontend.
        let written = sync::sync_new_project_from_existing_files(&incoming);
        if let Ok(ref files) = written {
            if !files.is_empty() {
                let detail = format!(
                    "{} file{}",
                    files.len(),
                    if files.len() == 1 { "" } else { "s" }
                );
                activity::log(
                    name,
                    ActivityEvent::ProjectSynced,
                    "Synced agent configs",
                    &detail,
                );
            }
        }
    } else {
        // ── Case 2 / ongoing saves: Existing project update ───────────────
        //
        // Use sync_without_autodetect so the user's explicit agent/skill
        // removals are respected -- we never re-add an agent the user
        // intentionally removed just because its config files still exist.
        //
        // Exception: when the user *adds* a new agent, read its existing
        // config files (if any) to discover MCP servers it already has
        // configured, and merge those into the project so they are not
        // silently discarded when Automatic writes its own config.
        let existing_project = core::read_project(name)
            .ok()
            .and_then(|raw| serde_json::from_str::<core::Project>(&raw).ok());

        let mut enriched = incoming.clone();

        if let Some(ref existing) = existing_project {
            // ── Diff and log agent changes ───────────────────────────────
            for agent in incoming
                .agents
                .iter()
                .filter(|a| !existing.agents.contains(a))
            {
                activity::log(name, ActivityEvent::AgentAdded, "Agent added", agent);
            }
            for agent in existing
                .agents
                .iter()
                .filter(|a| !incoming.agents.contains(a))
            {
                activity::log(name, ActivityEvent::AgentRemoved, "Agent removed", agent);
            }

            // ── Diff and log skill changes ───────────────────────────────
            for skill in incoming
                .skills
                .iter()
                .filter(|s| !existing.skills.contains(s))
            {
                activity::log(name, ActivityEvent::SkillAdded, "Skill added", skill);
            }
            for skill in existing
                .skills
                .iter()
                .filter(|s| !incoming.skills.contains(s))
            {
                activity::log(name, ActivityEvent::SkillRemoved, "Skill removed", skill);
            }

            // ── Diff and log MCP server changes ──────────────────────────
            // Case-insensitive so a case-only change (`Sentry` ⇄ `sentry`),
            // which enrich_project collapses on save, is not mis-logged as a
            // remove + add of the "same" server.
            for server in incoming
                .mcp_servers
                .iter()
                .filter(|s| !core::contains_ignore_ascii_case(&existing.mcp_servers, s.as_str()))
            {
                activity::log(
                    name,
                    ActivityEvent::McpServerAdded,
                    "MCP server added",
                    server,
                );
            }
            for server in existing
                .mcp_servers
                .iter()
                .filter(|s| !core::contains_ignore_ascii_case(&incoming.mcp_servers, s.as_str()))
            {
                activity::log(
                    name,
                    ActivityEvent::McpServerRemoved,
                    "MCP server removed",
                    server,
                );
            }

            let new_agent_ids: Vec<String> = incoming
                .agents
                .iter()
                .filter(|a| !existing.agents.contains(a))
                .cloned()
                .collect();

            if !new_agent_ids.is_empty() {
                let dir = std::path::PathBuf::from(&incoming.directory);
                let discovered = sync::discover_new_agent_mcp_configs(&dir, &new_agent_ids);

                for (server_name, config_str) in discovered {
                    // Add the server name to the project's selection list.
                    // Case-insensitive: the registry treats `Sentry`/`sentry` as
                    // one server, so a variant must not become a duplicate.
                    if !core::contains_ignore_ascii_case(&enriched.mcp_servers, &server_name) {
                        enriched.mcp_servers.push(server_name.clone());
                    }
                    // Add the config to the global registry so that
                    // sync_project_without_autodetect can include it when
                    // building the mcpServers map written to disk.  A name
                    // the registry already holds keeps its saved config.
                    let _ = core::add_discovered_mcp_server_config(&server_name, &config_str);
                }
            }

            // ── Enrich with plugin-provided skills/rules when a plugin
            //    tool is newly added to the project ───────────────────────
            let new_tools: Vec<String> = enriched
                .tools
                .iter()
                .filter(|t| !existing.tools.contains(t))
                .cloned()
                .collect();
            if !new_tools.is_empty() {
                core::enrich_project_with_plugin_resources(&mut enriched, &new_tools);
            }

            // ── Remove plugin-provided skills/rules when a plugin
            //    tool is removed from the project ─────────────────────────
            let removed_tools: Vec<String> = existing
                .tools
                .iter()
                .filter(|t| !enriched.tools.contains(t))
                .cloned()
                .collect();
            if !removed_tools.is_empty() {
                core::strip_plugin_resources(&mut enriched, &removed_tools);
            }
        }

        let enriched_data = serde_json::to_string_pretty(&enriched).map_err(|e| e.to_string())?;
        core::save_project(name, &enriched_data)?;

        if !enriched.agents.is_empty() {
            let mut written = match &existing_project {
                Some(existing) => sync::remove_dropped_agent_files(
                    existing,
                    &enriched,
                    &effective_project_dir(&enriched)?,
                )?,
                None => Vec::new(),
            };
            written.extend(sync::sync_project_without_autodetect(&mut enriched)?);
            if !written.is_empty() {
                let detail = format!(
                    "{} file{}",
                    written.len(),
                    if written.len() == 1 { "" } else { "s" }
                );
                activity::log(
                    name,
                    ActivityEvent::ProjectSynced,
                    "Synced agent configs",
                    &detail,
                );
            }
        }
    }

    // ── Fire-and-forget AI recommendations ───────────────────────────────────
    // Spawn on the Tauri async runtime so we never block the UI.
    // The throttle check inside ai_generate_project_recommendations_bg ensures
    // this runs at most once per 24 hours automatically; on a brand-new project
    // (is_new == true) we always run it regardless of the throttle, unless
    // the caller asked to skip it (bulk add).
    if !(is_new && recommendations == NewProjectRecommendations::Skip) {
        let project_name = name.to_string();
        let is_new_project = is_new;
        tauri::async_runtime::spawn(async move {
            if let Err(e) = run_ai_recommendations_bg(&project_name, is_new_project).await {
                eprintln!(
                    "[automatic] AI recommendations skipped for '{}': {}",
                    project_name, e
                );
            }
        });
    }

    Ok(name.clone())
}

/// Background helper: run AI recommendations for a project, respecting the
/// once-per-day throttle unless `force` is true (used for new projects).
async fn run_ai_recommendations_bg(project: &str, force: bool) -> Result<(), String> {
    // Honour the throttle for existing projects; always run for new ones.
    // The throttle timestamp is stored under the checkout's `local_key`.
    // This comes first because a project save reaches here every time, and a
    // throttled save must not touch the credential check.
    let store_key = crate::core::project_store_local_key(project)?;
    if !force && crate::recommendations::ai_recommendations_throttled(&store_key)? {
        return Ok(());
    }

    // Verify a credential exists before attempting the (potentially slow) AI call.
    crate::core::ai::ensure_agent_credential(crate::core::store_reads::StoreRead::Background)?;

    // Delegate to the full command implementation (re-uses all the same logic).
    super::recommendations::ai_generate_project_recommendations(project, Some(force)).await?;

    Ok(())
}

/// Classify a candidate project directory before the Add Project wizard
/// commits. Returns a `DirectoryStatus` variant tagged with `kind`
/// (`"Available"`, `"RegisteredHere"`, or `"OrphanConfig"`) so the frontend
/// can proceed straight to create, redirect to an existing project, or
/// prompt the user to import or discard an orphan on-disk config.
#[tauri::command]
pub fn inspect_project_directory(directory: &str) -> Result<core::DirectoryStatus, String> {
    core::inspect_project_directory(directory)
}

/// Turn read-only mode on or off for a project on this machine. Edits only
/// the registry pointer, never the project folder, so it works while the
/// project is read-only.
///
/// Turning it off catches up on what was skipped while it was on: the
/// folder's files are refreshed (keys and enrichment), and group profiles
/// and plugin-owned resources are reconciled. Renames and removals of
/// library items made in the meantime are not replayed; the project keeps
/// the names its committed config uses. Agent files are not synced here, so
/// any drift shows and the user syncs when ready.
#[tauri::command(async)]
pub fn set_project_read_only(name: &str, read_only: bool) -> Result<(), String> {
    // A sweep holds this while it works on a project, so the toggle lands
    // between projects, never during one.
    let writes = hold_library_writes();
    core::set_project_read_only(name, read_only)?;
    if read_only {
        return Ok(());
    }
    let ident = core::canonical_project_ident(name)?;
    core::refresh_project_files(&ident).map_err(|e| {
        format!(
            "Read-only mode is off, but the project's files could not be updated: {}",
            e
        )
    })?;
    drop(writes);
    reconcile_group_profiles_for_projects(
        std::slice::from_ref(&ident),
        &SweepProgress::silent(),
    );
    reconcile_plugin_resources_in_projects();
    Ok(())
}

/// Report what the Automatic config in `directory` would use from this
/// machine's library. Writes nothing, so it runs before the user chooses how
/// to add the project.
#[tauri::command(async)]
pub fn preview_project_library_use(directory: &str) -> Result<core::ProjectLibraryUse, String> {
    core::preview_project_library_use(directory)
}

/// Adopt an existing on-disk project config (`.automatic.json`, or a legacy
/// `.automatic/project.json`) as a registered project. Returns the adopted
/// project's `{name, local_key}`: open it by the key, since another project
/// may share the name.
#[tauri::command(async)]
pub fn import_existing_project(
    directory: &str,
    read_only: Option<bool>,
) -> Result<core::ImportedProject, String> {
    let _writes = hold_library_writes();
    let imported = core::import_existing_project(directory, read_only)?;
    // Write the new checkout's keys into the folder now, as a save would.
    // The import itself only writes the registry pointer. The refresh skips
    // a project added in read-only mode.
    if let Err(e) = core::refresh_project_files(&imported.local_key) {
        eprintln!(
            "[automatic] could not write keys into imported project '{}': {}",
            imported.name, e
        );
    }
    activity::log(
        &imported.local_key,
        ActivityEvent::ProjectCreated,
        "Project imported",
        &imported.name,
    );
    Ok(imported)
}

/// Delete the project config and state files in `<directory>` so the wizard
/// can start fresh in a directory that previously held an orphan config.
#[tauri::command]
pub fn delete_project_config(directory: &str) -> Result<(), String> {
    core::delete_project_config(directory)
}

#[tauri::command(async)]
pub fn rename_project(old_name: &str, new_name: &str) -> Result<(), String> {
    let _writes = hold_library_writes();
    // Address the project by its unique identifier throughout: after the
    // rename, the new name may be shared with another project.
    let ident = &crate::core::canonical_project_ident(old_name)?;
    let old_display_name = crate::core::canonical_project_name(ident)?;
    core::rename_project(ident, new_name)?;
    // A project without a key is addressed by its name, which just changed.
    let ident = &if crate::core::is_project_key(ident) {
        ident.clone()
    } else {
        new_name.to_string()
    };

    // Memory, features, groups, activity, recommendations and dev servers
    // are keyed by `id` or `local_key`, which a rename does not change, so
    // nothing moves. Dev servers still keyed by the old name (not migrated
    // yet) are moved under the key, or the new name for a project without
    // one, so a running server keeps its stop control (VEL-160).
    // Best-effort — the rename has already succeeded.
    let store_key = crate::core::project_store_local_key(ident)?;
    if let Err(e) = crate::plugins::dev_servers::adopt_legacy_project(&old_display_name, &store_key) {
        eprintln!(
            "rename_project: could not move dev-server config '{}' -> '{}': {}",
            old_display_name, store_key, e
        );
    }

    // Agent configs name the project by `id` in AUTOMATIC_PROJECT, and a
    // rename does not change it. A project with no id yet has its name
    // there, so only that case re-syncs.
    let raw = core::read_project(ident)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    // A read-only project is renamed on this machine only; nothing is synced.
    if project.id.is_empty()
        && !project.directory.is_empty()
        && !project.agents.is_empty()
        && !project.is_read_only_here()
    {
        let written = sync::sync_project(&project);
        if let Ok(ref files) = written {
            if !files.is_empty() {
                let detail = format!(
                    "{} file{}",
                    files.len(),
                    if files.len() == 1 { "" } else { "s" }
                );
                activity::log(
                    ident,
                    ActivityEvent::ProjectSynced,
                    "Synced agent configs",
                    &detail,
                );
            }
        }
        written?;
    }
    activity::log(
        ident,
        ActivityEvent::ProjectUpdated,
        "Project renamed",
        &format!("{} → {}", old_display_name, new_name),
    );

    Ok(())
}

#[tauri::command(async)]
pub fn delete_project(name: &str) -> Result<(), String> {
    let _writes = hold_library_writes();
    // Deleting an unknown project still clears what it left behind, as before.
    let display_name = core::resolve_project_name(name)?;
    let name = &core::resolve_project_ident(name)?.unwrap_or_else(|| name.to_string());
    // Resolve the store keys before the registry entry goes.
    let keys = core::project_store_keys(name)?;
    core::delete_project(name)?;

    // Memory, features, activity and recommendations are kept (user
    // decision, stage 3b step 2). Drop the dev-servers plugin's per-checkout
    // registry file so no orphan is left in the global Tools > Servers view
    // (VEL-160), and a legacy file still named by the project's name.
    // Best-effort — the project delete has already succeeded.
    // The legacy file is left alone while another project still has the
    // name: it may be that project's.
    let mut dev_server_keys = vec![keys.local_key.clone()];
    if keys.local_key != *name {
        dev_server_keys.push(name.clone());
    }
    if let Some(display_name) = display_name {
        let name_still_used = !matches!(core::resolve_project_name(&display_name), Ok(None));
        if !name_still_used && !dev_server_keys.contains(&display_name) {
            dev_server_keys.push(display_name);
        }
    }
    for key in dev_server_keys {
        if let Err(e) = crate::plugins::dev_servers::registry::remove_project(&key) {
            eprintln!(
                "delete_project: could not remove dev-server config '{}' for '{}': {}",
                key, name, e
            );
        }
    }

    Ok(())
}

// ── Project Documentation ─────────────────────────────────────────────────────

/// Return the parsed `.automatic/docs.json` for the given project as JSON.
/// Returns an empty object when the file does not exist yet.
#[tauri::command]
pub fn get_project_docs(name: &str) -> Result<String, String> {
    let name = &crate::core::canonical_project_ident(name)?;
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let docs = context::get_project_docs(&project.directory)?;
    serde_json::to_string(&docs).map_err(|e| e.to_string())
}

/// Return the raw text content of `.automatic/docs.json` for editing.
/// Returns an empty string when the file does not exist yet.
#[tauri::command]
pub fn read_project_docs_raw(name: &str) -> Result<String, String> {
    let name = &crate::core::canonical_project_ident(name)?;
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    if project.directory.is_empty() {
        return Err("Project has no directory configured".into());
    }

    let path = std::path::PathBuf::from(&project.directory)
        .join(".automatic")
        .join(context::DOCS_FILE_NAME);
    if !path.exists() {
        return Ok(String::new());
    }
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

/// Write raw JSON text to `.automatic/docs.json`, creating the `.automatic`
/// directory if it does not exist.
#[tauri::command]
pub fn save_project_docs_raw(name: &str, content: &str) -> Result<(), String> {
    ensure_writable(name)?;
    let name = &crate::core::canonical_project_ident(name)?;
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let docs: HashMap<String, context::DocEntry> =
        serde_json::from_str(content).map_err(|e| format!("Invalid docs JSON: {}", e))?;

    context::save_project_docs(&project.directory, &docs)
}

// ── Project Sync ─────────────────────────────────────────────────────────────

#[tauri::command(async)]
pub fn sync_project(name: &str) -> Result<String, String> {
    let _writes = hold_library_writes();
    ensure_writable(name)?;
    let name = &crate::core::canonical_project_ident(name)?;
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let written = sync::sync_project(&project)?;
    if !written.is_empty() {
        let detail = format!(
            "{} file{}",
            written.len(),
            if written.len() == 1 { "" } else { "s" }
        );
        activity::log(
            name,
            ActivityEvent::ProjectSynced,
            "Synced agent configs",
            &detail,
        );
    }
    serde_json::to_string_pretty(&written).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn rebuild_project(name: &str) -> Result<String, String> {
    let _writes = hold_library_writes();
    ensure_writable(name)?;
    let name = &crate::core::canonical_project_ident(name)?;
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    let mut rebuilt = sync::rebuild_project_state(&project)?;
    let rebuilt_json = serde_json::to_string_pretty(&rebuilt).map_err(|e| e.to_string())?;
    core::save_project(name, &rebuilt_json)?;

    sync::rebuild_instruction_snapshots(&rebuilt);
    core::record_instruction_hashes(&mut rebuilt);

    activity::log(
        name,
        ActivityEvent::ProjectUpdated,
        "Rebuilt project state",
        "Reloaded Automatic state from the current project files",
    );

    serde_json::to_string_pretty(&rebuilt).map_err(|e| e.to_string())
}

fn diff_category(
    key: &'static str,
    label: &'static str,
    automatic: &[String],
    disk: &[String],
) -> RebuildPreviewCategory {
    let automatic = sorted_unique(automatic);
    let disk = sorted_unique(disk);
    let added = disk
        .iter()
        .filter(|item| !automatic.contains(*item))
        .cloned()
        .collect();
    let removed = automatic
        .iter()
        .filter(|item| !disk.contains(*item))
        .cloned()
        .collect();

    RebuildPreviewCategory {
        key,
        label,
        automatic,
        disk,
        added,
        removed,
    }
}

fn sorted_unique(items: &[String]) -> Vec<String> {
    let mut values = items.to_vec();
    values.sort();
    values.dedup();
    values
}

fn instruction_files_for(project: &core::Project) -> Vec<String> {
    let mut files = Vec::new();
    for agent_id in &project.agents {
        if let Some(agent) = crate::agent::from_id(agent_id) {
            let filename = agent.project_file_name().to_string();
            if !files.contains(&filename) {
                files.push(filename);
            }
        }
    }
    files
}

fn custom_agent_names(project: &core::Project) -> Vec<String> {
    project
        .custom_agents
        .as_ref()
        .map(|agents| agents.iter().map(|agent| agent.name.clone()).collect())
        .unwrap_or_default()
}

fn custom_command_names(project: &core::Project) -> Vec<String> {
    project
        .custom_commands
        .as_ref()
        .map(|commands| commands.iter().map(|cmd| cmd.name.clone()).collect())
        .unwrap_or_default()
}

fn custom_skill_names(project: &core::Project) -> Vec<String> {
    project
        .custom_skills
        .as_ref()
        .map(|skills| skills.iter().map(|skill| skill.name.clone()).collect())
        .unwrap_or_default()
}

/// Preview what removing an agent in `mode` (`"remove"` or `"keep"`) does to
/// the project directory.  Read-only.  Returns a JSON array of
/// [`crate::agent::RemovalEntry`]; Keep's preview is always empty.
#[tauri::command]
pub fn get_agent_cleanup_preview(name: &str, agent_id: &str, mode: &str) -> Result<String, String> {
    let name = &crate::core::canonical_project_ident(name)?;
    let mode: crate::agent::RemovalMode = mode.parse()?;
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let preview = sync::get_agent_cleanup_preview(&project, agent_id, mode)?;
    serde_json::to_string(&preview).map_err(|e| e.to_string())
}

/// Remove an agent from a project.  `mode` is `"remove"` (delete the agent's
/// config from the project directory) or `"keep"` (stop syncing it and leave
/// every file in place).  Returns a JSON array of
/// [`crate::agent::RemovalEntry`] describing what changed.
#[tauri::command(async)]
pub fn remove_agent_from_project(name: &str, agent_id: &str, mode: &str) -> Result<String, String> {
    let _writes = hold_library_writes();
    ensure_writable(name)?;
    let name = &crate::core::canonical_project_ident(name)?;
    let mode: crate::agent::RemovalMode = mode.parse()?;
    let raw = core::read_project(name)?;
    let mut project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let removed = sync::remove_agent_from_project(&mut project, agent_id, mode)?;
    activity::log(name, ActivityEvent::AgentRemoved, "Agent removed", agent_id);
    serde_json::to_string(&removed).map_err(|e| e.to_string())
}

/// Check whether the on-disk agent configs have drifted from what Automatic would
/// generate.  Returns a JSON-serialised [`sync::DriftReport`] describing which
/// agents and files are out of sync.  This is a read-only operation.
#[tauri::command]
pub fn check_project_drift(name: &str) -> Result<String, String> {
    let name = &crate::core::canonical_project_ident(name)?;
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let report = sync::check_project_drift(&project)?;
    serde_json::to_string(&report).map_err(|e| e.to_string())
}

/// Check for known configuration problems in a project.
///
/// Currently detects MCP server name collisions between the project-local
/// `.mcp.json` and the Claude Code user-scoped config (`~/.claude.json`).
/// Returns a JSON-serialised [`sync::ProjectProblemsReport`].
/// This is a read-only operation.
#[tauri::command]
pub fn check_project_problems(name: &str) -> Result<String, String> {
    let name = &crate::core::canonical_project_ident(name)?;
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let report = sync::check_project_problems(&project)?;
    serde_json::to_string(&report).map_err(|e| e.to_string())
}

/// Adopt a stale skill by adding it to the project's skill list and re-syncing.
///
/// `skill_name` is the bare skill name (e.g. `"my-skill"`).  The skill must
/// already exist in the managed library (`~/.automatic/library/skills/`) for
/// the sync to succeed — if it only exists locally in the project directory,
/// it is imported as a local skill instead.
///
/// Call this when the user chooses "Add to project" in the drift resolution UI
/// for a stale skill directory.
#[tauri::command(async)]
pub fn adopt_stale_skill(name: &str, skill_name: &str) -> Result<(), String> {
    let _writes = hold_library_writes();
    ensure_writable(name)?;
    let name = &crate::core::canonical_project_ident(name)?;
    let raw = core::read_project(name)?;
    let mut project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    // Only add if not already present.
    if !project.skills.contains(&skill_name.to_string()) {
        project.skills.push(skill_name.to_string());
    }

    project.updated_at = chrono::Utc::now().to_rfc3339();
    let data = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;
    core::save_project(name, &data)?;

    activity::log(
        name,
        ActivityEvent::SkillAdded,
        "Skill adopted from disk",
        skill_name,
    );

    // Re-sync so the skill is properly linked for all agents.
    if !project.directory.is_empty() && !project.agents.is_empty() {
        sync::sync_project_without_autodetect(&mut project)?;
    }

    Ok(())
}

/// Remove a stale skill directory from disk without changing the project config.
///
/// `skill_name` is the bare skill name (e.g. `"my-skill"`).  This deletes the
/// skill directory from every agent's skill location within the project directory
/// (e.g. `.agents/skills/<name>`, `.claude/skills/<name>`).
///
/// Call this when the user chooses "Remove from disk" in the drift resolution UI
/// for a stale skill directory.
#[tauri::command(async)]
pub fn remove_stale_skill(name: &str, skill_name: &str) -> Result<(), String> {
    let _writes = hold_library_writes();
    // The name is joined into a path that is then deleted recursively.
    core::ensure_valid_asset_name("Skill", skill_name)?;
    ensure_writable(name)?;
    let name = &crate::core::canonical_project_ident(name)?;
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    if project.directory.is_empty() {
        return Err("Project has no directory configured".into());
    }

    let dir = std::path::PathBuf::from(&project.directory);
    if !dir.exists() {
        return Err("Project directory does not exist".into());
    }

    let mut removed_any = false;

    for agent_id in &project.agents {
        if let Some(agent_instance) = crate::agent::from_id(agent_id) {
            for skill_dir in agent_instance.skill_dirs(&dir) {
                let target = skill_dir.join(skill_name);
                if target.is_dir() {
                    std::fs::remove_dir_all(&target)
                        .map_err(|e| format!("Failed to remove {}: {}", target.display(), e))?;
                    removed_any = true;
                }
            }
        }
    }

    // Also check the project hub (.agents/skills/)
    let hub_dir = dir.join(".agents").join("skills").join(skill_name);
    if hub_dir.is_dir() {
        std::fs::remove_dir_all(&hub_dir)
            .map_err(|e| format!("Failed to remove {}: {}", hub_dir.display(), e))?;
        removed_any = true;
    }

    if !removed_any {
        return Err(format!(
            "Skill directory '{}' was not found in any agent skill location",
            skill_name
        ));
    }

    activity::log(
        name,
        ActivityEvent::SkillRemoved,
        "Stale skill removed from disk",
        skill_name,
    );

    Ok(())
}

/// Resolve the effective project directory used for skill/command sync (Silent
/// mode writes under `.automatic/silent/`).
fn effective_project_dir(project: &core::Project) -> Result<std::path::PathBuf, String> {
    let dir = std::path::PathBuf::from(&project.directory);
    match project.mode {
        core::ProjectMode::Silent => {
            let silent = dir.join(".automatic").join("silent");
            // A committed `.automatic/silent` link would send adopt reads and
            // overwrite writes outside the project.
            match sync::escaped_silent_dir(&dir, &silent) {
                Some(reason) => Err(format!("Refused to use the Silent-mode directory: {reason}")),
                None => Ok(silent),
            }
        }
        core::ProjectMode::Normal => Ok(dir),
    }
}

/// Refuse an `asset_name` that cannot be used in a path for its `kind`.
/// Skills and commands use the name as a file name. A rule uses its slug.
/// An agent uses the machine name from its stored content, which is checked
/// where that name is worked out.
fn ensure_custom_asset_name(kind: &str, asset_name: &str) -> Result<(), String> {
    match kind {
        "skill" => core::ensure_valid_asset_name("Custom skill", asset_name),
        "command" => core::ensure_valid_asset_name("Custom command", asset_name),
        "rule" => core::ensure_valid_custom_rule_name(asset_name),
        _ => Ok(()),
    }
}

/// Read a custom asset's on-disk file for adoption.  Adopted content is
/// stored in the project's committed config, so a file that links out of
/// the project (or a FIFO) is refused rather than imported.
fn read_adoptable_file(
    project_dir: &std::path::Path,
    path: &std::path::Path,
) -> Result<String, String> {
    sync::read_project_text(project_dir, path)?
        .ok_or_else(|| format!("Failed to read {}: it does not exist", path.display()))
}

fn strip_instructions_header(content: &str) -> String {
    content
        .strip_prefix("<!-- managed by Automatic — do not edit by hand -->\n\n")
        .unwrap_or(content)
        .trim_end()
        .to_string()
}

/// What adopting a custom agent changes: its new stored Markdown, and the
/// other agents' files to rewrite once that is saved.
struct AdoptedCustomAgent {
    content: String,
    other_files: Vec<sync::OtherAgentFile>,
}

/// Work out what adopting the on-disk version of `ca` changes.  The file is
/// the one the conflict scan reports, so adopting answers the conflict the
/// user was shown.
///
/// What holds afterwards: the adopted file no longer conflicts, and any other
/// agent's file that matched the stored copy still matches once
/// `other_files` are rewritten.  Another agent's file that really differs
/// stays a conflict, and so the next scan can still report one.  What the
/// file cannot represent, including a change of name, is refused with both
/// copies untouched.
fn adoptable_custom_agent_content(
    project: &core::Project,
    effective: &std::path::Path,
    ca: &core::CustomAgent,
) -> Result<AdoptedCustomAgent, String> {
    let (agent, conflict) = match sync::scan_custom_agent(project, effective, ca) {
        Some(sync::CustomAgentFinding::Conflict { agent, conflict }) => (agent, conflict),
        Some(sync::CustomAgentFinding::Unreadable(unreadable)) => {
            return Err(format!(
                "Cannot adopt agent '{}' from {}: {}",
                ca.name, unreadable.path, unreadable.reason
            ));
        }
        None => {
            return Err(format!(
                "No on-disk version of agent '{}' differs from Automatic's copy",
                ca.name
            ));
        }
    };

    let shared = project.agents.iter().any(|id| {
        id != agent.id()
            && crate::agent::from_id(id).is_some_and(|other| other.agents_dir(effective).is_some())
    });
    let content = sync::adopted_agent_markdown(agent, ca, &conflict.disk_content, shared).map_err(
        |refusal| {
            format!(
                "Cannot adopt agent '{}' from {}: {}",
                ca.name,
                conflict.path,
                refusal.message(agent.label())
            )
        },
    )?;

    let other_files = sync::other_agent_files_matching(project, effective, ca, agent);
    let blocked = sync::unwritable_other_agent_files(effective, &other_files);
    if !blocked.is_empty() {
        return Err(format!(
            "Cannot adopt agent '{}' from {}: these files cannot be updated: {}. Nothing was changed.",
            ca.name,
            conflict.path,
            blocked.join("; ")
        ));
    }

    Ok(AdoptedCustomAgent {
        content,
        other_files,
    })
}

/// The activity line for an adopt, which says when other files were missed.
fn adopt_activity_summary(kind: &str, rewrites: &Result<(), Vec<String>>) -> String {
    match rewrites {
        Ok(()) => format!("Custom {kind} adopted from disk"),
        Err(failed) => format!(
            "Custom {kind} adopted from disk, but not written to: {}",
            failed.join("; ")
        ),
    }
}

/// Adopt on-disk custom asset content into the project's stored snapshot.
///
/// `kind` is one of `"skill"`, `"rule"`, `"agent"`, `"command"`.
/// Favours the on-disk file over Automatic's stored copy.
#[tauri::command(async)]
pub fn adopt_custom_asset(name: &str, kind: &str, asset_name: &str) -> Result<(), String> {
    let _writes = hold_library_writes();
    ensure_custom_asset_name(kind, asset_name)?;
    ensure_writable(name)?;
    let name = &crate::core::canonical_project_ident(name)?;
    let raw = core::read_project(name)?;
    let mut project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    if project.directory.is_empty() {
        return Err("Project has no directory configured".into());
    }

    let effective = effective_project_dir(&project)?;
    let project_dir = std::path::PathBuf::from(&project.directory);
    let mut other_agent_rewrites = None;

    match kind {
        "skill" => {
            let skill_md = effective
                .join(".agents")
                .join("skills")
                .join(asset_name)
                .join("SKILL.md");
            let disk_content = read_adoptable_file(&project_dir, &skill_md)?;
            let custom = project.custom_skills.get_or_insert_with(Vec::new);
            let Some(entry) = custom.iter_mut().find(|cs| cs.name == asset_name) else {
                return Err(format!(
                    "Skill '{}' is not a project-scoped custom skill",
                    asset_name
                ));
            };
            entry.content = disk_content;
        }
        "rule" => {
            let path = std::path::Path::new(&project.directory)
                .join(".automatic")
                .join("instructions")
                .join(format!("custom-{}.md", core::custom_rule_slug(asset_name)));
            let raw_disk = read_adoptable_file(&project_dir, &path)?;
            let disk_body = strip_instructions_header(&raw_disk);
            let Some(entry) = project
                .custom_rules
                .iter_mut()
                .find(|r| r.name == asset_name)
            else {
                return Err(format!(
                    "Rule '{}' is not a project-scoped custom rule",
                    asset_name
                ));
            };
            entry.content = disk_body;
        }
        "agent" => {
            let custom = project.custom_agents.as_deref().unwrap_or(&[]);
            let Some(ca) = custom.iter().find(|a| a.name == asset_name).cloned() else {
                return Err(format!(
                    "Agent '{}' is not a project-scoped custom agent",
                    asset_name
                ));
            };
            let adopted = adoptable_custom_agent_content(&project, &effective, &ca)?;
            let updated = core::CustomAgent {
                content: adopted.content,
                ..ca
            };
            if let Some(entry) = project
                .custom_agents
                .as_mut()
                .and_then(|agents| agents.iter_mut().find(|a| a.name == asset_name))
            {
                *entry = updated.clone();
            }
            other_agent_rewrites = Some((adopted.other_files, updated));
        }
        "command" => {
            let path = effective
                .join(".agents")
                .join("commands")
                .join(format!("{}.md", asset_name));
            let disk_content = read_adoptable_file(&project_dir, &path)?;
            let custom = project.custom_commands.get_or_insert_with(Vec::new);
            let Some(entry) = custom.iter_mut().find(|c| c.name == asset_name) else {
                return Err(format!(
                    "Command '{}' is not a project-scoped custom command",
                    asset_name
                ));
            };
            entry.content = disk_content;
        }
        other => return Err(format!("Unknown custom asset kind '{}'", other)),
    }

    project.updated_at = chrono::Utc::now().to_rfc3339();
    let data = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;
    core::save_project(name, &data)?;

    let rewrites = match other_agent_rewrites {
        Some((files, adopted)) => sync::rewrite_other_agent_files(&effective, &files, &adopted),
        None => Ok(()),
    };
    activity::log(
        name,
        ActivityEvent::ProjectUpdated,
        &adopt_activity_summary(kind, &rewrites),
        asset_name,
    );

    rewrites.map_err(|failed| {
        format!(
            "Agent '{}' was adopted and saved, but these files could not be updated: {}. Fix them, then use Overwrite to bring them in step with Automatic's copy. Using the on-disk version of one would undo the adopt.",
            asset_name,
            failed.join("; ")
        )
    })
}

/// Overwrite the on-disk custom asset with Automatic's stored snapshot, then
/// re-sync so agent copies/symlinks match.
#[tauri::command(async)]
pub fn overwrite_custom_asset(name: &str, kind: &str, asset_name: &str) -> Result<(), String> {
    let _writes = hold_library_writes();
    ensure_custom_asset_name(kind, asset_name)?;
    ensure_writable(name)?;
    let name = &crate::core::canonical_project_ident(name)?;
    let raw = core::read_project(name)?;
    let mut project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    if project.directory.is_empty() {
        return Err("Project has no directory configured".into());
    }

    let effective = effective_project_dir(&project)?;
    let project_dir = std::path::PathBuf::from(&project.directory);

    match kind {
        "skill" => {
            let stored = project
                .custom_skills
                .as_ref()
                .and_then(|list| list.iter().find(|cs| cs.name == asset_name))
                .ok_or_else(|| {
                    format!(
                        "Skill '{}' is not a project-scoped custom skill",
                        asset_name
                    )
                })?
                .content
                .clone();
            let hub_dir = effective
                .join(".agents")
                .join("skills")
                .join(asset_name);
            sync::write_project_file(&effective, &hub_dir.join("SKILL.md"), &stored)?;
        }
        "rule" => {
            let stored = project
                .custom_rules
                .iter()
                .find(|r| r.name == asset_name)
                .ok_or_else(|| {
                    format!(
                        "Rule '{}' is not a project-scoped custom rule",
                        asset_name
                    )
                })?
                .content
                .clone();
            let instructions_dir = std::path::Path::new(&project.directory)
                .join(".automatic")
                .join("instructions");
            let path = instructions_dir
                .join(format!("custom-{}.md", core::custom_rule_slug(asset_name)));
            let file_content = format!(
                "<!-- managed by Automatic — do not edit by hand -->\n\n{}\n",
                stored.trim_end()
            );
            sync::write_project_file(&project_dir, &path, file_content)?;
        }
        "agent" => {
            let custom = project.custom_agents.clone().unwrap_or_default();
            let Some(ca) = custom.iter().find(|a| a.name == asset_name) else {
                return Err(format!(
                    "Agent '{}' is not a project-scoped custom agent",
                    asset_name
                ));
            };
            for agent_id in &project.agents {
                let Some(agent_instance) = crate::agent::from_id(agent_id) else {
                    continue;
                };
                let Some(agents_dir) = agent_instance.agents_dir(&effective) else {
                    continue;
                };
                let _ = sync::sync_custom_agents_force(
                    &effective,
                    &agents_dir,
                    std::slice::from_ref(ca),
                    agent_instance,
                )?;
            }
        }
        "command" => {
            let stored = project
                .custom_commands
                .as_ref()
                .and_then(|list| list.iter().find(|c| c.name == asset_name))
                .ok_or_else(|| {
                    format!(
                        "Command '{}' is not a project-scoped custom command",
                        asset_name
                    )
                })?
                .content
                .clone();
            let hub_dir = effective.join(".agents").join("commands");
            let rendered = crate::agent::render_markdown_command(&stored);
            sync::write_project_file(
                &effective,
                &hub_dir.join(format!("{}.md", asset_name)),
                rendered,
            )?;
        }
        other => return Err(format!("Unknown custom asset kind '{}'", other)),
    }

    // Re-sync so agent locations pick up the restored content. Disk now matches
    // stored for this asset, so the conflict filter will not skip it.
    if !project.agents.is_empty() {
        sync::sync_project_without_autodetect(&mut project)?;
    }

    activity::log(
        name,
        ActivityEvent::ProjectUpdated,
        &format!("Custom {} overwritten from Automatic", kind),
        asset_name,
    );

    Ok(())
}

// ── Cross-cutting helpers ────────────────────────────────────────────────────
//
// These are used by skills, rules, mcp_servers, and skill_store modules when
// a registry item is saved or deleted and projects referencing it need updating.

/// Call `f` with each registered project and its unique identifier (its
/// `local_key`, see `core::project_ident`). Save back with that identifier,
/// never the name, which another project may share. Show `project.name`.
pub(crate) fn with_each_project_mut<F>(mut f: F)
where
    F: FnMut(&str, &mut core::Project),
{
    let project_names = match core::list_project_idents() {
        Ok(names) => names,
        Err(e) => {
            eprintln!("Failed to list projects for config updates: {}", e);
            return;
        }
    };

    for project_name in project_names {
        let raw = match core::read_project(&project_name) {
            Ok(raw) => raw,
            Err(e) => {
                eprintln!("Failed to read project '{}': {}", project_name, e);
                continue;
            }
        };

        let mut project: core::Project = match serde_json::from_str(&raw) {
            Ok(project) => project,
            Err(e) => {
                eprintln!("Failed to parse project '{}': {}", project_name, e);
                continue;
            }
        };

        f(&project_name, &mut project);
    }
}

/// Refuse a command that writes into a project's folder when the project
/// is read-only on this machine, or its folder sits inside a read-only
/// project. `name` is the project's identifier.
pub(crate) fn ensure_writable(name: &str) -> Result<(), String> {
    if core::is_project_read_only(name)? {
        let shown = core::resolve_project_name(name)?.unwrap_or_else(|| name.to_string());
        return Err(core::read_only_error(&shown));
    }
    if let Some(directory) = core::project_directory(name)? {
        core::ensure_directory_writable(&directory)?;
    }
    Ok(())
}

/// [`with_each_project_mut`] for updates: skips projects that are read-only
/// on this machine, since nothing may be written into their folders. Use
/// [`with_each_project_mut`] to look at every project, read-only or not.
pub(crate) fn with_each_writable_project_mut<F>(mut f: F)
where
    F: FnMut(&str, &mut core::Project),
{
    let progress = SweepProgress::silent();
    sweep_projects(
        all_project_idents(&progress),
        &progress,
        Unreadable::Log,
        |_, _| true,
        |project_name, project| {
            f(project_name, project);
            Ok(())
        },
    );
}

fn all_project_idents(progress: &SweepProgress) -> Vec<String> {
    core::list_project_idents().unwrap_or_else(|e| {
        progress.report_failure("Project list", format!("Could not list projects: {}", e));
        Vec::new()
    })
}

/// Sweep errors reach the Task Log, so the detail stays in stderr: a parse
/// error means nothing there and can quote the file.
fn read_swept_project(ident: &str) -> Result<core::Project, String> {
    core::read_project(ident)
        .and_then(|raw| serde_json::from_str(&raw).map_err(|e| e.to_string()))
        .map_err(|e| {
            eprintln!("Failed to read project '{}': {}", ident, e);
            "The project's settings could not be read.".to_string()
        })
}

/// What a sweep does with a project it cannot read while choosing.
#[derive(Clone, Copy, PartialEq)]
enum Unreadable {
    /// Log it. It may have nothing to do with this change, and every save
    /// would otherwise report it.
    Log,
    /// Report it. A delete or rename would leave it naming the old item.
    Report,
}

/// Apply a change to each writable project in `idents` that `selects` picks.
/// Projects are chosen first, so `progress` knows the total, and then handled
/// one at a time. Each is read again under the project write lock, because
/// anything read earlier may be stale by the time the sweep reaches it. A
/// project that fails is recorded and the sweep carries on.
fn sweep_projects<S, A>(
    idents: Vec<String>,
    progress: &SweepProgress,
    unreadable: Unreadable,
    selects: S,
    mut apply: A,
) where
    S: Fn(&str, &core::Project) -> bool,
    A: FnMut(&str, &mut core::Project) -> Result<(), String>,
{
    let wanted = |ident: &str, project: &core::Project| {
        !project.is_read_only_here() && selects(ident, project)
    };
    let targets: Vec<String> = idents
        .into_iter()
        .filter(|ident| match read_swept_project(ident) {
            Ok(project) => wanted(ident, &project),
            Err(e) if unreadable == Unreadable::Report => {
                progress.report_failure(ident, e);
                false
            }
            Err(e) => {
                eprintln!("Skipping project '{}' during update: {}", ident, e);
                false
            }
        })
        .collect();
    progress.add_total(targets.len());

    for ident in targets {
        let mut shown = ident.clone();
        let writes = hold_library_writes();
        let outcome = read_swept_project(&ident).and_then(|mut project| {
            shown = project.name.clone();
            if wanted(&ident, &project) {
                apply(&ident, &mut project)
            } else {
                Ok(())
            }
        });
        drop(writes);
        progress.project_finished(&shown, outcome);
    }
}

/// Sweep `idents` with `mutate`, which reports whether it changed the
/// project. It runs on a copy to choose projects, so the total counts only
/// those that change. Each changed project is saved, then passed to `after`.
fn sweep_project_changes<M, A>(
    idents: Vec<String>,
    progress: &SweepProgress,
    unreadable: Unreadable,
    what: &str,
    mutate: M,
    mut after: A,
) where
    M: Fn(&str, &mut core::Project) -> bool,
    A: FnMut(&str, &mut core::Project) -> Result<(), String>,
{
    sweep_projects(
        idents,
        progress,
        unreadable,
        |ident, project| mutate(ident, &mut project.clone()),
        |ident, project| {
            if !mutate(ident, project) {
                return Ok(());
            }
            persist_swept_project(ident, project, what)?;
            after(ident, project)
        },
    );
}

/// Whether a project has somewhere to sync to.
fn is_sync_target(project: &core::Project) -> bool {
    !project.directory.is_empty() && !project.agents.is_empty() && !project.is_read_only_here()
}

/// Sync a project that has somewhere to sync to.
pub(crate) fn sync_configured_project(project: &mut core::Project) -> Result<(), String> {
    if !is_sync_target(project) {
        return Ok(());
    }
    sync::sync_project_without_autodetect(project).map(|_| ())
}

/// [`sync_configured_project`] for callers that report nothing.
pub(crate) fn sync_project_if_configured(project_name: &str, project: &mut core::Project) {
    if let Err(e) = sync_configured_project(project) {
        eprintln!(
            "Failed to sync project '{}' after registry update: {}",
            project_name, e
        );
    }
}

/// Sync every configured project for which `references` is true.
fn sync_projects_where<R>(progress: &SweepProgress, references: R)
where
    R: Fn(&core::Project) -> bool,
{
    sweep_projects(
        all_project_idents(progress),
        progress,
        Unreadable::Log,
        |_, project| is_sync_target(project) && references(project),
        |_, project| sync_configured_project(project),
    );
}

pub(crate) fn sync_projects_referencing_skill(skill_name: &str, progress: &SweepProgress) {
    sync_projects_referencing_skills(&[skill_name.to_string()], progress);
}

/// One sweep for several skills, so a project that uses two of them is
/// synced once.
pub(crate) fn sync_projects_referencing_skills(skill_names: &[String], progress: &SweepProgress) {
    sync_projects_where(progress, |project| {
        project.skills.iter().any(|skill| skill_names.contains(skill))
    });
}

pub(crate) fn sync_projects_referencing_mcp_server(server_name: &str, progress: &SweepProgress) {
    sync_projects_where(progress, |project| {
        project
            .mcp_servers
            .iter()
            .any(|server| server == server_name)
    });
}

pub(crate) fn sync_projects_referencing_rule(rule_name: &str, progress: &SweepProgress) {
    sync_projects_where(progress, |project| {
        project
            .file_rules
            .values()
            .any(|rules| rules.iter().any(|r| r == rule_name))
    });
}

pub(crate) fn sync_projects_referencing_subagent(machine_name: &str, progress: &SweepProgress) {
    sync_projects_where(progress, |project| {
        project.user_agents.iter().any(|name| name == machine_name)
    });
}

pub(crate) fn sync_projects_referencing_user_command(machine_name: &str, progress: &SweepProgress) {
    sync_projects_where(progress, |project| {
        project
            .user_commands
            .iter()
            .any(|name| name == machine_name)
    });
}

pub(crate) fn sync_projects_referencing_hook(machine_name: &str, progress: &SweepProgress) {
    sync_projects_where(progress, |project| {
        project.hooks.iter().any(|name| name == machine_name)
    });
}

/// Persist a project a sweep helper has mutated.
fn persist_swept_project(
    project_name: &str,
    project: &mut core::Project,
    what: &str,
) -> Result<(), String> {
    project.updated_at = chrono::Utc::now().to_rfc3339();
    let data = serde_json::to_string_pretty(project)
        .map_err(|e| format!("Could not serialise the project after {}: {}", what, e))?;
    core::save_project(project_name, &data)
        .map_err(|e| format!("Could not save the project after {}: {}", what, e))
}

/// Bring every project that references `profile_name` back in step with the
/// profile and re-sync the ones that changed. Called after a profile is
/// saved, and after a library asset it references is pruned or renamed.
pub(crate) fn reconcile_projects_referencing_profile(
    profile_name: &str,
    progress: &SweepProgress,
) {
    sweep_project_changes(
        all_project_idents(progress),
        progress,
        Unreadable::Log,
        "profile reconcile",
        |_, project| {
            let referenced = project.profiles.iter().any(|p| p == profile_name)
                || project.profile_contributions.contains_key(profile_name);
            referenced && core::reconcile_project_profiles(project).changed
        },
        |_, project| sync_configured_project(project),
    );
}

/// Bring the profiles of each named project in step with its groups, then
/// with the profiles themselves, and re-sync the ones that changed. Call
/// after a group is saved or deleted, with its members from before and
/// after the change (`core::group_member_idents`).
///
/// One unreadable project does not leave the rest out of step. A project
/// whose group membership cannot be read is reported and left as it is.
pub(crate) fn reconcile_group_profiles_for_projects(
    project_idents: &[String],
    progress: &SweepProgress,
) {
    let mut seen = std::collections::HashSet::new();
    let unique = project_idents
        .iter()
        .filter(|ident| seen.insert(ident.as_str()))
        .filter(|ident| match core::groups_listing(ident) {
            Ok(_) => true,
            Err(e) => {
                progress.report_failure(ident, e);
                false
            }
        })
        .cloned()
        .collect();
    sweep_project_changes(
        unique,
        progress,
        Unreadable::Log,
        "group profile reconcile",
        |ident, project| {
            // A group can go bad after the check above. Changing nothing is
            // safe; releasing on a partial list is not.
            let groups = match core::groups_listing(ident) {
                Ok(groups) => groups,
                Err(e) => {
                    eprintln!("Skipping the group reconcile for '{}': {}", ident, e);
                    return false;
                }
            };
            // Group profiles first: they decide which profiles are attached.
            let groups_changed = core::reconcile_group_profiles(project, &groups);
            let report = core::reconcile_project_profiles(project);
            groups_changed || report.changed
        },
        |_, project| sync_configured_project(project),
    );
}

/// Detach `profile_name` from every group and every project, dropping every
/// item the profile provides, then re-sync. Used before a profile is deleted.
/// Groups are cleared too, or a group would put the missing profile back.
pub(crate) fn detach_profile_from_projects(profile_name: &str, progress: &SweepProgress) {
    core::prune_profile_from_groups(profile_name);
    sweep_project_changes(
        all_project_idents(progress),
        progress,
        Unreadable::Report,
        "profile detach",
        |_, project| {
            let released = core::release_profile(project, profile_name);
            let group_recorded = core::strip_group_profile_contribution(project, profile_name);
            if !released && !group_recorded {
                return false;
            }
            core::reconcile_project_profiles(project);
            true
        },
        |_, project| sync_configured_project(project),
    );
}

/// Rewrite `profiles` entries and contribution keys from `old_name` to
/// `new_name` in every group and every project. Content is unchanged, so no
/// sync is needed.
pub(crate) fn rename_profile_in_projects(
    old_name: &str,
    new_name: &str,
    progress: &SweepProgress,
) {
    if old_name == new_name {
        return;
    }
    core::rename_profile_in_groups(old_name, new_name);
    sweep_project_changes(
        all_project_idents(progress),
        progress,
        Unreadable::Report,
        "profile rename",
        |_, project| core::rename_profile_in_project(project, old_name, new_name),
        |_, _| Ok(()),
    );
}

pub(crate) fn prune_hook_from_projects(machine_name: &str, progress: &SweepProgress) {
    sweep_project_changes(
        all_project_idents(progress),
        progress,
        Unreadable::Report,
        "hook removal",
        |_, project| {
            let before = project.hooks.len();
            project.hooks.retain(|name| name != machine_name);
            let stripped =
                core::strip_contribution(project, core::ProfileResourceKind::Hook, machine_name);
            project.hooks.len() != before || stripped
        },
        |_, project| sync_configured_project(project),
    );
}

/// Finish whatever a quit interrupted: bring every project in step with its
/// groups and profiles, re-sync them all and re-apply global MCP selections.
/// Safe to run again at any point. A project that matches already is written
/// with the same content.
///
/// A rename or delete cut short is not replayed. It can leave a project
/// naming an item the library no longer has, which sync skips without a
/// word, so those are reported for the user to fix.
pub(crate) fn repair_interrupted_sweep(progress: &SweepProgress) {
    let idents = all_project_idents(progress);
    reconcile_group_profiles_for_projects(&idents, progress);
    resync_all_projects(progress);
    for (agent, error) in sync::global_mcp::reapply_all_agents() {
        progress.report_failure(&agent, error);
    }
    with_each_project_mut(|_, project| {
        let missing = missing_library_references(project);
        if !project.is_read_only_here() && !missing.is_empty() {
            progress.report_failure(
                &project.name,
                format!("Uses items not in the library: {}", missing.join(", ")),
            );
        }
    });
}

/// What `project` names that the library cannot supply, as "kind name".
fn missing_library_references(project: &core::Project) -> Vec<String> {
    let rules: std::collections::BTreeSet<&String> = project.file_rules.values().flatten().collect();
    let mut missing: Vec<String> = Vec::new();
    let mut note = |kind: &str, name: &str, present: bool| {
        if !present {
            missing.push(format!("{kind} {name}"));
        }
    };
    for name in rules {
        note("rule", name, core::read_rule(name).is_ok());
    }
    for name in &project.skills {
        // A skill with no file reads as empty, not as an error.
        let present = core::read_skill(name).is_ok_and(|content| !content.is_empty());
        note("skill", name, present);
    }
    for name in &project.mcp_servers {
        let present = core::is_builtin_mcp_server(name) || core::read_mcp_server_config(name).is_ok();
        note("MCP server", name, present);
    }
    for name in &project.user_agents {
        note("sub-agent", name, core::read_subagent(name).is_ok());
    }
    for name in &project.user_commands {
        note("command", name, core::read_user_command(name).is_ok());
    }
    for name in &project.hooks {
        note("hook", name, core::read_hook(name).is_ok());
    }
    missing
}

/// Re-sync every project that has both a configured directory and at least
/// one agent. Used after bundled assets are reinstalled so any project that
/// references the updated library copies picks up the new content without
/// the user having to press "Sync now".
pub(crate) fn resync_all_projects(progress: &SweepProgress) {
    sync_projects_where(progress, |_| true);
}

/// Reconcile every project's plugin-owned rules and skills with its tools,
/// then save and re-sync the projects that changed. Run once on startup,
/// after the plugin registries are reconciled.
pub(crate) fn reconcile_plugin_resources_in_projects() {
    let plugins = match core::list_app_plugins() {
        Ok(plugins) => plugins,
        Err(e) => {
            eprintln!(
                "[automatic] skipped project plugin reconcile (plugin state unreadable): {}",
                e
            );
            return;
        }
    };

    with_each_writable_project_mut(|project_name, project| {
        if !core::reconcile_project_plugin_resources(project, &plugins) {
            return;
        }
        project.updated_at = chrono::Utc::now().to_rfc3339();
        let data = match serde_json::to_string_pretty(project) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Failed to serialize project '{}': {}", project_name, e);
                return;
            }
        };
        if let Err(e) = core::save_project(project_name, &data) {
            eprintln!("Failed to update project '{}': {}", project_name, e);
            return;
        }
        eprintln!(
            "[automatic] reconciled plugin rules and skills for project '{}'",
            project_name
        );
        sync_project_if_configured(project_name, project);
    });
}

pub(crate) fn prune_skill_from_projects(skill_name: &str, progress: &SweepProgress) {
    sweep_project_changes(
        all_project_idents(progress),
        progress,
        Unreadable::Report,
        "skill removal",
        |_, project| {
            let before = project.skills.len();
            project.skills.retain(|skill| skill != skill_name);
            let stripped =
                core::strip_contribution(project, core::ProfileResourceKind::Skill, skill_name);
            project.skills.len() != before || stripped
        },
        |_, project| sync_configured_project(project),
    );
}

pub(crate) fn prune_mcp_server_from_projects(server_name: &str, progress: &SweepProgress) {
    sweep_project_changes(
        all_project_idents(progress),
        progress,
        Unreadable::Report,
        "MCP server removal",
        |_, project| {
            let before = project.mcp_servers.len();
            project.mcp_servers.retain(|server| server != server_name);
            project
                .disabled_mcp_servers
                .retain(|server| server != server_name);
            let stripped = core::strip_contribution(
                project,
                core::ProfileResourceKind::McpServer,
                server_name,
            );
            project.mcp_servers.len() != before || stripped
        },
        |_, project| sync_configured_project(project),
    );
}

/// Rewrite every project's `mcp_servers` and `disabled_mcp_servers` entries
/// from `old_name` to `new_name`. Persists changed projects and re-syncs
/// them so on-disk agent config matches the renamed server.
pub(crate) fn rename_mcp_server_in_projects(
    old_name: &str,
    new_name: &str,
    progress: &SweepProgress,
) {
    if old_name == new_name {
        return;
    }
    sweep_project_changes(
        all_project_idents(progress),
        progress,
        Unreadable::Report,
        "MCP server rename",
        |_, project| {
            let mut changed = false;
            for server in project.mcp_servers.iter_mut() {
                if server == old_name {
                    *server = new_name.to_string();
                    changed = true;
                }
            }
            for server in project.disabled_mcp_servers.iter_mut() {
                if server == old_name {
                    *server = new_name.to_string();
                    changed = true;
                }
            }
            changed
                | core::rename_contribution(
                    project,
                    core::ProfileResourceKind::McpServer,
                    old_name,
                    new_name,
                )
        },
        |_, project| sync_configured_project(project),
    );
}

/// Run `mutate` over every template in the library and persist the ones it
/// reports as changed. Failures are logged and skipped, matching the pattern
/// used by the other propagation helpers. `action` names the caller in those
/// log lines.
fn update_each_template(action: &str, mutate: impl Fn(&mut core::ProjectTemplate) -> bool) {
    let _writes = hold_library_writes();
    let template_names = match core::list_templates() {
        Ok(names) => names,
        Err(e) => {
            eprintln!("Failed to list templates for {}: {}", action, e);
            return;
        }
    };

    for template_name in template_names {
        let raw = match core::read_template(&template_name) {
            Ok(raw) => raw,
            Err(e) => {
                eprintln!("Failed to read template '{}': {}", template_name, e);
                continue;
            }
        };

        let mut template: core::ProjectTemplate = match serde_json::from_str(&raw) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("Failed to parse template '{}': {}", template_name, e);
                continue;
            }
        };

        if !mutate(&mut template) {
            continue;
        }

        match serde_json::to_string_pretty(&template).map_err(|e| e.to_string()) {
            Ok(data) => {
                if let Err(e) = core::save_template(&template_name, &data) {
                    eprintln!(
                        "Failed to save template '{}' after {}: {}",
                        template_name, action, e
                    );
                }
            }
            Err(e) => {
                eprintln!(
                    "Failed to serialise template '{}' after {}: {}",
                    template_name, action, e
                );
            }
        }
    }
}

/// Rewrite every template's `mcp_servers` entries from `old_name` to
/// `new_name` and persist the ones that changed.
pub(crate) fn rename_mcp_server_in_templates(old_name: &str, new_name: &str) {
    if old_name == new_name {
        return;
    }
    update_each_template("MCP-server rename", |template| {
        let mut changed = false;
        for server in template.mcp_servers.iter_mut() {
            if server == old_name {
                *server = new_name.to_string();
                changed = true;
            }
        }
        changed
    });
}

/// Remove `rule_name` from every template's `unified_rules`. A template that
/// keeps the name of a deleted rule puts it back into each project the
/// template is applied to, where it shows as a rule that cannot be found.
pub(crate) fn prune_rule_from_templates(rule_name: &str) {
    update_each_template("rule delete", |template| {
        let before = template.unified_rules.len();
        template.unified_rules.retain(|r| r != rule_name);
        template.unified_rules.len() != before
    });
}

pub(crate) fn prune_rule_from_projects(rule_name: &str, progress: &SweepProgress) {
    sweep_project_changes(
        all_project_idents(progress),
        progress,
        Unreadable::Report,
        "rule removal",
        |_, project| {
            let mut changed = false;
            for rules in project.file_rules.values_mut() {
                let before = rules.len();
                rules.retain(|r| r != rule_name);
                if rules.len() != before {
                    changed = true;
                }
            }
            // Remove empty entries
            project.file_rules.retain(|_, rules| !rules.is_empty());
            changed
                | core::strip_contribution(project, core::ProfileResourceKind::Rule, rule_name)
        },
        |_, project| {
            // Re-inject rules for affected files, skipping any file whose rules
            // are managed via .claude/rules/ (inline injection must not happen there).
            for (filename, rules) in &project.file_rules {
                if core::project_uses_dot_claude_rules(project, filename) {
                    continue;
                }
                let _ = core::inject_rules_into_project_file(&project.directory, filename, rules);
            }
            sync_configured_project(project)
        },
    );
}

#[cfg(test)]
mod sweep_tests {
    use super::super::project_sweeps::tests::RecordingReporter;
    use super::super::project_sweeps::SweepReporter;
    use super::*;
    use std::sync::Arc;
    use tempfile::tempdir;

    /// A project's files live in its folder, so each needs its own.
    fn save_fixture(name: &str, home: &std::path::Path) {
        let directory = home.join(name);
        std::fs::create_dir_all(&directory).expect("project folder");
        let project = core::Project {
            name: name.to_string(),
            directory: directory.to_string_lossy().to_string(),
            agents: vec!["claude".to_string()],
            ..Default::default()
        };
        core::save_project(name, &serde_json::to_string_pretty(&project).expect("json"))
            .expect("save");
    }

    fn read_fixture(name: &str) -> core::Project {
        serde_json::from_str(&core::read_project(name).expect("read")).expect("parse")
    }

    fn with_home(test: impl FnOnce(&std::path::Path)) {
        let home = tempdir().expect("tempdir");
        core::with_test_home(home.path().to_path_buf(), || test(home.path()));
    }

    #[test]
    fn one_failing_project_is_named_and_the_others_still_run() {
        with_home(|home| {
            for name in ["sweep-a", "sweep-b", "sweep-c"] {
                save_fixture(name, home);
            }
            let recording = Arc::new(RecordingReporter::default());
            let reporter: Arc<dyn SweepReporter> = recording.clone();
            let progress = SweepProgress::reporting(reporter);

            let mut visited = Vec::new();
            sweep_projects(
                vec!["sweep-a".into(), "sweep-b".into(), "sweep-c".into()],
                &progress,
                Unreadable::Log,
                |_, _| true,
                |ident, _| {
                    visited.push(ident.to_string());
                    if ident == "sweep-b" {
                        Err("disk full".to_string())
                    } else {
                        Ok(())
                    }
                },
            );

            assert_eq!(visited, vec!["sweep-a", "sweep-b", "sweep-c"]);
            let events = recording.events.lock().expect("events");
            let last = events.last().expect("an event");
            assert_eq!((last.done, last.total), (3, 3));
            assert_eq!(last.failed.len(), 1);
            assert_eq!(last.failed[0].project, "sweep-b");
            assert_eq!(last.failed[0].error, "disk full");
        });
    }

    #[test]
    fn a_project_that_stops_matching_after_selection_is_left_alone() {
        with_home(|home| {
            save_fixture("sweep-gone", home);
            let progress = SweepProgress::silent();
            let chosen = std::cell::Cell::new(false);

            sweep_projects(
                vec!["sweep-gone".into()],
                &progress,
                Unreadable::Log,
                |_, _| !chosen.replace(true),
                |_, _| panic!("the project no longer matches"),
            );
        });
    }

    /// A save between selection and the sweep's turn: the worker must work
    /// from the saved project, not from what it read while choosing.
    #[test]
    fn a_project_saved_during_a_sweep_is_not_overwritten() {
        struct SaveWhenTotalIsKnown(String);
        impl SweepReporter for SaveWhenTotalIsKnown {
            fn report(&self, event: &super::super::project_sweeps::SweepEvent) {
                if event.done == 0 && !event.finished {
                    let mut project = read_fixture(&self.0);
                    project.description = "saved by the user".to_string();
                    let json = serde_json::to_string_pretty(&project).expect("json");
                    core::save_project(&self.0, &json).expect("save");
                }
            }
        }

        with_home(|home| {
            core::save_rule("sweep-rule", "Rule", "Body.\n").expect("rule");
            save_fixture("sweep-saved", home);
            let mut project = read_fixture("sweep-saved");
            project
                .file_rules
                .insert("_project".to_string(), vec!["sweep-rule".to_string()]);
            let json = serde_json::to_string_pretty(&project).expect("json");
            core::save_project("sweep-saved", &json).expect("save");

            let progress = SweepProgress::reporting(Arc::new(SaveWhenTotalIsKnown(
                "sweep-saved".to_string(),
            )));
            sync_projects_referencing_rule("sweep-rule", &progress);

            assert_eq!(read_fixture("sweep-saved").description, "saved by the user");
        });
    }

    #[test]
    fn the_total_counts_only_projects_that_will_change() {
        with_home(|home| {
            core::save_rule("sweep-rule", "Rule", "Body.\n").expect("rule");
            for (name, rules) in [("sweep-uses", vec!["sweep-rule"]), ("sweep-other", vec![])] {
                save_fixture(name, home);
                let mut project = read_fixture(name);
                if !rules.is_empty() {
                    project.file_rules.insert(
                        "_project".to_string(),
                        rules.iter().map(|r| r.to_string()).collect(),
                    );
                }
                let json = serde_json::to_string_pretty(&project).expect("json");
                core::save_project(name, &json).expect("save");
            }
            let recording = Arc::new(RecordingReporter::default());
            let reporter: Arc<dyn SweepReporter> = recording.clone();
            let progress = SweepProgress::reporting(reporter);

            prune_rule_from_projects("sweep-rule", &progress);

            let events = recording.events.lock().expect("events");
            assert_eq!(events.last().expect("an event").total, 1);
            assert!(read_fixture("sweep-uses").file_rules.is_empty());
        });
    }
}

#[cfg(test)]
mod sweep_race_tests {
    use super::super::project_sweeps::tests::{with_real_worker, RecordingReporter};
    use super::super::project_sweeps::SweepReporter;
    use super::propagation_tests::{make_project, with_temp_home};
    use super::*;
    use std::sync::{mpsc, Arc};
    use std::time::Duration;

    fn read_fixture(name: &str) -> core::Project {
        serde_json::from_str(&core::read_project(name).expect("read")).expect("parse")
    }

    fn save_fixture(project: &core::Project) {
        let json = serde_json::to_string_pretty(project).expect("json");
        core::save_project(&project.name, &json).expect("save");
    }

    fn recording_progress() -> (Arc<RecordingReporter>, SweepProgress) {
        let recording = Arc::new(RecordingReporter::default());
        let reporter: Arc<dyn SweepReporter> = recording.clone();
        (recording, SweepProgress::reporting(reporter))
    }

    fn save_profile(profile: &str, rule: &str) {
        core::save_rule(rule, "Profile Rule", "Profile rule body.\n").expect("save rule");
        let data = serde_json::to_string(&core::ProjectProfile {
            name: profile.to_string(),
            rules: vec![rule.to_string()],
            ..Default::default()
        })
        .expect("profile json");
        core::save_project_profile(profile, &data).expect("save profile");
    }

    /// Run `other` on its own thread, in `home`, starting once the sweep has
    /// read the project, and give it time to reach the lock before the sweep
    /// writes back.
    fn sweep_with_a_rival(
        home: &std::path::Path,
        name: &str,
        other: impl FnOnce() + Send + 'static,
    ) {
        let (started_tx, started_rx) = mpsc::channel();
        let rival_home = home.to_path_buf();
        let rival = std::thread::spawn(move || {
            core::with_test_home(rival_home, || {
                started_rx.recv().expect("recv");
                other();
            })
        });
        sweep_projects(
            vec![name.to_string()],
            &SweepProgress::silent(),
            Unreadable::Log,
            |_, _| true,
            |ident, project| {
                started_tx.send(()).expect("send");
                std::thread::sleep(Duration::from_millis(150));
                project.description = "changed by the sweep".to_string();
                persist_swept_project(ident, project, "test")
            },
        );
        rival.join().expect("rival");
    }

    #[test]
    fn activity_logged_during_a_sweep_keeps_both_changes() {
        with_temp_home(|home| {
            let (_keep, _dir) = make_project("sweep-logged", |_| {});

            sweep_with_a_rival(home, "sweep-logged", || {
                activity::log(
                    "sweep-logged",
                    ActivityEvent::ProjectUpdated,
                    "Logged",
                    "during a sweep",
                );
            });

            let saved = read_fixture("sweep-logged");
            assert_eq!(saved.description, "changed by the sweep");
            assert!(saved.last_activity.is_some(), "the log's write survives");
        });
    }

    #[test]
    fn a_read_only_toggle_during_a_project_sweep_survives_its_write() {
        with_temp_home(|home| {
            let (_keep, _dir) = make_project("sweep-toggled", |_| {});

            sweep_with_a_rival(home, "sweep-toggled", || {
                core::set_project_read_only("sweep-toggled", true).expect("toggle");
            });

            let saved = read_fixture("sweep-toggled");
            assert_eq!(saved.description, "changed by the sweep");
            assert!(saved.is_read_only_here(), "the toggle was not undone");
        });
    }

    #[test]
    fn a_project_made_read_only_after_selection_is_not_written() {
        struct ToggleWhenTotalIsKnown;
        impl SweepReporter for ToggleWhenTotalIsKnown {
            fn report(&self, event: &super::super::project_sweeps::SweepEvent) {
                if event.done == 0 && !event.finished {
                    core::set_project_read_only("sweep-late", true).expect("toggle");
                }
            }
        }

        with_temp_home(|_| {
            core::save_rule("late-rule", "Rule", "Body.\n").expect("rule");
            let (_keep, dir) = make_project("sweep-late", |p| {
                p.file_rules
                    .insert("_project".to_string(), vec!["late-rule".to_string()]);
            });

            let progress = SweepProgress::reporting(Arc::new(ToggleWhenTotalIsKnown));
            sync_projects_referencing_rule("late-rule", &progress);

            assert!(read_fixture("sweep-late").is_read_only_here());
            assert!(!dir.join(".claude").exists(), "the sweep left it alone");
        });
    }

    #[test]
    fn a_profile_sweep_syncs_only_the_projects_that_changed() {
        with_temp_home(|_| {
            save_profile("baseline", "profile-rule");
            let attach = |p: &mut core::Project| p.profiles = vec!["baseline".to_string()];
            let (_keep_a, dir_a) = make_project("in-step", attach);
            let (_keep_b, dir_b) = make_project("behind", attach);
            reconcile_projects_referencing_profile("baseline", &SweepProgress::silent());

            let mut behind = read_fixture("behind");
            behind.file_rules.clear();
            behind.profile_contributions.clear();
            save_fixture(&behind);
            std::fs::remove_dir_all(dir_a.join(".claude")).expect("clear a");
            std::fs::remove_dir_all(dir_b.join(".claude")).expect("clear b");

            let (recording, progress) = recording_progress();
            reconcile_projects_referencing_profile("baseline", &progress);

            assert!(!dir_a.join(".claude").exists(), "in step, so not synced");
            assert!(dir_b.join(".claude").exists(), "changed, so synced");
            let events = recording.events.lock().expect("events");
            assert_eq!(events.last().expect("an event").total, 1);
        });
    }

    #[test]
    fn repair_finishes_an_interrupted_update_and_can_run_twice() {
        with_temp_home(|_| {
            save_profile("baseline", "profile-rule");
            let (_keep_a, dir_a) = make_project("interrupted", |p| {
                p.profiles = vec!["baseline".to_string()];
            });
            let (_keep_b, dir_b) = make_project("read-only", |p| {
                p.profiles = vec!["baseline".to_string()];
            });
            core::set_project_read_only("read-only", true).expect("toggle");

            repair_interrupted_sweep(&SweepProgress::silent());

            let repaired = read_fixture("interrupted");
            assert_eq!(repaired.file_rules["_project"], vec!["profile-rule"]);
            assert!(dir_a.join(".claude").exists());
            assert!(read_fixture("read-only").file_rules.is_empty());
            assert!(!dir_b.join(".claude").exists(), "read-only stays untouched");

            repair_interrupted_sweep(&SweepProgress::silent());
            let again = read_fixture("interrupted");
            assert_eq!(again.file_rules, repaired.file_rules);
            assert_eq!(again.profile_contributions, repaired.profile_contributions);
        });
    }

    #[test]
    fn repair_reports_global_servers_an_apply_rejected() {
        with_temp_home(|_| {
            let config = r#"{"command":"npx","args":["--dir","${workspaceFolder}"]}"#;
            core::save_mcp_server_config("needs-a-folder", config).expect("save server");
            sync::global_mcp::apply_global_mcp("cursor", vec!["needs-a-folder".to_string()])
                .expect("apply");

            let failures = sync::global_mcp::reapply_all_agents();

            assert_eq!(failures.len(), 1, "{:?}", failures);
            assert_eq!(failures[0].0, "cursor");
            assert!(failures[0].1.contains("needs-a-folder"), "{:?}", failures);
        });
    }

    #[test]
    fn repair_names_references_to_items_the_library_lost() {
        with_temp_home(|_| {
            core::save_rule("kept-rule", "Rule", "Body.\n").expect("rule");
            let (_keep_a, _dir_a) = make_project("dangling", |p| {
                p.file_rules.insert(
                    "_project".to_string(),
                    vec!["kept-rule".to_string(), "lost-rule".to_string()],
                );
                p.skills = vec!["lost-skill".to_string()];
            });
            let (_keep_b, _dir_b) = make_project("whole", |p| {
                p.file_rules
                    .insert("_project".to_string(), vec!["kept-rule".to_string()]);
            });

            let (recording, progress) = recording_progress();
            repair_interrupted_sweep(&progress);

            let events = recording.events.lock().expect("events");
            let failed = &events.last().expect("an event").failed;
            assert_eq!(failed.len(), 1, "{:?}", failed);
            assert_eq!(failed[0].project, "dangling");
            assert!(failed[0].error.contains("rule lost-rule"), "{:?}", failed);
            assert!(failed[0].error.contains("skill lost-skill"), "{:?}", failed);
            assert!(!failed[0].error.contains("kept-rule"), "{:?}", failed);
        });
    }

    #[test]
    fn deletes_report_projects_they_cannot_read_and_saves_only_log_them() {
        with_temp_home(|_| {
            let sweep = |unreadable| {
                let (recording, progress) = recording_progress();
                sweep_projects(
                    vec!["no-such-project".to_string()],
                    &progress,
                    unreadable,
                    |_, _| true,
                    |_, _| Ok(()),
                );
                let failed = recording
                    .events
                    .lock()
                    .expect("events")
                    .last()
                    .map(|event| event.failed.len());
                failed
            };

            assert_eq!(sweep(Unreadable::Log), None);
            assert_eq!(sweep(Unreadable::Report), Some(1));
        });
    }

    #[test]
    fn a_save_runs_its_sweep_on_a_real_worker() {
        with_temp_home(|home| {
            core::save_rule("worker-rule", "Rule", "Old body.\n").expect("rule");
            let (_keep, dir) = make_project("worker-project", |p| {
                p.file_rules
                    .insert("_project".to_string(), vec!["worker-rule".to_string()]);
            });

            let (saved, recording) = with_real_worker(home, 1, || {
                crate::commands::save_rule("worker-rule", "Rule", "New body.\n")
            });

            saved.expect("save");
            let events = recording.events.lock().expect("events");
            let last = events.last().expect("an event");
            assert!(last.finished);
            assert_eq!((last.done, last.total), (1, 1));
            assert!(last.failed.is_empty(), "{:?}", last.failed);
            assert!(
                contains_in_tree(&dir, "New body."),
                "the worker synced the new content"
            );
        });
    }

    fn contains_in_tree(dir: &std::path::Path, needle: &str) -> bool {
        std::fs::read_dir(dir).expect("read dir").flatten().any(|entry| {
            let path = entry.path();
            if path.is_dir() {
                contains_in_tree(&path, needle)
            } else {
                std::fs::read_to_string(&path).is_ok_and(|text| text.contains(needle))
            }
        })
    }

    #[test]
    fn a_broken_project_file_keeps_its_parse_detail_out_of_the_task_log() {
        with_temp_home(|_| {
            let (_keep, dir) = make_project("sweep-broken", |_| {});
            std::fs::write(dir.join(".automatic.json"), "{ not json").expect("break");

            let (recording, progress) = recording_progress();
            sweep_projects(
                vec!["sweep-broken".to_string()],
                &progress,
                Unreadable::Report,
                |_, _| true,
                |_, _| Ok(()),
            );

            let events = recording.events.lock().expect("events");
            let failed = &events.last().expect("an event").failed;
            assert_eq!(failed[0].error, "The project's settings could not be read.");
        });
    }

    #[test]
    fn a_delete_returns_its_result_through_a_real_worker() {
        with_temp_home(|home| {
            core::save_rule("gone-rule", "Rule", "Body.\n").expect("rule");
            let (_keep, _dir) = make_project("gone-project", |p| {
                p.file_rules
                    .insert("_project".to_string(), vec!["gone-rule".to_string()]);
            });

            let (deleted, _recording) =
                with_real_worker(home, 1, || crate::commands::delete_rule("gone-rule"));

            deleted.expect("delete");
            assert!(read_fixture("gone-project").file_rules.is_empty());
            assert!(core::read_rule("gone-rule").is_err());
        });
    }
}

#[cfg(test)]
pub(crate) mod propagation_tests {
    use super::*;
    use tempfile::tempdir;

    /// Run `test` with a thread-local override of Automatic's HOME directory.
    /// Uses `core::paths::with_test_home` so parallel tests in other modules
    /// cannot race on the process-wide `$HOME` env var.
    pub(crate) fn with_temp_home<T>(test: impl FnOnce(&std::path::Path) -> T) -> T {
        let home = tempdir().expect("temp home");
        let home_path = home.path().to_path_buf();
        crate::core::with_test_home(home_path.clone(), || test(&home_path))
    }

    /// Create and persist a project whose directory is an empty tempdir.
    /// Returns the directory path so the test can inspect it after a sync.
    pub(crate) fn make_project(
        name: &str,
        mutate: impl FnOnce(&mut core::Project),
    ) -> (tempfile::TempDir, std::path::PathBuf) {
        let project_dir = tempdir().expect("project dir");
        let mut project = core::Project {
            name: name.to_string(),
            directory: project_dir.path().display().to_string(),
            agents: vec!["claude".to_string()],
            ..Default::default()
        };
        mutate(&mut project);
        let json = serde_json::to_string_pretty(&project).expect("project json");
        core::save_project(name, &json).expect("save project");
        let dir = project_dir.path().to_path_buf();
        (project_dir, dir)
    }

    /// Only projects that reference the named rule should be re-synced.
    /// A project that does not reference the rule must remain untouched on
    /// disk (no .claude/ directory created).
    #[test]
    fn sync_projects_referencing_rule_only_visits_referencing_projects() {
        with_temp_home(|_| {
            // The rule must exist in the library so the sync engine can
            // resolve its content when re-injecting.
            core::save_rule("test-rule", "Test Rule", "Rule body.\n")
                .expect("save rule to library");

            let (_keep_a, dir_a) = make_project("project-a", |p| {
                p.file_rules
                    .insert("_unified".to_string(), vec!["test-rule".to_string()]);
            });
            let (_keep_b, dir_b) = make_project("project-b", |_| {});

            sync_projects_referencing_rule("test-rule", &SweepProgress::silent());

            assert!(
                dir_a.join(".claude").exists(),
                "referencing project should have been synced"
            );
            assert!(
                !dir_b.join(".claude").exists(),
                "non-referencing project should NOT have been synced"
            );
        });
    }

    fn save_template_with_rules(name: &str, rules: &[&str]) {
        let data = serde_json::to_string(&core::ProjectTemplate {
            name: name.to_string(),
            unified_rules: rules.iter().map(|r| r.to_string()).collect(),
            ..Default::default()
        })
        .expect("template json");
        core::save_template(name, &data).expect("save template");
    }

    fn template_rules(name: &str) -> Vec<String> {
        let raw = core::read_template(name).expect("read template");
        let template: core::ProjectTemplate = serde_json::from_str(&raw).expect("parse template");
        template.unified_rules
    }

    /// Deleting a rule must drop it from every template that lists it, and
    /// leave the other rules and the other templates alone.
    #[test]
    fn prune_rule_from_templates_drops_only_the_named_rule() {
        with_temp_home(|_| {
            save_template_with_rules("with-rule", &["keep-rule", "gone-rule"]);
            save_template_with_rules("without-rule", &["keep-rule"]);

            prune_rule_from_templates("gone-rule");

            assert_eq!(template_rules("with-rule"), vec!["keep-rule"]);
            assert_eq!(template_rules("without-rule"), vec!["keep-rule"]);
        });
    }

    /// A template that still lists a rule the library no longer holds must
    /// not attach that rule to the project.
    #[test]
    fn apply_templates_skips_rules_missing_from_library() {
        with_temp_home(|_| {
            core::save_rule("keep-rule", "Keep Rule", "Rule body.\n").expect("save rule");
            save_template_with_rules("stale", &["keep-rule", "gone-rule"]);
            let (_keep, _dir) = make_project("project-a", |_| {});

            let result = core::apply_templates_to_project("project-a", &["stale".to_string()])
                .expect("apply templates");

            assert_eq!(result.project.file_rules["_project"], vec!["keep-rule"]);
            assert_eq!(result.pending_unified[0].rules, vec!["keep-rule"]);
            assert_eq!(
                read_back("project-a").file_rules["_project"],
                vec!["keep-rule"]
            );
        });
    }

    /// Run `body` as one machine: `home` is its `~/.automatic`.
    fn on_machine<T>(home: &std::path::Path, body: impl FnOnce() -> T) -> T {
        crate::core::with_test_home(home.to_path_buf(), body)
    }

    fn save_shared_profile(skills: &[&str]) {
        let data = serde_json::to_string(&core::ProjectProfile {
            name: "shared".to_string(),
            skills: skills.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        })
        .expect("profile json");
        core::save_project_profile("shared", &data).expect("save profile");
    }

    /// Stand in for a git push and pull: only the committed file travels.
    fn pull_config(from: &std::path::Path, to: &std::path::Path) {
        std::fs::copy(from.join(".automatic.json"), to.join(".automatic.json")).expect("pull");
    }

    /// An edit in the app: read the project and save it back unchanged.
    fn autosave(ident: &str) {
        let data = core::read_project(ident).expect("read");
        save_project_with(ident, &data, false, NewProjectRecommendations::Skip).expect("save");
    }

    /// Two people each have their own `shared` profile. Neither may remove
    /// what the other's profile added.
    #[test]
    fn teammates_with_different_same_name_profiles_keep_each_others_entries() {
        let alex_home = tempfile::tempdir().expect("home");
        let sam_home = tempfile::tempdir().expect("home");
        let alex_repo = tempfile::tempdir().expect("repo");
        let sam_repo = tempfile::tempdir().expect("repo");
        let alex_dir = alex_repo.path().to_str().unwrap().to_string();
        let sam_dir = sam_repo.path().to_str().unwrap().to_string();

        let alex = on_machine(alex_home.path(), || {
            save_shared_profile(&["react", "vitest"]);
            let project = serde_json::json!({
                "name": "demo",
                "directory": alex_dir,
                "profiles": ["shared"],
                "agents": ["claude"],
            });
            let ident = save_project_with(
                "demo",
                &project.to_string(),
                true,
                NewProjectRecommendations::Skip,
            )
            .expect("create");
            assert_eq!(read_back(&ident).skills, vec!["react", "vitest"]);
            ident
        });

        pull_config(alex_repo.path(), sam_repo.path());
        let sam = on_machine(sam_home.path(), || {
            save_shared_profile(&["react", "python"]);
            let ident = core::import_existing_project(&sam_dir, None).expect("import").local_key;
            autosave(&ident);
            let skills = read_back(&ident).skills;
            assert!(skills.contains(&"vitest".to_string()), "Sam removed vitest: {skills:?}");
            assert!(skills.contains(&"python".to_string()), "Sam lacks python: {skills:?}");
            ident
        });

        pull_config(sam_repo.path(), alex_repo.path());
        let committed = std::fs::read(alex_repo.path().join(".automatic.json")).unwrap();
        on_machine(alex_home.path(), || autosave(&alex));
        assert_eq!(
            std::fs::read(alex_repo.path().join(".automatic.json")).unwrap(),
            committed,
            "Alex's save rewrote the committed config"
        );

        pull_config(alex_repo.path(), sam_repo.path());
        on_machine(sam_home.path(), || autosave(&sam));
        assert_eq!(
            std::fs::read(sam_repo.path().join(".automatic.json")).unwrap(),
            committed,
            "Sam's save rewrote the committed config"
        );
    }

    /// Import a clone whose committed config attaches `shared`. It has no
    /// state file, so this checkout has no record yet.
    fn import_clone_with_shared_profile(repo: &std::path::Path) -> String {
        save_shared_profile(&["react"]);
        std::fs::write(
            repo.join(".automatic.json"),
            r#"{"name":"demo","profiles":["shared"],"skills":["react"]}"#,
        )
        .unwrap();
        core::import_existing_project(repo.to_str().unwrap(), None).expect("import").local_key
    }

    #[test]
    fn detach_command_removes_entries_on_a_checkout_with_no_record() {
        with_temp_home(|_| {
            let repo = tempdir().expect("repo");
            let ident = import_clone_with_shared_profile(repo.path());

            crate::commands::detach_profile_from_project(&ident, "shared").expect("detach");

            let project = read_back(&ident);
            assert!(project.profiles.is_empty());
            assert!(project.skills.is_empty());
        });
    }

    #[test]
    fn profile_delete_sweep_removes_entries_on_a_checkout_with_no_record() {
        with_temp_home(|_| {
            let repo = tempdir().expect("repo");
            let ident = import_clone_with_shared_profile(repo.path());

            detach_profile_from_projects("shared", &SweepProgress::silent());

            assert!(read_back(&ident).skills.is_empty());
        });
    }

    /// `.gitignore` does not stop a repository tracking `.automatic/`, so a
    /// clone can arrive with a state file that claims a record.
    #[test]
    fn committed_state_file_cannot_supply_a_profile_record() {
        with_temp_home(|_| {
            let repo = tempdir().expect("repo");
            let dir = repo.path().to_str().unwrap();
            std::fs::write(repo.path().join(".automatic.json"), r#"{"name":"demo"}"#).unwrap();
            std::fs::create_dir(repo.path().join(".automatic")).unwrap();
            std::fs::write(
                repo.path().join(".automatic").join("project.json"),
                r#"{
                    "profile_contributions": {"ghost": {"hooks": ["secret-scan"]}},
                    "group_profile_contributions": {"ghost": ["shared"]},
                    "group_context_contributions": {"ghost": ["policy"]}
                }"#,
            )
            .unwrap();

            let ident = core::import_existing_project(dir, None).expect("import").local_key;

            let project = read_back(&ident);
            assert!(project.profile_contributions.is_empty());
            assert!(project.group_profile_contributions.is_empty());
            assert!(project.group_context_contributions.is_empty());
        });
    }

    pub(crate) fn read_back(name: &str) -> core::Project {
        serde_json::from_str(&core::read_project(name).expect("read project")).expect("parse")
    }

    fn save_profile_with_rule(profile: &str, rule: &str) {
        core::save_rule(rule, "Profile Rule", "Profile rule body.\n").expect("save rule");
        let data = serde_json::to_string(&core::ProjectProfile {
            name: profile.to_string(),
            rules: vec![rule.to_string()],
            ..Default::default()
        })
        .expect("profile json");
        core::save_project_profile(profile, &data).expect("save profile");
    }

    /// Saving a profile reconciles and re-syncs only the projects that
    /// attach it; the rule lands in `_project` and is recorded as the
    /// profile's contribution.
    #[test]
    fn reconcile_projects_referencing_profile_only_visits_attached_projects() {
        with_temp_home(|_| {
            save_profile_with_rule("baseline", "profile-rule");

            let (_keep_a, dir_a) = make_project("project-a", |p| {
                p.profiles = vec!["baseline".to_string()];
            });
            let (_keep_b, dir_b) = make_project("project-b", |_| {});

            reconcile_projects_referencing_profile("baseline", &SweepProgress::silent());

            let a = read_back("project-a");
            assert_eq!(a.file_rules["_project"], vec!["profile-rule"]);
            assert_eq!(
                a.profile_contributions["baseline"].rules,
                vec!["profile-rule"]
            );
            assert!(dir_a.join(".claude").exists(), "attached project synced");

            let b = read_back("project-b");
            assert!(b.file_rules.is_empty());
            assert!(!dir_b.join(".claude").exists(), "other project untouched");
        });
    }

/// Detaching before delete removes the profile's entries and preserves
/// project entries that the profile does not provide.
#[test]
fn detach_profile_from_projects_removes_provided_entries() {
        with_temp_home(|_| {
            save_profile_with_rule("baseline", "profile-rule");
            core::save_rule("own-rule", "Own Rule", "Own rule body.\n").expect("save rule");

            let (_keep, _dir) = make_project("project-a", |p| {
                p.profiles = vec!["baseline".to_string()];
                p.file_rules
                    .insert("_project".to_string(), vec!["own-rule".to_string()]);
            });
            reconcile_projects_referencing_profile("baseline", &SweepProgress::silent());
            assert_eq!(
                read_back("project-a").file_rules["_project"],
                vec!["own-rule", "profile-rule"]
            );

            detach_profile_from_projects("baseline", &SweepProgress::silent());

            let a = read_back("project-a");
            assert!(a.profiles.is_empty());
            assert!(a.profile_contributions.is_empty());
            assert_eq!(a.file_rules["_project"], vec!["own-rule"]);
        });
    }

    /// A member of a group that provides a profile and a context. Returns
    /// the path of the group file, and the temp dir and project directory.
    fn project_in_group_with_entries(
    ) -> (std::path::PathBuf, tempfile::TempDir, std::path::PathBuf) {
        save_profile_with_rule("baseline", "profile-rule");
        let (keep, dir) = make_project("project-a", |_| {});
        save_group_command("team", &["project-a"], &["baseline"]);
        let mut group = read_group_back("team");
        group.contexts = vec!["shared".to_string()];
        super::super::groups::save_group("team", &serde_json::to_string(&group).expect("json"))
            .expect("save group");
        assert_keeps_group_entries();
        let path = core::get_groups_dir().expect("groups dir").join("team.json");
        (path, keep, dir)
    }

    fn assert_keeps_group_entries() {
        let project = read_back("project-a");
        assert_eq!(project.profiles, vec!["baseline"]);
        assert_eq!(project.group_profile_contributions["team"], vec!["baseline"]);
        assert_eq!(project.file_rules["_project"], vec!["profile-rule"]);
        assert_eq!(project.contexts, vec!["shared"]);
        assert_eq!(project.group_context_contributions["team"], vec!["shared"]);
    }

    fn save_project_a_again() {
        let data = serde_json::to_string(&read_back("project-a")).expect("json");
        save_project_with("project-a", &data, false, NewProjectRecommendations::Skip)
            .expect("the save goes ahead");
    }

    /// A corrupt group file leaves membership unknown. Saves and sweeps
    /// must not read that as the project leaving the group.
    #[test]
    fn corrupt_group_file_releases_nothing_on_save_or_sweep() {
        with_temp_home(|_| {
            let (group_file, _keep, _dir) = project_in_group_with_entries();
            let good = std::fs::read(&group_file).expect("read group file");

            std::fs::write(&group_file, "{ not json").expect("corrupt group file");
            save_project_a_again();
            assert_keeps_group_entries();

            let idents = vec!["project-a".to_string()];
            reconcile_group_profiles_for_projects(&idents, &SweepProgress::silent());
            core::reconcile_group_contexts_for_projects(&idents).expect("sweep");
            assert_keeps_group_entries();
            assert_eq!(
                std::fs::read_to_string(&group_file).expect("read group file"),
                "{ not json",
                "the damaged file stays for the user to repair"
            );

            std::fs::write(&group_file, &good).expect("repair group file");
            save_project_a_again();
            assert_keeps_group_entries();
        });
    }

    /// The other half of the guard: a group that is really gone still
    /// releases what it provided.
    #[test]
    fn removed_group_file_still_releases_on_save() {
        with_temp_home(|_| {
            let (group_file, _keep, _dir) = project_in_group_with_entries();

            std::fs::remove_file(&group_file).expect("remove group file");
            save_project_a_again();

            let project = read_back("project-a");
            assert!(project.profiles.is_empty());
            assert!(project.contexts.is_empty());
            assert!(project.group_profile_contributions.is_empty());
            assert!(project.group_context_contributions.is_empty());
        });
    }

    /// An explicit detach cannot tell whether the unreadable group provides
    /// the profile, so it refuses and changes nothing.
    #[test]
    fn detach_profile_refuses_while_a_group_file_is_corrupt() {
        with_temp_home(|_| {
            let (group_file, _keep, _dir) = project_in_group_with_entries();

            std::fs::write(&group_file, "{ not json").expect("corrupt group file");
            let error = super::super::project_profiles::detach_profile_from_project(
                "project-a",
                "baseline",
            )
            .expect_err("membership is unknown");

            assert!(error.contains("team"), "{error}");
            assert_keeps_group_entries();
        });
    }

    /// The `save_project` command reconciles on every save, so a project
    /// saved with a profile name attached picks the profile up immediately.
    #[test]
    fn save_project_command_reconciles_attached_profiles() {
        with_temp_home(|_| {
            save_profile_with_rule("baseline", "profile-rule");
            let project_dir = tempdir().expect("project dir");
            let project = core::Project {
                name: "project-a".to_string(),
                directory: project_dir.path().display().to_string(),
                agents: vec!["claude".to_string()],
                profiles: vec!["baseline".to_string()],
                ..Default::default()
            };
            let json = serde_json::to_string_pretty(&project).expect("json");

            save_project("project-a", &json, Some(true)).expect("save");

            let a = read_back("project-a");
            assert_eq!(a.file_rules["_project"], vec!["profile-rule"]);
            assert_eq!(
                a.profile_contributions["baseline"].rules,
                vec!["profile-rule"]
            );
        });
    }

    fn save_group_command(name: &str, projects: &[&str], profiles: &[&str]) {
        let group = core::ProjectGroup {
            name: name.to_string(),
            projects: projects.iter().map(|p| p.to_string()).collect(),
            profiles: profiles.iter().map(|p| p.to_string()).collect(),
            ..Default::default()
        };
        let data = serde_json::to_string(&group).expect("group json");
        super::super::groups::save_group(name, &data).expect("save group");
    }

    fn read_group_back(name: &str) -> core::ProjectGroup {
        serde_json::from_str(&core::read_group(name).expect("read group")).expect("parse group")
    }

    /// Saving a group gives its profiles to every member, materialises the
    /// profile's entries there, and syncs. A project that leaves the group,
    /// or whose group is deleted, loses both.
    #[test]
    fn save_group_command_gives_and_takes_group_profiles() {
        with_temp_home(|_| {
            save_profile_with_rule("baseline", "profile-rule");
            let (_keep_a, dir_a) = make_project("project-a", |_| {});
            let (_keep_b, dir_b) = make_project("project-b", |_| {});
            let (_keep_c, dir_c) = make_project("project-c", |_| {});

            save_group_command("team", &["project-a", "project-b"], &["baseline"]);

            for name in ["project-a", "project-b"] {
                let p = read_back(name);
                assert_eq!(p.profiles, vec!["baseline"], "{name}");
                assert_eq!(p.group_profile_contributions["team"], vec!["baseline"]);
                assert_eq!(p.file_rules["_project"], vec!["profile-rule"]);
                assert_eq!(p.profile_contributions["baseline"].rules, vec!["profile-rule"]);
            }
            assert!(dir_a.join(".claude").exists(), "member synced");
            assert!(dir_b.join(".claude").exists(), "member synced");
            let c = read_back("project-c");
            assert!(c.profiles.is_empty());
            assert!(!dir_c.join(".claude").exists(), "non-member untouched");

            // project-a leaves the group.
            save_group_command("team", &["project-b"], &["baseline"]);
            let a = read_back("project-a");
            assert!(a.profiles.is_empty());
            assert!(a.group_profile_contributions.is_empty());
            assert!(a.profile_contributions.is_empty());
            assert!(a.file_rules.is_empty());
            assert_eq!(read_back("project-b").profiles, vec!["baseline"]);

            super::super::groups::delete_group("team").expect("delete group");
            let b = read_back("project-b");
            assert!(b.profiles.is_empty());
            assert!(b.group_profile_contributions.is_empty());
            assert!(b.file_rules.is_empty());
        });
    }

    /// The group attach and detach commands edit the group file and reach
    /// the members. A member cannot detach a profile its group provides.
    #[test]
    fn group_profile_commands_reach_members_and_lock_the_profile() {
        use super::super::groups::{
            attach_profile_to_group, detach_profile_from_group, get_groups_referencing_profile,
        };
        use super::super::project_profiles::{
            attach_profile_to_project, detach_profile_from_project,
        };

        with_temp_home(|_| {
            save_profile_with_rule("baseline", "profile-rule");
            let (_keep, dir) = make_project("project-a", |_| {});
            save_group_command("team", &["project-a"], &[]);
            assert!(!dir.join(".claude").exists(), "nothing to sync yet");

            assert!(attach_profile_to_group("team", "ghost").is_err(), "unknown profile");
            assert!(attach_profile_to_group("nope", "baseline").is_err(), "unknown group");
            assert!(detach_profile_from_group("nope", "baseline").is_err(), "unknown group");

            attach_profile_to_group("team", "baseline").expect("attach");
            attach_profile_to_group("team", "baseline").expect("attach is idempotent");
            assert_eq!(read_group_back("team").profiles, vec!["baseline"]);
            assert_eq!(get_groups_referencing_profile("baseline").unwrap(), vec!["team"]);
            let a = read_back("project-a");
            assert_eq!(a.profiles, vec!["baseline"]);
            assert_eq!(a.file_rules["_project"], vec!["profile-rule"]);
            assert!(dir.join(".claude").exists(), "member synced");

            let err = detach_profile_from_project("project-a", "baseline").unwrap_err();
            assert_eq!(
                err,
                "Profile 'baseline' is provided by group 'team'. Detach it from the group instead."
            );
            assert_eq!(read_back("project-a").profiles, vec!["baseline"]);

            // Attaching it to the project again changes nothing: the group
            // still owns it.
            attach_profile_to_project("project-a", "baseline").expect("attach to project");
            let a = read_back("project-a");
            assert_eq!(a.profiles, vec!["baseline"]);
            assert_eq!(a.group_profile_contributions["team"], vec!["baseline"]);

            detach_profile_from_group("team", "baseline").expect("detach");
            detach_profile_from_group("team", "baseline").expect("detach is idempotent");
            assert!(read_group_back("team").profiles.is_empty());
            assert!(get_groups_referencing_profile("baseline").unwrap().is_empty());
            let a = read_back("project-a");
            assert!(a.profiles.is_empty());
            assert!(a.group_profile_contributions.is_empty());
            assert!(a.file_rules.is_empty());

            // With the group out of the way the project owns its profile.
            attach_profile_to_project("project-a", "baseline").expect("attach to project");
            detach_profile_from_project("project-a", "baseline").expect("detach from project");
            assert!(read_back("project-a").profiles.is_empty());
        });
    }

    /// A stale group record must not block a detach: the reconcile against
    /// the groups on disk clears it first.
    #[test]
    fn stale_group_record_does_not_block_a_project_detach() {
        with_temp_home(|_| {
            save_profile_with_rule("baseline", "profile-rule");
            let (_keep, _dir) = make_project("project-a", |p| {
                p.profiles = vec!["baseline".to_string(), "other".to_string()];
                p.group_profile_contributions
                    .insert("gone".to_string(), vec!["other".to_string()]);
            });

            super::super::project_profiles::detach_profile_from_project("project-a", "baseline")
                .expect("detach");

            let a = read_back("project-a");
            assert!(a.profiles.is_empty(), "the departed group's profile went too");
            assert!(a.group_profile_contributions.is_empty());
        });
    }

    /// The `save_project` command puts back a group profile a stale editor
    /// copy dropped, and materialises it in the same save.
    #[test]
    fn save_project_command_keeps_group_profiles() {
        with_temp_home(|_| {
            save_profile_with_rule("baseline", "profile-rule");
            let (_keep, _dir) = make_project("project-a", |_| {});
            save_group_command("team", &["project-a"], &["baseline"]);

            let mut stale = read_back("project-a");
            stale.profiles.clear();
            stale.group_profile_contributions.clear();
            let json = serde_json::to_string(&stale).expect("json");
            save_project("project-a", &json, None).expect("save");

            let a = read_back("project-a");
            assert_eq!(a.profiles, vec!["baseline"]);
            assert_eq!(a.group_profile_contributions["team"], vec!["baseline"]);
            assert_eq!(a.file_rules["_project"], vec!["profile-rule"]);
        });
    }

    /// Deleting a profile clears it from groups as well as projects, so no
    /// group puts the missing profile back. Renaming follows it everywhere.
    #[test]
    fn profile_delete_and_rename_cascade_into_groups() {
        use super::super::project_profiles::{delete_project_profile, rename_project_profile};

        with_temp_home(|_| {
            save_profile_with_rule("baseline", "profile-rule");
            let (_keep, _dir) = make_project("project-a", |_| {});
            save_group_command("team", &["project-a"], &["baseline"]);

            rename_project_profile("baseline", "base").expect("rename");
            assert_eq!(read_group_back("team").profiles, vec!["base"]);
            let a = read_back("project-a");
            assert_eq!(a.profiles, vec!["base"]);
            assert_eq!(a.group_profile_contributions["team"], vec!["base"]);
            assert_eq!(a.profile_contributions["base"].rules, vec!["profile-rule"]);
            assert_eq!(a.file_rules["_project"], vec!["profile-rule"]);

            delete_project_profile("base").expect("delete");
            assert!(read_group_back("team").profiles.is_empty());
            let a = read_back("project-a");
            assert!(a.profiles.is_empty());
            assert!(a.group_profile_contributions.is_empty());
            assert!(a.profile_contributions.is_empty());
            assert!(a.file_rules.is_empty());

            // Saving the group again must not bring the profile back.
            save_group_command("team", &["project-a"], &[]);
            assert!(read_back("project-a").profiles.is_empty());
        });
    }

    /// `resync_all_projects` must visit every configured project
    /// regardless of which assets it references.
    #[test]
    fn resync_all_projects_visits_every_configured_project() {
        with_temp_home(|_| {
            let (_keep_a, dir_a) = make_project("project-a", |_| {});
            let (_keep_b, dir_b) = make_project("project-b", |_| {});

            resync_all_projects(&SweepProgress::silent());

            assert!(
                dir_a.join(".claude").exists(),
                "project-a should have been synced"
            );
            assert!(
                dir_b.join(".claude").exists(),
                "project-b should have been synced"
            );
        });
    }

    /// Only projects that reference the named user command should be
    /// re-synced.
    #[test]
    fn sync_projects_referencing_user_command_only_visits_referencing_projects() {
        with_temp_home(|_| {
            core::save_user_command("review", "---\ndescription: Review\n---\n\nLook hard.\n")
                .expect("save user command");

            let (_keep_a, dir_a) = make_project("project-a", |p| {
                p.user_commands.push("review".to_string());
            });
            let (_keep_b, dir_b) = make_project("project-b", |_| {});

            sync_projects_referencing_user_command("review", &SweepProgress::silent());

            assert!(
                dir_a.join(".claude").exists(),
                "referencing project should have been synced"
            );
            assert!(
                !dir_b.join(".claude").exists(),
                "non-referencing project should NOT have been synced"
            );
        });
    }

    #[test]
    fn save_project_creating_true_rejects_a_taken_directory_and_preserves_data() {
        with_temp_home(|_| {
            let project_dir = tempdir().expect("project dir");
            let dir = project_dir.path().display().to_string();
            let existing = core::Project {
                name: "alpha".to_string(),
                directory: dir.clone(),
                skills: vec!["keep-me".to_string()],
                agents: vec!["claude".to_string()],
                ..Default::default()
            };
            let existing_json = serde_json::to_string_pretty(&existing).expect("json");
            core::save_project("alpha", &existing_json).expect("seed");

            let stub = core::Project {
                name: "alpha".to_string(),
                directory: dir,
                ..Default::default()
            };
            let stub_json = serde_json::to_string_pretty(&stub).expect("stub json");

            let err = save_project("alpha", &stub_json, Some(true))
                .expect_err("creating over an existing project's directory must fail");
            assert!(
                err.contains("already registered as project 'alpha'"),
                "unexpected error: {err}"
            );

            let raw = core::read_project("alpha").expect("read");
            let loaded: core::Project = serde_json::from_str(&raw).expect("parse");
            assert_eq!(
                loaded.skills,
                vec!["keep-me".to_string()],
                "failed create must not overwrite existing project data"
            );
        });
    }

    /// Stage 6: a second project may take a name another project uses. The
    /// create returns the new project's key, and the two stay independent.
    #[test]
    fn creating_a_project_with_a_taken_name_makes_a_separate_project() {
        with_temp_home(|_| {
            let first_dir = tempdir().expect("first dir");
            let second_dir = tempdir().expect("second dir");
            let stub = |dir: &std::path::Path, description: &str| {
                serde_json::to_string(&core::Project {
                    name: "website".to_string(),
                    description: description.to_string(),
                    directory: dir.display().to_string(),
                    ..Default::default()
                })
                .unwrap()
            };
            let first = save_project("website", &stub(first_dir.path(), "first"), Some(true))
                .expect("create the first");
            let second = save_project("website", &stub(second_dir.path(), "second"), Some(true))
                .expect("create the second under the same name");
            assert_ne!(first, second);
            assert!(core::is_project_key(&first) && core::is_project_key(&second));

            let load = |key: &str| -> core::Project {
                serde_json::from_str(&core::read_project(key).unwrap()).unwrap()
            };
            let (a, b) = (load(&first), load(&second));
            assert_eq!((a.name.as_str(), a.description.as_str()), ("website", "first"));
            assert_eq!((b.name.as_str(), b.description.as_str()), ("website", "second"));
            assert_ne!(a.id, b.id, "unrelated folders are different projects");
            assert_eq!(core::list_projects().unwrap(), vec!["website"]);
            assert_eq!(core::list_project_idents().unwrap().len(), 2);

            // Saving one by its key leaves the other alone.
            let mut edited = a.clone();
            edited.description = "first, edited".into();
            save_project(&first, &serde_json::to_string(&edited).unwrap(), None).expect("save");
            assert_eq!(load(&first).description, "first, edited");
            assert_eq!(load(&second).description, "second");

            // Memory is per project; activity per checkout.
            super::super::memory::store_memory(&first, "k", "one", None).unwrap();
            super::super::memory::store_memory(&second, "k", "two", None).unwrap();
            assert_eq!(super::super::memory::get_project_memories(&first).unwrap()["k"].value, "one");
            assert_eq!(super::super::memory::get_project_memories(&second).unwrap()["k"].value, "two");
            let err = super::super::memory::store_memory("website", "k", "?", None)
                .expect_err("a shared name is ambiguous");
            assert!(err.contains(&first) && err.contains(&second), "{err}");

            // Features are per project.
            crate::plugins::build::commands::create_feature(
                &first, "only first", None, None, None, None, None, None, None, None,
            )
            .expect("feature by key");
            assert_eq!(crate::plugins::build::commands::list_features(&first, None, None).unwrap().len(), 1);
            assert!(crate::plugins::build::commands::list_features(&second, None, None).unwrap().is_empty());

            // Groups hold ids; each checkout shows under its own key.
            core::save_group(
                "g",
                &serde_json::json!({"name": "g", "projects": [second.clone()]}).to_string(),
            )
            .unwrap();
            assert_eq!(super::super::groups::groups_for_project(&second).unwrap(), vec!["g"]);
            assert!(super::super::groups::groups_for_project(&first).unwrap().is_empty());
            assert!(super::super::groups::groups_for_project("website").is_err());

            // Drift and sync address one project each. With an agent, sync
            // writes into that project's folder only.
            let mut with_agent = load(&second);
            with_agent.agents = vec!["claude".into()];
            save_project(&second, &serde_json::to_string(&with_agent).unwrap(), None).expect("add agent");
            sync_project(&second).expect("sync by key");
            assert!(second_dir.path().join("CLAUDE.md").exists());
            assert!(!first_dir.path().join("CLAUDE.md").exists());
            check_project_drift(&first).expect("drift by key");
            let err = check_project_drift("website").expect_err("ambiguous");
            assert!(err.contains("More than one project is named 'website'"), "{err}");
        });
    }

    /// The Add Project wizard saves an agent-less stub with `creating: true`,
    /// then reads `autodetect_project_dependencies` to show what was found.
    /// That creation flow is the one place agents on disk are adopted.
    #[test]
    fn creating_a_project_from_existing_files_adopts_detected_agents() {
        with_temp_home(|_| {
            let project_dir = tempdir().expect("project dir");
            std::fs::write(project_dir.path().join("CLAUDE.md"), "# Claude").expect("CLAUDE.md");
            let stub = core::Project {
                name: "wizard".to_string(),
                directory: project_dir.path().display().to_string(),
                ..Default::default()
            };
            let stub_json = serde_json::to_string_pretty(&stub).expect("stub json");

            save_project("wizard", &stub_json, Some(true)).expect("create");

            assert_eq!(read_back("wizard").agents, vec!["claude".to_string()]);
            let detected: core::Project = serde_json::from_str(
                &autodetect_project_dependencies("wizard").expect("autodetect"),
            )
            .expect("parse");
            assert_eq!(detected.agents, vec!["claude".to_string()]);
        });
    }

    /// A folder added from the Find projects dialog goes through the same
    /// create path as the wizard: it is named after the folder and adopts
    /// the agents found on disk.
    #[test]
    fn adding_a_discovered_folder_creates_a_project_with_its_agents() {
        with_temp_home(|_| {
            let parent = tempdir().expect("parent");
            let project_dir = parent.path().canonicalize().expect("canonical").join("my-app");
            std::fs::create_dir_all(&project_dir).expect("mkdir");
            std::fs::write(project_dir.join("CLAUDE.md"), "# Claude").expect("CLAUDE.md");
            let dir = project_dir.display().to_string();

            let added = add_discovered_project(&dir, Some("user_1"), None).expect("add");
            assert_eq!(added.name, "my-app");
            assert!(!added.read_only);

            let saved = read_back(&added.local_key);
            assert_eq!(saved.name, "my-app");
            assert_eq!(saved.directory, dir);
            assert_eq!(saved.agents, vec!["claude".to_string()]);
            assert_eq!(saved.created_by.as_deref(), Some("user_1"));
            assert_eq!(saved.instruction_mode, "per-agent");

            let err = add_discovered_project(&dir, None, None).expect_err("second add");
            assert!(err.contains("Already added as project 'my-app'"), "{err}");
        });
    }

    #[test]
    fn adding_a_discovered_folder_rejects_bad_paths() {
        with_temp_home(|_| {
            assert!(add_discovered_project("relative/dir", None, None).is_err());

            let parent = tempdir().expect("parent");
            let base = parent.path().canonicalize().expect("canonical");
            std::fs::create_dir_all(base.join("x")).expect("mkdir");
            std::fs::create_dir_all(base.join("y")).expect("mkdir");
            let dotted = format!("{}/x/../y", base.display());
            let err = add_discovered_project(&dotted, None, None).expect_err("dotted path");
            assert!(err.contains("symlink or '..'"), "{err}");

            let file = base.join("file.txt");
            std::fs::write(&file, "x").expect("write");
            assert!(add_discovered_project(&file.display().to_string(), None, None).is_err());

            #[cfg(unix)]
            {
                let target = base.join("real");
                std::fs::create_dir_all(&target).expect("mkdir");
                let link = base.join("link");
                std::os::unix::fs::symlink(&target, &link).expect("symlink");
                assert!(add_discovered_project(&link.display().to_string(), None, None).is_err());
                // A symlink higher up the path is caught too.
                std::fs::create_dir_all(target.join("inner")).expect("mkdir");
                let through = link.join("inner");
                assert!(add_discovered_project(&through.display().to_string(), None, None).is_err());
            }
        });
    }

    /// A folder with an unregistered Automatic config is imported, so it
    /// keeps the name and id its committed config carries.
    #[test]
    fn adding_a_discovered_folder_imports_an_existing_config() {
        with_temp_home(|_| {
            let parent = tempdir().expect("parent");
            let dir = parent.path().canonicalize().expect("canonical").join("folder-name");
            std::fs::create_dir_all(&dir).expect("mkdir");
            std::fs::write(
                dir.join(".automatic.json"),
                r#"{"name":"ambient","id":"proj-ambient-1","agents":["claude"]}"#,
            )
            .expect("write config");
            let dir_str = dir.display().to_string();
            assert!(matches!(
                core::inspect_project_directory(&dir_str).expect("inspect"),
                core::DirectoryStatus::OrphanConfig { .. }
            ));

            let added = add_discovered_project(&dir_str, None, None).expect("import");

            assert_eq!(added.name, "ambient");
            assert!(added.read_only, "a folder with no recorded choice starts read only");
            let saved = read_back(&added.local_key);
            assert_eq!(saved.name, "ambient");
            assert_eq!(saved.id, "proj-ambient-1");
            assert_eq!(saved.directory, dir_str);
            assert!(matches!(
                core::inspect_project_directory(&dir_str).expect("inspect"),
                core::DirectoryStatus::RegisteredHere { .. }
            ));
        });
    }

    /// The choice on the folder's row decides the mode, over the committed hint.
    #[test]
    fn adding_a_discovered_folder_follows_the_recorded_choice() {
        with_temp_home(|_| {
            let parent = tempdir().expect("parent");
            let dir = parent.path().canonicalize().expect("canonical").join("chosen");
            std::fs::create_dir_all(&dir).expect("mkdir");
            std::fs::write(dir.join(".automatic.json"), r#"{"name":"chosen","read_only":true}"#)
                .expect("write config");
            let directory = dir.display().to_string();

            let choices = HashMap::from([(directory.clone(), false)]);
            let added = add_discovered_folders(vec![directory], None, &choices);

            assert!(added[0].local_key.is_some(), "{:?}", added[0].error);
            assert!(!added[0].read_only);
        });
    }

    /// A committed `read_only` hint is honoured on import, as in the wizard.
    #[test]
    fn adding_a_discovered_folder_imports_a_read_only_config_as_read_only() {
        with_temp_home(|_| {
            let parent = tempdir().expect("parent");
            let dir = parent.path().canonicalize().expect("canonical").join("locked");
            std::fs::create_dir_all(&dir).expect("mkdir");
            std::fs::write(
                dir.join(".automatic.json"),
                r#"{"name":"locked","id":"proj-locked-1","read_only":true}"#,
            )
            .expect("write config");

            let added = add_discovered_project(&dir.display().to_string(), None, None).expect("import");

            assert!(added.read_only);
        });
    }

    #[test]
    fn adding_a_discovered_folder_refuses_the_home_folder() {
        with_temp_home(|_| {
            let home = crate::agent::home_dir().expect("home");
            std::fs::create_dir_all(&home).expect("mkdir");
            let home = home.canonicalize().expect("canonical");
            std::fs::write(home.join("CLAUDE.md"), "# global").expect("write");

            let err = add_discovered_project(&home.display().to_string(), None, None)
                .expect_err("home must be refused");
            assert!(err.contains("home folder"), "{err}");
        });
    }

    #[test]
    fn adding_discovered_folders_reports_each_result() {
        with_temp_home(|_| {
            let parent = tempdir().expect("parent");
            let base = parent.path().canonicalize().expect("canonical");
            let good = base.join("good");
            std::fs::create_dir_all(&good).expect("mkdir");
            let missing = base.join("missing");

            let results = add_discovered_folders(
                vec![missing.display().to_string(), good.display().to_string()],
                None,
                &HashMap::new(),
            );

            assert_eq!(results.len(), 2);
            assert!(results[0].error.is_some() && results[0].local_key.is_none());
            assert!(results[1].error.is_none() && results[1].local_key.is_some());
        });
    }

    /// The wizard's create is one of the paths that mints identity keys. The
    /// editor's follow-up save carries no keys and must not erase them.
    #[test]
    fn creating_a_project_mints_keys_that_later_saves_keep() {
        with_temp_home(|_| {
            let project_dir = tempdir().expect("project dir");
            let stub = core::Project {
                name: "minted".to_string(),
                directory: project_dir.path().display().to_string(),
                ..Default::default()
            };
            let stub_json = serde_json::to_string_pretty(&stub).expect("stub json");

            save_project("minted", &stub_json, Some(true)).expect("create");
            let created = read_back("minted");
            assert!(uuid::Uuid::parse_str(&created.id).is_ok(), "create mints an id");
            assert!(uuid::Uuid::parse_str(&created.local_key).is_ok());

            save_project("minted", &stub_json, None).expect("keyless save");
            let saved = read_back("minted");
            assert_eq!(saved.id, created.id);
            assert_eq!(saved.local_key, created.local_key);
        });
    }

    /// An existing project whose agent list is empty (for example after the
    /// last agent was removed with "Keep files") must stay empty when it is
    /// synced or loaded, even though that agent's files are still on disk.
    #[test]
    fn existing_project_with_no_agents_never_adopts_agents_from_disk() {
        with_temp_home(|_| {
            let (_keep, dir) = make_project("kept-files", |p| p.agents.clear());
            std::fs::write(dir.join("CLAUDE.md"), "# Claude").expect("CLAUDE.md");
            std::fs::create_dir_all(dir.join(".claude").join("skills")).expect("mkdir");

            let written = sync_project("kept-files").expect("sync");
            assert_eq!(written.trim(), "[]");
            assert!(read_back("kept-files").agents.is_empty());

            let detected: core::Project = serde_json::from_str(
                &autodetect_project_dependencies("kept-files").expect("autodetect"),
            )
            .expect("parse");
            assert!(detected.agents.is_empty(), "got {:?}", detected.agents);
        });
    }

    #[test]
    fn save_project_without_creating_flag_still_updates_existing() {
        with_temp_home(|_| {
            let project_dir = tempdir().expect("project dir");
            let dir = project_dir.path().display().to_string();
            let existing = core::Project {
                name: "alpha".to_string(),
                directory: dir.clone(),
                skills: vec!["old".to_string()],
                agents: vec!["claude".to_string()],
                ..Default::default()
            };
            core::save_project(
                "alpha",
                &serde_json::to_string_pretty(&existing).expect("json"),
            )
            .expect("seed");

            let updated = core::Project {
                name: "alpha".to_string(),
                directory: dir,
                skills: vec!["new".to_string()],
                agents: vec!["claude".to_string()],
                ..Default::default()
            };
            save_project(
                "alpha",
                &serde_json::to_string_pretty(&updated).expect("json"),
                None,
            )
            .expect("normal save should update");

            let raw = core::read_project("alpha").expect("read");
            let loaded: core::Project = serde_json::from_str(&raw).expect("parse");
            assert_eq!(loaded.skills, vec!["new".to_string()]);
        });
    }

    // ── delete commands prune what they delete ───────────────────────────────

    fn save_profile(name: &str, edit: impl FnOnce(&mut core::ProjectProfile)) {
        let mut profile = core::ProjectProfile {
            name: name.to_string(),
            ..Default::default()
        };
        edit(&mut profile);
        let data = serde_json::to_string(&profile).expect("profile json");
        core::save_project_profile(name, &data).expect("save profile");
    }

    fn read_profile(name: &str) -> core::ProjectProfile {
        core::read_project_profile_parsed(name).expect("read profile")
    }

    #[test]
    fn deleting_an_mcp_server_prunes_it_from_projects_profiles_and_global_selections() {
        with_temp_home(|_| {
            // Named with the prefix no other test uses: the secret store is
            // shared by the whole test process.
            let name = "source-remove-delete-server";
            core::save_mcp_server_config(name, r#"{"type":"stdio","command":"echo"}"#)
                .expect("save server");
            save_profile("delete-profile", |p| p.mcp_servers = vec![name.to_string()]);
            let (_keep, dir) = make_project("project-a", |p| {
                p.mcp_servers = vec![name.to_string()];
            });
            sync_projects_referencing_mcp_server(name, &SweepProgress::silent());
            crate::sync::global_mcp::apply_global_mcp("cursor", vec![name.to_string()])
                .expect("select globally");
            let mcp_json = || std::fs::read_to_string(dir.join(".mcp.json")).unwrap_or_default();
            assert!(mcp_json().contains(name), "the project was synced first");

            super::super::mcp_servers::delete_mcp_server_config(name).expect("delete");

            assert!(read_back("project-a").mcp_servers.is_empty());
            assert!(read_profile("delete-profile").mcp_servers.is_empty());
            let state = core::global_mcp::load_global_mcp_state().expect("global state");
            assert!(state.agents["cursor"].selected.is_empty());
            assert!(!mcp_json().contains(name), "{}", mcp_json());
        });
    }

    #[test]
    fn deleting_a_rule_prunes_it_from_projects_profiles_and_templates() {
        with_temp_home(|_| {
            core::save_rule("delete-rule", "Delete Rule", "Body.\n").expect("save rule");
            core::save_rule("keep-rule", "Keep Rule", "Body.\n").expect("save rule");
            save_profile("delete-profile", |p| p.rules = vec!["delete-rule".to_string()]);
            save_template_with_rules("with-rule", &["delete-rule", "keep-rule"]);
            let (_keep, _dir) = make_project("project-a", |p| {
                p.file_rules.insert(
                    "_project".to_string(),
                    vec!["delete-rule".to_string(), "keep-rule".to_string()],
                );
            });

            super::super::rules::delete_rule("delete-rule").expect("delete");

            assert_eq!(read_back("project-a").file_rules["_project"], vec!["keep-rule"]);
            assert!(read_profile("delete-profile").rules.is_empty());
            assert_eq!(template_rules("with-rule"), vec!["keep-rule"]);
        });
    }

    #[test]
    fn deleting_a_skill_prunes_it_from_projects_and_profiles() {
        with_temp_home(|_| {
            core::save_skill(
                "delete-skill",
                "---\nname: delete-skill\ndescription: A skill\n---\n# Skill\n",
            )
            .expect("save skill");
            save_profile("delete-profile", |p| p.skills = vec!["delete-skill".to_string()]);
            let (_keep, _dir) = make_project("project-a", |p| {
                p.skills = vec!["delete-skill".to_string()];
            });

            super::super::skills::delete_skill("delete-skill").expect("delete");

            assert!(read_back("project-a").skills.is_empty());
            assert!(read_profile("delete-profile").skills.is_empty());
        });
    }

    #[test]
    fn deleting_a_command_prunes_it_from_profiles_and_their_projects() {
        with_temp_home(|_| {
            core::save_user_command("delete-command", "# A command\n").expect("save command");
            save_profile("delete-profile", |p| {
                p.user_commands = vec!["delete-command".to_string()]
            });
            let (_keep, _dir) = make_project("project-a", |p| {
                p.profiles = vec!["delete-profile".to_string()];
            });
            reconcile_projects_referencing_profile("delete-profile", &SweepProgress::silent());
            assert_eq!(read_back("project-a").user_commands, vec!["delete-command"]);

            super::super::user_commands::delete_user_command("delete-command".to_string())
                .expect("delete");

            assert!(read_back("project-a").user_commands.is_empty());
            assert!(read_profile("delete-profile").user_commands.is_empty());
        });
    }

    #[test]
    fn deleting_a_subagent_prunes_it_from_profiles_and_their_projects() {
        with_temp_home(|_| {
            core::save_subagent(
                "delete-agent",
                "Delete Agent",
                "---\nname: delete-agent\ndescription: An agent\n---\nBody\n",
            )
            .expect("save agent");
            save_profile("delete-profile", |p| {
                p.user_agents = vec!["delete-agent".to_string()]
            });
            let (_keep, _dir) = make_project("project-a", |p| {
                p.profiles = vec!["delete-profile".to_string()];
            });
            reconcile_projects_referencing_profile("delete-profile", &SweepProgress::silent());
            assert_eq!(read_back("project-a").user_agents, vec!["delete-agent"]);

            super::super::subagents::delete_subagent("delete-agent".to_string())
                .expect("delete");

            assert!(read_back("project-a").user_agents.is_empty());
            assert!(read_profile("delete-profile").user_agents.is_empty());
        });
    }
}

/// Commands accept a `local_key` or a name (stage 3b step 1), and each
/// store receives its key: `id` for memory, features and groups, `local_key`
/// for activity, recommendations and dev servers (stage 3b step 2).
/// Case (d): adopt and overwrite refuse a Silent-mode directory that links
/// out of the project.
#[cfg(all(test, unix))]
mod silent_dir_tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn effective_project_dir_refuses_a_linked_silent_directory() {
        let dir = tempdir().expect("tempdir");
        let outside = tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join(".automatic")).expect("mkdir");
        std::os::unix::fs::symlink(outside.path(), dir.path().join(".automatic/silent"))
            .expect("link");
        let mut project = core::Project {
            directory: dir.path().display().to_string(),
            mode: core::ProjectMode::Silent,
            ..Default::default()
        };

        assert!(effective_project_dir(&project).is_err());

        project.mode = core::ProjectMode::Normal;
        assert_eq!(
            effective_project_dir(&project).expect("normal mode"),
            dir.path().to_path_buf()
        );
    }
}

#[cfg(test)]
mod identifier_tests {
    use super::*;
    use tempfile::tempdir;

    /// Run `test` with a private Automatic home and one registered project,
    /// `site`, that has no folder. Passes the project's `local_key`.
    fn with_site(test: impl FnOnce(&str)) {
        let home = tempdir().expect("temp home");
        crate::core::with_test_home(home.path().to_path_buf(), || {
            let mut project = core::Project {
                name: "site".into(),
                ..Default::default()
            };
            core::fill_missing_project_keys(&mut project);
            core::save_project("site", &serde_json::to_string(&project).unwrap())
                .expect("save project");
            test(&project.local_key);
        });
    }

    /// Stored `project` values of every activity row, newest first.
    fn activity_projects() -> Vec<String> {
        let entries = crate::activity::get_all_activity(100).expect("activity");
        entries.into_iter().map(|e| e.project).collect()
    }

    fn site_id() -> String {
        let project: core::Project =
            serde_json::from_str(&core::read_project("site").unwrap()).unwrap();
        project.id
    }

    /// Register a second checkout of `site`: same `id`, its own `local_key`.
    fn add_checkout(name: &str) -> String {
        let project = core::Project {
            name: name.into(),
            id: site_id(),
            local_key: core::new_project_key(),
            ..Default::default()
        };
        core::save_project(name, &serde_json::to_string(&project).unwrap()).expect("save checkout");
        project.local_key
    }

    #[test]
    fn projects_command_given_a_key_logs_activity_under_the_key() {
        with_site(|key| {
            adopt_stale_skill(key, "my-skill").expect("adopt by key");

            let project: core::Project =
                serde_json::from_str(&core::read_project("site").unwrap()).unwrap();
            assert_eq!(project.skills, vec!["my-skill".to_string()]);
            assert_eq!(activity_projects(), vec![key.to_string()], "stored by local_key");

            // The activity command takes a key or a name, and shows rows by name.
            for ident in [key, "site"] {
                let raw = super::super::activity::get_project_activity(ident, 0).expect("by ident");
                let rows: Vec<crate::activity::ActivityEntry> = serde_json::from_str(&raw).unwrap();
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].project, "site");
                assert_eq!(rows[0].local_key.as_deref(), Some(key));
            }
            assert_eq!(super::super::activity::get_project_activity_count("site").unwrap(), 1);
        });
    }

    #[test]
    fn activity_rows_for_an_orphan_carry_no_key() {
        with_site(|_| {
            crate::activity::log("ghost", crate::activity::ActivityEvent::ProjectUpdated, "x", "");
            let raw = super::super::activity::get_all_activity(0).expect("all");
            let rows: Vec<crate::activity::ActivityEntry> = serde_json::from_str(&raw).unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].project, "ghost", "an orphan row is returned unchanged");
            assert_eq!(rows[0].local_key, None);
            assert!(!raw.contains("local_key"), "None is not serialised");
        });
    }

    #[test]
    fn memory_command_given_a_name_or_key_stores_under_the_id() {
        with_site(|key| {
            let id = site_id();
            super::super::memory::store_memory(key, "by-key", "v", None).expect("store by key");
            super::super::memory::store_memory("site", "by-name", "v", None).expect("store by name");
            let stored = crate::memory::get_all_memories(&id).unwrap();
            assert!(stored.contains_key("by-key") && stored.contains_key("by-name"));
            assert!(crate::memory::get_all_memories("site").unwrap().is_empty());
            assert!(crate::memory::get_all_memories(key).unwrap().is_empty());
            let db = super::super::memory::get_project_memories("site").expect("read by name");
            assert_eq!(db.len(), 2);
            assert_eq!(activity_projects(), vec![key.to_string(), key.to_string()]);
        });
    }

    #[test]
    fn checkouts_share_memory_and_features_but_not_activity() {
        with_site(|key| {
            let worktree_key = add_checkout("site-wt");
            super::super::memory::store_memory("site-wt", "k", "v", None).expect("store");
            let db = super::super::memory::get_project_memories("site").expect("read");
            assert!(db.contains_key("k"), "the other checkout sees the memory");

            crate::plugins::build::commands::create_feature(
                "site-wt", "shared", None, None, None, None, None, None, None, None,
            )
            .expect("create");
            let listed = crate::plugins::build::commands::list_features("site", None, None).unwrap();
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0].project, "site", "shown by the calling checkout's name");

            let raw = super::super::activity::get_project_activity("site", 0).unwrap();
            assert_eq!(raw, "[]", "activity stays with the checkout that acted");
            assert_eq!(activity_projects(), vec![worktree_key.clone(), worktree_key]);
            let _ = key;
        });
    }

    #[test]
    fn recommendations_are_stored_by_key_and_shown_by_name() {
        with_site(|key| {
            let params = |project: &str| crate::recommendations::AddRecommendationParams {
                project: project.into(),
                kind: "skill".into(),
                title: format!("t-{project}"),
                body: String::new(),
                priority: crate::recommendations::RecommendationPriority::Normal,
                source: "test".into(),
                metadata: String::new(),
            };
            super::super::recommendations::add_recommendation(params("site")).expect("add by name");
            super::super::recommendations::add_recommendation(params("ghost")).expect("add orphan");

            let stored = crate::recommendations::list_recommendations(
                key,
                crate::recommendations::ListRecommendationsFilter {
                    status: None,
                    kind: None,
                    source: None,
                    limit: None,
                },
            )
            .unwrap();
            assert_eq!(stored.len(), 1, "stored under the local_key");
            assert_eq!(stored[0].project, key);

            let shown = super::super::recommendations::list_all_pending_recommendations(None).unwrap();
            let site = shown.iter().find(|r| r.title == "t-site").unwrap();
            assert_eq!(site.project, "site");
            assert_eq!(site.local_key.as_deref(), Some(key));
            let ghost = shown.iter().find(|r| r.title == "t-ghost").unwrap();
            assert_eq!(ghost.project, "ghost");
            assert_eq!(ghost.local_key, None);
        });
    }

    #[test]
    fn memory_command_with_an_unknown_identifier_behaves_as_before() {
        with_site(|_| {
            super::super::memory::store_memory("orphan", "k", "v", None).expect("store");
            assert!(crate::memory::get_all_memories("orphan").unwrap().contains_key("k"));
            assert!(crate::memory::get_all_memories(&site_id()).unwrap().is_empty());
        });
    }

    #[test]
    fn feature_command_given_a_name_creates_under_the_id() {
        with_site(|key| {
            let id = site_id();
            let feature = crate::plugins::build::commands::create_feature(
                "site", "via name", None, None, None, None, None, None, None, None,
            )
            .expect("create by name");
            assert_eq!(feature.project, "site", "the row is named, not keyed");
            let stored = crate::plugins::build::features::list_features(
                &core::ProjectStoreKeys::unregistered(&id),
                None,
                false,
            )
            .expect("list by id");
            assert_eq!(stored.len(), 1);
            assert!(crate::plugins::build::features::list_features(
                &core::ProjectStoreKeys::unregistered("site"),
                None,
                false
            )
            .expect("list by name")
            .is_empty());
            let by_key = crate::plugins::build::commands::list_features(key, None, None).unwrap();
            assert_eq!(by_key.len(), 1);
        });
    }

    #[test]
    fn groups_store_ids_and_show_every_checkout_by_key() {
        with_site(|key| {
            let group = serde_json::json!({"name": "g", "projects": [key, "ghost", site_id()]});
            core::save_group("g", &group.to_string()).expect("save group");
            let raw: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(core::get_groups_dir().unwrap().join("g.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(
                raw["projects"],
                serde_json::json!([site_id(), "ghost"]),
                "a key becomes its id, an id stays, duplicates collapse, unknown values stay"
            );

            let worktree_key = add_checkout("site-wt");
            let read: core::ProjectGroup =
                serde_json::from_str(&super::super::groups::read_group("g").unwrap()).unwrap();
            assert_eq!(read.projects, vec![key.to_string(), worktree_key, "ghost".to_string()]);

            // Saving what was read stores the same ids.
            core::save_group("g", &serde_json::to_string(&read).unwrap()).expect("round trip");
            let raw: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(core::get_groups_dir().unwrap().join("g.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(raw["projects"], serde_json::json!([site_id(), "ghost"]));

            assert_eq!(super::super::groups::groups_for_project(key).unwrap(), vec!["g"]);
            assert_eq!(super::super::groups::groups_for_project("site-wt").unwrap(), vec!["g"]);
            assert!(super::super::groups::groups_for_project("orphan").unwrap().is_empty());
        });
    }

    #[test]
    fn rename_keeps_memory_features_activity_and_groups() {
        with_site(|key| {
            super::super::memory::store_memory("site", "k", "v", None).unwrap();
            crate::plugins::build::commands::create_feature(
                "site", "f", None, None, None, None, None, None, None, None,
            )
            .unwrap();
            core::save_group("g", &serde_json::json!({"name": "g", "projects": ["site"]}).to_string())
                .unwrap();

            rename_project("site", "renamed").expect("rename");

            assert!(super::super::memory::get_project_memories("renamed").unwrap().contains_key("k"));
            let features = crate::plugins::build::commands::list_features("renamed", None, None).unwrap();
            assert_eq!(features.len(), 1);
            assert_eq!(features[0].project, "renamed");
            let raw = super::super::activity::get_project_activity("renamed", 0).unwrap();
            let rows: Vec<crate::activity::ActivityEntry> = serde_json::from_str(&raw).unwrap();
            assert!(rows.iter().all(|r| r.project == "renamed"));
            assert!(rows.iter().any(|r| r.label == "Project renamed"));
            assert!(activity_projects().iter().all(|p| p == key), "every row is under the key");
            assert_eq!(super::super::groups::groups_for_project("renamed").unwrap(), vec!["g"]);
        });
    }

    #[test]
    fn delete_keeps_data_and_drops_the_group_id_with_the_last_checkout() {
        with_site(|key| {
            let id = site_id();
            super::super::memory::store_memory("site", "k", "v", None).unwrap();
            core::save_group("g", &serde_json::json!({"name": "g", "projects": [key]}).to_string())
                .unwrap();
            let worktree_key = add_checkout("site-wt");
            let dev_dir = core::get_automatic_dir().unwrap().join("dev-servers");
            std::fs::create_dir_all(&dev_dir).unwrap();
            std::fs::write(dev_dir.join(format!("{key}.json")), "[]").unwrap();

            delete_project(key).expect("delete by key");
            assert_eq!(core::list_projects().unwrap(), vec!["site-wt"]);
            assert!(crate::memory::get_all_memories(&id).unwrap().contains_key("k"), "memory kept");
            assert!(!dev_dir.join(format!("{key}.json")).exists(), "dev servers removed");
            let read: core::ProjectGroup =
                serde_json::from_str(&super::super::groups::read_group("g").unwrap()).unwrap();
            assert_eq!(read.projects, vec![worktree_key.clone()], "another checkout keeps the id in the group");

            delete_project(&worktree_key).expect("delete the last checkout");
            let read: core::ProjectGroup =
                serde_json::from_str(&super::super::groups::read_group("g").unwrap()).unwrap();
            assert!(read.projects.is_empty(), "the last checkout takes the id with it");
            assert!(crate::memory::get_all_memories(&id).unwrap().contains_key("k"), "memory kept");
        });
    }

    #[test]
    fn registry_commands_reject_an_unknown_identifier_as_before() {
        with_site(|_| {
            assert_eq!(
                check_project_drift("nope").unwrap_err(),
                "Project 'nope' not found"
            );
        });
    }

    #[test]
    fn get_project_summaries_command_returns_keys() {
        with_site(|key| {
            let summaries = get_project_summaries().expect("summaries");
            assert_eq!(summaries.len(), 1);
            assert_eq!(summaries[0].name, "site");
            assert_eq!(summaries[0].local_key, key);
        });
    }

    #[test]
    fn hook_and_profile_refs_carry_the_key() {
        with_site(|key| {
            let mut project: core::Project =
                serde_json::from_str(&core::read_project("site").unwrap()).unwrap();
            project.hooks.push("h".into());
            project.profiles.push("p".into());
            core::save_project("site", &serde_json::to_string(&project).unwrap()).unwrap();

            let hooks = super::super::hooks::get_projects_referencing_hook("h").unwrap();
            assert_eq!(hooks[0].name, "site");
            assert_eq!(hooks[0].local_key.as_deref(), Some(key));
            let profiles =
                super::super::project_profiles::get_projects_referencing_profile("p").unwrap();
            assert_eq!(profiles[0].name, "site");
            assert_eq!(profiles[0].local_key.as_deref(), Some(key));
        });
    }
}

#[cfg(test)]
mod read_only_tests {
    use super::propagation_tests::{make_project, with_temp_home};
    use super::*;
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::time::SystemTime;

    /// Every path under `dir`, with each file's bytes and modified time.
    /// Symlinks are recorded by target, not followed. A rewrite with the
    /// same content still shows as a changed modified time.
    fn snapshot(dir: &Path) -> BTreeMap<PathBuf, (Vec<u8>, Option<SystemTime>)> {
        fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, (Vec<u8>, Option<SystemTime>)>) {
            for entry in std::fs::read_dir(dir).expect("read_dir").flatten() {
                let path = entry.path();
                let rel = path.strip_prefix(root).unwrap().to_path_buf();
                let meta = std::fs::symlink_metadata(&path).expect("metadata");
                if meta.file_type().is_symlink() {
                    let target = std::fs::read_link(&path).expect("read_link");
                    out.insert(rel, (target.display().to_string().into_bytes(), None));
                } else if meta.is_dir() {
                    out.insert(rel, (Vec::new(), None));
                    walk(root, &path, out);
                } else {
                    let bytes = std::fs::read(&path).expect("read");
                    out.insert(rel, (bytes, meta.modified().ok()));
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(dir, dir, &mut out);
        out
    }

    /// The promise of read-only mode: once it is on, nothing Automatic runs
    /// changes the project folder. The folder is first synced, then made
    /// stale, so every path below would write if it were not guarded.
    #[test]
    fn read_only_project_folder_is_never_written() {
        with_temp_home(|_| {
            core::save_rule("test-rule", "Test Rule", "Rule body.\n").expect("save rule");
            let (_keep, dir) = make_project("ro", |p| {
                p.file_rules
                    .insert("_unified".to_string(), vec!["test-rule".to_string()]);
            });
            sync_project("ro").expect("initial sync");
            core::set_project_read_only("ro", true).expect("turn on read only");

            // Make the folder stale: a sync would put the instruction file
            // back, and a refresh would re-sort the config.
            for name in ["CLAUDE.md", "AGENTS.md"] {
                let _ = std::fs::remove_file(dir.join(name));
            }
            let config = dir.join(core::CONFIG_FILE_NAME);
            std::fs::write(&config, r#"{"name":"ro","agents":["claude"]}"#).expect("write config");
            let before = snapshot(&dir);

            // Reads and checks.
            let loaded: core::Project =
                serde_json::from_str(&core::read_project("ro").expect("read")).expect("parse");
            assert!(loaded.local_read_only);
            core::refresh_all_project_files().expect("refresh");
            let _ = check_project_drift("ro");
            let _ = check_project_problems("ro");

            // Background fan-outs.
            core::save_rule("test-rule", "Test Rule", "Changed body.\n").expect("edit rule");
            sync_projects_referencing_rule("test-rule", &SweepProgress::silent());
            resync_all_projects(&SweepProgress::silent());
            reconcile_plugin_resources_in_projects();
            prune_rule_from_projects("test-rule", &SweepProgress::silent());

            // Direct writes are refused with the read-only error.
            let refused = |result: Result<(), String>, what: &str| {
                let err = result.expect_err(what);
                assert!(err.contains("read only"), "{}: {}", what, err);
            };
            refused(sync_project("ro").map(|_| ()), "sync");
            refused(rebuild_project("ro").map(|_| ()), "rebuild");
            refused(
                save_project("ro", &serde_json::to_string(&loaded).unwrap(), None).map(|_| ()),
                "save",
            );
            refused(
                crate::commands::save_project_file("ro", "CLAUDE.md", "edited"),
                "instruction file",
            );
            refused(crate::commands::save_doc_note("ro", "note", "text"), "doc note");

            assert_eq!(snapshot(&dir), before, "the folder is unchanged");

            // Renaming and removing it leave the folder alone too.
            core::rename_project("ro", "ro-renamed").expect("rename");
            core::delete_project("ro-renamed").expect("delete");
            assert_eq!(snapshot(&dir), before, "rename and delete leave the folder alone");
        });
    }

    /// Control for the test above: the same stale folder, left writable, is
    /// rewritten by the startup refresh and by a resync. Without this, a
    /// snapshot that never changed could hide a check that sees nothing.
    #[test]
    fn the_same_steps_write_to_a_writable_project() {
        with_temp_home(|_| {
            let (_keep, dir) = make_project("rw", |_| {});
            sync_project("rw").expect("initial sync");
            let config = dir.join(core::CONFIG_FILE_NAME);
            std::fs::write(&config, r#"{"name":"rw","agents":["claude"]}"#).expect("write config");
            let _ = std::fs::remove_file(dir.join("CLAUDE.md"));
            let before = snapshot(&dir);

            core::refresh_all_project_files().expect("refresh");
            assert_ne!(snapshot(&dir), before, "the refresh rewrites the stale config");

            let before = snapshot(&dir);
            resync_all_projects(&SweepProgress::silent());
            assert_ne!(snapshot(&dir), before, "a resync writes the instruction file back");
        });
    }

    /// A project registered inside another project's folder before that
    /// project became read-only: reads report it as read-only, and the
    /// commands that delete or write files directly refuse it.
    #[test]
    fn a_nested_project_is_read_only_through_its_parent() {
        with_temp_home(|_| {
            let (_keep, outer_dir) = make_project("outer", |_| {});
            let inner_dir = outer_dir.join("api");
            std::fs::create_dir_all(&inner_dir).expect("mkdir");
            let inner = core::Project {
                name: "inner".into(),
                directory: inner_dir.display().to_string(),
                agents: vec!["claude".into()],
                ..Default::default()
            };
            core::save_project("inner", &serde_json::to_string(&inner).unwrap()).expect("save inner");
            sync_project("inner").expect("sync inner");
            std::fs::create_dir_all(inner_dir.join(".agents/skills/stale")).expect("stale skill");

            core::set_project_read_only("outer", true).expect("outer on");
            let loaded: core::Project =
                serde_json::from_str(&core::read_project("inner").unwrap()).unwrap();
            assert_eq!(loaded.read_only_owner.as_deref(), Some("outer"));
            assert!(loaded.is_read_only_here());
            let summary = core::get_project_summaries()
                .unwrap()
                .into_iter()
                .find(|s| s.name == "inner")
                .unwrap();
            assert!(summary.read_only, "the summary reports the inherited state");

            let before = snapshot(&inner_dir);
            let nested_refusal = |result: Result<(), String>, what: &str| {
                let err = result.expect_err(what);
                assert!(err.contains("inside project 'outer'"), "{}: {}", what, err);
            };
            nested_refusal(
                remove_agent_from_project("inner", "claude", "remove").map(|_| ()),
                "remove agent",
            );
            nested_refusal(remove_stale_skill("inner", "stale"), "remove stale skill");
            nested_refusal(crate::commands::save_doc_note("inner", "note", "text"), "doc note");
            resync_all_projects(&SweepProgress::silent());
            core::refresh_all_project_files().expect("refresh");
            assert_eq!(snapshot(&inner_dir), before, "the nested folder is unchanged");
        });
    }

    /// The startup rule migrations skip read-only and nested projects, and
    /// still reach writable ones.
    #[test]
    fn retired_rule_migration_skips_read_only_folders() {
        with_temp_home(|_| {
            core::save_rule("gone-rule", "Gone", "Body.\n").expect("save rule");
            let with_rule = |p: &mut core::Project| {
                p.file_rules
                    .insert("_unified".to_string(), vec!["gone-rule".to_string()]);
            };
            let (_keep_ro, ro_dir) = make_project("ro", with_rule);
            let nested_dir = ro_dir.join("sub");
            std::fs::create_dir_all(&nested_dir).expect("mkdir");
            let mut nested = core::Project {
                name: "nested".into(),
                directory: nested_dir.display().to_string(),
                ..Default::default()
            };
            with_rule(&mut nested);
            core::save_project("nested", &serde_json::to_string(&nested).unwrap()).expect("save nested");
            let (_keep_rw, rw_dir) = make_project("rw", with_rule);
            core::set_project_read_only("ro", true).expect("on");

            let before = snapshot(&ro_dir);
            core::remove_retired_rules(&["gone-rule".to_string()]).expect("migrate");

            assert_eq!(snapshot(&ro_dir), before, "read-only and nested folders are unchanged");
            let rw = std::fs::read_to_string(rw_dir.join(core::CONFIG_FILE_NAME)).unwrap();
            assert!(!rw.contains("gone-rule"), "the writable project is migrated: {}", rw);
        });
    }

    #[test]
    fn a_read_only_project_renames_without_an_error() {
        with_temp_home(|_| {
            let (_keep, dir) = make_project("ro", |_| {});
            core::set_project_read_only("ro", true).expect("on");
            let before = snapshot(&dir);
            rename_project("ro", "renamed").expect("rename reports success");
            assert_eq!(snapshot(&dir), before);
        });
    }

    #[test]
    fn a_project_without_a_folder_cannot_be_made_read_only() {
        with_temp_home(|_| {
            let project = core::Project {
                name: "nowhere".into(),
                ..Default::default()
            };
            core::save_project("nowhere", &serde_json::to_string(&project).unwrap()).expect("save");
            let err = core::set_project_read_only("nowhere", true).expect_err("refused");
            assert!(err.contains("has no folder"), "{}", err);
        });
    }

    #[test]
    fn turning_read_only_off_allows_writes_again() {
        with_temp_home(|_| {
            let (_keep, dir) = make_project("rw", |_| {});
            core::set_project_read_only("rw", true).expect("on");
            assert!(sync_project("rw").is_err());

            set_project_read_only("rw", false).expect("off");
            sync_project("rw").expect("sync after turning read only off");
            assert!(dir.join("CLAUDE.md").exists() || dir.join(".claude").exists());
        });
    }
}

#[cfg(test)]
mod adopt_custom_asset_tests {
    use super::propagation_tests::{make_project, with_temp_home};
    use super::*;
    use crate::agent::{Agent, ClaudeCode};

    const STORED: &str = "---\nname: planner\ndescription: Plans\n---\nStored.\n";

    /// Save a project `name` with one custom agent, `Planner`, stored as
    /// [`STORED`], and the given agents.  Returns the project directory.
    fn planner_project(
        name: &str,
        agents: &[&str],
    ) -> (tempfile::TempDir, std::path::PathBuf) {
        make_project(name, |p| {
            p.agents = agents.iter().map(|a| a.to_string()).collect();
            p.custom_agents = Some(vec![core::CustomAgent {
                name: "Planner".into(),
                content: STORED.into(),
            }]);
        })
    }

    fn write(path: &std::path::Path, content: &str) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, content).expect("write");
    }

    fn stored_planner(name: &str) -> String {
        let raw = core::read_project(name).expect("read project");
        let project: core::Project = serde_json::from_str(&raw).expect("project json");
        project.custom_agents.expect("custom agents")[0].content.clone()
    }

    /// Copilot names sub-agent files `{name}.agent.md`.  Adopting must read
    /// that file, not `{name}.md`.
    #[test]
    fn adopt_reads_copilot_agent_md_file() {
        with_temp_home(|_| {
            let (_keep, dir) = planner_project("copilot-adopt", &["copilot"]);
            let edited = "---\nname: planner\ndescription: Plans\n---\nEdited on disk.\n";
            write(&dir.join(".github/agents/planner.agent.md"), edited);

            adopt_custom_asset("copilot-adopt", "agent", "Planner").expect("adopt");

            assert_eq!(stored_planner("copilot-adopt"), edited);
        });
    }

    /// Adopt takes the file the conflict scan reported.  A Claude copy that
    /// still matches the stored version must not be adopted in place of the
    /// edited Copilot file.
    #[test]
    fn adopt_takes_the_conflicting_file_not_the_first_one() {
        with_temp_home(|_| {
            let (_keep, dir) = planner_project("two-agents", &["claude", "copilot"]);
            write(
                &dir.join(".claude/agents/planner.md"),
                &ClaudeCode.convert_agent_content(STORED, "planner"),
            );
            let edited = "---\nname: planner\ndescription: Plans\n---\nEdited for Copilot.\n";
            write(&dir.join(".github/agents/planner.agent.md"), edited);

            adopt_custom_asset("two-agents", "agent", "Planner").expect("adopt");

            assert_eq!(stored_planner("two-agents"), edited);
        });
    }

    /// Codex stores sub-agents as TOML.  Adopting one must store Markdown,
    /// or the next sync converts TOML as if it were Markdown.
    #[test]
    fn adopt_converts_a_codex_toml_file_to_markdown() {
        with_temp_home(|_| {
            let (_keep, dir) = planner_project("codex-adopt", &["codex"]);
            write(
                &dir.join(".codex/agents/planner.toml"),
                "name = \"planner\"\ndescription = \"Plans\"\ndeveloper_instructions = \"Edited on disk.\"\n",
            );

            adopt_custom_asset("codex-adopt", "agent", "Planner").expect("adopt");

            let stored = stored_planner("codex-adopt");
            assert!(stored.starts_with("---\nname: planner\n"), "{stored}");
            assert!(stored.contains("Edited on disk."), "{stored}");
            assert!(!stored.contains("developer_instructions"), "{stored}");
        });
    }

    /// Kiro's JSON has no conversion back to Markdown, so adopt refuses it
    /// and leaves the stored copy alone.
    #[test]
    fn adopt_refuses_a_format_with_no_reverse_conversion() {
        with_temp_home(|_| {
            let (_keep, dir) = planner_project("kiro-adopt", &["kiro"]);
            write(&dir.join(".kiro/agents/planner.json"), "{\"name\":\"planner\"}\n");

            let err = adopt_custom_asset("kiro-adopt", "agent", "Planner").unwrap_err();

            assert!(err.contains("cannot convert"), "{err}");
            assert_eq!(stored_planner("kiro-adopt"), STORED);
        });
    }

    /// Whatever adopt accepts must leave no conflict for the next scan.
    fn assert_no_conflict(name: &str, dir: &std::path::Path) {
        let raw = core::read_project(name).expect("read project");
        let project: core::Project = serde_json::from_str(&raw).expect("project json");
        let agents = project.custom_agents.clone().expect("custom agents");
        assert!(
            sync::scan_custom_agent(&project, dir, &agents[0]).is_none(),
            "adopted file still conflicts"
        );
    }

    /// A hand-written file never matches the rendered TOML byte for byte.
    /// With no `sandbox_mode`, adopting must not add one, and the conflict
    /// must end.
    #[test]
    fn adopted_codex_file_leaves_no_conflict() {
        with_temp_home(|_| {
            let (_keep, dir) = planner_project("codex-plain", &["codex"]);
            write(
                &dir.join(".codex/agents/planner.toml"),
                "developer_instructions = \"Edited.\"\nmodel_reasoning_effort = \"high\"\nmax_turns = 5\nmodel = \"gpt-5.4\"\ndescription = \"Plans\"\nname = \"planner\"\n",
            );

            adopt_custom_asset("codex-plain", "agent", "Planner").expect("adopt");

            assert!(!stored_planner("codex-plain").contains("tools:"));
            assert_no_conflict("codex-plain", &dir);
        });
    }

    #[test]
    fn adopted_read_only_codex_file_leaves_no_conflict() {
        with_temp_home(|_| {
            let (_keep, dir) = planner_project("codex-read-only", &["codex"]);
            write(
                &dir.join(".codex/agents/planner.toml"),
                "sandbox_mode = \"read-only\"\ndeveloper_instructions = \"Edited.\"\nname = \"planner\"\n",
            );

            adopt_custom_asset("codex-read-only", "agent", "Planner").expect("adopt");

            assert!(stored_planner("codex-read-only").contains("tools: inherit\n"));
            assert_no_conflict("codex-read-only", &dir);
        });
    }

    /// The scan compares fields and drift compares bytes.  After adopting, a
    /// formatting difference is no longer a conflict but still shows as a
    /// modified file, until sync rewrites it with every field kept.
    #[test]
    fn adopted_codex_file_shows_drift_until_sync_rewrites_it() {
        with_temp_home(|_| {
            let (_keep, dir) = planner_project("codex-drift", &["codex"]);
            let path = dir.join(".codex/agents/planner.toml");
            write(
                &path,
                "max_turns = 5\ndeveloper_instructions = \"Edited.\"\nname = \"planner\"\n",
            );
            adopt_custom_asset("codex-drift", "agent", "Planner").expect("adopt");

            let is_planner = |file: &sync::DriftedFile| file.path.ends_with("planner.toml");
            let drift_of = |name: &str| {
                let raw = core::read_project(name).expect("read project");
                let project: core::Project = serde_json::from_str(&raw).expect("project json");
                sync::check_project_drift(&project).expect("drift")
            };
            let before = drift_of("codex-drift");
            assert!(before.custom_conflicts.is_empty(), "{:?}", before.custom_conflicts);
            assert!(before.agents.iter().flat_map(|a| &a.files).any(is_planner));

            sync_project("codex-drift").expect("sync");

            let after = drift_of("codex-drift");
            assert!(!after.agents.iter().flat_map(|a| &a.files).any(is_planner));
            let rewritten = std::fs::read_to_string(&path).expect("read");
            assert!(rewritten.contains("max_turns = 5"), "{rewritten}");
            assert!(rewritten.contains("Edited."), "{rewritten}");
        });
    }

    /// Sync would delete a comment, so it is a conflict sync leaves alone.
    /// Adopting cannot keep it either, so it is refused and both copies stay.
    #[test]
    fn comment_in_a_codex_file_is_a_conflict_adopt_refuses_and_sync_keeps() {
        with_temp_home(|_| {
            let (_keep, dir) = planner_project("codex-comment", &["codex"]);
            let path = dir.join(".codex/agents/planner.toml");
            let rendered = crate::agent::CodexCli.convert_agent_content(STORED, "planner");
            let disk = format!("# hand note\n{rendered}");
            write(&path, &disk);

            let err = adopt_custom_asset("codex-comment", "agent", "Planner").unwrap_err();
            assert!(err.contains("changes: comments."), "{err}");
            assert_eq!(stored_planner("codex-comment"), STORED);

            sync_project("codex-comment").expect("sync");
            assert_eq!(std::fs::read_to_string(&path).expect("read"), disk);
        });
    }

    /// A setting Markdown cannot hold is refused, with the conflict left
    /// open and both copies untouched.
    #[test]
    fn adopt_refuses_codex_settings_it_cannot_keep() {
        for (key, line) in [
            ("sandbox_mode", "sandbox_mode = \"workspace-write\""),
            ("custom_flag", "custom_flag = true"),
        ] {
            with_temp_home(|_| {
                let (_keep, dir) = planner_project("codex-lossy", &["codex"]);
                let path = dir.join(".codex/agents/planner.toml");
                let disk = format!("{line}\nname = \"planner\"\ndeveloper_instructions = \"Edited.\"\n");
                write(&path, &disk);

                let err = adopt_custom_asset("codex-lossy", "agent", "Planner").unwrap_err();

                assert!(err.contains(&format!("changes: {key}.")), "{err}");
                assert!(err.contains("Codex"), "{err}");
                assert_eq!(stored_planner("codex-lossy"), STORED);
                assert_eq!(std::fs::read_to_string(&path).expect("read"), disk);
            });
        }
    }

    /// A Markdown file the user wrote has no `automatic-managed` marker.
    /// Sync adds one, so the missing marker must not keep the conflict open.
    #[test]
    fn adopted_markdown_file_without_a_marker_leaves_no_conflict() {
        with_temp_home(|_| {
            let (_keep, dir) = planner_project("markdown-adopt", &["claude"]);
            let edited = "---\nname: planner\ndescription: Plans\n---\nEdited on disk.\n";
            write(&dir.join(".claude/agents/planner.md"), edited);

            adopt_custom_asset("markdown-adopt", "agent", "Planner").expect("adopt");

            assert_eq!(stored_planner("markdown-adopt"), edited);
            assert_no_conflict("markdown-adopt", &dir);
        });
    }

    /// A body with a backslash and `"""` adopts, leaves no conflict, and
    /// the file the next sync writes still holds the same text.
    #[test]
    fn adopted_codex_file_with_special_characters_leaves_no_conflict() {
        with_temp_home(|_| {
            let (_keep, dir) = planner_project("codex-escapes", &["codex"]);
            let path = dir.join(".codex/agents/planner.toml");
            let body = "Match \\d and \"\"\"quoted\"\"\".";
            write(
                &path,
                &format!("name = \"planner\"\ndeveloper_instructions = '''{body}'''\n"),
            );

            adopt_custom_asset("codex-escapes", "agent", "Planner").expect("adopt");

            assert!(stored_planner("codex-escapes").contains(body));
            assert_no_conflict("codex-escapes", &dir);

            sync_project("codex-escapes").expect("sync");
            let rewritten = std::fs::read_to_string(&path).expect("read");
            let table = toml::from_str::<toml::Table>(&rewritten).expect("valid TOML");
            let instructions = table["developer_instructions"].as_str().expect("string");
            assert_eq!(instructions.trim(), body);
        });
    }

    /// An older version wrote `\d` unescaped, which Codex cannot load.
    /// After upgrading, that file differs from the new render and does not
    /// parse: it is a conflict, adopt refuses it, and overwrite repairs it.
    #[test]
    fn older_unparseable_codex_file_is_a_conflict_that_overwrite_repairs() {
        with_temp_home(|_| {
            let stored = "---\nname: planner\ndescription: Plans\n---\nMatch \\d.\n";
            let (_keep, dir) = make_project("codex-older", |p| {
                p.agents = vec!["codex".into()];
                p.custom_agents = Some(vec![core::CustomAgent {
                    name: "Planner".into(),
                    content: stored.into(),
                }]);
            });
            let path = dir.join(".codex/agents/planner.toml");
            let rendered = crate::agent::CodexCli.convert_agent_content(stored, "planner");
            let older = rendered.replace("\\\\d", "\\d");
            assert!(toml::from_str::<toml::Table>(&older).is_err(), "{older}");
            write(&path, &older);

            let raw = core::read_project("codex-older").expect("read project");
            let project: core::Project = serde_json::from_str(&raw).expect("project json");
            let agents = project.custom_agents.clone().expect("custom agents");
            assert!(matches!(
                sync::scan_custom_agent(&project, &dir, &agents[0]),
                Some(sync::CustomAgentFinding::Conflict { .. })
            ));

            let err = adopt_custom_asset("codex-older", "agent", "Planner").unwrap_err();
            assert!(err.contains("cannot convert"), "{err}");
            assert_eq!(stored_planner("codex-older"), stored);
            assert_eq!(std::fs::read_to_string(&path).expect("read"), older);

            overwrite_custom_asset("codex-older", "agent", "Planner").expect("overwrite");

            let repaired = std::fs::read_to_string(&path).expect("read");
            let table = toml::from_str::<toml::Table>(&repaired).expect("valid TOML");
            let instructions = table["developer_instructions"].as_str().expect("string");
            assert_eq!(instructions.trim(), "Match \\d.");
            assert_no_conflict("codex-older", &dir);
        });
    }

    /// Save a project whose stored Codex agent has `maxTurns: many`.  The
    /// render leaves that value out.
    fn bad_max_turns_project(name: &str) -> (tempfile::TempDir, std::path::PathBuf, &'static str) {
        let stored = "---\nname: planner\ndescription: Plans\nmaxTurns: many\n---\nPlan.\n";
        let (keep, dir) = make_project(name, |p| {
            p.agents = vec!["codex".into()];
            p.custom_agents = Some(vec![core::CustomAgent {
                name: "Planner".into(),
                content: stored.into(),
            }]);
        });
        (keep, dir, stored)
    }

    /// The render drops a bad `maxTurns`, so a file sync wrote matches the
    /// stored copy.  The stored Markdown keeps the bad value.
    #[test]
    fn bad_stored_max_turns_is_dropped_from_the_render_without_a_conflict() {
        with_temp_home(|_| {
            let (_keep, dir, stored) = bad_max_turns_project("turns-render");

            sync_project("turns-render").expect("sync");

            let rendered =
                std::fs::read_to_string(dir.join(".codex/agents/planner.toml")).expect("read");
            assert!(!rendered.contains("max_turns"), "{rendered}");
            assert!(toml::from_str::<toml::Table>(&rendered).is_ok(), "{rendered}");
            assert_eq!(stored_planner("turns-render"), stored);
            assert_no_conflict("turns-render", &dir);
        });
    }

    /// A valid `max_turns` on disk replaces the bad stored value.
    #[test]
    fn adopt_replaces_a_bad_stored_max_turns_with_a_valid_one() {
        with_temp_home(|_| {
            let (_keep, dir, _) = bad_max_turns_project("turns-valid");
            write(
                &dir.join(".codex/agents/planner.toml"),
                "name = \"planner\"\nmax_turns = 5\ndeveloper_instructions = \"Plan.\"\n",
            );

            adopt_custom_asset("turns-valid", "agent", "Planner").expect("adopt");

            assert!(stored_planner("turns-valid").contains("maxTurns: 5\n"));
            assert_no_conflict("turns-valid", &dir);
        });
    }

    /// A `max_turns` on disk that is not an integer cannot be kept, so the
    /// guard refuses it rather than dropping it.
    #[test]
    fn adopt_refuses_a_non_integer_max_turns_on_disk() {
        with_temp_home(|_| {
            let (_keep, dir) = planner_project("turns-refused", &["codex"]);
            let path = dir.join(".codex/agents/planner.toml");
            let disk = "name = \"planner\"\nmax_turns = \"many\"\ndeveloper_instructions = \"Edited.\"\n";
            write(&path, disk);

            let err = adopt_custom_asset("turns-refused", "agent", "Planner").unwrap_err();

            assert!(err.contains("changes: max_turns."), "{err}");
            assert_eq!(stored_planner("turns-refused"), STORED);
            assert_eq!(std::fs::read_to_string(&path).expect("read"), disk);
        });
    }

    /// An older version wrote `max_turns = many`, which does not parse.  It
    /// is a conflict, adopt refuses it, and overwrite repairs it.
    #[test]
    fn older_raw_max_turns_is_a_conflict_that_overwrite_repairs() {
        with_temp_home(|_| {
            let (_keep, dir, stored) = bad_max_turns_project("turns-older");
            let path = dir.join(".codex/agents/planner.toml");
            let older = "automatic_managed = true\nname = \"planner\"\ndescription = \"Plans\"\nmax_turns = many\n\ndeveloper_instructions = \"\"\"\nPlan.\n\"\"\"\n";
            assert!(toml::from_str::<toml::Table>(older).is_err());
            write(&path, older);

            let err = adopt_custom_asset("turns-older", "agent", "Planner").unwrap_err();
            assert!(err.contains("cannot convert"), "{err}");
            assert_eq!(stored_planner("turns-older"), stored);

            overwrite_custom_asset("turns-older", "agent", "Planner").expect("overwrite");

            let repaired = std::fs::read_to_string(&path).expect("read");
            assert!(toml::from_str::<toml::Table>(&repaired).is_ok(), "{repaired}");
            assert_no_conflict("turns-older", &dir);
        });
    }

    /// Keys only Claude reads, and a model and tool list Codex cannot hold.
    const SHARED: &str = "---\nname: planner\ndescription: Plans\nmodel: sonnet\ntools: Read, Grep\ndisallowedTools: Bash\npermissionMode: plan\ncolor: blue\n---\n\nStored body.\n";

    fn shared_project(name: &str, agents: &[&str]) -> (tempfile::TempDir, std::path::PathBuf) {
        make_project(name, |p| {
            p.agents = agents.iter().map(|a| a.to_string()).collect();
            p.custom_agents = Some(vec![core::CustomAgent {
                name: "Planner".into(),
                content: SHARED.into(),
            }]);
        })
    }

    fn codex_render(content: &str) -> String {
        crate::agent::CodexCli.convert_agent_content(content, "planner")
    }

    fn claude_render(content: &str) -> String {
        ClaudeCode.convert_agent_content(content, "planner")
    }

    /// Editing the Codex file and adopting it must leave nothing to resolve.
    /// Claude's file follows the new copy and keeps what only Claude reads.
    #[test]
    fn adopting_a_codex_edit_updates_a_matching_claude_file() {
        with_temp_home(|_| {
            let (_keep, dir) = shared_project("both", &["claude", "codex"]);
            let claude = dir.join(".claude/agents/planner.md");
            write(&claude, &claude_render(SHARED));
            write(
                &dir.join(".codex/agents/planner.toml"),
                &codex_render(SHARED).replace("Stored body.", "Edited body."),
            );

            adopt_custom_asset("both", "agent", "Planner").expect("adopt");

            let expected = SHARED.replace("Stored body.", "Edited body.");
            assert_eq!(stored_planner("both"), expected);
            assert_eq!(
                std::fs::read_to_string(&claude).expect("read"),
                claude_render(&expected)
            );
            assert_no_conflict("both", &dir);
        });
    }

    /// A Claude file the user really changed is their decision to make, so
    /// adopting the Codex edit must not overwrite it.
    #[test]
    fn adopting_a_codex_edit_leaves_a_differing_claude_file_as_a_conflict() {
        with_temp_home(|_| {
            let (_keep, dir) = shared_project("differs", &["codex", "claude"]);
            let claude = dir.join(".claude/agents/planner.md");
            let edited = claude_render(SHARED).replace("Stored body.", "Claude edit.");
            write(&claude, &edited);
            write(
                &dir.join(".codex/agents/planner.toml"),
                &codex_render(SHARED).replace("Stored body.", "Edited body."),
            );

            adopt_custom_asset("differs", "agent", "Planner").expect("adopt");

            assert_eq!(std::fs::read_to_string(&claude).expect("read"), edited);
            let raw = core::read_project("differs").expect("read project");
            let project: core::Project = serde_json::from_str(&raw).expect("project json");
            let agents = project.custom_agents.clone().expect("custom agents");
            let Some(sync::CustomAgentFinding::Conflict { conflict, .. }) =
                sync::scan_custom_agent(&project, &dir, &agents[0])
            else {
                panic!("the differing Claude file must stay a conflict");
            };
            assert_eq!(conflict.path, ".claude/agents/planner.md");
        });
    }

    /// Before this, taking Claude's file after a Codex adopt reverted it.
    /// A Markdown adopt keeps the other agents in step the same way.
    #[test]
    fn adopting_a_markdown_edit_updates_a_matching_codex_file() {
        with_temp_home(|_| {
            let (_keep, dir) = shared_project("markdown-first", &["claude", "codex"]);
            let codex = dir.join(".codex/agents/planner.toml");
            write(&codex, &codex_render(SHARED));
            let edited = SHARED.replace("Stored body.", "Edited in Claude.");
            write(&dir.join(".claude/agents/planner.md"), &edited);

            adopt_custom_asset("markdown-first", "agent", "Planner").expect("adopt");

            assert_eq!(stored_planner("markdown-first"), edited);
            assert_eq!(
                std::fs::read_to_string(&codex).expect("read"),
                codex_render(&edited)
            );
            assert_no_conflict("markdown-first", &dir);
        });
    }

    #[test]
    fn adopt_refuses_a_shared_model_edit_and_names_the_way_out() {
        with_temp_home(|_| {
            let (_keep, dir) = shared_project("shared-model", &["claude", "codex"]);
            let claude = dir.join(".claude/agents/planner.md");
            write(&claude, &claude_render(SHARED));
            let codex = dir.join(".codex/agents/planner.toml");
            let disk = codex_render(SHARED).replace("gpt-5.4", "gpt-5.5");
            write(&codex, &disk);

            let err = adopt_custom_asset("shared-model", "agent", "Planner").unwrap_err();

            assert!(err.contains("stores one model for every agent"), "{err}");
            assert!(err.contains("Overwrite"), "{err}");
            assert_eq!(stored_planner("shared-model"), SHARED);
            assert_eq!(std::fs::read_to_string(&codex).expect("read"), disk);
            assert_eq!(
                std::fs::read_to_string(&claude).expect("read"),
                claude_render(SHARED)
            );
        });
    }

    /// An agent with no `agents_dir` reads nothing from the stored copy, so
    /// it does not count as sharing it.
    #[test]
    fn an_agent_without_sub_agents_does_not_share_the_copy() {
        with_temp_home(|_| {
            let without = crate::agent::all()
                .into_iter()
                .find(|a| a.agents_dir(std::path::Path::new("x")).is_none())
                .expect("an agent with no sub-agents")
                .id();
            let (_keep, dir) = shared_project("alone", &["codex", without]);
            write(
                &dir.join(".codex/agents/planner.toml"),
                &codex_render(SHARED).replace("gpt-5.4", "gpt-5.5"),
            );

            adopt_custom_asset("alone", "agent", "Planner").expect("adopt");

            assert!(stored_planner("alone").contains("model: gpt-5.5\n"));
        });
    }

    /// A file that cannot be updated must stop the adopt before anything
    /// changes, or the copies end up out of step.  Fixing it and adopting
    /// again finishes the job.
    #[cfg(unix)]
    #[test]
    fn a_blocked_file_refuses_the_adopt_with_nothing_changed() {
        use std::os::unix::fs::PermissionsExt;
        with_temp_home(|_| {
            let (_keep, dir) = shared_project("locked", &["claude", "copilot", "codex"]);
            let claude = dir.join(".claude/agents/planner.md");
            write(&claude, &claude_render(SHARED));
            let copilot = dir.join(".github/agents/planner.agent.md");
            write(&copilot, &claude_render(SHARED));
            let codex = dir.join(".codex/agents/planner.toml");
            let edited = codex_render(SHARED).replace("Stored body.", "Edited body.");
            write(&codex, &edited);
            std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o444))
                .expect("chmod");

            let err = adopt_custom_asset("locked", "agent", "Planner").unwrap_err();

            assert!(err.contains(".claude/agents/planner.md"), "{err}");
            assert!(err.contains("Nothing was changed."), "{err}");
            assert_eq!(stored_planner("locked"), SHARED);
            assert_eq!(
                std::fs::read_to_string(&copilot).expect("read"),
                claude_render(SHARED)
            );
            assert_eq!(std::fs::read_to_string(&codex).expect("read"), edited);

            std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o644))
                .expect("chmod");
            adopt_custom_asset("locked", "agent", "Planner").expect("adopt after the fix");

            assert!(stored_planner("locked").contains("Edited body."));
            assert!(std::fs::read_to_string(&claude)
                .expect("read")
                .contains("Edited body."));
            assert!(std::fs::read_to_string(&copilot)
                .expect("read")
                .contains("Edited body."));
            assert_no_conflict("locked", &dir);
        });
    }

    #[test]
    fn the_activity_line_says_which_files_were_missed() {
        assert_eq!(
            adopt_activity_summary("agent", &Ok(())),
            "Custom agent adopted from disk"
        );
        assert_eq!(
            adopt_activity_summary("agent", &Err(vec!["one".into(), "two".into()])),
            "Custom agent adopted from disk, but not written to: one; two"
        );
    }

    /// Windows editors re-save with CRLF.  With no real edit there is nothing
    /// to resolve.
    #[test]
    fn a_crlf_resave_of_a_codex_file_is_no_conflict() {
        with_temp_home(|_| {
            let stored = SHARED.replace("Stored body.", "One.\nTwo.");
            let (_keep, dir) = make_project("crlf", |p| {
                p.agents = vec!["codex".into()];
                p.custom_agents = Some(vec![core::CustomAgent {
                    name: "Planner".into(),
                    content: stored.clone(),
                }]);
            });
            write(
                &dir.join(".codex/agents/planner.toml"),
                &codex_render(&stored).replace('\n', "\r\n"),
            );

            assert_no_conflict("crlf", &dir);
        });
    }

    /// Silent mode keeps agent files under `.automatic/silent`.  The rewrite
    /// must land there, contained by that directory.
    #[test]
    fn adopt_rewrites_other_files_inside_the_silent_directory() {
        with_temp_home(|_| {
            let (_keep, dir) = make_project("silent", |p| {
                p.mode = core::ProjectMode::Silent;
                p.agents = vec!["claude".into(), "codex".into()];
                p.custom_agents = Some(vec![core::CustomAgent {
                    name: "Planner".into(),
                    content: SHARED.into(),
                }]);
            });
            let silent = dir.join(".automatic/silent");
            let claude = silent.join(".claude/agents/planner.md");
            write(&claude, &claude_render(SHARED));
            write(
                &silent.join(".codex/agents/planner.toml"),
                &codex_render(SHARED).replace("Stored body.", "Edited body."),
            );

            adopt_custom_asset("silent", "agent", "Planner").expect("adopt");

            assert!(std::fs::read_to_string(&claude)
                .expect("read")
                .contains("Edited body."));
            assert!(!dir.join(".claude").exists());
            assert_no_conflict("silent", &silent);
        });
    }

    #[test]
    fn adopt_refuses_a_rename_and_changes_nothing() {
        with_temp_home(|_| {
            let (_keep, dir) = shared_project("rename", &["claude", "codex"]);
            let claude = dir.join(".claude/agents/planner.md");
            let edited = claude_render(SHARED).replace("name: planner", "name: reviewer");
            write(&claude, &edited);
            let codex = dir.join(".codex/agents/planner.toml");
            write(&codex, &codex_render(SHARED));

            let err = adopt_custom_asset("rename", "agent", "Planner").unwrap_err();

            assert!(err.contains("Adopting cannot rename an agent."), "{err}");
            assert_eq!(stored_planner("rename"), SHARED);
            assert_eq!(std::fs::read_to_string(&claude).expect("read"), edited);
            assert_eq!(
                std::fs::read_to_string(&codex).expect("read"),
                codex_render(SHARED)
            );
        });
    }

    const REFUSED_FILE: &str = "automatic_managed = true\nname = \"planner\"\ndescription = \"a\\u0007\\nb\"\n";

    /// Rebuild cannot read this file, but it holds the only copy of what the
    /// user wrote.  Rebuild keeps the stored agent, and sync keeps the file.
    #[test]
    fn rebuild_and_sync_keep_a_refused_file_with_a_stored_agent() {
        with_temp_home(|_| {
            let (_keep, dir) = planner_project("refused-stored", &["codex"]);
            let path = dir.join(".codex/agents/planner.toml");
            write(&path, REFUSED_FILE);

            rebuild_project("refused-stored").expect("rebuild");
            assert_eq!(stored_planner("refused-stored"), STORED);

            sync_project("refused-stored").expect("sync");
            assert_eq!(std::fs::read_to_string(&path).expect("read"), REFUSED_FILE);
        });
    }

    /// With no stored agent there is nothing to keep in the project, so the
    /// file itself must stay out of sync's stale sweep.
    #[test]
    fn rebuild_and_sync_keep_a_refused_file_with_no_stored_agent() {
        with_temp_home(|_| {
            let (_keep, dir) = make_project("refused-new", |p| {
                p.agents = vec!["codex".into()];
            });
            let path = dir.join(".codex/agents/planner.toml");
            write(&path, REFUSED_FILE);

            rebuild_project("refused-new").expect("rebuild");
            sync_project("refused-new").expect("sync");

            assert_eq!(std::fs::read_to_string(&path).expect("read"), REFUSED_FILE);
            let raw = core::read_project("refused-new").expect("read project");
            let project: core::Project = serde_json::from_str(&raw).expect("project json");
            assert!(project.custom_agents.is_none(), "{:?}", project.custom_agents);
        });
    }

    #[test]
    fn rebuild_preview_says_which_files_were_refused_and_why() {
        with_temp_home(|_| {
            let (_keep, dir) = planner_project("refused-preview", &["codex"]);
            write(&dir.join(".codex/agents/planner.toml"), REFUSED_FILE);

            let preview = preview_rebuild_project("refused-preview").expect("preview");

            let preview: serde_json::Value = serde_json::from_str(&preview).expect("json");
            let refused = preview["refused_files"].as_array().expect("refused files");
            assert_eq!(refused.len(), 1, "{refused:?}");
            let line = refused[0].as_str().expect("text");
            assert!(line.contains(".codex/agents/planner.toml"), "{line}");
            assert!(line.contains("description"), "{line}");
        });
    }

    /// A stale managed file rebuild can read is still removed.
    #[test]
    fn sync_still_removes_a_stale_managed_file_it_can_read() {
        with_temp_home(|_| {
            let (_keep, dir) = make_project("stale", |p| {
                p.agents = vec!["codex".into()];
            });
            let path = dir.join(".codex/agents/planner.toml");
            write(&path, "automatic_managed = true\nname = \"planner\"\n");

            sync_project("stale").expect("sync");

            assert!(!path.exists());
        });
    }

    /// Both quote kinds in a value that needs quoting: Codex stores it, but
    /// rebuild refuses to read it back.
    const REFUSED_STORED: &str =
        "---\nname: planner\ndescription: it's: \"both\"\n---\nStored.\n";

    /// A Codex project whose only agent renders to a file rebuild refuses.
    fn refused_render_project(name: &str) -> (tempfile::TempDir, std::path::PathBuf, String) {
        let (keep, dir) = make_project(name, |p| {
            p.agents = vec!["codex".into()];
            p.custom_agents = Some(vec![core::CustomAgent {
                name: "Planner".into(),
                content: REFUSED_STORED.into(),
            }]);
        });
        sync_project(name).expect("sync");
        let rendered = codex_render(REFUSED_STORED);
        let path = dir.join(".codex/agents/planner.toml");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), rendered);
        (keep, dir, rendered)
    }

    /// Save the project as stored, after `edit`.
    fn save_edited(name: &str, edit: impl FnOnce(&mut core::Project)) {
        let raw = core::read_project(name).expect("read project");
        let mut project: core::Project = serde_json::from_str(&raw).expect("project json");
        edit(&mut project);
        let json = serde_json::to_string(&project).expect("project json");
        save_project(name, &json, None).expect("save");
    }

    fn drift_reasons(name: &str) -> Vec<(String, String)> {
        let raw = core::read_project(name).expect("read project");
        let project: core::Project = serde_json::from_str(&raw).expect("project json");
        sync::check_project_drift(&project)
            .expect("drift")
            .agents
            .into_iter()
            .flat_map(|agent| agent.files)
            .map(|file| (file.path, file.reason))
            .collect()
    }

    /// Sync's sweep leaves a file rebuild refuses, so the save that drops the
    /// agent has to remove it.
    #[test]
    fn removing_an_agent_deletes_its_file_even_when_rebuild_refuses_it() {
        with_temp_home(|_| {
            let (_keep, dir, _) = refused_render_project("drop-refused");

            save_edited("drop-refused", |p| p.custom_agents = None);

            assert!(!dir.join(".codex/agents/planner.toml").exists());
            assert_eq!(drift_reasons("drop-refused"), Vec::new());
        });
    }

    /// A hand edit is not Automatic's to delete, and sync cannot clear it, so
    /// drift says what it is instead of calling it stale.
    #[test]
    fn removing_an_agent_keeps_an_edited_file_and_drift_marks_it_refused() {
        with_temp_home(|_| {
            let (_keep, dir, rendered) = refused_render_project("drop-edited");
            let path = dir.join(".codex/agents/planner.toml");
            let edited = format!("# mine\n{rendered}");
            write(&path, &edited);

            save_edited("drop-edited", |p| p.custom_agents = None);
            sync_project("drop-edited").expect("sync");

            assert_eq!(std::fs::read_to_string(&path).expect("read"), edited);
            assert_eq!(
                drift_reasons("drop-edited"),
                vec![(".codex/agents/planner.toml".to_string(), "refused".to_string())]
            );
        });
    }

    /// Renaming in the editor drops the old machine name.
    #[test]
    fn renaming_an_agent_deletes_the_file_under_its_old_name() {
        with_temp_home(|_| {
            let (_keep, dir, _) = refused_render_project("rename-refused");

            save_edited("rename-refused", |p| {
                p.custom_agents = Some(vec![core::CustomAgent {
                    name: "Reviewer".into(),
                    content: REFUSED_STORED.replace("name: planner", "name: reviewer"),
                }]);
            });

            assert!(!dir.join(".codex/agents/planner.toml").exists());
            assert!(dir.join(".codex/agents/reviewer.toml").exists());
        });
    }

    /// An agent another entry still renders to the same file was not dropped.
    #[test]
    fn a_display_name_change_keeps_the_file() {
        with_temp_home(|_| {
            let (_keep, dir, rendered) = refused_render_project("same-machine-name");

            save_edited("same-machine-name", |p| {
                p.custom_agents.as_mut().expect("agents")[0].name = "The Planner".into();
            });

            assert_eq!(
                std::fs::read_to_string(dir.join(".codex/agents/planner.toml")).expect("read"),
                rendered
            );
        });
    }

    /// A tool removed in the same save has stopped being synced.  Its files
    /// are the agent-removal flow's business.
    #[test]
    fn removing_an_agent_leaves_the_files_of_a_tool_removed_with_it() {
        with_temp_home(|_| {
            let (_keep, dir, rendered) = refused_render_project("drop-with-tool");

            save_edited("drop-with-tool", |p| {
                p.custom_agents = None;
                p.agents = vec!["claude".into()];
            });

            assert_eq!(
                std::fs::read_to_string(dir.join(".codex/agents/planner.toml")).expect("read"),
                rendered
            );
        });
    }

    /// One byte over the limit rebuild reads, marked as Automatic's.
    fn oversized_managed_command() -> String {
        let padding = "x".repeat(1024 * 1024);
        format!("---\nautomatic-managed: true\n---\n{padding}\n")
    }

    /// Rebuild cannot read this file, and it may be the only copy of what is
    /// in it, so sync must not take rebuild's silence for "stale".
    #[test]
    fn rebuild_and_sync_keep_a_command_rebuild_cannot_read() {
        with_temp_home(|_| {
            let (_keep, dir) = make_project("big-command", |p| {
                p.agents = vec!["claude".into()];
            });
            let path = dir.join(".claude/commands/big.md");
            let content = oversized_managed_command();
            write(&path, &content);

            let preview = preview_rebuild_project("big-command").expect("preview");
            rebuild_project("big-command").expect("rebuild");
            sync_project("big-command").expect("sync");

            assert_eq!(std::fs::read_to_string(&path).expect("read"), content);
            assert!(preview.contains(".claude/commands/big.md"), "{preview}");
        });
    }

    /// As for agents: the stored command stays when its file cannot be read.
    #[test]
    fn rebuild_keeps_a_stored_command_whose_file_it_cannot_read() {
        with_temp_home(|_| {
            let (_keep, dir) = make_project("big-stored", |p| {
                p.agents = vec!["claude".into()];
                p.custom_commands = Some(vec![core::CustomCommand {
                    name: "big".into(),
                    content: "Stored.\n".into(),
                }]);
            });
            let path = dir.join(".claude/commands/big.md");
            let oversized = oversized_managed_command();
            write(&path, &oversized);

            rebuild_project("big-stored").expect("rebuild");
            sync_project("big-stored").expect("sync");

            assert_eq!(std::fs::read_to_string(&path).expect("read"), oversized);
            let raw = core::read_project("big-stored").expect("read project");
            let project: core::Project = serde_json::from_str(&raw).expect("project json");
            let commands = project.custom_commands.expect("custom commands");
            assert_eq!(commands.len(), 1);
            assert_eq!(commands[0].content, "Stored.\n");
        });
    }

    /// A committed index that links to a device must not hang the sync.
    #[cfg(unix)]
    #[test]
    fn sync_with_no_commands_does_not_read_a_linked_index() {
        with_temp_home(|_| {
            let (_keep, dir) = make_project("linked-index", |p| {
                p.agents = vec!["claude".into()];
            });
            std::fs::create_dir_all(dir.join(".agents")).expect("mkdir");
            let index = dir.join(".agents/commands-index.md");
            std::os::unix::fs::symlink("/dev/zero", &index).expect("link");

            sync_project("linked-index").expect("sync");

            assert!(index.symlink_metadata().is_ok());
        });
    }

    /// Rebuild applies the same name check as adopt, so a file cannot reach
    /// the project under a name a YAML reader disagrees with.
    #[test]
    fn rebuild_refuses_a_file_with_another_form_of_its_name() {
        with_temp_home(|_| {
            let (_keep, dir) = make_project("quoted-name", |p| {
                p.agents = vec!["claude".into()];
            });
            let path = dir.join(".claude/agents/planner.md");
            for name_line in [
                "\"name\": other",
                "name : other",
                "  name: other",
                "name: \"'planner'\"",
            ] {
                let content = format!("---\n{name_line}\nautomatic-managed: true\n---\nBody.\n");
                write(&path, &content);

                let preview = preview_rebuild_project("quoted-name").expect("preview");
                rebuild_project("quoted-name").expect("rebuild");

                let raw = core::read_project("quoted-name").expect("read project");
                let project: core::Project = serde_json::from_str(&raw).expect("project json");
                assert!(project.custom_agents.is_none(), "{name_line}");
                assert!(preview.contains(".claude/agents/planner.md"), "{name_line}");
                assert_eq!(std::fs::read_to_string(&path).expect("read"), content);
            }
        });
    }

    /// The same for a tool whose commands are rendered rather than linked.
    #[test]
    fn sync_does_not_overwrite_a_rendered_command_it_cannot_read() {
        with_temp_home(|_| {
            let (_keep, dir) = make_project("big-gemini", |p| {
                p.agents = vec!["gemini".into()];
                p.custom_commands = Some(vec![core::CustomCommand {
                    name: "big".into(),
                    content: "Stored.\n".into(),
                }]);
            });
            let path = dir.join(".gemini/commands/big.toml");
            let oversized = format!("automatic_managed = true\n#{}\n", "x".repeat(1024 * 1024));
            write(&path, &oversized);

            sync_project("big-gemini").expect("sync");

            assert_eq!(std::fs::read_to_string(&path).expect("read"), oversized);
        });
    }

    /// Detaching a library agent is the same as removing a custom one.
    #[test]
    fn detaching_a_library_agent_deletes_its_file_even_when_rebuild_refuses_it() {
        with_temp_home(|_| {
            core::save_subagent("planner", "Planner", REFUSED_STORED).expect("save library agent");
            let (_keep, dir) = make_project("drop-library", |p| {
                p.agents = vec!["codex".into()];
                p.user_agents = vec!["planner".into()];
            });
            sync_project("drop-library").expect("sync");
            let path = dir.join(".codex/agents/planner.toml");
            assert!(path.exists());

            save_edited("drop-library", |p| p.user_agents.clear());

            assert!(!path.exists());
            assert_eq!(drift_reasons("drop-library"), Vec::new());
        });
    }

    /// A file rebuild cannot read cannot be matched to the library, and that
    /// must not detach the agent.
    #[test]
    fn rebuild_keeps_a_library_agent_whose_file_it_cannot_read() {
        with_temp_home(|_| {
            core::save_subagent("library-id", "Planner", STORED).expect("save library agent");
            let (_keep, dir) = make_project("big-library", |p| {
                p.agents = vec!["claude".into()];
                p.user_agents = vec!["library-id".into()];
            });
            write(&dir.join(".claude/agents/planner.md"), &oversized_managed_command());

            rebuild_project("big-library").expect("rebuild");

            let raw = core::read_project("big-library").expect("read project");
            let project: core::Project = serde_json::from_str(&raw).expect("project json");
            assert_eq!(project.user_agents, vec!["library-id".to_string()]);
        });
    }

    /// A name rebuild refuses keeps the file too, and drift says so rather
    /// than reporting something sync never clears as stale.
    #[cfg(unix)]
    #[test]
    fn rebuild_and_sync_keep_a_command_with_a_refused_name() {
        with_temp_home(|_| {
            let (_keep, dir) = make_project("bad-command", |p| {
                p.agents = vec!["claude".into()];
            });
            let path = dir.join(".claude/commands/back\\slash.md");
            write(&path, "---\nautomatic-managed: true\n---\nKeep me.\n");

            rebuild_project("bad-command").expect("rebuild");
            sync_project("bad-command").expect("sync");

            assert!(path.exists());
            assert_eq!(
                drift_reasons("bad-command"),
                vec![(".claude/commands/back\\slash.md".to_string(), "refused".to_string())]
            );
        });
    }

    /// Quoting is written only by rebuild and adopt, so it never reaches an
    /// untouched project, and it reads the same as the older form.  These
    /// `disk` strings are what the previous version wrote, so they stay
    /// literal.  A change here means every such project conflicts on upgrade.
    #[test]
    fn stored_values_older_versions_wrote_leave_no_conflict_after_upgrade() {
        let head = "automatic_managed = true\nname = \"planner\"\n";
        let body = "\ndeveloper_instructions = \"\"\"\nPlan.\n\"\"\"\n";
        let cases = [
            (
                "---\nname: planner\ndescription: \nmodel: inherit\ntools: inherit\n---\n\nPlan.\n",
                "description = \"\"\nmodel = \"inherit\"\nsandbox_mode = \"read-only\"\n",
            ),
            (
                "---\nname: planner\ndescription: ''\nmodel: inherit\n---\n\nPlan.\n",
                "description = \"\"\nmodel = \"inherit\"\n",
            ),
            (
                "---\nname: planner\ndescription: 'true'\nmodel: '5'\nmaxTurns: 5\n---\n\nPlan.\n",
                "description = \"true\"\nmodel = \"5\"\nmax_turns = 5\n",
            ),
            (
                "---\nname: planner\ndescription: true\nmodel: 5\n---\n\nPlan.\n",
                "description = \"true\"\nmodel = \"5\"\n",
            ),
        ];
        with_temp_home(|_| {
            for (index, (stored, fields)) in cases.into_iter().enumerate() {
                let name = format!("upgrade-{index}");
                let (_keep, dir) = make_project(&name, |p| {
                    p.agents = vec!["codex".into()];
                    p.custom_agents = Some(vec![core::CustomAgent {
                        name: "Planner".into(),
                        content: stored.into(),
                    }]);
                });
                let disk = format!("{head}{fields}{body}");
                write(&dir.join(".codex/agents/planner.toml"), &disk);

                assert_eq!(codex_render(stored), disk, "{stored}");
                assert_no_conflict(&name, &dir);
            }
        });
    }

    /// Older versions wrote these wrongly, or wrote TOML that does not load.
    /// The file really differs from the render, so one conflict shows, and
    /// Overwrite ends it.
    #[test]
    fn a_file_an_older_render_got_wrong_conflicts_until_overwritten() {
        let older = "automatic_managed = true\nname = \"planner\"\ndescription = \"Plans\"\n";
        let cases = [
            (
                "---\nname: planner\ndescription: Plans\n---\n\nMatch \\d here.\n",
                format!("{older}\ndeveloper_instructions = \"\"\"\nMatch \\d here.\n\"\"\"\n"),
            ),
            (
                "---\nname: planner\ndescription: Plans\nmaxTurns: many\n---\n\nPlan.\n",
                format!("{older}max_turns = many\n\ndeveloper_instructions = \"\"\"\nPlan.\n\"\"\"\n"),
            ),
        ];
        with_temp_home(|_| {
            for (index, (stored, disk)) in cases.into_iter().enumerate() {
                let name = format!("older-render-{index}");
                let (_keep, dir) = make_project(&name, |p| {
                    p.agents = vec!["codex".into()];
                    p.custom_agents = Some(vec![core::CustomAgent {
                        name: "Planner".into(),
                        content: stored.into(),
                    }]);
                });
                write(&dir.join(".codex/agents/planner.toml"), &disk);
                let raw = core::read_project(&name).expect("read project");
                let project: core::Project = serde_json::from_str(&raw).expect("project json");
                assert!(
                    matches!(
                        sync::scan_custom_agent(&project, &dir, &project.custom_agents.clone().expect("agents")[0]),
                        Some(sync::CustomAgentFinding::Conflict { .. })
                    ),
                    "{stored}"
                );

                overwrite_custom_asset(&name, "agent", "Planner").expect("overwrite");

                assert_no_conflict(&name, &dir);
            }
        });
    }

    /// Older versions mixed line endings when rendering a CRLF copy.  This
    /// `disk` string is what they wrote, so it stays literal.
    #[test]
    fn a_crlf_file_an_older_render_mixed_up_conflicts_until_overwritten() {
        let stored = "---\r\nname: planner\r\n---\r\nBody.\r\n";
        let disk = "---\n\nname: planner\r\nautomatic-managed: true\n---\r\nBody.\r\n";
        with_temp_home(|_| {
            let (_keep, dir) = make_project("older-crlf", |p| {
                p.agents = vec!["claude".into()];
                p.custom_agents = Some(vec![core::CustomAgent {
                    name: "Planner".into(),
                    content: stored.into(),
                }]);
            });
            write(&dir.join(".claude/agents/planner.md"), disk);
            let raw = core::read_project("older-crlf").expect("read project");
            let project: core::Project = serde_json::from_str(&raw).expect("project json");
            let agents = project.custom_agents.clone().expect("agents");
            assert!(matches!(
                sync::scan_custom_agent(&project, &dir, &agents[0]),
                Some(sync::CustomAgentFinding::Conflict { .. })
            ));

            overwrite_custom_asset("older-crlf", "agent", "Planner").expect("overwrite");

            assert_no_conflict("older-crlf", &dir);
        });
    }

    /// A byte order mark must not leave a conflict behind: the stored copy
    /// drops it, and the file still on disk then matches.
    #[test]
    fn adopted_markdown_file_with_a_byte_order_mark_leaves_no_conflict() {
        with_temp_home(|_| {
            let (_keep, dir) = planner_project("bom", &["claude"]);
            let edited = "---\nname: planner\ndescription: Plans\n---\nEdited.\n";
            write(&dir.join(".claude/agents/planner.md"), &format!("\u{feff}{edited}"));

            adopt_custom_asset("bom", "agent", "Planner").expect("adopt");

            assert_eq!(stored_planner("bom"), edited);
            assert_no_conflict("bom", &dir);
        });
    }

    /// With no differing file there is nothing to adopt.
    #[test]
    fn adopt_without_a_conflict_is_an_error() {
        with_temp_home(|_| {
            let (_keep, dir) = planner_project("in-sync", &["claude"]);
            write(
                &dir.join(".claude/agents/planner.md"),
                &ClaudeCode.convert_agent_content(STORED, "planner"),
            );

            let err = adopt_custom_asset("in-sync", "agent", "Planner").unwrap_err();

            assert!(err.contains("No on-disk version"), "{err}");
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_asset_commands_refuse_traversal_names_first() {
        for kind in ["skill", "command", "rule"] {
            let asset_name = if kind == "rule" { "../.." } else { "../../evil" };
            assert!(
                adopt_custom_asset("no-such-project", kind, asset_name)
                    .is_err_and(|e| e.contains("Custom")),
                "adopt {kind}"
            );
            assert!(
                overwrite_custom_asset("no-such-project", kind, asset_name)
                    .is_err_and(|e| e.contains("Custom")),
                "overwrite {kind}"
            );
        }
        assert!(remove_stale_skill("no-such-project", "../..").is_err_and(|e| e.contains("Skill")));
        assert!(ensure_custom_asset_name("command", "deploy").is_ok());
        assert!(ensure_custom_asset_name("rule", "CI/CD").is_ok());
    }
}
