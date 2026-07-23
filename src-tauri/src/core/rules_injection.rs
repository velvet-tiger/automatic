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

/// Build the rules section content from a list of rule machine names plus any
/// inline custom rule content strings.  Both sources are combined in order:
/// global rules first, then custom rules.
pub fn build_rules_section(rule_names: &[String]) -> Result<String, String> {
    build_rules_section_with_custom(rule_names, &[])
}

/// Build the rules section from global rule machine names and inline custom
/// rule content strings.  Either slice may be empty.
pub fn build_rules_section_with_custom(
    rule_names: &[String],
    custom_contents: &[String],
) -> Result<String, String> {
    let mut parts: Vec<String> = Vec::new();

    // Global rules resolved from the registry
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

    // Inline custom rules stored directly in the project
    for content in custom_contents {
        if !content.trim().is_empty() {
            parts.push(content.clone());
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
    save_project_file_with_rules_and_custom(directory, filename, user_content, rule_names, &[])
}

/// Write a project file with both global and inline custom rules appended.
pub fn save_project_file_with_rules_and_custom(
    directory: &str,
    filename: &str,
    user_content: &str,
    rule_names: &[String],
    custom_contents: &[String],
) -> Result<(), String> {
    let rules_section = build_rules_section_with_custom(rule_names, custom_contents)?;

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
    is_file_rules_current_with_custom(path, rule_names, &[])
}

/// Read-only check including custom rule contents.
pub fn is_file_rules_current_with_custom(
    path: &std::path::Path,
    rule_names: &[String],
    custom_contents: &[String],
) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }

    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let expected_section = build_rules_section_with_custom(rule_names, custom_contents)?;

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

// ── .cursor/rules/ MDC-based rules ──────────────────────────────────────────

/// Frontmatter key marking an `.mdc` rule file as Automatic-managed.
///
/// Unlike `.claude/rules/*.md`, an HTML comment before the `---` frontmatter
/// would break Cursor's MDC parsing, so ownership is declared *inside* the
/// frontmatter — consistent with the `automatic-managed: true` convention used
/// for command files.
const CURSOR_MDC_MANAGED_KEY: &str = "automatic-managed: true";

/// Returns `true` if `.mdc` content carries the Automatic-managed frontmatter key.
pub(crate) fn is_managed_mdc_content(content: &str) -> bool {
    let Some(rest) = content.strip_prefix("---\n") else {
        return false;
    };
    let Some(end) = rest.find("\n---") else {
        return false;
    };
    rest[..end]
        .lines()
        .any(|line| line.trim() == CURSOR_MDC_MANAGED_KEY)
}

/// Render a library rule as a Cursor MDC file.
///
/// `alwaysApply: true` matches the semantics of inline injection (rules are
/// unconditionally part of the context).  The rule's display name becomes the
/// `description` so the file is self-describing in Cursor's rules UI.
fn render_cursor_mdc_rule(display_name: &str, content: &str) -> String {
    // Minimal YAML string escaping for the description value.
    let escaped = display_name.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        "---\ndescription: \"{}\"\nalwaysApply: true\n{}\n---\n\n{}\n",
        escaped,
        CURSOR_MDC_MANAGED_KEY,
        content.trim_end()
    )
}

/// Read a rule's display name + content from the library.
/// Falls back to the machine name when the display name is empty.
fn read_rule_for_mdc(machine_name: &str) -> Option<(String, String)> {
    let raw = read_rule(machine_name).ok()?;
    let rule: Rule = serde_json::from_str(&raw).ok()?;
    if rule.content.trim().is_empty() {
        return None;
    }
    let display = if rule.name.trim().is_empty() {
        machine_name.to_string()
    } else {
        rule.name.clone()
    };
    Some((display, rule.content))
}

/// Write rules as individual MDC files under `<project_dir>/.cursor/rules/`.
///
/// Cursor's native project-rule format: each rule becomes
/// `.cursor/rules/<machine_name>.mdc` with YAML frontmatter.  Files managed by
/// Automatic carry an `automatic-managed: true` frontmatter key; previously
/// managed files no longer in the rule list are deleted, user-authored `.mdc`
/// files are never touched.
///
/// Returns the list of files written or removed.
pub fn sync_rules_to_cursor_mdc_rules(
    project_dir: &str,
    rule_names: &[String],
) -> Result<Vec<String>, String> {
    let rules_dir = PathBuf::from(project_dir).join(".cursor").join("rules");
    fs::create_dir_all(&rules_dir)
        .map_err(|e| format!("Failed to create .cursor/rules/: {}", e))?;

    let mut touched: Vec<String> = Vec::new();
    let intended: HashSet<String> = rule_names.iter().map(|n| format!("{}.mdc", n)).collect();

    // Remove stale Automatic-managed files.
    if let Ok(entries) = fs::read_dir(&rules_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("mdc") {
                continue;
            }
            let file_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            if intended.contains(&file_name) {
                continue; // Will be (re-)written below.
            }
            if let Ok(content) = fs::read_to_string(&path) {
                if is_managed_mdc_content(&content) && fs::remove_file(&path).is_ok() {
                    touched.push(path.display().to_string());
                }
            }
        }
    }

    // Write each rule as `<machine_name>.mdc` (write-if-different).
    for machine_name in rule_names {
        let Some((display, content)) = read_rule_for_mdc(machine_name) else {
            continue; // Skip missing or empty rules silently.
        };

        let file_path = rules_dir.join(format!("{}.mdc", machine_name));
        let file_content = render_cursor_mdc_rule(&display, &content);

        let existing = fs::read_to_string(&file_path).unwrap_or_default();
        if existing != file_content {
            fs::write(&file_path, &file_content)
                .map_err(|e| format!("Failed to write rule '{}': {}", machine_name, e))?;
            touched.push(file_path.display().to_string());
        }
    }

    Ok(touched)
}

