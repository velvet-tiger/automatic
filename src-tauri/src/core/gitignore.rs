//! Managed `.gitignore` block for Automatic-written files.
//!
//! When a project opts in (`Project.manage_gitignore`), the sync engine keeps a
//! bounded block in the project's `.gitignore` listing every path Automatic
//! writes, so generated agent configuration is not committed.  The block is
//! delimited by explicit markers and regenerated on every sync, so it is safe
//! to re-run.  Everything outside the markers is preserved untouched.

use std::fs;
use std::path::{Path, PathBuf};

use crate::agent::ManagedPath;

/// First line of the managed block.  Presence of this exact prefix identifies
/// an existing block to replace or remove.
const BEGIN_MARKER: &str = "# BEGIN Automatic-managed";
/// Last line of the managed block.
const END_MARKER: &str = "# END Automatic-managed";
/// Explanatory line written immediately after the begin marker.
const HEADER_NOTE: &str =
    "# Files generated and managed by Automatic. Regenerated on sync; do not edit inside this block.";

/// Top-level directories written by the sync engine itself (not by any single
/// agent).  Ignored wholesale; any agent path nested beneath one is redundant.
/// Names only, no trailing slash.
const UNIVERSAL_DIRS: &[&str] = &[".agents", ".automatic"];

/// Directories that belong to a general-purpose tool — a VCS host or an editor —
/// rather than to any AI agent.  Automatic writes only specific files inside
/// these, so they are ignored file-by-file, never wholesale.  Ignoring the
/// whole directory would drop the user's CI workflows (`.github/`) or editor
/// settings (`.vscode/`, `.zed/`), which Automatic never touches.
const SHARED_TOOL_DIRS: &[&str] = &[".github", ".vscode", ".zed"];

/// Split a project-relative path into its normal components as strings.
fn rel_components(rel: &Path) -> Vec<String> {
    rel.components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str().map(|x| x.to_string()),
            _ => None,
        })
        .collect()
}

/// Build the ordered, deduplicated list of `.gitignore` patterns for a project.
///
/// Patterns are anchored to the project root with a leading `/` so they match
/// only Automatic's files, never a same-named file in a subdirectory the user
/// tracks deliberately.  Directories carry a trailing `/`.
///
/// Each managed path is reduced to the coarsest safe ignore:
/// - A path inside a [`SHARED_TOOL_DIRS`] directory is kept as its exact
///   relative path (surgical), so the rest of that shared directory stays
///   tracked.
/// - A path nested inside an agent's own top-level directory collapses to that
///   directory (e.g. `.codex/agents` → `.codex/`), so the whole agent directory
///   is ignored rather than a few hand-picked subpaths.
/// - A root-level file or directory is kept as-is.
///
/// In `silent` mode every synced file lives under `.automatic/`, so that single
/// directory is the only pattern returned.
///
/// `project_dir` is the absolute project root; `agent_paths` are the
/// [`ManagedPath`] values collected from each selected agent.
pub fn build_patterns(
    project_dir: &Path,
    agent_paths: &[ManagedPath],
    silent: bool,
) -> Vec<String> {
    if silent {
        return vec!["/.automatic/".to_string()];
    }

    let mut patterns: Vec<String> = UNIVERSAL_DIRS.iter().map(|d| format!("/{d}/")).collect();

    // Agent-specific patterns, deduplicated and sorted for deterministic output.
    let mut agent_patterns: Vec<String> = Vec::new();
    for mp in agent_paths {
        let Ok(rel) = mp.path.strip_prefix(project_dir) else {
            // Defensive: a path outside the project root is never expected. Skip
            // rather than emit a broken absolute pattern.
            continue;
        };
        let comps = rel_components(rel);
        let Some(first) = comps.first() else {
            continue;
        };
        // Already covered by a wholesale universal directory ignore.
        if UNIVERSAL_DIRS.contains(&first.as_str()) {
            continue;
        }

        let pattern = if SHARED_TOOL_DIRS.contains(&first.as_str()) {
            // Surgical: keep the exact path so the rest of the shared directory
            // (CI workflows, editor settings) stays tracked.
            let mut s = comps.join("/");
            if mp.is_dir {
                s.push('/');
            }
            s
        } else if comps.len() > 1 {
            // Nested inside an agent's own directory — collapse to that
            // directory so all of it is ignored.
            format!("{first}/")
        } else {
            // Root-level file or directory.
            let mut s = first.clone();
            if mp.is_dir {
                s.push('/');
            }
            s
        };
        agent_patterns.push(format!("/{pattern}"));
    }
    agent_patterns.sort();
    agent_patterns.dedup();
    patterns.extend(agent_patterns);
    patterns
}

