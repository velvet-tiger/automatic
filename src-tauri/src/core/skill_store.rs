use serde::{Deserialize, Serialize};
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
    let tmp_dir = std::env::temp_dir().join(format!(
        "automatic-skill-{}-{}",
        source.replace('/', "-"),
        name
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
            "Could not fetch SKILL.md for '{}': git clone failed (is git installed?)",
            name
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
                name
            ))
        }
    };

    let branch = match branch_result {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => "main".to_string(),
    };

    let file_list = String::from_utf8_lossy(&ls_output);
    let raw_base = format!("https://raw.githubusercontent.com/{}/{}", source, branch);

    // Find ALL SKILL.md files in the tree.  The directory name may differ
    // from the skills.sh name (e.g. dir "react-best-practices" with
    // frontmatter `name: vercel-react-best-practices`), so we collect every
    // SKILL.md and rely on the frontmatter check below to identify the
    // correct one.
    let mut candidate_paths: Vec<&str> = file_list
        .lines()
        .filter(|p| p.ends_with("/SKILL.md") || *p == "SKILL.md")
        .collect();

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
pub fn record_skill_source(name: &str, source: &str, id: &str, kind: &str) -> Result<(), String> {
    let mut registry = read_skill_sources()?;
    registry.insert(
        name.to_string(),
        SkillSource {
            source: source.to_string(),
            id: id.to_string(),
            kind: kind.to_string(),
        },
    );
    write_skill_sources(&registry)
}

/// Remove the remote origin record for a skill (called on delete).
pub fn remove_skill_source(name: &str) -> Result<(), String> {
    let mut registry = read_skill_sources()?;
    registry.remove(name);
    write_skill_sources(&registry)
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
        if parts.len() < 2 {return Err("GitHub URL must include owner and repository name".to_string());
        }

        return Ok((parts[0].to_string(), parts[1].to_string()));
    }

    // Try without protocol prefix
    if url.starts_with("github.com/") {
        let rest = url.strip_prefix("github.com/").ok_or("Invalid GitHub URL")?;
        let parts: Vec<&str> = rest.split('/').collect();
        if parts.len() < 2 {
            return Err("GitHub URL must include owner and repository name".to_string());}

        return Ok((parts[0].to_string(), parts[1].to_string()));
    }

    // Try owner/repo shorthand
    let parts: Vec<&str> = url.split('/').collect();
    if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
        return Ok((parts[0].to_string(), parts[1].to_string()));
    }

    Err("Invalid GitHub URL format. Expected: https://github.com/owner/repo or owner/repo".to_string())
}

/// Import a skill from a GitHub repository URL.
///
/// Accepts URLs in the following formats:
/// - https://github.com/owner/repo
/// - github.com/owner/repo
/// - owner/repo
///
/// For multi-skill repos, this imports the first SKILL.md found.
/// Returns the imported skill name and source info.
pub async fn import_skill_from_repository(
    repo_url: &str,
    skill_name: Option<&str>,
) -> Result<ImportedSkillFromRepo, String> {
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
        vec![
            repo.clone(),
            repo_lower.clone(),
            repo_kebab.clone(),
        ]
    };

    let mut last_error: Option<String> = None;

    for name in names_to_try {
        match fetch_remote_skill_content(&source, &name).await {
            Ok(content) => {
                // Determine the actual skill name from frontmatter
                let actual_name = extract_frontmatter_name(&content)
                    .unwrap_or_else(|| name.clone());

                // Save the skill
                super::save_skill(&actual_name, &content)?;

                // Record the source
                let id = format!("{}/{}", source, actual_name);
                record_skill_source(&actual_name, &source, &id, "github")?;

                // Auto-assign to a collection named after the repo
                let _ = super::set_skill_collection(&actual_name, &source);

                return Ok(ImportedSkillFromRepo {
                    name: actual_name,
                    source,
                    id,
                });
            }
            Err(e) => {
                last_error = Some(e);
            }
        }
    }

    // If no skill found with derived names, try to discover skills via skill.json
    let skills_json_url = format!("https://raw.githubusercontent.com/{}/main/skill.json", source);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .get(&skills_json_url)
        .header("User-Agent", "automatic-desktop/1.0")
        .send()
        .await;

    if let Ok(resp) = resp {
        if resp.status().is_success() {
            if let Ok(text) = resp.text().await {
                if let Ok(manifest) = serde_json::from_str::<super::types::SkillsJson>(&text) {
                    if let Some(first_skill) = manifest.skills.first() {
                        let name = &first_skill.name;
                        match fetch_remote_skill_content(&source, name).await {
                            Ok(content) => {
                                let actual_name = extract_frontmatter_name(&content)
                                    .unwrap_or_else(|| name.clone());

                                super::save_skill(&actual_name, &content)?;

                                let id = format!("{}/{}", source, actual_name);
                                record_skill_source(&actual_name, &source, &id, "github")?;

                                return Ok(ImportedSkillFromRepo {
                                    name: actual_name,
                                    source,
                                    id,
                                });
                            }
                            Err(e) => {
                                last_error = Some(e);
                            }
                        }
                    }
                }
            }
        }
    }

    // Try master branch if main failed
    let skills_json_url_master = format!("https://raw.githubusercontent.com/{}/master/skill.json", source);
    let resp_master = client
        .get(&skills_json_url_master)
        .header("User-Agent", "automatic-desktop/1.0")
        .send()
        .await;

    if let Ok(resp) = resp_master {
        if resp.status().is_success() {
            if let Ok(text) = resp.text().await {
                if let Ok(manifest) = serde_json::from_str::<super::types::SkillsJson>(&text) {
                    if let Some(first_skill) = manifest.skills.first() {
                        let name = &first_skill.name;
                        match fetch_remote_skill_content(&source, name).await {
                            Ok(content) => {
                                let actual_name = extract_frontmatter_name(&content)
                                    .unwrap_or_else(|| name.clone());

                                super::save_skill(&actual_name, &content)?;

                                let id = format!("{}/{}", source, actual_name);
                                record_skill_source(&actual_name, &source, &id, "github")?;

                                return Ok(ImportedSkillFromRepo {
                                    name: actual_name,
                                    source,
                                    id,
                                });
                            }
                            Err(e) => {
                                last_error = Some(e);
                            }
                        }
                    }
                }
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
}