/// Read-only check: returns `true` if the on-disk `.cursor/rules/` directory
/// already contains exactly the MDC files that would be generated from
/// `rule_names` (same filenames, same content, no extra managed files).
pub fn is_cursor_mdc_rules_current(
    project_dir: &str,
    rule_names: &[String],
) -> Result<bool, String> {
    let rules_dir = PathBuf::from(project_dir).join(".cursor").join("rules");

    let mut managed_on_disk: HashSet<String> = HashSet::new();
    if let Ok(entries) = fs::read_dir(&rules_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("mdc") {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&path) {
                if is_managed_mdc_content(&content) {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        managed_on_disk.insert(name.to_string());
                    }
                }
            }
        }
    }

    let intended: HashSet<String> = rule_names
        .iter()
        .filter(|n| read_rule_for_mdc(n).is_some())
        .map(|n| format!("{}.mdc", n))
        .collect();

    if managed_on_disk != intended {
        return Ok(false);
    }

    for machine_name in rule_names {
        let Some((display, content)) = read_rule_for_mdc(machine_name) else {
            continue;
        };
        let expected = render_cursor_mdc_rule(&display, &content);
        let actual = match fs::read_to_string(rules_dir.join(format!("{}.mdc", machine_name))) {
            Ok(c) => c,
            Err(_) => return Ok(false),
        };
        if actual != expected {
            return Ok(false);
        }
    }

    Ok(true)
}

// ── Instructions Index Mode ──────────────────────────────────────────────────
//
// When a project has `instructions_index_mode: true`, rules are written as
// individual files under `.automatic/instructions/` rather than being injected
// inline.  The main instruction file (CLAUDE.md, AGENTS.md, etc.) receives a
// short index section listing those files.

const INDEX_START_MARKER: &str = "<!-- automatic:index:start -->";
const INDEX_END_MARKER: &str = "<!-- automatic:index:end -->";
const AUTOMATIC_INSTRUCTIONS_DIR: &str = ".automatic/instructions";

/// Marker placed at the start of every rule file written by Automatic under
/// `.automatic/instructions/`.
const AUTOMATIC_INSTRUCTIONS_MANAGED_HEADER: &str =
    "<!-- managed by Automatic — do not edit by hand -->\n\n";

/// Write rules as individual Markdown files under
/// `<project_dir>/.automatic/instructions/`.
///
/// Global rules are written as `<machine_name>.md`; custom rules are written
/// as `custom-<slug>.md` where `slug` is the lowercased, hyphenated rule name.
/// Returns the list of files written or removed.
pub fn sync_rules_to_automatic_instructions(
    project_dir: &str,
    rule_names: &[String],
    custom_rules: &[crate::core::CustomRule],
) -> Result<Vec<String>, String> {
    let instructions_dir = PathBuf::from(project_dir).join(AUTOMATIC_INSTRUCTIONS_DIR);
    fs::create_dir_all(&instructions_dir)
        .map_err(|e| format!("Failed to create .automatic/instructions/: {}", e))?;

    let mut touched: Vec<String> = Vec::new();

    // Build the set of filenames we intend to write.
    let mut intended: HashSet<String> = rule_names.iter().map(|n| format!("{}.md", n)).collect();
    for cr in custom_rules {
        intended.insert(format!("custom-{}.md", slugify(&cr.name)));
    }

    // Remove stale Automatic-managed files no longer in the intended set.
    if let Ok(entries) = fs::read_dir(&instructions_dir) {
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
                continue;
            }
            if let Ok(content) = fs::read_to_string(&path) {
                if content.starts_with(AUTOMATIC_INSTRUCTIONS_MANAGED_HEADER) {
                    if fs::remove_file(&path).is_ok() {
                        touched.push(path.display().to_string());
                    }
                }
            }
        }
    }

    // Write global rule files.
    for machine_name in rule_names {
        let content = match read_rule_content(machine_name) {
            Ok(c) if !c.trim().is_empty() => c,
            _ => continue,
        };

        let file_path = instructions_dir.join(format!("{}.md", machine_name));
        let file_content = format!(
            "{}{}\n",
            AUTOMATIC_INSTRUCTIONS_MANAGED_HEADER,
            content.trim_end()
        );

        let existing = fs::read_to_string(&file_path).unwrap_or_default();
        if existing != file_content {
            fs::write(&file_path, &file_content)
                .map_err(|e| format!("Failed to write rule '{}': {}", machine_name, e))?;
            touched.push(file_path.display().to_string());
        }
    }

    // Write custom rule files.
    for cr in custom_rules {
        if cr.content.trim().is_empty() {
            continue;
        }
        let slug = slugify(&cr.name);
        let file_path = instructions_dir.join(format!("custom-{}.md", slug));
        let file_content = format!(
            "{}{}\n",
            AUTOMATIC_INSTRUCTIONS_MANAGED_HEADER,
            cr.content.trim_end()
        );

        let existing = fs::read_to_string(&file_path).unwrap_or_default();
        if existing != file_content {
            fs::write(&file_path, &file_content)
                .map_err(|e| format!("Failed to write custom rule '{}': {}", cr.name, e))?;
            touched.push(file_path.display().to_string());
        }
    }

    Ok(touched)
}

