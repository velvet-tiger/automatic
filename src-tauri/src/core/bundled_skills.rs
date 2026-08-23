//! Install the app's built-in skills into `~/.automatic/library/skills/`.
//!
//! Skill content comes from two sources:
//!
//! - `bundled_library` — Automatic-authored engineering skills shipped via
//!   the `automatic-library` git submodule (see
//!   `src-tauri/src/core/bundled_library.rs`).
//! - `bundled_app_skills` — product/plugin-specific and third-party skills
//!   the app bundles directly.
//!
//! `install_default_skills_inner` writes the union of the library's own
//! `skills/skill.json` auto-install set and `bundled_app_skills::APP_AUTO_INSTALL`
//! to disk. `install_skills_from_bundle` supports on-demand installation of
//! any skill known to either source (including template-only skills).

use std::fs;

use super::asset_security::{
    enforce_text_asset, scan_text_asset_report, validate_relative_asset_path, AssetKind,
};
use super::bundled_app_skills::{APP_AUTO_INSTALL, APP_SKILL_CONTENTS, APP_SKILL_RESOURCES};
use super::bundled_library;
use super::paths::get_library_skills_dir;
use super::skill_store::record_skill_source;
use super::skills::record_skill_scan_state;

/// Skill content from either source. Owned strings so callers do not need to
/// worry about the underlying lifetime (app-side is `&'static`, library-side
/// is heap-allocated after archive extraction).
struct SkillPayload {
    /// The primary SKILL.md contents.
    skill_md: String,
    /// Companion files as `(relative path within the skill dir, contents)`.
    companions: Vec<(String, Vec<u8>)>,
}

fn load_skill(name: &str) -> Option<SkillPayload> {
    // App-vendored wins on collision, though there are none today.
    if let Some(md) = APP_SKILL_CONTENTS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, c)| *c)
    {
        let companions = APP_SKILL_RESOURCES
            .iter()
            .filter(|(skill, _, _)| *skill == name)
            .map(|(_, rel, content)| ((*rel).to_string(), content.as_bytes().to_vec()))
            .collect();
        return Some(SkillPayload {
            skill_md: md.to_string(),
            companions,
        });
    }

    let entry = bundled_library::skills().into_iter().find(|s| s.id == name)?;
    let mut skill_md = String::new();
    let mut companions = Vec::new();
    for file in &entry.files {
        let bytes = bundled_library::read_file(&file.path).ok()?;
        let rel = file.path.strip_prefix(&entry.root).unwrap_or(&file.path);
        let rel = rel.trim_start_matches('/');
        if rel.eq_ignore_ascii_case("SKILL.md") {
            skill_md = String::from_utf8(bytes).ok()?;
        } else {
            companions.push((rel.to_string(), bytes));
        }
    }
    Some(SkillPayload {
        skill_md,
        companions,
    })
}

/// Names of skills that should be installed automatically on first run.
/// Combines the library's own `skills/skill.json` auto-install set with
/// `bundled_app_skills::APP_AUTO_INSTALL`.
fn auto_install_skill_names() -> Vec<String> {
    let mut names = auto_install_from_library();
    for name in APP_AUTO_INSTALL {
        if !names.iter().any(|n| n == name) {
            names.push((*name).to_string());
        }
    }
    names
}

fn auto_install_from_library() -> Vec<String> {
    #[derive(serde::Deserialize)]
    struct SkillListEntry {
        name: String,
    }
    #[derive(serde::Deserialize)]
    struct SkillManifest {
        skills: Vec<SkillListEntry>,
    }

    let raw = match bundled_library::read_file_string("skills/skill.json") {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "[automatic] library skill.json unreadable, no library auto-install skills: {}",
                e
            );
            return Vec::new();
        }
    };
    let manifest: SkillManifest = match serde_json::from_str(&raw) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[automatic] library skill.json failed to parse: {}", e);
            return Vec::new();
        }
    };

    let library_ids: std::collections::HashSet<String> = bundled_library::skills()
        .into_iter()
        .map(|s| s.id)
        .collect();

    manifest
        .skills
        .into_iter()
        .filter(|entry| library_ids.contains(&entry.name))
        .map(|entry| entry.name)
        .collect()
}

/// Write auto-install skills to the managed library
/// (`~/.automatic/library/skills/`).
///
/// When `force` is `false` (normal first-run path), only missing skills are
/// written — files already on disk are left untouched.
///
/// When `force` is `true` (version-upgrade path), every auto-install skill
/// is overwritten unconditionally so the on-disk copies always match the
/// binary. Companion files are also refreshed.
///
/// Each skill is recorded in the skills registry with source
/// "automatic/automatic-app" so the UI resolves the author as "Automatic".
pub fn install_default_skills_inner(force: bool) -> Result<(), String> {
    let library_dir = get_library_skills_dir()?;
    let names = auto_install_skill_names();

    for name in &names {
        let Some(payload) = load_skill(name) else {
            continue;
        };
        let scan = scan_text_asset_report(AssetKind::Skill, &payload.skill_md);
        if scan.blocked() {
            return Err(scan.to_display_message(&format!("bundled skill '{}'", name)));
        }
        let skill_dir = library_dir.join(name);
        if !skill_dir.exists() {
            fs::create_dir_all(&skill_dir).map_err(|e| e.to_string())?;
        }
        let skill_path = skill_dir.join("SKILL.md");
        if force || !skill_path.exists() {
            fs::write(&skill_path, &payload.skill_md).map_err(|e| e.to_string())?;
        }
        install_companions(&skill_dir, name, &payload.companions, force)?;
        let _ = record_skill_scan_state(name, &scan.to_record());
        let id = format!("automatic/automatic-app/{}", name);
        let _ = record_skill_source(name, "automatic/automatic-app", &id, "bundled");
    }

    let _ = super::skills::set_skills_collection(&names, "automatic-skills");

    Ok(())
}

