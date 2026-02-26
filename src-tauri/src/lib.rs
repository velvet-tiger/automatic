pub mod agent;
pub mod core;
pub mod mcp;
pub mod memory;
pub mod sync;
pub mod context;

// ── Tauri Command Wrappers ───────────────────────────────────────────────────
//
// Thin wrappers that delegate to core:: functions. All business logic lives in
// core.rs so it can be shared with the MCP server and other interfaces.

// ── API Keys ─────────────────────────────────────────────────────────────────

#[tauri::command]
fn save_api_key(provider: &str, key: &str) -> Result<(), String> {
    core::save_api_key(provider, key)
}

#[tauri::command]
fn get_api_key(provider: &str) -> Result<String, String> {
    core::get_api_key(provider)
}

// ── User Profile ─────────────────────────────────────────────────────────────

#[tauri::command]
fn read_profile() -> Result<Option<core::UserProfile>, String> {
    core::read_profile()
}

#[tauri::command]
fn save_profile(profile: core::UserProfile) -> Result<(), String> {
    core::save_profile(&profile)
}

// ── Settings ─────────────────────────────────────────────────────────────────

#[tauri::command]
fn read_settings() -> Result<core::Settings, String> {
    core::read_settings()
}

#[tauri::command]
fn write_settings(settings: core::Settings) -> Result<(), String> {
    core::write_settings(&settings)
}

// ── Agents ───────────────────────────────────────────────────────────────────

#[tauri::command]
fn list_agents() -> Vec<agent::AgentInfo> {
    agent::all().iter().map(|a| agent::AgentInfo::from_agent(*a)).collect()
}

/// Returns each agent with the list of projects that reference it.
#[tauri::command]
fn list_agents_with_projects() -> Result<String, String> {
    let agents = agent::all();
    let project_names = core::list_projects().unwrap_or_default();

    // Read all projects once
    let projects: Vec<core::Project> = project_names
        .iter()
        .filter_map(|name| {
            core::read_project(name)
                .ok()
                .and_then(|raw| serde_json::from_str::<core::Project>(&raw).ok())
        })
        .collect();

    let result: Vec<serde_json::Value> = agents
        .iter()
        .map(|a| {
            let agent_projects: Vec<serde_json::Value> = projects
                .iter()
                .filter(|p| p.agents.iter().any(|id| id == a.id()))
                .map(|p| {
                    serde_json::json!({
                        "name": p.name,
                        "directory": p.directory,
                    })
                })
                .collect();

            serde_json::json!({
                "id": a.id(),
                "label": a.label(),
                "description": a.config_description(),
                "project_file": a.project_file_name(),
                "mcp_note": a.mcp_note(),
                "projects": agent_projects,
            })
        })
        .collect();

    serde_json::to_string(&result).map_err(|e| e.to_string())
}

// ── Skills ───────────────────────────────────────────────────────────────────

#[tauri::command]
fn get_skills() -> Result<Vec<core::SkillEntry>, String> {
    core::list_skills()
}

#[tauri::command]
fn read_skill(name: &str) -> Result<String, String> {
    core::read_skill(name)
}

#[tauri::command]
fn save_skill(name: &str, content: &str) -> Result<(), String> {
    core::save_skill(name, content)?;
    sync_projects_referencing_skill(name);
    Ok(())
}

#[tauri::command]
fn delete_skill(name: &str) -> Result<(), String> {
    core::delete_skill(name)?;
    prune_skill_from_projects(name);
    Ok(())
}

/// Sync a single skill across both global directories (~/.agents/skills/ and
/// ~/.claude/skills/).
#[tauri::command]
fn sync_skill(name: &str) -> Result<(), String> {
    core::sync_skill(name)
}

/// Sync all skills across both global directories.  Returns the list of
/// skill names that were synced.
#[tauri::command]
fn sync_all_skills() -> Result<Vec<String>, String> {
    core::sync_all_skills()
}

// ── Templates ────────────────────────────────────────────────────────────────

#[tauri::command]
fn get_templates() -> Result<Vec<String>, String> {
    core::list_templates()
}

// ── Project Templates ─────────────────────────────────────────────────────────

#[tauri::command]
fn get_project_templates() -> Result<Vec<String>, String> {
    core::list_project_templates()
}

// ── Template Marketplace (bundled) ────────────────────────────────────────────

#[tauri::command]
fn list_bundled_project_templates() -> Result<String, String> {
    core::list_bundled_project_templates()
}

#[tauri::command]
fn read_bundled_project_template(name: &str) -> Result<String, String> {
    core::read_bundled_project_template(name)
}

