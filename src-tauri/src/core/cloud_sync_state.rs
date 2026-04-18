//! Persistent client state for bidirectional cloud sync.
//!
//! See `docs/plans/cloud-sync/client-state.md` for the full design.
//!
//! This module is responsible for:
//! - Generating and persisting a stable `device_id` (UUIDv4) per install
//! - Recording what the client has last confirmed with the server (`seen`)
//! - Tracking deletions the client has observed locally but not yet synced
//!   (`pending_tombstones`)
//!
//! The state file lives at `{automatic_dir}/cloud-sync-state.json`.
//! Writes are atomic: write to a `.tmp` sibling, fsync, rename. This
//! guarantees the file is either the old content or the new content — never
//! a partial write — even if the process is killed mid-sync.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use super::paths::get_automatic_dir;

// ── State-file schema ─────────────────────────────────────────────────────────

/// State-file schema version. Bumped only for on-disk format changes. The
/// bundle `schema_version` in `contract.md` is a separate namespace.
pub const STATE_SCHEMA_VERSION: u32 = 1;

/// What the client last confirmed with the server for a single asset. Used
/// by the diff algorithm in `cloud_sync_diff` to distinguish "user deleted
/// X" from "this device never had X".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeenEntry {
    pub content_hash: String,
    pub updated_at: String,
}

/// A delete detected locally but not yet accepted by the server. Retained
/// across failed syncs and retried until the server accepts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingTombstone {
    pub kind: String,
    pub machine_name: String,
    pub deleted_at: String,
}

/// The top-level state file. Field order matches `client-state.md`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudSyncState {
    pub version: u32,
    pub device_id: String,
    pub display_name: String,
    pub os: String,
    pub app_version: String,
    #[serde(default)]
    pub last_sync_at: Option<String>,
    #[serde(default)]
    pub seen: HashMap<String, HashMap<String, SeenEntry>>,
    #[serde(default)]
    pub pending_tombstones: Vec<PendingTombstone>,
}

impl CloudSyncState {
    /// Construct a fresh state for a brand-new install. The `device_id` is
    /// a newly-minted UUIDv4 and will never change for this install.
    pub fn new_for_this_install() -> Self {
        Self {
            version: STATE_SCHEMA_VERSION,
            device_id: uuid::Uuid::new_v4().to_string(),
            display_name: default_display_name(),
            os: std::env::consts::OS.to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            last_sync_at: None,
            seen: HashMap::new(),
            pending_tombstones: Vec::new(),
        }
    }
}

// ── File I/O ──────────────────────────────────────────────────────────────────

/// Path to the state file. The parent directory is created on first write.
pub fn state_file_path() -> Result<PathBuf, String> {
    Ok(get_automatic_dir()?.join("cloud-sync-state.json"))
}

/// Load the state file, creating a fresh one (with a new `device_id`) if
/// it doesn't exist or is unreadable/corrupt.
///
/// A corrupted file is treated the same as a missing file — the caller's
/// diff pass will see an empty `seen` map and send every on-disk asset as
/// an upsert with no tombstones. This is the safe failure mode: no data
/// loss, just a noisier first sync.
pub fn load_or_init() -> Result<CloudSyncState, String> {
    let path = state_file_path()?;
    match fs::read_to_string(&path) {
        Ok(raw) => match serde_json::from_str::<CloudSyncState>(&raw) {
            Ok(state) => Ok(state),
            Err(e) => {
                eprintln!(
                    "cloud_sync_state: ignoring corrupt state file at {}: {}",
                    path.display(),
                    e
                );
                Ok(CloudSyncState::new_for_this_install())
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok(CloudSyncState::new_for_this_install())
        }
        Err(e) => Err(format!(
            "failed to read state file {}: {}",
            path.display(),
            e
        )),
    }
}

/// Atomically persist the state file. Guarantees:
/// 1. The file ends up either fully old or fully new content
/// 2. Contents are fsynced before the rename
/// 3. The parent directory exists
///
/// We do **not** fsync the parent directory. On modern filesystems (APFS,
/// ext4, NTFS) the rename is durable once the tmp file's data reaches
/// stable storage; an extra parent-dir fsync would double the cost for
/// essentially no real-world benefit on user machines.
pub fn save(state: &CloudSyncState) -> Result<(), String> {
    let path = state_file_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create state dir: {}", e))?;
    }
    write_atomic(&path, state)
}