/// Build the index section that is injected into the main instruction file when
/// `instructions_index_mode` is enabled.  Lists all rule files with their
/// relative path from the project root.
///
/// Duplicate rule machine names are collapsed to a single entry (preserving
/// first occurrence).  This is a defensive layer: `ensure_automatic_rules` is
/// the primary dedup site, but pinning the contract here protects against any
/// future caller that bypasses it.
pub fn build_index_section(
    rule_names: &[String],
    custom_rules: &[crate::core::CustomRule],
) -> String {
    let mut entries: Vec<String> = Vec::new();
    let mut seen_rule_names: HashSet<&str> = HashSet::new();

    for machine_name in rule_names {
        if !seen_rule_names.insert(machine_name.as_str()) {
            continue;
        }
        entries.push(format!(
            "- [{}]({}/{}.md)",
            machine_name_to_display(machine_name),
            AUTOMATIC_INSTRUCTIONS_DIR,
            machine_name
        ));
    }

    for cr in custom_rules {
        if cr.content.trim().is_empty() {
            continue;
        }
        entries.push(format!(
            "- [{}]({}/custom-{}.md)",
            cr.name,
            AUTOMATIC_INSTRUCTIONS_DIR,
            slugify(&cr.name)
        ));
    }

    if entries.is_empty() {
        return String::new();
    }

    format!(
        "{}\n## Agent Instructions\n\nThe following instruction files are loaded by Automatic. \
         Refer to them for project-specific rules and context.\n\n{}\n{}",
        INDEX_START_MARKER,
        entries.join("\n"),
        INDEX_END_MARKER
    )
}

