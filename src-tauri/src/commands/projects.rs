use crate::activity::{self, ActivityEvent};
use crate::context;
use crate::core;
use crate::sync;
use serde::Serialize;
use std::collections::HashMap;

// ── Projects ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_projects() -> Result<Vec<String>, String> {
    core::list_projects()
}

/// Every registered project as `{local_key, id, name, directory}`, sorted by
/// name. Reads only; unlike `read_project` it never writes project files.
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
}

#[tauri::command]
pub fn preview_rebuild_project(name: &str) -> Result<String, String> {
    let name = &crate::core::canonical_project_ident(name)?;
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let rebuilt = sync::rebuild_project_state(&project)?;

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

/// Save a project and sync it. `name` is an identifier (a `local_key`, or a
/// name). Returns the saved project's `local_key` (its name while it has no
/// key), which the Add Project wizard uses to address the project it just
/// created: the name may be shared with another project.
#[tauri::command]
pub fn save_project(name: &str, data: &str, creating: Option<bool>) -> Result<String, String> {
    let mut incoming: core::Project =
        serde_json::from_str(data).map_err(|e| format!("Invalid project data: {}", e))?;
    let creating = creating.unwrap_or(false);

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
    let member_groups = core::groups_for_project(name);
    core::reconcile_group_profiles(&mut incoming, &member_groups);
    core::reconcile_project_profiles(&mut incoming);
    // Groups: same idea for contexts. Membership lives in the group files,
    // so a stale editor copy cannot drop or duplicate a group's contexts.
    core::reconcile_group_contexts(&mut incoming, &member_groups);

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
                    // Persist the config to the global registry so that
                    // sync_project_without_autodetect can include it when
                    // building the mcpServers map written to disk.
                    let config_str = core::mark_discovered_if_blocked(&config_str);
                    let _ = core::save_mcp_server_config(&server_name, &config_str);
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
            let written = sync::sync_project_without_autodetect(&mut enriched)?;
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
    // (is_new == true) we always run it regardless of the throttle.
    {
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
    // Verify a key exists before attempting the (potentially slow) AI call.
    crate::core::ai::resolve_api_key(None)?;

    // Honour the throttle for existing projects; always run for new ones.
    // The throttle timestamp is stored under the checkout's `local_key`.
    let store_key = crate::core::project_store_local_key(project)?;
    if !force && crate::recommendations::ai_recommendations_throttled(&store_key)? {
        return Ok(());
    }

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

/// Adopt an existing on-disk project config (`.automatic.json`, or a legacy
/// `.automatic/project.json`) as a registered project. Returns the adopted
/// project's `{name, local_key}`: open it by the key, since another project
/// may share the name.
#[tauri::command]
pub fn import_existing_project(directory: &str) -> Result<core::ImportedProject, String> {
    let imported = core::import_existing_project(directory)?;
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

#[tauri::command]
pub fn rename_project(old_name: &str, new_name: &str) -> Result<(), String> {
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
    if project.id.is_empty() && !project.directory.is_empty() && !project.agents.is_empty() {
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

#[tauri::command]
pub fn delete_project(name: &str) -> Result<(), String> {
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
    let name = &crate::core::canonical_project_ident(name)?;
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let docs: HashMap<String, context::DocEntry> =
        serde_json::from_str(content).map_err(|e| format!("Invalid docs JSON: {}", e))?;

    context::save_project_docs(&project.directory, &docs)
}

// ── Project Sync ─────────────────────────────────────────────────────────────

#[tauri::command]
pub fn sync_project(name: &str) -> Result<String, String> {
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

#[tauri::command]
pub fn rebuild_project(name: &str) -> Result<String, String> {
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
#[tauri::command]
pub fn remove_agent_from_project(name: &str, agent_id: &str, mode: &str) -> Result<String, String> {
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
#[tauri::command]
pub fn adopt_stale_skill(name: &str, skill_name: &str) -> Result<(), String> {
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
#[tauri::command]
pub fn remove_stale_skill(name: &str, skill_name: &str) -> Result<(), String> {
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
fn effective_project_dir(project: &core::Project) -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(&project.directory);
    match project.mode {
        core::ProjectMode::Silent => dir.join(".automatic").join("silent"),
        core::ProjectMode::Normal => dir,
    }
}

fn custom_rule_slug(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn strip_instructions_header(content: &str) -> String {
    content
        .strip_prefix("<!-- managed by Automatic — do not edit by hand -->\n\n")
        .unwrap_or(content)
        .trim_end()
        .to_string()
}

/// Adopt on-disk custom asset content into the project's stored snapshot.
///
/// `kind` is one of `"skill"`, `"rule"`, `"agent"`, `"command"`.
/// Favours the on-disk file over Automatic's stored copy.
#[tauri::command]
pub fn adopt_custom_asset(name: &str, kind: &str, asset_name: &str) -> Result<(), String> {
    let name = &crate::core::canonical_project_ident(name)?;
    let raw = core::read_project(name)?;
    let mut project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    if project.directory.is_empty() {
        return Err("Project has no directory configured".into());
    }

    let effective = effective_project_dir(&project);

    match kind {
        "skill" => {
            let skill_md = effective
                .join(".agents")
                .join("skills")
                .join(asset_name)
                .join("SKILL.md");
            let disk_content = std::fs::read_to_string(&skill_md).map_err(|e| {
                format!("Failed to read {}: {}", skill_md.display(), e)
            })?;
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
                .join(format!("custom-{}.md", custom_rule_slug(asset_name)));
            let raw_disk = std::fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
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
            // Prefer the first matching on-disk file under any agent dir.
            let mut found: Option<String> = None;
            if let Some(agents) = project.custom_agents.as_ref() {
                if let Some(ca) = agents.iter().find(|a| a.name == asset_name) {
                    let machine_name = sync::extract_agent_machine_name_pub(&ca.content)
                        .unwrap_or_else(|| ca.name.to_lowercase().replace(' ', "-"));
                    for agent_id in &project.agents {
                        let Some(agent_instance) = crate::agent::from_id(agent_id) else {
                            continue;
                        };
                        let Some(agents_dir) = agent_instance.agents_dir(&effective) else {
                            continue;
                        };
                        let ext = agent_instance.agents_file_ext();
                        let path = agents_dir.join(format!("{}.{}", machine_name, ext));
                        if path.exists() {
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                found = Some(content);
                                break;
                            }
                        }
                    }
                }
            }
            let disk_content = found.ok_or_else(|| {
                format!(
                    "On-disk agent '{}' was not found in any agent directory",
                    asset_name
                )
            })?;
            let custom = project.custom_agents.get_or_insert_with(Vec::new);
            let Some(entry) = custom.iter_mut().find(|a| a.name == asset_name) else {
                return Err(format!(
                    "Agent '{}' is not a project-scoped custom agent",
                    asset_name
                ));
            };
            entry.content = disk_content;
        }
        "command" => {
            let path = effective
                .join(".agents")
                .join("commands")
                .join(format!("{}.md", asset_name));
            let disk_content = std::fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
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

    activity::log(
        name,
        ActivityEvent::ProjectUpdated,
        &format!("Custom {} adopted from disk", kind),
        asset_name,
    );

    Ok(())
}

/// Overwrite the on-disk custom asset with Automatic's stored snapshot, then
/// re-sync so agent copies/symlinks match.
#[tauri::command]
pub fn overwrite_custom_asset(name: &str, kind: &str, asset_name: &str) -> Result<(), String> {
    let name = &crate::core::canonical_project_ident(name)?;
    let raw = core::read_project(name)?;
    let mut project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    if project.directory.is_empty() {
        return Err("Project has no directory configured".into());
    }

    let effective = effective_project_dir(&project);

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
            std::fs::create_dir_all(&hub_dir)
                .map_err(|e| format!("Failed to create {}: {}", hub_dir.display(), e))?;
            std::fs::write(hub_dir.join("SKILL.md"), &stored)
                .map_err(|e| format!("Failed to write skill: {}", e))?;
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
            std::fs::create_dir_all(&instructions_dir)
                .map_err(|e| format!("Failed to create {}: {}", instructions_dir.display(), e))?;
            let path = instructions_dir.join(format!("custom-{}.md", custom_rule_slug(asset_name)));
            let file_content = format!(
                "<!-- managed by Automatic — do not edit by hand -->\n\n{}\n",
                stored.trim_end()
            );
            std::fs::write(&path, file_content)
                .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
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
            std::fs::create_dir_all(&hub_dir)
                .map_err(|e| format!("Failed to create {}: {}", hub_dir.display(), e))?;
            let rendered = crate::agent::render_markdown_command(&stored);
            std::fs::write(hub_dir.join(format!("{}.md", asset_name)), rendered)
                .map_err(|e| format!("Failed to write command: {}", e))?;
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

pub(crate) fn sync_project_if_configured(project_name: &str, project: &mut core::Project) {
    if project.directory.is_empty() || project.agents.is_empty() {
        return;
    }

    if let Err(e) = sync::sync_project_without_autodetect(project) {
        eprintln!(
            "Failed to sync project '{}' after registry update: {}",
            project_name, e
        );
    }
}

pub(crate) fn sync_projects_referencing_skill(skill_name: &str) {
    with_each_project_mut(|project_name, project| {
        if project.skills.iter().any(|skill| skill == skill_name) {
            sync_project_if_configured(project_name, project);
        }
    });
}

pub(crate) fn sync_projects_referencing_mcp_server(server_name: &str) {
    with_each_project_mut(|project_name, project| {
        if project
            .mcp_servers
            .iter()
            .any(|server| server == server_name)
        {
            sync_project_if_configured(project_name, project);
        }
    });
}

pub(crate) fn sync_projects_referencing_rule(rule_name: &str) {
    with_each_project_mut(|project_name, project| {
        let referenced = project
            .file_rules
            .values()
            .any(|rules| rules.iter().any(|r| r == rule_name));
        if referenced {
            sync_project_if_configured(project_name, project);
        }
    });
}

pub(crate) fn sync_projects_referencing_subagent(machine_name: &str) {
    with_each_project_mut(|project_name, project| {
        if project.user_agents.iter().any(|name| name == machine_name) {
            sync_project_if_configured(project_name, project);
        }
    });
}

pub(crate) fn sync_projects_referencing_user_command(machine_name: &str) {
    with_each_project_mut(|project_name, project| {
        if project
            .user_commands
            .iter()
            .any(|name| name == machine_name)
        {
            sync_project_if_configured(project_name, project);
        }
    });
}

pub(crate) fn sync_projects_referencing_hook(machine_name: &str) {
    with_each_project_mut(|project_name, project| {
        if project.hooks.iter().any(|name| name == machine_name) {
            sync_project_if_configured(project_name, project);
        }
    });
}

/// Persist a project a sweep helper has mutated. Failures are logged, never
/// returned: every sweep is best-effort, matching the other helpers here.
fn persist_swept_project(project_name: &str, project: &mut core::Project, what: &str) {
    project.updated_at = chrono::Utc::now().to_rfc3339();
    match serde_json::to_string_pretty(project).map_err(|e| e.to_string()) {
        Ok(data) => {
            if let Err(e) = core::save_project(project_name, &data) {
                eprintln!(
                    "Failed to update project '{}' after {}: {}",
                    project_name, what, e
                );
            }
        }
        Err(e) => {
            eprintln!(
                "Failed to serialise project '{}' after {}: {}",
                project_name, what, e
            );
        }
    }
}

/// Bring every project that references `profile_name` back in step with the
/// profile and re-sync the ones that changed. Called after a profile is
/// saved, and after a library asset it references is pruned or renamed.
pub(crate) fn reconcile_projects_referencing_profile(profile_name: &str) {
    with_each_project_mut(|project_name, project| {
        let referenced = project.profiles.iter().any(|p| p == profile_name)
            || project.profile_contributions.contains_key(profile_name);
        if !referenced {
            return;
        }
        let report = core::reconcile_project_profiles(project);
        if !report.changed {
            return;
        }
        persist_swept_project(project_name, project, "profile reconcile");
        sync_project_if_configured(project_name, project);
    });
}

/// Bring the profiles of each named project in step with its groups, then
/// with the profiles themselves, and re-sync the ones that changed. Call
/// after a group is saved or deleted, with its members from before and
/// after the change (`core::group_member_idents`).
///
/// Per-project failures are logged and skipped so one unreadable project
/// does not leave the rest out of step.
pub(crate) fn reconcile_group_profiles_for_projects(project_idents: &[String]) {
    let mut seen = std::collections::HashSet::new();
    for ident in project_idents {
        if !seen.insert(ident.as_str()) {
            continue;
        }
        let parsed = core::read_project(ident).and_then(|raw| {
            serde_json::from_str::<core::Project>(&raw).map_err(|e| e.to_string())
        });
        let mut project = match parsed {
            Ok(project) => project,
            Err(e) => {
                eprintln!("group profiles reconcile: skipping project '{}': {}", ident, e);
                continue;
            }
        };
        // Group profiles first: they decide which profiles are attached.
        let groups_changed =
            core::reconcile_group_profiles(&mut project, &core::groups_for_project(ident));
        let report = core::reconcile_project_profiles(&mut project);
        if !groups_changed && !report.changed {
            continue;
        }
        persist_swept_project(ident, &mut project, "group profile reconcile");
        sync_project_if_configured(ident, &mut project);
    }
}

/// Detach `profile_name` from every group and every project, dropping every
/// item the profile provides, then re-sync. Used before a profile is deleted.
/// Groups are cleared too, or a group would put the missing profile back.
pub(crate) fn detach_profile_from_projects(profile_name: &str) {
    core::prune_profile_from_groups(profile_name);
    with_each_project_mut(|project_name, project| {
        let before = project.profiles.len();
        project.profiles.retain(|p| p != profile_name);
        let recorded = project.profile_contributions.contains_key(profile_name);
        let group_recorded = core::strip_group_profile_contribution(project, profile_name);
        if project.profiles.len() == before && !recorded && !group_recorded {
            return;
        }
        core::reconcile_project_profiles(project);
        persist_swept_project(project_name, project, "profile detach");
        sync_project_if_configured(project_name, project);
    });
}

/// Rewrite `profiles` entries and contribution keys from `old_name` to
/// `new_name` in every group and every project. Content is unchanged, so no
/// sync is needed.
pub(crate) fn rename_profile_in_projects(old_name: &str, new_name: &str) {
    if old_name == new_name {
        return;
    }
    core::rename_profile_in_groups(old_name, new_name);
    with_each_project_mut(|project_name, project| {
        if core::rename_profile_in_project(project, old_name, new_name) {
            persist_swept_project(project_name, project, "profile rename");
        }
    });
}

pub(crate) fn prune_hook_from_projects(machine_name: &str) {
    with_each_project_mut(|project_name, project| {
        let before = project.hooks.len();
        project.hooks.retain(|name| name != machine_name);
        let stripped =
            core::strip_contribution(project, core::ProfileResourceKind::Hook, machine_name);
        if project.hooks.len() != before || stripped {
            project.updated_at = chrono::Utc::now().to_rfc3339();
            match serde_json::to_string_pretty(project).map_err(|e| e.to_string()) {
                Ok(data) => {
                    if let Err(e) = core::save_project(project_name, &data) {
                        eprintln!(
                            "Failed to update project '{}' after detaching hook '{}': {}",
                            project_name, machine_name, e
                        );
                    }
                }
                Err(e) => {
                    eprintln!(
                        "Failed to serialise project '{}' after detaching hook '{}': {}",
                        project_name, machine_name, e
                    );
                }
            }
            sync_project_if_configured(project_name, project);
        }
    });
}

/// Re-sync every project that has both a configured directory and at least
/// one agent. Used after upgrade-time bundled-asset reinstalls so any project
/// that references the updated library copies picks up the new content
/// without the user having to press "Sync now". Silent failures are logged
/// to stderr; this is best-effort.
pub(crate) fn resync_all_projects() {
    with_each_project_mut(|project_name, project| {
        sync_project_if_configured(project_name, project);
    });
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

    with_each_project_mut(|project_name, project| {
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

pub(crate) fn prune_skill_from_projects(skill_name: &str) {
    with_each_project_mut(|project_name, project| {
        let before = project.skills.len();
        project.skills.retain(|skill| skill != skill_name);
        let stripped =
            core::strip_contribution(project, core::ProfileResourceKind::Skill, skill_name);

        if project.skills.len() != before || stripped {
            project.updated_at = chrono::Utc::now().to_rfc3339();
            match serde_json::to_string_pretty(project).map_err(|e| e.to_string()) {
                Ok(data) => {
                    if let Err(e) = core::save_project(project_name, &data) {
                        eprintln!("Failed to update project '{}': {}", project_name, e);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to serialize project '{}': {}", project_name, e);
                }
            }
            sync_project_if_configured(project_name, project);
        }
    });
}

pub(crate) fn prune_mcp_server_from_projects(server_name: &str) {
    with_each_project_mut(|project_name, project| {
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

        if project.mcp_servers.len() != before || stripped {
            project.updated_at = chrono::Utc::now().to_rfc3339();
            match serde_json::to_string_pretty(project).map_err(|e| e.to_string()) {
                Ok(data) => {
                    if let Err(e) = core::save_project(project_name, &data) {
                        eprintln!("Failed to update project '{}': {}", project_name, e);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to serialize project '{}': {}", project_name, e);
                }
            }
            sync_project_if_configured(project_name, project);
        }
    });
}

/// Rewrite every project's `mcp_servers` and `disabled_mcp_servers` entries
/// from `old_name` to `new_name`. Persists changed projects and re-syncs
/// them so on-disk agent config matches the renamed server.
pub(crate) fn rename_mcp_server_in_projects(old_name: &str, new_name: &str) {
    if old_name == new_name {
        return;
    }
    with_each_project_mut(|project_name, project| {
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
        changed |= core::rename_contribution(
            project,
            core::ProfileResourceKind::McpServer,
            old_name,
            new_name,
        );

        if changed {
            project.updated_at = chrono::Utc::now().to_rfc3339();
            match serde_json::to_string_pretty(project).map_err(|e| e.to_string()) {
                Ok(data) => {
                    if let Err(e) = core::save_project(project_name, &data) {
                        eprintln!("Failed to update project '{}': {}", project_name, e);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to serialize project '{}': {}", project_name, e);
                }
            }
            sync_project_if_configured(project_name, project);
        }
    });
}

/// Rewrite every template's `mcp_servers` entries from `old_name` to
/// `new_name` and persist the ones that changed. Failures are logged and
/// skipped, matching the pattern used by the other propagation helpers.
pub(crate) fn rename_mcp_server_in_templates(old_name: &str, new_name: &str) {
    if old_name == new_name {
        return;
    }
    let template_names = match core::list_templates() {
        Ok(names) => names,
        Err(e) => {
            eprintln!("Failed to list templates for MCP-server rename: {}", e);
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

        let mut changed = false;
        for server in template.mcp_servers.iter_mut() {
            if server == old_name {
                *server = new_name.to_string();
                changed = true;
            }
        }

        if changed {
            match serde_json::to_string_pretty(&template).map_err(|e| e.to_string()) {
                Ok(data) => {
                    if let Err(e) = core::save_template(&template_name, &data) {
                        eprintln!(
                            "Failed to save template '{}' after MCP-server rename: {}",
                            template_name, e
                        );
                    }
                }
                Err(e) => {
                    eprintln!(
                        "Failed to serialise template '{}' after MCP-server rename: {}",
                        template_name, e
                    );
                }
            }
        }
    }
}

pub(crate) fn prune_rule_from_projects(rule_name: &str) {
    with_each_project_mut(|project_name, project| {
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
        changed |= core::strip_contribution(project, core::ProfileResourceKind::Rule, rule_name);

        if changed {
            project.updated_at = chrono::Utc::now().to_rfc3339();
            match serde_json::to_string_pretty(project).map_err(|e| e.to_string()) {
                Ok(data) => {
                    if let Err(e) = core::save_project(project_name, &data) {
                        eprintln!("Failed to update project '{}': {}", project_name, e);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to serialize project '{}': {}", project_name, e);
                }
            }
            // Re-inject rules for affected files, skipping any file whose rules
            // are managed via .claude/rules/ (inline injection must not happen there).
            for (filename, rules) in &project.file_rules {
                if core::project_uses_dot_claude_rules(project, filename) {
                    continue;
                }
                let _ = core::inject_rules_into_project_file(&project.directory, filename, rules);
            }
            sync_project_if_configured(project_name, project);
        }
    });
}

#[cfg(test)]
mod propagation_tests {
    use super::*;
    use tempfile::tempdir;

    /// Run `test` with a thread-local override of Automatic's HOME directory.
    /// Uses `core::paths::with_test_home` so parallel tests in other modules
    /// cannot race on the process-wide `$HOME` env var.
    fn with_temp_home<T>(test: impl FnOnce(&std::path::Path) -> T) -> T {
        let home = tempdir().expect("temp home");
        let home_path = home.path().to_path_buf();
        crate::core::with_test_home(home_path.clone(), || test(&home_path))
    }

    /// Create and persist a project whose directory is an empty tempdir.
    /// Returns the directory path so the test can inspect it after a sync.
    fn make_project(
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

            sync_projects_referencing_rule("test-rule");

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

    fn read_back(name: &str) -> core::Project {
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

            reconcile_projects_referencing_profile("baseline");

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
            reconcile_projects_referencing_profile("baseline");
            assert_eq!(
                read_back("project-a").file_rules["_project"],
                vec!["own-rule", "profile-rule"]
            );

            detach_profile_from_projects("baseline");

            let a = read_back("project-a");
            assert!(a.profiles.is_empty());
            assert!(a.profile_contributions.is_empty());
            assert_eq!(a.file_rules["_project"], vec!["own-rule"]);
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

            resync_all_projects();

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

            sync_projects_referencing_user_command("review");

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
}

/// Commands accept a `local_key` or a name (stage 3b step 1), and each
/// store receives its key: `id` for memory, features and groups, `local_key`
/// for activity, recommendations and dev servers (stage 3b step 2).
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
