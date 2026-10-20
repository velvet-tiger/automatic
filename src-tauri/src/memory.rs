use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

/// A single memory entry with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// The stored value.
    pub value: String,
    /// ISO 8601 timestamp when this entry was created or last updated.
    pub timestamp: String,
    /// Optional source identifier (e.g., which agent or tool stored this).
    pub source: Option<String>,
    /// Clerk user ID of the user whose agent stored this entry.
    /// Populated by the frontend/MCP caller for future team/cloud sync.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

/// Memory database type: a simple key-value store.
pub type MemoryDb = HashMap<String, MemoryEntry>;

pub fn get_memory_dir() -> Result<PathBuf, String> {
    Ok(crate::core::get_automatic_dir()?.join("memory"))
}

fn get_project_memory_path(project_name: &str) -> Result<PathBuf, String> {
    if !crate::core::is_valid_name(project_name) {
        return Err("Invalid project name".into());
    }
    let dir = get_memory_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    Ok(dir.join(format!("{}.json", project_name)))
}

pub fn read_memory_db(project_name: &str) -> Result<MemoryDb, String> {
    read_memory_db_at(&get_project_memory_path(project_name)?)
}

pub fn write_memory_db(project_name: &str, db: &MemoryDb) -> Result<(), String> {
    write_memory_db_at(&get_project_memory_path(project_name)?, db)
}

/// Reads the memory store at `path`. A missing or empty file is an empty
/// store. A file that does not parse is an error, never an empty store:
/// every write is a read-modify-write, so treating it as empty would let the
/// next `store_memory` replace all of the project's memories with one entry.
fn read_memory_db_at(path: &Path) -> Result<MemoryDb, String> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(e) => return Err(format!("Failed to read memory store {}: {}", path.display(), e)),
    };
    if raw.trim().is_empty() {
        return Ok(HashMap::new());
    }
    serde_json::from_str(&raw).map_err(|e| {
        format!(
            "Memory store {} is not valid JSON ({}). Fix the file or move it \
             aside. Automatic will not change it until it parses.",
            path.display(),
            e
        )
    })
}

/// Writes the memory store at `path` atomically, so a crash or a concurrent
/// reader never sees a partly written file.
fn write_memory_db_at(path: &Path, db: &MemoryDb) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(db)
        .map_err(|e| format!("Failed to serialise memory store {}: {}", path.display(), e))?;
    crate::core::write_registry_file(path, &raw)
}

// ── Path-injectable helpers used by tests ────────────────────────────────────

#[cfg(test)]
/// Read the memory DB from an explicit base directory (used in tests).
fn read_db_at(base: &Path, project_name: &str) -> Result<MemoryDb, String> {
    read_memory_db_at(&base.join(format!("{}.json", project_name)))
}

#[cfg(test)]
/// Write the memory DB to an explicit base directory (used in tests).
fn write_db_at(base: &Path, project_name: &str, db: &MemoryDb) -> Result<(), String> {
    write_memory_db_at(&base.join(format!("{}.json", project_name)), db)
}

#[cfg(test)]
/// Store a memory entry into an explicit base directory (used in tests).
fn store_at(
    base: &Path,
    project_name: &str,
    key: &str,
    value: &str,
    source: Option<&str>,
) -> Result<String, String> {
    let mut db = read_db_at(base, project_name)?;
    db.insert(
        key.to_string(),
        MemoryEntry {
            value: value.to_string(),
            timestamp: current_timestamp(),
            source: source.map(|s| s.to_string()),
            created_by: None,
        },
    );
    write_db_at(base, project_name, &db)?;
    Ok(format!(
        "Memory stored: key='{}' for project '{}'",
        key, project_name
    ))
}

#[cfg(test)]
/// Delete a memory entry from an explicit base directory (used in tests).
fn delete_at(base: &Path, project_name: &str, key: &str) -> Result<String, String> {
    let mut db = read_db_at(base, project_name)?;
    if db.remove(key).is_none() {
        return Err(format!("Memory key '{}' not found", key));
    }
    write_db_at(base, project_name, &db)?;
    Ok(format!(
        "Memory deleted: key='{}' for project '{}'",
        key, project_name
    ))
}

#[cfg(test)]
/// Clear memory entries from an explicit base directory (used in tests).
fn clear_at(
    base: &Path,
    project_name: &str,
    pattern: Option<&str>,
    confirm: bool,
) -> Result<usize, String> {
    if !confirm {
        return Err("Deletion not confirmed.".to_string());
    }
    let mut db = read_db_at(base, project_name)?;
    let deleted_count = if let Some(pat) = pattern {
        let pat_lower = pat.to_lowercase();
        let keys: Vec<String> = db
            .keys()
            .filter(|k| k.to_lowercase().contains(&pat_lower))
            .cloned()
            .collect();
        let n = keys.len();
        for k in keys {
            db.remove(&k);
        }
        n
    } else {
        let n = db.len();
        db.clear();
        n
    };
    write_db_at(base, project_name, &db)?;
    Ok(deleted_count)
}

