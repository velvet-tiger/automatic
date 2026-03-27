use std::fs;
use std::path::PathBuf;

use super::paths::{get_agents_skills_dir, get_automatic_dir, is_valid_name};
use super::skill_store::record_skill_source;

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

/// The bundled skill manifest (`src-tauri/skills/skill.json`), embedded at
/// compile time.  At runtime, `auto_install_skill_names()` parses this to
/// determine which skills should be written to `~/.agents/skills/` on startup.
/// To add a new auto-install skill: add it to `skill.json` and add its
/// `include_str!` entry here.  No other code changes are required.
const BUNDLED_SKILL_JSON: &str = include_str!("../../skills/skill.json");

/// All skill content shipped with the binary, keyed by skill name.
/// Skills listed in `skill.json` are auto-installed; all others are
/// available on demand (e.g. selected via a project template).
const BUNDLED_SKILL_CONTENTS: &[(&str, &str)] = &[
    ("automatic", include_str!("../../skills/automatic/SKILL.md")),
    (
        "automatic-features",
        include_str!("../../skills/automatic-features/SKILL.md"),
    ),
    (
        "automatic-api-design",
        include_str!("../../skills/automatic-api-design/SKILL.md"),
    ),
    (
        "automatic-code-review",
        include_str!("../../skills/automatic-code-review/SKILL.md"),
    ),
    (
        "automatic-database-design",
        include_str!("../../skills/automatic-database-design/SKILL.md"),
    ),
    (
        "automatic-debugging",
        include_str!("../../skills/automatic-debugging/SKILL.md"),
    ),
    (
        "automatic-documentation",
        include_str!("../../skills/automatic-documentation/SKILL.md"),
    ),
    (
        "automatic-llms-txt",
        include_str!("../../skills/automatic-llms-txt/SKILL.md"),
    ),
    (
        "automatic-performance",
        include_str!("../../skills/automatic-performance/SKILL.md"),
    ),
    (
        "automatic-refactoring",
        include_str!("../../skills/automatic-refactoring/SKILL.md"),
    ),
    (
        "automatic-security-review",
        include_str!("../../skills/automatic-security-review/SKILL.md"),
    ),
    (
        "automatic-testing",
        include_str!("../../skills/automatic-testing/SKILL.md"),
    ),
    // Plugin-provided skills (installed by the auto-docs plugin)
    (
        "automatic-docs",
        include_str!("../../skills/automatic-docs/SKILL.md"),
    ),
    (
        "automatic-docs-find",
        include_str!("../../skills/automatic-docs-find/SKILL.md"),
    ),
    // Template-only skills (on-demand, not auto-installed)
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
    ("php-pro", include_str!("../../skills/php-pro/SKILL.md")),
    (
        "python-pro",
        include_str!("../../skills/python-pro/SKILL.md"),
    ),
];

/// Companion resource files shipped with bundled skills.
/// Each entry is (skill_name, relative_path, content).
/// These are installed alongside the SKILL.md when the skill is written to disk.
const BUNDLED_SKILL_RESOURCES: &[(&str, &str, &str)] = &[
    (
        "automatic-docs",
        "references/specification.md",
        include_str!("../../skills/automatic-docs/references/specification.md"),
    ),
];

/// Parse `skill.json` (embedded at compile time) and return the names of
/// skills that should be auto-installed.  Falls back to an empty list if
/// the JSON cannot be parsed, so a malformed manifest never hard-crashes startup.
fn auto_install_skill_names() -> Vec<&'static str> {
    #[derive(serde::Deserialize)]
    struct SkillEntry {
        name: String,
    }
    #[derive(serde::Deserialize)]
    struct Manifest {
        skills: Vec<SkillEntry>,
    }

    let manifest: Manifest = match serde_json::from_str(BUNDLED_SKILL_JSON) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[automatic] failed to parse bundled skill.json: {}", e);
            return Vec::new();
        }
    };

    // Return only names that also have content in BUNDLED_SKILL_CONTENTS.
    manifest
        .skills
        .into_iter()
        .filter_map(|entry| {
            BUNDLED_SKILL_CONTENTS
                .iter()
                .find(|(n, _)| *n == entry.name.as_str())
                .map(|(n, _)| *n)
        })
        .collect()
}

