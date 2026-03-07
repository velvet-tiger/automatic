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
        // In unified mode, collect all existing agent files and pick the one
        // with the most recently modified timestamp.  This ensures that if the
        // user edits one file externally, the unified view shows the updated
        // content rather than stale content from an arbitrary first file.
        let project_dir = std::path::Path::new(&project.directory);
        let mut candidates: Vec<(String, std::time::SystemTime)> = Vec::new();

        let mut seen = std::collections::HashSet::new();
        for agent_id in &project.agents {
            if let Some(a) = agent::from_id(agent_id) {
                let f = a.project_file_name().to_string();
                if seen.contains(&f) {
                    continue;
                }
                seen.insert(f.clone());

                let path = project_dir.join(&f);
                if path.exists() {
                    let mtime = path
                        .metadata()
                        .and_then(|m| m.modified())
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                    candidates.push((f, mtime));
                }
            }
        }

        if candidates.is_empty() {
            return Ok(String::new());
        }

        // Pick the most recently modified file so external edits are visible.
        candidates.sort_by(|a, b| b.1.cmp(&a.1));
        let best = &candidates[0].0;
        core::read_project_file(&project.directory, best)
    } else {
        core::read_project_file(&project.directory, filename)
    }
}

#[tauri::command]
pub fn save_project_file(name: &str, filename: &str, content: &str) -> Result<(), String> {
    let raw = core::read_project(name)?;
    let mut project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    core::save_project_file_for_project(&project, filename, content)?;

    // Record updated hashes so drift detection reflects what we just wrote.
    core::record_instruction_hashes(name, &mut project);
    Ok(())
}

/// Adopt the current on-disk content of an instruction file into Automatic's
/// editor.  This is a no-op write: the file is read, its user-authored content
/// is extracted (stripping Automatic-managed sections), and then re-written
/// through the normal save path so that managed sections are correctly
/// re-applied.  After this call the file is considered in sync and the
/// conflict is resolved.
///
/// Call this when the user chooses "Use existing file" in the conflict
/// resolution UI.
#[tauri::command]
pub fn adopt_instruction_file(name: &str, filename: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let mut project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    // read_project_file strips managed sections and returns only user content.
    let user_content = core::read_project_file(&project.directory, filename)?;

    // Re-write through the standard path so rules are correctly applied.
    core::save_project_file_for_project(&project, filename, &user_content)?;

    // Record updated hashes so drift detection reflects what we just wrote.
    core::record_instruction_hashes(name, &mut project);

    // Return the adopted user content so the frontend can update its editor state.
    Ok(user_content)
}

/// Overwrite an instruction file with Automatic's stored content (empty user
/// content plus any configured rules).  This erases any content that was
/// manually added outside of Automatic.
///
/// Call this when the user chooses "Overwrite with Automatic content" in the
/// conflict resolution UI.
#[tauri::command]
pub fn overwrite_instruction_file(name: &str, filename: &str) -> Result<(), String> {
    let raw = core::read_project(name)?;
    let mut project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    // Write an empty user-content file with the configured rules re-applied.
    core::save_project_file_for_project(&project, filename, "")?;

    // Record updated hashes so drift detection reflects what we just wrote.
    core::record_instruction_hashes(name, &mut project);
    Ok(())
}

/// Returns the list of instruction file conflicts for a project — files that
/// exist on disk with user content that differs from what Automatic has stored.
/// Serialised as a JSON array of [`InstructionFileConflict`] objects.
#[tauri::command]
pub fn get_instruction_file_conflicts(name: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    let dir = std::path::PathBuf::from(&project.directory);
    if project.directory.is_empty() || !dir.exists() {
        return serde_json::to_string(&[] as &[crate::sync::InstructionFileConflict])
            .map_err(|e| e.to_string());
    }

    let conflicts = crate::sync::collect_instruction_conflicts_pub(&project, &dir);
    serde_json::to_string(&conflicts).map_err(|e| e.to_string())
}
