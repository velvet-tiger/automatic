use crate::core;
use crate::sync;

// ── Projects ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_projects() -> Result<Vec<String>, String> {
    core::list_projects()
}

#[tauri::command]
pub fn read_project(name: &str) -> Result<String, String> {
    core::read_project(name)
}

#[tauri::command]
pub fn autodetect_project_dependencies(name: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let updated = sync::autodetect_project_dependencies(&project)?;
    serde_json::to_string_pretty(&updated).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_project(name: &str, data: &str) -> Result<(), String> {
    let incoming: core::Project =
        serde_json::from_str(data).map_err(|e| format!("Invalid project data: {}", e))?;

    // No directory configured yet -- just persist to the registry and return.
    // There is nothing to sync until the user has pointed us at a real directory.
    if incoming.directory.is_empty() {
        return core::save_project(name, data);
    }

    // Detect whether this is a brand-new project (no existing registry entry).
    let is_new = core::read_project(name).is_err();

    if is_new {
        // ── Case 1: Project is being added for the first time ─────────────
        //
        // Save the initial state so the project exists in the registry even if
        // the subsequent autodetect fails for any reason.  Then run full
        // autodetect to discover all agents, skills, and MCP servers that are
        // already present in the directory.  The enriched project config is
        // written back to disk by sync_project (via sync_project_without_autodetect).
        //
        // Nothing is deleted during this step -- autodetect only adds findings.
        core::save_project(name, data)?;

        // Errors are intentionally swallowed: partial success (project saved
        // but no agent configs written because the directory has no AI tools)
        // is better than returning a hard error to the frontend.
        let _ = sync::sync_project(&incoming);
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

        if let Some(existing) = existing_project {
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
                    if !enriched.mcp_servers.contains(&server_name) {
                        enriched.mcp_servers.push(server_name.clone());
                    }
                    // Persist the config to the global registry so that
                    // sync_project_without_autodetect can include it when
                    // building the mcpServers map written to disk.
                    let _ = core::save_mcp_server_config(&server_name, &config_str);
                }
            }
        }

        let enriched_data = serde_json::to_string_pretty(&enriched).map_err(|e| e.to_string())?;
        core::save_project(name, &enriched_data)?;

        if !enriched.agents.is_empty() {
            sync::sync_project_without_autodetect(&enriched)?;
        }
    }

    Ok(())
}

#[tauri::command]
pub fn rename_project(old_name: &str, new_name: &str) -> Result<(), String> {
    core::rename_project(old_name, new_name)?;

    // Re-sync agent configs so AUTOMATIC_PROJECT reflects the new name.
    let raw = core::read_project(new_name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    if !project.directory.is_empty() && !project.agents.is_empty() {
        sync::sync_project(&project)?;
    }

    Ok(())
}

#[tauri::command]
pub fn delete_project(name: &str) -> Result<(), String> {
    core::delete_project(name)
}

// ── Project Sync ─────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn sync_project(name: String) -> Result<String, String> {
    let raw = core::read_project(&name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let _ = sync::sync_project(&project)?;
    Ok("Sync successful".to_string())
}

/// Return the list of file/directory paths that would be removed if the given
/// agent were removed from the project.  Read-only -- used to populate the
/// confirmation dialog before the user commits to the removal.
#[tauri::command]
pub fn get_agent_cleanup_preview(name: &str, agent_id: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let preview = sync::get_agent_cleanup_preview(&project, agent_id)?;
    serde_json::to_string(&preview).map_err(|e| e.to_string())
}

/// Remove an agent from a project and delete all files it wrote.
/// The project config is persisted and remaining agents are re-synced.
/// Returns a JSON array of paths that were removed or modified.
#[tauri::command]
pub fn remove_agent_from_project(name: &str, agent_id: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let mut project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let removed = sync::remove_agent_from_project(&mut project, agent_id)?;
    serde_json::to_string(&removed).map_err(|e| e.to_string())
}

/// Check whether the on-disk agent configs have drifted from what Automatic would
/// generate.  Returns a JSON-serialised [`sync::DriftReport`] describing which
/// agents and files are out of sync.  This is a read-only operation.
#[tauri::command]
pub async fn check_project_drift(name: String) -> Result<String, String> {
    let raw = core::read_project(&name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let report = sync::check_project_drift(&project)?;
    serde_json::to_string(&report).map_err(|e| e.to_string())
}

// ── Cross-cutting helpers ────────────────────────────────────────────────────
//
// These are used by skills, rules, mcp_servers, and skill_store modules when
// a registry item is saved or deleted and projects referencing it need updating.

pub(crate) fn with_each_project_mut<F>(mut f: F)
where
    F: FnMut(&str, &mut core::Project),
{
    let project_names = match core::list_projects() {
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

pub(crate) fn sync_project_if_configured(project_name: &str, project: &core::Project) {
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

pub(crate) fn prune_skill_from_projects(skill_name: &str) {
    with_each_project_mut(|project_name, project| {
        let before = project.skills.len();
        project.skills.retain(|skill| skill != skill_name);

        if project.skills.len() != before {
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

        if project.mcp_servers.len() != before {
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

#[allow(dead_code)]
pub(crate) fn sync_projects_referencing_rule(rule_name: &str) {
    with_each_project_mut(|project_name, project| {
        let references_rule = project
            .file_rules
            .values()
            .any(|rules| rules.iter().any(|r| r == rule_name));
        if references_rule {
            // Re-inject rules into any project files that use this rule
            for (filename, rules) in &project.file_rules {
                if rules.iter().any(|r| r == rule_name) {
                    let _ =
                        core::inject_rules_into_project_file(&project.directory, filename, rules);
                }
            }
            sync_project_if_configured(project_name, project);
        }
    });
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
            // Re-inject rules for affected files
            for (filename, rules) in &project.file_rules {
                let _ = core::inject_rules_into_project_file(&project.directory, filename, rules);
            }
            sync_project_if_configured(project_name, project);
        }
    });
}
