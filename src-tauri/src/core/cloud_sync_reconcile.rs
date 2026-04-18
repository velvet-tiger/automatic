//! Apply a server sync response to local state.
//!
//! Reconciliation is split in two:
//! 1. **State-only pass** (`apply_server_response`) — mutates `CloudSyncState`
//!    (`seen`, `pending_tombstones`, `last_sync_at`) and returns a `FileOpsPlan`
//!    describing what writes/deletes the caller must perform on disk.
//! 2. **File-ops pass** — the orchestrator executes the plan using per-kind
//!    asset writers. This module doesn't touch disk itself so the state
//!    transition is independently testable.
//!
//! Ordering rule from `docs/plans/cloud-sync/client-state.md` §"Post-sync
//! reconciliation": **apply `remote_upserts` before `remote_tombstones`** so a
//! resurrection-then-redelete sequence in the same response doesn't leave a
//! stray file on disk. The file-ops plan is emitted in that order.
//!
//! Conflict UX per user decision: same-user-different-device → silent
//! overwrite. `rejected_upserts` do not surface a UI prompt; the server's
//! newer version arrives in `remote_upserts` in the same response and
//! overwrites the local edit.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

use super::cloud_sync_diff::UpsertRecord;
use super::cloud_sync_state::CloudSyncState;

// ── Server response shape ─────────────────────────────────────────────────────

/// Matches the body of `POST /api/library/sync` as specified in
/// `docs/plans/cloud-sync/contract.md` §"Sync response".
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServerSyncResponse {
    /// `{ kind -> [machine_name] }` — upserts the server accepted.
    #[serde(default)]
    pub accepted_upserts: Value,
    /// `{ kind -> [{machine_name, reason, server_updated_at}] }`.
    #[serde(default)]
    pub rejected_upserts: Value,
    /// `{ kind -> [machine_name] }` — tombstones the server applied.
    #[serde(default)]
    pub applied_tombstones: Value,
    /// `{ kind -> [{machine_name, reason, server_updated_at}] }`.
    #[serde(default)]
    pub rejected_tombstones: Value,
    /// Full asset records changed on the server by other devices.
    #[serde(default)]
    pub remote_upserts: Vec<RemoteAssetRecord>,
    /// Tombstones this device hasn't observed yet.
    #[serde(default)]
    pub remote_tombstones: Vec<RemoteTombstoneRecord>,
    /// Authoritative RFC-3339 timestamp to store as `last_sync_at`.
    pub server_time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteAssetRecord {
    pub kind: String,
    pub machine_name: String,
    pub content_hash: String,
    pub updated_at: String,
    #[serde(flatten)]
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteTombstoneRecord {
    pub kind: String,
    pub machine_name: String,
    pub deleted_at: String,
}

// ── File ops plan ─────────────────────────────────────────────────────────────

/// A single disk operation the caller must perform after the state pass.
/// Kept abstract so this module stays I/O-free and testable without fixtures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileOp {
    /// Write/overwrite an asset on disk. `payload` is the per-kind body
    /// (shape as per the server's asset record).
    WriteAsset {
        kind: String,
        machine_name: String,
        content_hash: String,
        updated_at: String,
        payload: Value,
    },
    /// Delete an asset from disk. No-op if already absent.
    DeleteAsset {
        kind: String,
        machine_name: String,
    },
}

/// Ordered list of disk operations produced by `apply_server_response`.
/// Writes come before deletes (resurrection-before-redelete rule).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileOpsPlan {
    pub ops: Vec<FileOp>,
}

// ── Reconciliation ────────────────────────────────────────────────────────────

