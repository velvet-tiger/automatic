use std::fs;
use std::path::PathBuf;

use super::*;

// ── Rules Injection ─────────────────────────────────────────────────────────

const RULES_START_MARKER: &str = "<!-- automatic:rules:start -->";
const RULES_END_MARKER: &str = "<!-- automatic:rules:end -->";

/// Public wrapper for `strip_rules_section` (used by sync).
pub fn strip_rules_section_pub(content: &str) -> String {
    strip_rules_section(content)
}

/// Strip the `<!-- automatic:rules:start -->...<!-- automatic:rules:end -->` section.
pub(crate) fn strip_rules_section(content: &str) -> String {
    if let (Some(start), Some(end)) = (
        content.find(RULES_START_MARKER),
        content.find(RULES_END_MARKER),
    ) {
        let before = &content[..start];
        let after = &content[end + RULES_END_MARKER.len()..];
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

/// Build the rules section content from a list of rule machine names.
pub fn build_rules_section(rule_names: &[String]) -> Result<String, String> {
    if rule_names.is_empty() {
        return Ok(String::new());
    }

    let mut parts = Vec::new();
    for machine_name in rule_names {
        match read_rule_content(machine_name) {
            Ok(content) if !content.trim().is_empty() => {
                parts.push(content);
            }
            _ => {
                // Skip missing or empty rules silently
            }
        }
    }

    if parts.is_empty() {
        return Ok(String::new());
    }

    let mut section = String::new();
    section.push_str(RULES_START_MARKER);
    section.push('\n');
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            section.push('\n');
        }
        section.push_str(part.trim());
        section.push('\n');
    }
    section.push_str(RULES_END_MARKER);

    Ok(section)
}

/// Write a project file with rules appended.  The user content is written
/// first, then any rules configured for this file are appended inside markers.
pub fn save_project_file_with_rules(
    directory: &str,
    filename: &str,
    user_content: &str,
    rule_names: &[String],
) -> Result<(), String> {
    let rules_section = build_rules_section(rule_names)?;

    let full_content = if rules_section.is_empty() {
        user_content.to_string()
    } else {
        format!("{}\n\n{}\n", user_content.trim_end(), rules_section)
    };

    save_project_file(directory, filename, &full_content)
}

/// Read-only check: returns `true` if the on-disk file already contains the
/// rules section that would be generated from the given rule names.
/// Used to show green/yellow status in the Rules UI without writing anything.
pub fn is_project_file_rules_current(
    directory: &str,
    filename: &str,
    rule_names: &[String],
) -> Result<bool, String> {
    if directory.is_empty() {
        return Ok(true);
    }

    let path = PathBuf::from(directory).join(filename);
    if !path.exists() {
        // File doesn't exist yet — not current.
        return Ok(false);
    }

    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let user_content = strip_rules_section(&strip_managed_section(&raw));

    let rules_section = build_rules_section(rule_names)?;

    let expected = if rules_section.is_empty() {
        user_content
    } else {
        format!("{}\n\n{}\n", user_content.trim_end(), rules_section)
    };

    Ok(expected == raw)
}

/// Re-inject rules into an existing project file.  Reads the file, strips
/// any existing rules section, rebuilds it from the provided rule names,
/// and writes back.  Used during sync to keep rules current.
pub fn inject_rules_into_project_file(
    directory: &str,
    filename: &str,
    rule_names: &[String],
) -> Result<bool, String> {
    if directory.is_empty() {
        return Ok(false);
    }

    let path = PathBuf::from(directory).join(filename);
    if !path.exists() {
        return Ok(false);
    }

    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let user_content = strip_rules_section(&strip_managed_section(&raw));

    let rules_section = build_rules_section(rule_names)?;

    let full_content = if rules_section.is_empty() {
        user_content.clone()
    } else {
        format!("{}\n\n{}\n", user_content.trim_end(), rules_section)
    };

    // Only write if content actually changed
    if full_content != raw {
        fs::write(&path, full_content).map_err(|e| e.to_string())?;
        Ok(true)
    } else {
        Ok(false)
    }
}
