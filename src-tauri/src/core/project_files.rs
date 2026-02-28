use std::fs;
use std::path::PathBuf;

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