/// Mutate `state` based on `response`, return the disk operations required.
///
/// `sent_upserts` is the list we sent in the request, keyed for lookup so we
/// can record the right `content_hash`/`updated_at` in `seen` when the
/// server accepts. The server response only echoes machine_names for
/// accepted upserts — not the hash — so we need our own record.
pub fn apply_server_response(
    state: &mut CloudSyncState,
    sent_upserts: &[UpsertRecord],
    response: &ServerSyncResponse,
) -> FileOpsPlan {
    let mut ops: Vec<FileOp> = Vec::new();

    // 1. accepted_upserts → update `seen` from what we sent.
    for (kind, names) in iter_kind_name_map(&response.accepted_upserts) {
        for name in names {
            if let Some(record) = find_sent(sent_upserts, &kind, &name) {
                state.mark_seen(&kind, &name, &record.content_hash, &record.updated_at);
            }
            // If we can't find our own sent record, something went badly
            // wrong at the protocol level — but there's no disk corruption
            // risk in silently skipping. The next sync will re-emit.
        }
    }

    // 2. rejected_upserts → do nothing here. The server's newer version
    //    will arrive in `remote_upserts` and be handled in step 5.

    // 3. applied_tombstones → drop from `seen` and `pending_tombstones`.
    let applied: HashSet<(String, String)> =
        iter_kind_name_map(&response.applied_tombstones)
            .flat_map(|(kind, names)| names.into_iter().map(move |n| (kind.clone(), n)))
            .collect();
    for (kind, name) in &applied {
        state.forget_seen(kind, name);
    }

    // 4. rejected_tombstones → drop from `pending_tombstones`; the
    //    resurrection will be in `remote_upserts`.
    let rejected_tombstones: HashSet<(String, String)> =
        iter_kind_name_map_with_reasons(&response.rejected_tombstones)
            .flat_map(|(kind, names)| names.into_iter().map(move |n| (kind.clone(), n)))
            .collect();

    // Prune `pending_tombstones`: anything the server accepted or rejected
    // is no longer pending for us.
    state.pending_tombstones.retain(|t| {
        let key = (t.kind.clone(), t.machine_name.clone());
        !applied.contains(&key) && !rejected_tombstones.contains(&key)
    });

    // 5. remote_upserts → disk write + `seen` update. Queue disk ops in
    //    order received.
    for remote in &response.remote_upserts {
        state.mark_seen(
            &remote.kind,
            &remote.machine_name,
            &remote.content_hash,
            &remote.updated_at,
        );
        ops.push(FileOp::WriteAsset {
            kind: remote.kind.clone(),
            machine_name: remote.machine_name.clone(),
            content_hash: remote.content_hash.clone(),
            updated_at: remote.updated_at.clone(),
            payload: remote.payload.clone(),
        });
    }

    // 6. remote_tombstones → delete from disk and `seen`. Queue AFTER
    //    writes so resurrect-then-redelete in the same response leaves
    //    disk empty, not stale.
    for remote in &response.remote_tombstones {
        state.forget_seen(&remote.kind, &remote.machine_name);
        ops.push(FileOp::DeleteAsset {
            kind: remote.kind.clone(),
            machine_name: remote.machine_name.clone(),
        });
    }

    // 7. Advance the sync cursor.
    state.last_sync_at = Some(response.server_time.clone());

    FileOpsPlan { ops }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Walk a `{ kind -> [machine_name] }` JSON value tolerantly. Anything that
/// isn't an object of arrays of strings is skipped — we never want to panic
/// on an unexpected server shape.
fn iter_kind_name_map(v: &Value) -> impl Iterator<Item = (String, Vec<String>)> + '_ {
    v.as_object()
        .into_iter()
        .flat_map(|obj| obj.iter())
        .map(|(k, v)| {
            let names = v
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|s| s.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            (k.clone(), names)
        })
}

/// Walk a `{ kind -> [{machine_name, reason, ...}] }` JSON value, returning
/// just the machine_names. Tolerant of shape drift.
fn iter_kind_name_map_with_reasons(v: &Value) -> impl Iterator<Item = (String, Vec<String>)> + '_ {
    v.as_object()
        .into_iter()
        .flat_map(|obj| obj.iter())
        .map(|(k, v)| {
            let names = v
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|item| {
                            item.get("machine_name")
                                .and_then(|n| n.as_str())
                                .map(|s| s.to_string())
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            (k.clone(), names)
        })
}

