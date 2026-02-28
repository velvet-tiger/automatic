use crate::agent;
use crate::core;

// ── Project Files ────────────────────────────────────────────────────────────

/// Returns JSON array of unique project file info objects for the project's agents.
/// Each entry: { filename, agents: ["Claude Code", ...] }
#[tauri::command]
pub fn get_project_file_info(name: &str) -> Result<String, String> {
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
pub fn read_project_file(name: &str, filename: &str) -> Result<String, String> {
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
        // No file exists yet -- return empty
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
pub fn save_project_file(name: &str, filename: &str, content: &str) -> Result<(), String> {
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
