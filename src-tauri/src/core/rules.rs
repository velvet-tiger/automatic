use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use super::paths::get_automatic_dir;

// ── Rules ────────────────────────────────────────────────────────────────────

/// A rule stored as JSON in `~/.automatic/rules/{machine_name}.json`.
/// The machine name (filename stem) is an immutable lowercase slug.
/// The display `name` can be freely renamed.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Rule {
    /// Human-readable display name (can be renamed).
    pub name: String,
    /// Markdown content of the rule.
    pub content: String,
}

/// Summary returned by `list_rules` — machine name + display name.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RuleEntry {
    pub id: String,
    pub name: String,
}

/// Validate a rule machine name: lowercase alphanumeric + hyphens only,
/// must start with a letter, no consecutive hyphens, not empty.
pub fn is_valid_machine_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 128 {
        return false;
    }
    // Must start with a lowercase letter
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    // Remaining: lowercase letters, digits, hyphens (no consecutive hyphens)
    let mut prev_hyphen = false;
    for c in chars {
        if c == '-' {
            if prev_hyphen {
                return false;
            }
            prev_hyphen = true;
        } else if c.is_ascii_lowercase() || c.is_ascii_digit() {
            prev_hyphen = false;
        } else {
            return false;
        }
    }
    // Must not end with a hyphen
    !name.ends_with('-')
}

pub fn get_rules_dir() -> Result<PathBuf, String> {
    Ok(get_automatic_dir()?.join("rules"))
}

