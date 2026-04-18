## Cloud sync — desktop client state & diff algorithm

Paired with `./contract.md`. Covers what the desktop keeps on disk to make
bidirectional merge sync correct across multiple signed-in devices.

### State file

**Path**: `{automatic_dir}/cloud-sync-state.json` — `~/.automatic-dev/` in
debug builds, `~/.automatic/` in release, resolved via
[`get_automatic_dir()`](../../../src-tauri/src/core/paths.rs).

Single JSON file, authoritative record of what this device has confirmed
with the server. Rewritten atomically (tmp + fsync + rename) after every
successful sync.

```jsonc
{
  "version": 1,                              // state-file schema, independent of bundle schema_version
  "device_id": "9a3f1e…",                    // UUIDv4, generated once, never changes
  "display_name": "xtfer MacBook Pro",
  "os": "darwin",
  "app_version": "1.5.0",
  "last_sync_at": "2026-04-18T10:15:00Z",    // from server.server_time of the last successful sync
  "seen": {
    "skill":     { "laravel-specialist": { "content_hash": "abc…", "updated_at": "…" } },
    "rule":      { "…": { "content_hash": "…", "updated_at": "…" } },
    "template":  { "…": {...} },
    "sub_agent": { "…": {...} },
    "command":   { "…": {...} },
    "mcp_server":{ "…": {...} },
    "collection":{ "…": {...} },
    "project_template": { "…": {...} },
    "instruction":      { "…": {...} }
  },
  "pending_tombstones": [
    { "kind": "skill", "machine_name": "obsolete-helper", "deleted_at": "2026-04-18T08:47:01Z" }
  ]
}
```

- **`seen`** — the view of the library this device has most recently
  confirmed with the server. Updated from accepted upserts + remote upserts
  in every sync response. This is what distinguishes "user deleted X" from
  "this device never had X".
- **`pending_tombstones`** — deletes detected locally that haven't been
  accepted by the server yet. Retained across failed syncs and retried
  until accepted.

### Diff algorithm — building the outgoing bundle

Run at the start of each sync:

1. Enumerate current on-disk assets per kind (reuse existing readers —
   `core/skills.rs`, `core/rules.rs`, etc.). Hash each body with the same
   canonical-JSON sha256 used by v1.
2. For each `(kind, machine_name)`, compare `disk` vs `seen`:
   - In `disk`, not in `seen` → **upsert**, `updated_at = now()`. (New asset.)
   - In `disk`, in `seen`, `hash` changed → **upsert**, `updated_at = now()`. (Edited.)
   - In `disk`, in `seen`, `hash` same → **skip**.
   - Not in `disk`, in `seen` → **tombstone**, `deleted_at = now()`. Append
     to `pending_tombstones`.
3. Include every entry from `pending_tombstones` in the outgoing bundle —
   the last sync may have failed after sending but before persisting, and
   the server dedupes via timestamp LWW.

### Post-sync reconciliation

When the server responds 200:

1. **`accepted_upserts`** → for each entry update `seen[kind][machine_name]`
   to the `content_hash`+`updated_at` we sent. No disk change (content is
   already on disk).
2. **`rejected_upserts`** (`reason: server_newer`) → the corresponding asset
   will also appear in `remote_upserts`. Let step 5 handle it. Silent
   overwrite — the remote edit came from the same user on another device,
   so their more recent version wins without prompting.
3. **`applied_tombstones`** → remove matching entries from
   `pending_tombstones` and from `seen`.
4. **`rejected_tombstones`** → asset was resurrected on another device. The
   resurrection will be in `remote_upserts`; step 5 writes it to disk. Drop
   the matching entry from `pending_tombstones`.
5. **`remote_upserts`** → for each, write content to disk and update `seen`.
   - If the asset already exists on disk with a different hash: resolve by
     timestamp LWW using the file's `mtime` vs `remote.updated_at`. If local
     is newer, leave the file and emit an upsert on the next sync (treat
     the local as a pending edit). Otherwise overwrite.
6. **`remote_tombstones`** → delete matching files/directories from disk and
   remove from `seen`. Never delete without a matching `seen` entry (if we
   never had it, there's nothing to do).
7. Set `last_sync_at = response.server_time`. Write the state file
   atomically (tmp + fsync + rename).

Ordering note: apply `remote_tombstones` **after** `remote_upserts` so a
resurrection-then-redelete sequence in the same response doesn't leave a
stray file on disk.

### File deletion detection — edge cases

- **State file missing** — treated as a fresh install. Send everything as
  upserts, no tombstones. Same code path as a first sync on a new device.
- **State file corrupt** — same as missing. Log a warning. Worst case: a
  noisy first sync with zero tombstones, which is safe (no data loss).
- **User manually edits state file** — unsupported but not destructive.
  Recovery: delete the file.
- **Time Machine restore** of a previously-deleted asset — diff sees it in
  `disk`, not in `seen` → treated as a new upsert with `updated_at = now()`.
  Against the server's still-present tombstone, LWW resolves the
  resurrection: `now > tombstone.deleted_at` → server accepts.
- **Mass local deletion** (e.g., user `rm -rf ~/.agents/skills`) — diff
  would emit many tombstones. **The UI should prompt** when
  `len(tombstones) > threshold` (suggest 10) with a "Delete N assets from
  cloud?" confirmation before sending the bundle.

### Atomic state file writes

```
1. write {automatic_dir}/cloud-sync-state.json.tmp
2. fsync the tmp file
3. rename(.tmp → final)   // atomic on POSIX
```

If sync fails after the HTTP 200 but before the rename, the next sync
re-observes the same deltas. The server deduplicates via timestamp LWW —
identical `updated_at`/`deleted_at` arrive again and are rejected as
`server_newer` (the server already has that version), so re-sending is a
no-op.

### Device lifecycle

- **First run** — generate `device_id` (UUIDv4), default `display_name =
  hostname`, write an empty state file.
- **User renames device** in Settings — local update + `PATCH /api/devices/:id`.
- **Sign out** — **do not** delete the state file. If the user signs back
  in as the same Clerk user, `device_id` still matches and tombstone GC
  continues working. If they sign in as a different user, the server
  treats this device as new for that user.
- **"Remove this device" in Settings** — call `DELETE /api/devices/:id`,
  then delete the state file. A fresh `device_id` will be generated on the
  next sign-in.

### Open questions

- **Binary files in skills** — v1 rejects non-text resources. v2 inherits
  that restriction for now; object-storage support is a separate plan.
- **Background sync / push** — still deferred. Merge semantics are the
  same; only the trigger changes.