#[tauri::command]
fn import_bundled_project_template(name: &str) -> Result<(), String> {
    core::import_bundled_project_template(name)
}

#[tauri::command]
fn search_bundled_project_templates(query: &str) -> Result<String, String> {
    core::search_bundled_project_templates(query)
}

#[tauri::command]
fn read_project_template(name: &str) -> Result<String, String> {
    core::read_project_template(name)
}

#[tauri::command]
fn save_project_template(name: &str, data: &str) -> Result<(), String> {
    core::save_project_template(name, data)
}

#[tauri::command]
fn delete_project_template(name: &str) -> Result<(), String> {
    core::delete_project_template(name)
}

#[tauri::command]
fn rename_project_template(old_name: &str, new_name: &str) -> Result<(), String> {
    core::rename_project_template(old_name, new_name)
}

#[tauri::command]
fn read_template(name: &str) -> Result<String, String> {
    core::read_template(name)
}

#[tauri::command]
fn save_template(name: &str, content: &str) -> Result<(), String> {
    core::save_template(name, content)
}

#[tauri::command]
fn delete_template(name: &str) -> Result<(), String> {
    core::delete_template(name)
}

// ── Rules ────────────────────────────────────────────────────────────────────

#[tauri::command]
fn get_rules() -> Result<Vec<core::RuleEntry>, String> {
    core::list_rules()
}

#[tauri::command]
fn read_rule(machine_name: &str) -> Result<String, String> {
    core::read_rule(machine_name)
}

#[tauri::command]
fn save_rule(machine_name: &str, name: &str, content: &str) -> Result<(), String> {
    core::save_rule(machine_name, name, content)?;
    // Re-sync all projects that reference this rule in their file_rules
    sync_projects_referencing_rule(machine_name);
    Ok(())
}

#[tauri::command]
fn delete_rule(machine_name: &str) -> Result<(), String> {
    core::delete_rule(machine_name)?;
    prune_rule_from_projects(machine_name);
    Ok(())
}

// ── Project Files ────────────────────────────────────────────────────────────

/// Returns JSON array of unique project file info objects for the project's agents.
/// Each entry: { filename, agents: ["Claude Code", ...] }
#[tauri::command]
fn get_project_file_info(name: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    let project_dir = std::path::Path::new(&project.directory);

    // Collect all unique agent filenames and their labels
    let mut files: Vec<serde_json::Value> = Vec::new();
    let mut seen_filenames: Vec<String> = Vec::new();

    for agent_id in &project.agents {
        if let Some(a) = agent::from_id(agent_id) {
            let filename = a.project_file_name().to_string();
            let exists = project_dir.join(&filename).exists();

            if !seen_filenames.contains(&filename) {
                seen_filenames.push(filename.clone());
                files.push(serde_json::json!({
                    "filename": filename,
                    "agents": [a.label()],
                    "exists": exists
                }));
            } else {
                // Append agent label to existing entry
                for file in &mut files {
                    if file["filename"].as_str() == Some(&filename) {
                        if let Some(agents) = file["agents"].as_array_mut() {
                            agents.push(serde_json::json!(a.label()));
                        }
                    }
                }
            }
        }
    }

    if project.instruction_mode == "unified" {
        // In unified mode return a single virtual entry that targets all agent files
        let empty_vec = vec![];
        let all_agents: Vec<String> = files
            .iter()
            .flat_map(|f| {
                f["agents"]
                    .as_array()
                    .unwrap_or(&empty_vec)
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
            })
            .collect();
        let all_filenames: Vec<String> = seen_filenames.clone();
        let any_exists = files.iter().any(|f| f["exists"].as_bool().unwrap_or(false));

        let unified = serde_json::json!({
            "filename": "_unified",
            "agents": all_agents,
            "exists": any_exists,
            "target_files": all_filenames
        });
        serde_json::to_string(&vec![unified]).map_err(|e| e.to_string())
    } else {
        files.sort_by(|a, b| {
            let fa = a["filename"].as_str().unwrap_or("");
            let fb = b["filename"].as_str().unwrap_or("");
            fa.cmp(fb)
        });
        serde_json::to_string(&files).map_err(|e| e.to_string())
    }
}

#[tauri::command]
fn read_project_file(name: &str, filename: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    if filename == "_unified" {
        // Read from the first existing agent file
        let project_dir = std::path::Path::new(&project.directory);
        for agent_id in &project.agents {
            if let Some(a) = agent::from_id(agent_id) {
                let f = a.project_file_name();
                if project_dir.join(f).exists() {
                    return core::read_project_file(&project.directory, f);
                }
            }
        }
        // No file exists yet — return empty
        Ok(String::new())
    } else {
        core::read_project_file(&project.directory, filename)
    }
}

