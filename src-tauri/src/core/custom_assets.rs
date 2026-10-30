//! Names of a project's custom assets: `custom_skills`, `custom_commands`,
//! `custom_agents` and `custom_rules`.
//!
//! These names come from the committed `.automatic.json`, so a cloned repo
//! controls them, and sync turns them into file and directory names. A name
//! such as `../../.claude/agents/evil` would make sync write outside the
//! project. Every name is checked when a project is loaded or saved, and
//! again by each writer before it writes anything.

use super::paths::is_valid_name;
use super::types::{CustomAgent, Project};

/// `true` when `name` can be used as a single file or directory name.
///
/// Adds to [`is_valid_name`]: control characters, NUL among them, are never
/// part of a name a user meant to type. On Windows a `:` is refused too.
/// There `C:evil` is a drive-relative path, and `Path::join` replaces the
/// whole base path with it. Elsewhere `:` stays allowed, because command
/// names such as `git:commit` already sync there.
///
/// Windows also drops a trailing `.` or space, so there `my-skill.` would
/// write into `my-skill`, and `.. ` could reach the parent folder. Those
/// names stay allowed elsewhere, where the file system keeps them as typed.
pub fn is_valid_asset_name(name: &str) -> bool {
    is_valid_name(name)
        && !name.chars().any(char::is_control)
        && !(cfg!(windows)
            && (name.contains(':')
                || name.ends_with(['.', ' '])
                || is_windows_device_name(name)))
}

/// `true` when `name` is a Windows device name such as `CON` or `com1`, with
/// or without an extension. On Windows `CON.md` opens the console, not a file.
/// Windows also counts the superscript digits ¹, ² and ³ as port numbers.
/// `CONIN$` and `CONOUT$` are the console too.
pub(crate) fn is_windows_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).trim_end().to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$") {
        return true;
    }
    let Some(port) = stem.strip_prefix("COM").or_else(|| stem.strip_prefix("LPT")) else {
        return false;
    };
    let mut digits = port.chars();
    matches!(
        (digits.next(), digits.next()),
        (Some('1'..='9' | '\u{b9}' | '\u{b2}' | '\u{b3}'), None)
    )
}

/// The characters [`is_valid_asset_name`] refuses, for error messages.
const REFUSED_CHARACTERS: &str = if cfg!(windows) {
    "'/', '\\', ':' or control characters, end in '.' or a space, or be a device name such as CON"
} else {
    "'/', '\\' or control characters"
};

/// `Ok` when `name` passes [`is_valid_asset_name`]. The error names the asset
/// `kind` (for example "Custom skill") and says what a name may not contain.
pub fn ensure_valid_asset_name(kind: &str, name: &str) -> Result<(), String> {
    if is_valid_asset_name(name) {
        Ok(())
    } else {
        Err(format!(
            "{kind} name {name:?} cannot be used as a file name. A name cannot be empty, \
             \".\" or \"..\", and cannot contain {REFUSED_CHARACTERS}"
        ))
    }
}

