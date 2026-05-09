//! Cloud library sync orchestrator.
//!
//! Bidirectional delta sync per `docs/plans/cloud-sync/contract.md` and
//! `client-state.md`. Flow:
//!
//! 1. Load `CloudSyncState` from `{automatic_dir}/cloud-sync-state.json`.
//! 2. Read every asset from disk into a flat `Vec<DiskAsset>` using the
//!    per-kind bundle readers in this module.
//! 3. Diff disk vs `state.seen` (via `cloud_sync_diff`) to produce
//!    `{ upserts, new_tombstones }`.
//! 4. Merge `new_tombstones` into `state.pending_tombstones`, then build
//!    the outgoing delta bundle envelope.
//! 5. POST to `/api/library/sync`, refresh token once on 401.
//! 6. Apply the `ServerSyncResponse` via `cloud_sync_reconcile` — state
//!    mutations happen in-memory, disk ops return as a `FileOpsPlan`.
//! 7. Execute the plan against disk (per-kind writers).
//! 8. Atomically save the mutated `CloudSyncState`.
//!
//! Secret scrubbing (MCP `env` values) happens inside the per-kind reader
//! for MCP servers and is the only point where decrypted env leaves the
//! machine — see `scrub_mcp_config`.
//!
//! Feature-gated behind the `cloud_sync` flag at the command layer — see
//! `commands/cloud_sync.rs`. This module itself contains no gating; callers
//! enforce it.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::fs;

use crate::account;
use crate::core::cloud_sync_diff::{diff, DiskAsset, UpsertRecord};
use crate::core::cloud_sync_reconcile::{
    apply_server_response, FileOp, FileOpsPlan, ServerSyncResponse,
};
use crate::core::cloud_sync_state::{self, PendingTombstone};
use crate::core::KEYCHAIN_SERVICE;

// ── Constants ─────────────────────────────────────────────────────────────────

const SCHEMA_VERSION: u32 = 1;
const ACCESS_TOKEN_USER: &str = "automatic_account_access_token";
const REFRESH_TOKEN_USER: &str = "automatic_account_refresh_token";
const CLIENT_ID: &str = "automatic-desktop";

// ── Public types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub app_version: String,
    pub os: String,
}

/// Outgoing delta bundle, matches the wire format in
/// `docs/plans/cloud-sync/contract.md` §"Bundle shape".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaBundle {
    pub schema_version: u32,
    pub device_id: String,
    pub client: ClientInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync_at: Option<String>,
    pub upserts: Vec<UpsertBundleEntry>,
    pub tombstones: Vec<TombstoneBundleEntry>,
}

/// Per-asset wire shape inside `upserts`. Differs from `UpsertRecord` in that
/// `payload` is flattened into the outer object (so fields like `files`,
/// `content`, `config` appear at the top level per the contract).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertBundleEntry {
    pub kind: String,
    pub machine_name: String,
    pub content_hash: String,
    pub updated_at: String,
    #[serde(flatten)]
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TombstoneBundleEntry {
    pub kind: String,
    pub machine_name: String,
    pub deleted_at: String,
}

/// Client-side preview of what the next sync will send. Returned by
/// `preview_sync` so the UI can show counts before the user commits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPreview {
    pub upsert_count_by_kind: std::collections::BTreeMap<String, usize>,
    pub tombstone_count_by_kind: std::collections::BTreeMap<String, usize>,
    pub total_upserts: usize,
    pub total_tombstones: usize,
}

/// Result of a successful sync, shaped for the UI. Mirrors the server's
/// response plus a few client-side derived fields.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncSummary {
    /// What the server accepted from us.
    pub accepted_upserts: Value,
    /// Our edits the server rejected (its copy was newer) — silent
    /// overwrite UX per design decision, still surfaced here for telemetry.
    pub rejected_upserts: Value,
    pub applied_tombstones: Value,
    pub rejected_tombstones: Value,
    /// Counts of what came down from other devices.
    pub remote_upsert_count: usize,
    pub remote_tombstone_count: usize,
    pub server_time: String,
}

// ── Orchestration ─────────────────────────────────────────────────────────────

/// Compute what the next sync would send, without making any network calls
/// or mutating state. Used by the UI for a pre-flight preview.
pub fn preview_sync() -> Result<SyncPreview, String> {
    let state = cloud_sync_state::load_or_init()?;
    let disk = read_all_disk_assets()?;
    let delta = diff(&disk, &state, &now_rfc3339());

    use std::collections::BTreeMap;
    let mut upserts: BTreeMap<String, usize> = BTreeMap::new();
    for u in &delta.upserts {
        *upserts.entry(u.kind.clone()).or_insert(0) += 1;
    }

    let mut tombstones: BTreeMap<String, usize> = BTreeMap::new();
    // Pending + newly detected are both part of what we'd send.
    for t in state.pending_tombstones.iter().chain(delta.new_tombstones.iter()) {
        *tombstones.entry(t.kind.clone()).or_insert(0) += 1;
    }

    let total_upserts = delta.upserts.len();
    let total_tombstones = tombstones.values().sum();

    Ok(SyncPreview {
        upsert_count_by_kind: upserts,
        tombstone_count_by_kind: tombstones,
        total_upserts,
        total_tombstones,
    })
}