/// Generates an ISO 8601 timestamp for the current time.
pub fn current_timestamp() -> String {
    chrono::Utc::now().to_rfc3339()
}

// ============================================================================
// Raw API for Frontend UI
// ============================================================================

/// Every entry in the store keyed `store_key` (a project `id`, or a value
/// passed through for an unregistered project).
pub fn get_all_memories(store_key: &str) -> Result<MemoryDb, String> {
    read_memory_db(store_key)
}

// ============================================================================
// Formatted API for MCP (Agents)
// ============================================================================
//
// Memory is keyed by project `id` (stage 3b step 2 of the project identity
// plan), so every checkout of a project shares one store. These functions
// take the project's resolved `ProjectStoreKeys`: `id` picks the file,
// `name` is used in messages, and `local_key` tags the activity row with
// the checkout the call came from.

pub fn store_memory(
    project: &crate::core::ProjectStoreKeys,
    key: &str,
    value: &str,
    source: Option<&str>,
) -> Result<String, String> {
    let mut db = read_memory_db(&project.id)?;

    db.insert(
        key.to_string(),
        MemoryEntry {
            value: value.to_string(),
            timestamp: current_timestamp(),
            source: source.map(|s| s.to_string()),
            created_by: None,
        },
    );

    write_memory_db(&project.id, &db)?;

    crate::activity::log(
        &project.local_key,
        crate::activity::ActivityEvent::MemoryStored,
        &format!("Memory stored: {}", key),
        key,
    );

    Ok(format!(
        "Memory stored: key='{}' for project '{}'",
        key, project.name
    ))
}

pub fn get_memory(project: &crate::core::ProjectStoreKeys, key: &str) -> Result<String, String> {
    let db = read_memory_db(&project.id)?;

    if let Some(entry) = db.get(key) {
        let mut output = format!("# Memory: {}\n\n", key);
        output.push_str(&format!("**Value:** {}\n", entry.value));
        output.push_str(&format!("**Timestamp:** {}\n", entry.timestamp));
        if let Some(src) = &entry.source {
            output.push_str(&format!("**Source:** {}\n", src));
        }
        Ok(output)
    } else {
        Err(format!("Memory key '{}' not found", key))
    }
}

pub fn list_memories(project: &crate::core::ProjectStoreKeys, pattern: Option<&str>) -> Result<String, String> {
    let db = read_memory_db(&project.id)?;

    if db.is_empty() {
        return Ok(format!("No memories stored for project '{}'", project.name));
    }

    let mut keys: Vec<&String> = db.keys().collect();
    keys.sort();

    let filtered_keys: Vec<&String> = if let Some(pat) = pattern {
        let pat_lower = pat.to_lowercase();
        keys.into_iter()
            .filter(|k| k.to_lowercase().contains(&pat_lower))
            .collect()
    } else {
        keys
    };

    if filtered_keys.is_empty() {
        return Ok(format!(
            "No memories matching pattern '{}' for project '{}'",
            pattern.unwrap_or(""),
            project.name
        ));
    }

    let mut output = format!("# Memories for '{}'\n\n", project.name);
    if let Some(pat) = pattern {
        output.push_str(&format!("Filtered by: {}\n\n", pat));
    }

    for key in filtered_keys {
        if let Some(entry) = db.get(key) {
            output.push_str(&format!("- **{}**\n", key));
            output.push_str(&format!("  Timestamp: {}\n", entry.timestamp));
            if let Some(src) = &entry.source {
                output.push_str(&format!("  Source: {}\n", src));
            }
            let preview = if entry.value.len() > 100 {
                format!("{}...", &entry.value[..100])
            } else {
                entry.value.clone()
            };
            output.push_str(&format!("  Preview: {}\n", preview));
        }
    }

    Ok(output)
}

pub fn search_memories(project: &crate::core::ProjectStoreKeys, query: &str) -> Result<String, String> {
    let db = read_memory_db(&project.id)?;

    if db.is_empty() {
        return Ok(format!("No memories stored for project '{}'", project.name));
    }

    let query_lower = query.to_lowercase();
    let mut matches: Vec<(&String, &MemoryEntry)> = db
        .iter()
        .filter(|(k, v)| {
            k.to_lowercase().contains(&query_lower) || v.value.to_lowercase().contains(&query_lower)
        })
        .collect();

    if matches.is_empty() {
        return Ok(format!(
            "No memories matching query '{}' for project '{}'",
            query, project.name
        ));
    }

    matches.sort_by_key(|(k, _)| *k);

    let mut output = format!("# Search results for '{}' in '{}'\n\n", query, project.name);
    output.push_str(&format!("Found {} match(es)\n\n", matches.len()));

    for (key, entry) in matches {
        output.push_str(&format!("## {}\n", key));
        output.push_str(&format!("**Value:** {}\n", entry.value));
        output.push_str(&format!("**Timestamp:** {}\n", entry.timestamp));
        if let Some(src) = &entry.source {
            output.push_str(&format!("**Source:** {}\n", src));
        }
        output.push('\n');
    }

    Ok(output)
}

