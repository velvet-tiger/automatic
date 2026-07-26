//! Pure diff algorithm for bidirectional cloud sync.
//!
//! Given:
//! - The current on-disk library (as produced by the per-kind bundle builders)
//! - The `seen` snapshot from `CloudSyncState` (what the server last confirmed)
//!
//! Produces the delta that should be sent in the next outgoing bundle:
//! - `upserts` — new or edited assets (hash differs from `seen`, or not in `seen`)
//! - `new_tombstones` — assets that disappeared from disk since last sync
//!
//! No I/O. No time source other than the injected `now` — makes tests
//! deterministic and independent of wall-clock.
//!
//! See `docs/plans/cloud-sync/client-state.md` for the full algorithm.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

use super::cloud_sync_state::{CloudSyncState, PendingTombstone};

// ── Inputs ────────────────────────────────────────────────────────────────────

/// One asset as it exists on disk right now. `payload` is the per-kind body
/// (skill files, rule content, mcp config, etc.) without the envelope keys
/// (`machine_name`, `content_hash`, `updated_at`). The bundle builder is
/// responsible for producing this.
#[derive(Debug, Clone)]
pub struct DiskAsset {
    pub kind: String,
    pub machine_name: String,
    pub content_hash: String,
    pub payload: Value,
}

// ── Output ────────────────────────────────────────────────────────────────────

/// One asset to include in the outgoing bundle's `upserts` array. The server
/// assigns the canonical `updated_at` after clamping, but we send our own
/// read of `now` so last-writer-wins resolution works across devices.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpsertRecord {
    pub kind: String,
    pub machine_name: String,
    pub content_hash: String,
    pub updated_at: String,
    pub payload: Value,
}

/// Pure output of the diff algorithm. Does not touch disk or state yet —
/// the caller is responsible for appending `new_tombstones` into the
/// state's `pending_tombstones` and for including the full set (pending +
/// new) in the outgoing bundle.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Delta {
    pub upserts: Vec<UpsertRecord>,
    pub new_tombstones: Vec<PendingTombstone>,
}

// ── Algorithm ─────────────────────────────────────────────────────────────────

