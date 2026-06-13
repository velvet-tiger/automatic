## Cloud sync — webapp contract

**Base URL**: `https://tryautomatic.app` (prod) / `http://localhost:3000` (dev)

Bidirectional merge sync with per-device tombstones and timestamp
last-writer-wins. The webapp and desktop are in lockstep development — no
deployed clients — so this is the only contract version. `schema_version`
in the bundle exists for future compatibility, not backward compatibility.

### OAuth 2.1 PKCE (loopback)

Unchanged from v1.

| Endpoint | Method | Notes |
|---|---|---|
| `/oauth/authorize` | GET | Clerk-session-gated. Params `response_type=code`, `client_id`, `redirect_uri`, `code_challenge`, `code_challenge_method=S256`, optional `state`. |
| `/oauth/token` | POST | Grants `authorization_code` and `refresh_token`. |
| `/oauth/revoke` | POST | RFC 7009. Always 200. |

- **Access token**: HS256 JWT, `iss=tryautomatic.app`, `aud=automatic-cloud-api`, `sub=<clerk_user_id>`, 1h TTL.
- **Refresh token**: opaque random, 30d TTL, revocable.

### Device registration

Every sync is associated with a **`device_id`** — a UUIDv4 generated once per
install and persisted in the client's sync-state file. Devices are registered
implicitly on first sync and can be listed/removed by the user.

| Endpoint | Method | Notes |
|---|---|---|
| `/api/devices` | GET | `[{ device_id, display_name, os, app_version, created_at, last_sync_at }]`, sorted by `last_sync_at DESC` |
| `/api/devices` | PATCH | Rename. Body `{ device_id, display_name }` — `display_name` may be `null` or a string ≤ 128 chars. Returns the updated row. |
| `/api/devices/:device_id` | DELETE | De-register. Releases this device's hold on tombstone GC. Returns `204`. |

Sending an unknown `device_id` on `/api/library/sync` auto-registers it, using
`client.app_version`/`client.os` from the bundle. `display_name` defaults to
`"{os} device"` and can be renamed from `/api/devices`.

### Library API (bearer access token required)

| Endpoint | Method | Notes |
|---|---|---|
| `/api/me` | GET | `{ user_id, email, display_name }` |
| `/api/library` | GET | Current server state, grouped by asset type (read-only view; tombstones not included) |
| `/api/library/sync` | POST | Bidirectional delta sync — see "Bundle shape" below |
| `/api/library/assets/:kind/:machine_name` | PUT | Edit a single synced asset from the webapp — see "Webapp-side edits" below |

### Webapp-side edits

The webapp can mutate a synced asset directly (rule body, template body, sub-agent body, command body, instruction body, a single file inside a skill, or an MCP server `config` JSON). The endpoint is **Clerk-session-gated**, not bearer-token-gated — it is for the dashboard UI, not for desktop clients.

```jsonc
PUT /api/library/assets/:kind/:machine_name
{
  "content": "<new body text>",
  "expected_updated_at": "2026-04-18T09:02:13.000Z",   // optimistic-concurrency guard
  "file": "SKILL.md"                                   // required iff kind=skill, forbidden otherwise
}
```

- `expected_updated_at` must equal the row's current `updated_at`. Mismatch returns `409 conflict` with `server_updated_at` so the editor can prompt the user to reload.
- Editable kinds: `rule`, `template`, `sub_agent`, `command`, `instruction`, `skill` (one file at a time), `mcp_server` (the `config` object only — `env` is rejected at the edit boundary just like in sync).
- Creating new skill files is not supported (rejects unknown `file`); the user must create the file on the desktop and let it sync up.
- Successful writes stamp `updated_by_device = "webapp"`. Every real device will see this as "other" on its next sync and pull the change down.

Response:
```jsonc
{
  "kind": "rule",
  "machine_name": "naming",
  "content_hash": "<sha256 hex>",
  "updated_at": "2026-04-18T10:15:00.123Z",
  "updated_by_device": "webapp"
}
```

### Bundle shape

```jsonc
{
  "schema_version": 1,
  "device_id": "9a3f1e…",
  "client": { "app_version": "1.5.0", "os": "darwin" },
  "last_sync_at": "2026-04-17T10:15:00Z",   // null on very first sync for this device
  "upserts": [
    {
      "kind": "skill",
      "machine_name": "laravel-specialist",
      "content_hash": "<sha256 hex>",
      "updated_at": "2026-04-18T09:02:13Z",  // client clock — when the edit was made
      // payload fields per kind (files / content / config / skills / etc.)
      "files": { "SKILL.md": "...", "helper.py": "..." }
    }
  ],
  "tombstones": [
    { "kind": "skill", "machine_name": "obsolete-helper", "deleted_at": "2026-04-18T08:47:01Z" }
  ]
}
```