fn write_atomic(final_path: &Path, state: &CloudSyncState) -> Result<(), String> {
    let tmp_path = tmp_path_for(final_path);

    // Remove a stale tmp from a previous crash, if any.
    let _ = fs::remove_file(&tmp_path);

    {
        let mut tmp = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
            .map_err(|e| format!("failed to open tmp state file: {}", e))?;

        let body = serde_json::to_vec_pretty(state)
            .map_err(|e| format!("failed to serialise state: {}", e))?;

        tmp.write_all(&body)
            .map_err(|e| format!("failed to write tmp state: {}", e))?;
        tmp.sync_all()
            .map_err(|e| format!("failed to fsync tmp state: {}", e))?;
    } // file handle closed before rename

    fs::rename(&tmp_path, final_path).map_err(|e| {
        // Best-effort cleanup so a failed rename doesn't leave litter.
        let _ = fs::remove_file(&tmp_path);
        format!("failed to rename state file into place: {}", e)
    })
}

fn tmp_path_for(final_path: &Path) -> PathBuf {
    let mut s = final_path.as_os_str().to_os_string();
    s.push(".tmp");
    PathBuf::from(s)
}

// ── Defaults ──────────────────────────────────────────────────────────────────

fn default_display_name() -> String {
    // gethostname isn't a dep and adding one just for this is overkill.
    // Fall back to a readable OS-based label; the user can rename the device
    // from Settings once the UI lands.
    let os = std::env::consts::OS;
    match os {
        "macos" => "macOS device".to_string(),
        "linux" => "Linux device".to_string(),
        "windows" => "Windows device".to_string(),
        other => format!("{} device", other),
    }
}

// ── Ergonomics ────────────────────────────────────────────────────────────────

