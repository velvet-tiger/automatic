use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

use super::types::SkillsJson;
use super::*;

// ── Skills Store (skills.sh) ─────────────────────────────────────────────────

/// A skill result from the skills.sh search API.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RemoteSkillResult {
    /// Full slug: "owner/repo/skill-name" — used to build the skills.sh URL.
    pub id: String,
    /// The skill name (e.g. "vercel-react-best-practices").
    pub name: String,
    /// Number of times installed across the ecosystem.
    pub installs: u64,
    /// The GitHub source in "owner/repo" format.
    pub source: String,
}

/// Search skills.sh for skills matching `query`.
/// Calls `https://skills.sh/api/search?q=<query>&limit=20`.
pub async fn search_remote_skills(query: &str) -> Result<Vec<RemoteSkillResult>, String> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }

    let url = format!(
        "https://skills.sh/api/search?q={}&limit=20",
        urlencoding::encode(query)
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .get(&url)
        .header("User-Agent", "automatic-desktop/1.0")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("skills.sh returned status {}", resp.status()));
    }

    #[derive(Deserialize)]
    struct ApiResponse {
        skills: Vec<ApiSkill>,
    }

    #[derive(Deserialize)]
    struct ApiSkill {
        id: String,
        name: String,
        installs: u64,
        source: String,
    }

    let body: ApiResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(body
        .skills
        .into_iter()
        .map(|s| RemoteSkillResult {
            id: s.id,
            name: s.name,
            installs: s.installs,
            source: s.source,
        })
        .collect())
}

