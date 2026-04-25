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
            let exists = project_dir.join(&filename).is_file();

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
                if path.is_file() {
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
    let touched_files = core::resolve_instruction_target_filenames(&project, filename);

    core::save_project_file_for_project(&project, filename, content)?;

    // Record only the files we actually wrote so unrelated conflicts remain visible.
    core::record_instruction_hashes_for_filenames(name, &mut project, &touched_files);
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
    let touched_files = core::resolve_instruction_target_filenames(&project, filename);

    // read_project_file strips managed sections and returns only user content.
    let user_content = core::read_project_file(&project.directory, filename)?;

    // Re-write through the standard path so rules are correctly applied.
    core::save_project_file_for_project(&project, filename, &user_content)?;

    // Record only the files we actually wrote so unrelated conflicts remain visible.
    core::record_instruction_hashes_for_filenames(name, &mut project, &touched_files);

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
    let touched_files = core::resolve_instruction_target_filenames(&project, filename);
    let automatic_content = core::resolve_instruction_snapshot_content(&project, filename);

    // Restore the user-authored content Automatic last stored for this file.
    // If no snapshot exists, this falls back to an empty user-content file.
    core::save_project_file_for_project(&project, filename, &automatic_content)?;

    // Record only the files we actually wrote so unrelated conflicts remain visible.
    core::record_instruction_hashes_for_filenames(name, &mut project, &touched_files);
    Ok(())
}

/// Per-agent instruction file candidate returned by
/// [`inspect_unified_candidates`] for the unified-mode source picker.
#[derive(serde::Serialize)]
pub struct UnifiedCandidate {
    /// Concrete instruction filename (e.g. `"AGENTS.md"`, `"CLAUDE.md"`).
    pub filename: String,
    /// Agent labels whose primary instruction file is this filename.
    pub agent_labels: Vec<String>,
    /// User-authored content currently on disk (managed sections stripped).
    /// Empty string if the file does not exist on disk.
    pub user_content: String,
    /// True if the file exists on disk.
    pub exists: bool,
    /// Last-modified time as milliseconds since the Unix epoch, or `None` if
    /// the file does not exist or its mtime cannot be read.
    pub modified_ms: Option<u64>,
}

/// Result of inspecting the per-agent files in preparation for switching to
/// unified instruction mode.
#[derive(serde::Serialize)]
pub struct UnifiedInspection {
    /// Per-agent files that would be unified, in agent registration order.
    pub candidates: Vec<UnifiedCandidate>,
    /// True if every existing file's user content matches (whitespace-trimmed).
    /// When this is true the frontend can switch silently; otherwise it must
    /// ask the user which file's content should become the unified source.
    pub consistent: bool,
}

/// Inspect a project's per-agent instruction files in preparation for
/// switching to unified mode.
///
/// Returns each unique agent file with its user-authored content (managed
/// sections stripped) and a `consistent` flag.  When `consistent` is `false`
/// the on-disk files have diverged and the frontend must present a picker so
/// the user can choose which file becomes the canonical unified source —
/// switching silently would let one file's content silently overwrite
/// another's via the next save.
#[tauri::command]
pub fn inspect_unified_candidates(name: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    let empty = UnifiedInspection {
        candidates: vec![],
        consistent: true,
    };

    if project.directory.is_empty() {
        return serde_json::to_string(&empty).map_err(|e| e.to_string());
    }

    let project_dir = std::path::PathBuf::from(&project.directory);
    let mut candidates: Vec<UnifiedCandidate> = Vec::new();

    for agent_id in &project.agents {
        let Some(a) = agent::from_id(agent_id) else { continue };
        if !a.capabilities().instructions {
            continue;
        }
        let filename = a.project_file_name().to_string();
        let label = a.label().to_string();

        if let Some(existing) = candidates.iter_mut().find(|c| c.filename == filename) {
            if !existing.agent_labels.iter().any(|l| l == &label) {
                existing.agent_labels.push(label);
            }
            continue;
        }

        let path = project_dir.join(&filename);
        let exists = path.is_file();
        let user_content = if exists {
            core::read_project_file(&project.directory, &filename).unwrap_or_default()
        } else {
            String::new()
        };
        let modified_ms = if exists {
            path.metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
        } else {
            None
        };

        candidates.push(UnifiedCandidate {
            filename,
            agent_labels: vec![label],
            user_content,
            exists,
            modified_ms,
        });
    }

    let non_empty: Vec<&str> = candidates
        .iter()
        .filter(|c| c.exists && !c.user_content.trim().is_empty())
        .map(|c| c.user_content.trim())
        .collect();
    let consistent = non_empty.len() < 2 || non_empty.iter().all(|c| *c == non_empty[0]);

    serde_json::to_string(&UnifiedInspection {
        candidates,
        consistent,
    })
    .map_err(|e| e.to_string())
}

