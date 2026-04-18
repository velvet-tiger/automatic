# Plan: Cloud Library Sync (Up-first, Bidirectional Later)

see also ./docs/plans/cloud-sync/contract.md

## Context

Automatic currently stores a user's library (nine asset types — skills, instructions, rules, templates, sub-agents, commands, MCP servers, collections, project templates) on disk in `~/.automatic/` and `~/.agents/skills/`. There is no server-side copy. The product goal is to give signed-in users a cloud mirror of their library at `tryautomatic.app` so they can see it online and, later, share libraries with teammates via Clerk Organizations.

This plan delivers **v1: up-only sync** (desktop → cloud) full-stack, covering both `automatic-app` (Rust desktop client) and `automatic-webapp` (Next.js + Clerk). The webapp currently has no API routes and no database, so both are added here. The design keeps the door open for v2 (down-sync) and v3 (team libraries) without breaking changes.

**Key decisions** (confirmed with the user):
- Full-stack in this plan (both repos).
- Auth via Clerk-gated OAuth 2.1 PKCE loopback — reuses the pattern in `oauth.rs`.
- Manual "Sync to cloud" button only (no background sync in v1).
- Upload everything in the library regardless of provenance.
- **Secrets are stripped**: MCP server env *values* never leave the machine; only env *keys* are uploaded.

## Architecture Overview

```
Desktop (Rust/Tauri)                    Cloud (Next.js on webapp)
──────────────────────                  ────────────────────────────
1. Login flow (PKCE)       ──────►      /oauth/authorize (Clerk-gated)
                           ◄──────      authorization_code
2. Token exchange          ──────►      /oauth/token
                           ◄──────      access_token + refresh_token
3. Build LibraryBundle     ──────►      POST /api/library/sync
   (full snapshot, JSON)   ◄──────      { added, updated, deleted, unchanged }
4. Store tokens in
   platform keychain
```

Cloud stores one row per asset keyed by `(owner_type, owner_id, asset_type, machine_name)`. On each sync the server computes the diff vs its stored state and applies it; the desktop sends a full snapshot and gets a summary back. This avoids needing a local "last-synced manifest" on the desktop — the cloud is the diff authority.

---

## v1 Scope — What Ships

### Desktop (`automatic-app`)

1. **New module `core/cloud_sync/`** (sibling of `core/remote_sources.rs`).
   - `auth.rs` — PKCE flow, token storage/refresh via keyring
   - `bundle.rs` — build a `LibraryBundle` from the on-disk library
   - `client.rs` — authenticated HTTP client, retry/backoff
   - `secrets.rs` — strip MCP env values before upload
2. **New Tauri commands** in `commands/cloud_sync.rs`:
   - `cloud_login()` → opens browser, completes PKCE, returns user profile
   - `cloud_logout()` → revokes refresh token, clears keychain entries
   - `cloud_whoami()` → returns cached profile or `None`
   - `cloud_sync_library()` → builds bundle, POSTs, returns sync summary
3. **Settings UI** — a Cloud section with sign-in state, "Sync now" button, last-synced timestamp, and a summary modal showing what changed.
4. **Keychain entries** (via existing `keyring` crate):
   - `automatic_cloud_access_token`
   - `automatic_cloud_refresh_token`
   - `automatic_cloud_user_profile` (JSON: `{ user_id, email, display_name }`)

### Webapp (`automatic-webapp`)

1. **Database** — add Postgres (Neon via `@neondatabase/serverless`) + `drizzle-orm` + `drizzle-kit` for migrations. Two tables (see schema below).
2. **OAuth endpoints** (session-gated via Clerk):
   - `GET /oauth/authorize` — if signed out, redirect to Clerk sign-in; if signed in, show consent page; on consent, issue one-shot `authorization_code` and redirect to `redirect_uri` (loopback).
   - `POST /oauth/token` — exchange `authorization_code` + `code_verifier` for an access/refresh pair. Access token is a short-lived JWT signed by the webapp (claim: `sub = clerk_user_id`). Refresh token is an opaque random string persisted server-side for revocation.
   - `POST /oauth/revoke` — invalidate a refresh token.
3. **Library API** (protected — requires valid access token):
   - `GET /api/me` — returns `{ user_id, email, display_name }` for client identity probe.
   - `POST /api/library/sync` — accepts a `LibraryBundle`, computes diff, applies, returns summary. See contract below.
   - `GET /api/library` — returns current stored library (read-only placeholder for v2 down-sync; v1 UI uses it for "what's in the cloud" view).
4. **Dashboard page** `/dashboard/library` — lists cloud-stored assets grouped by type. Read-only in v1.

---

## Data Model

### Database schema (Postgres, Drizzle)

