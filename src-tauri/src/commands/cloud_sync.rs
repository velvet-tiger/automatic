// ── Cloud Sync Commands ──────────────────────────────────────────────────────
//
// Tauri command bindings for the desktop ↔ webapp library sync flow.
// See `crate::core::cloud_sync` for the underlying delta-sync implementation.
//
// Gated behind the `cloud_sync` feature flag (see src-tauri/src/core/flags.rs).
// When the flag is off the commands are still registered — so the frontend's
// `invoke()` call does not throw "command not found" — but they short-circuit
// so a production build without the flag cannot upload library data.
//
// The command *names* stay stable across the v1→v2 transition so the frontend
// doesn't have to branch; only the return shapes have changed.

use crate::core::cloud_sync::{SyncPreview, SyncSummary};

const FLAG: &str = "cloud_sync";

fn flag_enabled() -> bool {
    crate::core::get_feature_flags().contains_key(FLAG)
}

fn disabled_error() -> String {
    "Cloud sync is disabled in this build.".to_string()
}

/// Compute what the next sync would send — counts of upserts and tombstones
/// broken down by kind — without making any network calls or mutating state.
/// Used by the UI for a pre-flight preview.
#[tauri::command]
pub fn cloud_build_bundle() -> Result<SyncPreview, String> {
    if !flag_enabled() {
        return Err(disabled_error());
    }
    crate::core::cloud_sync::preview_sync()
}

/// Run a full bidirectional sync: diff local state against the server's last
/// `seen` snapshot, POST the delta, apply the server's merged response, and
/// persist the updated client state. Returns per-kind accept/reject counts
/// plus remote change totals.
#[tauri::command]
pub async fn cloud_sync_library() -> Result<SyncSummary, String> {
    if !flag_enabled() {
        return Err(disabled_error());
    }
    crate::core::cloud_sync::run_sync().await
}