/// Switch a project to unified instruction mode.
///
/// When `source_filename` matches one of the project's agent instruction
/// files, that file's user-authored content is propagated to **every** agent
/// instruction file — overwriting any divergent content elsewhere — so the
/// project starts unified mode in a consistent state.  Pass an empty string
/// (or any filename that does not match an agent file) to skip propagation;
/// only the `instruction_mode` flag is updated and existing files are left
/// untouched.  The frontend uses the empty path when it has already verified
/// that the per-agent files agree, and the picker path when they disagree.
#[tauri::command]
pub fn switch_to_unified_mode(name: &str, source_filename: &str) -> Result<(), String> {
    let raw = core::read_project(name)?;
    let mut project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    project.instruction_mode = "unified".to_string();
    project.updated_at = chrono::Utc::now().to_rfc3339();
    let updated_json = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;
    core::save_project(name, &updated_json)?;

    let agent_filenames = core::collect_agent_filenames(&project);
    if !source_filename.is_empty() && agent_filenames.iter().any(|f| f == source_filename) {
        let user_content = core::read_project_file(&project.directory, source_filename)?;
        let touched = core::resolve_instruction_target_filenames(&project, "_unified");
        core::save_project_file_for_project(&project, "_unified", &user_content)?;
        core::record_instruction_hashes_for_filenames(name, &mut project, &touched);
    }

    Ok(())
}

/// Use AI to analyse the project directory and generate a starter markdown body
/// for an agent instruction file (e.g. `CLAUDE.md` or `AGENTS.md`).
///
/// The generated text covers only the **user-authored section** — it does not
/// include Automatic-managed skill/rules blocks.  The frontend previews the
/// result in the editor before the user calls `save_project_file`.
///
/// `filename` is either a concrete filename (e.g. `"CLAUDE.md"`) or the
/// virtual key `"_unified"` for unified-mode projects.  In unified mode the
/// prompt notes that the file will be shared across all configured agents.
#[tauri::command]
pub async fn ai_generate_instruction(name: &str, filename: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    if project.directory.is_empty() {
        return Err("Project has no directory configured".into());
    }

    // Build a project snapshot the same way context generation does so the AI
    // has a rich, factual view of the repository without needing tool calls.
    let snapshot = crate::context::build_project_snapshot(&project.directory)?;

    // Optionally include any already-generated context.json so the AI can
    // reference discovered commands, conventions, and concepts.
    let context_snippet = {
        let ctx_path = std::path::PathBuf::from(&project.directory)
            .join(".automatic")
            .join("context.json");
        if ctx_path.exists() {
            match std::fs::read_to_string(&ctx_path) {
                Ok(s) => format!("\n\n<context_json>\n{}\n</context_json>", s),
                Err(_) => String::new(),
            }
        } else {
            String::new()
        }
    };

    // Describe which agent(s) will read this file so the AI can tailor wording.
    let agent_context = if filename == "_unified" {
        let labels: Vec<String> = project
            .agents
            .iter()
            .filter_map(|id| crate::agent::from_id(id).map(|a| a.label().to_string()))
            .collect();
        if labels.is_empty() {
            "all configured AI coding agents".to_string()
        } else {
            format!("all configured AI coding agents ({})", labels.join(", "))
        }
    } else {
        // Attempt to map filename → agent label for a friendlier prompt.
        let label = project
            .agents
            .iter()
            .filter_map(|id| crate::agent::from_id(id))
            .find(|a| a.project_file_name() == filename)
            .map(|a| a.label().to_string())
            .unwrap_or_else(|| filename.to_string());
        label
    };

    let existing_content =
        core::read_project_file(&project.directory, filename).unwrap_or_default();
    let existing_section = if existing_content.trim().is_empty() {
        String::new()
    } else {
        format!(
            "\n\n<existing_user_content>\n{}\n</existing_user_content>\n\
             The user has already written the above content. Improve and expand it — \
             do not discard sections they have already authored.",
            existing_content
        )
    };

    let system = "You are a senior software engineer writing an AI coding agent instruction \
        file for a real software project. Your output is the raw Markdown content \
        of the file — no preamble, no code fences, no meta-commentary. \
        Structure the document with clear ## headings. Cover: \
        (1) Project overview — what the project does and its primary tech stack. \
        (2) Build & run commands. \
        (3) Architecture overview — key modules/directories and what they do. \
        (4) Coding conventions — naming, error handling, typing, style patterns observed in the code. \
        (5) Agent guidance — what the agent should and should not do (e.g. always run tests, \
            never commit secrets, ask before deleting files). \
        Keep each section concise (3–8 bullet points or 2–4 sentences). \
        Use only information evidenced by the snapshot; do not invent facts.";

    let user_msg = format!(
        "Generate a project instruction file for **{}**.\n\
         This file will be read by {}.\n\
         Project name: \"{}\"\n\n\
         Project snapshot:\n{}{}{}\n\n\
         Write the full Markdown content of the instruction file now.",
        filename, agent_context, name, snapshot, context_snippet, existing_section
    );

    crate::core::ai::chat(
        vec![crate::core::ai::AiMessage {
            role: "user".into(),
            content: user_msg,
        }],
        None,
        None,
        Some(system.to_string()),
        Some(4096),
    )
    .await
}

