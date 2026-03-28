use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use crate::agent;

use super::*;

// ── Project Files ────────────────────────────────────────────────────────────

/// Read a project file from the project's directory, stripping any
/// Automatic-managed sections (skills markers) and rules sections.  Returns
/// the user-authored content only.
pub fn read_project_file(directory: &str, filename: &str) -> Result<String, String> {
    if directory.is_empty() {
        return Err("Project has no directory configured".into());
    }

    let path = PathBuf::from(directory).join(filename);
    if !path.exists() {
        return Ok(String::new());
    }

    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    Ok(strip_groups_section(&strip_rules_section(
        &strip_managed_section(&content),
    )))
}

/// Write a project file to the project's directory.  Writes exactly what the
/// user provides — the sync process will re-merge any managed sections (skills)
/// on the next sync run.
pub fn save_project_file(directory: &str, filename: &str, content: &str) -> Result<(), String> {
    if directory.is_empty() {
        return Err("Project has no directory configured".into());
    }

    let dir = PathBuf::from(directory);
    if !dir.exists() {
        return Err(format!("Directory '{}' does not exist", directory));
    }

    let path = dir.join(filename);
    fs::write(&path, content).map_err(|e| e.to_string())
}

// ── Rule-aware project file writes ──────────────────────────────────────────

/// Save user content to the appropriate project instruction file(s), applying
/// rules according to the project's configuration.
///
/// This is the **single code path** for all rule-aware file saves.  It handles:
/// - Unified vs per-file instruction mode
/// - Routing Claude Code rules to `.claude/rules/` when that option is enabled
/// - Inline rule injection for all other agents
///
/// `filename` is either an actual filename (e.g. `"CLAUDE.md"`) or `"_unified"`
/// for unified-mode saves.
pub fn save_project_file_for_project(
    project: &Project,
    filename: &str,
    user_content: &str,
) -> Result<(), String> {
    let is_unified = filename == "_unified" || project.instruction_mode == "unified";

    // Resolve rules: project-level key ("_project") takes precedence over the
    // legacy per-file / unified keys so that saves are consistent with sync.
    // Mandatory rules (e.g. automatic-service) are always included.
    let rules: Vec<String> = ensure_mandatory_rules(&if let Some(r) =
        project.file_rules.get("_project").filter(|v| !v.is_empty())
    {
        r.clone()
    } else {
        let rule_key = if is_unified { "_unified" } else { filename };
        project
            .file_rules
            .get(rule_key)
            .cloned()
            .unwrap_or_default()
    });

    // Collect inline custom rule content strings from the project.
    // These are project-scoped rules that don't live in the global registry.
    let custom_contents: Vec<String> = project
        .custom_rules
        .iter()
        .filter(|r| !r.content.trim().is_empty())
        .map(|r| r.content.clone())
        .collect();

    let target_files = if is_unified {
        collect_agent_filenames(project)
    } else {
        vec![filename.to_string()]
    };

    let mut dot_claude_synced = false;

    for f in &target_files {
        if project_uses_dot_claude_rules(project, f) {
            // Save with custom rules inline — global rules go to .claude/rules/.
            // Custom rules are always injected inline because they don't have a
            // machine name to use as a filename in .claude/rules/.
            save_project_file_with_rules_and_custom(
                &project.directory,
                f,
                user_content,
                &[],
                &custom_contents,
            )?;
            if !dot_claude_synced && !rules.is_empty() {
                sync_rules_to_dot_claude_rules(&project.directory, &rules)?;
                dot_claude_synced = true;
            }
        } else {
            save_project_file_with_rules_and_custom(
                &project.directory,
                f,
                user_content,
                &rules,
                &custom_contents,
            )?;
        }

        // Persist a snapshot of the user content so drift detection can diff
        // against what Automatic last wrote, not just detect that a change occurred.
        let _ = save_instruction_snapshot(&project.directory, f, user_content);
    }

    Ok(())
}

