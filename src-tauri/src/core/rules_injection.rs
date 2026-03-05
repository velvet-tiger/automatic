use std::collections::HashSet;
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
/// exact rules section that would be generated from the given rule names.
/// Only compares the rules section — ignores user content and managed sections.
pub fn is_file_rules_current(
    path: &std::path::Path,
    rule_names: &[String],
) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }

    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let expected_section = build_rules_section(rule_names)?;

    if expected_section.is_empty() {
        // No rules expected — current if the file has no rules section.
        return Ok(!raw.contains(RULES_START_MARKER));
    }

    // Check if the file contains the exact expected rules section.
    Ok(raw.contains(&expected_section))
}

// ── .claude/rules/ directory-based rules ────────────────────────────────────

/// Write rules as individual Markdown files under `<project_dir>/.claude/rules/`.
///
/// This is the format recommended by the Claude Code documentation:
/// each rule becomes a file named `<machine_name>.md` inside `.claude/rules/`.
/// Files managed by Automatic are given a comment header; files that do not
/// belong to the current rule set (and were previously written by Automatic)
/// are deleted so the directory stays in sync with the project's rule list.
///
/// Returns the list of files written or removed.
pub fn sync_rules_to_dot_claude_rules(
    project_dir: &str,
    rule_names: &[String],
) -> Result<Vec<String>, String> {
    let rules_dir = PathBuf::from(project_dir).join(".claude").join("rules");
    fs::create_dir_all(&rules_dir)
        .map_err(|e| format!("Failed to create .claude/rules/: {}", e))?;

    let mut touched: Vec<String> = Vec::new();

    // Build the set of filenames we intend to write so we can remove stale ones.
    let intended: HashSet<String> = rule_names.iter().map(|n| format!("{}.md", n)).collect();

    // Remove files that were previously managed by Automatic but are no
    // longer in the rule list.  We only remove files whose first line
    // carries the Automatic-managed marker so we never clobber user files.
    if let Ok(entries) = fs::read_dir(&rules_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let file_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            if intended.contains(&file_name) {
                continue; // Will be (re-)written below — leave it for now.
            }
            // Only remove files that carry our managed header.
            if let Ok(content) = fs::read_to_string(&path) {
                if content.starts_with(CLAUDE_RULES_MANAGED_HEADER) {
                    if fs::remove_file(&path).is_ok() {
                        touched.push(path.display().to_string());
                    }
                }
            }
        }
    }

    // Write each rule as `<machine_name>.md`.
    for machine_name in rule_names {
        let content = match read_rule_content(machine_name) {
            Ok(c) if !c.trim().is_empty() => c,
            _ => continue, // Skip missing or empty rules silently.
        };

        let file_path = rules_dir.join(format!("{}.md", machine_name));
        let file_content = format!("{}{}\n", CLAUDE_RULES_MANAGED_HEADER, content.trim_end());

        // Only write if different from what is already on disk.
        let existing = fs::read_to_string(&file_path).unwrap_or_default();
        if existing != file_content {
            fs::write(&file_path, &file_content)
                .map_err(|e| format!("Failed to write rule '{}': {}", machine_name, e))?;
            touched.push(file_path.display().to_string());
        }
    }

    Ok(touched)
}

/// Marker placed at the very start of every rule file written by Automatic.
/// Used to identify files that are safe to delete on cleanup.
const CLAUDE_RULES_MANAGED_HEADER: &str = "<!-- managed by Automatic — do not edit by hand -->\n\n";

/// Read-only check: returns `true` if the on-disk `.claude/rules/` directory
/// already contains exactly the files that would be generated from `rule_names`
/// (same filenames, same content, no extra Automatic-managed files).
pub fn is_dot_claude_rules_current(
    project_dir: &str,
    rule_names: &[String],
) -> Result<bool, String> {
    let rules_dir = PathBuf::from(project_dir).join(".claude").join("rules");

    // Collect currently-managed files (those with our header).
    let mut managed_on_disk: HashSet<String> = HashSet::new();
    if let Ok(entries) = fs::read_dir(&rules_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&path) {
                if content.starts_with(CLAUDE_RULES_MANAGED_HEADER) {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        managed_on_disk.insert(name.to_string());
                    }
                }
            }
        }
    }

    let intended: HashSet<String> = rule_names.iter().map(|n| format!("{}.md", n)).collect();

    // Extra managed files that shouldn't be there.
    if managed_on_disk != intended {
        return Ok(false);
    }

    // Check content of each intended file.
    for machine_name in rule_names {
        let content = match read_rule_content(machine_name) {
            Ok(c) if !c.trim().is_empty() => c,
            _ => continue,
        };
        let expected = format!("{}{}\n", CLAUDE_RULES_MANAGED_HEADER, content.trim_end());
        let actual = match fs::read_to_string(rules_dir.join(format!("{}.md", machine_name))) {
            Ok(c) => c,
            Err(_) => return Ok(false),
        };
        if actual != expected {
            return Ok(false);
        }
    }

    Ok(true)
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
