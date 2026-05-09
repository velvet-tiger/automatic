use std::fs;
use std::path::{Path, PathBuf};

#[cfg(test)]
use std::cell::RefCell;

// ── Path Helpers ─────────────────────────────────────────────────────────────

fn home_dir() -> Result<PathBuf, String> {
    #[cfg(test)]
    if let Some(path) = TEST_HOME_OVERRIDE.with(|override_path| override_path.borrow().clone()) {
        return Ok(path);
    }

    dirs::home_dir().ok_or("Could not find home directory".to_string())
}

/// Returns the root Automatic data directory.
///
/// - **Debug builds** (`cargo tauri dev`, `cargo test`, etc.): `~/.automatic-dev`
/// - **Release builds**: `~/.automatic`
///
/// All other path helpers call this function so that dev and production data
/// are always kept separate.
pub fn get_automatic_dir() -> Result<PathBuf, String> {
    let home = home_dir()?;
    #[cfg(debug_assertions)]
    let dir = home.join(".automatic-dev");
    #[cfg(not(debug_assertions))]
    let dir = home.join(".automatic");
    Ok(dir)
}

/// Root of Automatic's managed library — the single namespace under which
/// all reusable assets (skills, rules, instructions, templates, sub-agents,
/// commands, MCP servers, tools) live. Per-project sync copies items from
/// here into each project's agent-specific directory on demand; nothing in
/// the library is auto-loaded globally.
pub fn get_library_dir() -> Result<PathBuf, String> {
    Ok(get_automatic_dir()?.join("library"))
}

/// Primary skills directory — Automatic's managed library.
///
/// This is the only location Automatic writes to. Per-project sync copies
/// skills from here into each project's agent-specific skill directory on
/// demand; nothing is auto-loaded globally.
pub fn get_library_skills_dir() -> Result<PathBuf, String> {
    Ok(get_library_dir()?.join("skills"))
}

/// External skill directory — the agentskills.io standard location.
///
/// Read-only for Automatic. Scanned so that skills installed here by other
/// tools (e.g. `npx skills add`) are visible in the UI, but Automatic does
/// not write to this path. Some agents (notably OpenCode) auto-load from
/// this directory globally — anything here applies to every project that
/// uses such agents, which is why Automatic's managed library lives
/// elsewhere.
pub fn get_agents_skills_dir() -> Result<PathBuf, String> {
    let home = home_dir()?;
    Ok(home.join(".agents/skills"))
}

/// External skill directory — Claude Code's location. Read-only for
/// Automatic; same rationale as [`get_agents_skills_dir`].
pub fn get_claude_skills_dir() -> Result<PathBuf, String> {
    let home = home_dir()?;
    Ok(home.join(".claude/skills"))
}

pub fn get_projects_dir() -> Result<PathBuf, String> {
    Ok(get_automatic_dir()?.join("projects"))
}

pub fn get_commands_dir() -> Result<PathBuf, String> {
    Ok(get_library_dir()?.join("commands"))
}

pub fn get_groups_dir() -> Result<PathBuf, String> {
    Ok(get_automatic_dir()?.join("groups"))
}

// ── Library Layout Migration ────────────────────────────────────────────────

/// Mapping from legacy top-level directory names to the new library layout.
///
/// Some entries also rename the directory (e.g. on-disk `templates/` is the
/// old home of the UI's "Instructions" page; on-disk `project_templates/` is
/// the old home of the UI's "Templates" page).
const LEGACY_LAYOUT: &[(&str, &str)] = &[
    ("rules", "rules"),
    ("templates", "instructions"),
    ("project_templates", "templates"),
    ("agents", "subagents"),
    ("commands", "commands"),
    ("mcp_servers", "mcp_servers"),
    ("tools", "tools"),
];