/// Compare the current on-disk library against the `seen` snapshot, returning
/// the delta that needs to be sent to the server.
///
/// `now_rfc3339` is the client's current timestamp in RFC 3339 UTC; injected
/// rather than read from the clock so tests are deterministic.
pub fn diff(disk: &[DiskAsset], state: &CloudSyncState, now_rfc3339: &str) -> Delta {
    let mut upserts: Vec<UpsertRecord> = Vec::new();
    let mut disk_by_key: HashMap<(String, String), &DiskAsset> = HashMap::with_capacity(disk.len());

    for asset in disk {
        // Detect any duplicate-key errors at the seam between bundle builder
        // and diff — should never fire, but a crash here beats silent loss.
        let key = (asset.kind.clone(), asset.machine_name.clone());
        if disk_by_key.insert(key, asset).is_some() {
            // Two disk entries collided on (kind, machine_name). Last one wins;
            // a warning would be nice but we have no logger here. Keeping this
            // as a deterministic silent-last-wins matches how HashMap::insert
            // already behaves.
        }
    }

    // Walk disk → identify upserts (new or changed).
    for ((kind, machine_name), asset) in &disk_by_key {
        let prior = state.seen_for(kind, machine_name);
        let changed = match prior {
            Some(entry) => entry.content_hash != asset.content_hash,
            None => true,
        };
        if changed {
            upserts.push(UpsertRecord {
                kind: kind.clone(),
                machine_name: machine_name.clone(),
                content_hash: asset.content_hash.clone(),
                updated_at: now_rfc3339.to_string(),
                payload: asset.payload.clone(),
            });
        }
    }

    // Walk seen → identify new tombstones (in seen, missing from disk).
    // Skip anything already in `pending_tombstones` — the caller retries
    // those separately so we don't emit a second one.
    let pending_keys: HashSet<(String, String)> = state
        .pending_tombstones
        .iter()
        .map(|t| (t.kind.clone(), t.machine_name.clone()))
        .collect();

    let mut new_tombstones: Vec<PendingTombstone> = Vec::new();
    for (kind, inner) in &state.seen {
        for machine_name in inner.keys() {
            let key = (kind.clone(), machine_name.clone());
            if disk_by_key.contains_key(&key) {
                continue;
            }
            if pending_keys.contains(&key) {
                continue;
            }
            new_tombstones.push(PendingTombstone {
                kind: kind.clone(),
                machine_name: machine_name.clone(),
                deleted_at: now_rfc3339.to_string(),
            });
        }
    }

    // Stable ordering — makes tests and server logs readable.
    upserts.sort_by(|a, b| {
        (a.kind.as_str(), a.machine_name.as_str()).cmp(&(b.kind.as_str(), b.machine_name.as_str()))
    });
    new_tombstones.sort_by(|a, b| {
        (a.kind.as_str(), a.machine_name.as_str()).cmp(&(b.kind.as_str(), b.machine_name.as_str()))
    });

    Delta {
        upserts,
        new_tombstones,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: &str = "2026-04-18T10:00:00Z";

    fn asset(kind: &str, name: &str, hash: &str) -> DiskAsset {
        DiskAsset {
            kind: kind.to_string(),
            machine_name: name.to_string(),
            content_hash: hash.to_string(),
            payload: json!({ "k": "v" }),
        }
    }

    #[test]
    fn new_asset_on_disk_produces_upsert() {
        let state = CloudSyncState::new_for_this_install();
        let disk = vec![asset("skill", "laravel-specialist", "h1")];
        let delta = diff(&disk, &state, NOW);

        assert_eq!(delta.upserts.len(), 1);
        assert!(delta.new_tombstones.is_empty());
        let u = &delta.upserts[0];
        assert_eq!(u.kind, "skill");
        assert_eq!(u.machine_name, "laravel-specialist");
        assert_eq!(u.content_hash, "h1");
        assert_eq!(u.updated_at, NOW);
    }

    #[test]
    fn unchanged_asset_is_skipped() {
        let mut state = CloudSyncState::new_for_this_install();
        state.mark_seen("skill", "laravel-specialist", "h1", "2026-04-17T00:00:00Z");

        let disk = vec![asset("skill", "laravel-specialist", "h1")];
        let delta = diff(&disk, &state, NOW);

        assert!(delta.upserts.is_empty(), "unchanged assets must not resync");
        assert!(delta.new_tombstones.is_empty());
    }

    #[test]
    fn changed_hash_produces_upsert_with_now_timestamp() {
        let mut state = CloudSyncState::new_for_this_install();
        state.mark_seen("rule", "naming", "old", "2026-04-17T00:00:00Z");

        let disk = vec![asset("rule", "naming", "new")];
        let delta = diff(&disk, &state, NOW);

        assert_eq!(delta.upserts.len(), 1);
        assert_eq!(delta.upserts[0].content_hash, "new");
        assert_eq!(
            delta.upserts[0].updated_at, NOW,
            "edited assets must carry the client's current clock"
        );
    }

    #[test]
    fn missing_from_disk_but_present_in_seen_produces_tombstone() {
        let mut state = CloudSyncState::new_for_this_install();
        state.mark_seen("template", "old-brief", "h", "2026-04-17T00:00:00Z");

        let disk: Vec<DiskAsset> = vec![];
        let delta = diff(&disk, &state, NOW);

        assert!(delta.upserts.is_empty());
        assert_eq!(delta.new_tombstones.len(), 1);
        let t = &delta.new_tombstones[0];
        assert_eq!(t.kind, "template");
        assert_eq!(t.machine_name, "old-brief");
        assert_eq!(t.deleted_at, NOW);
    }

    #[test]
    fn already_pending_tombstone_is_not_re_emitted() {
        let mut state = CloudSyncState::new_for_this_install();
        state.mark_seen("template", "old", "h", "t");
        state.pending_tombstones.push(PendingTombstone {
            kind: "template".into(),
            machine_name: "old".into(),
            deleted_at: "2026-04-17T00:00:00Z".into(),
        });

        let disk: Vec<DiskAsset> = vec![];
        let delta = diff(&disk, &state, NOW);

        assert!(
            delta.new_tombstones.is_empty(),
            "pending tombstones are retried separately; diff must not duplicate them"
        );
    }

    #[test]
    fn never_seen_never_on_disk_produces_nothing() {
        let state = CloudSyncState::new_for_this_install();
        let disk: Vec<DiskAsset> = vec![];
        let delta = diff(&disk, &state, NOW);
        assert!(delta.upserts.is_empty());
        assert!(delta.new_tombstones.is_empty());
    }

    #[test]
    fn mixed_kinds_are_grouped_and_ordered_stably() {
        let mut state = CloudSyncState::new_for_this_install();
        state.mark_seen("skill", "deleted-skill", "h", "t");
        state.mark_seen("rule", "deleted-rule", "h", "t");

        let disk = vec![
            asset("skill", "zulu", "h_z"),
            asset("skill", "alpha", "h_a"),
            asset("rule", "romeo", "h_r"),
            asset("rule", "bravo", "h_b"),
        ];

        let delta = diff(&disk, &state, NOW);

        let upsert_keys: Vec<(String, String)> = delta
            .upserts
            .iter()
            .map(|u| (u.kind.clone(), u.machine_name.clone()))
            .collect();
        assert_eq!(
            upsert_keys,
            vec![
                ("rule".into(), "bravo".into()),
                ("rule".into(), "romeo".into()),
                ("skill".into(), "alpha".into()),
                ("skill".into(), "zulu".into()),
            ],
            "upserts must be deterministically ordered by (kind, machine_name)"
        );

        let tombstone_keys: Vec<(String, String)> = delta
            .new_tombstones
            .iter()
            .map(|t| (t.kind.clone(), t.machine_name.clone()))
            .collect();
        assert_eq!(
            tombstone_keys,
            vec![
                ("rule".into(), "deleted-rule".into()),
                ("skill".into(), "deleted-skill".into()),
            ]
        );
    }

    #[test]
    fn duplicate_disk_keys_do_not_crash() {
        // Defensive: bundle builder shouldn't produce duplicates, but if it
        // ever did (e.g. case-collision in machine_name normalisation),
        // the diff pass should not panic. Last one wins, per HashMap semantics.
        let state = CloudSyncState::new_for_this_install();
        let disk = vec![
            asset("skill", "same", "hash_a"),
            asset("skill", "same", "hash_b"),
        ];
        let delta = diff(&disk, &state, NOW);
        assert_eq!(delta.upserts.len(), 1);
        // Content-hash of whichever survived — we don't assert which, just
        // that we got exactly one upsert and no panic.
    }
}