// ── Instruction file snapshots ───────────────────────────────────────────────

/// Directory (relative to project root) where Automatic stores its snapshots.
const SNAPSHOT_DIR: &str = ".automatic/snapshots";

/// Persist `user_content` for `filename` into `<project>/.automatic/snapshots/<filename>`.
///
/// This is called every time Automatic writes an instruction file so that
/// drift detection can diff the on-disk content against what Automatic last wrote.
pub fn save_instruction_snapshot(
    directory: &str,
    filename: &str,
    user_content: &str,
) -> Result<(), String> {
    if directory.is_empty() {
        return Ok(());
    }
    let snap_dir = PathBuf::from(directory).join(SNAPSHOT_DIR);
    fs::create_dir_all(&snap_dir).map_err(|e| e.to_string())?;
    let path = snap_dir.join(filename);
    fs::write(&path, user_content).map_err(|e| e.to_string())
}

/// Read the snapshot for `filename` from `<project>/.automatic/snapshots/<filename>`.
/// Returns `None` if no snapshot exists (Automatic has never written this file).
pub fn read_instruction_snapshot(directory: &str, filename: &str) -> Option<String> {
    if directory.is_empty() {
        return None;
    }
    let path = PathBuf::from(directory).join(SNAPSHOT_DIR).join(filename);
    fs::read_to_string(&path).ok()
}

/// Returns `true` if rules for the given project file should be written to
/// `.claude/rules/` rather than injected inline into the project file.
///
/// `key` can be an actual filename (e.g. `"CLAUDE.md"`), `"_unified"`, or
/// `"_project"`.  The latter two map to all agents — if one of them is
/// Claude Code with `claude_rules_in_dot_claude` enabled, this returns `true`.
pub fn project_uses_dot_claude_rules(project: &Project, key: &str) -> bool {
    if !project.agents.iter().any(|a| a == "claude") {
        return false;
    }
    let claude_file = agent::from_id("claude")
        .map(|a| a.project_file_name())
        .unwrap_or("CLAUDE.md");
    // "_unified" and "_project" map to all agents, so if Claude is present
    // and the option is enabled, rules go to .claude/rules/.
    if key != claude_file && key != "_unified" && key != "_project" {
        return false;
    }
    project
        .agent_options
        .get("claude")
        .cloned()
        .unwrap_or_default()
        .claude_rules_in_dot_claude
}

/// Collect the unique project filenames for all agents in a project.
fn collect_agent_filenames(project: &Project) -> Vec<String> {
    let mut filenames = Vec::new();
    for agent_id in &project.agents {
        if let Some(a) = agent::from_id(agent_id) {
            let f = a.project_file_name().to_string();
            if !filenames.contains(&f) {
                filenames.push(f);
            }
        }
    }
    filenames
}

/// Public wrapper for `strip_managed_section` (used by sync).
pub fn strip_managed_section_pub(content: &str) -> String {
    strip_managed_section(content)
}