/// Full sync: diff → upload → reconcile → save state.
pub async fn run_sync() -> Result<SyncSummary, String> {
    let mut state = cloud_sync_state::load_or_init()?;
    let disk = read_all_disk_assets()?;
    let delta = diff(&disk, &state, &now_rfc3339());

    // Merge newly-detected deletes into pending (retry-safe).
    for new_tombstone in delta.new_tombstones {
        let already_pending = state
            .pending_tombstones
            .iter()
            .any(|t| t.kind == new_tombstone.kind && t.machine_name == new_tombstone.machine_name);
        if !already_pending {
            state.pending_tombstones.push(new_tombstone);
        }
    }

    let bundle = DeltaBundle {
        schema_version: SCHEMA_VERSION,
        device_id: state.device_id.clone(),
        client: ClientInfo {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            os: std::env::consts::OS.to_string(),
        },
        last_sync_at: state.last_sync_at.clone(),
        upserts: delta
            .upserts
            .iter()
            .cloned()
            .map(upsert_to_bundle_entry)
            .collect(),
        tombstones: state
            .pending_tombstones
            .iter()
            .cloned()
            .map(tombstone_to_bundle_entry)
            .collect(),
    };

    // Everything we sent — reconcile needs this to update `seen` for
    // accepted upserts using the hash/updated_at we generated client-side.
    let sent_upserts = delta.upserts;

    let response = upload_bundle(&bundle).await?;

    let plan = apply_server_response(&mut state, &sent_upserts, &response);

    // Apply disk ops. Currently a stub — see module-level TODO.
    apply_file_ops_plan(&plan)?;

    cloud_sync_state::save(&state)?;

    Ok(SyncSummary {
        accepted_upserts: response.accepted_upserts,
        rejected_upserts: response.rejected_upserts,
        applied_tombstones: response.applied_tombstones,
        rejected_tombstones: response.rejected_tombstones,
        remote_upsert_count: response.remote_upserts.len(),
        remote_tombstone_count: response.remote_tombstones.len(),
        server_time: response.server_time,
    })
}

fn upsert_to_bundle_entry(r: UpsertRecord) -> UpsertBundleEntry {
    UpsertBundleEntry {
        kind: r.kind,
        machine_name: r.machine_name,
        content_hash: r.content_hash,
        updated_at: r.updated_at,
        payload: r.payload,
    }
}

fn tombstone_to_bundle_entry(t: PendingTombstone) -> TombstoneBundleEntry {
    TombstoneBundleEntry {
        kind: t.kind,
        machine_name: t.machine_name,
        deleted_at: t.deleted_at,
    }
}

// ── Disk asset readers ────────────────────────────────────────────────────────

/// Concatenate every kind's on-disk assets into a single vec for diffing.
fn read_all_disk_assets() -> Result<Vec<DiskAsset>, String> {
    let mut out: Vec<DiskAsset> = Vec::new();
    out.extend(read_skills()?);
    out.extend(read_rules()?);
    out.extend(read_templates()?);
    out.extend(read_sub_agents()?);
    out.extend(read_commands()?);
    out.extend(read_mcp_servers()?);
    out.extend(read_collections()?);
    out.extend(read_project_templates()?);
    // `instruction` is per-project; not part of the user library.
    Ok(out)
}

fn read_skills() -> Result<Vec<DiskAsset>, String> {
    let entries = super::skills::list_skills().unwrap_or_default();
    let mut out: Vec<DiskAsset> = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(skill_dir) = super::skills::get_skill_dir(&entry.name).ok().flatten() else {
            continue;
        };
        let entrypoint = super::skills::get_skill_path(&entry.name).ok().flatten();
        let skill_md = entrypoint
            .as_ref()
            .and_then(|p| fs::read_to_string(p).ok())
            .unwrap_or_default();

        let mut files = Map::new();
        files.insert("SKILL.md".to_string(), Value::String(skill_md));
        if let Ok(resources) = super::skills::list_skill_resources(&entry.name) {
            for file in &resources.root_files {
                if file.path == "SKILL.md" {
                    continue;
                }
                if let Ok(body) = fs::read_to_string(skill_dir.join(&file.path)) {
                    files.insert(file.path.clone(), Value::String(body));
                }
            }
            for dir in &resources.dirs {
                for file in &dir.files {
                    let rel = format!("{}/{}", dir.name, file.path);
                    if let Ok(body) = fs::read_to_string(skill_dir.join(&rel)) {
                        files.insert(rel, Value::String(body));
                    }
                }
            }
        }

        let payload = json!({ "files": files });
        out.push(DiskAsset {
            kind: "skill".to_string(),
            machine_name: name_to_machine(&entry.name),
            content_hash: content_hash(&payload),
            payload,
        });
    }
    Ok(out)
}

fn read_rules() -> Result<Vec<DiskAsset>, String> {
    let entries = super::rules::list_rules().unwrap_or_default();
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        let raw = match super::rules::read_rule(&entry.id) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let parsed: Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let content = parsed
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let payload = json!({ "content": content });
        out.push(DiskAsset {
            kind: "rule".to_string(),
            machine_name: name_to_machine(&entry.id),
            content_hash: content_hash(&payload),
            payload,
        });
    }
    Ok(out)
}

fn read_templates() -> Result<Vec<DiskAsset>, String> {
    let names = super::instructions::list_instructions().unwrap_or_default();
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        let content = match super::instructions::read_instruction(&name) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let payload = json!({ "content": content, "format": "markdown" });
        out.push(DiskAsset {
            kind: "template".to_string(),
            machine_name: name_to_machine(&name),
            content_hash: content_hash(&payload),
            payload,
        });
    }
    Ok(out)
}

fn read_sub_agents() -> Result<Vec<DiskAsset>, String> {
    let entries = super::subagents::list_subagents().unwrap_or_default();
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        // Codex-derived sub-agents are synthetic — don't upload them.
        if entry.id.starts_with("codex-") {
            continue;
        }
        let raw = match super::subagents::read_subagent(&entry.id) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let parsed: Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let content = parsed
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let payload = json!({ "content": content });
        out.push(DiskAsset {
            kind: "sub_agent".to_string(),
            machine_name: name_to_machine(&entry.id),
            content_hash: content_hash(&payload),
            payload,
        });
    }
    Ok(out)
}