impl CloudSyncState {
    /// Look up what the client last confirmed with the server for an asset.
    pub fn seen_for<'a>(
        &'a self,
        kind: &str,
        machine_name: &str,
    ) -> Option<&'a SeenEntry> {
        self.seen.get(kind).and_then(|inner| inner.get(machine_name))
    }

    /// Record a freshly-accepted asset into `seen`, replacing any prior entry.
    pub fn mark_seen(
        &mut self,
        kind: &str,
        machine_name: &str,
        content_hash: &str,
        updated_at: &str,
    ) {
        self.seen
            .entry(kind.to_string())
            .or_default()
            .insert(
                machine_name.to_string(),
                SeenEntry {
                    content_hash: content_hash.to_string(),
                    updated_at: updated_at.to_string(),
                },
            );
    }

    /// Remove an entry from `seen` — either because the asset was tombstoned
    /// and the server confirmed the delete, or because a `remote_tombstone`
    /// instructs this device to drop it.
    pub fn forget_seen(&mut self, kind: &str, machine_name: &str) {
        if let Some(inner) = self.seen.get_mut(kind) {
            inner.remove(machine_name);
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::paths::with_test_home;
    use tempfile::TempDir;

    fn use_temp_home<T>(test: impl FnOnce() -> T) -> T {
        let tmp = TempDir::new().expect("tempdir");
        with_test_home(tmp.path().to_path_buf(), test)
    }

    #[test]
    fn load_on_missing_file_returns_fresh_state_with_new_device_id() {
        use_temp_home(|| {
            let state = load_or_init().expect("load");
            assert_eq!(state.version, STATE_SCHEMA_VERSION);
            assert_eq!(state.device_id.len(), 36, "UUID hyphenated form");
            assert!(state.last_sync_at.is_none());
            assert!(state.seen.is_empty());
            assert!(state.pending_tombstones.is_empty());
        });
    }

    #[test]
    fn save_then_load_preserves_device_id_and_seen() {
        use_temp_home(|| {
            let mut state = load_or_init().expect("init");
            let original_id = state.device_id.clone();
            state.mark_seen("skill", "laravel-specialist", "abc123", "2026-04-18T10:00:00Z");
            state.last_sync_at = Some("2026-04-18T10:15:00Z".to_string());
            save(&state).expect("save");

            let loaded = load_or_init().expect("reload");
            assert_eq!(loaded.device_id, original_id);
            assert_eq!(loaded.last_sync_at.as_deref(), Some("2026-04-18T10:15:00Z"));
            let seen = loaded
                .seen_for("skill", "laravel-specialist")
                .expect("entry present");
            assert_eq!(seen.content_hash, "abc123");
            assert_eq!(seen.updated_at, "2026-04-18T10:00:00Z");
        });
    }

    #[test]
    fn corrupt_state_file_is_replaced_with_fresh_defaults() {
        use_temp_home(|| {
            // Seed a garbage state file.
            let path = state_file_path().unwrap();
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, b"{ not valid json").unwrap();

            // load_or_init should swallow the error and mint fresh state.
            let state = load_or_init().expect("load despite corruption");
            assert_eq!(state.version, STATE_SCHEMA_VERSION);
            assert_eq!(state.device_id.len(), 36);
        });
    }

    #[test]
    fn atomic_write_leaves_no_tmp_file_on_success() {
        use_temp_home(|| {
            let state = CloudSyncState::new_for_this_install();
            save(&state).expect("save");

            let final_path = state_file_path().unwrap();
            let tmp = tmp_path_for(&final_path);
            assert!(final_path.exists(), "final state file must exist");
            assert!(
                !tmp.exists(),
                ".tmp sibling must not linger after successful save"
            );
        });
    }

    #[test]
    fn stale_tmp_from_prior_crash_is_reclaimed_on_next_save() {
        use_temp_home(|| {
            // Simulate a prior crashed save leaving a .tmp behind.
            let final_path = state_file_path().unwrap();
            fs::create_dir_all(final_path.parent().unwrap()).unwrap();
            let tmp = tmp_path_for(&final_path);
            fs::write(&tmp, b"garbage from last crash").unwrap();
            assert!(tmp.exists());

            let state = CloudSyncState::new_for_this_install();
            save(&state).expect("save must succeed despite stale tmp");

            assert!(!tmp.exists(), "stale .tmp must be reclaimed");
            assert!(final_path.exists());
        });
    }

    #[test]
    fn forget_seen_removes_entry_without_touching_other_kinds() {
        let mut state = CloudSyncState::new_for_this_install();
        state.mark_seen("skill", "a", "h1", "t1");
        state.mark_seen("skill", "b", "h2", "t2");
        state.mark_seen("rule", "a", "h3", "t3");

        state.forget_seen("skill", "a");

        assert!(state.seen_for("skill", "a").is_none());
        assert!(state.seen_for("skill", "b").is_some());
        assert!(
            state.seen_for("rule", "a").is_some(),
            "other kinds must be untouched"
        );
    }

    #[test]
    fn mark_seen_overwrites_existing_entry() {
        let mut state = CloudSyncState::new_for_this_install();
        state.mark_seen("skill", "x", "old-hash", "2026-04-17T00:00:00Z");
        state.mark_seen("skill", "x", "new-hash", "2026-04-18T00:00:00Z");

        let entry = state.seen_for("skill", "x").unwrap();
        assert_eq!(entry.content_hash, "new-hash");
        assert_eq!(entry.updated_at, "2026-04-18T00:00:00Z");
    }
}