/// Collect the unique project filenames for all agents in a project.
fn collect_agent_filenames(project: &core::Project) -> Vec<String> {
    let mut filenames = Vec::new();
    for agent_id in &project.agents {
        if let Some(a) = agent::from_id(agent_id) {
            let f = a.project_file_name().to_string();
            if !filenames.contains(&f) {
                filenames.push(f);
            }
        }
    }
    filenames
}

#[tauri::command]
fn save_project_file(name: &str, filename: &str, content: &str) -> Result<(), String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    if filename == "_unified" || project.instruction_mode == "unified" {
        // Write the same content (with rules) to every agent project file
        let rules = project
            .file_rules
            .get("_unified")
            .cloned()
            .unwrap_or_default();
        for f in collect_agent_filenames(&project) {
            core::save_project_file_with_rules(&project.directory, &f, content, &rules)?;
        }
        Ok(())
    } else {
        let rules = project
            .file_rules
            .get(filename)
            .cloned()
            .unwrap_or_default();
        core::save_project_file_with_rules(&project.directory, filename, content, &rules)
    }
}

// ── MCP Servers ──────────────────────────────────────────────────────────────

#[tauri::command]
fn get_mcp_servers() -> Result<String, String> {
    core::list_mcp_servers()
}

#[tauri::command]
fn list_mcp_server_configs() -> Result<Vec<String>, String> {
    core::list_mcp_server_configs()
}

#[tauri::command]
fn read_mcp_server_config(name: &str) -> Result<String, String> {
    core::read_mcp_server_config(name)
}

#[tauri::command]
fn save_mcp_server_config(name: &str, data: &str) -> Result<(), String> {
    core::save_mcp_server_config(name, data)?;
    sync_projects_referencing_mcp_server(name);
    Ok(())
}

#[tauri::command]
fn delete_mcp_server_config(name: &str) -> Result<(), String> {
    core::delete_mcp_server_config(name)?;
    prune_mcp_server_from_projects(name);
    Ok(())
}

#[tauri::command]
fn import_mcp_servers() -> Result<String, String> {
    let imported = core::import_mcp_servers_from_claude()?;
    if !imported.is_empty() {
        sync_projects_referencing_mcp_servers(&imported);
    }
    serde_json::to_string(&imported).map_err(|e| e.to_string())
}

// ── Projects ─────────────────────────────────────────────────────────────────

#[tauri::command]
fn get_projects() -> Result<Vec<String>, String> {
    core::list_projects()
}

#[tauri::command]
fn read_project(name: &str) -> Result<String, String> {
    core::read_project(name)
}

#[tauri::command]
fn autodetect_project_dependencies(name: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let updated = sync::autodetect_project_dependencies(&project)?;
    serde_json::to_string_pretty(&updated).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_project(name: &str, data: &str) -> Result<(), String> {
    let incoming: core::Project =
        serde_json::from_str(data).map_err(|e| format!("Invalid project data: {}", e))?;

    // No directory configured yet — just persist to the registry and return.
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
        // Nothing is deleted during this step — autodetect only adds findings.
        core::save_project(name, data)?;

        // Errors are intentionally swallowed: partial success (project saved
        // but no agent configs written because the directory has no AI tools)
        // is better than returning a hard error to the frontend.
        let _ = sync::sync_project(&incoming);
    } else {
        // ── Case 2 / ongoing saves: Existing project update ───────────────
        //
        // Use sync_without_autodetect so the user's explicit agent/skill
        // removals are respected — we never re-add an agent the user
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
                let discovered =
                    sync::discover_new_agent_mcp_configs(&dir, &new_agent_ids);

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

        let enriched_data = serde_json::to_string_pretty(&enriched)
            .map_err(|e| e.to_string())?;
        core::save_project(name, &enriched_data)?;

        if !enriched.agents.is_empty() {
            sync::sync_project_without_autodetect(&enriched)?;
        }
    }

    Ok(())
}