/// The first non-empty `name` line in agent frontmatter, without quotes.
pub fn extract_agent_name(content: &str) -> Option<String> {
    for line in crate::agent::frontmatter_block(content)?.lines() {
        if let Some(name_val) = line.trim().strip_prefix("name:") {
            let name = name_val.trim().trim_matches('"').trim_matches('\'');
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// The file-name form of an agent name: lowercase, with hyphens for spaces.
pub fn agent_machine_slug(name: &str) -> String {
    name.to_lowercase().replace(' ', "-")
}

/// The `name` field of agent frontmatter, lowercased with spaces replaced by
/// hyphens. `None` when the content has no frontmatter or no `name`.
///
/// The result is not validated. Use [`agent_machine_name`] for any name that
/// becomes a file name.
pub fn extract_agent_machine_name(content: &str) -> Option<String> {
    extract_agent_name(content).map(|name| agent_machine_slug(&name))
}

/// The machine name a sub-agent is written under: its frontmatter `name`, or
/// `fallback` (lowercased, spaces to hyphens) when there is none.
///
/// The frontmatter can come from a cloned repo or an imported library agent,
/// so the result is checked with [`ensure_valid_asset_name`]. `kind` labels
/// the agent in the error, and a name taken from the frontmatter says so.
pub fn agent_machine_name(kind: &str, content: &str, fallback: &str) -> Result<String, String> {
    match extract_agent_machine_name(content) {
        Some(name) => {
            ensure_valid_asset_name(kind, &name).map_err(|e| {
                format!("{e}. It comes from the `name:` in the frontmatter of '{fallback}'")
            })?;
            Ok(name)
        }
        None => {
            let name = agent_machine_slug(fallback);
            ensure_valid_asset_name(kind, &name)?;
            Ok(name)
        }
    }
}

/// [`agent_machine_name`] for a project's custom agent.
pub fn custom_agent_machine_name(agent: &CustomAgent) -> Result<String, String> {
    agent_machine_name("Custom agent", &agent.content, &agent.name)
}

/// The slug in a custom rule's file name, `custom-<slug>.md`: the rule name
/// lowercased, with every run of other characters turned into one hyphen.
/// The slug holds only letters, digits and hyphens, so it cannot leave
/// `.automatic/instructions/`.
pub fn custom_rule_slug(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// `Ok` when a custom rule's name gives a non-empty slug.
///
/// Rule names are display names, and names such as "CI/CD" are normal, so
/// the name itself is not held to [`is_valid_asset_name`]. The slug is what
/// reaches the file system, and an empty one would name every such rule
/// `custom-.md`.
pub fn ensure_valid_custom_rule_name(name: &str) -> Result<(), String> {
    if custom_rule_slug(name).is_empty() {
        Err(format!(
            "Custom rule name {name:?} cannot be used as a file name. It needs at least one \
             letter or digit"
        ))
    } else {
        Ok(())
    }
}

/// Check the name of every custom asset in `project`. The error names the
/// first invalid entry.
pub fn validate_custom_asset_names(project: &Project) -> Result<(), String> {
    for skill in project.custom_skills.iter().flatten() {
        ensure_valid_asset_name("Custom skill", &skill.name)?;
    }
    for command in project.custom_commands.iter().flatten() {
        ensure_valid_asset_name("Custom command", &command.name)?;
    }
    for agent in project.custom_agents.iter().flatten() {
        custom_agent_machine_name(agent)?;
    }
    for rule in &project.custom_rules {
        ensure_valid_custom_rule_name(&rule.name)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{CustomCommand, CustomRule, CustomSkill};

    #[test]
    fn asset_names_refuse_traversal_and_separators() {
        for name in [
            "",
            ".",
            "..",
            "../evil",
            "a/b",
            "a\\b",
            "nul\0byte",
            "line\nbreak",
        ] {
            assert!(!is_valid_asset_name(name), "{name:?} must be refused");
        }
        for name in ["my-skill", "My Skill", "deploy_prod", ".hidden", "v1.2"] {
            assert!(is_valid_asset_name(name), "{name:?} must be accepted");
        }
        assert_eq!(is_valid_asset_name("C:evil"), !cfg!(windows));
        assert_eq!(is_valid_asset_name("git:commit"), !cfg!(windows));
        assert_eq!(is_valid_asset_name("con"), !cfg!(windows));
        assert_eq!(is_valid_asset_name("COM1.md"), !cfg!(windows));
        for name in ["my-skill.", "my-skill ", "...", ".. "] {
            assert_eq!(is_valid_asset_name(name), !cfg!(windows), "{name:?}");
        }
    }

    #[test]
    fn windows_device_names_are_recognised() {
        for name in [
            "CON",
            "con",
            "nul.md",
            "Com1",
            "LPT9.txt",
            "aux ",
            "COM\u{b9}",
            "lpt\u{b3}.md",
            "CONIN$",
            "conout$.md",
        ] {
            assert!(is_windows_device_name(name), "{name:?}");
        }
        for name in ["console", "com0", "com10", "lpt", "connect.md", "auxiliary"] {
            assert!(!is_windows_device_name(name), "{name:?}");
        }
    }

    /// Every caller reads the name through the one frontmatter reader, so a
    /// CRLF file names the same agent and a false closing line names none.
    #[test]
    fn agent_name_reads_crlf_and_needs_a_real_closing_line() {
        assert_eq!(
            extract_agent_name("---\r\nname: Planner\r\n---\r\n").as_deref(),
            Some("Planner")
        );
        assert_eq!(extract_agent_name("---\nname: x\n-----\nBody\n"), None);
        assert_eq!(extract_agent_name("---\nname: x\n---x\nBody\n"), None);
    }

    #[test]
    fn agent_machine_name_refuses_traversal_in_frontmatter() {
        let content = "---\nname: ../../.claude/agents/evil\n---\nBody\n";
        let err = agent_machine_name("Sub-agent", content, "Evil")
            .expect_err("traversal must be refused");
        assert!(err.contains("Evil"), "error names the agent: {err}");
        assert!(err.contains("frontmatter"), "error says where the name is: {err}");
    }

    #[test]
    fn agent_machine_name_refuses_traversal_in_fallback() {
        assert!(agent_machine_name("Sub-agent", "No frontmatter", "../evil").is_err());
        assert_eq!(
            agent_machine_name("Sub-agent", "No frontmatter", "Code Reviewer").unwrap(),
            "code-reviewer"
        );
    }

    #[test]
    fn rule_names_are_checked_by_slug() {
        assert!(ensure_valid_custom_rule_name("CI/CD conventions").is_ok());
        assert_eq!(custom_rule_slug("../../evil"), "evil");
        assert!(ensure_valid_custom_rule_name("../..").is_err());
    }

    fn project_with(f: impl FnOnce(&mut Project)) -> Project {
        let mut project = Project {
            name: "p".into(),
            ..Default::default()
        };
        f(&mut project);
        project
    }

    #[test]
    fn validate_refuses_each_kind() {
        let skill = project_with(|p| {
            p.custom_skills = Some(vec![CustomSkill {
                name: "../evil".into(),
                content: String::new(),
            }])
        });
        let command = project_with(|p| {
            p.custom_commands = Some(vec![CustomCommand {
                name: "../../../.claude/agents/evil".into(),
                content: String::new(),
            }])
        });
        let agent = project_with(|p| {
            p.custom_agents = Some(vec![CustomAgent {
                name: "Evil".into(),
                content: "---\nname: ../evil\n---\n".into(),
            }])
        });
        let rule = project_with(|p| {
            p.custom_rules = vec![CustomRule {
                name: "../..".into(),
                content: "x".into(),
            }]
        });
        for (kind, project) in [
            ("Custom skill", skill),
            ("Custom command", command),
            ("Custom agent", agent),
            ("Custom rule", rule),
        ] {
            let err = validate_custom_asset_names(&project).expect_err(kind);
            assert!(err.starts_with(kind), "error names the kind: {err}");
        }
    }

    #[test]
    fn validate_accepts_ordinary_names() {
        let project = project_with(|p| {
            p.custom_skills = Some(vec![CustomSkill {
                name: "my-skill".into(),
                content: String::new(),
            }]);
            p.custom_commands = Some(vec![CustomCommand {
                name: "deploy".into(),
                content: String::new(),
            }]);
            p.custom_agents = Some(vec![CustomAgent {
                name: "Code Reviewer".into(),
                content: "Body".into(),
            }]);
            p.custom_rules = vec![CustomRule {
                name: "CI/CD conventions".into(),
                content: "x".into(),
            }];
        });
        assert!(validate_custom_asset_names(&project).is_ok());
    }
}
