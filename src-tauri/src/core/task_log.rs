use serde::{Deserialize, Serialize};
use std::fs;

use super::paths::get_automatic_dir;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Maximum number of entries retained on disk. Oldest entries are pruned
/// when this limit is exceeded.
const MAX_ENTRIES: usize = 500;

// ── Types ─────────────────────────────────────────────────────────────────────

/// A single persisted task log entry. Mirrors the shape used by the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedTaskLogEntry {
    pub id: String,
    /// ISO-8601 timestamp string (serialised by the frontend before calling
    /// `append_task_log_entry`).
    pub timestamp: String,
    pub message: String,
    pub status: String,
}

// ── File path ─────────────────────────────────────────────────────────────────

fn task_log_path() -> Result<std::path::PathBuf, String> {
    Ok(get_automatic_dir()?.join("task_log.json"))
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Read all persisted entries, newest-first order is preserved as stored.
/// Returns an empty Vec if the file does not exist yet.
pub fn read_task_log() -> Result<Vec<PersistedTaskLogEntry>, String> {
    let path = task_log_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

/// Append a batch of entries (one per `log`/`update` call from the frontend),
/// then prune to the last `MAX_ENTRIES` entries and persist.
///
/// Accepts a Vec so that the initial bulk-write on first load can be done in
/// one round-trip, but callers may pass a single-element Vec for incremental
/// writes.
pub fn append_task_log_entries(new_entries: Vec<PersistedTaskLogEntry>) -> Result<(), String> {
    let path = task_log_path()?;

    // Ensure parent directory exists.
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }

    let mut entries = if path.exists() {
        let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str::<Vec<PersistedTaskLogEntry>>(&raw).unwrap_or_default()
    } else {
        Vec::new()
    };

    // Update existing entries (matching by id) or push new ones.
    for new_entry in new_entries {
        if let Some(existing) = entries.iter_mut().find(|e| e.id == new_entry.id) {
            *existing = new_entry;
        } else {
            entries.push(new_entry);
        }
    }

    // Prune to the most recent MAX_ENTRIES.
    if entries.len() > MAX_ENTRIES {
        let drop = entries.len() - MAX_ENTRIES;
        entries.drain(0..drop);
    }

    let raw = serde_json::to_string_pretty(&entries).map_err(|e| e.to_string())?;
    fs::write(&path, raw).map_err(|e| e.to_string())
}
