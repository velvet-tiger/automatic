use std::fs;

use super::asset_security::{
    enforce_text_asset, scan_text_asset_report, validate_relative_asset_path, AssetKind,
};
use super::paths::get_library_skills_dir;
use super::skill_store::record_skill_source;
use super::skills::record_skill_scan_state;

/// The bundled skill manifest (`src-tauri/assets/skills/skill.json`),
/// embedded at compile time. At runtime, `auto_install_skill_names()` parses
/// this to determine which skills should be written to the managed library
/// (`~/.automatic/library/skills/`) on startup. To add a new auto-install
/// skill: add it to `skill.json` and add its `include_str!` entry below. No
/// other code changes are required.
const BUNDLED_SKILL_JSON: &str = include_str!("../../assets/skills/skill.json");

/// All skill content shipped with the binary, keyed by skill name. Skills
/// listed in `skill.json` are auto-installed; all others are available on
/// demand (e.g. selected via a project template).
const BUNDLED_SKILL_CONTENTS: &[(&str, &str)] = &[
    (
        "automatic",
        include_str!("../../assets/skills/automatic/SKILL.md"),
    ),
    (
        "automatic-features",
        include_str!("../../assets/skills/automatic-features/SKILL.md"),
    ),
    (
        "automatic-api-design",
        include_str!("../../assets/skills/automatic-api-design/SKILL.md"),
    ),
    (
        "automatic-code-review",
        include_str!("../../assets/skills/automatic-code-review/SKILL.md"),
    ),
    (
        "automatic-database-design",
        include_str!("../../assets/skills/automatic-database-design/SKILL.md"),
    ),
    (
        "automatic-debugging",
        include_str!("../../assets/skills/automatic-debugging/SKILL.md"),
    ),
    (
        "automatic-documentation",
        include_str!("../../assets/skills/automatic-documentation/SKILL.md"),
    ),
    (
        "automatic-llms-txt",
        include_str!("../../assets/skills/automatic-llms-txt/SKILL.md"),
    ),
    (
        "automatic-performance",
        include_str!("../../assets/skills/automatic-performance/SKILL.md"),
    ),
    (
        "automatic-refactoring",
        include_str!("../../assets/skills/automatic-refactoring/SKILL.md"),
    ),
    (
        "automatic-security-review",
        include_str!("../../assets/skills/automatic-security-review/SKILL.md"),
    ),
    (
        "automatic-testing",
        include_str!("../../assets/skills/automatic-testing/SKILL.md"),
    ),
    (
        "automatic-remote-source-authoring",
        include_str!("../../assets/skills/automatic-remote-source-authoring/SKILL.md"),
    ),
    (
        "claude-4-7",
        include_str!("../../assets/skills/claude-4-7/SKILL.md"),
    ),
    // Template-only skills (on-demand, not auto-installed)
    (
        "vercel-react-best-practices",
        include_str!("../../assets/skills/vercel-react-best-practices/SKILL.md"),
    ),
    (
        "tailwindcss-development",
        include_str!("../../assets/skills/tailwindcss-development/SKILL.md"),
    ),
    (
        "laravel-specialist",
        include_str!("../../assets/skills/laravel-specialist/SKILL.md"),
    ),
    (
        "pennant-development",
        include_str!("../../assets/skills/pennant-development/SKILL.md"),
    ),
    (
        "terraform-skill",
        include_str!("../../assets/skills/terraform-skill/SKILL.md"),
    ),
    (
        "php-pro",
        include_str!("../../assets/skills/php-pro/SKILL.md"),
    ),
    (
        "python-pro",
        include_str!("../../assets/skills/python-pro/SKILL.md"),
    ),
];

/// Companion resource files shipped with bundled skills. Each entry is
/// (skill_name, relative_path, content). These are installed alongside the
/// SKILL.md when the skill is written to disk.
const BUNDLED_SKILL_RESOURCES: &[(&str, &str, &str)] = &[];

/// Parse `skill.json` (embedded at compile time) and return the names of
/// skills that should be auto-installed. Falls back to an empty list if the
/// JSON cannot be parsed, so a malformed manifest never hard-crashes startup.
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