/// Use AI to update an **existing** agent instruction file in-place.
///
/// Unlike `ai_generate_instruction` — which produces a fresh document from
/// scratch and may discard nuance the user has added — this command takes the
/// current content and asks the model to refine it while preserving every
/// deliberate decision the user has already made.
///
/// The prompt explicitly instructs the model to:
/// 1. Keep all sections that are already correct.
/// 2. Update stale facts (e.g. commands, paths, version references) based on
///    the latest project snapshot.
/// 3. Fill in obvious gaps discovered from the snapshot but not yet documented.
/// 4. Maintain the existing structure, tone, and level of detail.
///
/// `filename` and `current_content` must both be supplied by the caller.
/// `current_content` is the raw user-authored Markdown currently in the editor
/// (excluding any Automatic-managed skill/rules blocks).
#[tauri::command]
pub async fn ai_update_instruction(
    name: &str,
    filename: &str,
    current_content: &str,
) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    if project.directory.is_empty() {
        return Err("Project has no directory configured".into());
    }

    if current_content.trim().is_empty() {
        return Err(
            "No existing content to update — use Generate to create the file from scratch".into(),
        );
    }

    let snapshot = crate::context::build_project_snapshot(&project.directory)?;

    let context_snippet = {
        let ctx_path = std::path::PathBuf::from(&project.directory)
            .join(".automatic")
            .join("context.json");
        if ctx_path.exists() {
            match std::fs::read_to_string(&ctx_path) {
                Ok(s) => format!("\n\n<context_json>\n{}\n</context_json>", s),
                Err(_) => String::new(),
            }
        } else {
            String::new()
        }
    };

    let agent_context = if filename == "_unified" {
        let labels: Vec<String> = project
            .agents
            .iter()
            .filter_map(|id| crate::agent::from_id(id).map(|a| a.label().to_string()))
            .collect();
        if labels.is_empty() {
            "all configured AI coding agents".to_string()
        } else {
            format!("all configured AI coding agents ({})", labels.join(", "))
        }
    } else {
        project
            .agents
            .iter()
            .filter_map(|id| crate::agent::from_id(id))
            .find(|a| a.project_file_name() == filename)
            .map(|a| a.label().to_string())
            .unwrap_or_else(|| filename.to_string())
    };

    let system = "You are a senior software engineer updating an existing AI coding agent \
        instruction file for a real software project. \
        Your output is the raw Markdown content of the updated file — \
        no preamble, no code fences, no meta-commentary. \
        Rules you must follow: \
        (1) Preserve every section heading and deliberate decision the user has already authored. \
        (2) Correct any facts that have become stale (commands, file paths, version numbers, \
            module names) based on the project snapshot provided. \
        (3) Add concise new content only where genuine gaps exist — do not pad or repeat. \
        (4) Do not remove or rewrite sections simply to show changes; stability is valuable. \
        (5) Maintain the existing tone and level of detail. \
        Use only information evidenced by the snapshot; do not invent facts.";

    let user_msg = format!(
        "Update the existing project instruction file for **{}**.\n\
         This file is read by {}.\n\
         Project name: \"{}\"\n\n\
         <current_content>\n{}\n</current_content>\n\n\
         Project snapshot (latest state of the repository):\n{}{}\n\n\
         Return the complete updated Markdown content of the instruction file.",
        filename, agent_context, name, current_content, snapshot, context_snippet
    );

    crate::core::ai::chat(
        vec![crate::core::ai::AiMessage {
            role: "user".into(),
            content: user_msg,
        }],
        None,
        None,
        Some(system.to_string()),
        Some(4096),
    )
    .await
}

