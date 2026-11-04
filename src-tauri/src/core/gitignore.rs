//! Managed `.gitignore` block for Automatic-written files.
//!
//! When a project opts in (`Project.manage_gitignore`), the sync engine keeps a
//! bounded block in the project's `.gitignore` listing every path Automatic
//! writes, so generated agent configuration is not committed.  The block is
//! delimited by explicit markers and regenerated on every sync, so it is safe
//! to re-run.  Everything outside the markers is preserved untouched.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::agent::ManagedPath;
use crate::core::{Project, ProjectMode};

/// First line of the managed block.  Presence of this exact prefix identifies
/// an existing block to replace or remove.
pub(crate) const BEGIN_MARKER: &str = "# BEGIN Automatic-managed";
/// Last line of the managed block.
pub(crate) const END_MARKER: &str = "# END Automatic-managed";
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

/// Build the patterns for `project` from the paths every selected agent
/// reports it owns.
///
/// The sync engine writes the block from this list and drift detection
/// compares the block on disk against it, so both must call this one
/// function.  `project_dir` is the project root, never the Silent-mode
/// redirect directory.
pub fn project_patterns(project_dir: &Path, project: &Project) -> Vec<String> {
    let managed_paths: Vec<ManagedPath> = project
        .agents
        .iter()
        .filter_map(|id| crate::agent::from_id(id))
        .flat_map(|agent| agent.managed_gitignore_paths(project_dir))
        .collect();
    let silent = matches!(project.mode, ProjectMode::Silent);
    build_patterns(project_dir, &managed_paths, silent)
}

/// Render the managed block (markers included) from a pattern list.
pub fn render_block(patterns: &[String]) -> String {
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

/// Largest `.gitignore` Automatic reads.  Real files are a few kilobytes; the
/// cap stops the timer-driven drift check reading a huge or endless file.
const MAX_GITIGNORE_BYTES: u64 = 1024 * 1024;

/// What `<project_dir>/.gitignore` holds, as far as the managed block goes.
enum Contents {
    Absent,
    /// No begin marker.  Kept as raw bytes so a file that is not UTF-8
    /// survives an append unchanged.
    NoBlock(Vec<u8>),
    /// UTF-8 text holding at least one begin marker.
    Block(String),
    /// A `.gitignore` Automatic must not edit, with the reason.
    Unmanageable(String),
}

/// Classify `<project_dir>/.gitignore`.  Read-only.
///
/// A symlink is followed only when it resolves to a regular file inside the
/// project, so a sync never edits a file elsewhere on disk.  Directories,
/// FIFOs, symlinks that leave the project, files over
/// [`MAX_GITIGNORE_BYTES`], and non-UTF-8 files that hold a block are
/// [`Contents::Unmanageable`].  `Err` is reserved for I/O failures.
fn inspect(project_dir: &Path) -> Result<Contents, String> {
    let path = project_dir.join(".gitignore");
    let meta = match fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Contents::Absent),
        Err(e) => return Err(format!("Failed to inspect {}: {e}", path.display())),
    };
    if meta.file_type().is_symlink() {
        if let Some(reason) = symlink_problem(project_dir, &path) {
            return Ok(Contents::Unmanageable(reason));
        }
    } else if !meta.file_type().is_file() {
        return Ok(Contents::Unmanageable(format!(
            "{} is not a regular file",
            path.display()
        )));
    }

    let file =
        fs::File::open(&path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(MAX_GITIGNORE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    if bytes.len() as u64 > MAX_GITIGNORE_BYTES {
        return Ok(Contents::Unmanageable(format!(
            "{} is larger than {MAX_GITIGNORE_BYTES} bytes",
            path.display()
        )));
    }
    if !has_block(&bytes) {
        return Ok(Contents::NoBlock(bytes));
    }
    // The text around a block must survive a rewrite unchanged, so a file
    // that is not UTF-8 is never rewritten lossily.
    match String::from_utf8(bytes) {
        Ok(text) => Ok(Contents::Block(text)),
        Err(_) => Ok(Contents::Unmanageable(format!(
            "{} is not UTF-8 text",
            path.display()
        ))),
    }
}

/// Why a symlinked `.gitignore` must not be followed, or `None` when it
/// resolves to a regular file inside the project.
///
/// Targets under `.git/` are refused too: a cloned repository could commit
/// `.gitignore -> .git/config`, and appending the block there breaks git.
fn symlink_problem(project_dir: &Path, path: &Path) -> Option<String> {
    let resolves_inside = match (fs::canonicalize(path), fs::canonicalize(project_dir)) {
        (Ok(target), Ok(root)) => {
            target.strip_prefix(&root).is_ok_and(|rel| {
                !rel.components()
                    .any(|c| c.as_os_str().eq_ignore_ascii_case(".git"))
            }) && fs::metadata(&target).is_ok_and(|m| m.is_file())
        }
        _ => false,
    };
    (!resolves_inside).then(|| {
        format!(
            "{} is a symlink to something other than a file inside the project",
            path.display()
        )
    })
}

/// `true` when the raw file contains the begin marker.  Checked on bytes so a
/// file that is not UTF-8, and holds no block, is never a problem.
fn has_block(bytes: &[u8]) -> bool {
    bytes
        .windows(BEGIN_MARKER.len())
        .any(|w| w == BEGIN_MARKER.as_bytes())
}

/// Inclusive line ranges of every managed block in `lines`.
///
/// A block whose end marker is missing (before the file ends or the next
/// begin marker) is just its begin line, so a sync replaces the marker rather
/// than swallowing the user's lines below it.  More than one block only
/// appears after a bad merge, and every one of them is Automatic's.
fn block_ranges(lines: &[&str]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if !lines[i].starts_with(BEGIN_MARKER) {
            i += 1;
            continue;
        }
        let end = lines[i + 1..]
            .iter()
            .position(|l| l.starts_with(END_MARKER) || l.starts_with(BEGIN_MARKER))
            .map(|offset| i + 1 + offset)
            .filter(|&j| lines[j].starts_with(END_MARKER))
            .unwrap_or(i);
        ranges.push((i, end));
        i = end + 1;
    }
    ranges
}

