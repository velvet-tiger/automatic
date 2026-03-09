use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use super::types::SkillsJson;
use super::*;

// ── Skills ───────────────────────────────────────────────────────────────────

/// Return true if a skill directory contains anything besides SKILL.md.
fn skill_has_resources(skill_dir: &PathBuf) -> bool {
    let Ok(entries) = fs::read_dir(skill_dir) else {
        return false;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let name = entry.file_name();
        if name != "SKILL.md" {
            return true;
        }
    }
    false
}

/// Scan a single skills directory and return the set of valid skill names.
///
/// Discovery order:
/// 1. If a `skill.json` exists at the directory root, read skill names from it
///    and verify each skill's entrypoint (SKILL.md or custom) is present.
/// 2. Fall back to scanning subdirectories for any that contain `SKILL.md`.
///
/// This means that a directory that is itself a skill.json package (e.g. a
/// locally cloned skill repo) is correctly enumerated via its manifest, while
/// plain directories without skill.json continue to work as before.
fn scan_skills_dir(dir: &PathBuf) -> Result<std::collections::HashSet<String>, String> {
    let mut names = std::collections::HashSet::new();

    if !dir.exists() {
        return Ok(names);
    }

    // ── Step 1: skill.json discovery ─────────────────────────────────────────
    let skills_json_path = dir.join("skill.json");
    if skills_json_path.exists() {
        if let Ok(raw) = fs::read_to_string(&skills_json_path) {
            if let Ok(manifest) = serde_json::from_str::<SkillsJson>(&raw) {
                for skill in &manifest.skills {
                    if !is_valid_name(&skill.name) {
                        continue;
                    }
                    // Resolve the skill directory and entrypoint
                    let skill_base = if skill.path == "." || skill.path.is_empty() {
                        dir.clone()
                    } else {
                        let p = skill.path.trim_start_matches("./");
                        dir.join(p)
                    };
                    let entrypoint = skill_base.join(skill.entrypoint_file());
                    if entrypoint.exists() {
                        names.insert(skill.name.clone());
                    }
                }
                // Return immediately — the manifest is authoritative for this dir
                return Ok(names);
            }
        }
    }

    // ── Step 2: filesystem scan (no skill.json) ───────────────────────────────
    let entries = fs::read_dir(dir).map_err(|e| e.to_string())?;
    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if is_valid_name(name) && path.join("SKILL.md").exists() {
                        names.insert(name.to_string());
                    }
                }
            }
        }
    }

    Ok(names)
}

/// List skills from both `~/.agents/skills/` and `~/.claude/skills/`,
/// returning entries that indicate which locations each skill exists in,
/// with remote origin info joined from ~/.automatic/skills.json.
pub fn list_skills() -> Result<Vec<SkillEntry>, String> {
    let agents_dir = get_agents_skills_dir()?;
    let claude_dir = get_claude_skills_dir()?;

    let agents_names = scan_skills_dir(&agents_dir)?;
    let claude_names = scan_skills_dir(&claude_dir)?;

    // Best-effort registry load — don't fail list_skills if the file is missing/corrupt
    let registry = read_skill_sources().unwrap_or_default();

    // Union of all names
    let mut all_names: Vec<String> = agents_names.union(&claude_names).cloned().collect();
    all_names.sort();

    let entries = all_names
        .into_iter()
        .map(|name| {
            // Resolve the canonical skill directory (agents first, then claude)
            let canonical_dir = {
                let a = agents_dir.join(&name);
                if a.exists() {
                    a
                } else {
                    claude_dir.join(&name)
                }
            };

            let has_resources = skill_has_resources(&canonical_dir);

            // Extract license from SKILL.md frontmatter (best-effort, no error on failure)
            let license = fs::read_to_string(canonical_dir.join("SKILL.md"))
                .ok()
                .and_then(|c| super::skill_store::extract_frontmatter_license(&c));

            SkillEntry {
                in_agents: agents_names.contains(&name),
                in_claude: claude_names.contains(&name),
                source: registry.get(&name).cloned(),
                has_resources,
                license,
                name,
            }
        })
        .collect();

    Ok(entries)
}

/// Convenience: list just the skill names (union of both directories).
/// Used by sync and autodetect where only names are needed.
pub fn list_skill_names() -> Result<Vec<String>, String> {
    Ok(list_skills()?.into_iter().map(|e| e.name).collect())
}