#[tauri::command]
fn rename_project(old_name: &str, new_name: &str) -> Result<(), String> {
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
fn delete_project(name: &str) -> Result<(), String> {
    core::delete_project(name)
}

// ── Project Sync ─────────────────────────────────────────────────────────────

#[tauri::command]
fn sync_project(name: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let written = sync::sync_project(&project)?;
    serde_json::to_string_pretty(&written).map_err(|e| e.to_string())
}

/// Return the list of file/directory paths that would be removed if the given
/// agent were removed from the project.  Read-only — used to populate the
/// confirmation dialog before the user commits to the removal.
#[tauri::command]
fn get_agent_cleanup_preview(name: &str, agent_id: &str) -> Result<String, String> {
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
fn remove_agent_from_project(name: &str, agent_id: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let mut project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let removed = sync::remove_agent_from_project(&mut project, agent_id)?;
    serde_json::to_string(&removed).map_err(|e| e.to_string())
}

/// Check whether the on-disk agent configs have drifted from what Nexus would
/// generate.  Returns a JSON-serialised [`sync::DriftReport`] describing which
/// agents and files are out of sync.  This is a read-only operation.
#[tauri::command]
fn check_project_drift(name: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let report = sync::check_project_drift(&project)?;
    serde_json::to_string(&report).map_err(|e| e.to_string())
}

fn with_each_project_mut<F>(mut f: F)
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

fn sync_project_if_configured(project_name: &str, project: &core::Project) {
    if project.directory.is_empty() || project.agents.is_empty() {
        return;
    }

    if let Err(e) = sync::sync_project_without_autodetect(project) {
        eprintln!("Failed to sync project '{}' after registry update: {}", project_name, e);
    }
}

fn sync_projects_referencing_skill(skill_name: &str) {
    with_each_project_mut(|project_name, project| {
        if project.skills.iter().any(|skill| skill == skill_name) {
            sync_project_if_configured(project_name, project);
        }
    });
}

fn sync_projects_referencing_mcp_server(server_name: &str) {
    with_each_project_mut(|project_name, project| {
        if project.mcp_servers.iter().any(|server| server == server_name) {
            sync_project_if_configured(project_name, project);
        }
    });
}

fn sync_projects_referencing_mcp_servers(server_names: &[String]) {
    with_each_project_mut(|project_name, project| {
        if project
            .mcp_servers
            .iter()
            .any(|server| server_names.iter().any(|name| name == server))
        {
            sync_project_if_configured(project_name, project);
        }
    });
}

fn prune_skill_from_projects(skill_name: &str) {
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

fn prune_mcp_server_from_projects(server_name: &str) {
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

fn sync_projects_referencing_rule(rule_name: &str) {
    with_each_project_mut(|project_name, project| {
        let references_rule = project
            .file_rules
            .values()
            .any(|rules| rules.iter().any(|r| r == rule_name));
        if references_rule {
            // Re-inject rules into any project files that use this rule
            for (filename, rules) in &project.file_rules {
                if rules.iter().any(|r| r == rule_name) {
                    let _ = core::inject_rules_into_project_file(
                        &project.directory,
                        filename,
                        rules,
                    );
                }
            }
            sync_project_if_configured(project_name, project);
        }
    });
}

fn prune_rule_from_projects(rule_name: &str) {
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
                let _ = core::inject_rules_into_project_file(
                    &project.directory,
                    filename,
                    rules,
                );
            }
            sync_project_if_configured(project_name, project);
        }
    });
}

// ── Local Skills ─────────────────────────────────────────────────────────

/// Import a local skill into the global registry and promote it to a normal
/// project skill.  Returns the updated project JSON.
#[tauri::command]
fn import_local_skill(name: &str, skill_name: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let updated = sync::import_local_skill(&project, skill_name)?;
    sync_project_if_configured(name, &updated);
    serde_json::to_string_pretty(&updated).map_err(|e| e.to_string())
}

/// Copy all local skills to every agent's skill directory in the project.
/// Returns the list of files written.
#[tauri::command]
fn sync_local_skills(name: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    let written = sync::sync_local_skills_across_agents(&project)?;
    serde_json::to_string_pretty(&written).map_err(|e| e.to_string())
}

// ── Skills Store ─────────────────────────────────────────────────────────────

#[tauri::command]
async fn search_remote_skills(query: String) -> Result<Vec<core::RemoteSkillResult>, String> {
    core::search_remote_skills(&query).await
}

#[tauri::command]
async fn fetch_remote_skill_content(source: String, name: String) -> Result<String, String> {
    core::fetch_remote_skill_content(&source, &name).await
}

/// Import a skill from skills.sh: save content + record its remote origin.
#[tauri::command]
async fn import_remote_skill(
    name: String,
    content: String,
    source: String,
    id: String,
) -> Result<(), String> {
    core::save_skill(&name, &content)?;
    core::record_skill_source(&name, &source, &id)?;
    sync_projects_referencing_skill(&name);
    Ok(())
}

/// Return all entries from ~/.automatic/skills.json as a JSON object.
#[tauri::command]
fn get_skill_sources() -> Result<String, String> {
    let registry = core::read_skill_sources()?;
    serde_json::to_string(&registry).map_err(|e| e.to_string())
}

