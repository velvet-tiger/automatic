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