/// Render the managed block (markers included) from a pattern list.
fn render_block(patterns: &[String]) -> String {
    let mut out = String::new();
    out.push_str(BEGIN_MARKER);
    out.push('\n');
    out.push_str(HEADER_NOTE);
    out.push('\n');
    for p in patterns {
        out.push_str(p);
        out.push('\n');
    }
    out.push_str(END_MARKER);
    out.push('\n');
    out
}

/// Split existing `.gitignore` content into the parts before and after any
/// existing managed block.  When no block is present, `after` is empty and
/// `before` is the whole content.
fn split_around_block(content: &str) -> (String, String) {
    let lines: Vec<&str> = content.lines().collect();
    let begin = lines.iter().position(|l| l.starts_with(BEGIN_MARKER));
    let Some(begin) = begin else {
        return (content.to_string(), String::new());
    };
    // End marker at or after the begin marker; fall back to begin so a
    // truncated block (missing end marker) is still replaced rather than
    // duplicated.
    let end = lines[begin..]
        .iter()
        .position(|l| l.starts_with(END_MARKER))
        .map(|offset| begin + offset)
        .unwrap_or(begin);

    let before = lines[..begin].join("\n");
    let after = if end + 1 < lines.len() {
        lines[end + 1..].join("\n")
    } else {
        String::new()
    };
    (before, after)
}

/// Join a leading region, the managed block, and a trailing region into the
/// final file content with tidy blank-line separation.
fn assemble(before: &str, block: &str, after: &str) -> String {
    let before = before.trim_end_matches('\n');
    let after = after.trim_start_matches('\n').trim_end_matches('\n');

    let mut out = String::new();
    if !before.is_empty() {
        out.push_str(before);
        out.push_str("\n\n");
    }
    out.push_str(block);
    if !after.is_empty() {
        out.push('\n');
        out.push_str(after);
        out.push('\n');
    }
    out
}

/// Write or replace the Automatic-managed block in `<project_dir>/.gitignore`.
///
/// Idempotent: an existing block is replaced in place, otherwise the block is
/// appended.  The file is created when absent.  Returns the path written.
pub fn write_managed_block(project_dir: &Path, patterns: &[String]) -> Result<PathBuf, String> {
    let path = project_dir.join(".gitignore");
    let existing = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("Failed to read {}: {e}", path.display())),
    };

    let (before, after) = split_around_block(&existing);
    let block = render_block(patterns);
    let content = assemble(&before, &block, &after);

    fs::write(&path, content).map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
    Ok(path)
}