/// Split `.gitignore` content into the parts before the first managed block
/// and after it, with every block removed.  `None` when there is no block.
fn split_around_block(content: &str) -> Option<(String, String)> {
    let lines: Vec<&str> = content.lines().collect();
    let ranges = block_ranges(&lines);
    let (first, _) = *ranges.first()?;
    let in_block = |i: usize| ranges.iter().any(|&(b, e)| (b..=e).contains(&i));

    let before = lines[..first].join("\n");
    let after: Vec<&str> = (first..lines.len())
        .filter(|&i| !in_block(i))
        .map(|i| lines[i])
        .collect();
    Some((before, after.join("\n")))
}

/// Every managed block in `.gitignore` content, markers included and each
/// newline-terminated, in the form [`render_block`] produces.  Uses the same
/// bounds as [`split_around_block`].  Two blocks are returned together, so
/// they never match a single rendered block.
fn extract_blocks(content: &str) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    let ranges = block_ranges(&lines);
    if ranges.is_empty() {
        return None;
    }
    let mut out = String::new();
    for (begin, end) in ranges {
        out.push_str(&lines[begin..=end].join("\n"));
        out.push('\n');
    }
    Some(out)
}

/// The managed block in `<project_dir>/.gitignore`, as drift detection sees it.
#[derive(Debug, PartialEq, Eq)]
pub enum BlockOnDisk {
    /// No block: the file is absent or holds no begin marker.
    Absent,
    /// Every block found, directly comparable with [`render_block`] output.
    Present(String),
    /// `.gitignore` exists but Automatic will not edit it.  The reason is the
    /// same one a sync reports.
    Unmanageable(String),
}