/// Write auto-install skills to `~/.agents/skills/`.
///
/// The set of skills to install is read from the embedded `skill.json` manifest,
/// so adding a new default skill only requires updating that file and adding a
/// corresponding `include_str!` entry in `BUNDLED_SKILL_CONTENTS`.
///
/// When `force` is `false` (normal first-run path), only missing skills are
/// written — files already on disk are left untouched.
///
/// When `force` is `true` (version-upgrade path), every auto-install skill is
/// overwritten unconditionally so the on-disk copies always match the binary.
///
/// Each skill is recorded in the skills registry with source
/// "automatic/automatic-app" so the UI resolves the author as "Automatic".
pub fn install_default_skills_inner(force: bool) -> Result<(), String> {
    let agents_dir = get_agents_skills_dir()?;
    let names = auto_install_skill_names();

    for name in &names {
        let Some((_, content)) = BUNDLED_SKILL_CONTENTS.iter().find(|(n, _)| n == name) else {
            continue;
        };
        let skill_dir = agents_dir.join(name);
        if !skill_dir.exists() {
            fs::create_dir_all(&skill_dir).map_err(|e| e.to_string())?;
        }
        let skill_path = skill_dir.join("SKILL.md");
        if force || !skill_path.exists() {
            fs::write(&skill_path, content).map_err(|e| e.to_string())?;
        }
        // Register source so the UI shows "Automatic" as the author.
        // Best-effort — registry I/O errors must not prevent skill installation.
        let id = format!("automatic/automatic-app/{}", name);
        let _ = record_skill_source(name, "automatic/automatic-app", &id, "bundled");
    }

    // Auto-assign bundled skills to the "automatic-skills" collection.
    let name_strings: Vec<String> = names.iter().map(|s| s.to_string()).collect();
    let _ = super::skills::set_skills_collection(&name_strings, "automatic-skills");

    Ok(())
}

/// Convenience wrapper used by the erase-data path and the MCP tool where
/// write-once (non-forcing) behaviour is always correct.
pub fn install_default_skills() -> Result<(), String> {
    install_default_skills_inner(false)
}

/// Install a subset of bundled skills by name, skipping any that are already
/// present on disk.  Searches all of `BUNDLED_SKILL_CONTENTS`.
/// Silently ignores names not found in the bundle.
pub fn install_skills_from_bundle(skill_names: &[String]) -> Result<(), String> {
    let agents_dir = get_agents_skills_dir()?;

    for name in skill_names {
        let Some((_, content)) = BUNDLED_SKILL_CONTENTS
            .iter()
            .find(|(n, _)| *n == name.as_str())
        else {
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

        // Install companion resource files for this skill.
        for (res_skill, rel_path, res_content) in BUNDLED_SKILL_RESOURCES {
            if *res_skill != name.as_str() {
                continue;
            }
            let res_path = skill_dir.join(rel_path);
            if let Some(parent) = res_path.parent() {
                if !parent.exists() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
            }
            if !res_path.exists() {
                fs::write(&res_path, res_content).map_err(|e| e.to_string())?;
            }
        }
    }

    Ok(())
}

/// Return the names of all skills shipped with the app (auto-install and
/// template-only combined).
pub fn bundled_skill_names() -> Vec<&'static str> {
    BUNDLED_SKILL_CONTENTS
        .iter()
        .map(|(name, _)| *name)
        .collect()
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

/// Write default templates to `~/.automatic/templates/`.
///
/// When `force` is `false`, existing files are left untouched so user edits
/// are preserved.  When `force` is `true`, every bundled template is
/// overwritten unconditionally — used by the "Reinstall Defaults" reset path.
pub fn install_default_templates_inner(force: bool) -> Result<(), String> {
    let dir = get_templates_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    for (name, content) in DEFAULT_TEMPLATES {
        let path = dir.join(format!("{}.md", name));
        if force || !path.exists() {
            fs::write(&path, content).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

/// Write any missing default templates to `~/.automatic/templates/`.
/// Existing files are left untouched, so user edits are always preserved.
pub fn install_default_templates() -> Result<(), String> {
    install_default_templates_inner(false)
}