fn find_sent<'a>(
    sent: &'a [UpsertRecord],
    kind: &str,
    machine_name: &str,
) -> Option<&'a UpsertRecord> {
    sent.iter()
        .find(|r| r.kind == kind && r.machine_name == machine_name)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use crate::core::cloud_sync_state::PendingTombstone;

    fn upsert(kind: &str, name: &str, hash: &str, updated_at: &str) -> UpsertRecord {
        UpsertRecord {
            kind: kind.into(),
            machine_name: name.into(),
            content_hash: hash.into(),
            updated_at: updated_at.into(),
            payload: json!({}),
        }
    }

    #[test]
    fn accepted_upsert_marks_seen_using_sent_record() {
        let mut state = CloudSyncState::new_for_this_install();
        let sent = vec![upsert("skill", "a", "h1", "2026-04-18T10:00:00Z")];
        let response = ServerSyncResponse {
            accepted_upserts: json!({ "skill": ["a"] }),
            server_time: "2026-04-18T10:15:00Z".into(),
            ..Default::default()
        };

        let plan = apply_server_response(&mut state, &sent, &response);

        let seen = state.seen_for("skill", "a").expect("marked seen");
        assert_eq!(seen.content_hash, "h1");
        assert_eq!(seen.updated_at, "2026-04-18T10:00:00Z");
        assert!(plan.ops.is_empty(), "no disk ops for accepted locals");
        assert_eq!(state.last_sync_at.as_deref(), Some("2026-04-18T10:15:00Z"));
    }

    #[test]
    fn applied_tombstone_drops_seen_and_pending() {
        let mut state = CloudSyncState::new_for_this_install();
        state.mark_seen("template", "old", "h", "t");
        state.pending_tombstones.push(PendingTombstone {
            kind: "template".into(),
            machine_name: "old".into(),
            deleted_at: "2026-04-18T09:00:00Z".into(),
        });

        let response = ServerSyncResponse {
            applied_tombstones: json!({ "template": ["old"] }),
            server_time: "2026-04-18T10:15:00Z".into(),
            ..Default::default()
        };

        apply_server_response(&mut state, &[], &response);

        assert!(state.seen_for("template", "old").is_none());
        assert!(
            state.pending_tombstones.is_empty(),
            "applied tombstone must be drained from pending"
        );
    }

    #[test]
    fn rejected_tombstone_is_dropped_from_pending_without_touching_seen() {
        let mut state = CloudSyncState::new_for_this_install();
        state.mark_seen("skill", "resurrected", "old_h", "t");
        state.pending_tombstones.push(PendingTombstone {
            kind: "skill".into(),
            machine_name: "resurrected".into(),
            deleted_at: "2026-04-18T09:00:00Z".into(),
        });

        let response = ServerSyncResponse {
            rejected_tombstones: json!({
                "skill": [{ "machine_name": "resurrected", "reason": "server_newer", "server_updated_at": "..." }]
            }),
            // The resurrection arrives here:
            remote_upserts: vec![RemoteAssetRecord {
                kind: "skill".into(),
                machine_name: "resurrected".into(),
                content_hash: "new_h".into(),
                updated_at: "2026-04-18T09:30:00Z".into(),
                payload: json!({ "files": {} }),
            }],
            server_time: "2026-04-18T10:15:00Z".into(),
            ..Default::default()
        };

        let plan = apply_server_response(&mut state, &[], &response);

        // Tombstone cleared.
        assert!(state.pending_tombstones.is_empty());
        // Seen updated to the resurrected version.
        let seen = state.seen_for("skill", "resurrected").unwrap();
        assert_eq!(seen.content_hash, "new_h");
        // File write queued.
        assert_eq!(plan.ops.len(), 1);
        matches!(&plan.ops[0], FileOp::WriteAsset { kind, machine_name, .. }
                 if kind == "skill" && machine_name == "resurrected");
    }

    #[test]
    fn remote_upserts_queue_writes_before_deletes() {
        let mut state = CloudSyncState::new_for_this_install();
        let response = ServerSyncResponse {
            remote_upserts: vec![RemoteAssetRecord {
                kind: "rule".into(),
                machine_name: "new-rule".into(),
                content_hash: "h".into(),
                updated_at: "t".into(),
                payload: json!({ "content": "..." }),
            }],
            remote_tombstones: vec![RemoteTombstoneRecord {
                kind: "rule".into(),
                machine_name: "old-rule".into(),
                deleted_at: "t".into(),
            }],
            server_time: "s".into(),
            ..Default::default()
        };

        let plan = apply_server_response(&mut state, &[], &response);

        assert_eq!(plan.ops.len(), 2);
        assert!(
            matches!(&plan.ops[0], FileOp::WriteAsset { .. }),
            "writes must come before deletes (resurrect-before-redelete rule)"
        );
        assert!(matches!(&plan.ops[1], FileOp::DeleteAsset { .. }));
    }

    #[test]
    fn resurrect_then_redelete_in_same_response_leaves_disk_empty() {
        // Server might report both a remote_upsert and remote_tombstone for
        // the same asset if we're catching up from a long offline period.
        // Ordering guarantees the delete wins on disk.
        let mut state = CloudSyncState::new_for_this_install();
        let response = ServerSyncResponse {
            remote_upserts: vec![RemoteAssetRecord {
                kind: "skill".into(),
                machine_name: "flapper".into(),
                content_hash: "h1".into(),
                updated_at: "2026-04-18T09:00:00Z".into(),
                payload: json!({}),
            }],
            remote_tombstones: vec![RemoteTombstoneRecord {
                kind: "skill".into(),
                machine_name: "flapper".into(),
                deleted_at: "2026-04-18T09:30:00Z".into(),
            }],
            server_time: "2026-04-18T10:15:00Z".into(),
            ..Default::default()
        };

        let plan = apply_server_response(&mut state, &[], &response);

        assert!(
            state.seen_for("skill", "flapper").is_none(),
            "seen should be cleared by the later tombstone"
        );
        assert_eq!(plan.ops.len(), 2);
        // The delete is ordered AFTER the write so disk ends up empty.
        let last = plan.ops.last().unwrap();
        assert!(matches!(last, FileOp::DeleteAsset { machine_name, .. } if machine_name == "flapper"));
    }

    #[test]
    fn remote_tombstone_without_prior_seen_is_harmless() {
        let mut state = CloudSyncState::new_for_this_install();
        let response = ServerSyncResponse {
            remote_tombstones: vec![RemoteTombstoneRecord {
                kind: "rule".into(),
                machine_name: "never-had-it".into(),
                deleted_at: "t".into(),
            }],
            server_time: "s".into(),
            ..Default::default()
        };

        let plan = apply_server_response(&mut state, &[], &response);

        // Still queues the delete — disk op is idempotent (no-op if missing).
        assert_eq!(plan.ops.len(), 1);
        assert!(matches!(&plan.ops[0], FileOp::DeleteAsset { .. }));
    }

    #[test]
    fn server_time_replaces_last_sync_at_even_with_empty_response() {
        let mut state = CloudSyncState::new_for_this_install();
        state.last_sync_at = Some("old".into());

        let response = ServerSyncResponse {
            server_time: "2026-04-18T10:15:00Z".into(),
            ..Default::default()
        };
        apply_server_response(&mut state, &[], &response);

        assert_eq!(state.last_sync_at.as_deref(), Some("2026-04-18T10:15:00Z"));
    }

    #[test]
    fn unexpected_response_shape_does_not_panic() {
        // Server sends garbage in accepted_upserts (not an object of arrays).
        let mut state = CloudSyncState::new_for_this_install();
        let response = ServerSyncResponse {
            accepted_upserts: json!("oops"),
            applied_tombstones: json!(42),
            rejected_tombstones: json!(null),
            server_time: "s".into(),
            ..Default::default()
        };
        // Must not panic.
        let plan = apply_server_response(&mut state, &[], &response);
        assert!(plan.ops.is_empty());
    }
}