pub fn delete_memory(project: &crate::core::ProjectStoreKeys, key: &str) -> Result<String, String> {
    let mut db = read_memory_db(&project.id)?;

    if db.remove(key).is_none() {
        return Err(format!("Memory key '{}' not found", key));
    }

    write_memory_db(&project.id, &db)?;

    crate::activity::log(
        &project.local_key,
        crate::activity::ActivityEvent::MemoryDeleted,
        &format!("Memory deleted: {}", key),
        key,
    );

    Ok(format!(
        "Memory deleted: key='{}' for project '{}'",
        key, project.name
    ))
}

pub fn clear_memories(
    project: &crate::core::ProjectStoreKeys,
    pattern: Option<&str>,
    confirm: bool,
) -> Result<String, String> {
    if !confirm {
        return Err("Deletion not confirmed. Set 'confirm' to true to proceed.".to_string());
    }

    let mut db = read_memory_db(&project.id)?;
    let deleted_count;

    if let Some(pat) = pattern {
        let pat_lower = pat.to_lowercase();
        let keys_to_delete: Vec<String> = db
            .keys()
            .filter(|k| k.to_lowercase().contains(&pat_lower))
            .cloned()
            .collect();

        deleted_count = keys_to_delete.len();
        for key in keys_to_delete {
            db.remove(&key);
        }
    } else {
        deleted_count = db.len();
        db.clear();
    }

    write_memory_db(&project.id, &db)?;

    let detail = if let Some(pat) = pattern {
        format!("{} entries matching '{}'", deleted_count, pat)
    } else {
        format!("{} entries", deleted_count)
    };

    crate::activity::log(
        &project.local_key,
        crate::activity::ActivityEvent::MemoryCleared,
        "Memory cleared",
        &detail,
    );

    if let Some(pat) = pattern {
        Ok(format!(
            "Cleared {} memor{} matching pattern '{}' for project '{}'",
            deleted_count,
            if deleted_count == 1 { "y" } else { "ies" },
            pat,
            project.name
        ))
    } else {
        Ok(format!(
            "Cleared all {} memor{} for project '{}'",
            deleted_count,
            if deleted_count == 1 { "y" } else { "ies" },
            project.name
        ))
    }
}

// ============================================================================
// Claude Auto-Memory Integration
// ============================================================================

/// The content read from Claude's auto-memory directory for a project.
///
/// `MEMORY.md` is the index file Claude keeps concise (first 200 lines are
/// loaded by Claude Code automatically each session).  `topic_files` lists
/// any additional topic files Claude has created alongside it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeMemoryContent {
    /// Absolute path to the memory directory (for display / linking in the UI).
    pub memory_dir: String,
    /// Contents of MEMORY.md, or None if the file does not exist yet.
    pub memory_md: Option<String>,
    /// Names and contents of any topic files in the directory (e.g. debugging.md).
    pub topic_files: Vec<ClaudeMemoryTopicFile>,
}

/// A single topic file inside the Claude auto-memory directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeMemoryTopicFile {
    /// Filename (e.g. "debugging.md").
    pub name: String,
    /// Full text content of the file.
    pub content: String,
}

/// Longest folder name Claude Code writes as-is. A longer encoded path is
/// cut to this length and given a `-<hash>` suffix.
const CLAUDE_FOLDER_NAME_MAX: usize = 200;

/// Largest Claude memory file Automatic reads. A longer file is cut at this
/// size and marked with [`CLAUDE_TRUNCATED_MARKER`].
const CLAUDE_MEMORY_FILE_MAX_BYTES: usize = 1024 * 1024;

/// Appended to a Claude memory file that was cut at
/// [`CLAUDE_MEMORY_FILE_MAX_BYTES`], so neither a reader nor a promotion
/// mistakes the excerpt for the whole file.
pub const CLAUDE_TRUNCATED_MARKER: &str =
    "\n\n[Truncated by Automatic: only the first 1 MiB of this file was read.]";

/// Encodes an absolute project path the way Claude Code names its
/// per-project folder under `~/.claude/projects/`.
///
/// Claude Code replaces every character that is not an ASCII letter or digit
/// with `-`, one `-` per UTF-16 code unit. So `/`, `.`, `_` and spaces each
/// become `-`, and a character outside the Basic Multilingual Plane (an
/// emoji, for example) becomes `--`. For example,
/// `/work/_active/app/.claude/worktrees/x` becomes
/// `-work--active-app--claude-worktrees-x`. The result is pure ASCII.
pub fn encode_claude_project_dir(project_dir: &str) -> String {
    let mut encoded = String::with_capacity(project_dir.len());
    for c in project_dir.chars() {
        if c.is_ascii_alphanumeric() {
            encoded.push(c);
        } else {
            for _ in 0..c.len_utf16() {
                encoded.push('-');
            }
        }
    }
    encoded
}