/// Extract the value of a named YAML frontmatter field from raw SKILL.md text.
/// Handles the `---\nkey: value\n---` block at the top of the file.
/// Only handles simple scalar values (not block scalars or nested YAML).
fn extract_frontmatter_field(content: &str, field: &str) -> Option<String> {
    let inner = content
        .strip_prefix("---")?
        .trim_start_matches('\n')
        .trim_start_matches('\r');
    let end = inner.find("\n---")?;
    let prefix = format!("{}:", field);
    for line in inner[..end].lines() {
        if let Some(rest) = line.strip_prefix(&*prefix) {
            let val = rest.trim().trim_matches('"').trim_matches('\'');
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
    }
    None
}

/// Convenience wrapper — extracts the `name:` frontmatter field.
fn extract_frontmatter_name(content: &str) -> Option<String> {
    extract_frontmatter_field(content, "name")
}

/// Extracts the `license:` frontmatter field from a SKILL.md.
pub fn extract_frontmatter_license(content: &str) -> Option<String> {
    extract_frontmatter_field(content, "license")
}

/// Blobless shallow clone of `source` (owner/repo) followed by a local tree
/// walk to find every `SKILL.md` path in the repository. Used as a
/// last-resort discovery mechanism when a skill isn't found at any of the
/// known static layouts and there's no `skill.json` manifest to consult —
/// e.g. "collection" repos that just have `skills/<name>/SKILL.md`
/// directories with no manifest at all. Returns the list of `SKILL.md`
/// paths (relative to the repo root) and the branch that was checked out.
fn clone_and_list_skill_md_paths(source: &str) -> Result<(Vec<String>, String), String> {
    let tmp_dir = std::env::temp_dir().join(format!(
        "automatic-skill-scan-{}-{}",
        source.replace('/', "-"),
        std::process::id()
    ));
    // Clean up any leftover from a previous failed attempt.
    let _ = std::fs::remove_dir_all(&tmp_dir);

    let clone_url = format!("https://github.com/{}.git", source);
    let clone_result = std::process::Command::new("git")
        .args([
            "clone",
            "--depth",
            "1",
            "--filter=blob:none",
            "--no-checkout",
            "--quiet",
            &clone_url,
            tmp_dir.to_str().unwrap_or(""),
        ])
        .output();

    let clone_ok = match &clone_result {
        Ok(out) => out.status.success(),
        Err(_) => false,
    };

    if !clone_ok {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(format!(
            "Could not clone '{}': git clone failed (is git installed?)",
            source
        ));
    }

    // Get the flat file list from the local clone.
    let ls_result = std::process::Command::new("git")
        .args([
            "-C",
            tmp_dir.to_str().unwrap_or(""),
            "ls-tree",
            "-r",
            "--name-only",
            "HEAD",
        ])
        .output();

    // Get the actual branch name so we can build a raw.githubusercontent.com URL.
    let branch_result = std::process::Command::new("git")
        .args([
            "-C",
            tmp_dir.to_str().unwrap_or(""),
            "rev-parse",
            "--abbrev-ref",
            "HEAD",
        ])
        .output();

    let _ = std::fs::remove_dir_all(&tmp_dir);

    let ls_output = match ls_result {
        Ok(out) if out.status.success() => out.stdout,
        _ => {
            return Err(format!(
                "Could not list files in cloned repo for '{}'",
                source
            ))
        }
    };

    let branch = match branch_result {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => "main".to_string(),
    };

    let file_list = String::from_utf8_lossy(&ls_output);
    let paths: Vec<String> = file_list
        .lines()
        .filter(|p| p.ends_with("/SKILL.md") || *p == "SKILL.md")
        .map(|p| p.to_string())
        .collect();

    Ok((paths, branch))
}

/// Fetch the SKILL.md content for a remote skill by constructing the GitHub
/// raw content URL from the skill's `source` ("owner/repo") and `name`.
///
/// The canonical skill name is defined by the `name:` field in the SKILL.md
/// frontmatter — it may differ from both the registry ID and the directory
/// name (e.g. dir "react-best-practices" has frontmatter `name: vercel-react-best-practices`).
///
/// Strategy:
/// 1. Try obvious static paths against `main` then `master` via raw.githubusercontent.com
///    (no API calls, covers the majority of repos).
/// 2. If nothing matched, do a blobless shallow git clone
///    (`git clone --depth 1 --filter=blob:none --no-checkout`) into a temp dir,
///    run `git ls-tree -r --name-only HEAD` to get a flat file listing, find the
///    matching SKILL.md path, then fetch that file via raw.githubusercontent.com.
///    This handles arbitrary repo layouts (e.g. hashicorp/agent-skills, wshobson/agents)
///    with no GitHub API calls and no rate-limit exposure. The blobless clone
///    downloads only git metadata (~100-200 KB), not file contents.
pub async fn fetch_remote_skill_content(source: &str, name: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    // ── Step 1: static candidates fired in parallel ───────────────────────────
    // All candidate URLs (5 layouts × 2 branch names) are fetched
    // concurrently. The first one that returns a matching SKILL.md wins.
    // raw.githubusercontent.com is unauthenticated and not rate-limited.
    let static_urls: Vec<String> = ["main", "master"]
        .iter()
        .flat_map(|branch| {
            let base = format!("https://raw.githubusercontent.com/{}/{}", source, branch);
            vec![
                // Dedicated skill repo layout (e.g. vercel-labs/agent-skills)
                format!("{}/skills/{}/SKILL.md", base, name),
                // agentskills.io standard install path (npx skills add)
                format!("{}/.agents/skills/{}/SKILL.md", base, name),
                // Claude Code install path
                format!("{}/.claude/skills/{}/SKILL.md", base, name),
                // Flat layout
                format!("{}/{}/SKILL.md", base, name),
                // Single-skill repo
                format!("{}/SKILL.md", base),
            ]
        })
        .collect();

    let mut tasks = tokio::task::JoinSet::new();
    for url in static_urls {
        let client2 = client.clone();
        let name2 = name.to_string();
        tasks.spawn(async move {
            let resp = client2
                .get(&url)
                .header("User-Agent", "automatic-desktop/1.0")
                .send()
                .await
                .ok()?;
            if !resp.status().is_success() {
                return None;
            }
            let content = resp.text().await.ok()?;
            match extract_frontmatter_name(&content) {
                Some(ref n) if n == &name2 => Some(content),
                None => Some(content),
                _ => None,
            }
        });
    }

    while let Some(result) = tasks.join_next().await {
        if let Ok(Some(content)) = result {
            tasks.abort_all();
            return Ok(content);
        }
    }

    // ── Step 1b: skill.json at repo root ─────────────────────────────────────
    // Try fetching skill.json from the well-known repo root for main/master.
    // This is faster than a git clone and covers repos that publish
    // skill.json package metadata per the velvet-tiger/skills-json spec.
    for branch in &["main", "master"] {
        let skills_json_url = format!(
            "https://raw.githubusercontent.com/{}/{}/skill.json",
            source, branch
        );

        let skills_json_resp = client
            .get(&skills_json_url)
            .header("User-Agent", "automatic-desktop/1.0")
            .send()
            .await;

        let skills_json_text = match skills_json_resp {
            Ok(r) if r.status().is_success() => match r.text().await {
                Ok(t) => t,
                Err(_) => continue,
            },
            _ => continue,
        };

        let manifest: SkillsJson = match serde_json::from_str(&skills_json_text) {
            Ok(m) => m,
            Err(_) => continue,
        };

        // Find the matching skill entry by name
        let skill_entry = manifest.skills.iter().find(|s| s.name == name);
        let skill_entry = match skill_entry {
            Some(e) => e,
            None => continue,
        };

        // Resolve the SKILL.md (or custom entrypoint) path from skill.json
        let entrypoint = skill_entry.entrypoint_file();
        let skill_path = if skill_entry.path == "." || skill_entry.path.is_empty() {
            entrypoint.to_string()
        } else {
            let p = skill_entry.path.trim_start_matches("./");
            format!("{}/{}", p, entrypoint)
        };

        let skill_url = format!(
            "https://raw.githubusercontent.com/{}/{}/{}",
            source, branch, skill_path
        );

        let skill_resp = client
            .get(&skill_url)
            .header("User-Agent", "automatic-desktop/1.0")
            .send()
            .await;

        let content = match skill_resp {
            Ok(r) if r.status().is_success() => match r.text().await {
                Ok(t) => t,
                Err(_) => continue,
            },
            _ => continue,
        };

        // Validate: frontmatter name must match or be absent
        match extract_frontmatter_name(&content) {
            Some(ref n) if n == name => return Ok(content),
            None => return Ok(content),
            _ => {}
        }
    }

    // ── Step 2: blobless shallow clone + local tree walk ─────────────────────
    // Clone only the git metadata (no file blobs). This is ~100-200 KB and
    // takes under a second. No GitHub API involved — no rate limit.
    let (all_paths, branch) = clone_and_list_skill_md_paths(source)?;
    let raw_base = format!("https://raw.githubusercontent.com/{}/{}", source, branch);

    // Find ALL SKILL.md files in the tree.  The directory name may differ
    // from the skills.sh name (e.g. dir "react-best-practices" with
    // frontmatter `name: vercel-react-best-practices`), so we collect every
    // SKILL.md and rely on the frontmatter check below to identify the
    // correct one.
    let mut candidate_paths: Vec<&str> = all_paths.iter().map(|s| s.as_str()).collect();

    // Try exact directory-name matches first (fast path), then everything
    // else.  Within each tier the original tree order is preserved.
    candidate_paths.sort_by_key(|p| {
        let parent = std::path::Path::new(p)
            .parent()
            .and_then(|d| d.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if parent == name {
            0usize
        } else {
            1usize
        }
    });

    for path in candidate_paths {
        let url = format!("{}/{}", raw_base, path);
        let resp = match client
            .get(&url)
            .header("User-Agent", "automatic-desktop/1.0")
            .send()
            .await
        {
            Ok(r) => r,
            Err(_) => continue,
        };
        if !resp.status().is_success() {
            continue;
        }
        let content = match resp.text().await {
            Ok(t) => t,
            Err(_) => continue,
        };
        // The frontmatter `name:` field is authoritative when present.
        // When absent, only accept the file if the directory name matches
        // the requested skill name (or it's the repo root SKILL.md for a
        // single-skill repo).  This prevents false positives in multi-skill
        // repos where a different skill's SKILL.md lacks frontmatter.
        let dir_matches = std::path::Path::new(path)
            .parent()
            .and_then(|d| d.file_name())
            .and_then(|n| n.to_str())
            .map_or(false, |p| p == name);
        match extract_frontmatter_name(&content) {
            Some(ref n) if n == name => return Ok(content),
            None if dir_matches || path == "SKILL.md" => return Ok(content),
            _ => {}
        }
    }

    Err(format!("Could not fetch SKILL.md for '{}'", name))
}

/// Best-effort lookup of the publisher-side version for a single skill in a
/// GitHub source. Tries `main` then `master` for `skill.json` at the repo
/// root, parses the manifest, finds the entry whose `name` matches `name`,
/// and returns its `version` (falling back to the package-level `version` if
/// the skill entry has no override).
///
/// Returns `Ok(None)` when no `skill.json` exists, the network call fails,
/// or no matching skill entry is present. This is intentionally tolerant —
/// it is used to enrich install metadata, not to validate the import.
pub async fn fetch_remote_skill_version(source: &str, name: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;

    for branch in &["main", "master"] {
        let url = format!(
            "https://raw.githubusercontent.com/{}/{}/skill.json",
            source, branch
        );
        let resp = match client
            .get(&url)
            .header("User-Agent", "automatic-desktop/1.0")
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => r,
            _ => continue,
        };
        let text = match resp.text().await {
            Ok(t) => t,
            Err(_) => continue,
        };
        let manifest: SkillsJson = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if let Some(entry) = manifest.skills.iter().find(|s| s.name == name) {
            return entry
                .version
                .clone()
                .or_else(|| Some(manifest.version.clone()));
        }
    }

    None
}

// ── Skills Registry (~/.automatic/skills.json) ───────────────────────────────────
//
// Tracks the remote origin of skills imported from skills.sh.
// Local skills (not imported) simply have no entry in this file.
//
// Format:
//   {
//     "skill-name": { "source": "owner/repo", "id": "owner/repo/skill-name" },
//     ...
//   }

fn get_skills_registry_path() -> Result<PathBuf, String> {
    Ok(super::paths::get_automatic_dir()?.join("skills.json"))
}

/// Read the full registry.  Returns an empty map if the file doesn't exist.
pub fn read_skill_sources() -> Result<std::collections::HashMap<String, SkillSource>, String> {
    let path = get_skills_registry_path()?;
    if !path.exists() {
        return Ok(std::collections::HashMap::new());
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| format!("Invalid skills.json: {}", e))
}

/// Write the full registry atomically.
fn write_skill_sources(
    registry: &std::collections::HashMap<String, SkillSource>,
) -> Result<(), String> {
    let path = get_skills_registry_path()?;
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let json = serde_json::to_string_pretty(registry).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}

/// Record that a skill was imported from a remote source, or is bundled with
/// the app.  `kind` is "github" for registry-imported skills, "bundled" for
/// skills shipped with Automatic.
///
/// Existing call sites that do not have install metadata (e.g. bundled skills,
/// project imports, drift-recovery paths) continue to use this entry point;
/// new install paths should use `record_skill_source_with_meta` so the
/// "is this skill out of date?" check has something to compare against.
pub fn record_skill_source(name: &str, source: &str, id: &str, kind: &str) -> Result<(), String> {
    record_skill_source_with_meta(name, source, id, kind, None, None, None)
}

/// Variant of `record_skill_source` that also persists install metadata used
/// by the update-check feature: a SHA256 of the content as fetched, the
/// publisher-side version from `skill.json` if present, and an ISO 8601
/// timestamp marking when the record was written.
pub fn record_skill_source_with_meta(
    name: &str,
    source: &str,
    id: &str,
    kind: &str,
    installed_sha: Option<String>,
    installed_version: Option<String>,
    installed_at: Option<String>,
) -> Result<(), String> {
    let mut registry = read_skill_sources()?;
    registry.insert(
        name.to_string(),
        SkillSource {
            source: source.to_string(),
            id: id.to_string(),
            kind: kind.to_string(),
            installed_sha,
            installed_version,
            installed_at,
        },
    );
    write_skill_sources(&registry)
}

/// Hex-encoded SHA256 of the given content. Used to capture the canonical
/// state of an imported skill so we can later distinguish "no change",
/// "upstream changed", and "locally edited".
pub fn sha256_hex(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Current ISO 8601 timestamp (UTC) suitable for `installed_at`.
pub fn now_iso8601() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Remove the remote origin record for a skill (called on delete).
pub fn remove_skill_source(name: &str) -> Result<(), String> {
    let mut registry = read_skill_sources()?;
    registry.remove(name);
    write_skill_sources(&registry)
}

// ── Update check ────────────────────────────────────────────────────────────

/// Outcome of comparing a locally-installed skill against its GitHub source.
///
/// The `status` string is the part the UI keys off; the other fields are
/// supporting detail for the badge / tooltip:
///
/// - `up_to_date` — local content matches the install SHA and matches the
///   remote SHA (or, when install SHA is missing, local matches remote).
/// - `update_available` — the remote SKILL.md differs from what we recorded
///   at install time, and the user has not edited the local copy.
/// - `local_modified` — the user has edited the local copy since import.
///   We do not surface "and the remote also moved" yet because the UI just
///   needs to tell the user "your copy diverges from the source".
/// - `unknown` — we couldn't tell. The `reason` field explains why
///   (e.g. bundled skill, not a remote skill, network failure, ambiguous
///   remote content). The UI should hide the badge in this case.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkillUpdateStatus {
    /// One of "up_to_date", "update_available", "local_modified", "unknown".
    pub status: String,
    /// The hex SHA recorded at install time (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_sha: Option<String>,
    /// The hex SHA of the current local SKILL.md.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_sha: Option<String>,
    /// The hex SHA of the current remote SKILL.md.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_sha: Option<String>,
    /// Version recorded at install time (from `skill.json`, if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_version: Option<String>,
    /// Version reported by the current `skill.json` at the remote.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_version: Option<String>,
    /// When this skill was last imported / refreshed locally.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_at: Option<String>,
    /// Free-text explanation, shown when `status == "unknown"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl SkillUpdateStatus {
    fn unknown(reason: impl Into<String>) -> Self {
        SkillUpdateStatus {
            status: "unknown".into(),
            installed_sha: None,
            local_sha: None,
            remote_sha: None,
            installed_version: None,
            remote_version: None,
            installed_at: None,
            reason: Some(reason.into()),
        }
    }
}

/// Compare a locally-installed skill against its GitHub origin.
///
/// We only support `kind == "github"` here: bundled skills are versioned with
/// the app binary and "updates" are not a meaningful concept for them.
///
/// Logic:
/// 1. Read local SKILL.md and the recorded `SkillSource`.
/// 2. If the skill has no source, or `kind != "github"`, return `unknown`.
/// 3. Fetch the remote SKILL.md and (optionally) `skill.json` version.
/// 4. Compare:
///    - If `installed_sha` is present and matches local but differs from
///      remote → `update_available`.
///    - If `installed_sha` is present and local differs from `installed_sha`
///      → `local_modified` (the user has edited the file in the library).
///    - If `installed_sha` is absent (skill was imported before this feature
///      shipped), fall back to comparing local SHA against remote SHA — this
///      cannot distinguish a local edit from an upstream change, so a "diff"
///      result is reported as `update_available`. The frontend should make
///      this caveat visible.
pub async fn check_skill_update(name: &str) -> Result<SkillUpdateStatus, String> {
    let registry = read_skill_sources()?;
    let source = match registry.get(name) {
        Some(s) => s.clone(),
        None => return Ok(SkillUpdateStatus::unknown("Skill has no recorded source.")),
    };

    if source.kind != "github" {
        return Ok(SkillUpdateStatus::unknown(format!(
            "Skill source kind '{}' does not support update checks.",
            source.kind
        )));
    }

    let local_content = super::read_skill(name)
        .map_err(|e| format!("Failed to read local skill '{}': {}", name, e))?;
    let local_sha = sha256_hex(&local_content);

    // Network call. Bubble up the error so the UI can show "Couldn't check
    // for updates" instead of silently claiming the skill is up to date.
    let remote_content = fetch_remote_skill_content(&source.source, name)
        .await
        .map_err(|e| format!("Could not fetch remote skill: {}", e))?;
    let remote_sha = sha256_hex(&remote_content);
    let remote_version = fetch_remote_skill_version(&source.source, name).await;

    let status = if let Some(installed_sha) = source.installed_sha.as_deref() {
        if local_sha == installed_sha && remote_sha == installed_sha {
            "up_to_date"
        } else if local_sha == installed_sha && remote_sha != installed_sha {
            "update_available"
        } else if local_sha != installed_sha {
            // User-edited copy. We do not currently flag a concurrent
            // upstream update here — the user has already diverged, so
            // "your copy is modified" is the more important signal.
            "local_modified"
        } else {
            "unknown"
        }
    } else if local_sha == remote_sha {
        "up_to_date"
    } else {
        "update_available"
    };

    Ok(SkillUpdateStatus {
        status: status.to_string(),
        installed_sha: source.installed_sha,
        local_sha: Some(local_sha),
        remote_sha: Some(remote_sha),
        installed_version: source.installed_version,
        remote_version,
        installed_at: source.installed_at,
        reason: None,
    })
}

/// Re-fetch a remote skill from its recorded source and overwrite the local
/// copy.  Refreshes install metadata so a subsequent update check will see
/// the new SHA / version / timestamp.
///
/// This is the "Update Now" entry point.  It is intentionally idempotent —
/// running it on a skill that is already up to date will simply rewrite the
/// same content and bump `installed_at`.  That is by design: skill
/// versioning at the publisher level is still maturing and the user may
/// want to force a re-fetch (e.g. after a publisher rewrote the upstream
/// without bumping the version field in `skill.json`).
///
/// Returns the freshly-computed `SkillUpdateStatus` so the UI can refresh
/// its badge without making a second network round-trip.
pub async fn update_skill_from_source(name: &str) -> Result<SkillUpdateStatus, String> {
    let registry = read_skill_sources()?;
    let source = registry
        .get(name)
        .cloned()
        .ok_or_else(|| format!("Skill '{}' has no recorded source.", name))?;

    if source.kind != "github" {
        return Err(format!(
            "Skill source kind '{}' does not support remote updates.",
            source.kind
        ));
    }

    let remote_content = fetch_remote_skill_content(&source.source, name)
        .await
        .map_err(|e| format!("Could not fetch remote skill: {}", e))?;

    super::save_skill(name, &remote_content)
        .map_err(|e| format!("Failed to save updated skill: {}", e))?;

    let remote_sha = sha256_hex(&remote_content);
    let remote_version = fetch_remote_skill_version(&source.source, name).await;
    let installed_at = now_iso8601();

    record_skill_source_with_meta(
        name,
        &source.source,
        &source.id,
        "github",
        Some(remote_sha.clone()),
        remote_version.clone(),
        Some(installed_at.clone()),
    )?;

    Ok(SkillUpdateStatus {
        status: "up_to_date".to_string(),
        installed_sha: Some(remote_sha.clone()),
        local_sha: Some(remote_sha.clone()),
        remote_sha: Some(remote_sha),
        installed_version: remote_version.clone(),
        remote_version,
        installed_at: Some(installed_at),
        reason: None,
    })
}

// ── Repository Import ───────────────────────────────────────────────────────────

/// Parse a GitHub repository URL and extract the owner/repo pair.
/// Supports: https://github.com/owner/repo, github.com/owner/repo, owner/repo
fn parse_github_url(url: &str) -> Result<(String, String), String> {
    let url = url.trim();

    // Remove trailing .git if present
    let url = url.trim_end_matches(".git");

    // Remove trailing slashes
    let url = url.trim_end_matches('/');

    // Try to parse as full URL
    if url.starts_with("https://github.com/") || url.starts_with("http://github.com/") {
        let rest = url
            .strip_prefix("https://github.com/")
            .or_else(|| url.strip_prefix("http://github.com/"))
            .ok_or("Invalid GitHub URL")?;

        let parts: Vec<&str> = rest.split('/').collect();
        if parts.len() < 2 {
            return Err("GitHub URL must include owner and repository name".to_string());
        }

        return Ok((parts[0].to_string(), parts[1].to_string()));
    }

    // Try without protocol prefix
    if url.starts_with("github.com/") {
        let rest = url
            .strip_prefix("github.com/")
            .ok_or("Invalid GitHub URL")?;
        let parts: Vec<&str> = rest.split('/').collect();
        if parts.len() < 2 {
            return Err("GitHub URL must include owner and repository name".to_string());
        }

        return Ok((parts[0].to_string(), parts[1].to_string()));
    }

    // Try owner/repo shorthand
    let parts: Vec<&str> = url.split('/').collect();
    if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
        return Ok((parts[0].to_string(), parts[1].to_string()));
    }

    Err(
        "Invalid GitHub URL format. Expected: https://github.com/owner/repo or owner/repo"
            .to_string(),
    )
}