/// Write auto-install skills to the managed library
/// (`~/.automatic/library/skills/`).
///
/// The set of skills to install is read from the embedded `skill.json`
/// manifest, so adding a new default skill only requires updating that file
/// and adding a corresponding `include_str!` entry in
/// `BUNDLED_SKILL_CONTENTS`.
///
/// When `force` is `false` (normal first-run path), only missing skills are
/// written — files already on disk are left untouched.
///
/// When `force` is `true` (version-upgrade path), every auto-install skill
/// is overwritten unconditionally so the on-disk copies always match the
/// binary.
///
/// Each skill is recorded in the skills registry with source
/// "automatic/automatic-app" so the UI resolves the author as "Automatic".
pub fn install_default_skills_inner(force: bool) -> Result<(), String> {
    let library_dir = get_library_skills_dir()?;
    let names = auto_install_skill_names();

    for name in &names {
        let Some((_, content)) = BUNDLED_SKILL_CONTENTS.iter().find(|(n, _)| n == name) else {
            continue;
        };
        let scan = scan_text_asset_report(AssetKind::Skill, content);
        if scan.blocked() {
            return Err(scan.to_display_message(&format!("bundled skill '{}'", name)));
        }
        let skill_dir = library_dir.join(name);
        if !skill_dir.exists() {
            fs::create_dir_all(&skill_dir).map_err(|e| e.to_string())?;
        }
        let skill_path = skill_dir.join("SKILL.md");
        if force || !skill_path.exists() {
            fs::write(&skill_path, content).map_err(|e| e.to_string())?;
        }
        let _ = record_skill_scan_state(name, &scan.to_record());
        let id = format!("automatic/automatic-app/{}", name);
        let _ = record_skill_source(name, "automatic/automatic-app", &id, "bundled");
    }

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
/// present on disk. Searches all of `BUNDLED_SKILL_CONTENTS`. Silently
/// ignores names not found in the bundle.
pub fn install_skills_from_bundle(skill_names: &[String]) -> Result<(), String> {
    let library_dir = get_library_skills_dir()?;

    for name in skill_names {
        let Some((_, content)) = BUNDLED_SKILL_CONTENTS
            .iter()
            .find(|(n, _)| *n == name.as_str())
        else {
            continue;
        };
        let scan = scan_text_asset_report(AssetKind::Skill, content);
        if scan.blocked() {
            return Err(scan.to_display_message(&format!("bundled skill '{}'", name)));
        }
        let skill_dir = library_dir.join(name);
        if !skill_dir.exists() {
            fs::create_dir_all(&skill_dir).map_err(|e| e.to_string())?;
        }
        let skill_path = skill_dir.join("SKILL.md");
        if !skill_path.exists() {
            fs::write(&skill_path, content).map_err(|e| e.to_string())?;
        }
        let _ = record_skill_scan_state(name, &scan.to_record());

        for (res_skill, rel_path, res_content) in BUNDLED_SKILL_RESOURCES {
            if *res_skill != name.as_str() {
                continue;
            }
            validate_relative_asset_path(rel_path, "bundled skill resource")?;
            enforce_text_asset(
                AssetKind::CompanionFile,
                &format!("bundled companion file '{}:{}'", name, rel_path),
                res_content,
            )?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::asset_security::{
        enforce_text_asset, validate_relative_asset_path, AssetKind,
    };
    use crate::core::paths::with_test_home;
    use crate::core::types::SkillsJson;
    use std::path::Path;

    fn with_temp_home<T>(test: impl FnOnce(&Path) -> T) -> T {
        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), || test(temp.path()))
    }

    #[test]
    fn bundled_skill_manifest_paths_are_safe() {
        let manifest: SkillsJson =
            serde_json::from_str(BUNDLED_SKILL_JSON).expect("bundled skill.json should parse");

        for skill in manifest.skills {
            if skill.path != "." && !skill.path.is_empty() {
                validate_relative_asset_path(&skill.path, "bundled skill path")
                    .expect("bundled skill path should be safe");
            }

            validate_relative_asset_path(skill.entrypoint_file(), "bundled skill entrypoint")
                .expect("bundled skill entrypoint should be safe");
        }
    }

    #[test]
    fn bundled_skill_contents_pass_security_scan() {
        for (name, content) in BUNDLED_SKILL_CONTENTS {
            let result = enforce_text_asset(
                AssetKind::Skill,
                &format!("bundled skill '{}'", name),
                content,
            );
            assert!(
                result.is_ok(),
                "expected bundled skill {} to pass: {:?}",
                name,
                result
            );
        }
    }

    #[test]
    fn install_default_skills_inner_writes_auto_installed_skills() {
        with_temp_home(|home| {
            install_default_skills_inner(false).expect("install default skills");

            assert!(home
                .join(".automatic-dev/library/skills/automatic/SKILL.md")
                .exists());
            assert!(home
                .join(".automatic-dev/library/skills/automatic-code-review/SKILL.md")
                .exists());
        });
    }

    #[test]
    fn install_skills_from_bundle_writes_requested_skill() {
        with_temp_home(|home| {
            install_skills_from_bundle(&["php-pro".to_string()]).expect("install bundle subset");

            assert!(home
                .join(".automatic-dev/library/skills/php-pro/SKILL.md")
                .exists());
        });
    }
}