/// One-off, idempotent migration that moves legacy top-level library
/// directories under `~/.automatic/library/` and renames the three that
/// historically used names that didn't match the user-facing UI labels
/// (templates → instructions, project_templates → templates, agents →
/// subagents).
///
/// Safe to re-run: once a legacy directory no longer exists at the top
/// level, that pair is skipped. If both a legacy directory and its target
/// exist, entries the target does not yet have are merged in and the legacy
/// directory is removed. Returns the list of `(legacy, new)` pairs that
/// were migrated.
pub fn migrate_top_level_to_library() -> Result<Vec<(&'static str, &'static str)>, String> {
    let automatic = get_automatic_dir()?;
    let library = get_library_dir()?;
    let mut migrated = Vec::new();

    for (legacy_name, new_name) in LEGACY_LAYOUT {
        let legacy = automatic.join(legacy_name);
        let target = library.join(new_name);

        if !legacy.exists() {
            continue;
        }

        fs::create_dir_all(&library)
            .map_err(|e| format!("Failed to create library dir {}: {}", library.display(), e))?;

        if target.exists() {
            // Target already owns this kind. Merge any legacy entries the
            // target does not yet have (so a partial earlier run cannot
            // lose data), then drop the legacy dir.
            merge_dir_into(&legacy, &target)?;
            fs::remove_dir_all(&legacy).map_err(|e| {
                format!(
                    "Failed to remove legacy dir {} after merge: {}",
                    legacy.display(),
                    e
                )
            })?;
        } else if fs::rename(&legacy, &target).is_err() {
            // Cross-filesystem fallback.
            copy_dir_recursive(&legacy, &target)?;
            fs::remove_dir_all(&legacy).map_err(|e| {
                format!(
                    "Failed to remove legacy dir {} after copy: {}",
                    legacy.display(),
                    e
                )
            })?;
        }

        migrated.push((*legacy_name, *new_name));
    }

    Ok(migrated)
}

/// Recursive directory copy. Used by migrations that need a cross-filesystem
/// fallback when `fs::rename` cannot move a directory between mount points.
pub(crate) fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<(), String> {
    if !dest.exists() {
        fs::create_dir_all(dest)
            .map_err(|e| format!("Failed to create {}: {}", dest.display(), e))?;
    }
    for entry in
        fs::read_dir(src).map_err(|e| format!("Failed to read {}: {}", src.display(), e))?
    {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let path = entry.path();
        let dest_path = dest.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &dest_path)?;
        } else {
            fs::copy(&path, &dest_path)
                .map_err(|e| format!("Failed to copy {}: {}", path.display(), e))?;
        }
    }
    Ok(())
}

/// Merge entries from `src` into `dest`, only copying entries `dest` does
/// not already contain. Used by the migration so that running it after a
/// partial earlier attempt cannot drop data that has already been copied.
fn merge_dir_into(src: &Path, dest: &Path) -> Result<(), String> {
    if !dest.exists() {
        fs::create_dir_all(dest)
            .map_err(|e| format!("Failed to create {}: {}", dest.display(), e))?;
    }
    for entry in
        fs::read_dir(src).map_err(|e| format!("Failed to read {}: {}", src.display(), e))?
    {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        if dest_path.exists() {
            // Target wins: the existing entry stays untouched.
            continue;
        }
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            fs::copy(&src_path, &dest_path).map_err(|e| {
                format!("Failed to copy {}: {}", src_path.display(), e)
            })?;
        }
    }
    Ok(())
}

pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && !name.contains('\\') && name != "." && name != ".."
}