/// Read the managed block currently in `<project_dir>/.gitignore`.  Read-only.
pub fn read_managed_block(project_dir: &Path) -> Result<BlockOnDisk, String> {
    Ok(match inspect(project_dir)? {
        Contents::Absent | Contents::NoBlock(_) => BlockOnDisk::Absent,
        Contents::Block(text) => {
            extract_blocks(&text).map_or(BlockOnDisk::Absent, BlockOnDisk::Present)
        }
        Contents::Unmanageable(reason) => BlockOnDisk::Unmanageable(reason),
    })
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

/// Append the block after existing content, separated by one blank line.
/// Works on bytes so a file that is not UTF-8 keeps its content unchanged.
fn append_block(mut existing: Vec<u8>, block: &str) -> Vec<u8> {
    while existing.last() == Some(&b'\n') {
        existing.pop();
    }
    if !existing.is_empty() {
        existing.extend_from_slice(b"\n\n");
    }
    existing.extend_from_slice(block.as_bytes());
    existing
}

/// Write or replace the Automatic-managed block in `<project_dir>/.gitignore`.
///
/// Idempotent: an existing block is replaced in place (extra blocks left by a
/// merge are dropped), otherwise the block is appended.  The file is created
/// when absent.  Fails, naming the reason, when `.gitignore` is one Automatic
/// will not edit (see [`inspect`]).  Returns the path written.
pub fn write_managed_block(project_dir: &Path, patterns: &[String]) -> Result<PathBuf, String> {
    let path = project_dir.join(".gitignore");
    let block = render_block(patterns);
    let content = match inspect(project_dir)? {
        Contents::Unmanageable(reason) => {
            return Err(format!(
                "{reason}, so Automatic cannot keep its managed block there. Fix the file \
                 or turn off Manage .gitignore for this project."
            ))
        }
        Contents::Absent => block.into_bytes(),
        Contents::NoBlock(bytes) => append_block(bytes, &block),
        Contents::Block(text) => match split_around_block(&text) {
            Some((before, after)) => assemble(&before, &block, &after).into_bytes(),
            // The marker text appears mid-line only: not a block.
            None => append_block(text.into_bytes(), &block),
        },
    };

    fs::write(&path, content).map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
    Ok(path)
}

/// Remove every Automatic-managed block from `<project_dir>/.gitignore`.
///
/// Leaves every other line untouched.  No-op when the file or the block is
/// absent.  When removing the block empties the file, the file is deleted so no
/// stray empty `.gitignore` is left behind.
pub fn remove_managed_block(project_dir: &Path) -> Result<(), String> {
    let path = project_dir.join(".gitignore");
    let text = match inspect(project_dir)? {
        Contents::Absent | Contents::NoBlock(_) => return Ok(()),
        // This runs on every sync of a project with the setting off.  The user
        // has not asked Automatic to manage this file, so one it will not edit
        // is left alone rather than failing their sync.  Drift ignores it too.
        Contents::Unmanageable(_) => return Ok(()),
        Contents::Block(text) => text,
    };
    let Some((before, after)) = split_around_block(&text) else {
        return Ok(());
    };
    let before = before.trim_end_matches('\n');
    let after = after.trim_start_matches('\n').trim_end_matches('\n');

    let content = match (before.is_empty(), after.is_empty()) {
        (true, true) => String::new(),
        (false, true) => format!("{before}\n"),
        (true, false) => format!("{after}\n"),
        (false, false) => format!("{before}\n\n{after}\n"),
    };

    // Removing a symlink would leave the block in its target, so a followed
    // symlink keeps its (now empty) target instead.
    let is_symlink = fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink());
    if content.is_empty() && !is_symlink {
        // The block was the only content; do not leave an empty file behind.
        fs::remove_file(&path).map_err(|e| format!("Failed to remove {}: {e}", path.display()))?;
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
    fn non_utf8_file_without_block_is_never_a_problem() {
        let tmp = tempdir().unwrap();
        let gi = tmp.path().join(".gitignore");
        // Latin-1 "café/", which is not valid UTF-8.
        let latin1 = b"caf\xe9/\n".to_vec();
        fs::write(&gi, &latin1).unwrap();

        assert_eq!(read_managed_block(tmp.path()).unwrap(), BlockOnDisk::Absent);
        remove_managed_block(tmp.path()).unwrap();
        assert_eq!(fs::read(&gi).unwrap(), latin1);

        // Appending keeps the user's bytes exactly.
        write_managed_block(tmp.path(), &["/CLAUDE.md".to_string()]).unwrap();
        let written = fs::read(&gi).unwrap();
        assert!(written.starts_with(b"caf\xe9/\n\n"));
        assert!(written.ends_with(render_block(&["/CLAUDE.md".to_string()]).as_bytes()));
    }

    #[test]
    fn non_utf8_file_with_block_is_never_rewritten() {
        let tmp = tempdir().unwrap();
        let gi = tmp.path().join(".gitignore");
        let mut bytes = b"caf\xe9/\n\n".to_vec();
        bytes.extend_from_slice(render_block(&["/CLAUDE.md".to_string()]).as_bytes());
        fs::write(&gi, &bytes).unwrap();

        assert!(matches!(
            read_managed_block(tmp.path()).unwrap(),
            BlockOnDisk::Unmanageable(reason) if reason.contains("not UTF-8")
        ));
        let err = write_managed_block(tmp.path(), &["/AGENTS.md".to_string()]).unwrap_err();
        assert!(err.contains("not UTF-8"), "{err}");
        // With the setting off the sync carries on and the file is untouched.
        remove_managed_block(tmp.path()).unwrap();
        assert_eq!(fs::read(&gi).unwrap(), bytes);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_leaving_the_project_is_never_read_or_written_through() {
        let tmp = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let target = outside.path().join("elsewhere");
        let target_content = format!("keep\n{}", render_block(&["/CLAUDE.md".to_string()]));
        fs::write(&target, &target_content).unwrap();
        std::os::unix::fs::symlink(&target, tmp.path().join(".gitignore")).unwrap();

        assert!(matches!(
            read_managed_block(tmp.path()).unwrap(),
            BlockOnDisk::Unmanageable(reason) if reason.contains("symlink")
        ));
        let err = write_managed_block(tmp.path(), &["/AGENTS.md".to_string()]).unwrap_err();
        assert!(err.contains("Manage .gitignore"), "{err}");
        remove_managed_block(tmp.path()).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), target_content);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_to_a_file_inside_the_project_is_followed() {
        let tmp = tempdir().unwrap();
        let target = tmp.path().join("config").join("gitignore");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "node_modules/\n").unwrap();
        let link = tmp.path().join(".gitignore");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let patterns = vec!["/CLAUDE.md".to_string()];
        write_managed_block(tmp.path(), &patterns).unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert!(fs::read_to_string(&target).unwrap().contains("/CLAUDE.md"));
        assert_eq!(
            read_managed_block(tmp.path()).unwrap(),
            BlockOnDisk::Present(render_block(&patterns))
        );
        remove_managed_block(tmp.path()).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "node_modules/\n");

        // When the block was the target's only content, removal empties the
        // target and keeps the link, rather than deleting the link and
        // leaving the block behind.
        fs::write(&target, "").unwrap();
        write_managed_block(tmp.path(), &patterns).unwrap();
        remove_managed_block(tmp.path()).unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(&target).unwrap(), "");
        assert_eq!(read_managed_block(tmp.path()).unwrap(), BlockOnDisk::Absent);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_into_dot_git_is_never_followed() {
        let tmp = tempdir().unwrap();
        let config = tmp.path().join(".git").join("config");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, "[core]\n").unwrap();
        std::os::unix::fs::symlink(&config, tmp.path().join(".gitignore")).unwrap();

        assert!(matches!(
            read_managed_block(tmp.path()).unwrap(),
            BlockOnDisk::Unmanageable(_)
        ));
        assert!(write_managed_block(tmp.path(), &["/CLAUDE.md".to_string()]).is_err());
        assert_eq!(fs::read_to_string(&config).unwrap(), "[core]\n");
    }

    #[test]
    fn oversized_gitignore_is_unmanageable_but_never_fails_removal() {
        let tmp = tempdir().unwrap();
        let big = vec![b'x'; MAX_GITIGNORE_BYTES as usize + 1];
        fs::write(tmp.path().join(".gitignore"), &big).unwrap();

        assert!(matches!(
            read_managed_block(tmp.path()).unwrap(),
            BlockOnDisk::Unmanageable(reason) if reason.contains("larger than")
        ));
        assert!(write_managed_block(tmp.path(), &["/CLAUDE.md".to_string()]).is_err());
        remove_managed_block(tmp.path()).unwrap();
        assert_eq!(fs::read(tmp.path().join(".gitignore")).unwrap(), big);
    }

    #[test]
    fn directory_named_gitignore_is_unmanageable() {
        let tmp = tempdir().unwrap();
        fs::create_dir(tmp.path().join(".gitignore")).unwrap();
        assert!(matches!(
            read_managed_block(tmp.path()).unwrap(),
            BlockOnDisk::Unmanageable(reason) if reason.contains("not a regular file")
        ));
        remove_managed_block(tmp.path()).unwrap();
    }

    #[test]
    fn duplicate_blocks_are_reported_and_collapsed() {
        let tmp = tempdir().unwrap();
        let gi = tmp.path().join(".gitignore");
        let one = render_block(&["/CLAUDE.md".to_string()]);
        let two = render_block(&["/AGENTS.md".to_string()]);
        fs::write(&gi, format!("top/\n{one}middle/\n{two}bottom/\n")).unwrap();

        // Both blocks come back together, so they never match one block.
        assert_eq!(
            read_managed_block(tmp.path()).unwrap(),
            BlockOnDisk::Present(format!("{one}{two}"))
        );

        write_managed_block(tmp.path(), &["/.claude/".to_string()]).unwrap();
        let content = fs::read_to_string(&gi).unwrap();
        assert_eq!(content.matches(BEGIN_MARKER).count(), 1);
        assert_eq!(
            content,
            format!(
                "top/\n\n{}\nmiddle/\nbottom/\n",
                render_block(&["/.claude/".to_string()])
            )
        );

        fs::write(&gi, format!("top/\n{one}middle/\n{two}bottom/\n")).unwrap();
        remove_managed_block(tmp.path()).unwrap();
        assert_eq!(fs::read_to_string(&gi).unwrap(), "top/\n\nmiddle/\nbottom/\n");
    }

    #[test]
    fn truncated_block_does_not_swallow_a_later_block() {
        // The first block lost its end marker. Its range must stop at the
        // next begin marker, not run on to the second block's end marker.
        let lines = vec![BEGIN_MARKER, "user/", BEGIN_MARKER, "/CLAUDE.md", END_MARKER];
        assert_eq!(block_ranges(&lines), vec![(0, 0), (2, 4)]);
    }

    #[test]
    fn read_returns_written_block_verbatim() {
        let tmp = tempdir().unwrap();
        let gi = tmp.path().join(".gitignore");
        fs::write(&gi, "node_modules/\n").unwrap();
        let patterns = vec!["/.claude/".to_string(), "/CLAUDE.md".to_string()];
        write_managed_block(tmp.path(), &patterns).unwrap();
        fs::write(&gi, format!("{}tail/\n", fs::read_to_string(&gi).unwrap())).unwrap();

        assert_eq!(
            read_managed_block(tmp.path()).unwrap(),
            BlockOnDisk::Present(render_block(&patterns))
        );
    }

    #[test]
    fn read_returns_absent_without_block_or_file() {
        let tmp = tempdir().unwrap();
        assert_eq!(read_managed_block(tmp.path()).unwrap(), BlockOnDisk::Absent);
        fs::write(tmp.path().join(".gitignore"), "node_modules/\n").unwrap();
        assert_eq!(read_managed_block(tmp.path()).unwrap(), BlockOnDisk::Absent);
    }

    #[test]
    fn read_detects_edited_and_truncated_blocks() {
        let tmp = tempdir().unwrap();
        let gi = tmp.path().join(".gitignore");
        let patterns = vec!["/CLAUDE.md".to_string()];
        let expected = render_block(&patterns);

        // A hand-added line inside the markers no longer matches.
        let edited = expected.replace("/CLAUDE.md\n", "/CLAUDE.md\n/extra\n");
        fs::write(&gi, &edited).unwrap();
        assert_eq!(
            read_managed_block(tmp.path()).unwrap(),
            BlockOnDisk::Present(edited)
        );

        // A block that lost its end marker is reduced to its begin line.
        fs::write(&gi, format!("{BEGIN_MARKER}\n/CLAUDE.md\n")).unwrap();
        assert_eq!(
            read_managed_block(tmp.path()).unwrap(),
            BlockOnDisk::Present(format!("{BEGIN_MARKER}\n"))
        );
    }

    #[test]
    fn remove_is_noop_when_file_absent() {
        let tmp = tempdir().unwrap();
        remove_managed_block(tmp.path()).unwrap();
        assert!(!tmp.path().join(".gitignore").exists());
    }
}