// ── Plugins / Sessions ───────────────────────────────────────────────────────

#[tauri::command]
fn install_plugin_marketplace() -> Result<String, String> {
    core::install_plugin_marketplace()
}

#[tauri::command]
fn get_sessions() -> Result<String, String> {
    core::list_sessions()
}

// ── Memory ───────────────────────────────────────────────────────────────────

#[tauri::command]
fn get_project_memories(project: &str) -> Result<memory::MemoryDb, String> {
    memory::get_all_memories(project)
}

#[tauri::command]
fn store_memory(project: &str, key: &str, value: &str, source: Option<&str>) -> Result<String, String> {
    memory::store_memory(project, key, value, source)
}

#[tauri::command]
fn get_memory(project: &str, key: &str) -> Result<String, String> {
    memory::get_memory(project, key)
}

#[tauri::command]
fn list_memories(project: &str, pattern: Option<&str>) -> Result<String, String> {
    memory::list_memories(project, pattern)
}

#[tauri::command]
fn search_memories(project: &str, query: &str) -> Result<String, String> {
    memory::search_memories(project, query)
}

#[tauri::command]
fn delete_memory(project: &str, key: &str) -> Result<String, String> {
    memory::delete_memory(project, key)
}

#[tauri::command]
fn clear_memories(project: &str, pattern: Option<&str>, confirm: bool) -> Result<String, String> {
    memory::clear_memories(project, pattern, confirm)
}

// ── Editor Detection & Open ───────────────────────────────────────────────────

#[tauri::command]
fn check_installed_editors() -> Vec<core::EditorInfo> {
    core::check_installed_editors()
}

#[tauri::command]
fn open_in_editor(editor_id: &str, path: &str) -> Result<(), String> {
    core::open_in_editor(editor_id, path)
}

#[tauri::command]
fn get_editor_icon(editor_id: &str) -> Result<String, String> {
    core::get_editor_icon(editor_id)
}

// ── App Updates ───────────────────────────────────────────────────────────────

/// Restart the application to apply a freshly-installed update.
#[tauri::command]
fn restart_app(app: tauri::AppHandle) {
    app.restart();
}

// ── App Entry ────────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|_app| {
            // Ensure plugin marketplace exists on disk; register with Claude
            // Code if the CLI is available.  Runs on a background thread so
            // it never blocks the UI.
            std::thread::spawn(|| {
                if let Err(e) = core::install_default_skills() {
                    eprintln!("[automatic] skill install error: {}", e);
                }
                if let Err(e) = core::install_default_templates() {
                    eprintln!("[automatic] template install error: {}", e);
                }
                if let Err(e) = core::install_default_rules() {
                    eprintln!("[automatic] rule install error: {}", e);
                }
                match core::install_plugin_marketplace() {
                    Ok(msg) => eprintln!("[automatic] plugin startup: {}", msg),
                    Err(e) => eprintln!("[automatic] plugin startup error: {}", e),
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            read_profile,
            save_profile,
            read_settings,
            write_settings,
            save_api_key,
            get_api_key,
            list_agents,
            list_agents_with_projects,
            get_skills,
            read_skill,
            save_skill,
            delete_skill,
            sync_skill,
            sync_all_skills,
            get_templates,
            read_template,
            save_template,
            delete_template,
            get_rules,
            read_rule,
            save_rule,
            delete_rule,
            get_project_templates,
            read_project_template,
            save_project_template,
            delete_project_template,
            rename_project_template,
            list_bundled_project_templates,
            read_bundled_project_template,
            import_bundled_project_template,
            search_bundled_project_templates,
            get_project_file_info,
            read_project_file,
            save_project_file,
            get_mcp_servers,
            list_mcp_server_configs,
            read_mcp_server_config,
            save_mcp_server_config,
            delete_mcp_server_config,
            import_mcp_servers,

            get_projects,
            read_project,
            autodetect_project_dependencies,
            save_project,
            rename_project,
            delete_project,
            sync_project,
            get_agent_cleanup_preview,
            remove_agent_from_project,
            check_project_drift,
            import_local_skill,
            sync_local_skills,
            install_plugin_marketplace,
            get_sessions,
            search_remote_skills,
            fetch_remote_skill_content,
            import_remote_skill,
            get_skill_sources,
            get_project_memories,
            store_memory,
            get_memory,
            list_memories,
            search_memories,
            delete_memory,
            clear_memories,
            check_installed_editors,
            open_in_editor,
            get_editor_icon,
            restart_app,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