fn read_commands() -> Result<Vec<DiskAsset>, String> {
    let entries = super::commands::list_user_commands().unwrap_or_default();
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        let content = match super::commands::read_user_command(&entry.id) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let payload = json!({ "content": content });
        out.push(DiskAsset {
            kind: "command".to_string(),
            machine_name: name_to_machine(&entry.id),
            content_hash: content_hash(&payload),
            payload,
        });
    }
    Ok(out)
}

fn read_mcp_servers() -> Result<Vec<DiskAsset>, String> {
    let names = super::mcp_servers::list_mcp_server_configs().unwrap_or_default();
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        let raw = match super::mcp_servers::read_mcp_server_config(&name) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let parsed: Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let config = scrub_mcp_config(&parsed);
        let payload = json!({ "config": config });
        out.push(DiskAsset {
            kind: "mcp_server".to_string(),
            machine_name: name_to_machine(&name),
            content_hash: content_hash(&payload),
            payload,
        });
    }
    Ok(out)
}

fn read_collections() -> Result<Vec<DiskAsset>, String> {
    let collections = super::skills::list_skill_collections().unwrap_or_default();
    let mut out = Vec::with_capacity(collections.len());
    for collection in collections {
        let payload = json!({ "skills": collection.skills });
        out.push(DiskAsset {
            kind: "collection".to_string(),
            machine_name: name_to_machine(&collection.name),
            content_hash: content_hash(&payload),
            payload,
        });
    }
    Ok(out)
}

fn read_project_templates() -> Result<Vec<DiskAsset>, String> {
    let names = super::templates::list_templates().unwrap_or_default();
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        let raw = match super::templates::read_template(&name) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let parsed: Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let payload = json!({ "content": parsed });
        out.push(DiskAsset {
            kind: "project_template".to_string(),
            machine_name: name_to_machine(&name),
            content_hash: content_hash(&payload),
            payload,
        });
    }
    Ok(out)
}

// ── File ops plan execution ───────────────────────────────────────────────────

/// Execute the disk writes and deletes returned by `apply_server_response`.
///
/// Errors from individual kinds are logged and swallowed: one broken asset
/// should not abort an otherwise successful sync. Anything that fails to
/// write simply stays out-of-date locally and will be re-emitted on the next
/// sync round when the hash mismatch is detected again. The `seen` state has
/// already been updated in the reconcile pass, so a failed write here will
/// surface as a local-disk-missing-but-server-has-it discrepancy on the next
/// diff — the correct fallback.
fn apply_file_ops_plan(plan: &FileOpsPlan) -> Result<(), String> {
    for op in &plan.ops {
        match op {
            FileOp::WriteAsset {
                kind,
                machine_name,
                payload,
                ..
            } => {
                if let Err(e) = write_asset(kind, machine_name, payload) {
                    eprintln!(
                        "cloud_sync: failed to write {}:{} — {}",
                        kind, machine_name, e
                    );
                }
            }
            FileOp::DeleteAsset { kind, machine_name } => {
                if let Err(e) = delete_asset(kind, machine_name) {
                    eprintln!(
                        "cloud_sync: failed to delete {}:{} — {}",
                        kind, machine_name, e
                    );
                }
            }
        }
    }
    Ok(())
}

/// Dispatch a write to the right per-kind writer. The `payload` shape matches
/// the matching `read_*` function in this module (same kind, same fields).
fn write_asset(kind: &str, machine_name: &str, payload: &Value) -> Result<(), String> {
    match kind {
        "skill" => write_skill_asset(machine_name, payload),
        "rule" => write_rule_asset(machine_name, payload),
        "template" => write_template_asset(machine_name, payload),
        "sub_agent" => write_sub_agent_asset(machine_name, payload),
        "command" => write_command_asset(machine_name, payload),
        "mcp_server" => write_mcp_server_asset(machine_name, payload),
        "collection" => write_collection_asset(machine_name, payload),
        "project_template" => write_project_template_asset(machine_name, payload),
        // `instruction` is per-project and never shows up in the user library;
        // anything else is forward-compat chatter from a newer server we can
        // safely ignore.
        _ => Ok(()),
    }
}

fn delete_asset(kind: &str, machine_name: &str) -> Result<(), String> {
    match kind {
        "skill" => super::skills::delete_skill(machine_name),
        "rule" => super::rules::delete_rule(machine_name),
        "template" => super::instructions::delete_instruction(machine_name),
        "sub_agent" => super::subagents::delete_subagent(machine_name),
        "command" => super::commands::delete_user_command(machine_name),
        "mcp_server" => super::mcp_servers::delete_mcp_server_config(machine_name),
        "collection" => delete_collection_asset(machine_name),
        "project_template" => super::templates::delete_template(machine_name),
        _ => Ok(()),
    }
}

// ── Per-kind writers ──────────────────────────────────────────────────────────