#[cfg(test)]
thread_local! {
    static TEST_HOME_OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn with_test_home<T>(path: PathBuf, test: impl FnOnce() -> T) -> T {
    struct RestoreHome(Option<PathBuf>);

    impl Drop for RestoreHome {
        fn drop(&mut self) {
            TEST_HOME_OVERRIDE.with(|override_path| {
                *override_path.borrow_mut() = self.0.take();
            });
        }
    }

    let previous = TEST_HOME_OVERRIDE.with(|override_path| override_path.replace(Some(path)));
    let _restore = RestoreHome(previous);
    test()
}

#[cfg(test)]
mod migration_tests {
    use super::*;
    use tempfile::TempDir;

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn read_file(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    #[test]
    fn migrate_empty_home_does_nothing() {
        let tmp = TempDir::new().unwrap();
        with_test_home(tmp.path().to_path_buf(), || {
            let migrated = migrate_top_level_to_library().unwrap();
            assert!(migrated.is_empty());
        });
    }

    #[test]
    fn migrate_moves_legacy_dir_with_rename() {
        let tmp = TempDir::new().unwrap();
        with_test_home(tmp.path().to_path_buf(), || {
            let automatic = get_automatic_dir().unwrap();
            // Legacy `templates/` (UI Instructions) and `project_templates/`
            // (UI Templates) — distinct content, must each land in the
            // correct new slot.
            write_file(&automatic.join("templates/Brief.md"), "hello brief");
            write_file(&automatic.join("project_templates/foo.json"), "{}");
            write_file(&automatic.join("agents/qa.md"), "qa agent");
            write_file(&automatic.join("rules/style.md"), "rule");

            let migrated = migrate_top_level_to_library().unwrap();
            let names: Vec<_> = migrated.iter().map(|(l, _)| *l).collect();
            assert!(names.contains(&"templates"));
            assert!(names.contains(&"project_templates"));
            assert!(names.contains(&"agents"));
            assert!(names.contains(&"rules"));

            let library = get_library_dir().unwrap();
            assert_eq!(read_file(&library.join("instructions/Brief.md")), "hello brief");
            assert_eq!(read_file(&library.join("templates/foo.json")), "{}");
            assert_eq!(read_file(&library.join("subagents/qa.md")), "qa agent");
            assert_eq!(read_file(&library.join("rules/style.md")), "rule");

            // Legacy directories must be gone.
            assert!(!automatic.join("templates").exists());
            assert!(!automatic.join("project_templates").exists());
            assert!(!automatic.join("agents").exists());
            assert!(!automatic.join("rules").exists());
        });
    }

    #[test]
    fn migrate_does_not_swap_templates_and_instructions() {
        let tmp = TempDir::new().unwrap();
        with_test_home(tmp.path().to_path_buf(), || {
            let automatic = get_automatic_dir().unwrap();
            // The two distinct shapes — markdown vs JSON — must end up
            // in the correct new home and never trade places.
            write_file(&automatic.join("templates/AgentBrief.md"), "INSTRUCTIONS_BODY");
            write_file(&automatic.join("project_templates/starter.json"), "TEMPLATES_BODY");

            migrate_top_level_to_library().unwrap();

            let library = get_library_dir().unwrap();
            assert_eq!(
                read_file(&library.join("instructions/AgentBrief.md")),
                "INSTRUCTIONS_BODY"
            );
            assert_eq!(
                read_file(&library.join("templates/starter.json")),
                "TEMPLATES_BODY"
            );
        });
    }

    #[test]
    fn migrate_merges_disjoint_existing_target() {
        let tmp = TempDir::new().unwrap();
        with_test_home(tmp.path().to_path_buf(), || {
            let automatic = get_automatic_dir().unwrap();
            // Library already has rule_b.md; legacy has rule_a.md. Both
            // should survive, and the legacy dir must be removed.
            write_file(
                &automatic.join("library/rules/rule_b.md"),
                "in library",
            );
            write_file(&automatic.join("rules/rule_a.md"), "in legacy");

            migrate_top_level_to_library().unwrap();

            let library_rules = automatic.join("library/rules");
            assert_eq!(read_file(&library_rules.join("rule_a.md")), "in legacy");
            assert_eq!(read_file(&library_rules.join("rule_b.md")), "in library");
            assert!(!automatic.join("rules").exists());
        });
    }

    #[test]
    fn migrate_target_wins_on_collision() {
        let tmp = TempDir::new().unwrap();
        with_test_home(tmp.path().to_path_buf(), || {
            let automatic = get_automatic_dir().unwrap();
            // Same filename in legacy and target — target's content
            // is kept, legacy is dropped.
            write_file(&automatic.join("library/rules/style.md"), "library wins");
            write_file(&automatic.join("rules/style.md"), "legacy loses");

            migrate_top_level_to_library().unwrap();

            assert_eq!(
                read_file(&automatic.join("library/rules/style.md")),
                "library wins"
            );
            assert!(!automatic.join("rules").exists());
        });
    }

    #[test]
    fn migrate_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        with_test_home(tmp.path().to_path_buf(), || {
            let automatic = get_automatic_dir().unwrap();
            write_file(&automatic.join("rules/rule.md"), "x");
            write_file(&automatic.join("agents/agent.md"), "y");

            let first = migrate_top_level_to_library().unwrap();
            assert!(!first.is_empty());

            let second = migrate_top_level_to_library().unwrap();
            assert!(second.is_empty());

            assert_eq!(read_file(&automatic.join("library/rules/rule.md")), "x");
            assert_eq!(
                read_file(&automatic.join("library/subagents/agent.md")),
                "y"
            );
        });
    }
}
