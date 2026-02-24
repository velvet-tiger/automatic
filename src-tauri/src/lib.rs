pub mod agent;
pub mod core;
pub mod mcp;
pub mod sync;

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

// ── Project Files ────────────────────────────────────────────────────────────

/// Returns JSON array of unique project file info objects for the project's agents.
/// Each entry: { filename, agents: ["Claude Code", ...] }
#[tauri::command]
fn get_project_file_info(name: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    let project_dir = std::path::Path::new(&project.directory);

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

    serde_json::to_string(&files).map_err(|e| e.to_string())
}

#[tauri::command]
fn read_project_file(name: &str, filename: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    core::read_project_file(&project.directory, filename)
}

#[tauri::command]
fn save_project_file(name: &str, filename: &str, content: &str) -> Result<(), String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;
    core::save_project_file(&project.directory, filename, content)
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
    core::save_project(name, data)?;

    let project: core::Project =
        serde_json::from_str(data).map_err(|e| format!("Invalid project data: {}", e))?;

    // Sync uses the same path as the explicit sync_project command (including
    // autodetect) so that save and sync are functionally identical.
    if !project.directory.is_empty() && !project.agents.is_empty() {
        sync::sync_project(&project)?;
    }

    Ok(())
}

#[tauri::command]
fn rename_project(old_name: &str, new_name: &str) -> Result<(), String> {
    core::rename_project(old_name, new_name)
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

/// Return all entries from ~/.nexus/skills.json as a JSON object.
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

// ── App Entry ────────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|_app| {
            // Ensure plugin marketplace exists on disk; register with Claude
            // Code if the CLI is available.  Runs on a background thread so
            // it never blocks the UI.
            std::thread::spawn(|| {
                if let Err(e) = core::install_default_templates() {
                    eprintln!("[nexus] template install error: {}", e);
                }
                match core::install_plugin_marketplace() {
                    Ok(msg) => eprintln!("[nexus] plugin startup: {}", msg),
                    Err(e) => eprintln!("[nexus] plugin startup error: {}", e),
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
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
            check_project_drift,
            import_local_skill,
            sync_local_skills,
            install_plugin_marketplace,
            get_sessions,
            search_remote_skills,
            fetch_remote_skill_content,
            import_remote_skill,
            get_skill_sources,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