/// Write a skill: the payload carries a `files: { path -> string }` map.
/// Strategy: clear the existing skill directory in the managed library
/// (`~/.automatic/library/skills/`), then write every file from the payload.
/// A full replace keeps local state in lockstep with the server — any file
/// the server doesn't know about is removed, so renames and deletions
/// inside a skill round-trip correctly.
fn write_skill_asset(machine_name: &str, payload: &Value) -> Result<(), String> {
    use super::asset_security::validate_relative_asset_path;
    use super::paths::{get_library_skills_dir, is_valid_name};

    if !is_valid_name(machine_name) {
        return Err(format!("invalid skill name '{}'", machine_name));
    }
    let files = payload
        .get("files")
        .and_then(|v| v.as_object())
        .ok_or_else(|| "skill payload missing `files` object".to_string())?;

    let skills_root = get_library_skills_dir()?;
    let skill_dir = skills_root.join(machine_name);

    // Wipe + recreate so server is authoritative over the skill's full tree.
    if skill_dir.exists() {
        fs::remove_dir_all(&skill_dir)
            .map_err(|e| format!("failed to clear skill dir: {}", e))?;
    }
    fs::create_dir_all(&skill_dir)
        .map_err(|e| format!("failed to create skill dir: {}", e))?;

    for (rel_path, content) in files {
        // Defensive: the server validates these, but we never trust a remote
        // path unconditionally — `..` traversal would escape the skills tree.
        validate_relative_asset_path(rel_path, "skill file")?;
        let Some(content_str) = content.as_str() else {
            continue;
        };
        let target = skill_dir.join(rel_path);
        if let Some(parent) = target.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("failed to create {}: {}", parent.display(), e))?;
            }
        }
        fs::write(&target, content_str)
            .map_err(|e| format!("failed to write {}: {}", target.display(), e))?;
    }
    Ok(())
}

fn write_rule_asset(machine_name: &str, payload: &Value) -> Result<(), String> {
    let content = payload
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "rule payload missing `content`".to_string())?;
    // `save_rule` needs a display name. The contract doesn't carry one — fall
    // back to the existing local display name if the rule is already on disk
    // (so a round-trip preserves it), otherwise default to the machine name.
    let display_name = super::rules::read_rule(machine_name)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(String::from))
        .unwrap_or_else(|| machine_name.to_string());
    super::rules::save_rule(machine_name, &display_name, content)
}

fn write_template_asset(machine_name: &str, payload: &Value) -> Result<(), String> {
    let content = payload
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "instruction payload missing `content`".to_string())?;
    super::instructions::save_instruction(machine_name, content)
}

fn write_sub_agent_asset(machine_name: &str, payload: &Value) -> Result<(), String> {
    let content = payload
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "sub_agent payload missing `content`".to_string())?;
    // Display name: prefer the YAML frontmatter `name`, then fall back to
    // existing local display name, then to machine_name.
    let display_name = extract_frontmatter_name(content)
        .or_else(|| {
            super::subagents::read_subagent(machine_name)
                .ok()
                .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
                .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(String::from))
        })
        .unwrap_or_else(|| machine_name.to_string());
    super::subagents::save_subagent(machine_name, &display_name, content)
}

fn write_command_asset(machine_name: &str, payload: &Value) -> Result<(), String> {
    let content = payload
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "command payload missing `content`".to_string())?;
    super::commands::save_user_command(machine_name, content)
}