```ts
// src/db/schema.ts (webapp)
export const libraryAssets = pgTable("library_assets", {
  id: uuid("id").primaryKey().defaultRandom(),
  ownerType: text("owner_type").notNull(),      // 'user' | 'organization' (v3)
  ownerId: text("owner_id").notNull(),          // clerk user_id or org_id
  assetType: text("asset_type").notNull(),      // one of the nine types
  machineName: text("machine_name").notNull(),  // stable slug
  content: jsonb("content").notNull(),          // shape per type (see below)
  contentHash: text("content_hash").notNull(),  // sha256 of canonical JSON
  metadata: jsonb("metadata"),                  // provenance, displayName, etc.
  createdAt: timestamp("created_at", { withTimezone: true }).defaultNow().notNull(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).defaultNow().notNull(),
}, (t) => ({
  uniq: uniqueIndex("library_assets_owner_type_name_unique")
        .on(t.ownerType, t.ownerId, t.assetType, t.machineName),
}));

export const oauthRefreshTokens = pgTable("oauth_refresh_tokens", {
  token: text("token").primaryKey(),            // opaque random
  clerkUserId: text("clerk_user_id").notNull(),
  issuedAt: timestamp("issued_at", { withTimezone: true }).defaultNow().notNull(),
  expiresAt: timestamp("expires_at", { withTimezone: true }).notNull(),
  revokedAt: timestamp("revoked_at", { withTimezone: true }),
});
```

Short-lived authorization codes and access tokens are kept stateless — JWTs signed with a webapp secret and a 10-min / 60-min TTL respectively. Refresh tokens are persisted so they can be revoked.

### `LibraryBundle` payload (shared contract)

Reuses the shape of `AutomaticManifest` from [remote_sources.rs:15](../../../src-tauri/src/core/remote_sources.rs) but inlines content. Rust `LibraryBundle` lives in `core/cloud_sync/bundle.rs`; a matching TypeScript Zod schema lives in `automatic-webapp/src/api/schema.ts`.

```jsonc
{
  "schema_version": 1,
  "client": { "app_version": "1.4.4", "os": "darwin" },
  "assets": {
    "skill":     [{ "machine_name": "laravel-specialist", "files": { "SKILL.md": "...", "helper.py": "..." }, "manifest": {...}, "metadata": {...} }],
    "rule":      [{ "machine_name": "automatic-testing",  "content": "<markdown>", "metadata": {...} }],
    "template":  [...],
    "sub_agent": [...],
    "command":   [...],
    "mcp_server":[{ "machine_name": "brandfetch", "config": { "command":"npx", "args":[...], "env_keys":["API_KEY"] }, "metadata": {...} }],
    "collection":[{ "machine_name": "automatic-skills", "skills": ["foo","bar"] }],
    "project_template":[...],
    "instruction":[...]
  }
}
```

### Sync response

```jsonc
{
  "added":     { "skill": ["foo"], "rule": [] },
  "updated":   { "skill": ["bar"] },
  "deleted":   { "template": ["old-template"] },
  "unchanged_count": 42,
  "server_time": "2026-04-17T10:15:00Z"
}
```

---

## Critical Implementation Details

### Desktop — building the bundle
- Reuse existing readers: [core/skills.rs](../../../src-tauri/src/core/skills.rs), [core/rules.rs](../../../src-tauri/src/core/rules.rs), [core/templates.rs](../../../src-tauri/src/core/templates.rs), [core/user_agents.rs](../../../src-tauri/src/core/user_agents.rs), [core/commands.rs](../../../src-tauri/src/core/commands.rs), [core/mcp_servers.rs](../../../src-tauri/src/core/mcp_servers.rs), [core/project_templates.rs](../../../src-tauri/src/core/project_templates.rs), and the collections registry at `~/.automatic/skill-collections.json`.
- For skills, walk the directory and include every file under it (text only — reject binaries by MIME sniff, same spirit as `enforce_text_asset`).
- **MCP server secrets** must be stripped in `core/cloud_sync/secrets.rs`. Read the encrypted env via the existing `env_crypto` layer, extract only the key *names*, set `env_keys: [...]`. Never decrypt values for upload. A unit test must assert that a known plaintext secret never appears in the serialized bundle (mirroring the existing `env_values_are_encrypted_at_rest` test in [core/mcp_servers.rs](../../../src-tauri/src/core/mcp_servers.rs)).
- Compute `content_hash` as `sha256(canonical_json(content))` per asset so v2 down-sync can diff without a full re-read.