/// Import skills from a GitHub repository URL.
///
/// Accepts URLs in the following formats:
/// - https://github.com/owner/repo
/// - github.com/owner/repo
/// - owner/repo
///
/// For multi-skill repos with a skill.json manifest, imports all listed skills.
/// Returns a list of all imported skills.
pub async fn import_skill_from_repository(
    repo_url: &str,
    skill_name: Option<&str>,
) -> Result<Vec<ImportedSkillFromRepo>, String> {
    let (owner, repo) = parse_github_url(repo_url)?;
    let source = format!("{}/{}", owner, repo);

    // If skill_name is provided, try to fetch that specific skill
    // Otherwise, try common skill names derived from repo name
    let names_to_try = if let Some(name) = skill_name {
        vec![name.to_string()]
    } else {
        // Try repo name as-is, then lowercased, then kebab-cased variations
        let repo_lower = repo.to_lowercase();
        let repo_kebab = repo_lower.replace('_', "-");
        vec![repo.clone(), repo_lower.clone(), repo_kebab.clone()]
    };

    let mut last_error: Option<String> = None;

    for name in names_to_try {
        match fetch_remote_skill_content(&source, &name).await {
            Ok(content) => {
                let actual_name =
                    extract_frontmatter_name(&content).unwrap_or_else(|| name.clone());

                let was_updated = super::skill_exists(&actual_name);
                super::save_skill(&actual_name, &content)?;

                let id = format!("{}/{}", source, actual_name);
                let installed_sha = Some(sha256_hex(&content));
                let installed_version = fetch_remote_skill_version(&source, &actual_name).await;
                let installed_at = Some(now_iso8601());
                record_skill_source_with_meta(
                    &actual_name,
                    &source,
                    &id,
                    "github",
                    installed_sha,
                    installed_version,
                    installed_at,
                )?;
                let _ = super::set_skill_collection(&actual_name, &source);

                return Ok(vec![ImportedSkillFromRepo {
                    name: actual_name,
                    source,
                    id,
                    was_updated,
                }]);
            }
            Err(e) => {
                last_error = Some(e);
            }
        }
    }

    // If no skill found with derived names, try to discover skills via skill.json
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    for branch in &["main", "master"] {
        let skills_json_url = format!(
            "https://raw.githubusercontent.com/{}/{}/skill.json",
            source, branch
        );

        let resp = client
            .get(&skills_json_url)
            .header("User-Agent", "automatic-desktop/1.0")
            .send()
            .await;

        let text = match resp {
            Ok(r) if r.status().is_success() => match r.text().await {
                Ok(t) => t,
                Err(_) => continue,
            },
            _ => continue,
        };

        let manifest: super::types::SkillsJson = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(_) => continue,
        };

        if manifest.skills.is_empty() {
            continue;
        }

        // Import ALL skills from the manifest, not just the first one
        let mut imported = Vec::new();
        for skill_entry in &manifest.skills {
            let name = &skill_entry.name;
            // Per-skill version override falls back to the package-level
            // version per the skills-json spec.
            let manifest_version = skill_entry
                .version
                .clone()
                .or_else(|| Some(manifest.version.clone()));
            match fetch_remote_skill_content(&source, name).await {
                Ok(content) => {
                    let actual_name =
                        extract_frontmatter_name(&content).unwrap_or_else(|| name.clone());

                    let was_updated = super::skill_exists(&actual_name);
                    if let Err(e) = super::save_skill(&actual_name, &content) {
                        eprintln!("[automatic] Failed to save skill '{}': {}", actual_name, e);
                        continue;
                    }

                    let id = format!("{}/{}", source, actual_name);
                    let _ = record_skill_source_with_meta(
                        &actual_name,
                        &source,
                        &id,
                        "github",
                        Some(sha256_hex(&content)),
                        manifest_version.clone(),
                        Some(now_iso8601()),
                    );
                    let _ = super::set_skill_collection(&actual_name, &source);

                    imported.push(ImportedSkillFromRepo {
                        name: actual_name,
                        source: source.clone(),
                        id,
                        was_updated,
                    });
                }
                Err(e) => {
                    eprintln!("[automatic] Failed to fetch skill '{}': {}", name, e);
                    last_error = Some(e);
                }
            }
        }

        if !imported.is_empty() {
            return Ok(imported);
        }
    }

    // ── Fallback: no name match, no skill.json — walk the full repo tree ────
    // Handles "collection" repos that just have `skills/<name>/SKILL.md`
    // directories with no manifest at all (e.g. repos built around Claude
    // Code's plugin-marketplace convention, which ships a
    // `.claude-plugin/marketplace.json` instead of a `skill.json`). Every
    // SKILL.md found is imported; each still goes through the same
    // asset-security scan as any other import, via `save_skill`.
    if let Ok((paths, branch)) = clone_and_list_skill_md_paths(&source) {
        if !paths.is_empty() {
            let raw_base = format!("https://raw.githubusercontent.com/{}/{}", source, branch);
            let mut imported = Vec::new();

            for path in &paths {
                let url = format!("{}/{}", raw_base, path);
                let content = match client
                    .get(&url)
                    .header("User-Agent", "automatic-desktop/1.0")
                    .send()
                    .await
                {
                    Ok(resp) if resp.status().is_success() => match resp.text().await {
                        Ok(t) => t,
                        Err(_) => continue,
                    },
                    _ => continue,
                };

                let dir_name = std::path::Path::new(path)
                    .parent()
                    .and_then(|d| d.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("skill");
                let actual_name =
                    extract_frontmatter_name(&content).unwrap_or_else(|| dir_name.to_string());

                let was_updated = super::skill_exists(&actual_name);
                if let Err(e) = super::save_skill(&actual_name, &content) {
                    eprintln!("[automatic] Failed to save skill '{}': {}", actual_name, e);
                    continue;
                }

                let id = format!("{}/{}", source, actual_name);
                let _ = record_skill_source_with_meta(
                    &actual_name,
                    &source,
                    &id,
                    "github",
                    Some(sha256_hex(&content)),
                    None,
                    Some(now_iso8601()),
                );
                let _ = super::set_skill_collection(&actual_name, &source);

                imported.push(ImportedSkillFromRepo {
                    name: actual_name,
                    source: source.clone(),
                    id,
                    was_updated,
                });
            }

            if !imported.is_empty() {
                return Ok(imported);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| {
        format!(
            "No skills found in repository '{}'. Make sure the repository contains a SKILL.md file or skill.json manifest.",
            source
        )
    }))
}

/// Result of importing a skill from a repository.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ImportedSkillFromRepo {
    pub name: String,
    pub source: String,
    pub id: String,
    /// True when the library already contained a skill with this name and
    /// the import overwrote it. False when this import created the skill
    /// for the first time. Lets the UI show "added" vs "updated".
    #[serde(default)]
    pub was_updated: bool,
}