/// Write an MCP server config. The server-side version has `env` scrubbed and
/// replaced by `env_keys` (string array, keys only). We restore the local
/// plaintext `env` for any key the user already has set — losing a secret
/// because another device re-saved the config would be a hostile UX.
/// Unknown keys in `env_keys` land as empty strings so the user can fill them
/// in later.
fn write_mcp_server_asset(machine_name: &str, payload: &Value) -> Result<(), String> {
    let config_value = payload
        .get("config")
        .cloned()
        .ok_or_else(|| "mcp_server payload missing `config`".to_string())?;
    let mut config = config_value;
    let config_obj = config
        .as_object_mut()
        .ok_or_else(|| "mcp_server `config` must be an object".to_string())?;

    // Collect the env_keys the server advertises and drop the scrubbed field
    // — it doesn't belong in the on-disk shape.
    let env_keys: Vec<String> = config_obj
        .remove("env_keys")
        .and_then(|v| v.as_array().cloned())
        .map(|arr| {
            arr.into_iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    // Merge with any existing local env so we never clobber secrets the user
    // has set on this device.
    let existing_env: Map<String, Value> = super::mcp_servers::read_mcp_server_config(machine_name)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|v| v.get("env").and_then(|e| e.as_object()).cloned())
        .unwrap_or_default();

    if !env_keys.is_empty() || !existing_env.is_empty() {
        let mut merged = Map::new();
        for key in &env_keys {
            let value = existing_env
                .get(key)
                .cloned()
                .unwrap_or_else(|| Value::String(String::new()));
            merged.insert(key.clone(), value);
        }
        config_obj.insert("env".to_string(), Value::Object(merged));
    }

    let data = serde_json::to_string(&config)
        .map_err(|e| format!("failed to serialise mcp_server config: {}", e))?;
    super::mcp_servers::save_mcp_server_config(machine_name, &data)
}

/// Reconcile a skill collection: the payload has `skills: [string]` — the
/// full membership list. We must match that list exactly on this device.
fn write_collection_asset(machine_name: &str, payload: &Value) -> Result<(), String> {
    let incoming: Vec<String> = payload
        .get("skills")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    // Current skills in this collection on disk.
    let current: Vec<String> = super::skills::list_skill_collections()
        .unwrap_or_default()
        .into_iter()
        .find(|c| c.name == machine_name)
        .map(|c| c.skills)
        .unwrap_or_default();

    // Remove skills that were in the collection but aren't any more. We only
    // unassign if the skill still shows this collection — if the user has
    // since moved it to another collection we don't meddle.
    let registry = super::skills::read_skill_collections().unwrap_or_default();
    for skill in &current {
        if !incoming.contains(skill) && registry.get(skill).map(|c| c.as_str()) == Some(machine_name)
        {
            let _ = super::skills::remove_skill_collection(skill);
        }
    }

    if !incoming.is_empty() {
        super::skills::set_skills_collection(&incoming, machine_name)?;
    }
    Ok(())
}

/// Deleting a collection means removing every skill's assignment to it.
fn delete_collection_asset(machine_name: &str) -> Result<(), String> {
    let registry = super::skills::read_skill_collections().unwrap_or_default();
    for (skill, collection) in &registry {
        if collection == machine_name {
            let _ = super::skills::remove_skill_collection(skill);
        }
    }
    Ok(())
}

fn write_project_template_asset(machine_name: &str, payload: &Value) -> Result<(), String> {
    let content = payload
        .get("content")
        .ok_or_else(|| "project_template payload missing `content`".to_string())?;
    let data = serde_json::to_string(content)
        .map_err(|e| format!("failed to serialise project_template: {}", e))?;
    super::templates::save_template(machine_name, &data)
}

/// Extract the `name:` field from a Markdown-with-YAML-frontmatter sub-agent
/// file. Lightweight parser — user_agents::extract_name_from_frontmatter is
/// private, and we only need a best-effort fallback here.
fn extract_frontmatter_name(content: &str) -> Option<String> {
    let body = content.strip_prefix("---\n").or_else(|| content.strip_prefix("---\r\n"))?;
    let end = body.find("\n---").or_else(|| body.find("\r\n---"))?;
    for line in body[..end].lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("name:") {
            let value = rest.trim().trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

// ── Secret scrubbing ──────────────────────────────────────────────────────────

/// Remove the decrypted `env` field from an MCP server config and replace it
/// with `env_keys: [string]` (key names only). Nothing else is changed.
///
/// **This is the ONLY point where the decrypted env values for MCP servers
/// touch the outgoing bundle.** Any code path that uploads an MCP server
/// config MUST go through this function.
pub fn scrub_mcp_config(config: &Value) -> Value {
    let mut out = config.clone();
    if let Some(obj) = out.as_object_mut() {
        let env_keys: Vec<String> = obj
            .get("env")
            .and_then(|v| v.as_object())
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        obj.remove("env");
        obj.insert(
            "env_keys".to_string(),
            Value::Array(env_keys.into_iter().map(Value::String).collect()),
        );
    }
    out
}

// ── Hashing & normalisation ───────────────────────────────────────────────────

/// Canonical SHA256 of a JSON value: serialize with sorted keys, then hash.
fn content_hash(value: &Value) -> String {
    let canonical = canonicalize(value);
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    hex::encode(hasher.finalize())
}

fn canonicalize(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys
                .into_iter()
                .map(|k| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(k).unwrap_or_default(),
                        canonicalize(&map[k])
                    )
                })
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        Value::Array(arr) => {
            let parts: Vec<String> = arr.iter().map(canonicalize).collect();
            format!("[{}]", parts.join(","))
        }
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}

/// Convert an on-disk asset name to a machine name matching the server's
/// strict regex `^[a-z0-9][a-z0-9._-]*$` (max 128). Lowercases, replaces
/// anything outside the allowed alphabet with `-`, collapses runs of `-`,
/// trims any leading non-alphanumeric prefix and any trailing `-`/`.`/`_`,
/// and truncates to 128 characters.
fn name_to_machine(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut last_dash = false;
    for c in lower.chars() {
        let mapped = if c.is_ascii_alphanumeric() || c == '.' || c == '_' {
            c
        } else {
            '-'
        };
        if mapped == '-' {
            if last_dash || out.is_empty() {
                continue;
            }
            last_dash = true;
        } else {
            last_dash = false;
        }
        out.push(mapped);
    }
    while let Some(c) = out.chars().next() {
        if c.is_ascii_alphanumeric() {
            break;
        }
        out.remove(0);
    }
    while out.ends_with('-') || out.ends_with('.') || out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("unnamed");
    }
    if out.len() > 128 {
        out.truncate(128);
        while out.ends_with('-') || out.ends_with('.') || out.ends_with('_') {
            out.pop();
        }
    }
    out
}

fn now_rfc3339() -> String {
    // Seconds-granularity UTC ISO-8601 without any chrono dependency.
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_rfc3339_utc(secs)
}

/// Minimal UNIX-epoch-seconds → "YYYY-MM-DDTHH:MM:SSZ" formatter.
/// Good for every real-world sync time; we don't need sub-second precision
/// because the server clamps to `min(client_ts, server_now)` anyway.
fn format_rfc3339_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = (secs % 86_400) as i64;
    let hour = rem / 3_600;
    let minute = (rem % 3_600) / 60;
    let second = rem % 60;
    let (y, m, d) = days_to_ymd(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y, m, d, hour, minute, second
    )
}

/// Days since 1970-01-01 → (year, month, day), both 1-indexed. Uses the
/// civil_from_days algorithm from Howard Hinnant's date library (public domain).
fn days_to_ymd(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (year, m as u32, d as u32)
}

// ── HTTP ──────────────────────────────────────────────────────────────────────

async fn upload_bundle(bundle: &DeltaBundle) -> Result<ServerSyncResponse, String> {
    let webapp = account::webapp_url();
    let url = format!("{}/api/library/sync", webapp);
    let client = http_client()?;

    let access = keychain_load(ACCESS_TOKEN_USER)
        .map_err(|_| "Not signed in. Please sign in first.".to_string())?;

    let resp = client
        .post(&url)
        .bearer_auth(&access)
        .json(bundle)
        .send()
        .await
        .map_err(|e| format!("sync request failed: {}", e))?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        let new_access = refresh_access_token(&client, &webapp).await?;
        let retry = client
            .post(&url)
            .bearer_auth(&new_access)
            .json(bundle)
            .send()
            .await
            .map_err(|e| format!("sync retry failed: {}", e))?;
        return handle_sync_response(retry).await;
    }

    handle_sync_response(resp).await
}