/// The forms of `project_dir` Claude Code may have named its folder after,
/// most likely first. Claude Code resolves symlinks and normalises to NFC
/// before encoding, so the canonical path comes first. The path as
/// registered comes second, for a folder that no longer exists or cannot be
/// resolved.
fn claude_project_path_candidates(project_dir: &str) -> Vec<String> {
    let mut candidates: Vec<String> = Vec::new();
    if let Ok(canonical) = fs::canonicalize(project_dir) {
        candidates.push(canonical.to_string_lossy().nfc().collect());
    }
    let raw: String = project_dir.nfc().collect();
    if !candidates.contains(&raw) {
        candidates.push(raw);
    }
    candidates
}

/// The folder under `projects_root` that Claude Code uses for `encoded`, or
/// `None` when the name is too long and no single folder matches. A long
/// name carries a hash suffix Automatic cannot compute, so it is found by
/// its 200-character prefix followed by `-`. Only a unique match is
/// accepted: two projects whose paths share that prefix must never read
/// each other's memory, and the hash cannot tell them apart.
fn claude_project_folder(projects_root: &Path, encoded: &str) -> Option<PathBuf> {
    if encoded.len() <= CLAUDE_FOLDER_NAME_MAX {
        return Some(projects_root.join(encoded));
    }
    // `encoded` is ASCII, so this byte index is a character boundary.
    let prefix = format!("{}-", &encoded[..CLAUDE_FOLDER_NAME_MAX]);
    let entries = fs::read_dir(projects_root).ok()?;
    let names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| name.starts_with(&prefix))
        .collect();
    match names.as_slice() {
        [only] => Some(projects_root.join(only)),
        [] => None,
        _ => {
            eprintln!(
                "[automatic] not reading Claude memory: {} folders in {} share the prefix {}",
                names.len(),
                projects_root.display(),
                prefix
            );
            None
        }
    }
}

/// The Claude memory folder for `project_dir` under `projects_root`. Each
/// candidate path is tried in turn, and the first whose project folder
/// exists wins. When none exists, the first candidate's expected path is
/// returned so the UI can show where Claude Code would write.
fn claude_memory_dir_in(projects_root: &Path, project_dir: &str) -> PathBuf {
    let mut fallback: Option<PathBuf> = None;
    for candidate in claude_project_path_candidates(project_dir) {
        let encoded = encode_claude_project_dir(&candidate);
        match claude_project_folder(projects_root, &encoded) {
            Some(folder) if folder.is_dir() => return folder.join("memory"),
            Some(folder) => {
                fallback.get_or_insert(folder);
            }
            None => {
                fallback.get_or_insert_with(|| {
                    projects_root.join(&encoded[..CLAUDE_FOLDER_NAME_MAX])
                });
            }
        }
    }
    fallback
        .unwrap_or_else(|| projects_root.join(encode_claude_project_dir(project_dir)))
        .join("memory")
}

/// Derives the Claude auto-memory directory for a project directory:
/// `~/.claude/projects/<encoded>/memory`, with `<encoded>` from
/// [`encode_claude_project_dir`].
///
/// For example:
///   `/Users/alice/projects/foo` → `~/.claude/projects/-Users-alice-projects-foo/memory`
///
/// If `project_dir` is empty or the home directory cannot be resolved, returns
/// `None`.
pub fn claude_memory_dir(project_dir: &str) -> Option<PathBuf> {
    if project_dir.is_empty() {
        return None;
    }
    let home = crate::core::home_dir().ok()?;
    let projects_root = home.join(".claude").join("projects");
    Some(claude_memory_dir_in(&projects_root, project_dir))
}

/// Reads one Claude memory file, or `None` when it is absent or is not a
/// regular file. Symlinks are not followed: the folder belongs to another
/// tool, and a link could point the read (and the MCP tool that returns it)
/// at any file the user can read. A FIFO or device is skipped because
/// reading it could block. Content past [`CLAUDE_MEMORY_FILE_MAX_BYTES`] is
/// dropped and [`CLAUDE_TRUNCATED_MARKER`] is appended.
fn read_claude_memory_file(path: &Path) -> Result<Option<String>, String> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Failed to inspect {}: {}", path.display(), e)),
    };
    if !meta.file_type().is_file() {
        eprintln!(
            "[automatic] skipped Claude memory entry {}: not a regular file",
            path.display()
        );
        return Ok(None);
    }
    let file = fs::File::open(path)
        .map_err(|e| format!("Failed to open {}: {}", path.display(), e))?;
    let mut bytes = Vec::new();
    file.take(CLAUDE_MEMORY_FILE_MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    let truncated = bytes.len() > CLAUDE_MEMORY_FILE_MAX_BYTES;
    bytes.truncate(CLAUDE_MEMORY_FILE_MAX_BYTES);
    let mut text = match String::from_utf8(bytes) {
        Ok(text) => text,
        // The cut can land inside a multi-byte character. Drop that
        // incomplete tail. Invalid UTF-8 anywhere else is an error.
        Err(e) if truncated && e.utf8_error().error_len().is_none() => {
            let valid = e.utf8_error().valid_up_to();
            let mut bytes = e.into_bytes();
            bytes.truncate(valid);
            String::from_utf8(bytes).map_err(|e| {
                format!("{} is not valid UTF-8: {}", path.display(), e)
            })?
        }
        Err(e) => return Err(format!("{} is not valid UTF-8: {}", path.display(), e)),
    };
    if truncated {
        text.push_str(CLAUDE_TRUNCATED_MARKER);
    }
    Ok(Some(text))
}

