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
    let rules: Vec<String> =
        if let Some(r) = project.file_rules.get("_project").filter(|v| !v.is_empty()) {
            r.clone()
        } else {
            let rule_key = if is_unified { "_unified" } else { filename };
            project
                .file_rules
                .get(rule_key)
                .cloned()
                .unwrap_or_default()
        };

    let target_files = if is_unified {
        collect_agent_filenames(project)
    } else {
        vec![filename.to_string()]
    };

    let mut dot_claude_synced = false;

    for f in &target_files {
        if project_uses_dot_claude_rules(project, f) {
            // Save without inline rules — rules go to .claude/rules/
            save_project_file_with_rules(&project.directory, f, user_content, &[])?;
            if !dot_claude_synced && !rules.is_empty() {
                sync_rules_to_dot_claude_rules(&project.directory, &rules)?;
                dot_claude_synced = true;
            }
        } else {
            save_project_file_with_rules(&project.directory, f, user_content, &rules)?;
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