/// Strip the `<!-- automatic:index:start -->...<!-- automatic:index:end -->`
/// section from file content.
pub fn strip_index_section(content: &str) -> String {
    if let (Some(start), Some(end)) = (
        content.find(INDEX_START_MARKER),
        content.find(INDEX_END_MARKER),
    ) {
        let before = &content[..start];
        let after = &content[end + INDEX_END_MARKER.len()..];
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

/// Inject or update the index section in a project instruction file when
/// `instructions_index_mode` is enabled.  Returns `true` if the file was written.
pub fn inject_index_into_project_file(
    directory: &str,
    filename: &str,
    rule_names: &[String],
    custom_rules: &[crate::core::CustomRule],
) -> Result<bool, String> {
    if directory.is_empty() {
        return Ok(false);
    }

    let path = PathBuf::from(directory).join(filename);
    if !path.exists() {
        return Ok(false);
    }

    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;

    // Strip all managed sections to isolate user content.
    let user_content = strip_index_section(&strip_groups_section(&strip_rules_section(
        &strip_managed_section(&raw),
    )));
    // The groups section is no longer written into instruction files;
    // related-project context is exposed via the
    // `automatic_get_related_projects` MCP tool instead. Any pre-existing
    // groups block is dropped on the next sync.
    let groups_section = String::new();
    let index_section = build_index_section(rule_names, custom_rules);

    let full_content = assemble_file_with_index(&user_content, &groups_section, &index_section);

    if full_content != raw {
        fs::write(&path, full_content).map_err(|e| e.to_string())?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Assemble a file with an index section instead of an inline rules section.
fn assemble_file_with_index(
    user_content: &str,
    groups_section: &str,
    index_section: &str,
) -> String {
    let mut parts: Vec<&str> = vec![user_content.trim_end()];

    if !groups_section.is_empty() {
        parts.push(groups_section.trim());
    }
    if !index_section.is_empty() {
        parts.push(index_section.trim());
    }

    if parts.len() == 1 && parts[0].is_empty() {
        return String::new();
    }

    format!("{}\n", parts.join("\n\n"))
}

/// Convert a rule machine name (e.g. `automatic-service`) to a display label
/// (e.g. `Automatic Service`).
fn machine_name_to_display(name: &str) -> String {
    name.split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Convert a display name to a URL-safe slug (lowercase, hyphens).
fn slugify(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Re-inject rules into an existing project file.  Reads the file, strips
/// any existing rules section, rebuilds it from the provided rule names,
/// and writes back.  Used during sync to keep rules current.
pub fn inject_rules_into_project_file(
    directory: &str,
    filename: &str,
    rule_names: &[String],
) -> Result<bool, String> {
    inject_rules_into_project_file_with_custom(directory, filename, rule_names, &[])
}

/// Re-inject rules (global + custom) into an existing project file.
pub fn inject_rules_into_project_file_with_custom(
    directory: &str,
    filename: &str,
    rule_names: &[String],
    custom_contents: &[String],
) -> Result<bool, String> {
    if directory.is_empty() {
        return Ok(false);
    }

    let path = PathBuf::from(directory).join(filename);
    if !path.exists() {
        return Ok(false);
    }

    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    // Strip all managed sections so we start from pure user content.
    // Also strip the index section so switching from index mode to inline mode
    // cleanly removes the old index block.
    let user_content = strip_index_section(&strip_groups_section(&strip_rules_section(
        &strip_managed_section(&raw),
    )));

    let rules_section = build_rules_section_with_custom(rule_names, custom_contents)?;

    // The groups section is no longer written into instruction files;
    // related-project context is exposed via the
    // `automatic_get_related_projects` MCP tool instead. Any pre-existing
    // groups block is dropped on the next sync.
    let groups_section = String::new();

    let full_content = assemble_file(&user_content, &groups_section, &rules_section);

    // Only write if content actually changed
    if full_content != raw {
        fs::write(&path, full_content).map_err(|e| e.to_string())?;
        Ok(true)
    } else {
        Ok(false)
    }
}

// ── Project Group Context Injection ──────────────────────────────────────────

const GROUPS_START_MARKER: &str = "<!-- automatic:groups:start -->";
const GROUPS_END_MARKER: &str = "<!-- automatic:groups:end -->";

/// Strip the `<!-- automatic:groups:start -->...<!-- automatic:groups:end -->` section.
pub fn strip_groups_section(content: &str) -> String {
    if let (Some(start), Some(end)) = (
        content.find(GROUPS_START_MARKER),
        content.find(GROUPS_END_MARKER),
    ) {
        let before = &content[..start];
        let after = &content[end + GROUPS_END_MARKER.len()..];
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

/// Build the groups context section for injection into an instruction file.
///
/// Always returns an empty string. Project group membership is no longer
/// written into instruction files because (a) every member-project file would
/// have to be rewritten on every membership change, creating churn on tracked
/// files, and (b) the peer project names risk leaking into public repos when
/// instruction files are committed.
///
/// Agents that need related-project context should call the
/// `automatic_get_related_projects` MCP tool instead, which returns the same
/// information on demand.
pub fn build_groups_section(
    _this_project_name: &str,
    _this_project_dir: &str,
    _groups: &[crate::core::ProjectGroup],
) -> String {
    String::new()
}

/// Compute a relative path from `from_dir` to `to_dir`.
///
/// Returns the `to_dir` path as-is if either argument is empty or the paths
/// share no common prefix (e.g. on different drives on Windows).
pub fn compute_relative_path(from_dir: &str, to_dir: &str) -> String {
    if from_dir.is_empty() || to_dir.is_empty() {
        return to_dir.to_string();
    }

    let from = PathBuf::from(from_dir);
    let to = PathBuf::from(to_dir);

    // Collect path components.
    let from_components: Vec<_> = from.components().collect();
    let to_components: Vec<_> = to.components().collect();

    // Find the length of the common prefix.
    let common_len = from_components
        .iter()
        .zip(to_components.iter())
        .take_while(|(a, b)| a == b)
        .count();

    // If nothing is shared (e.g. different drive letters on Windows), fall back.
    if common_len == 0 {
        return to_dir.to_string();
    }

    let up_count = from_components.len() - common_len;
    let mut rel = PathBuf::new();

    for _ in 0..up_count {
        rel.push("..");
    }
    for comp in &to_components[common_len..] {
        rel.push(comp);
    }

    if rel.as_os_str().is_empty() {
        ".".to_string()
    } else {
        rel.display().to_string()
    }
}

/// Assemble the full file content from user content, groups section, and rules
/// section.  Any combination of empty sections is handled gracefully.
fn assemble_file(user_content: &str, groups_section: &str, rules_section: &str) -> String {
    let mut parts: Vec<&str> = vec![user_content.trim_end()];

    if !groups_section.is_empty() {
        parts.push(groups_section.trim());
    }
    if !rules_section.is_empty() {
        parts.push(rules_section.trim());
    }

    if parts.len() == 1 && parts[0].is_empty() {
        return String::new();
    }

    format!("{}\n", parts.join("\n\n"))
}

/// Inject or update the groups context section in a project instruction file.
///
/// This is called by the sync engine after `clean_project_file` and before
/// rules injection.  It reads the on-disk file, strips any existing groups
/// section, rebuilds it from the provided `groups` slice, and writes back only
/// if the content actually changed.
///
/// Returns `true` if the file was written, `false` if it was already current
/// or the file does not exist.
pub fn inject_groups_into_project_file(
    directory: &str,
    filename: &str,
    this_project_name: &str,
    groups: &[crate::core::ProjectGroup],
) -> Result<bool, String> {
    if directory.is_empty() {
        return Ok(false);
    }

    let path = PathBuf::from(directory).join(filename);
    if !path.exists() {
        return Ok(false);
    }

    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;

    // Decompose the current file into its three layers, stripping the index
    // section from user content so it doesn't accumulate if the mode changes.
    let user_content = strip_index_section(&strip_groups_section(&strip_rules_section(
        &strip_managed_section(&raw),
    )));
    let rules_section = extract_rules_section(&raw);
    let index_section = extract_index_section(&raw);
    let new_groups_section = build_groups_section(this_project_name, directory, groups);

    // Preserve whichever trailing section the file has: an index section
    // (index mode) or an inline rules section (default mode).
    let full_content = if !index_section.is_empty() {
        assemble_file_with_index(&user_content, &new_groups_section, &index_section)
    } else {
        assemble_file(&user_content, &new_groups_section, &rules_section)
    };

    if full_content != raw {
        fs::write(&path, full_content).map_err(|e| e.to_string())?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Extract the raw `<!-- automatic:rules:start -->...<!-- automatic:rules:end -->` block
/// (including markers) from a file, or return an empty string if absent.
fn extract_rules_section(content: &str) -> String {
    if let (Some(start), Some(end)) = (
        content.find(RULES_START_MARKER),
        content.find(RULES_END_MARKER),
    ) {
        content[start..end + RULES_END_MARKER.len()].to_string()
    } else {
        String::new()
    }
}

/// Extract the raw `<!-- automatic:index:start -->...<!-- automatic:index:end -->` block
/// (including markers) from a file, or return an empty string if absent.
fn extract_index_section(content: &str) -> String {
    if let (Some(start), Some(end)) = (
        content.find(INDEX_START_MARKER),
        content.find(INDEX_END_MARKER),
    ) {
        content[start..end + INDEX_END_MARKER.len()].to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn tmp() -> TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn custom(s: &str) -> Vec<String> {
        vec![s.to_string()]
    }

    fn customs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn no_rules() -> Vec<String> {
        vec![]
    }

    // ── strip_rules_section ──────────────────────────────────────────────────

    #[test]
    fn strip_leaves_content_without_markers_unchanged() {
        let content = "# My file\n\nSome content here.";
        assert_eq!(strip_rules_section(content), content);
    }

    #[test]
    fn strip_removes_rules_section_between_markers() {
        let content = "# Header\n\n<!-- automatic:rules:start -->\nrule content\n<!-- automatic:rules:end -->\n\nTrailing.";
        let result = strip_rules_section(content);
        assert!(!result.contains("<!-- automatic:rules:start -->"));
        assert!(!result.contains("rule content"));
        assert!(result.contains("# Header"));
        assert!(result.contains("Trailing."));
    }

    #[test]
    fn strip_returns_empty_string_when_only_rules_section() {
        let content = "<!-- automatic:rules:start -->\nsome rule\n<!-- automatic:rules:end -->";
        let result = strip_rules_section(content);
        assert_eq!(result, "");
    }

    #[test]
    fn strip_is_idempotent() {
        let content =
            "# File\n\n<!-- automatic:rules:start -->\nrule\n<!-- automatic:rules:end -->";
        let once = strip_rules_section(content);
        let twice = strip_rules_section(&once);
        assert_eq!(once, twice);
    }

    #[test]
    fn strip_handles_missing_end_marker_by_leaving_content() {
        let content = "# File\n\n<!-- automatic:rules:start -->\nrule content";
        let result = strip_rules_section(content);
        // Only one marker present — content is returned as-is.
        assert_eq!(result, content);
    }

    // ── build_rules_section_with_custom ──────────────────────────────────────

    #[test]
    fn build_returns_empty_when_no_rules_and_no_custom() {
        let result = build_rules_section_with_custom(&[], &[]).expect("build");
        assert_eq!(result, "");
    }

    #[test]
    fn build_wraps_custom_content_in_markers() {
        let result = build_rules_section_with_custom(&[], &custom("Do the thing.")).expect("build");
        assert!(result.starts_with("<!-- automatic:rules:start -->"));
        assert!(result.ends_with("<!-- automatic:rules:end -->"));
        assert!(result.contains("Do the thing."));
    }

    #[test]
    fn build_skips_empty_custom_content() {
        let result = build_rules_section_with_custom(&[], &customs(&["", "  ", "real rule"]))
            .expect("build");
        // Only the non-empty entry should appear.
        assert!(result.contains("real rule"));
        // Blank entries produce no extra separating newlines between markers and content.
        assert!(!result.contains("  \n"));
    }

    #[test]
    fn build_joins_multiple_custom_rules_with_newline() {
        let result =
            build_rules_section_with_custom(&[], &customs(&["Rule A.", "Rule B."])).expect("build");
        assert!(result.contains("Rule A."));
        assert!(result.contains("Rule B."));
    }

    #[test]
    fn build_trims_custom_content_whitespace() {
        let result =
            build_rules_section_with_custom(&[], &custom("  trimmed content  ")).expect("build");
        // The trimmed version should appear; leading/trailing spaces should not.
        assert!(result.contains("trimmed content"));
    }

    // ── inject_rules_into_project_file_with_custom ───────────────────────────

    #[test]
    fn inject_returns_false_when_directory_is_empty() {
        let result = inject_rules_into_project_file_with_custom(
            "",
            "AGENTS.md",
            &no_rules(),
            &custom("A rule."),
        );
        assert_eq!(result, Ok(false));
    }

    #[test]
    fn inject_returns_false_when_file_does_not_exist() {
        let dir = tmp();
        let result = inject_rules_into_project_file_with_custom(
            dir.path().to_str().unwrap(),
            "MISSING.md",
            &no_rules(),
            &custom("A rule."),
        );
        assert_eq!(result, Ok(false));
    }

    #[test]
    fn inject_appends_rules_to_existing_file() {
        let dir = tmp();
        let file = dir.path().join("AGENTS.md");
        fs::write(&file, "# My File\n\nUser content.").expect("write");

        let changed = inject_rules_into_project_file_with_custom(
            dir.path().to_str().unwrap(),
            "AGENTS.md",
            &no_rules(),
            &custom("Always be kind."),
        )
        .expect("inject");

        assert!(changed);
        let on_disk = fs::read_to_string(&file).expect("read");
        assert!(on_disk.contains("User content."));
        assert!(on_disk.contains("Always be kind."));
        assert!(on_disk.contains("<!-- automatic:rules:start -->"));
        assert!(on_disk.contains("<!-- automatic:rules:end -->"));
    }

    #[test]
    fn inject_replaces_existing_rules_section() {
        let dir = tmp();
        let file = dir.path().join("AGENTS.md");

        // Write initial file with a rules section.
        let initial =
            "# File\n\n<!-- automatic:rules:start -->\nOld rule.\n<!-- automatic:rules:end -->\n";
        fs::write(&file, initial).expect("write");

        inject_rules_into_project_file_with_custom(
            dir.path().to_str().unwrap(),
            "AGENTS.md",
            &no_rules(),
            &custom("New rule."),
        )
        .expect("inject");

        let on_disk = fs::read_to_string(&file).expect("read");
        assert!(on_disk.contains("New rule."));
        assert!(!on_disk.contains("Old rule."));
        // No duplicate markers.
        assert_eq!(on_disk.matches("<!-- automatic:rules:start -->").count(), 1);
    }

    #[test]
    fn inject_returns_false_when_content_unchanged() {
        let dir = tmp();
        let file = dir.path().join("AGENTS.md");

        // Create the file with user content first.
        fs::write(&file, "# My File\n\nUser content.").expect("write initial");

        // First inject to establish the rules section.
        inject_rules_into_project_file_with_custom(
            dir.path().to_str().unwrap(),
            "AGENTS.md",
            &no_rules(),
            &custom("Stable rule."),
        )
        .expect("first inject");

        let before = fs::read_to_string(&file).expect("read after first inject");

        // Second inject with the same rule — content is already current, should not change.
        let changed = inject_rules_into_project_file_with_custom(
            dir.path().to_str().unwrap(),
            "AGENTS.md",
            &no_rules(),
            &custom("Stable rule."),
        )
        .expect("second inject");

        let after = fs::read_to_string(&file).expect("read after second inject");
        assert!(!changed, "content should not have changed on second inject");
        assert_eq!(before, after);
    }

    #[test]
    fn inject_removes_rules_section_when_no_rules_given() {
        let dir = tmp();
        let file = dir.path().join("AGENTS.md");

        let initial =
            "# File\n\n<!-- automatic:rules:start -->\nSome rule.\n<!-- automatic:rules:end -->\n";
        fs::write(&file, initial).expect("write");

        inject_rules_into_project_file_with_custom(
            dir.path().to_str().unwrap(),
            "AGENTS.md",
            &no_rules(),
            &[],
        )
        .expect("inject");

        let on_disk = fs::read_to_string(&file).expect("read");
        assert!(!on_disk.contains("<!-- automatic:rules:start -->"));
        assert!(!on_disk.contains("Some rule."));
    }

    // ── is_file_rules_current_with_custom ────────────────────────────────────

    #[test]
    fn file_with_correct_rules_is_reported_as_current() {
        let dir = tmp();
        let file = dir.path().join("AGENTS.md");

        // Build and write the section we will then check against.
        let section =
            build_rules_section_with_custom(&[], &custom("My rule.")).expect("build section");
        let content = format!("# File\n\n{}\n", section);
        fs::write(&file, &content).expect("write");

        let is_current = is_file_rules_current_with_custom(&file, &no_rules(), &custom("My rule."))
            .expect("check");
        assert!(is_current);
    }

    #[test]
    fn file_with_stale_rules_is_reported_as_not_current() {
        let dir = tmp();
        let file = dir.path().join("AGENTS.md");
        fs::write(
            &file,
            "# File\n\n<!-- automatic:rules:start -->\nOld rule.\n<!-- automatic:rules:end -->\n",
        )
        .expect("write");

        let is_current =
            is_file_rules_current_with_custom(&file, &no_rules(), &custom("New rule."))
                .expect("check");
        assert!(!is_current);
    }

    #[test]
    fn missing_file_is_reported_as_not_current() {
        let dir = tmp();
        let file = dir.path().join("NONEXISTENT.md");

        let is_current = is_file_rules_current_with_custom(&file, &no_rules(), &custom("A rule."))
            .expect("check");
        assert!(!is_current);
    }

    // ── sync_rules_to_dot_claude_rules ───────────────────────────────────────

    #[test]
    fn sync_creates_dot_claude_rules_directory() {
        let dir = tmp();
        let rules_dir = dir.path().join(".claude").join("rules");
        assert!(!rules_dir.exists());

        // No rule names → nothing to write, but dir should be created.
        sync_rules_to_dot_claude_rules(dir.path().to_str().unwrap(), &no_rules()).expect("sync");

        assert!(rules_dir.exists());
    }

    #[test]
    fn sync_with_no_rules_removes_managed_files() {
        let dir = tmp();
        let rules_dir = dir.path().join(".claude").join("rules");
        fs::create_dir_all(&rules_dir).expect("create dir");

        // Write a managed file manually.
        let managed_content =
            "<!-- managed by Automatic — do not edit by hand -->\n\nSome content.\n";
        fs::write(rules_dir.join("old-rule.md"), managed_content).expect("write managed");

        sync_rules_to_dot_claude_rules(dir.path().to_str().unwrap(), &no_rules()).expect("sync");

        // Managed file should have been removed.
        assert!(!rules_dir.join("old-rule.md").exists());
    }

    #[test]
    fn sync_does_not_remove_unmanaged_user_files() {
        let dir = tmp();
        let rules_dir = dir.path().join(".claude").join("rules");
        fs::create_dir_all(&rules_dir).expect("create dir");

        // Write a file WITHOUT the managed header.
        fs::write(rules_dir.join("user-file.md"), "# User wrote this").expect("write user file");

        sync_rules_to_dot_claude_rules(dir.path().to_str().unwrap(), &no_rules()).expect("sync");

        // User file should be untouched.
        assert!(rules_dir.join("user-file.md").exists());
    }

    // ── sync_rules_to_cursor_mdc_rules ───────────────────────────────────────

    #[test]
    fn mdc_sync_creates_cursor_rules_directory() {
        let dir = tmp();
        let rules_dir = dir.path().join(".cursor").join("rules");
        assert!(!rules_dir.exists());

        sync_rules_to_cursor_mdc_rules(dir.path().to_str().unwrap(), &no_rules()).expect("sync");

        assert!(rules_dir.exists());
    }

    #[test]
    fn mdc_sync_with_no_rules_removes_managed_files() {
        let dir = tmp();
        let rules_dir = dir.path().join(".cursor").join("rules");
        fs::create_dir_all(&rules_dir).expect("create dir");

        let managed = "---\ndescription: \"Old\"\nalwaysApply: true\nautomatic-managed: true\n---\n\nOld content.\n";
        fs::write(rules_dir.join("old-rule.mdc"), managed).expect("write managed");

        sync_rules_to_cursor_mdc_rules(dir.path().to_str().unwrap(), &no_rules()).expect("sync");

        assert!(!rules_dir.join("old-rule.mdc").exists());
    }

    #[test]
    fn mdc_sync_does_not_remove_unmanaged_user_files() {
        let dir = tmp();
        let rules_dir = dir.path().join(".cursor").join("rules");
        fs::create_dir_all(&rules_dir).expect("create dir");

        // User .mdc without the managed frontmatter key, and one with
        // frontmatter but no managed key.
        fs::write(rules_dir.join("user-plain.mdc"), "# User wrote this").expect("write");
        fs::write(
            rules_dir.join("user-fm.mdc"),
            "---\ndescription: \"Mine\"\nglobs: \"src/**\"\n---\n\nUser rule.\n",
        )
        .expect("write");

        sync_rules_to_cursor_mdc_rules(dir.path().to_str().unwrap(), &no_rules()).expect("sync");

        assert!(rules_dir.join("user-plain.mdc").exists());
        assert!(rules_dir.join("user-fm.mdc").exists());
    }

    #[test]
    fn mdc_managed_detection() {
        assert!(is_managed_mdc_content(
            "---\ndescription: \"X\"\nautomatic-managed: true\n---\n\nBody"
        ));
        assert!(!is_managed_mdc_content("# Plain markdown"));
        assert!(!is_managed_mdc_content(
            "---\ndescription: \"X\"\n---\n\nautomatic-managed: true"
        ));
    }

    #[test]
    fn mdc_render_includes_frontmatter_and_escapes_description() {
        let rendered = render_cursor_mdc_rule("My \"quoted\" rule", "Body content.");
        assert!(rendered.starts_with("---\n"));
        assert!(rendered.contains("description: \"My \\\"quoted\\\" rule\""));
        assert!(rendered.contains("alwaysApply: true"));
        assert!(rendered.contains("automatic-managed: true"));
        assert!(rendered.ends_with("Body content.\n"));
        assert!(is_managed_mdc_content(&rendered));
    }

    #[test]
    fn mdc_sync_writes_library_rule_and_reports_current() {
        let home = tempfile::tempdir().expect("home tempdir");
        crate::core::paths::with_test_home(home.path().to_path_buf(), || {
            let rules_lib = crate::core::rules::get_rules_dir().expect("rules dir");
            fs::create_dir_all(&rules_lib).expect("create library rules dir");
            fs::write(
                rules_lib.join("my-rule.json"),
                r#"{"name":"My Rule","content":"Always be testing."}"#,
            )
            .expect("write library rule");

            let dir = tempfile::tempdir().expect("tempdir");
            let project_dir = dir.path().to_str().unwrap();
            let rule_names = vec!["my-rule".to_string()];

            let touched =
                sync_rules_to_cursor_mdc_rules(project_dir, &rule_names).expect("sync");
            assert_eq!(touched.len(), 1);

            let on_disk =
                fs::read_to_string(dir.path().join(".cursor/rules/my-rule.mdc")).expect("read");
            assert!(on_disk.contains("description: \"My Rule\""));
            assert!(on_disk.contains("Always be testing."));

            // Reported current after sync, and idempotent.
            assert!(is_cursor_mdc_rules_current(project_dir, &rule_names).expect("check"));
            let touched2 =
                sync_rules_to_cursor_mdc_rules(project_dir, &rule_names).expect("re-sync");
            assert!(touched2.is_empty());

            // Modifying the managed file flips the check.
            fs::write(dir.path().join(".cursor/rules/my-rule.mdc"), "---\nautomatic-managed: true\n---\ntampered").expect("tamper");
            assert!(!is_cursor_mdc_rules_current(project_dir, &rule_names).expect("check"));
        });
    }

    // ── strip_index_section ──────────────────────────────────────────────────

    #[test]
    fn strip_index_removes_markers_and_content() {
        let content = "# Header\n\n<!-- automatic:index:start -->\n## Agent Instructions\n\nSome index.\n<!-- automatic:index:end -->\n\nTrailing.";
        let result = strip_index_section(content);
        assert!(!result.contains("<!-- automatic:index:start -->"));
        assert!(!result.contains("Some index."));
        assert!(result.contains("# Header"));
        assert!(result.contains("Trailing."));
    }

    #[test]
    fn strip_index_leaves_content_without_markers_unchanged() {
        let content = "# My file\n\nNo index here.";
        assert_eq!(strip_index_section(content), content);
    }

    #[test]
    fn strip_index_is_idempotent() {
        let content =
            "# File\n\n<!-- automatic:index:start -->\nindex\n<!-- automatic:index:end -->";
        let once = strip_index_section(content);
        let twice = strip_index_section(&once);
        assert_eq!(once, twice);
    }

    // ── build_index_section ──────────────────────────────────────────────────

    #[test]
    fn build_index_returns_empty_when_no_rules_and_no_custom() {
        let result = build_index_section(&[], &[]);
        assert_eq!(result, "");
    }

    #[test]
    fn build_index_wraps_rule_names_in_markers() {
        let rules = vec!["my-rule".to_string()];
        let result = build_index_section(&rules, &[]);
        assert!(result.starts_with("<!-- automatic:index:start -->"));
        assert!(result.ends_with("<!-- automatic:index:end -->"));
        assert!(result.contains(".automatic/instructions/my-rule.md"));
        assert!(result.contains("My Rule"));
    }

    #[test]
    fn build_index_includes_custom_rules() {
        use crate::core::CustomRule;
        let custom = vec![CustomRule {
            name: "My Custom Rule".to_string(),
            content: "Some rule content.".to_string(),
        }];
        let result = build_index_section(&[], &custom);
        assert!(result.contains(".automatic/instructions/custom-my-custom-rule.md"));
        assert!(result.contains("My Custom Rule"));
    }

    #[test]
    fn build_index_skips_empty_custom_rules() {
        use crate::core::CustomRule;
        let custom = vec![CustomRule {
            name: "Empty Rule".to_string(),
            content: "".to_string(),
        }];
        let result = build_index_section(&[], &custom);
        assert_eq!(result, "");
    }

    #[test]
    fn build_index_collapses_duplicate_rule_names() {
        // Pins the defensive contract: even if a caller hands us a list
        // with duplicates (bypassing `ensure_automatic_rules`), the rendered
        // index must list each rule once.
        let rules = vec![
            "automatic-process".to_string(),
            "automatic-process".to_string(),
        ];
        let result = build_index_section(&rules, &[]);
        let occurrences = result.matches("automatic-process.md").count();
        assert_eq!(
            occurrences, 1,
            "duplicate rule names must collapse to a single index entry; got: {}",
            result
        );
    }

    // ── sync_rules_to_automatic_instructions ────────────────────────────────

    #[test]
    fn sync_creates_automatic_instructions_directory() {
        let dir = tmp();
        sync_rules_to_automatic_instructions(dir.path().to_str().unwrap(), &no_rules(), &[])
            .expect("sync");
        assert!(dir.path().join(".automatic").join("instructions").exists());
    }

    #[test]
    fn sync_instructions_removes_stale_managed_files() {
        let dir = tmp();
        let instructions_dir = dir.path().join(".automatic").join("instructions");
        fs::create_dir_all(&instructions_dir).expect("create dir");

        // Write a managed file that won't be in the new rule set.
        let managed_content =
            "<!-- managed by Automatic — do not edit by hand -->\n\nOld content.\n";
        fs::write(instructions_dir.join("old-rule.md"), managed_content).expect("write");

        sync_rules_to_automatic_instructions(dir.path().to_str().unwrap(), &no_rules(), &[])
            .expect("sync");

        assert!(!instructions_dir.join("old-rule.md").exists());
    }

    #[test]
    fn sync_instructions_does_not_remove_unmanaged_user_files() {
        let dir = tmp();
        let instructions_dir = dir.path().join(".automatic").join("instructions");
        fs::create_dir_all(&instructions_dir).expect("create dir");

        // Write a file WITHOUT the managed header.
        fs::write(instructions_dir.join("user-file.md"), "# User wrote this").expect("write");

        sync_rules_to_automatic_instructions(dir.path().to_str().unwrap(), &no_rules(), &[])
            .expect("sync");

        assert!(instructions_dir.join("user-file.md").exists());
    }

    // ── inject_index_into_project_file ──────────────────────────────────────

    #[test]
    fn inject_index_appends_index_to_existing_file() {
        let dir = tmp();
        let file = dir.path().join("AGENTS.md");
        fs::write(&file, "# My File\n\nUser content.").expect("write");

        inject_index_into_project_file(dir.path().to_str().unwrap(), "AGENTS.md", &no_rules(), &[])
            .expect("inject");

        // No rules and no custom → index section should not be present.
        let on_disk = fs::read_to_string(&file).expect("read");
        assert!(!on_disk.contains("<!-- automatic:index:start -->"));
        assert!(on_disk.contains("User content."));
    }

    #[test]
    fn inject_index_replaces_existing_index_section() {
        let dir = tmp();
        let file = dir.path().join("AGENTS.md");
        let initial = "# File\n\n<!-- automatic:index:start -->\n## Agent Instructions\n\nOld index.\n<!-- automatic:index:end -->\n";
        fs::write(&file, initial).expect("write");

        // No rules in this call → index section is removed.
        inject_index_into_project_file(dir.path().to_str().unwrap(), "AGENTS.md", &no_rules(), &[])
            .expect("inject");

        let on_disk = fs::read_to_string(&file).expect("read");
        assert!(!on_disk.contains("<!-- automatic:index:start -->"));
        assert!(!on_disk.contains("Old index."));
        assert!(on_disk.contains("# File"));
    }

    // ── slugify ──────────────────────────────────────────────────────────────

    #[test]
    fn slugify_lowercases_and_replaces_spaces() {
        assert_eq!(slugify("My Custom Rule"), "my-custom-rule");
    }

    #[test]
    fn slugify_removes_consecutive_separators() {
        assert_eq!(slugify("foo  bar"), "foo-bar");
    }

    // ── machine_name_to_display ──────────────────────────────────────────────

    #[test]
    fn machine_name_to_display_capitalises_words() {
        assert_eq!(
            machine_name_to_display("automatic-service"),
            "Automatic Service"
        );
    }

    #[test]
    fn machine_name_to_display_handles_single_word() {
        assert_eq!(machine_name_to_display("coding"), "Coding");
    }
}