/// Strip the `<!-- automatic:skills:start -->...<!-- automatic:skills:end -->` section.
pub(crate) fn strip_managed_section(content: &str) -> String {
    let start_marker = "<!-- automatic:skills:start -->";
    let end_marker = "<!-- automatic:skills:end -->";

    if let (Some(start), Some(end)) = (content.find(start_marker), content.find(end_marker)) {
        let before = &content[..start];
        let after = &content[end + end_marker.len()..];
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

// ── Instruction file hash tracking ──────────────────────────────────────────

/// Compute a deterministic hash of file content.  Used to detect external
/// modifications to instruction files.
pub fn compute_content_hash(content: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Read all instruction files for a project's agents from disk and return a
/// map of `filename → hash(full_content)`.  Only files that exist on disk are
/// included.
pub fn compute_instruction_hashes(project: &Project) -> HashMap<String, String> {
    let mut hashes = HashMap::new();
    if project.directory.is_empty() {
        return hashes;
    }

    let dir = PathBuf::from(&project.directory);
    if !dir.exists() {
        return hashes;
    }

    let mut seen = std::collections::HashSet::new();
    for agent_id in &project.agents {
        if let Some(a) = agent::from_id(agent_id) {
            if !a.capabilities().instructions {
                continue;
            }
            let filename = a.project_file_name().to_string();
            if seen.contains(&filename) {
                continue;
            }
            seen.insert(filename.clone());

            let path = dir.join(&filename);
            if let Ok(content) = fs::read_to_string(&path) {
                hashes.insert(filename, compute_content_hash(&content));
            }
        }
    }

    hashes
}

/// Record the current on-disk hashes for all instruction files into the
/// project's config and persist to the registry.  Call after any operation
/// that writes instruction files.
pub fn record_instruction_hashes(project_name: &str, project: &mut Project) {
    project.instruction_file_hashes = compute_instruction_hashes(project);

    // Persist updated hashes to the registry.
    if let Ok(data) = serde_json::to_string_pretty(project) {
        let _ = save_project(project_name, &data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn tmp() -> TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    /// Build a minimal project with the given agents and directory.
    fn make_project(dir: &str, agents: &[&str]) -> Project {
        Project {
            name: "test-project".to_string(),
            directory: dir.to_string(),
            agents: agents.iter().map(|s| s.to_string()).collect(),
            instruction_mode: "per-agent".to_string(),
            ..Default::default()
        }
    }

    fn make_unified_project(dir: &str, agents: &[&str]) -> Project {
        Project {
            name: "test-project".to_string(),
            directory: dir.to_string(),
            agents: agents.iter().map(|s| s.to_string()).collect(),
            instruction_mode: "unified".to_string(),
            ..Default::default()
        }
    }

    // ── Bug: Instructions not saved to AGENTS.md ────────────────────────────
    //
    // When a user types instructions in the UI and saves, the content should
    // appear in AGENTS.md (for OpenCode/Codex agents).

    #[test]
    fn save_writes_user_content_to_agents_md() {
        let dir = tmp();
        let project = make_project(dir.path().to_str().unwrap(), &["opencode"]);

        save_project_file_for_project(&project, "AGENTS.md", "# My Instructions\n\nDo the thing.")
            .expect("save");

        let on_disk = fs::read_to_string(dir.path().join("AGENTS.md")).expect("read");
        assert!(
            on_disk.contains("# My Instructions"),
            "User content should be written to AGENTS.md, but file contains: {:?}",
            on_disk
        );
        assert!(
            on_disk.contains("Do the thing."),
            "User content should be written to AGENTS.md"
        );
    }

    #[test]
    fn save_writes_user_content_to_claude_md() {
        let dir = tmp();
        let project = make_project(dir.path().to_str().unwrap(), &["claude"]);

        save_project_file_for_project(
            &project,
            "CLAUDE.md",
            "# Claude Instructions\n\nBe helpful.",
        )
        .expect("save");

        let on_disk = fs::read_to_string(dir.path().join("CLAUDE.md")).expect("read");
        assert!(
            on_disk.contains("# Claude Instructions"),
            "User content should be written to CLAUDE.md, but file contains: {:?}",
            on_disk
        );
        assert!(
            on_disk.contains("Be helpful."),
            "User content should be written to CLAUDE.md"
        );
    }

    // ── Bug: Unified mode should write to ALL agent files ───────────────────

    #[test]
    fn unified_save_writes_to_both_agents_md_and_claude_md() {
        let dir = tmp();
        let project = make_unified_project(dir.path().to_str().unwrap(), &["claude", "opencode"]);

        save_project_file_for_project(
            &project,
            "_unified",
            "# Shared Instructions\n\nApply everywhere.",
        )
        .expect("save");

        let claude_content = fs::read_to_string(dir.path().join("CLAUDE.md")).expect("read CLAUDE");
        let agents_content = fs::read_to_string(dir.path().join("AGENTS.md")).expect("read AGENTS");

        assert!(
            claude_content.contains("# Shared Instructions"),
            "Unified save should write to CLAUDE.md, but file contains: {:?}",
            claude_content
        );
        assert!(
            agents_content.contains("# Shared Instructions"),
            "Unified save should write to AGENTS.md, but file contains: {:?}",
            agents_content
        );
    }

    // ── Bug: Rules should NOT be inline in CLAUDE.md when dot-claude is on ──

    #[test]
    fn save_inlines_custom_rules_in_claude_md_even_with_dot_claude_enabled() {
        let dir = tmp();
        let mut project = make_project(dir.path().to_str().unwrap(), &["claude"]);

        // Enable dot-claude rules (default).
        // Global rules go to .claude/rules/ but custom rules are always inline
        // because they don't have a machine name for a separate file.
        project.agent_options.insert(
            "claude".to_string(),
            AgentOptions {
                claude_rules_in_dot_claude: true,
            },
        );

        // Add custom rules to the project
        project.custom_rules = vec![CustomRule {
            name: "test-rule".to_string(),
            content: "Always test your code.".to_string(),
        }];

        save_project_file_for_project(&project, "CLAUDE.md", "# Instructions").expect("save");

        let on_disk = fs::read_to_string(dir.path().join("CLAUDE.md")).expect("read");

        // User instructions should be present
        assert!(
            on_disk.contains("# Instructions"),
            "User content should be in CLAUDE.md"
        );

        // Custom rules SHOULD be inline even in dot-claude mode (they have no
        // machine name to use as a filename in .claude/rules/).
        assert!(
            on_disk.contains("<!-- automatic:rules:start -->"),
            "Custom rules should be inline in CLAUDE.md even with dot-claude enabled, but found: {:?}",
            on_disk
        );
        assert!(
            on_disk.contains("Always test your code."),
            "Custom rule content should be inline in CLAUDE.md, but found: {:?}",
            on_disk
        );
    }

    // ── Verify custom rules are included in non-claude files ────────────────

    #[test]
    fn save_includes_custom_rules_in_agents_md() {
        let dir = tmp();
        let mut project = make_project(dir.path().to_str().unwrap(), &["opencode"]);

        project.custom_rules = vec![CustomRule {
            name: "test-rule".to_string(),
            content: "Always test your code.".to_string(),
        }];

        save_project_file_for_project(&project, "AGENTS.md", "# Instructions").expect("save");

        let on_disk = fs::read_to_string(dir.path().join("AGENTS.md")).expect("read");

        // User instructions should be present
        assert!(
            on_disk.contains("# Instructions"),
            "User content should be in AGENTS.md"
        );

        // Custom rules should be inline for non-claude agents
        assert!(
            on_disk.contains("Always test your code."),
            "Custom rule content should be inline in AGENTS.md for non-claude agents, but found: {:?}",
            on_disk
        );
    }

    // ── Unified mode: custom rules should be in AGENTS.md but not CLAUDE.md ─

    #[test]
    fn unified_save_puts_custom_rules_in_both_agents_and_claude() {
        let dir = tmp();
        let mut project =
            make_unified_project(dir.path().to_str().unwrap(), &["claude", "opencode"]);

        project.agent_options.insert(
            "claude".to_string(),
            AgentOptions {
                claude_rules_in_dot_claude: true,
            },
        );

        project.custom_rules = vec![CustomRule {
            name: "test-rule".to_string(),
            content: "Always test your code.".to_string(),
        }];

        save_project_file_for_project(&project, "_unified", "# Shared Instructions").expect("save");

        let claude_content = fs::read_to_string(dir.path().join("CLAUDE.md")).expect("read CLAUDE");
        let agents_content = fs::read_to_string(dir.path().join("AGENTS.md")).expect("read AGENTS");

        // User content in both files
        assert!(
            claude_content.contains("# Shared Instructions"),
            "User content should be in CLAUDE.md"
        );
        assert!(
            agents_content.contains("# Shared Instructions"),
            "User content should be in AGENTS.md"
        );

        // Custom rules SHOULD be inline in CLAUDE.md (custom rules have no
        // machine name for .claude/rules/, so they go inline even in dot-claude
        // mode — matching the engine sync behaviour).
        assert!(
            claude_content.contains("Always test your code."),
            "Custom rules should be inline in CLAUDE.md, but found: {:?}",
            claude_content
        );

        // Custom rules SHOULD be inline in AGENTS.md
        assert!(
            agents_content.contains("Always test your code."),
            "Custom rules should be inline in AGENTS.md, but found: {:?}",
            agents_content
        );
    }

    // ── Read roundtrip: saved instructions can be read back ─────────────────

    #[test]
    fn read_project_file_returns_user_content_without_rules() {
        let dir = tmp();
        let dir_str = dir.path().to_str().unwrap();

        // Write a file with both user content and a rules section
        let content = "# User Content\n\nMy instructions.\n\n<!-- automatic:rules:start -->\nSome rule.\n<!-- automatic:rules:end -->\n";
        fs::write(dir.path().join("AGENTS.md"), content).expect("write");

        let user_content = read_project_file(dir_str, "AGENTS.md").expect("read");
        assert!(user_content.contains("# User Content"));
        assert!(user_content.contains("My instructions."));
        assert!(!user_content.contains("<!-- automatic:rules:start -->"));
        assert!(!user_content.contains("Some rule."));
    }

    // ── save + read roundtrip ───────────────────────────────────────────────

    #[test]
    fn save_then_read_roundtrips_user_content() {
        let dir = tmp();
        let dir_str = dir.path().to_str().unwrap();
        let project = make_project(dir_str, &["opencode"]);

        let original_content = "# My Instructions\n\nDo the thing.";
        save_project_file_for_project(&project, "AGENTS.md", original_content).expect("save");

        let read_back = read_project_file(dir_str, "AGENTS.md").expect("read");
        assert_eq!(
            read_back.trim(),
            original_content.trim(),
            "Read should return exactly the user content that was saved"
        );
    }

    // ── Mandatory rule enforcement ──────────────────────────────────────────

    #[test]
    fn save_always_includes_mandatory_rule_even_with_no_file_rules() {
        let dir = tmp();
        let project = make_project(dir.path().to_str().unwrap(), &["opencode"]);
        // project.file_rules is empty — no rules configured by the user.

        save_project_file_for_project(&project, "AGENTS.md", "# Instructions").expect("save");

        let on_disk = fs::read_to_string(dir.path().join("AGENTS.md")).expect("read");
        assert!(
            on_disk.contains("<!-- automatic:rules:start -->"),
            "Mandatory rule should be injected even with no configured rules, but found: {:?}",
            on_disk
        );
        // The automatic-service rule content should be present (it is resolved
        // from the global registry via read_rule_content, which may not be
        // available in tests — but the rules section markers should be).
    }

    #[test]
    fn save_does_not_duplicate_mandatory_rule_when_already_configured() {
        let dir = tmp();
        let mut project = make_project(dir.path().to_str().unwrap(), &["opencode"]);
        // User has already added automatic-service to their rules.
        project.file_rules.insert(
            "_project".to_string(),
            vec!["automatic-service".to_string()],
        );

        save_project_file_for_project(&project, "AGENTS.md", "# Instructions").expect("save");

        let on_disk = fs::read_to_string(dir.path().join("AGENTS.md")).expect("read");
        // The rules section should appear exactly once.
        let marker_count = on_disk.matches("<!-- automatic:rules:start -->").count();
        assert_eq!(
            marker_count, 1,
            "Rules section should appear exactly once, found {} in: {:?}",
            marker_count, on_disk
        );
    }
}