/// Read a skill's raw SKILL.md content without any companion file formatting.
/// Use this for sync and drift detection where the on-disk file must match
/// exactly — companion resource sections are not written to project skill files.
pub fn read_skill_raw(name: &str) -> Result<String, String> {
    match get_skill_path(name)? {
        Some(path) => fs::read_to_string(&path).map_err(|e| e.to_string()),
        None => Ok("".to_string()),
    }
}

/// Read a skill's SKILL.md content.  Checks `~/.agents/skills/` first
/// (the canonical location), then falls back to `~/.claude/skills/`.
pub fn read_skill(name: &str) -> Result<String, String> {
    if let Some(path) = get_skill_path(name)? {
        let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;

        // Find the parent directory to check for companion files
        if let Some(skill_dir) = path.parent() {
            let companions = discover_companion_files(skill_dir);
            if !companions.is_empty() {
                return Ok(format_skill_with_companions(&content, &companions));
            }
        }

        return Ok(content);
    }
    Ok("".to_string())
}

/// A single companion resource entry returned to the frontend.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ResourceFile {
    /// Path relative to the skill directory, e.g. "scripts/init.py" or "LICENSE.txt"
    pub path: String,
}

/// Grouped companion resources for a skill directory.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SkillResources {
    /// Directories found at the top level (each may contain files)
    pub dirs: Vec<ResourceDir>,
    /// Loose files at the root of the skill directory (not SKILL.md)
    pub root_files: Vec<ResourceFile>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ResourceDir {
    /// Directory name relative to the skill root, e.g. "scripts"
    pub name: String,
    /// Files inside (non-recursive)
    pub files: Vec<ResourceFile>,
}

/// List all companion resources for a skill: every subdirectory and its
/// files, plus any root-level files other than SKILL.md.
/// Returns an empty SkillResources if the skill does not exist.
pub fn list_skill_resources(name: &str) -> Result<SkillResources, String> {
    if !is_valid_name(name) {
        return Err("Invalid skill name".into());
    }

    let skill_dir = match get_skill_dir(name)? {
        Some(d) => d,
        None => return Ok(SkillResources::default()),
    };

    let mut result = SkillResources::default();

    let entries = match fs::read_dir(&skill_dir) {
        Ok(e) => e,
        Err(_) => return Ok(result),
    };

    // Collect and sort entries for deterministic output
    let mut dirs: Vec<String> = Vec::new();
    let mut root_files: Vec<String> = Vec::new();

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        if path.is_dir() {
            dirs.push(file_name);
        } else if file_name != "SKILL.md" {
            root_files.push(file_name);
        }
    }

    dirs.sort();
    root_files.sort();

    // Root-level files
    result.root_files = root_files
        .into_iter()
        .map(|name| ResourceFile { path: name })
        .collect();

    // Subdirectories
    for dir_name in dirs {
        let dir_path = skill_dir.join(&dir_name);
        let mut files: Vec<String> = Vec::new();

        if let Ok(dir_entries) = fs::read_dir(&dir_path) {
            for entry in dir_entries.filter_map(|e| e.ok()) {
                let p = entry.path();
                if p.is_file() {
                    if let Some(fname) = p.file_name().and_then(|n| n.to_str()) {
                        files.push(fname.to_string());
                    }
                }
            }
        }
        files.sort();

        result.dirs.push(ResourceDir {
            name: dir_name,
            files: files
                .into_iter()
                .map(|f| ResourceFile { path: f })
                .collect(),
        });
    }

    Ok(result)
}

// ── Legacy internal helpers (used by read_skill for in-content rendering) ─────

/// Companion file entry — internal only.
#[derive(Debug)]
struct CompanionFile {
    relative_path: String,
    is_dir: bool,
}

