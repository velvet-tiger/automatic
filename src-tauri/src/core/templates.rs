use std::fs;
use std::path::PathBuf;

use super::paths::{get_agents_skills_dir, get_automatic_dir, is_valid_name};

// ── Templates ────────────────────────────────────────────────────────────────

pub fn get_templates_dir() -> Result<PathBuf, String> {
    Ok(get_automatic_dir()?.join("templates"))
}

pub fn list_templates() -> Result<Vec<String>, String> {
    let dir = get_templates_dir()?;

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut templates = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| e.to_string())?;

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "md") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if is_valid_name(stem) {
                        templates.push(stem.to_string());
                    }
                }
            }
        }
    }

    Ok(templates)
}

pub fn read_template(name: &str) -> Result<String, String> {
    if !is_valid_name(name) {
        return Err("Invalid template name".into());
    }
    let dir = get_templates_dir()?;
    let path = dir.join(format!("{}.md", name));

    if path.exists() {
        fs::read_to_string(path).map_err(|e| e.to_string())
    } else {
        Err(format!("Template '{}' not found", name))
    }
}

pub fn save_template(name: &str, content: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid template name".into());
    }

    let dir = get_templates_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    let path = dir.join(format!("{}.md", name));
    fs::write(path, content).map_err(|e| e.to_string())
}

pub fn delete_template(name: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid template name".into());
    }
    let dir = get_templates_dir()?;
    let path = dir.join(format!("{}.md", name));

    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Built-in skills shipped with the app.  Each entry is (name, content).
/// These are written to `~/.agents/skills/<name>/SKILL.md` on first run (or
/// when the file is missing), but never overwrite existing files — user edits
/// are always preserved.
const DEFAULT_SKILLS: &[(&str, &str)] = &[
    ("automatic", include_str!("../../skills/automatic/SKILL.md")),
    // Skills required by bundled marketplace templates.
    // Never overwrite existing user installations.
    (
        "vercel-react-best-practices",
        include_str!("../../skills/vercel-react-best-practices/SKILL.md"),
    ),
    (
        "tailwindcss-development",
        include_str!("../../skills/tailwindcss-development/SKILL.md"),
    ),
    (
        "laravel-specialist",
        include_str!("../../skills/laravel-specialist/SKILL.md"),
    ),
    (
        "pennant-development",
        include_str!("../../skills/pennant-development/SKILL.md"),
    ),
    (
        "terraform-skill",
        include_str!("../../skills/terraform-skill/SKILL.md"),
    ),
    (
        "github-workflow-automation",
        include_str!("../../skills/github-workflow-automation/SKILL.md"),
    ),
];

/// Write any missing default skills to `~/.agents/skills/`.
/// Existing files are left untouched, so user edits are always preserved.
pub fn install_default_skills() -> Result<(), String> {
    let agents_dir = get_agents_skills_dir()?;

    for (name, content) in DEFAULT_SKILLS {
        let skill_dir = agents_dir.join(name);
        if !skill_dir.exists() {
            fs::create_dir_all(&skill_dir).map_err(|e| e.to_string())?;
        }
        let skill_path = skill_dir.join("SKILL.md");
        if !skill_path.exists() {
            fs::write(&skill_path, content).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

/// Install a subset of bundled skills by name, skipping any that are already
/// present on disk.  Silently ignores names not found in DEFAULT_SKILLS.
pub fn install_skills_from_bundle(skill_names: &[String]) -> Result<(), String> {
    let agents_dir = get_agents_skills_dir()?;

    for name in skill_names {
        // Only install if it's actually bundled.
        let Some((_, content)) = DEFAULT_SKILLS.iter().find(|(n, _)| *n == name.as_str()) else {
            continue;
        };
        let skill_dir = agents_dir.join(name);
        if !skill_dir.exists() {
            fs::create_dir_all(&skill_dir).map_err(|e| e.to_string())?;
        }
        let skill_path = skill_dir.join("SKILL.md");
        if !skill_path.exists() {
            fs::write(&skill_path, content).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

/// Return the set of skill names that are shipped with the app (i.e. present
/// in DEFAULT_SKILLS).
pub fn bundled_skill_names() -> Vec<&'static str> {
    DEFAULT_SKILLS.iter().map(|(name, _)| *name).collect()
}

/// Built-in templates shipped with the app.  Each entry is (name, content).
/// These are written to `~/.automatic/templates/` on first run (or when missing),
/// but never overwrite a file that already exists — user edits are preserved.
const DEFAULT_TEMPLATES: &[(&str, &str)] = &[
    (
        "Agent Project Brief",
        include_str!("../../templates/Agent Project Brief.md"),
    ),
    (
        "Session Context",
        include_str!("../../templates/Session Context.md"),
    ),
];

/// Write any missing default templates to `~/.automatic/templates/`.
/// Existing files are left untouched, so user edits are always preserved.
pub fn install_default_templates() -> Result<(), String> {
    let dir = get_templates_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    for (name, content) in DEFAULT_TEMPLATES {
        let path = dir.join(format!("{}.md", name));
        if !path.exists() {
            fs::write(&path, content).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}