async fn handle_sync_response(resp: reqwest::Response) -> Result<ServerSyncResponse, String> {
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Sync failed (HTTP {}): {}", status, text));
    }
    resp.json::<ServerSyncResponse>()
        .await
        .map_err(|e| format!("failed to parse sync response: {}", e))
}

async fn refresh_access_token(
    client: &reqwest::Client,
    webapp: &str,
) -> Result<String, String> {
    let refresh = keychain_load(REFRESH_TOKEN_USER)
        .map_err(|_| "Session expired. Please sign in again.".to_string())?;

    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh.as_str()),
        ("client_id", CLIENT_ID),
    ];
    let resp = client
        .post(format!("{}/oauth/token", webapp))
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("refresh request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Session refresh failed (HTTP {}): {}", status, text));
    }

    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: String,
        #[serde(default)]
        refresh_token: Option<String>,
    }
    let tokens: TokenResponse = resp
        .json()
        .await
        .map_err(|e| format!("failed to parse refresh response: {}", e))?;

    keychain_store(ACCESS_TOKEN_USER, &tokens.access_token)?;
    if let Some(new_refresh) = &tokens.refresh_token {
        keychain_store(REFRESH_TOKEN_USER, new_refresh)?;
    }
    Ok(tokens.access_token)
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("failed to build HTTP client: {}", e))
}

fn keychain_store(user: &str, value: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, user).map_err(|e| e.to_string())?;
    entry.set_password(value).map_err(|e| e.to_string())
}