Per-kind payload fields match v1 (see the payload table below). The bundle
envelope is the part that changed.

| Kind | Payload fields (alongside `machine_name`, `content_hash`, `updated_at`) |
|---|---|
| `skill` | `files: { path -> string }`, `manifest`, `metadata` |
| `rule` | `content: string`, `metadata` |
| `template` | `content: string`, `format: string` |
| `sub_agent` | `content: string`, `config` |
| `command` | `content: string` |
| `mcp_server` | `config: { command, args, env_keys: [string] }` — **never** `env` |
| `collection` | `skills: [string]` |
| `project_template` | `content: object` |
| `instruction` | `content: string` |

- `kind` ∈ `skill | rule | template | sub_agent | command | mcp_server | collection | project_template | instruction`
- `machine_name` — `^[a-z0-9][a-z0-9._-]*$`, max 128 (unchanged)
- `content_hash` — sha256 of canonical JSON of the content body, excluding `machine_name`, `content_hash`, `updated_at`, `metadata` (unchanged algorithm from v1)
- `updated_at` / `deleted_at` — RFC 3339 UTC, millisecond precision
- **MCP secrets**: `env` rejected; only `env_keys: [string]` allowed — `400 secret_leak_detected` if `env` present (unchanged from v1)
- **Body limit**: 10 MB (unchanged)

### Sync response

```jsonc
{
  "accepted_upserts":    { "skill": ["laravel-specialist"], "rule": [] },
  "rejected_upserts":    { "skill": [
    { "machine_name": "foo", "reason": "server_newer", "server_updated_at": "..." }
  ]},
  "applied_tombstones":  { "skill": ["obsolete-helper"] },
  "rejected_tombstones": { "skill": [
    { "machine_name": "foo", "reason": "server_newer", "server_updated_at": "..." }
  ]},
  "remote_upserts": [
    // full asset records changed on the server by OTHER devices since client's last_sync_at
    { "kind": "rule", "machine_name": "naming", "content_hash": "…", "updated_at": "…", "content": "…" }
  ],
  "remote_tombstones": [
    { "kind": "template", "machine_name": "old-brief", "deleted_at": "…" }
  ],
  "server_time": "2026-04-18T10:15:00Z"
}
```

The client MUST apply `remote_upserts` and `remote_tombstones` to local
disk. Rejected client-side upserts imply the server has a newer version
that will appear in `remote_upserts` in the same response.

### Merge semantics

**Per-asset last-writer-wins by timestamp.** For each incoming record the
server compares its timestamp against `(owner_type, owner_id, kind, machine_name)`:

| Server state | Incoming | Outcome |
|---|---|---|
| nothing | upsert | insert |
| live asset (`updated_at = S`) | upsert (`updated_at = C`) | `C > S` → update; `C ≤ S` → reject `server_newer` |
| live asset (`updated_at = S`) | tombstone (`deleted_at = C`) | `C > S` → delete asset + write tombstone; `C ≤ S` → reject |
| tombstone (`deleted_at = T`) | upsert (`updated_at = C`) | `C > T` → resurrect asset + drop tombstone; `C ≤ T` → reject |
| tombstone (`deleted_at = T`) | tombstone (`deleted_at = C`) | keep `max(T, C)`, always accepted |

**Clock-skew clamp**: server clamps all incoming `updated_at` and
`deleted_at` to `min(client_ts, server_now)`. Clients with fast clocks
cannot permanently win all conflicts.

**Exact-tie rule**: if `C == S`, server wins (deterministic).

**Cutoff for `remote_*`**: the server scans for `updated_at > cutoff` /
`deleted_at > cutoff`, where `cutoff = earlier(bundle.last_sync_at,
device.last_sync_at)`. The device row's `last_sync_at` is the authoritative
floor; falling back to it covers the case where the client's local state
file was wiped or restored from an older backup. A bundle with `last_sync_at:
null` against an existing device row uses that row's value (i.e. does not
re-download everything).

**"Other devices" filter**: `remote_upserts` and `remote_tombstones` exclude
rows whose `updated_by_device` / `deleted_by_device` equals the caller's
`device_id`. Rows with a NULL device attribution are treated as "other" so
backfilled or legacy rows still reach devices that haven't seen them.

### Tombstone lifecycle & GC

- Tombstones are first-class rows: `asset_tombstones(owner_type, owner_id, kind, machine_name, deleted_at, deleted_by_device)`.
- Every sync, the server returns tombstones where `deleted_at > cutoff`
  (see "Cutoff for `remote_*`" above) so every device eventually observes
  every delete.
- **GC rule**: a tombstone is safe to hard-delete when
  `deleted_at ≤ MIN(device.last_sync_at)` across all **registered, non-stale**
  devices for that owner.