### Desktop — auth flow
- Mirror the PKCE flow in [src-tauri/src/oauth.rs](../../../src-tauri/src/oauth.rs): generate `code_verifier` + `code_challenge`, spin up a loopback HTTP listener on an ephemeral port, open the system browser via `tauri-plugin-opener` to `https://tryautomatic.app/oauth/authorize?...`.
- Tokens stored via the existing `keyring` helpers in [core/credentials.rs](../../../src-tauri/src/core/credentials.rs) under a dedicated service namespace (`automatic_cloud`).
- Access-token refresh happens lazily in `client.rs` on 401 — one retry, then surface an error to the user with a "Sign in again" CTA.

### Desktop — Tauri wiring
- Register new commands in `tauri::generate_handler![]` in [src-tauri/src/lib.rs](../../../src-tauri/src/lib.rs). (This is the documented easy-to-miss step from project memory.)
- Command wrappers match the thin-delegation style of existing wrappers in [commands/projects.rs](../../../src-tauri/src/commands/projects.rs).

### Webapp — OAuth endpoints
- `/oauth/authorize` uses `auth()` from `@clerk/nextjs/server` — if no `userId`, redirect to Clerk sign-in with return URL preserved.
- Consent screen on first sign-in per client; auto-consent on subsequent sign-ins (store a `consented_at` flag on a `oauth_clients` table if we want multiple client_ids later — out of scope for v1, assume single desktop client_id baked into both apps).
- Authorization codes: signed JWT (10-min TTL) containing `sub`, `code_challenge`, `redirect_uri`. One-shot — validate on token exchange and reject if re-used via a 10-min in-memory nonce set (or `INSERT … ON CONFLICT DO NOTHING` on a tiny `used_codes` table).

### Webapp — sync endpoint
- Verify access-token JWT, extract `clerk_user_id`, set `ownerType='user'`, `ownerId=clerk_user_id`.
- Validate bundle with Zod. Reject payloads > 10 MB (skills with huge companion files would hit this — revisit in v2 with object storage).
- Within a single Drizzle transaction:
  1. Read all existing `(assetType, machineName)` for this owner.
  2. Compute three sets: `toInsert` (in bundle, not in DB), `toUpdate` (in both, `content_hash` differs), `toDelete` (in DB, not in bundle).
  3. Apply, return summary.
- This makes v1 **destructive on delete** — assets missing from the bundle are removed server-side. Document prominently in the UI ("Cloud is a mirror; anything you delete locally disappears from the cloud on next sync").

### Webapp — identity & config
- Clerk user id is already available; store `email` and `firstName+lastName` on first sync as metadata to avoid Clerk lookups on reads.
- Env: `DATABASE_URL`, `OAUTH_JWT_SECRET`, `OAUTH_ACCESS_TTL_SECONDS`, `OAUTH_REFRESH_TTL_DAYS`. Document in `.env.example`.

---

## Critical Files

### Desktop (`automatic-app`)
| File | Change |
|---|---|
| `src-tauri/src/core/mod.rs` | Register new `cloud_sync` module |
| `src-tauri/src/core/cloud_sync/mod.rs` | **NEW** — module root |
| `src-tauri/src/core/cloud_sync/auth.rs` | **NEW** — PKCE, keyring, refresh |
| `src-tauri/src/core/cloud_sync/bundle.rs` | **NEW** — `LibraryBundle` type + builder reading from existing `core/*` readers |
| `src-tauri/src/core/cloud_sync/secrets.rs` | **NEW** — MCP env scrubber |
| `src-tauri/src/core/cloud_sync/client.rs` | **NEW** — `reqwest` wrapper with auth + retry |
| `src-tauri/src/commands/cloud_sync.rs` | **NEW** — four Tauri commands |
| `src-tauri/src/commands/mod.rs` | Add `pub mod cloud_sync;` |
| `src-tauri/src/lib.rs` | Register commands in `generate_handler![]` |
| `src/pages/Settings*` (TBD exact path) | Cloud section UI |
| `src/components/CloudSyncPanel.tsx` | **NEW** — sign-in state + Sync button + summary modal |
| `src-tauri/Cargo.toml` | No new deps — reuses `reqwest`, `keyring`, `sha2`, `serde_json` |