fn keychain_load(user: &str) -> Result<String, String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, user).map_err(|e| e.to_string())?;
    entry.get_password().map_err(|e| e.to_string())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub_removes_env_values_keeps_keys() {
        let config = json!({
            "command": "npx",
            "args": ["-y", "my-server"],
            "env": {
                "API_KEY": "super-secret-value",
                "DB_URL": "postgres://u:p@h/d"
            }
        });
        let scrubbed = scrub_mcp_config(&config);

        assert!(scrubbed.get("env").is_none(), "env must be removed");
        let keys: Vec<String> = scrubbed
            .get("env_keys")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(sorted, vec!["API_KEY".to_string(), "DB_URL".to_string()]);

        let serialised = serde_json::to_string(&scrubbed).expect("serialise");
        assert!(!serialised.contains("super-secret-value"));
        assert!(!serialised.contains("postgres://u:p@h/d"));
        assert_eq!(scrubbed["command"].as_str().unwrap(), "npx");
    }

    #[test]
    fn scrub_handles_missing_env_gracefully() {
        let config = json!({ "command": "node", "args": [] });
        let scrubbed = scrub_mcp_config(&config);
        assert_eq!(
            scrubbed["env_keys"].as_array().map(|a| a.len()),
            Some(0)
        );
    }

    #[test]
    fn content_hash_stable_across_key_order() {
        let a = json!({ "a": 1, "b": 2, "c": [1, 2, 3] });
        let b = json!({ "c": [1, 2, 3], "b": 2, "a": 1 });
        assert_eq!(content_hash(&a), content_hash(&b));
    }

    #[test]
    fn content_hash_differs_on_value_change() {
        let a = json!({ "content": "hello" });
        let b = json!({ "content": "hello!" });
        assert_ne!(content_hash(&a), content_hash(&b));
    }

    #[test]
    fn name_to_machine_basic_conversions() {
        assert_eq!(name_to_machine("Agent Project Brief"), "agent-project-brief");
        assert_eq!(name_to_machine("  Session Context  "), "session-context");
        assert_eq!(name_to_machine("A/B - Test"), "a-b-test");
        assert_eq!(name_to_machine("already-ok"), "already-ok");
        assert_eq!(name_to_machine(""), "unnamed");
    }

    fn matches_server_regex(s: &str) -> bool {
        if s.is_empty() || s.len() > 128 {
            return false;
        }
        let mut chars = s.chars();
        let first = chars.next().unwrap();
        if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
            return false;
        }
        chars.all(|c| {
            c.is_ascii_lowercase()
                || c.is_ascii_digit()
                || c == '.'
                || c == '_'
                || c == '-'
        })
    }

    #[test]
    fn name_to_machine_always_matches_server_regex() {
        let inputs = [
            "Agent Project Brief", "  Session Context  ", "A/B - Test",
            "already-ok", "", "UPPER CASE", "123abc", "-leading-dash",
            ".leading-dot", "_leading-underscore", "trailing-dash-",
            "trailing.dot.", "trailing_underscore_", "weird!@#$%^&*()chars",
            "multi    spaces", "unicodé-name", "a", "9", "!!!",
            &"x".repeat(200),
            &format!("{}{}", "x".repeat(120), "-".repeat(20)),
        ];
        for input in inputs {
            let machine = name_to_machine(input);
            assert!(
                matches_server_regex(&machine),
                "name_to_machine({:?}) = {:?} does not match server regex",
                input, machine,
            );
        }
    }

    #[test]
    fn format_rfc3339_utc_renders_known_epochs() {
        // 2026-04-18T10:15:00Z = 1 776 507 300s from 1970-01-01T00:00:00Z
        // (56 years × 365 days + 14 leap days + 107 days into 2026 + 10h15m)
        assert_eq!(
            format_rfc3339_utc(1_776_507_300),
            "2026-04-18T10:15:00Z".to_string()
        );
        // UNIX epoch itself
        assert_eq!(
            format_rfc3339_utc(0),
            "1970-01-01T00:00:00Z".to_string()
        );
        // A known sanity check — leap day 2024
        assert_eq!(
            format_rfc3339_utc(1_709_164_800),
            "2024-02-29T00:00:00Z".to_string()
        );
    }

    #[test]
    fn read_all_disk_assets_machine_names_all_valid() {
        // Smoke test over the real dev ~/.automatic-dev layout.
        // Skips silently if nothing is configured (CI).
        let assets = match read_all_disk_assets() {
            Ok(a) => a,
            Err(_) => return,
        };
        for asset in assets {
            assert!(
                matches_server_regex(&asset.machine_name),
                "{}:{} does not match server regex",
                asset.kind, asset.machine_name,
            );
        }
    }

    // ── Writer tests ──────────────────────────────────────────────────────────

    use crate::core::paths::with_test_home;

    fn with_temp_home<T>(test: impl FnOnce(&std::path::Path) -> T) -> T {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().to_path_buf();
        with_test_home(path.clone(), || test(&path))
    }

    #[test]
    fn write_skill_asset_writes_multi_file_payload() {
        with_temp_home(|home| {
            let payload = json!({
                "files": {
                    "SKILL.md": "# Hello",
                    "scripts/run.sh": "#!/bin/sh\necho hi",
                }
            });
            write_skill_asset("my-skill", &payload).expect("write skill");

            let skill_dir = home.join(".automatic-dev/library/skills/my-skill");
            assert_eq!(
                fs::read_to_string(skill_dir.join("SKILL.md")).unwrap(),
                "# Hello"
            );
            assert_eq!(
                fs::read_to_string(skill_dir.join("scripts/run.sh")).unwrap(),
                "#!/bin/sh\necho hi"
            );
        });
    }

    #[test]
    fn write_skill_asset_replaces_existing_tree_exactly() {
        // If the server drops a file from the skill, the local copy must
        // lose it too — otherwise "file renamed on device A" would leave
        // the old filename orphaned on device B forever.
        with_temp_home(|home| {
            let skill_dir = home.join(".automatic-dev/library/skills/my-skill");
            fs::create_dir_all(skill_dir.join("scripts")).unwrap();
            fs::write(skill_dir.join("SKILL.md"), "# Old").unwrap();
            fs::write(skill_dir.join("scripts/legacy.sh"), "legacy").unwrap();

            let payload = json!({
                "files": { "SKILL.md": "# New" }
            });
            write_skill_asset("my-skill", &payload).expect("write skill");

            assert_eq!(
                fs::read_to_string(skill_dir.join("SKILL.md")).unwrap(),
                "# New"
            );
            assert!(
                !skill_dir.join("scripts/legacy.sh").exists(),
                "stale file must be removed by the full-replace semantics"
            );
        });
    }

    #[test]
    fn write_skill_asset_rejects_parent_traversal() {
        with_temp_home(|_| {
            let payload = json!({
                "files": { "../escape.md": "nope" }
            });
            let err = write_skill_asset("my-skill", &payload).expect_err("must reject");
            assert!(
                err.contains("parent traversal") || err.contains("within the package root"),
                "unexpected error: {err}"
            );
        });
    }

    #[test]
    fn write_rule_asset_creates_rule_on_disk() {
        with_temp_home(|home| {
            let payload = json!({ "content": "Do the thing" });
            write_rule_asset("my-rule", &payload).expect("write rule");

            let raw =
                fs::read_to_string(home.join(".automatic-dev/library/rules/my-rule.json")).unwrap();
            let v: Value = serde_json::from_str(&raw).unwrap();
            assert_eq!(v["content"].as_str().unwrap(), "Do the thing");
            // Display name falls back to machine name when no prior rule exists.
            assert_eq!(v["name"].as_str().unwrap(), "my-rule");
        });
    }

    #[test]
    fn write_rule_asset_preserves_existing_display_name() {
        with_temp_home(|_| {
            super::super::rules::save_rule("my-rule", "My Friendly Rule", "initial")
                .expect("seed rule");

            let payload = json!({ "content": "updated body" });
            write_rule_asset("my-rule", &payload).expect("write rule");

            let raw = super::super::rules::read_rule("my-rule").unwrap();
            let v: Value = serde_json::from_str(&raw).unwrap();
            assert_eq!(v["name"].as_str().unwrap(), "My Friendly Rule");
            assert_eq!(v["content"].as_str().unwrap(), "updated body");
        });
    }

    #[test]
    fn write_template_asset_creates_template_file() {
        with_temp_home(|home| {
            let payload = json!({ "content": "# Template body", "format": "markdown" });
            write_template_asset("my-template", &payload).expect("write template");

            let body =
                fs::read_to_string(home.join(".automatic-dev/library/instructions/my-template.md")).unwrap();
            assert_eq!(body, "# Template body");
        });
    }

    #[test]
    fn write_command_asset_creates_command_file() {
        with_temp_home(|home| {
            let payload = json!({ "content": "---\ndescription: test\n---\nbody" });
            write_command_asset("my-cmd", &payload).expect("write command");

            let body =
                fs::read_to_string(home.join(".automatic-dev/library/commands/my-cmd.md")).unwrap();
            assert!(body.contains("body"));
        });
    }

    #[test]
    fn write_sub_agent_asset_uses_frontmatter_name() {
        with_temp_home(|_| {
            let content = "---\nname: Friendly Name\n---\nAgent body text";
            let payload = json!({ "content": content });
            write_sub_agent_asset("my-agent", &payload).expect("write");

            let raw = super::super::subagents::read_subagent("my-agent").unwrap();
            let v: Value = serde_json::from_str(&raw).unwrap();
            assert_eq!(v["name"].as_str().unwrap(), "Friendly Name");
        });
    }

    #[test]
    fn write_mcp_server_asset_preserves_existing_env_values() {
        // The scrubbed config we receive from the server only carries env_keys.
        // When the user already has secrets for those keys on this device, we
        // must keep them — otherwise a sync would silently blank the user's
        // API keys.
        with_temp_home(|_| {
            let initial = json!({
                "command": "npx",
                "env": { "API_KEY": "local-secret", "DB_URL": "postgres://local" }
            });
            super::super::mcp_servers::save_mcp_server_config(
                "my-server",
                &serde_json::to_string(&initial).unwrap(),
            )
            .expect("seed server");

            let payload = json!({
                "config": {
                    "command": "npx",
                    "args": ["--new-flag"],
                    "env_keys": ["API_KEY", "DB_URL"]
                }
            });
            write_mcp_server_asset("my-server", &payload).expect("write server");

            let raw = super::super::mcp_servers::read_mcp_server_config("my-server").unwrap();
            let v: Value = serde_json::from_str(&raw).unwrap();
            assert_eq!(v["command"].as_str().unwrap(), "npx");
            assert_eq!(v["args"][0].as_str().unwrap(), "--new-flag");
            assert_eq!(v["env"]["API_KEY"].as_str().unwrap(), "local-secret");
            assert_eq!(v["env"]["DB_URL"].as_str().unwrap(), "postgres://local");
        });
    }

    #[test]
    fn write_mcp_server_asset_blanks_new_keys_without_local_secret() {
        with_temp_home(|_| {
            // No prior config — every env_key lands as empty string so the
            // user can fill it in from the UI later.
            let payload = json!({
                "config": {
                    "command": "npx",
                    "env_keys": ["TOKEN"]
                }
            });
            write_mcp_server_asset("new-server", &payload).expect("write");

            let raw = super::super::mcp_servers::read_mcp_server_config("new-server").unwrap();
            let v: Value = serde_json::from_str(&raw).unwrap();
            assert_eq!(v["env"]["TOKEN"].as_str().unwrap(), "");
        });
    }

    #[test]
    fn write_collection_asset_reconciles_membership() {
        with_temp_home(|_| {
            // Pre-populate: "alpha" and "beta" assigned to "cool-set".
            super::super::skills::set_skills_collection(
                &["alpha".to_string(), "beta".to_string()],
                "cool-set",
            )
            .expect("seed");

            // Server says: "cool-set" is now just { beta, gamma }.
            let payload = json!({ "skills": ["beta", "gamma"] });
            write_collection_asset("cool-set", &payload).expect("reconcile");

            let registry = super::super::skills::read_skill_collections().unwrap();
            assert_eq!(registry.get("alpha"), None, "alpha must leave the set");
            assert_eq!(registry.get("beta").map(|s| s.as_str()), Some("cool-set"));
            assert_eq!(registry.get("gamma").map(|s| s.as_str()), Some("cool-set"));
        });
    }

    #[test]
    fn write_collection_asset_leaves_skills_in_other_collections_alone() {
        // If a skill has been moved to a different collection locally, dropping
        // it from THIS collection on the server must not yank it out of the
        // other one.
        with_temp_home(|_| {
            super::super::skills::set_skill_collection("alpha", "old-set").expect("seed");
            super::super::skills::set_skill_collection("alpha", "new-set")
                .expect("move alpha");

            // Now "old-set" server payload drops alpha. Since alpha is no
            // longer a member of old-set on disk, we mustn't touch it.
            let payload = json!({ "skills": [] });
            write_collection_asset("old-set", &payload).expect("reconcile");

            let registry = super::super::skills::read_skill_collections().unwrap();
            assert_eq!(
                registry.get("alpha").map(|s| s.as_str()),
                Some("new-set"),
                "alpha should remain in new-set"
            );
        });
    }

    #[test]
    fn delete_collection_asset_unassigns_all_members() {
        with_temp_home(|_| {
            super::super::skills::set_skills_collection(
                &["alpha".to_string(), "beta".to_string()],
                "doomed-set",
            )
            .expect("seed");

            delete_collection_asset("doomed-set").expect("delete");

            let registry = super::super::skills::read_skill_collections().unwrap();
            assert!(!registry.contains_key("alpha"));
            assert!(!registry.contains_key("beta"));
        });
    }

    #[test]
    fn delete_asset_dispatches_to_the_right_kind() {
        with_temp_home(|home| {
            super::super::instructions::save_instruction("to-delete", "# gone soon")
                .expect("seed instruction");
            assert!(home
                .join(".automatic-dev/library/instructions/to-delete.md")
                .exists());

            delete_asset("template", "to-delete").expect("delete");
            assert!(!home
                .join(".automatic-dev/library/instructions/to-delete.md")
                .exists());
        });
    }

    #[test]
    fn write_asset_ignores_unknown_kinds() {
        // Forward compat: if the server ever sends a newer kind we don't
        // recognise, we ignore it rather than erroring.
        with_temp_home(|_| {
            let payload = json!({ "content": "??" });
            write_asset("some_future_kind", "whatever", &payload).expect("tolerate");
        });
    }

    #[test]
    fn extract_frontmatter_name_handles_crlf_and_quotes() {
        assert_eq!(
            extract_frontmatter_name("---\nname: Plain\n---\nbody"),
            Some("Plain".to_string())
        );
        assert_eq!(
            extract_frontmatter_name("---\r\nname: \"Quoted\"\r\n---\r\nbody"),
            Some("Quoted".to_string())
        );
        assert_eq!(extract_frontmatter_name("no frontmatter here"), None);
    }
}