// ── Doc Notes ────────────────────────────────────────────────────────────────

/// Read the contents of a Markdown note stored in `{project_dir}/.automatic/docs/<name>.md`.
///
/// Returns an empty string if the file does not exist yet (so the frontend
/// can treat it as a new note without an extra existence check).
#[tauri::command]
pub fn read_doc_note(name: &str, note_name: &str) -> Result<String, String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    if project.directory.is_empty() {
        return Err("Project has no directory configured".into());
    }

    let note_path = std::path::PathBuf::from(&project.directory)
        .join(".automatic")
        .join("docs")
        .join(note_name);

    if !note_path.exists() {
        return Ok(String::new());
    }

    std::fs::read_to_string(&note_path).map_err(|e| format!("Failed to read note: {}", e))
}

/// Write a Markdown note to `{project_dir}/.automatic/docs/<name>.md`.
///
/// Creates the `.automatic/docs/` directory if it does not exist.
#[tauri::command]
pub fn save_doc_note(name: &str, note_name: &str, content: &str) -> Result<(), String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    if project.directory.is_empty() {
        return Err("Project has no directory configured".into());
    }

    let docs_dir = std::path::PathBuf::from(&project.directory)
        .join(".automatic")
        .join("docs");

    std::fs::create_dir_all(&docs_dir)
        .map_err(|e| format!("Failed to create docs directory: {}", e))?;

    let note_path = docs_dir.join(note_name);
    std::fs::write(&note_path, content).map_err(|e| format!("Failed to save note: {}", e))
}

/// Delete a Markdown note file from `{project_dir}/.automatic/docs/<name>.md`.
///
/// Returns `Ok(())` if the file did not exist (idempotent).
#[tauri::command]
pub fn delete_doc_note(name: &str, note_name: &str) -> Result<(), String> {
    let raw = core::read_project(name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    if project.directory.is_empty() {
        return Err("Project has no directory configured".into());
    }

    let note_path = std::path::PathBuf::from(&project.directory)
        .join(".automatic")
        .join("docs")
        .join(note_name);

    if note_path.exists() {
        std::fs::remove_file(&note_path).map_err(|e| format!("Failed to delete note: {}", e))?;
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Project;
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn with_temp_home<T>(test: impl FnOnce(&Path) -> T) -> T {
        let _lock = env_lock().lock().expect("env lock");
        let home = tempdir().expect("temp home");
        let original = std::env::var("HOME").ok();
        unsafe {
            std::env::set_var("HOME", home.path());
        }

        let result = test(home.path());

        unsafe {
            if let Some(value) = original {
                std::env::set_var("HOME", value);
            } else {
                std::env::remove_var("HOME");
            }
        }

        result
    }

    #[test]
    fn overwrite_instruction_file_restores_snapshot_content() {
        with_temp_home(|_| {
            let project_dir = tempdir().expect("project dir");
            let project = Project {
                name: "test-project".to_string(),
                directory: project_dir.path().display().to_string(),
                agents: vec!["opencode".to_string()],
                ..Default::default()
            };

            let project_json = serde_json::to_string_pretty(&project).expect("project json");
            crate::core::save_project(&project.name, &project_json).expect("save project");

            crate::core::save_instruction_snapshot(
                project_dir.path().to_str().unwrap(),
                "AGENTS.md",
                "# Stored Instructions\n\nKeep this.",
            )
            .expect("save snapshot");
            std::fs::write(
                project_dir.path().join("AGENTS.md"),
                "# Different\n\nDisk content",
            )
            .expect("write disk content");

            overwrite_instruction_file(&project.name, "AGENTS.md").expect("overwrite");

            let on_disk =
                std::fs::read_to_string(project_dir.path().join("AGENTS.md")).expect("read file");
            assert!(
                on_disk.contains("# Stored Instructions"),
                "overwrite should restore Automatic's stored snapshot content"
            );
            assert!(
                on_disk.contains("Keep this."),
                "restored file should contain the stored user-authored content"
            );
        });
    }
}