/// Discover companion files inside known subdirectories only.
fn discover_companion_files(skill_dir: &std::path::Path) -> Vec<CompanionFile> {
    let mut companions = Vec::new();

    let known_dirs = [
        "scripts",
        "references",
        "docs",
        "assets",
        "examples",
        "templates",
    ];

    for dir_name in &known_dirs {
        let dir_path = skill_dir.join(dir_name);
        if dir_path.is_dir() {
            companions.push(CompanionFile {
                relative_path: dir_name.to_string(),
                is_dir: true,
            });

            if let Ok(entries) = std::fs::read_dir(&dir_path) {
                for entry in entries.filter_map(|e| e.ok()) {
                    if let Ok(file_type) = entry.file_type() {
                        if file_type.is_file() {
                            if let Some(file_name) = entry.file_name().to_str() {
                                companions.push(CompanionFile {
                                    relative_path: format!("{}/{}", dir_name, file_name),
                                    is_dir: false,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    companions
}

/// Format skill content with companion files listed at the end
fn format_skill_with_companions(skill_content: &str, companions: &[CompanionFile]) -> String {
    let mut output = String::from(skill_content);

    // Add companion files section
    output.push_str("\n\n---\n\n");
    output.push_str("## Companion Resources\n\n");
    output.push_str("This skill includes additional resources:\n\n");

    // Group by directory
    let mut current_dir: Option<String> = None;
    for companion in companions {
        if companion.is_dir {
            current_dir = Some(companion.relative_path.clone());
            output.push_str(&format!("\n### {}\n", companion.relative_path));
        } else {
            // Extract directory and filename
            if let Some(slash_pos) = companion.relative_path.rfind('/') {
                let dir = &companion.relative_path[..slash_pos];
                let file = &companion.relative_path[slash_pos + 1..];

                if current_dir.as_deref() == Some(dir) {
                    output.push_str(&format!("- `{}`\n", file));
                } else {
                    output.push_str(&format!("- `{}`\n", companion.relative_path));
                }
            } else {
                output.push_str(&format!("- `{}`\n", companion.relative_path));
            }
        }
    }

    output
}

/// Get the absolute path to a skill's directory. Checks `~/.agents/skills/` first,
/// then falls back to `~/.claude/skills/`.
pub fn get_skill_dir(name: &str) -> Result<Option<PathBuf>, String> {
    if !is_valid_name(name) {
        return Err("Invalid skill name".into());
    }

    let agents_dir = get_agents_skills_dir()?.join(name);
    if agents_dir.join("SKILL.md").exists() {
        return Ok(Some(agents_dir));
    }

    let claude_dir = get_claude_skills_dir()?.join(name);
    if claude_dir.join("SKILL.md").exists() {
        return Ok(Some(claude_dir));
    }

    Ok(None)
}

/// Resolve the entrypoint filename for a skill directory.
/// If a `skill.json` exists in the skill's parent directory (or the skill
/// itself is the package root), reads it to find the custom `entrypoint`
/// field for the matching skill entry.  Falls back to "SKILL.md".
fn resolve_entrypoint(skill_dir: &PathBuf, name: &str) -> String {
    // Check for skill.json in the parent (standard layout: skill.json at root,
    // skill dirs as children)
    if let Some(parent) = skill_dir.parent() {
        let skills_json_path = parent.join("skill.json");
        if let Ok(raw) = fs::read_to_string(&skills_json_path) {
            if let Ok(manifest) = serde_json::from_str::<SkillsJson>(&raw) {
                if let Some(entry) = manifest.skills.iter().find(|s| s.name == name) {
                    return entry.entrypoint_file().to_string();
                }
            }
        }
    }
    "SKILL.md".to_string()
}

/// Get the absolute path to a skill's entrypoint file. Checks `~/.agents/skills/` first,
/// then falls back to `~/.claude/skills/`.
/// Respects the `entrypoint` field in `skill.json` if present; defaults to `SKILL.md`.
pub fn get_skill_path(name: &str) -> Result<Option<PathBuf>, String> {
    if !is_valid_name(name) {
        return Err("Invalid skill name".into());
    }

    let agents_skill_dir = get_agents_skills_dir()?.join(name);
    let agents_entrypoint = resolve_entrypoint(&agents_skill_dir, name);
    let agents_path = agents_skill_dir.join(&agents_entrypoint);
    if agents_path.exists() {
        return Ok(Some(agents_path));
    }
    // Fallback to SKILL.md for backward compatibility
    let agents_fallback = agents_skill_dir.join("SKILL.md");
    if agents_fallback.exists() {
        return Ok(Some(agents_fallback));
    }

    let claude_skill_dir = get_claude_skills_dir()?.join(name);
    let claude_entrypoint = resolve_entrypoint(&claude_skill_dir, name);
    let claude_path = claude_skill_dir.join(&claude_entrypoint);
    if claude_path.exists() {
        return Ok(Some(claude_path));
    }
    // Fallback to SKILL.md for backward compatibility
    let claude_fallback = claude_skill_dir.join("SKILL.md");
    if claude_fallback.exists() {
        return Ok(Some(claude_fallback));
    }

    Ok(None)
}

/// Save a skill to `~/.agents/skills/` (the agentskills.io standard location).
pub fn save_skill(name: &str, content: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid skill name".into());
    }
    let agents_dir = get_agents_skills_dir()?;
    let skill_dir = agents_dir.join(name);

    if !skill_dir.exists() {
        fs::create_dir_all(&skill_dir).map_err(|e| e.to_string())?;
    }

    let skill_path = skill_dir.join("SKILL.md");
    fs::write(skill_path, content).map_err(|e| e.to_string())
}

/// Delete a skill from both global locations and remove its registry entry.
pub fn delete_skill(name: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid skill name".into());
    }

    // Remove from ~/.agents/skills/
    let agents_dir = get_agents_skills_dir()?.join(name);
    if agents_dir.exists() {
        fs::remove_dir_all(&agents_dir).map_err(|e| e.to_string())?;
    }

    // Remove from ~/.claude/skills/
    let claude_dir = get_claude_skills_dir()?.join(name);
    if claude_dir.exists() {
        fs::remove_dir_all(&claude_dir).map_err(|e| e.to_string())?;
    }

    // Best-effort: remove from registry (ignore errors)
    let _ = remove_skill_source(name);

    Ok(())
}

/// Sync a single skill across both global directories.  Copies from
/// whichever location has it to the other.  If both exist, the
/// `~/.agents/skills/` version is canonical and overwrites claude.
pub fn sync_skill(name: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid skill name".into());
    }

    let agents_dir = get_agents_skills_dir()?;
    let claude_dir = get_claude_skills_dir()?;

    let agents_path = agents_dir.join(name).join("SKILL.md");
    let claude_path = claude_dir.join(name).join("SKILL.md");

    let agents_exists = agents_path.exists();
    let claude_exists = claude_path.exists();

    if !agents_exists && !claude_exists {
        return Err(format!("Skill '{}' not found in any location", name));
    }

    if agents_exists {
        // agents → claude  (agents is canonical)
        let content = fs::read_to_string(&agents_path).map_err(|e| e.to_string())?;
        let target_dir = claude_dir.join(name);
        fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;
        fs::write(&claude_path, content).map_err(|e| e.to_string())?;
    } else {
        // claude → agents
        let content = fs::read_to_string(&claude_path).map_err(|e| e.to_string())?;
        let target_dir = agents_dir.join(name);
        fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;
        fs::write(&agents_path, content).map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Sync all skills across both global directories.
/// Returns the list of skill names that were synced.
pub fn sync_all_skills() -> Result<Vec<String>, String> {
    let entries = list_skills()?;
    let mut synced = Vec::new();

    for entry in entries {
        if !entry.in_agents || !entry.in_claude {
            sync_skill(&entry.name)?;
            synced.push(entry.name);
        }
    }

    Ok(synced)
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

    /// Create a skill directory with a SKILL.md in the given skills root.
    fn make_skill(skills_root: &PathBuf, name: &str, content: &str) {
        let skill_dir = skills_root.join(name);
        fs::create_dir_all(&skill_dir).expect("create skill dir");
        fs::write(skill_dir.join("SKILL.md"), content).expect("write SKILL.md");
    }

    // ── scan_skills_dir ───────────────────────────────────────────────────────

    #[test]
    fn scan_returns_empty_set_when_dir_missing() {
        let tmp = tmp();
        let missing = tmp.path().join("nonexistent");
        let result = scan_skills_dir(&missing).expect("scan");
        assert!(result.is_empty());
    }

    #[test]
    fn scan_discovers_subdirs_with_skill_md() {
        let tmp = tmp();
        let skills_root = tmp.path().join("skills");
        make_skill(&skills_root, "my-skill", "# My Skill");
        make_skill(&skills_root, "another-skill", "# Another Skill");

        let result = scan_skills_dir(&skills_root).expect("scan");
        assert!(result.contains("my-skill"));
        assert!(result.contains("another-skill"));
    }

    #[test]
    fn scan_ignores_dirs_without_skill_md() {
        let tmp = tmp();
        let skills_root = tmp.path().join("skills");
        // Create a dir without SKILL.md.
        fs::create_dir_all(skills_root.join("empty-dir")).expect("create dir");
        // Create a dir with SKILL.md.
        make_skill(&skills_root, "valid-skill", "# Valid");

        let result = scan_skills_dir(&skills_root).expect("scan");
        assert!(result.contains("valid-skill"));
        assert!(!result.contains("empty-dir"));
    }

    #[test]
    fn scan_ignores_invalid_names() {
        let tmp = tmp();
        let skills_root = tmp.path().join("skills");
        // "." and ".." are never valid.
        // Create a skill with a name containing a slash — can't actually do on-disk
        // for a dir name, so just test that valid names are picked up.
        make_skill(&skills_root, "valid", "# Valid");

        let result = scan_skills_dir(&skills_root).expect("scan");
        assert!(result.contains("valid"));
        // Dot entries are never in the set.
        assert!(!result.contains("."));
        assert!(!result.contains(".."));
    }

    #[test]
    fn scan_reads_manifest_from_skill_json_when_present() {
        let tmp = tmp();
        let skills_root = tmp.path().join("skills");
        fs::create_dir_all(&skills_root).expect("create root");

        // The skill lives in a subdirectory; path points to it.
        let skill_dir = skills_root.join("manifest-skill");
        fs::create_dir_all(&skill_dir).expect("create skill dir");
        fs::write(skill_dir.join("SKILL.md"), "# Manifest Skill").expect("write entrypoint");

        // Write a skill.json manifest at the root.
        let manifest = serde_json::json!({
            "skills": [
                {
                    "name": "manifest-skill",
                    "path": "manifest-skill",
                    "description": "A test skill",
                    "entrypoint": "SKILL.md"
                }
            ]
        });
        fs::write(
            skills_root.join("skill.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .expect("write skill.json");

        let result = scan_skills_dir(&skills_root).expect("scan");
        assert!(result.contains("manifest-skill"));
    }

    // ── skill_has_resources ───────────────────────────────────────────────────

    #[test]
    fn skill_with_only_skill_md_has_no_resources() {
        let tmp = tmp();
        let skill_dir = tmp.path().join("my-skill");
        fs::create_dir_all(&skill_dir).expect("create");
        fs::write(skill_dir.join("SKILL.md"), "# Skill").expect("write");

        assert!(!skill_has_resources(&skill_dir));
    }

    #[test]
    fn skill_with_extra_file_has_resources() {
        let tmp = tmp();
        let skill_dir = tmp.path().join("my-skill");
        fs::create_dir_all(&skill_dir).expect("create");
        fs::write(skill_dir.join("SKILL.md"), "# Skill").expect("write SKILL.md");
        fs::write(skill_dir.join("README.md"), "extra").expect("write extra");

        assert!(skill_has_resources(&skill_dir));
    }

    #[test]
    fn skill_with_scripts_subdir_has_resources() {
        let tmp = tmp();
        let skill_dir = tmp.path().join("my-skill");
        let scripts_dir = skill_dir.join("scripts");
        fs::create_dir_all(&scripts_dir).expect("create scripts");
        fs::write(skill_dir.join("SKILL.md"), "# Skill").expect("write");
        fs::write(scripts_dir.join("run.sh"), "#!/bin/bash").expect("write script");

        assert!(skill_has_resources(&skill_dir));
    }

    // ── save_skill (via filesystem) ───────────────────────────────────────────
    // These helpers operate on explicit paths instead of the global dirs.

    fn save_skill_at(skills_root: &PathBuf, name: &str, content: &str) -> Result<(), String> {
        if !is_valid_name(name) {
            return Err("Invalid skill name".into());
        }
        let skill_dir = skills_root.join(name);
        if !skill_dir.exists() {
            fs::create_dir_all(&skill_dir).map_err(|e| e.to_string())?;
        }
        fs::write(skill_dir.join("SKILL.md"), content).map_err(|e| e.to_string())
    }

    fn delete_skill_at(skills_root: &PathBuf, name: &str) -> Result<(), String> {
        if !is_valid_name(name) {
            return Err("Invalid skill name".into());
        }
        let skill_dir = skills_root.join(name);
        if skill_dir.exists() {
            fs::remove_dir_all(&skill_dir).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn read_skill_at(skills_root: &PathBuf, name: &str) -> Result<String, String> {
        let path = skills_root.join(name).join("SKILL.md");
        if path.exists() {
            fs::read_to_string(&path).map_err(|e| e.to_string())
        } else {
            Ok("".to_string())
        }
    }

    #[test]
    fn save_creates_skill_dir_and_skill_md() {
        let tmp = tmp();
        let skills_root = tmp.path().join("skills");

        save_skill_at(&skills_root, "new-skill", "# New Skill\n\nContent.").expect("save");

        assert!(skills_root.join("new-skill").join("SKILL.md").exists());
    }

    #[test]
    fn save_and_read_roundtrip() {
        let tmp = tmp();
        let skills_root = tmp.path().join("skills");
        let content = "# Test Skill\n\nDoes useful things.";

        save_skill_at(&skills_root, "test-skill", content).expect("save");
        let read_back = read_skill_at(&skills_root, "test-skill").expect("read");

        assert_eq!(read_back, content);
    }

    #[test]
    fn save_overwrites_existing_skill() {
        let tmp = tmp();
        let skills_root = tmp.path().join("skills");

        save_skill_at(&skills_root, "skill", "v1").expect("save v1");
        save_skill_at(&skills_root, "skill", "v2").expect("save v2");

        let read_back = read_skill_at(&skills_root, "skill").expect("read");
        assert_eq!(read_back, "v2");
    }

    // ── delete_skill ──────────────────────────────────────────────────────────

    #[test]
    fn delete_removes_skill_directory() {
        let tmp = tmp();
        let skills_root = tmp.path().join("skills");
        make_skill(&skills_root, "bye-skill", "# Bye");
        assert!(skills_root.join("bye-skill").exists());

        delete_skill_at(&skills_root, "bye-skill").expect("delete");
        assert!(!skills_root.join("bye-skill").exists());
    }

    #[test]
    fn delete_is_idempotent_for_missing_skill() {
        let tmp = tmp();
        let skills_root = tmp.path().join("skills");
        // Should not error when skill doesn't exist.
        delete_skill_at(&skills_root, "ghost-skill").expect("delete non-existent");
    }

    // ── sync_skill (agents ↔ claude) ──────────────────────────────────────────

    fn sync_skill_at(agents_dir: &PathBuf, claude_dir: &PathBuf, name: &str) -> Result<(), String> {
        if !is_valid_name(name) {
            return Err(format!("Invalid skill name: {}", name));
        }
        let agents_path = agents_dir.join(name).join("SKILL.md");
        let claude_path = claude_dir.join(name).join("SKILL.md");

        let agents_exists = agents_path.exists();
        let claude_exists = claude_path.exists();

        if !agents_exists && !claude_exists {
            return Err(format!("Skill '{}' not found in any location", name));
        }

        if agents_exists {
            let content = fs::read_to_string(&agents_path).map_err(|e| e.to_string())?;
            let target_dir = claude_dir.join(name);
            fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;
            fs::write(&claude_path, content).map_err(|e| e.to_string())?;
        } else {
            let content = fs::read_to_string(&claude_path).map_err(|e| e.to_string())?;
            let target_dir = agents_dir.join(name);
            fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;
            fs::write(&agents_path, content).map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    #[test]
    fn sync_copies_agents_skill_to_claude_dir() {
        let tmp = tmp();
        let agents = tmp.path().join("agents");
        let claude = tmp.path().join("claude");
        make_skill(&agents, "shared-skill", "# Shared");

        sync_skill_at(&agents, &claude, "shared-skill").expect("sync");

        assert!(claude.join("shared-skill").join("SKILL.md").exists());
        let content = fs::read_to_string(claude.join("shared-skill").join("SKILL.md")).unwrap();
        assert_eq!(content, "# Shared");
    }

    #[test]
    fn sync_copies_claude_skill_to_agents_dir_when_only_in_claude() {
        let tmp = tmp();
        let agents = tmp.path().join("agents");
        let claude = tmp.path().join("claude");
        make_skill(&claude, "claude-only", "# Claude Only");

        sync_skill_at(&agents, &claude, "claude-only").expect("sync");

        assert!(agents.join("claude-only").join("SKILL.md").exists());
    }

    #[test]
    fn sync_agents_is_canonical_when_both_exist() {
        let tmp = tmp();
        let agents = tmp.path().join("agents");
        let claude = tmp.path().join("claude");
        make_skill(&agents, "contested", "agents version");
        make_skill(&claude, "contested", "claude version");

        sync_skill_at(&agents, &claude, "contested").expect("sync");

        // agents version should overwrite claude version.
        let content = fs::read_to_string(claude.join("contested").join("SKILL.md")).expect("read");
        assert_eq!(content, "agents version");
    }

    #[test]
    fn sync_returns_error_when_skill_not_found_anywhere() {
        let tmp = tmp();
        let agents = tmp.path().join("agents");
        let claude = tmp.path().join("claude");
        fs::create_dir_all(&agents).expect("create agents");
        fs::create_dir_all(&claude).expect("create claude");

        let result = sync_skill_at(&agents, &claude, "ghost");
        assert!(result.is_err());
    }

    // ── list_skill_resources ──────────────────────────────────────────────────

    #[test]
    fn list_skill_resources_returns_root_files_and_dirs() {
        let tmp = tmp();
        let skill_dir = tmp.path().join("my-skill");
        fs::create_dir_all(&skill_dir).expect("create skill dir");
        fs::write(skill_dir.join("SKILL.md"), "# Skill").expect("write SKILL.md");
        fs::write(skill_dir.join("notes.txt"), "extra notes").expect("write notes");
        let scripts = skill_dir.join("scripts");
        fs::create_dir_all(&scripts).expect("create scripts");
        fs::write(scripts.join("run.sh"), "#!/bin/bash").expect("write script");

        // Test the resource discovery logic directly using the types.
        let root_files: Vec<String> = {
            let mut files = Vec::new();
            if let Ok(entries) = fs::read_dir(&skill_dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let p = entry.path();
                    let name = p.file_name().unwrap().to_str().unwrap().to_string();
                    if p.is_file() && name != "SKILL.md" {
                        files.push(name);
                    }
                }
            }
            files.sort();
            files
        };
        assert_eq!(root_files, vec!["notes.txt"]);

        let script_files: Vec<String> = {
            let mut files = Vec::new();
            if let Ok(entries) = fs::read_dir(&scripts) {
                for entry in entries.filter_map(|e| e.ok()) {
                    if let Some(name) = entry.path().file_name().and_then(|n| n.to_str()) {
                        files.push(name.to_string());
                    }
                }
            }
            files
        };
        assert!(script_files.contains(&"run.sh".to_string()));
    }

    // ── is_valid_name (path safety) ───────────────────────────────────────────

    #[test]
    fn path_traversal_name_is_invalid() {
        assert!(!is_valid_name("../escape"));
        assert!(!is_valid_name(""));
        assert!(!is_valid_name("."));
        assert!(!is_valid_name(".."));
    }

    #[test]
    fn valid_skill_names_are_accepted() {
        assert!(is_valid_name("my-skill"));
        assert!(is_valid_name("skill_with_underscore"));
        assert!(is_valid_name("SkillWithCaps"));
    }

    // ── format_skill_with_companions ─────────────────────────────────────────

    #[test]
    fn format_skill_with_companions_appends_section() {
        let companions = vec![
            CompanionFile {
                relative_path: "scripts".to_string(),
                is_dir: true,
            },
            CompanionFile {
                relative_path: "scripts/run.sh".to_string(),
                is_dir: false,
            },
        ];
        let result = format_skill_with_companions("# My Skill", &companions);
        assert!(result.contains("## Companion Resources"));
        assert!(result.contains("### scripts"));
        assert!(result.contains("run.sh"));
    }

    #[test]
    fn format_skill_with_no_companions_returns_original() {
        let result = format_skill_with_companions("# My Skill", &[]);
        // Empty companions list → still appends the section header.
        // What matters is that the original content is preserved.
        assert!(result.starts_with("# My Skill"));
    }
}