/// Reads Claude's auto-memory content for a project directory.
///
/// Returns a `ClaudeMemoryContent` describing the MEMORY.md index and any
/// topic files present.  Does not error if the directory does not exist yet —
/// in that case `memory_md` is `None` and `topic_files` is empty.
pub fn read_claude_memory(project_dir: &str) -> Result<ClaudeMemoryContent, String> {
    let memory_dir = claude_memory_dir(project_dir).ok_or_else(|| {
        "Cannot derive Claude memory path: project directory is not set".to_string()
    })?;
    read_claude_memory_at(&memory_dir)
}

/// Reads the Claude memory folder at `memory_dir`. See
/// [`read_claude_memory_file`] for which files are read.
fn read_claude_memory_at(memory_dir: &Path) -> Result<ClaudeMemoryContent, String> {
    let memory_dir_str = memory_dir.to_string_lossy().to_string();
    let no_memory = || ClaudeMemoryContent {
        memory_dir: memory_dir_str.clone(),
        memory_md: None,
        topic_files: Vec::new(),
    };

    // The project folder and the memory folder must be real folders. A
    // symlink at either level could point the read (and the MCP tool that
    // returns it) at any folder the user can read. A link is treated as "no
    // memory folder" and logged, so the panel stays usable.
    let project_folder = memory_dir.parent().filter(|p| !p.as_os_str().is_empty());
    for folder in project_folder.into_iter().chain(std::iter::once(memory_dir)) {
        match fs::symlink_metadata(folder) {
            Ok(meta) if meta.file_type().is_symlink() => {
                eprintln!(
                    "[automatic] not reading Claude memory: {} is a symlink",
                    folder.display()
                );
                return Ok(no_memory());
            }
            Ok(_) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(no_memory()),
            Err(e) => return Err(format!("Failed to inspect {}: {}", folder.display(), e)),
        }
    }

    let memory_md = read_claude_memory_file(&memory_dir.join("MEMORY.md"))?;

    // Topic files: every other `.md` entry.
    let entries = fs::read_dir(memory_dir)
        .map_err(|e| format!("Failed to list {}: {}", memory_dir.display(), e))?;
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(".md") && n != "MEMORY.md")
        .collect();
    names.sort();

    let mut topic_files: Vec<ClaudeMemoryTopicFile> = Vec::new();
    for name in names {
        let path = memory_dir.join(&name);
        match read_claude_memory_file(&path) {
            Ok(Some(content)) => topic_files.push(ClaudeMemoryTopicFile { name, content }),
            Ok(None) => {}
            // One unreadable topic file should not hide the others. The
            // error is logged so the gap can be diagnosed.
            Err(e) => eprintln!("[automatic] could not read Claude topic file: {}", e),
        }
    }

    Ok(ClaudeMemoryContent {
        memory_dir: memory_dir_str,
        memory_md,
        topic_files,
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // ── store / get ──────────────────────────────────────────────────────────

    #[test]
    fn store_creates_entry() {
        let dir = tempdir().unwrap();
        store_at(dir.path(), "proj", "k1", "hello", None).unwrap();

        let db = read_db_at(dir.path(), "proj").unwrap();
        let entry = db.get("k1").expect("key must exist");
        assert_eq!(entry.value, "hello");
        assert!(entry.source.is_none());
        assert!(!entry.timestamp.is_empty());
    }

    #[test]
    fn store_records_source() {
        let dir = tempdir().unwrap();
        store_at(dir.path(), "proj", "k1", "v", Some("agent-x")).unwrap();

        let db = read_db_at(dir.path(), "proj").unwrap();
        assert_eq!(db["k1"].source.as_deref(), Some("agent-x"));
    }

    #[test]
    fn store_overwrites_existing_key() {
        let dir = tempdir().unwrap();
        store_at(dir.path(), "proj", "k1", "first", None).unwrap();
        store_at(dir.path(), "proj", "k1", "second", None).unwrap();

        let db = read_db_at(dir.path(), "proj").unwrap();
        assert_eq!(db["k1"].value, "second");
        assert_eq!(db.len(), 1);
    }

    #[test]
    fn read_empty_project_returns_empty_map() {
        let dir = tempdir().unwrap();
        let db = read_db_at(dir.path(), "nonexistent").unwrap();
        assert!(db.is_empty());
    }

    #[test]
    fn store_multiple_keys_all_persisted() {
        let dir = tempdir().unwrap();
        store_at(dir.path(), "proj", "a", "1", None).unwrap();
        store_at(dir.path(), "proj", "b", "2", None).unwrap();
        store_at(dir.path(), "proj", "c", "3", None).unwrap();

        let db = read_db_at(dir.path(), "proj").unwrap();
        assert_eq!(db.len(), 3);
        assert_eq!(db["a"].value, "1");
        assert_eq!(db["b"].value, "2");
        assert_eq!(db["c"].value, "3");
    }

    // ── delete ───────────────────────────────────────────────────────────────

    #[test]
    fn delete_removes_key() {
        let dir = tempdir().unwrap();
        store_at(dir.path(), "proj", "k1", "v", None).unwrap();
        store_at(dir.path(), "proj", "k2", "v2", None).unwrap();

        delete_at(dir.path(), "proj", "k1").unwrap();

        let db = read_db_at(dir.path(), "proj").unwrap();
        assert!(!db.contains_key("k1"), "k1 should be gone");
        assert!(db.contains_key("k2"), "k2 should remain");
    }

    #[test]
    fn delete_missing_key_returns_err() {
        let dir = tempdir().unwrap();
        let err = delete_at(dir.path(), "proj", "ghost").unwrap_err();
        assert!(err.contains("not found"), "unexpected error: {}", err);
    }

    // ── clear ────────────────────────────────────────────────────────────────

    #[test]
    fn clear_all_empties_db() {
        let dir = tempdir().unwrap();
        store_at(dir.path(), "proj", "a", "1", None).unwrap();
        store_at(dir.path(), "proj", "b", "2", None).unwrap();

        let n = clear_at(dir.path(), "proj", None, true).unwrap();
        assert_eq!(n, 2);

        let db = read_db_at(dir.path(), "proj").unwrap();
        assert!(db.is_empty());
    }

    #[test]
    fn clear_with_pattern_removes_only_matching() {
        let dir = tempdir().unwrap();
        store_at(dir.path(), "proj", "foo/bar", "1", None).unwrap();
        store_at(dir.path(), "proj", "foo/baz", "2", None).unwrap();
        store_at(dir.path(), "proj", "other", "3", None).unwrap();

        let n = clear_at(dir.path(), "proj", Some("foo"), true).unwrap();
        assert_eq!(n, 2);

        let db = read_db_at(dir.path(), "proj").unwrap();
        assert!(!db.contains_key("foo/bar"));
        assert!(!db.contains_key("foo/baz"));
        assert!(db.contains_key("other"), "non-matching key must survive");
    }

    #[test]
    fn clear_pattern_case_insensitive() {
        let dir = tempdir().unwrap();
        store_at(dir.path(), "proj", "PREFIX/thing", "v", None).unwrap();

        let n = clear_at(dir.path(), "proj", Some("prefix"), true).unwrap();
        assert_eq!(n, 1);
        assert!(read_db_at(dir.path(), "proj").unwrap().is_empty());
    }

    #[test]
    fn clear_without_confirm_returns_err() {
        let dir = tempdir().unwrap();
        store_at(dir.path(), "proj", "k", "v", None).unwrap();
        let err = clear_at(dir.path(), "proj", None, false).unwrap_err();
        assert!(err.contains("not confirmed"), "unexpected error: {}", err);

        // DB must be untouched.
        assert_eq!(read_db_at(dir.path(), "proj").unwrap().len(), 1);
    }

    #[test]
    fn clear_empty_project_succeeds_with_zero_count() {
        let dir = tempdir().unwrap();
        let n = clear_at(dir.path(), "proj", None, true).unwrap();
        assert_eq!(n, 0);
    }

    // ── persistence ──────────────────────────────────────────────────────────

    #[test]
    fn data_survives_read_write_roundtrip() {
        let dir = tempdir().unwrap();
        let mut db: MemoryDb = HashMap::new();
        db.insert(
            "conventions/naming".to_string(),
            MemoryEntry {
                value: "use snake_case".to_string(),
                timestamp: current_timestamp(),
                source: Some("claude-code".to_string()),
                created_by: None,
            },
        );
        write_db_at(dir.path(), "proj", &db).unwrap();

        let loaded = read_db_at(dir.path(), "proj").unwrap();
        assert_eq!(loaded["conventions/naming"].value, "use snake_case");
        assert_eq!(
            loaded["conventions/naming"].source.as_deref(),
            Some("claude-code")
        );
    }

    // ── project isolation ────────────────────────────────────────────────────

    #[test]
    fn different_projects_are_isolated() {
        let dir = tempdir().unwrap();
        store_at(dir.path(), "project-a", "shared-key", "alpha", None).unwrap();
        store_at(dir.path(), "project-b", "shared-key", "beta", None).unwrap();

        let a = read_db_at(dir.path(), "project-a").unwrap();
        let b = read_db_at(dir.path(), "project-b").unwrap();

        assert_eq!(a["shared-key"].value, "alpha");
        assert_eq!(b["shared-key"].value, "beta");
    }

    // ── Corrupt store ────────────────────────────────────────────────────────

    #[test]
    fn corrupt_store_reads_as_error_naming_the_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("proj.json");
        fs::write(&path, "{ not json").unwrap();

        let err = read_db_at(dir.path(), "proj").unwrap_err();
        assert!(err.contains(&path.display().to_string()), "{}", err);
        assert!(err.contains("Fix the file or move it"), "{}", err);
    }

    #[test]
    fn empty_store_file_reads_as_empty() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("proj.json"), "  \n").unwrap();
        assert!(read_db_at(dir.path(), "proj").unwrap().is_empty());
    }

    #[test]
    fn every_store_call_refuses_a_corrupt_store_and_leaves_it_unchanged() {
        let home = tempdir().unwrap();
        crate::core::with_test_home(home.path().to_path_buf(), || {
            let keys = crate::core::ProjectStoreKeys::unregistered("corrupt-proj");
            let path = get_project_memory_path(&keys.id).unwrap();
            let corrupt = "{\"k\": {\"value\": \"v\", \"timestamp\": \"t\"";
            fs::write(&path, corrupt).unwrap();

            assert!(store_memory(&keys, "new", "value", None).is_err());
            assert!(get_memory(&keys, "k").is_err());
            assert!(list_memories(&keys, None).is_err());
            assert!(search_memories(&keys, "v").is_err());
            assert!(delete_memory(&keys, "k").is_err());
            assert!(clear_memories(&keys, None, true).is_err());
            assert!(get_all_memories(&keys.id).is_err());

            assert_eq!(fs::read_to_string(&path).unwrap(), corrupt);
        });
    }

    #[test]
    fn write_leaves_no_temporary_file() {
        let dir = tempdir().unwrap();
        store_at(dir.path(), "proj", "k", "v", None).unwrap();
        let names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names, vec!["proj.json".to_string()]);
    }

    // ── Claude project folder encoding ──────────────────────────────────────

    #[test]
    fn encode_claude_project_dir_replaces_slashes() {
        assert_eq!(
            encode_claude_project_dir("/Users/alice/projects/foo"),
            "-Users-alice-projects-foo"
        );
    }

    #[test]
    fn encode_claude_project_dir_replaces_every_non_alphanumeric() {
        // Folder names taken from a real ~/.claude/projects listing.
        assert_eq!(
            encode_claude_project_dir("/Users/x/aurabx/_active/aura"),
            "-Users-x-aurabx--active-aura"
        );
        assert_eq!(
            encode_claude_project_dir("/Users/x/app/.claude/worktrees/vel-395"),
            "-Users-x-app--claude-worktrees-vel-395"
        );
        assert_eq!(
            encode_claude_project_dir("/Users/x/My Project/v1.2"),
            "-Users-x-My-Project-v1-2"
        );
    }

    #[test]
    fn encode_claude_project_dir_counts_utf16_units() {
        // é is one UTF-16 unit; 🦀 is two.
        assert_eq!(encode_claude_project_dir("/a/é/🦀"), "-a-----");
        assert_eq!(encode_claude_project_dir("/🦀"), "---");
    }

    #[test]
    fn claude_memory_dir_uses_encoded_folder_under_home() {
        let home = tempdir().unwrap();
        crate::core::with_test_home(home.path().to_path_buf(), || {
            let dir = claude_memory_dir("/no/such/_active/aura").expect("home resolves");
            assert_eq!(
                dir,
                home.path()
                    .join(".claude/projects/-no-such--active-aura/memory")
            );
        });
    }

    #[test]
    fn claude_memory_dir_finds_a_hashed_long_folder() {
        let home = tempdir().unwrap();
        let project_dir = format!("/no/such/{}", "a".repeat(250));
        let encoded = encode_claude_project_dir(&project_dir);
        let hashed = format!("{}-1x2y3z", &encoded[..200]);
        let root = home.path().join(".claude/projects");
        fs::create_dir_all(root.join(&hashed)).unwrap();
        // A folder that shares the prefix without the `-` separator is not it.
        fs::create_dir_all(root.join(format!("{}a", &encoded[..200]))).unwrap();

        crate::core::with_test_home(home.path().to_path_buf(), || {
            assert_eq!(
                claude_memory_dir(&project_dir).unwrap(),
                root.join(&hashed).join("memory")
            );
        });
    }

    #[test]
    fn claude_memory_dir_refuses_an_ambiguous_long_prefix() {
        let home = tempdir().unwrap();
        let project_dir = format!("/no/such/{}", "a".repeat(250));
        let encoded = encode_claude_project_dir(&project_dir);
        let root = home.path().join(".claude/projects");
        fs::create_dir_all(root.join(format!("{}-hash1", &encoded[..200]))).unwrap();
        fs::create_dir_all(root.join(format!("{}-hash2", &encoded[..200]))).unwrap();

        assert_eq!(claude_project_folder(&root, &encoded), None);
        crate::core::with_test_home(home.path().to_path_buf(), || {
            let content = read_claude_memory(&project_dir).unwrap();
            assert!(content.memory_md.is_none());
            assert!(content.topic_files.is_empty());
        });
    }

    #[test]
    #[cfg(unix)]
    fn claude_memory_dir_prefers_the_canonical_path() {
        let home = tempdir().unwrap();
        let work = tempdir().unwrap();
        let real = work.path().join("real");
        fs::create_dir_all(&real).unwrap();
        let link = work.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let canonical = fs::canonicalize(&real).unwrap();
        let root = home.path().join(".claude/projects");
        let folder = root.join(encode_claude_project_dir(&canonical.to_string_lossy()));
        fs::create_dir_all(&folder).unwrap();

        crate::core::with_test_home(home.path().to_path_buf(), || {
            assert_eq!(
                claude_memory_dir(&link.to_string_lossy()).unwrap(),
                folder.join("memory")
            );
        });
    }

    #[test]
    fn claude_project_path_candidates_normalise_to_nfc() {
        // "e" + combining acute accent (NFD) becomes "é" (NFC).
        let candidates = claude_project_path_candidates("/no/such/caf\u{65}\u{301}");
        assert_eq!(candidates, vec!["/no/such/caf\u{e9}".to_string()]);
    }

    // ── Claude memory reader ────────────────────────────────────────────────

    #[test]
    #[cfg(unix)]
    fn claude_reader_skips_symlinks() {
        let dir = tempdir().unwrap();
        let outside = dir.path().join("secret.txt");
        fs::write(&outside, "do not read").unwrap();
        let memory = dir.path().join("memory");
        fs::create_dir_all(&memory).unwrap();
        fs::write(memory.join("real.md"), "kept").unwrap();
        std::os::unix::fs::symlink(&outside, memory.join("MEMORY.md")).unwrap();
        std::os::unix::fs::symlink(&outside, memory.join("linked.md")).unwrap();

        let content = read_claude_memory_at(&memory).unwrap();
        assert!(content.memory_md.is_none());
        let names: Vec<&str> = content.topic_files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["real.md"]);
    }

    #[test]
    #[cfg(unix)]
    fn claude_reader_refuses_a_symlinked_memory_or_project_folder() {
        let dir = tempdir().unwrap();
        let real = dir.path().join("real");
        fs::create_dir_all(real.join("memory")).unwrap();
        fs::write(real.join("memory/MEMORY.md"), "outside").unwrap();
        fs::write(real.join("memory/topic.md"), "outside").unwrap();

        // The memory folder is a link.
        let project = dir.path().join("projects/-a");
        fs::create_dir_all(&project).unwrap();
        std::os::unix::fs::symlink(real.join("memory"), project.join("memory")).unwrap();
        let content = read_claude_memory_at(&project.join("memory")).unwrap();
        assert!(content.memory_md.is_none());
        assert!(content.topic_files.is_empty());

        // The project folder is a link.
        let linked_project = dir.path().join("projects/-b");
        std::os::unix::fs::symlink(&real, &linked_project).unwrap();
        let content = read_claude_memory_at(&linked_project.join("memory")).unwrap();
        assert!(content.memory_md.is_none());
        assert!(content.topic_files.is_empty());

        // The real folder itself still reads.
        let content = read_claude_memory_at(&real.join("memory")).unwrap();
        assert_eq!(content.memory_md.as_deref(), Some("outside"));
    }

    #[test]
    #[cfg(unix)]
    fn claude_reader_skips_a_fifo() {
        let dir = tempdir().unwrap();
        let memory = dir.path().join("memory");
        fs::create_dir_all(&memory).unwrap();
        let fifo = memory.join("pipe.md");
        let status = std::process::Command::new("mkfifo").arg(&fifo).status().unwrap();
        assert!(status.success());

        // Without the regular-file check this read would block forever.
        let content = read_claude_memory_at(&memory).unwrap();
        assert!(content.topic_files.is_empty());
    }

    #[test]
    fn claude_reader_truncates_large_files() {
        let dir = tempdir().unwrap();
        let memory = dir.path().join("memory");
        fs::create_dir_all(&memory).unwrap();
        // A multi-byte character straddles the cut.
        let mut big = "a".repeat(CLAUDE_MEMORY_FILE_MAX_BYTES - 1);
        big.push('é');
        big.push_str("tail");
        fs::write(memory.join("big.md"), &big).unwrap();
        fs::write(memory.join("small.md"), "small").unwrap();

        let content = read_claude_memory_at(&memory).unwrap();
        let big_read = &content.topic_files[0];
        assert_eq!(big_read.name, "big.md");
        assert!(big_read.content.ends_with(CLAUDE_TRUNCATED_MARKER));
        let body = big_read.content.strip_suffix(CLAUDE_TRUNCATED_MARKER).unwrap();
        assert_eq!(body, "a".repeat(CLAUDE_MEMORY_FILE_MAX_BYTES - 1));
        assert_eq!(content.topic_files[1].content, "small");
    }
}
