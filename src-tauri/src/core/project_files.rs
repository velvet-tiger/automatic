use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use crate::agent;

use super::*;

// ── Project Files ────────────────────────────────────────────────────────────

/// Read a project file from the project's directory, stripping any
/// Automatic-managed sections (skills markers) and rules sections.  Returns
/// the user-authored content only.
pub fn read_project_file(directory: &str, filename: &str) -> Result<String, String> {
    if directory.is_empty() {
        return Err("Project has no directory configured".into());
    }

    let path = PathBuf::from(directory).join(filename);
    if !path.exists() {
        return Ok(String::new());
    }

    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    Ok(strip_rules_section(&strip_managed_section(&content)))
}

/// Write a project file to the project's directory.  Writes exactly what the
/// user provides — the sync process will re-merge any managed sections (skills)
/// on the next sync run.
pub fn save_project_file(directory: &str, filename: &str, content: &str) -> Result<(), String> {
    if directory.is_empty() {
        return Err("Project has no directory configured".into());
    }

    let dir = PathBuf::from(directory);
    if !dir.exists() {
        return Err(format!("Directory '{}' does not exist", directory));
    }

    let path = dir.join(filename);
    fs::write(&path, content).map_err(|e| e.to_string())
}

/// Public wrapper for `strip_managed_section` (used by sync).
pub fn strip_managed_section_pub(content: &str) -> String {
    strip_managed_section(content)
}

/// Strip the `<!-- automatic:skills:start -->...<!-- automatic:skills:end -->` section.
pub(crate) fn strip_managed_section(content: &str) -> String {
    let start_marker = "<!-- automatic:skills:start -->";
    let end_marker = "<!-- automatic:skills:end -->";

    if let (Some(start), Some(end)) = (content.find(start_marker), content.find(end_marker)) {
        let before = &content[..start];
        let after = &content[end + end_marker.len()..];
        let result = format!("{}{}", before.trim_end(), after.trim_start());
        if result.trim().is_empty() {
            String::new()
        } else {
            result
        }
    } else {
        content.to_string()
    }
}

// ── Instruction file hash tracking ──────────────────────────────────────────

/// Compute a deterministic hash of file content.  Used to detect external
/// modifications to instruction files.
pub fn compute_content_hash(content: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Read all instruction files for a project's agents from disk and return a
/// map of `filename → hash(full_content)`.  Only files that exist on disk are
/// included.
pub fn compute_instruction_hashes(project: &Project) -> HashMap<String, String> {
    let mut hashes = HashMap::new();
    if project.directory.is_empty() {
        return hashes;
    }

    let dir = PathBuf::from(&project.directory);
    if !dir.exists() {
        return hashes;
    }

    let mut seen = std::collections::HashSet::new();
    for agent_id in &project.agents {
        if let Some(a) = agent::from_id(agent_id) {
            if !a.capabilities().instructions {
                continue;
            }
            let filename = a.project_file_name().to_string();
            if seen.contains(&filename) {
                continue;
            }
            seen.insert(filename.clone());

            let path = dir.join(&filename);
            if let Ok(content) = fs::read_to_string(&path) {
                hashes.insert(filename, compute_content_hash(&content));
            }
        }
    }

    hashes
}

/// Record the current on-disk hashes for all instruction files into the
/// project's config and persist to the registry.  Call after any operation
/// that writes instruction files.
pub fn record_instruction_hashes(project_name: &str, project: &mut Project) {
    project.instruction_file_hashes = compute_instruction_hashes(project);

    // Persist updated hashes to the registry.
    if let Ok(data) = serde_json::to_string_pretty(project) {
        let _ = save_project(project_name, &data);
    }
}