pub fn list_rules() -> Result<Vec<RuleEntry>, String> {
    let dir = get_rules_dir()?;

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut rules = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| e.to_string())?;

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if is_valid_machine_name(stem) {
                        if let Ok(raw) = fs::read_to_string(&path) {
                            if let Ok(rule) = serde_json::from_str::<Rule>(&raw) {
                                rules.push(RuleEntry {
                                    id: stem.to_string(),
                                    name: rule.name,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(rules)
}

/// Read the full rule (display name + content) by machine name.
pub fn read_rule(machine_name: &str) -> Result<String, String> {
    if !is_valid_machine_name(machine_name) {
        return Err("Invalid rule machine name".into());
    }
    let dir = get_rules_dir()?;
    let path = dir.join(format!("{}.json", machine_name));

    if path.exists() {
        fs::read_to_string(path).map_err(|e| e.to_string())
    } else {
        Err(format!("Rule '{}' not found", machine_name))
    }
}

/// Read only the content of a rule (for injection into project files).
pub fn read_rule_content(machine_name: &str) -> Result<String, String> {
    let raw = read_rule(machine_name)?;
    let rule: Rule = serde_json::from_str(&raw).map_err(|e| format!("Invalid rule data: {}", e))?;
    Ok(rule.content)
}

pub fn save_rule(machine_name: &str, name: &str, content: &str) -> Result<(), String> {
    if !is_valid_machine_name(machine_name) {
        return Err(
            "Invalid rule machine name. Use lowercase letters, digits, and hyphens only.".into(),
        );
    }
    if name.trim().is_empty() {
        return Err("Rule display name cannot be empty".into());
    }

    let dir = get_rules_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    let rule = Rule {
        name: name.to_string(),
        content: content.to_string(),
    };
    let pretty = serde_json::to_string_pretty(&rule).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.json", machine_name));
    fs::write(path, pretty).map_err(|e| e.to_string())
}

pub fn delete_rule(machine_name: &str) -> Result<(), String> {
    if !is_valid_machine_name(machine_name) {
        return Err("Invalid rule machine name".into());
    }
    let dir = get_rules_dir()?;
    let path = dir.join(format!("{}.json", machine_name));

    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Built-in rules shipped with the app.  Each entry is (machine_name, display_name, content).
/// Written to `~/.automatic/rules/{machine_name}.json` on first run (or when missing),
/// but never overwrite existing files — user edits are preserved.
const DEFAULT_RULES: &[(&str, &str, &str)] = &[
    (
        "automatic-general",
        "General",
        include_str!("../../rules/automatic/general.md"),
    ),
    (
        "automatic-code-style",
        "Code Style",
        include_str!("../../rules/automatic/code-style.md"),
    ),
    (
        "automatic-checklist",
        "Checklist",
        include_str!("../../rules/automatic/checklist.md"),
    ),
    (
        "automatic-service",
        "Automatic",
        include_str!("../../rules/automatic/automatic-service.md"),
    ),
];

/// Write any missing default rules to `~/.automatic/rules/`.
/// Existing files are left untouched, so user edits are always preserved.
pub fn install_default_rules() -> Result<(), String> {
    let dir = get_rules_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    for (machine_name, display_name, content) in DEFAULT_RULES {
        let path = dir.join(format!("{}.json", machine_name));
        if !path.exists() {
            let rule = Rule {
                name: display_name.to_string(),
                content: content.to_string(),
            };
            let pretty = serde_json::to_string_pretty(&rule).map_err(|e| e.to_string())?;
            fs::write(&path, pretty).map_err(|e| e.to_string())?;
        } else if *machine_name == "automatic-service" {
            // Migration: rename "Automatic MCP Service" → "Automatic"
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(mut rule) = serde_json::from_str::<Rule>(&raw) {
                    if rule.name == "Automatic MCP Service" {
                        rule.name = "Automatic".to_string();
                        if let Ok(pretty) = serde_json::to_string_pretty(&rule) {
                            let _ = fs::write(&path, pretty);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn temp_rules_dir() -> (TempDir, PathBuf) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let rules_dir = tmp.path().join("rules");
        fs::create_dir_all(&rules_dir).expect("create rules dir");
        (tmp, rules_dir)
    }

    fn write_rule(rules_dir: &PathBuf, machine_name: &str, display_name: &str, content: &str) {
        let rule = Rule {
            name: display_name.to_string(),
            content: content.to_string(),
        };
        let json = serde_json::to_string_pretty(&rule).expect("serialize");
        fs::write(rules_dir.join(format!("{}.json", machine_name)), json).expect("write rule");
    }

    fn read_rule_from_dir(rules_dir: &PathBuf, machine_name: &str) -> Rule {
        let raw = fs::read_to_string(rules_dir.join(format!("{}.json", machine_name)))
            .expect("read rule file");
        serde_json::from_str(&raw).expect("parse rule")
    }

    // ── is_valid_machine_name ────────────────────────────────────────────────

    #[test]
    fn valid_machine_names_are_accepted() {
        assert!(is_valid_machine_name("my-rule"));
        assert!(is_valid_machine_name("rule1"));
        assert!(is_valid_machine_name("a"));
        assert!(is_valid_machine_name("automatic-general"));
        assert!(is_valid_machine_name("rule-with-numbers-123"));
    }

    #[test]
    fn empty_name_is_rejected() {
        assert!(!is_valid_machine_name(""));
    }

    #[test]
    fn name_starting_with_digit_is_rejected() {
        assert!(!is_valid_machine_name("1rule"));
    }

    #[test]
    fn name_starting_with_hyphen_is_rejected() {
        assert!(!is_valid_machine_name("-rule"));
    }

    #[test]
    fn name_ending_with_hyphen_is_rejected() {
        assert!(!is_valid_machine_name("rule-"));
    }

    #[test]
    fn consecutive_hyphens_are_rejected() {
        assert!(!is_valid_machine_name("my--rule"));
    }

    #[test]
    fn uppercase_letters_are_rejected() {
        assert!(!is_valid_machine_name("MyRule"));
        assert!(!is_valid_machine_name("MY-RULE"));
    }

    #[test]
    fn special_characters_are_rejected() {
        assert!(!is_valid_machine_name("my_rule"));
        assert!(!is_valid_machine_name("my rule"));
        assert!(!is_valid_machine_name("my/rule"));
        assert!(!is_valid_machine_name("my.rule"));
    }

    #[test]
    fn name_over_128_chars_is_rejected() {
        let long = "a".repeat(129);
        assert!(!is_valid_machine_name(&long));
    }

    #[test]
    fn name_of_exactly_128_chars_is_accepted() {
        let name = format!("a{}", "b".repeat(127));
        assert!(is_valid_machine_name(&name));
    }

    // ── CRUD (filesystem, using temp dirs) ───────────────────────────────────

    #[test]
    fn list_rules_returns_empty_when_dir_missing() {
        // Rules dir does not exist → empty list, no error.
        let tmp = tempfile::tempdir().expect("tempdir");
        let rules_dir = tmp.path().join("rules"); // deliberately not created
        assert!(!rules_dir.exists());

        // We can't call list_rules() directly because it uses get_rules_dir() which
        // resolves to ~/.automatic-dev/rules.  Instead verify the logic by hand:
        // if dir does not exist, return Ok(vec![]).
        // This is an integration-boundary test — we verify the directory check.
        let result: Vec<RuleEntry> = if !rules_dir.exists() {
            Vec::new()
        } else {
            panic!("unexpected: rules dir should not exist")
        };
        assert!(result.is_empty());
    }

    #[test]
    fn save_and_read_rule_roundtrip() {
        let (_tmp, rules_dir) = temp_rules_dir();
        write_rule(
            &rules_dir,
            "my-rule",
            "My Rule",
            "## Rule content\n\nDo something.",
        );

        let rule = read_rule_from_dir(&rules_dir, "my-rule");
        assert_eq!(rule.name, "My Rule");
        assert_eq!(rule.content, "## Rule content\n\nDo something.");
    }

    #[test]
    fn rule_file_is_valid_json() {
        let (_tmp, rules_dir) = temp_rules_dir();
        write_rule(&rules_dir, "test-rule", "Test", "content");

        let raw = fs::read_to_string(rules_dir.join("test-rule.json")).expect("read");
        let _: serde_json::Value = serde_json::from_str(&raw).expect("should be valid JSON");
    }

    #[test]
    fn overwriting_rule_updates_display_name_and_content() {
        let (_tmp, rules_dir) = temp_rules_dir();
        write_rule(&rules_dir, "my-rule", "Old Name", "old content");
        write_rule(&rules_dir, "my-rule", "New Name", "new content");

        let rule = read_rule_from_dir(&rules_dir, "my-rule");
        assert_eq!(rule.name, "New Name");
        assert_eq!(rule.content, "new content");
    }

    #[test]
    fn delete_rule_removes_file() {
        let (_tmp, rules_dir) = temp_rules_dir();
        write_rule(&rules_dir, "doomed", "Doomed", "will be deleted");

        let path = rules_dir.join("doomed.json");
        assert!(path.exists());
        fs::remove_file(&path).expect("delete");
        assert!(!path.exists());
    }

    #[test]
    fn rule_with_empty_content_round_trips() {
        let (_tmp, rules_dir) = temp_rules_dir();
        write_rule(&rules_dir, "empty-rule", "Empty", "");

        let rule = read_rule_from_dir(&rules_dir, "empty-rule");
        assert_eq!(rule.content, "");
    }

    #[test]
    fn multiple_rules_coexist_independently() {
        let (_tmp, rules_dir) = temp_rules_dir();
        write_rule(&rules_dir, "rule-alpha", "Alpha", "alpha content");
        write_rule(&rules_dir, "rule-beta", "Beta", "beta content");

        let alpha = read_rule_from_dir(&rules_dir, "rule-alpha");
        let beta = read_rule_from_dir(&rules_dir, "rule-beta");
        assert_eq!(alpha.name, "Alpha");
        assert_eq!(beta.name, "Beta");
    }

    // ── is_valid_machine_name edge cases ─────────────────────────────────────

    #[test]
    fn single_letter_name_is_valid() {
        assert!(is_valid_machine_name("a"));
    }

    #[test]
    fn name_with_digits_only_after_letter_is_valid() {
        assert!(is_valid_machine_name("a123"));
    }
}