### Webapp (`automatic-webapp`)
| File | Change |
|---|---|
| `package.json` | Add `drizzle-orm`, `drizzle-kit`, `@neondatabase/serverless`, `zod`, `jose` |
| `drizzle.config.ts` | **NEW** |
| `src/db/schema.ts` | **NEW** — tables above |
| `src/db/client.ts` | **NEW** — Drizzle + Neon client |
| `src/db/migrations/0000_init.sql` | **NEW** — initial migration |
| `src/api/schema.ts` | **NEW** — shared Zod types mirroring Rust `LibraryBundle` |
| `src/app/oauth/authorize/route.ts` | **NEW** — GET handler, Clerk-gated |
| `src/app/oauth/consent/page.tsx` | **NEW** — consent UI |
| `src/app/oauth/token/route.ts` | **NEW** — POST handler |
| `src/app/oauth/revoke/route.ts` | **NEW** — POST handler |
| `src/app/api/me/route.ts` | **NEW** — GET, access-token gated |
| `src/app/api/library/route.ts` | **NEW** — GET current library |
| `src/app/api/library/sync/route.ts` | **NEW** — POST sync bundle |
| `src/app/dashboard/library/page.tsx` | **NEW** — read-only library view |
| `src/lib/access-token.ts` | **NEW** — JWT sign/verify via `jose` |
| `src/middleware.ts` | Ensure `/api/*` and `/oauth/token` bypass Clerk-session auth (they use bearer tokens / codes) |
| `.env.example` | Add `DATABASE_URL`, `OAUTH_JWT_SECRET`, TTLs |

---

## Explicitly Deferred (v2 / v3)

- **Down-sync (v2)** — desktop pulls server state. Schema already supports this via `content_hash`; the plan is to add `GET /api/library/diff?since_hash=...` and a client reconciler. Conflict policy: if local and cloud both changed an asset since last sync, prompt user to pick.
- **Team libraries (v3)** — Clerk Organizations are already enabled on the webapp. Extend `ownerType` to `'organization'`; add `orgId` selector in desktop UI; reuse all endpoints with a path param `/api/library/org/:orgId/sync`.
- **Object storage for large skill bundles** — v1 uses JSONB inline; switch to Vercel Blob / S3 when payloads commonly exceed 1 MB.
- **Automatic sync triggers** — on-change or periodic; add after v1 UX is validated.
- **Webhooks / push to desktop** — real-time cloud→desktop when team member updates an asset.

---

## Verification

### Desktop unit tests (Rust, run via `cargo test` from `src-tauri/`)
- `cloud_sync::secrets` — a known plaintext env value never appears in a serialized bundle (strongest correctness test).
- `cloud_sync::bundle` — builder produces expected asset counts for a fixture `~/.automatic-dev` tree; content hash is stable across reruns.
- `cloud_sync::auth` — PKCE code_verifier/challenge pair validates; refresh flow substitutes the new access token.

### Webapp unit/integration tests
- Zod schema round-trips a sample bundle without loss.
- `/api/library/sync` with a fixed bundle against a fresh DB produces the expected `added/updated/deleted` shape; a second sync of the same bundle produces all-`unchanged`.
- `/oauth/authorize` returns 302 to Clerk sign-in when signed out; returns a code when signed in.
- `/oauth/token` rejects a replayed authorization code.

### End-to-end manual checklist
1. `make dev` (desktop) and `pnpm dev` (webapp); configure `CLOUD_API_BASE=http://localhost:3000` in desktop debug settings.
2. Click **Sign in to cloud** in Settings → browser opens → sign in via Clerk → redirect back → desktop shows "Signed in as <email>".
3. Click **Sync now** → observe summary modal listing counts.
4. Visit `http://localhost:3000/dashboard/library` → assets appear grouped by type.
5. Locally delete one skill and one rule; click **Sync now** → summary shows `deleted: { skill: [...], rule: [...] }`; webapp dashboard no longer lists them.
6. Create an MCP server locally with a sensitive env var, sync, then `curl http://localhost:3000/api/library` with a valid token and confirm the env **value** is absent and only the key name is present.
7. Sign out on desktop → access token removed from keychain (verify with `security find-generic-password -s automatic_cloud` on macOS); subsequent sync attempts fail with "Sign in required".
8. Run `cargo test -p automatic` and `pnpm test` — both green.

## Risks & Open Items

- **Clerk as OAuth IdP vs session gate**: the plan uses Clerk as a *session gate* in front of webapp-minted tokens (simplest and most portable). If the team prefers Clerk's own OAuth-provider feature (when/if fully GA on their plan), swap `jose`-signed JWTs for Clerk-issued ones — isolated to `src/lib/access-token.ts` and a few routes.
- **Payload size for skill-heavy libraries**: the 10 MB ceiling is arbitrary. We should log actual payload sizes in the first weeks of use and revisit.
- **Collections semantics**: collections are currently a flat map (`skill-name → collection-name`). On upload they'll become top-level `collection` rows keyed by collection machine name with a `skills` array — a small normalization step in `bundle.rs`.
- **"Everything in the library" + remote-installed assets**: uploading content that originated in a public GitHub repo means we're hosting someone else's content. For v1 this is fine (it's only visible to the uploading user), but in v3 with team sharing we'll need a provenance display so teammates see where assets came from. Metadata already carries provenance — plumb it through to the dashboard.