/// Convenience wrapper used by the erase-data path and the MCP tool where
/// write-once (non-forcing) behaviour is always correct.
pub fn install_default_skills() -> Result<(), String> {
    install_default_skills_inner(false)
}

/// Install a subset of bundled skills by name, skipping any that are already
/// present on disk. Searches both the app-vendored set and the library.
/// Silently ignores names that are unknown to either.
pub fn install_skills_from_bundle(skill_names: &[String]) -> Result<(), String> {
    let library_dir = get_library_skills_dir()?;

    for name in skill_names {
        let Some(payload) = load_skill(name) else {
            continue;
        };
        let scan = scan_text_asset_report(AssetKind::Skill, &payload.skill_md);
        if scan.blocked() {
            return Err(scan.to_display_message(&format!("bundled skill '{}'", name)));
        }
        let skill_dir = library_dir.join(name);
        if !skill_dir.exists() {
            fs::create_dir_all(&skill_dir).map_err(|e| e.to_string())?;
        }
        let skill_path = skill_dir.join("SKILL.md");
        if !skill_path.exists() {
            fs::write(&skill_path, &payload.skill_md).map_err(|e| e.to_string())?;
        }
        install_companions(&skill_dir, name, &payload.companions, false)?;
        let _ = record_skill_scan_state(name, &scan.to_record());
    }

    Ok(())
}

fn install_companions(
    skill_dir: &std::path::Path,
    skill_name: &str,
    companions: &[(String, Vec<u8>)],
    force: bool,
) -> Result<(), String> {
    for (rel_path, content) in companions {
        validate_relative_asset_path(rel_path, "bundled skill resource")?;
        // Text-content scan only when we can interpret as UTF-8. Non-UTF-8
        // companion files are written without a text scan (mirrors how the
        // old path only shipped ASCII content).
        if let Ok(text) = std::str::from_utf8(content) {
            enforce_text_asset(
                AssetKind::CompanionFile,
                &format!("bundled companion file '{}:{}'", skill_name, rel_path),
                text,
            )?;
        }
        let res_path = skill_dir.join(rel_path);
        if let Some(parent) = res_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
        }
        if force || !res_path.exists() {
            fs::write(&res_path, content).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Return the names of every skill available to the app across both the
/// library and the app-vendored set. Auto-install and template-only skills
/// combined.
pub fn bundled_skill_names() -> Vec<String> {
    let mut names: Vec<String> = APP_SKILL_CONTENTS
        .iter()
        .map(|(n, _)| (*n).to_string())
        .collect();
    for skill in bundled_library::skills() {
        if !names.iter().any(|n| *n == skill.id) {
            names.push(skill.id);
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::asset_security::{enforce_text_asset, AssetKind};
    use crate::core::paths::with_test_home;
    use std::path::Path;

    fn with_temp_home<T>(test: impl FnOnce(&Path) -> T) -> T {
        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), || test(temp.path()))
    }

    #[test]
    fn app_bundled_skill_contents_pass_security_scan() {
        for (name, content) in APP_SKILL_CONTENTS {
            let result = enforce_text_asset(
                AssetKind::Skill,
                &format!("app-bundled skill '{}'", name),
                content,
            );
            assert!(
                result.is_ok(),
                "expected app-bundled skill {} to pass: {:?}",
                name,
                result
            );
        }
    }

    #[test]
    fn library_bundled_skill_contents_pass_security_scan() {
        for skill in bundled_library::skills() {
            let payload = load_skill(&skill.id).expect("skill loadable");
            let result = enforce_text_asset(
                AssetKind::Skill,
                &format!("library skill '{}'", skill.id),
                &payload.skill_md,
            );
            assert!(
                result.is_ok(),
                "expected library skill {} to pass: {:?}",
                skill.id,
                result
            );
        }
    }

    #[test]
    fn install_default_skills_inner_writes_auto_installed_skills() {
        with_temp_home(|home| {
            install_default_skills_inner(false).expect("install default skills");

            // Product-specific skill from bundled_app_skills.
            assert!(home
                .join(".automatic-dev/library/skills/automatic/SKILL.md")
                .exists());
            // Library-provided auto-install skill.
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

    #[test]
    fn bundled_skill_names_covers_both_sources() {
        let names = bundled_skill_names();
        assert!(names.iter().any(|n| n == "automatic"));
        assert!(names.iter().any(|n| n == "php-pro"));
        assert!(names.iter().any(|n| n == "automatic-code-review"));
    }

    #[test]
    fn auto_install_includes_library_and_app_defaults() {
        let names = auto_install_skill_names();
        assert!(names.iter().any(|n| n == "automatic-code-review"));
        assert!(names.iter().any(|n| n == "automatic"));
        // Third-party skills are NOT auto-installed.
        assert!(!names.iter().any(|n| n == "php-pro"));
    }

    /// End-to-end parity for a library skill: the SKILL.md written to disk
    /// after `install_default_skills_inner` must match the bytes in the
    /// `automatic-library/` submodule byte-for-byte. Closes the last link in
    /// the pipeline (submodule → archive → in-memory extract → disk).
    #[test]
    fn library_skill_installs_verbatim() {
        with_temp_home(|home| {
            install_default_skills_inner(false).expect("install default skills");

            let installed = std::fs::read_to_string(
                home.join(".automatic-dev/library/skills/automatic-debugging/SKILL.md"),
            )
            .expect("installed SKILL.md readable");
            let source = std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .unwrap()
                    .join("automatic-library/skills/automatic-debugging/SKILL.md"),
            )
            .expect("source SKILL.md readable");
            assert_eq!(installed, source);
        });
    }
}