- **Stale-device policy**: devices idle for > 90 days are auto-deregistered
  by a daily job, unblocking GC. Users can also explicitly deregister via
  `DELETE /api/devices/:device_id` (from any signed-in device).

### Error envelope

All 4xx/5xx return `{ "error": "<code>", "error_description"?: "..." }`.

Codes currently raised by the live API:

- `missing_bearer_token` / `invalid_bearer_token` — bearer-token gate failures
- `invalid_request` — malformed JSON, missing required body fields
- `invalid_grant` / `unauthorized_client` / `unsupported_grant_type` — OAuth token endpoint
- `invalid_bundle` — Zod validation failure on the sync payload
- `secret_leak_detected` — an `mcp_server` upsert contained `env`; the
  caller must scrub to `env_keys: string[]` before retrying
- `payload_too_large` — body exceeded 10 MB (HTTP 413)
- `sync_failed` — uncaught server-side error during the merge pass
- `device_not_found` — `PATCH`/`DELETE /api/devices` referenced a `device_id`
  the caller doesn't own
- `conflict` — `PUT /api/library/assets/...` `expected_updated_at` did not
  match the stored row (response body also includes `server_updated_at`)
- `not_found` / `unsupported_kind` / `invalid_kind` / `invalid_machine_name`
  / `invalid_body` — `PUT /api/library/assets/...` argument validation
- `unauthenticated` — `PUT /api/library/assets/...` invoked without a Clerk
  session

`server_newer` is **not** an HTTP error. Per-asset rejections come back in
the 200 response body inside `rejected_upserts` / `rejected_tombstones`.

### Database schema

```sql
CREATE TABLE library_assets (
  owner_type        TEXT        NOT NULL,
  owner_id          TEXT        NOT NULL,
  kind              TEXT        NOT NULL,
  machine_name      TEXT        NOT NULL,
  content_hash      TEXT        NOT NULL,
  content           JSONB       NOT NULL,
  updated_at        TIMESTAMPTZ NOT NULL,
  updated_by_device TEXT,
  PRIMARY KEY (owner_type, owner_id, kind, machine_name)
);
CREATE INDEX library_assets_owner_idx         ON library_assets (owner_type, owner_id);
CREATE INDEX library_assets_owner_updated_idx ON library_assets (owner_type, owner_id, updated_at);

CREATE TABLE asset_tombstones (
  owner_type        TEXT        NOT NULL,
  owner_id          TEXT        NOT NULL,
  kind              TEXT        NOT NULL,
  machine_name      TEXT        NOT NULL,
  deleted_at        TIMESTAMPTZ NOT NULL,
  deleted_by_device TEXT,
  PRIMARY KEY (owner_type, owner_id, kind, machine_name)
);
CREATE INDEX asset_tombstones_owner_idx         ON asset_tombstones (owner_type, owner_id);
CREATE INDEX asset_tombstones_owner_deleted_idx ON asset_tombstones (owner_type, owner_id, deleted_at);

CREATE TABLE devices (
  user_id      TEXT        NOT NULL,
  device_id    TEXT        NOT NULL,
  display_name TEXT,
  os           TEXT,
  app_version  TEXT,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_sync_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (user_id, device_id)
);
CREATE INDEX devices_user_last_sync_idx ON devices (user_id, last_sync_at);

-- OAuth token storage. Refresh tokens are persisted so they can be revoked;
-- access tokens stay stateless (HS256 JWT).
CREATE TABLE oauth_refresh_tokens (
  token         TEXT PRIMARY KEY,
  clerk_user_id TEXT        NOT NULL,
  client_id     TEXT        NOT NULL,
  issued_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at    TIMESTAMPTZ NOT NULL,
  revoked_at    TIMESTAMPTZ
);

-- One-shot replay protection for authorization codes. `jti` is the JWT id
-- of the consumed code; rows are TTL'd via `expires_at` and a periodic
-- sweep (out of band of the OAuth endpoints themselves).
CREATE TABLE oauth_used_codes (
  jti        TEXT PRIMARY KEY,
  used_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at TIMESTAMPTZ NOT NULL
);
```

### First-sync behaviour

A new device has `last_sync_at = null`. The server returns all live assets
as `remote_upserts` and no tombstones. The client merges against its disk
with timestamp LWW: files with `mtime > remote.updated_at` stay local
(and get pushed up on the next sync as upserts); the rest overwrite.

### Client config baked into desktop

- `client_id = "automatic-desktop"` (unchanged)
- `redirect_uri = http://127.0.0.1:<ephemeral_port>/callback` (unchanged)
- `device_id` — UUIDv4, persisted in `~/.automatic/cloud-sync-state.json` (new — see `client-state.md`)
- Tokens in keychain under service `automatic_cloud` (unchanged)