/// Remove the Automatic-managed block from `<project_dir>/.gitignore`.
///
/// Leaves every other line untouched.  No-op when the file or the block is
/// absent.  When removing the block empties the file, the file is deleted so no
/// stray empty `.gitignore` is left behind.
pub fn remove_managed_block(project_dir: &Path) -> Result<(), String> {
    let path = project_dir.join(".gitignore");
    let existing = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("Failed to read {}: {e}", path.display())),
    };

    if !existing.lines().any(|l| l.starts_with(BEGIN_MARKER)) {
        return Ok(());
    }

    let (before, after) = split_around_block(&existing);
    let before = before.trim_end_matches('\n');
    let after = after.trim_start_matches('\n').trim_end_matches('\n');

    let content = match (before.is_empty(), after.is_empty()) {
        (true, true) => String::new(),
        (false, true) => format!("{before}\n"),
        (true, false) => format!("{after}\n"),
        (false, false) => format!("{before}\n\n{after}\n"),
    };

    if content.is_empty() {
        // The block was the only content; do not leave an empty file behind.
        fs::remove_file(&path)
            .map_err(|e| format!("Failed to remove {}: {e}", path.display()))?;
    } else {
        fs::write(&path, content)
            .map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn dir_path(root: &Path, rel: &str) -> ManagedPath {
        ManagedPath {
            path: root.join(rel),
            is_dir: true,
        }
    }
    fn file_path(root: &Path, rel: &str) -> ManagedPath {
        ManagedPath {
            path: root.join(rel),
            is_dir: false,
        }
    }

    #[test]
    fn build_patterns_collapses_agent_subpaths_to_top_dir() {
        let root = PathBuf::from("/proj");
        // Several subpaths of .claude collapse to a single wholesale ignore.
        let paths = vec![
            file_path(&root, "CLAUDE.md"),
            dir_path(&root, ".claude/agents"),
            dir_path(&root, ".claude/commands"),
            dir_path(&root, ".claude/skills"),
            file_path(&root, ".mcp.json"),
        ];
        let patterns = build_patterns(&root, &paths, false);
        assert_eq!(
            patterns,
            vec![
                "/.agents/".to_string(),
                "/.automatic/".to_string(),
                "/.claude/".to_string(),
                "/.mcp.json".to_string(),
                "/CLAUDE.md".to_string(),
            ]
        );
    }

    #[test]
    fn build_patterns_wholesale_dir_covers_merged_config_file() {
        let root = PathBuf::from("/proj");
        // Codex only exposes .codex/agents, but the wholesale .codex/ ignore
        // also sweeps up the merged .codex/config.toml that lives beside it.
        let paths = vec![
            file_path(&root, "AGENTS.md"),
            dir_path(&root, ".codex/agents"),
        ];
        let patterns = build_patterns(&root, &paths, false);
        assert!(patterns.contains(&"/.codex/".to_string()));
        assert!(!patterns.iter().any(|p| p.contains("config.toml")));
    }

    #[test]
    fn build_patterns_keeps_shared_tool_dirs_surgical() {
        let root = PathBuf::from("/proj");
        // .github and .vscode belong to CI / the editor — never ignore them
        // wholesale, only the exact files Automatic writes.
        let paths = vec![
            file_path(&root, ".github/copilot-instructions.md"),
            dir_path(&root, ".github/prompts"),
            file_path(&root, ".vscode/mcp.json"),
            dir_path(&root, ".zed/agents"),
            file_path(&root, ".rules"),
        ];
        let patterns = build_patterns(&root, &paths, false);
        assert_eq!(
            patterns,
            vec![
                "/.agents/".to_string(),
                "/.automatic/".to_string(),
                "/.github/copilot-instructions.md".to_string(),
                "/.github/prompts/".to_string(),
                "/.rules".to_string(),
                "/.vscode/mcp.json".to_string(),
                "/.zed/agents/".to_string(),
            ]
        );
        // The whole shared directory is never emitted.
        assert!(!patterns.contains(&"/.github/".to_string()));
        assert!(!patterns.contains(&"/.vscode/".to_string()));
        assert!(!patterns.contains(&"/.zed/".to_string()));
    }

    #[test]
    fn build_patterns_suppresses_children_of_universal_dirs() {
        let root = PathBuf::from("/proj");
        // .agents/skills is redundant with the universal /.agents/ entry.
        let paths = vec![dir_path(&root, ".agents/skills")];
        let patterns = build_patterns(&root, &paths, false);
        assert_eq!(
            patterns,
            vec!["/.agents/".to_string(), "/.automatic/".to_string()]
        );
    }

    #[test]
    fn build_patterns_dedupes_shared_instruction_file() {
        let root = PathBuf::from("/proj");
        // Several agents share AGENTS.md; it must appear once.
        let paths = vec![
            file_path(&root, "AGENTS.md"),
            file_path(&root, "AGENTS.md"),
            file_path(&root, "WARP.md"),
        ];
        let patterns = build_patterns(&root, &paths, false);
        assert_eq!(
            patterns,
            vec![
                "/.agents/".to_string(),
                "/.automatic/".to_string(),
                "/AGENTS.md".to_string(),
                "/WARP.md".to_string(),
            ]
        );
    }

    #[test]
    fn build_patterns_silent_mode_is_automatic_only() {
        let root = PathBuf::from("/proj");
        let paths = vec![file_path(&root, "CLAUDE.md")];
        let patterns = build_patterns(&root, &paths, true);
        assert_eq!(patterns, vec!["/.automatic/".to_string()]);
    }

    #[test]
    fn write_creates_file_when_absent() {
        let tmp = tempdir().unwrap();
        write_managed_block(tmp.path(), &["/CLAUDE.md".to_string()]).unwrap();
        let content = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(content.contains(BEGIN_MARKER));
        assert!(content.contains("/CLAUDE.md"));
        assert!(content.trim_end().ends_with(END_MARKER));
    }

    #[test]
    fn write_appends_and_preserves_existing_content() {
        let tmp = tempdir().unwrap();
        let gi = tmp.path().join(".gitignore");
        fs::write(&gi, "node_modules/\ndist/\n").unwrap();
        write_managed_block(tmp.path(), &["/.claude/".to_string()]).unwrap();
        let content = fs::read_to_string(&gi).unwrap();
        assert!(content.starts_with("node_modules/\ndist/\n"));
        assert!(content.contains(BEGIN_MARKER));
        assert!(content.contains("/.claude/"));
    }

    #[test]
    fn write_is_idempotent_and_replaces_block() {
        let tmp = tempdir().unwrap();
        write_managed_block(tmp.path(), &["/CLAUDE.md".to_string()]).unwrap();
        let first = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        // Re-run with the same patterns → byte-identical.
        write_managed_block(tmp.path(), &["/CLAUDE.md".to_string()]).unwrap();
        let second = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert_eq!(first, second);
        // Re-run with different patterns → old block gone, new block present,
        // exactly one block.
        write_managed_block(tmp.path(), &["/AGENTS.md".to_string()]).unwrap();
        let third = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert_eq!(third.matches(BEGIN_MARKER).count(), 1);
        assert!(third.contains("/AGENTS.md"));
        assert!(!third.contains("/CLAUDE.md"));
    }

    #[test]
    fn remove_strips_block_and_keeps_surrounding_lines() {
        let tmp = tempdir().unwrap();
        let gi = tmp.path().join(".gitignore");
        fs::write(&gi, "node_modules/\n").unwrap();
        write_managed_block(tmp.path(), &["/CLAUDE.md".to_string()]).unwrap();
        remove_managed_block(tmp.path()).unwrap();
        let content = fs::read_to_string(&gi).unwrap();
        assert!(!content.contains(BEGIN_MARKER));
        assert!(!content.contains("/CLAUDE.md"));
        assert_eq!(content, "node_modules/\n");
    }

    #[test]
    fn remove_deletes_file_when_block_was_only_content() {
        let tmp = tempdir().unwrap();
        write_managed_block(tmp.path(), &["/CLAUDE.md".to_string()]).unwrap();
        remove_managed_block(tmp.path()).unwrap();
        assert!(!tmp.path().join(".gitignore").exists());
    }

    #[test]
    fn remove_is_noop_when_no_block_present() {
        let tmp = tempdir().unwrap();
        let gi = tmp.path().join(".gitignore");
        fs::write(&gi, "node_modules/\n").unwrap();
        remove_managed_block(tmp.path()).unwrap();
        let content = fs::read_to_string(&gi).unwrap();
        assert_eq!(content, "node_modules/\n");
    }

    #[test]
    fn remove_is_noop_when_file_absent() {
        let tmp = tempdir().unwrap();
        remove_managed_block(tmp.path()).unwrap();
        assert!(!tmp.path().join(".gitignore").exists());
    }
}
