use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

// ── Path Helpers ─────────────────────────────────────────────────────────────

/// Primary skills directory — the agentskills.io standard location.
pub fn get_agents_skills_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".agents/skills"))
}

/// Secondary skills directory — Claude Code's location.
pub fn get_claude_skills_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".claude/skills"))
}

pub fn get_projects_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".automatic/projects"))
}

pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && !name.contains('\\') && name != "." && name != ".."
}

// ── Data Structures ──────────────────────────────────────────────────────────

/// Remote origin of a skill imported from skills.sh.
/// Stored in ~/.automatic/skills.json keyed by skill name.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkillSource {
    /// GitHub owner/repo, e.g. "vercel-labs/skills"
    pub source: String,
    /// Full skills.sh id, e.g. "vercel-labs/skills/find-skills"
    pub id: String,
}

/// A skill entry with its name and which global directories it exists in.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkillEntry {
    pub name: String,
    /// Exists in `~/.agents/skills/` (agentskills.io standard)
    pub in_agents: bool,
    /// Exists in `~/.claude/skills/` (Claude Code)
    pub in_claude: bool,
    /// Remote origin from ~/.automatic/skills.json, if this was imported from skills.sh
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SkillSource>,
    /// True if the skill directory contains any files or subdirectories besides SKILL.md
    #[serde(default)]
    pub has_resources: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Project {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub directory: String,
    #[serde(default)]
    pub skills: Vec<String>,
    /// Skills that exist only in the project directory, not in the global
    /// registry.  Discovered during autodetection but never auto-imported.
    #[serde(default)]
    pub local_skills: Vec<String>,
    #[serde(default)]
    pub mcp_servers: Vec<String>,
    #[serde(default)]
    pub providers: Vec<String>,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    /// Clerk user ID of the user who created this project.  Populated by the
    /// frontend from the useProfile hook.  Used for future team/cloud sync.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    /// Rules attached to each project instruction file.  Maps filename
    /// (e.g. "CLAUDE.md") to an ordered list of rule names whose content is
    /// appended below the user-authored content when the file is written.
    /// In unified mode the key `"_unified"` is used for all files.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub file_rules: std::collections::HashMap<String, Vec<String>>,
    /// `"unified"` — one set of instructions written to all agent files.
    /// `"per-agent"` (default) — each agent file is edited independently.
    #[serde(default = "default_instruction_mode")]
    pub instruction_mode: String,
}

fn default_instruction_mode() -> String {
    "per-agent".to_string()
}

// ── API Keys ─────────────────────────────────────────────────────────────────

pub fn save_api_key(provider: &str, key: &str) -> Result<(), String> {
    let entry = Entry::new("automatic_desktop", provider).map_err(|e| e.to_string())?;
    entry.set_password(key).map_err(|e| e.to_string())
}

pub fn get_api_key(provider: &str) -> Result<String, String> {
    let entry = Entry::new("automatic_desktop", provider).map_err(|e| e.to_string())?;
    entry.get_password().map_err(|e| e.to_string())
}

// ── Skills ───────────────────────────────────────────────────────────────────

/// Return true if a skill directory contains anything besides SKILL.md.
fn skill_has_resources(skill_dir: &PathBuf) -> bool {
    let Ok(entries) = fs::read_dir(skill_dir) else { return false };
    for entry in entries.filter_map(|e| e.ok()) {
        let name = entry.file_name();
        if name != "SKILL.md" {
            return true;
        }
    }
    false
}

/// Scan a single skills directory and return the set of valid skill names.
fn scan_skills_dir(dir: &PathBuf) -> Result<std::collections::HashSet<String>, String> {
    let mut names = std::collections::HashSet::new();

    if !dir.exists() {
        return Ok(names);
    }

    let entries = fs::read_dir(dir).map_err(|e| e.to_string())?;
    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if is_valid_name(name) && path.join("SKILL.md").exists() {
                        names.insert(name.to_string());
                    }
                }
            }
        }
    }

    Ok(names)
}

/// List skills from both `~/.agents/skills/` and `~/.claude/skills/`,
/// returning entries that indicate which locations each skill exists in,
/// with remote origin info joined from ~/.automatic/skills.json.
pub fn list_skills() -> Result<Vec<SkillEntry>, String> {
    let agents_dir = get_agents_skills_dir()?;
    let claude_dir = get_claude_skills_dir()?;

    let agents_names = scan_skills_dir(&agents_dir)?;
    let claude_names = scan_skills_dir(&claude_dir)?;

    // Best-effort registry load — don't fail list_skills if the file is missing/corrupt
    let registry = read_skill_sources().unwrap_or_default();

    // Union of all names
    let mut all_names: Vec<String> = agents_names.union(&claude_names).cloned().collect();
    all_names.sort();

    let entries = all_names
        .into_iter()
        .map(|name| {
            // Check for resources in the canonical location (agents first, then claude)
            let has_resources = {
                let agents_dir = agents_dir.join(&name);
                let claude_dir = claude_dir.join(&name);
                if agents_dir.exists() {
                    skill_has_resources(&agents_dir)
                } else {
                    skill_has_resources(&claude_dir)
                }
            };
            SkillEntry {
                in_agents: agents_names.contains(&name),
                in_claude: claude_names.contains(&name),
                source: registry.get(&name).cloned(),
                has_resources,
                name,
            }
        })
        .collect();

    Ok(entries)
}

/// Convenience: list just the skill names (union of both directories).
/// Used by sync and autodetect where only names are needed.
pub fn list_skill_names() -> Result<Vec<String>, String> {
    Ok(list_skills()?.into_iter().map(|e| e.name).collect())
}

/// Read a skill's SKILL.md content.  Checks `~/.agents/skills/` first
/// (the canonical location), then falls back to `~/.claude/skills/`.
pub fn read_skill(name: &str) -> Result<String, String> {
    if let Some(path) = get_skill_path(name)? {
        let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        
        // Find the parent directory to check for companion files
        if let Some(skill_dir) = path.parent() {
            let companions = discover_companion_files(skill_dir);
            if !companions.is_empty() {
                return Ok(format_skill_with_companions(&content, &companions));
            }
        }
        
        return Ok(content);
    }
    Ok("".to_string())
}

/// A single companion resource entry returned to the frontend.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ResourceFile {
    /// Path relative to the skill directory, e.g. "scripts/init.py" or "LICENSE.txt"
    pub path: String,
}

/// Grouped companion resources for a skill directory.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SkillResources {
    /// Directories found at the top level (each may contain files)
    pub dirs: Vec<ResourceDir>,
    /// Loose files at the root of the skill directory (not SKILL.md)
    pub root_files: Vec<ResourceFile>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ResourceDir {
    /// Directory name relative to the skill root, e.g. "scripts"
    pub name: String,
    /// Files inside (non-recursive)
    pub files: Vec<ResourceFile>,
}

/// List all companion resources for a skill: every subdirectory and its
/// files, plus any root-level files other than SKILL.md.
/// Returns an empty SkillResources if the skill does not exist.
pub fn list_skill_resources(name: &str) -> Result<SkillResources, String> {
    if !is_valid_name(name) {
        return Err("Invalid skill name".into());
    }

    let skill_dir = match get_skill_dir(name)? {
        Some(d) => d,
        None => return Ok(SkillResources::default()),
    };

    let mut result = SkillResources::default();

    let entries = match fs::read_dir(&skill_dir) {
        Ok(e) => e,
        Err(_) => return Ok(result),
    };

    // Collect and sort entries for deterministic output
    let mut dirs: Vec<String> = Vec::new();
    let mut root_files: Vec<String> = Vec::new();

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        if path.is_dir() {
            dirs.push(file_name);
        } else if file_name != "SKILL.md" {
            root_files.push(file_name);
        }
    }

    dirs.sort();
    root_files.sort();

    // Root-level files
    result.root_files = root_files
        .into_iter()
        .map(|name| ResourceFile { path: name })
        .collect();

    // Subdirectories
    for dir_name in dirs {
        let dir_path = skill_dir.join(&dir_name);
        let mut files: Vec<String> = Vec::new();

        if let Ok(dir_entries) = fs::read_dir(&dir_path) {
            for entry in dir_entries.filter_map(|e| e.ok()) {
                let p = entry.path();
                if p.is_file() {
                    if let Some(fname) = p.file_name().and_then(|n| n.to_str()) {
                        files.push(fname.to_string());
                    }
                }
            }
        }
        files.sort();

        result.dirs.push(ResourceDir {
            name: dir_name,
            files: files
                .into_iter()
                .map(|f| ResourceFile { path: f })
                .collect(),
        });
    }

    Ok(result)
}

// ── Legacy internal helpers (used by read_skill for in-content rendering) ─────

/// Companion file entry — internal only.
#[derive(Debug)]
struct CompanionFile {
    relative_path: String,
    is_dir: bool,
}

/// Discover companion files inside known subdirectories only.
fn discover_companion_files(skill_dir: &std::path::Path) -> Vec<CompanionFile> {
    let mut companions = Vec::new();

    let known_dirs = [
        "scripts",
        "references",
        "docs",
        "assets",
        "examples",
        "templates",
    ];

    for dir_name in &known_dirs {
        let dir_path = skill_dir.join(dir_name);
        if dir_path.is_dir() {
            companions.push(CompanionFile {
                relative_path: dir_name.to_string(),
                is_dir: true,
            });

            if let Ok(entries) = std::fs::read_dir(&dir_path) {
                for entry in entries.filter_map(|e| e.ok()) {
                    if let Ok(file_type) = entry.file_type() {
                        if file_type.is_file() {
                            if let Some(file_name) = entry.file_name().to_str() {
                                companions.push(CompanionFile {
                                    relative_path: format!("{}/{}", dir_name, file_name),
                                    is_dir: false,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    companions
}

/// Format skill content with companion files listed at the end
fn format_skill_with_companions(skill_content: &str, companions: &[CompanionFile]) -> String {
    let mut output = String::from(skill_content);

    // Add companion files section
    output.push_str("\n\n---\n\n");
    output.push_str("## Companion Resources\n\n");
    output.push_str("This skill includes additional resources:\n\n");

    // Group by directory
    let mut current_dir: Option<String> = None;
    for companion in companions {
        if companion.is_dir {
            current_dir = Some(companion.relative_path.clone());
            output.push_str(&format!("\n### {}\n", companion.relative_path));
        } else {
            // Extract directory and filename
            if let Some(slash_pos) = companion.relative_path.rfind('/') {
                let dir = &companion.relative_path[..slash_pos];
                let file = &companion.relative_path[slash_pos + 1..];

                if current_dir.as_deref() == Some(dir) {
                    output.push_str(&format!("- `{}`\n", file));
                } else {
                    output.push_str(&format!("- `{}`\n", companion.relative_path));
                }
            } else {
                output.push_str(&format!("- `{}`\n", companion.relative_path));
            }
        }
    }

    output
}

/// Get the absolute path to a skill's directory. Checks `~/.agents/skills/` first,
/// then falls back to `~/.claude/skills/`.
pub fn get_skill_dir(name: &str) -> Result<Option<PathBuf>, String> {
    if !is_valid_name(name) {
        return Err("Invalid skill name".into());
    }

    let agents_dir = get_agents_skills_dir()?.join(name);
    if agents_dir.join("SKILL.md").exists() {
        return Ok(Some(agents_dir));
    }

    let claude_dir = get_claude_skills_dir()?.join(name);
    if claude_dir.join("SKILL.md").exists() {
        return Ok(Some(claude_dir));
    }

    Ok(None)
}

/// Get the absolute path to a skill's SKILL.md file. Checks `~/.agents/skills/` first,
/// then falls back to `~/.claude/skills/`.
pub fn get_skill_path(name: &str) -> Result<Option<PathBuf>, String> {
    if !is_valid_name(name) {
        return Err("Invalid skill name".into());
    }

    let agents_path = get_agents_skills_dir()?.join(name).join("SKILL.md");
    if agents_path.exists() {
        return Ok(Some(agents_path));
    }

    let claude_path = get_claude_skills_dir()?.join(name).join("SKILL.md");
    if claude_path.exists() {
        return Ok(Some(claude_path));
    }

    Ok(None)
}

/// Save a skill to `~/.agents/skills/` (the agentskills.io standard location).
pub fn save_skill(name: &str, content: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid skill name".into());
    }
    let agents_dir = get_agents_skills_dir()?;
    let skill_dir = agents_dir.join(name);

    if !skill_dir.exists() {
        fs::create_dir_all(&skill_dir).map_err(|e| e.to_string())?;
    }

    let skill_path = skill_dir.join("SKILL.md");
    fs::write(skill_path, content).map_err(|e| e.to_string())
}

/// Delete a skill from both global locations and remove its registry entry.
pub fn delete_skill(name: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid skill name".into());
    }

    // Remove from ~/.agents/skills/
    let agents_dir = get_agents_skills_dir()?.join(name);
    if agents_dir.exists() {
        fs::remove_dir_all(&agents_dir).map_err(|e| e.to_string())?;
    }

    // Remove from ~/.claude/skills/
    let claude_dir = get_claude_skills_dir()?.join(name);
    if claude_dir.exists() {
        fs::remove_dir_all(&claude_dir).map_err(|e| e.to_string())?;
    }

    // Best-effort: remove from registry (ignore errors)
    let _ = remove_skill_source(name);

    Ok(())
}

/// Sync a single skill across both global directories.  Copies from
/// whichever location has it to the other.  If both exist, the
/// `~/.agents/skills/` version is canonical and overwrites claude.
pub fn sync_skill(name: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid skill name".into());
    }

    let agents_dir = get_agents_skills_dir()?;
    let claude_dir = get_claude_skills_dir()?;

    let agents_path = agents_dir.join(name).join("SKILL.md");
    let claude_path = claude_dir.join(name).join("SKILL.md");

    let agents_exists = agents_path.exists();
    let claude_exists = claude_path.exists();

    if !agents_exists && !claude_exists {
        return Err(format!("Skill '{}' not found in any location", name));
    }

    if agents_exists {
        // agents → claude  (agents is canonical)
        let content = fs::read_to_string(&agents_path).map_err(|e| e.to_string())?;
        let target_dir = claude_dir.join(name);
        fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;
        fs::write(&claude_path, content).map_err(|e| e.to_string())?;
    } else {
        // claude → agents
        let content = fs::read_to_string(&claude_path).map_err(|e| e.to_string())?;
        let target_dir = agents_dir.join(name);
        fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;
        fs::write(&agents_path, content).map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Sync all skills across both global directories.
/// Returns the list of skill names that were synced.
pub fn sync_all_skills() -> Result<Vec<String>, String> {
    let entries = list_skills()?;
    let mut synced = Vec::new();

    for entry in entries {
        if !entry.in_agents || !entry.in_claude {
            sync_skill(&entry.name)?;
            synced.push(entry.name);
        }
    }

    Ok(synced)
}

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

/// Extract the value of a YAML frontmatter field from raw SKILL.md text.
/// Handles the `---\nkey: value\n---` block at the top of the file.
fn extract_frontmatter_name(content: &str) -> Option<String> {
    let inner = content.strip_prefix("---")?.trim_start_matches('\n').trim_start_matches('\r');
    let end = inner.find("\n---")?;
    for line in inner[..end].lines() {
        if let Some(rest) = line.strip_prefix("name:") {
            let val = rest.trim().trim_matches('"').trim_matches('\'');
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
    }
    None
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
            let base = format!(
                "https://raw.githubusercontent.com/{}/{}",
                source, branch
            );
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
            "--depth", "1",
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
        .args(["-C", tmp_dir.to_str().unwrap_or(""), "ls-tree", "-r", "--name-only", "HEAD"])
        .output();

    // Get the actual branch name so we can build a raw.githubusercontent.com URL.
    let branch_result = std::process::Command::new("git")
        .args(["-C", tmp_dir.to_str().unwrap_or(""), "rev-parse", "--abbrev-ref", "HEAD"])
        .output();

    let _ = std::fs::remove_dir_all(&tmp_dir);

    let ls_output = match ls_result {
        Ok(out) if out.status.success() => out.stdout,
        _ => return Err(format!("Could not list files in cloned repo for '{}'", name)),
    };

    let branch = match branch_result {
        Ok(out) if out.status.success() => {
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        }
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
        if parent == name { 0usize } else { 1usize }
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
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".automatic/skills.json"))
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

/// Record that a skill was imported from a remote source.
pub fn record_skill_source(name: &str, source: &str, id: &str) -> Result<(), String> {
    let mut registry = read_skill_sources()?;
    registry.insert(
        name.to_string(),
        SkillSource {
            source: source.to_string(),
            id: id.to_string(),
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

// ── User Profile (~/.automatic/profile.json) ─────────────────────────────────
//
// Stores the local user identity.  Authentication is handled externally (a
// future web-service authorisation flow will populate `clerk_id` with a
// real user ID).  Until that happens a stable machine-local UUID is generated
// on first run and used as the `clerk_id` placeholder, so that `created_by`
// relationships on projects are always consistently tagged.

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserProfile {
    /// Stable user ID.  Set to a machine-local UUID by default; replaced by
    /// the web-service user ID once authorisation is implemented.
    pub clerk_id: String,
    /// User email address (empty until set by a web service).
    #[serde(default)]
    pub email: String,
    /// Display name (empty until set by a web service).
    #[serde(default)]
    pub display_name: String,
    /// Avatar URL (none until set by a web service).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    /// ISO 8601 timestamp of the first time this profile was saved locally.
    #[serde(default)]
    pub created_at: String,
    /// ISO 8601 timestamp of the last profile update.
    #[serde(default)]
    pub updated_at: String,
}

fn get_profile_path() -> Result<std::path::PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".automatic/profile.json"))
}

/// Returns the local user profile, bootstrapping a default one on first run.
///
/// If no profile file exists a stable UUID is generated and persisted as the
/// local user ID.  This ensures `userId` is always non-null in the frontend
/// and `created_by` fields on projects are consistently tagged from the
/// very first launch, ready to be replaced by a real web-service ID later.
pub fn read_profile() -> Result<Option<UserProfile>, String> {
    let path = get_profile_path()?;
    if path.exists() {
        let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let profile: UserProfile =
            serde_json::from_str(&raw).map_err(|e| format!("Invalid profile data: {}", e))?;
        return Ok(Some(profile));
    }

    // No profile on disk — bootstrap a default local profile and persist it.
    let local_id = format!("local_{}", uuid::Uuid::new_v4().simple());
    let now = chrono::Utc::now().to_rfc3339();
    let default_profile = UserProfile {
        clerk_id: local_id,
        email: String::new(),
        display_name: String::new(),
        avatar_url: None,
        created_at: now.clone(),
        updated_at: now,
    };
    save_profile(&default_profile)?;
    Ok(Some(default_profile))
}

/// Save or update the local user profile.  On the first save the `created_at`
/// field is set; subsequent saves only update `updated_at`.
pub fn save_profile(profile: &UserProfile) -> Result<(), String> {
    let path = get_profile_path()?;
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }

    let mut to_save = profile.clone();
    let now = chrono::Utc::now().to_rfc3339();

    // Preserve original created_at if the file already exists
    if let Ok(Some(existing)) = read_profile() {
        if !existing.created_at.is_empty() {
            to_save.created_at = existing.created_at;
        }
    }
    if to_save.created_at.is_empty() {
        to_save.created_at = now.clone();
    }
    to_save.updated_at = now;

    let raw = serde_json::to_string_pretty(&to_save).map_err(|e| e.to_string())?;
    fs::write(&path, raw).map_err(|e| e.to_string())
}

// ── Settings (~/.automatic/settings.json) ────────────────────────────────────

/// Onboarding answers captured by the first-run wizard.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct OnboardingData {
    /// The user's primary development role (e.g. "fullstack", "backend").
    #[serde(default)]
    pub role: String,
    /// How the user incorporates AI into their workflow (e.g. "full_agentic").
    #[serde(default)]
    pub ai_usage: String,
    /// Agent IDs the user selected during onboarding.
    #[serde(default)]
    pub agents: Vec<String>,
    /// Email address provided for newsletter subscription (empty if skipped).
    #[serde(default)]
    pub email: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub skill_sync_mode: String,
    /// Whether the user has opted in to anonymous analytics.
    /// Defaults to true; can be disabled in Settings.
    #[serde(default = "default_analytics_enabled")]
    pub analytics_enabled: bool,
    /// Set to true once the first-run wizard has been completed.
    /// If false (or absent), the wizard is shown on next launch.
    #[serde(default)]
    pub wizard_completed: bool,
    /// Answers collected during the first-run wizard.
    #[serde(default)]
    pub onboarding: OnboardingData,
    /// Agent IDs that are automatically pre-selected when creating a new project.
    #[serde(default)]
    pub default_agents: Vec<String>,
}

fn default_analytics_enabled() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            skill_sync_mode: "symlink".to_string(),
            analytics_enabled: true,
            wizard_completed: false,
            onboarding: OnboardingData::default(),
            default_agents: Vec::new(),
        }
    }
}

pub fn read_settings() -> Result<Settings, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    let path = home.join(".automatic/settings.json");
    if !path.exists() {
        return Ok(Settings::default());
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

pub fn write_settings(settings: &Settings) -> Result<(), String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    let path = home.join(".automatic/settings.json");
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let raw = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&path, raw).map_err(|e| e.to_string())
}

// ── Newsletter subscription (Attio) ──────────────────────────────────────────
//
// Flow:
//   1. Assert (upsert) a Person record matched on email_addresses.
//   2. Assert a list entry on the "automatic-updates" list for that person.
//
// The Attio API key is stored in the system keychain under the provider name
// "attio" using the same save_api_key / get_api_key mechanism used elsewhere.

/// Subscribe an email address to the Automatic newsletter via Attio.
/// Returns `Ok(())` on success, or a human-readable error string.
pub async fn subscribe_newsletter(email: &str) -> Result<(), String> {
    let api_key = option_env!("ATTIO_API_KEY")
        .ok_or("Newsletter subscription is not configured in this build")?;

    let client = reqwest::Client::new();
    let auth = format!("Bearer {}", api_key);

    // ── Step 1: assert person ─────────────────────────────────────────────────
    let person_body = serde_json::json!({
        "data": {
            "email_addresses": [{ "email_address": email }]
        }
    });

    let person_resp = client
        .put("https://api.attio.com/v2/objects/people/records")
        .header("Authorization", &auth)
        .header("Content-Type", "application/json")
        .query(&[("matching_attribute", "email_addresses")])
        .json(&person_body)
        .send()
        .await
        .map_err(|e| format!("Attio request failed: {e}"))?;

    if !person_resp.status().is_success() {
        let status = person_resp.status();
        let body = person_resp.text().await.unwrap_or_default();
        return Err(format!("Attio person upsert failed ({status}): {body}"));
    }

    let person_json: serde_json::Value = person_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Attio person response: {e}"))?;

    let record_id = person_json
        .pointer("/data/id/record_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Attio response missing record_id".to_string())?
        .to_string();

    // ── Step 2: assert list entry ─────────────────────────────────────────────
    let entry_body = serde_json::json!({
        "data": {
            "parent_record_id": record_id,
            "parent_object": "people"
        }
    });

    let entry_resp = client
        .put("https://api.attio.com/v2/lists/automatic-updates/entries")
        .header("Authorization", &auth)
        .header("Content-Type", "application/json")
        .json(&entry_body)
        .send()
        .await
        .map_err(|e| format!("Attio list entry request failed: {e}"))?;

    if !entry_resp.status().is_success() {
        let status = entry_resp.status();
        let body = entry_resp.text().await.unwrap_or_default();
        return Err(format!("Attio list entry upsert failed ({status}): {body}"));
    }

    Ok(())
}

// ── Templates ────────────────────────────────────────────────────────────────

pub fn get_templates_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".automatic/templates"))
}

pub fn list_templates() -> Result<Vec<String>, String> {
    let dir = get_templates_dir()?;

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut templates = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| e.to_string())?;

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "md") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if is_valid_name(stem) {
                        templates.push(stem.to_string());
                    }
                }
            }
        }
    }

    Ok(templates)
}

pub fn read_template(name: &str) -> Result<String, String> {
    if !is_valid_name(name) {
        return Err("Invalid template name".into());
    }
    let dir = get_templates_dir()?;
    let path = dir.join(format!("{}.md", name));

    if path.exists() {
        fs::read_to_string(path).map_err(|e| e.to_string())
    } else {
        Err(format!("Template '{}' not found", name))
    }
}

pub fn save_template(name: &str, content: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid template name".into());
    }

    let dir = get_templates_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    let path = dir.join(format!("{}.md", name));
    fs::write(path, content).map_err(|e| e.to_string())
}

pub fn delete_template(name: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid template name".into());
    }
    let dir = get_templates_dir()?;
    let path = dir.join(format!("{}.md", name));

    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Built-in skills shipped with the app.  Each entry is (name, content).
/// These are written to `~/.agents/skills/<name>/SKILL.md` on first run (or
/// when the file is missing), but never overwrite existing files — user edits
/// are always preserved.
const DEFAULT_SKILLS: &[(&str, &str)] = &[
    ("automatic", include_str!("../skills/automatic/SKILL.md")),
    // Skills required by bundled marketplace templates.
    // Never overwrite existing user installations.
    (
        "vercel-react-best-practices",
        include_str!("../skills/vercel-react-best-practices/SKILL.md"),
    ),
    (
        "tailwindcss-development",
        include_str!("../skills/tailwindcss-development/SKILL.md"),
    ),
    (
        "laravel-specialist",
        include_str!("../skills/laravel-specialist/SKILL.md"),
    ),
    (
        "pennant-development",
        include_str!("../skills/pennant-development/SKILL.md"),
    ),
    (
        "terraform-skill",
        include_str!("../skills/terraform-skill/SKILL.md"),
    ),
    (
        "github-workflow-automation",
        include_str!("../skills/github-workflow-automation/SKILL.md"),
    ),
];

/// Write any missing default skills to `~/.agents/skills/`.
/// Existing files are left untouched, so user edits are always preserved.
pub fn install_default_skills() -> Result<(), String> {
    let agents_dir = get_agents_skills_dir()?;

    for (name, content) in DEFAULT_SKILLS {
        let skill_dir = agents_dir.join(name);
        if !skill_dir.exists() {
            fs::create_dir_all(&skill_dir).map_err(|e| e.to_string())?;
        }
        let skill_path = skill_dir.join("SKILL.md");
        if !skill_path.exists() {
            fs::write(&skill_path, content).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

/// Install a subset of bundled skills by name, skipping any that are already
/// present on disk.  Silently ignores names not found in DEFAULT_SKILLS.
pub fn install_skills_from_bundle(skill_names: &[String]) -> Result<(), String> {
    let agents_dir = get_agents_skills_dir()?;

    for name in skill_names {
        // Only install if it's actually bundled.
        let Some((_, content)) = DEFAULT_SKILLS.iter().find(|(n, _)| *n == name.as_str()) else {
            continue;
        };
        let skill_dir = agents_dir.join(name);
        if !skill_dir.exists() {
            fs::create_dir_all(&skill_dir).map_err(|e| e.to_string())?;
        }
        let skill_path = skill_dir.join("SKILL.md");
        if !skill_path.exists() {
            fs::write(&skill_path, content).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

/// Return the set of skill names that are shipped with the app (i.e. present
/// in DEFAULT_SKILLS).
pub fn bundled_skill_names() -> Vec<&'static str> {
    DEFAULT_SKILLS.iter().map(|(name, _)| *name).collect()
}

/// Built-in templates shipped with the app.  Each entry is (name, content).
/// These are written to `~/.automatic/templates/` on first run (or when missing),
/// but never overwrite a file that already exists — user edits are preserved.
const DEFAULT_TEMPLATES: &[(&str, &str)] = &[
    (
        "Agent Project Brief",
        include_str!("../templates/Agent Project Brief.md"),
    ),
    (
        "Session Context",
        include_str!("../templates/Session Context.md"),
    ),
];

/// Write any missing default templates to `~/.automatic/templates/`.
/// Existing files are left untouched, so user edits are always preserved.
pub fn install_default_templates() -> Result<(), String> {
    let dir = get_templates_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    for (name, content) in DEFAULT_TEMPLATES {
        let path = dir.join(format!("{}.md", name));
        if !path.exists() {
            fs::write(&path, content).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

// ── Rules ────────────────────────────────────────────────────────────────────

/// A rule stored as JSON in `~/.automatic/rules/{machine_name}.json`.
/// The machine name (filename stem) is an immutable lowercase slug.
/// The display `name` can be freely renamed.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Rule {
    /// Human-readable display name (can be renamed).
    pub name: String,
    /// Markdown content of the rule.
    pub content: String,
}

/// Summary returned by `list_rules` — machine name + display name.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RuleEntry {
    pub id: String,
    pub name: String,
}

/// Validate a rule machine name: lowercase alphanumeric + hyphens only,
/// must start with a letter, no consecutive hyphens, not empty.
pub fn is_valid_machine_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 128 {
        return false;
    }
    // Must start with a lowercase letter
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    // Remaining: lowercase letters, digits, hyphens (no consecutive hyphens)
    let mut prev_hyphen = false;
    for c in chars {
        if c == '-' {
            if prev_hyphen {
                return false;
            }
            prev_hyphen = true;
        } else if c.is_ascii_lowercase() || c.is_ascii_digit() {
            prev_hyphen = false;
        } else {
            return false;
        }
    }
    // Must not end with a hyphen
    !name.ends_with('-')
}

pub fn get_rules_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".automatic/rules"))
}

pub fn list_rules() -> Result<Vec<RuleEntry>, String> {
    let dir = get_rules_dir()?;

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut rules = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| e.to_string())?;

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if is_valid_machine_name(stem) {
                        if let Ok(raw) = fs::read_to_string(&path) {
                            if let Ok(rule) = serde_json::from_str::<Rule>(&raw) {
                                rules.push(RuleEntry {
                                    id: stem.to_string(),
                                    name: rule.name,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(rules)
}

/// Read the full rule (display name + content) by machine name.
pub fn read_rule(machine_name: &str) -> Result<String, String> {
    if !is_valid_machine_name(machine_name) {
        return Err("Invalid rule machine name".into());
    }
    let dir = get_rules_dir()?;
    let path = dir.join(format!("{}.json", machine_name));

    if path.exists() {
        fs::read_to_string(path).map_err(|e| e.to_string())
    } else {
        Err(format!("Rule '{}' not found", machine_name))
    }
}

/// Read only the content of a rule (for injection into project files).
pub fn read_rule_content(machine_name: &str) -> Result<String, String> {
    let raw = read_rule(machine_name)?;
    let rule: Rule =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid rule data: {}", e))?;
    Ok(rule.content)
}

pub fn save_rule(machine_name: &str, name: &str, content: &str) -> Result<(), String> {
    if !is_valid_machine_name(machine_name) {
        return Err("Invalid rule machine name. Use lowercase letters, digits, and hyphens only.".into());
    }
    if name.trim().is_empty() {
        return Err("Rule display name cannot be empty".into());
    }

    let dir = get_rules_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    let rule = Rule {
        name: name.to_string(),
        content: content.to_string(),
    };
    let pretty = serde_json::to_string_pretty(&rule).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.json", machine_name));
    fs::write(path, pretty).map_err(|e| e.to_string())
}

pub fn delete_rule(machine_name: &str) -> Result<(), String> {
    if !is_valid_machine_name(machine_name) {
        return Err("Invalid rule machine name".into());
    }
    let dir = get_rules_dir()?;
    let path = dir.join(format!("{}.json", machine_name));

    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Built-in rules shipped with the app.  Each entry is (machine_name, display_name, content).
/// Written to `~/.automatic/rules/{machine_name}.json` on first run (or when missing),
/// but never overwrite existing files — user edits are preserved.
const DEFAULT_RULES: &[(&str, &str, &str)] = &[
    (
        "automatic-general",
        "General",
        include_str!("../rules/automatic/general.md"),
    ),
    (
        "automatic-code-style",
        "Code Style",
        include_str!("../rules/automatic/code-style.md"),
    ),
    (
        "automatic-checklist",
        "Checklist",
        include_str!("../rules/automatic/checklist.md"),
    ),
    (
        "automatic-service",
        "Automatic MCP Service",
        include_str!("../rules/automatic/automatic-service.md"),
    ),
];

/// Write any missing default rules to `~/.automatic/rules/`.
/// Existing files are left untouched, so user edits are always preserved.
pub fn install_default_rules() -> Result<(), String> {
    let dir = get_rules_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    for (machine_name, display_name, content) in DEFAULT_RULES {
        let path = dir.join(format!("{}.json", machine_name));
        if !path.exists() {
            let rule = Rule {
                name: display_name.to_string(),
                content: content.to_string(),
            };
            let pretty = serde_json::to_string_pretty(&rule).map_err(|e| e.to_string())?;
            fs::write(&path, pretty).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

// ── Rules Injection ─────────────────────────────────────────────────────────

const RULES_START_MARKER: &str = "<!-- automatic:rules:start -->";
const RULES_END_MARKER: &str = "<!-- automatic:rules:end -->";

/// Public wrapper for `strip_rules_section` (used by sync).
pub fn strip_rules_section_pub(content: &str) -> String {
    strip_rules_section(content)
}

/// Strip the `<!-- automatic:rules:start -->...<!-- automatic:rules:end -->` section.
fn strip_rules_section(content: &str) -> String {
    if let (Some(start), Some(end)) = (content.find(RULES_START_MARKER), content.find(RULES_END_MARKER)) {
        let before = &content[..start];
        let after = &content[end + RULES_END_MARKER.len()..];
        let result = format!("{}{}", before.trim_end(), after.trim_start());
        if result.trim().is_empty() {
            String::new()
        } else {
            result
        }
    } else {
        content.to_string()
    }
}

/// Build the rules section content from a list of rule machine names.
pub fn build_rules_section(rule_names: &[String]) -> Result<String, String> {
    if rule_names.is_empty() {
        return Ok(String::new());
    }

    let mut parts = Vec::new();
    for machine_name in rule_names {
        match read_rule_content(machine_name) {
            Ok(content) if !content.trim().is_empty() => {
                parts.push(content);
            }
            _ => {
                // Skip missing or empty rules silently
            }
        }
    }

    if parts.is_empty() {
        return Ok(String::new());
    }

    let mut section = String::new();
    section.push_str(RULES_START_MARKER);
    section.push('\n');
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            section.push('\n');
        }
        section.push_str(part.trim());
        section.push('\n');
    }
    section.push_str(RULES_END_MARKER);

    Ok(section)
}

/// Write a project file with rules appended.  The user content is written
/// first, then any rules configured for this file are appended inside markers.
pub fn save_project_file_with_rules(
    directory: &str,
    filename: &str,
    user_content: &str,
    rule_names: &[String],
) -> Result<(), String> {
    let rules_section = build_rules_section(rule_names)?;

    let full_content = if rules_section.is_empty() {
        user_content.to_string()
    } else {
        format!("{}\n\n{}\n", user_content.trim_end(), rules_section)
    };

    save_project_file(directory, filename, &full_content)
}

/// Re-inject rules into an existing project file.  Reads the file, strips
/// any existing rules section, rebuilds it from the provided rule names,
/// and writes back.  Used during sync to keep rules current.
pub fn inject_rules_into_project_file(
    directory: &str,
    filename: &str,
    rule_names: &[String],
) -> Result<bool, String> {
    if directory.is_empty() {
        return Ok(false);
    }

    let path = PathBuf::from(directory).join(filename);
    if !path.exists() {
        return Ok(false);
    }

    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let user_content = strip_rules_section(&strip_managed_section(&raw));

    let rules_section = build_rules_section(rule_names)?;

    let full_content = if rules_section.is_empty() {
        user_content.clone()
    } else {
        format!("{}\n\n{}\n", user_content.trim_end(), rules_section)
    };

    // Only write if content actually changed
    if full_content != raw {
        fs::write(&path, full_content).map_err(|e| e.to_string())?;
        Ok(true)
    } else {
        Ok(false)
    }
}

// ── Project Files ────────────────────────────────────────────────────────────

/// Read a project file from the project's directory, stripping any
/// Nexus-managed sections (skills markers) and rules sections.  Returns
/// the user-authored content only.
pub fn read_project_file(directory: &str, filename: &str) -> Result<String, String> {
    if directory.is_empty() {
        return Err("Project has no directory configured".into());
    }

    let path = PathBuf::from(directory).join(filename);
    if !path.exists() {
        return Ok(String::new());
    }

    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    Ok(strip_rules_section(&strip_managed_section(&content)))
}

/// Write a project file to the project's directory.  Writes exactly what the
/// user provides — the sync process will re-merge any managed sections (skills)
/// on the next sync run.
pub fn save_project_file(directory: &str, filename: &str, content: &str) -> Result<(), String> {
    if directory.is_empty() {
        return Err("Project has no directory configured".into());
    }

    let dir = PathBuf::from(directory);
    if !dir.exists() {
        return Err(format!("Directory '{}' does not exist", directory));
    }

    let path = dir.join(filename);
    fs::write(&path, content).map_err(|e| e.to_string())
}

/// Public wrapper for `strip_managed_section` (used by sync).
pub fn strip_managed_section_pub(content: &str) -> String {
    strip_managed_section(content)
}

/// Strip the `<!-- automatic:skills:start -->...<!-- automatic:skills:end -->` section.
fn strip_managed_section(content: &str) -> String {
    let start_marker = "<!-- automatic:skills:start -->";
    let end_marker = "<!-- automatic:skills:end -->";

    if let (Some(start), Some(end)) = (content.find(start_marker), content.find(end_marker)) {
        let before = &content[..start];
        let after = &content[end + end_marker.len()..];
        let result = format!("{}{}", before.trim_end(), after.trim_start());
        if result.trim().is_empty() {
            String::new()
        } else {
            result
        }
    } else {
        content.to_string()
    }
}

// ── MCP Servers ──────────────────────────────────────────────────────────────

pub fn get_mcp_servers_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".automatic/mcp_servers"))
}

pub fn list_mcp_server_configs() -> Result<Vec<String>, String> {
    let dir = get_mcp_servers_dir()?;

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut servers = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| e.to_string())?;

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if is_valid_name(stem) {
                        servers.push(stem.to_string());
                    }
                }
            }
        }
    }

    Ok(servers)
}

pub fn read_mcp_server_config(name: &str) -> Result<String, String> {
    if !is_valid_name(name) {
        return Err("Invalid server name".into());
    }
    let dir = get_mcp_servers_dir()?;
    let path = dir.join(format!("{}.json", name));

    if path.exists() {
        fs::read_to_string(path).map_err(|e| e.to_string())
    } else {
        Err(format!("MCP server '{}' not found", name))
    }
}

pub fn save_mcp_server_config(name: &str, data: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid server name".into());
    }

    // Validate that data is valid JSON
    serde_json::from_str::<serde_json::Value>(data).map_err(|e| format!("Invalid JSON: {}", e))?;

    let dir = get_mcp_servers_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    let path = dir.join(format!("{}.json", name));
    fs::write(path, data).map_err(|e| e.to_string())
}

pub fn delete_mcp_server_config(name: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid server name".into());
    }
    let dir = get_mcp_servers_dir()?;
    let path = dir.join(format!("{}.json", name));

    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Read raw Claude Desktop config (kept for backward compatibility).
pub fn list_mcp_servers() -> Result<String, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    let config_path = home.join("Library/Application Support/Claude/claude_desktop_config.json");

    if config_path.exists() {
        fs::read_to_string(config_path).map_err(|e| e.to_string())
    } else {
        Ok("{}".to_string())
    }
}

// ── Plugins ──────────────────────────────────────────────────────────────────

pub fn get_plugins_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".automatic/plugins"))
}

pub fn get_sessions_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".automatic/sessions.json"))
}

/// The name used in marketplace.json and for `claude plugin` commands.
const MARKETPLACE_NAME: &str = "automatic-plugins";

/// Current plugin version — bump when plugin content changes so Claude Code
/// picks up updates via its cache.
const PLUGIN_VERSION: &str = "0.1.0";

// ── Plugin file contents ────────────────────────────────────────────────────

const HOOKS_JSON: &str = r#"
{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "${CLAUDE_PLUGIN_ROOT}/scripts/register-session.sh"
          }
        ]
      }
    ],
    "SessionEnd": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "${CLAUDE_PLUGIN_ROOT}/scripts/deregister-session.sh"
          }
        ]
      }
    ]
  }
}
"#;

const REGISTER_SESSION_SH: &str = r#"#!/usr/bin/env bash
# register-session.sh — Called by the SessionStart hook.
# Reads hook JSON from stdin, writes an entry to ~/.automatic/sessions.json.
set -euo pipefail

SESSIONS_FILE="$HOME/.automatic/sessions.json"

# Read the full hook input from stdin
INPUT=$(cat)

SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')
CWD=$(echo "$INPUT"        | jq -r '.cwd // empty')
MODEL=$(echo "$INPUT"       | jq -r '.model // "unknown"')
SOURCE=$(echo "$INPUT"      | jq -r '.source // "unknown"')

# Nothing to do without a session id
if [ -z "$SESSION_ID" ]; then
  exit 0
fi

# Portable UTC timestamp
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

# Ensure the store file exists
if [ ! -f "$SESSIONS_FILE" ]; then
  mkdir -p "$(dirname "$SESSIONS_FILE")"
  echo '{}' > "$SESSIONS_FILE"
fi

# Add / update this session (atomic via temp file)
TMPFILE=$(mktemp)
jq --arg id "$SESSION_ID" \
   --arg cwd "$CWD" \
   --arg model "$MODEL" \
   --arg source "$SOURCE" \
   --arg ts "$TIMESTAMP" \
   '.[$id] = {
      "session_id": $id,
      "cwd":        $cwd,
      "model":      $model,
      "source":     $source,
      "started_at": $ts,
      "last_seen":  $ts
    }' \
   "$SESSIONS_FILE" > "$TMPFILE" && mv "$TMPFILE" "$SESSIONS_FILE"

# Prune stale sessions (started > 24 h ago).
# macOS uses -v, GNU date uses -d.  Skip cleanup if neither works.
CUTOFF=$(date -u -v-24H +"%Y-%m-%dT%H:%M:%SZ" 2>/dev/null \
      || date -u -d '24 hours ago' +"%Y-%m-%dT%H:%M:%SZ" 2>/dev/null \
      || echo "")

if [ -n "$CUTOFF" ]; then
  TMPFILE=$(mktemp)
  jq --arg cutoff "$CUTOFF" \
     'with_entries(select(.value.started_at >= $cutoff))' \
     "$SESSIONS_FILE" > "$TMPFILE" && mv "$TMPFILE" "$SESSIONS_FILE"
fi

exit 0
"#;

const DEREGISTER_SESSION_SH: &str = r#"#!/usr/bin/env bash
# deregister-session.sh — Called by the SessionEnd hook.
# Removes the session entry from ~/.automatic/sessions.json.
set -euo pipefail

SESSIONS_FILE="$HOME/.automatic/sessions.json"

INPUT=$(cat)
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')

if [ -z "$SESSION_ID" ] || [ ! -f "$SESSIONS_FILE" ]; then
  exit 0
fi

TMPFILE=$(mktemp)
jq --arg id "$SESSION_ID" 'del(.[$id])' \
   "$SESSIONS_FILE" > "$TMPFILE" && mv "$TMPFILE" "$SESSIONS_FILE"

exit 0
"#;

// ── Sessions reader ─────────────────────────────────────────────────────────

/// Read active sessions from the store file.  Returns the raw JSON string
/// (an object keyed by session_id).  Returns "{}" if the file doesn't exist.
pub fn list_sessions() -> Result<String, String> {
    let path = get_sessions_path()?;
    if path.exists() {
        fs::read_to_string(&path).map_err(|e| e.to_string())
    } else {
        Ok("{}".into())
    }
}

// ── Plugin writer ───────────────────────────────────────────────────────────

/// Helper: create a directory if it doesn't exist.
fn ensure_dir(path: &std::path::Path) -> Result<(), String> {
    if !path.exists() {
        fs::create_dir_all(path)
            .map_err(|e| format!("Failed to create {}: {}", path.display(), e))?;
    }
    Ok(())
}

/// Helper: write a file and return its path string.
fn write_file(path: &std::path::Path, content: &str) -> Result<(), String> {
    fs::write(path, content).map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}

/// Helper: make a file executable (Unix only).
#[cfg(unix)]
fn make_executable(path: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)
        .map_err(|e| format!("Failed to read metadata for {}: {}", path.display(), e))?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)
        .map_err(|e| format!("Failed to chmod {}: {}", path.display(), e))
}

#[cfg(not(unix))]
fn make_executable(_path: &std::path::Path) -> Result<(), String> {
    Ok(()) // no-op on Windows
}

/// Resolve the Automatic binary path.  Uses the current executable when available
/// (gives an absolute path that survives being called from any directory),
/// otherwise falls back to "automatic" on PATH.
fn find_automatic_binary() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "automatic".to_string())
}

/// Write the full Automatic plugin to disk.
fn write_automatic_plugin(plugin_dir: &std::path::Path) -> Result<(), String> {
    // .claude-plugin/plugin.json
    let manifest_dir = plugin_dir.join(".claude-plugin");
    ensure_dir(&manifest_dir)?;

    let plugin_json = serde_json::json!({
        "name": "automatic",
        "description": "Automatic desktop app integration — session tracking and MCP tools",
        "version": PLUGIN_VERSION
    });
    write_file(
        &manifest_dir.join("plugin.json"),
        &serde_json::to_string_pretty(&plugin_json).map_err(|e| format!("JSON error: {}", e))?,
    )?;

    // .mcp.json — makes the Automatic MCP server available in every session
    let automatic_binary = find_automatic_binary();
    let mcp_json = serde_json::json!({
        "mcpServers": {
            "automatic": {
                "command": automatic_binary,
                "args": ["mcp-serve"]
            }
        }
    });
    write_file(
        &plugin_dir.join(".mcp.json"),
        &serde_json::to_string_pretty(&mcp_json).map_err(|e| format!("JSON error: {}", e))?,
    )?;

    // hooks/hooks.json
    let hooks_dir = plugin_dir.join("hooks");
    ensure_dir(&hooks_dir)?;
    write_file(&hooks_dir.join("hooks.json"), HOOKS_JSON)?;

    // scripts/
    let scripts_dir = plugin_dir.join("scripts");
    ensure_dir(&scripts_dir)?;

    let register_path = scripts_dir.join("register-session.sh");
    write_file(&register_path, REGISTER_SESSION_SH)?;
    make_executable(&register_path)?;

    let deregister_path = scripts_dir.join("deregister-session.sh");
    write_file(&deregister_path, DEREGISTER_SESSION_SH)?;
    make_executable(&deregister_path)?;

    Ok(())
}

/// Ensure the local plugin marketplace directory exists with a valid
/// marketplace.json and the full Automatic plugin.
///
/// Layout:
///   ~/.automatic/plugins/
///   ├── .claude-plugin/
///   │   └── marketplace.json
///   └── automatic/
///       ├── .claude-plugin/
///       │   └── plugin.json
///       ├── .mcp.json
///       ├── hooks/
///       │   └── hooks.json
///       └── scripts/
///           ├── register-session.sh
///           └── deregister-session.sh
pub fn ensure_plugin_marketplace() -> Result<PathBuf, String> {
    let plugins_dir = get_plugins_dir()?;

    // ── marketplace manifest ────────────────────────────────────────────
    let manifest_dir = plugins_dir.join(".claude-plugin");
    ensure_dir(&manifest_dir)?;

    let marketplace_json = serde_json::json!({
        "name": MARKETPLACE_NAME,
        "owner": { "name": "Automatic" },
        "metadata": {
            "description": "Plugins bundled with the Automatic desktop app"
        },
        "plugins": [
            {
                "name": "automatic",
                "source": "./automatic",
                "description": "Automatic desktop app integration — session tracking via hooks",
                "version": PLUGIN_VERSION
            }
        ]
    });
    write_file(
        &manifest_dir.join("marketplace.json"),
        &serde_json::to_string_pretty(&marketplace_json)
            .map_err(|e| format!("JSON error: {}", e))?,
    )?;

    // ── automatic plugin ─────────────────────────────────────────────────
    write_automatic_plugin(&plugins_dir.join("automatic"))?;

    Ok(plugins_dir)
}

/// Shell out to the `claude` CLI to register the local marketplace and
/// install plugins.  Skips silently if `claude` is not on PATH (the user
/// may not have Claude Code installed yet).
pub fn install_plugin_marketplace() -> Result<String, String> {
    let plugins_dir = ensure_plugin_marketplace()?;
    let plugins_path = plugins_dir
        .to_str()
        .ok_or("Plugin path contains invalid UTF-8")?;

    // Check if claude CLI is available
    let claude_check = std::process::Command::new("claude")
        .arg("--version")
        .output();

    if claude_check.is_err() {
        return Ok("claude CLI not found — skipping plugin marketplace install".into());
    }

    // Register the marketplace (idempotent — re-adding updates it)
    let add_result = std::process::Command::new("claude")
        .args(["plugin", "marketplace", "add", plugins_path])
        .output()
        .map_err(|e| format!("Failed to run claude plugin marketplace add: {}", e))?;

    if !add_result.status.success() {
        let stderr = String::from_utf8_lossy(&add_result.stderr);
        // "already added" is fine — treat as success
        if !stderr.contains("already") {
            return Err(format!("claude plugin marketplace add failed: {}", stderr));
        }
    }

    // Install the automatic plugin (idempotent — reinstall is a no-op)
    let install_result = std::process::Command::new("claude")
        .args(["plugin", "install", &format!("automatic@{}", MARKETPLACE_NAME)])
        .output()
        .map_err(|e| format!("Failed to run claude plugin install: {}", e))?;

    if !install_result.status.success() {
        let stderr = String::from_utf8_lossy(&install_result.stderr);
        if !stderr.contains("already installed") {
            return Err(format!("claude plugin install failed: {}", stderr));
        }
    }

    Ok("Plugin marketplace registered and plugins installed".into())
}

// ── Project Templates ─────────────────────────────────────────────────────────
//
// Project Templates capture agents, skills, MCP servers and a description that
// can be applied when creating a new project or merged into an existing one.
// Stored as JSON files in `~/.automatic/project_templates/{name}.json`.

/// A single project file stored inline in a project template.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct TemplateProjectFile {
    pub filename: String,
    #[serde(default)]
    pub content: String,
}

/// A template that captures the shareable parts of a project configuration.
/// Excludes per-project fields like `directory`, `created_at`, `updated_at`.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ProjectTemplate {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub mcp_servers: Vec<String>,
    #[serde(default)]
    pub providers: Vec<String>,
    #[serde(default)]
    pub agents: Vec<String>,
    /// Project files (e.g. CLAUDE.md) stored inline so they can be written
    /// to a project's directory when the template is applied.
    #[serde(default)]
    pub project_files: Vec<TemplateProjectFile>,
    /// Single unified project instruction content (written to all agent
    /// instruction files when the template is applied in unified mode).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unified_instruction: String,
    /// Rule IDs attached to the unified instruction. These are written into
    /// the project's `file_rules["_unified"]` when the template is applied.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unified_rules: Vec<String>,
}

pub fn get_project_templates_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".automatic/project_templates"))
}

pub fn list_project_templates() -> Result<Vec<String>, String> {
    let dir = get_project_templates_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut templates = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| e.to_string())?;
    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if is_valid_name(stem) {
                        templates.push(stem.to_string());
                    }
                }
            }
        }
    }
    templates.sort();
    Ok(templates)
}

pub fn read_project_template(name: &str) -> Result<String, String> {
    if !is_valid_name(name) {
        return Err("Invalid template name".into());
    }
    let dir = get_project_templates_dir()?;
    let path = dir.join(format!("{}.json", name));
    if path.exists() {
        fs::read_to_string(path).map_err(|e| e.to_string())
    } else {
        Err(format!("Project template '{}' not found", name))
    }
}

pub fn save_project_template(name: &str, data: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid template name".into());
    }

    // Validate that data is valid JSON for a ProjectTemplate
    let template: ProjectTemplate =
        serde_json::from_str(data).map_err(|e| format!("Invalid template data: {}", e))?;
    let pretty = serde_json::to_string_pretty(&template).map_err(|e| e.to_string())?;

    let dir = get_project_templates_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    let path = dir.join(format!("{}.json", name));
    fs::write(path, pretty).map_err(|e| e.to_string())
}

pub fn delete_project_template(name: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid template name".into());
    }
    let dir = get_project_templates_dir()?;
    let path = dir.join(format!("{}.json", name));
    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn rename_project_template(old_name: &str, new_name: &str) -> Result<(), String> {
    if !is_valid_name(old_name) {
        return Err("Invalid current template name".into());
    }
    if !is_valid_name(new_name) {
        return Err("Invalid new template name".into());
    }
    if old_name == new_name {
        return Ok(());
    }

    let dir = get_project_templates_dir()?;
    let old_path = dir.join(format!("{}.json", old_name));
    let new_path = dir.join(format!("{}.json", new_name));

    if !old_path.exists() {
        return Err(format!("Project template '{}' not found", old_name));
    }
    if new_path.exists() {
        return Err(format!("A project template named '{}' already exists", new_name));
    }

    // Read, update name field, write to new path, remove old
    let raw = fs::read_to_string(&old_path).map_err(|e| e.to_string())?;
    let mut template: ProjectTemplate =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid template data: {}", e))?;
    template.name = new_name.to_string();
    let pretty = serde_json::to_string_pretty(&template).map_err(|e| e.to_string())?;
    fs::write(&new_path, pretty).map_err(|e| e.to_string())?;
    fs::remove_file(&old_path).map_err(|e| e.to_string())?;

    Ok(())
}

// ── Bundled Project Template Marketplace ─────────────────────────────────────
//
// Templates shipped with the app, compiled in via `include_str!`.
// These are served to the Template Marketplace UI without any network calls.
// Users can import them into `~/.automatic/project_templates/` as editable copies.

/// A bundled project template marketplace entry (richer than ProjectTemplate).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BundledProjectTemplate {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub category: String,
    pub tags: Vec<String>,
    pub skills: Vec<String>,
    pub mcp_servers: Vec<String>,
    pub providers: Vec<String>,
    pub agents: Vec<String>,
    pub project_files: Vec<TemplateProjectFile>,
    #[serde(default)]
    pub unified_instruction: String,
    #[serde(default)]
    pub unified_rules: Vec<String>,
    /// Optional icon filename (png or svg) relative to the template-icons asset
    /// directory, e.g. "nextjs.svg". Served at /template-icons/<icon> in the
    /// frontend. When absent the UI falls back to the first letter of the name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// All bundled marketplace templates, compiled in at build time.
const BUNDLED_TEMPLATES: &[(&str, &str)] = &[
    (
        "nextjs-saas-starter",
        include_str!("../project-templates/nextjs-saas-starter.json"),
    ),
    (
        "laravel-api-backend",
        include_str!("../project-templates/laravel-api-backend.json"),
    ),
    (
        "python-data-pipeline",
        include_str!("../project-templates/python-data-pipeline.json"),
    ),
    (
        "tauri-desktop-app",
        include_str!("../project-templates/tauri-desktop-app.json"),
    ),
    (
        "terraform-aws-infrastructure",
        include_str!("../project-templates/terraform-aws-infrastructure.json"),
    ),
    (
        "react-component-library",
        include_str!("../project-templates/react-component-library.json"),
    ),
];

/// Return all bundled marketplace templates as JSON array.
pub fn list_bundled_project_templates() -> Result<String, String> {
    let templates: Result<Vec<BundledProjectTemplate>, _> = BUNDLED_TEMPLATES
        .iter()
        .map(|(_, raw)| serde_json::from_str::<BundledProjectTemplate>(raw))
        .collect();

    let templates = templates.map_err(|e| format!("Failed to parse bundled template: {}", e))?;
    serde_json::to_string(&templates).map_err(|e| e.to_string())
}

/// Return a single bundled marketplace template by name as JSON.
pub fn read_bundled_project_template(name: &str) -> Result<String, String> {
    for (slug, raw) in BUNDLED_TEMPLATES {
        if *slug == name {
            return Ok(raw.to_string());
        }
    }
    Err(format!("Bundled template '{}' not found", name))
}

/// Import a bundled marketplace template into the user's local project templates.
/// If a template with the same name already exists it is overwritten.
/// Any skills listed in the template that are bundled with the app are installed
/// to `~/.agents/skills/` at this point (skipping any already present).
pub fn import_bundled_project_template(name: &str) -> Result<(), String> {
    let raw = read_bundled_project_template(name)?;
    let bundled: BundledProjectTemplate =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid template: {}", e))?;

    // Install any bundled skills the template requires (skip already-installed ones).
    install_skills_from_bundle(&bundled.skills)?;

    // Convert to the standard ProjectTemplate structure for storage
    let pt = ProjectTemplate {
        name: bundled.name.clone(),
        description: bundled.description,
        skills: bundled.skills,
        mcp_servers: bundled.mcp_servers,
        providers: bundled.providers,
        agents: bundled.agents,
        project_files: bundled.project_files,
        unified_instruction: bundled.unified_instruction,
        unified_rules: bundled.unified_rules,
    };

    let json = serde_json::to_string_pretty(&pt).map_err(|e| e.to_string())?;
    save_project_template(&bundled.name, &json)
}

/// Search bundled templates by query (matches name, display_name, description, tags, category).
pub fn search_bundled_project_templates(query: &str) -> Result<String, String> {
    let q = query.to_lowercase();
    let templates: Result<Vec<BundledProjectTemplate>, _> = BUNDLED_TEMPLATES
        .iter()
        .map(|(_, raw)| serde_json::from_str::<BundledProjectTemplate>(raw))
        .collect();

    let templates = templates.map_err(|e| format!("Failed to parse bundled template: {}", e))?;

    if q.trim().is_empty() {
        let json = serde_json::to_string(&templates).map_err(|e| e.to_string())?;
        return Ok(json);
    }

    let filtered: Vec<&BundledProjectTemplate> = templates
        .iter()
        .filter(|t| {
            t.name.to_lowercase().contains(&q)
                || t.display_name.to_lowercase().contains(&q)
                || t.description.to_lowercase().contains(&q)
                || t.category.to_lowercase().contains(&q)
                || t.tags.iter().any(|tag| tag.to_lowercase().contains(&q))
        })
        .collect();

    serde_json::to_string(&filtered).map_err(|e| e.to_string())
}

// ── Template Dependency Checking ─────────────────────────────────────────────

/// The status of a single skill dependency for a template.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkillDependencyStatus {
    /// Skill name as listed in the template (e.g. "vercel-react-best-practices").
    pub name: String,
    /// Whether the skill is currently installed locally.
    pub installed: bool,
    /// Whether the skill is shipped with the app and can be installed without
    /// a network call.  If `true` and `installed` is `false`, importing the
    /// template will install it automatically.
    pub bundled: bool,
}

/// Dependency check result for a template.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TemplateDependencyReport {
    /// Dependency status for every skill the template requires.
    pub skills: Vec<SkillDependencyStatus>,
    /// MCP server names required by the template that are not configured locally.
    pub missing_mcp_servers: Vec<String>,
}

/// Check which skills and MCP servers a bundled template requires are missing
/// locally.  Bundled skills (shipped with the app) are flagged as installable
/// without a network call — no skills.sh lookup is performed.
pub fn check_template_dependencies(template_name: &str) -> Result<String, String> {
    let raw = read_bundled_project_template(template_name)?;
    let bundled: BundledProjectTemplate =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid template JSON: {}", e))?;

    let installed_names: std::collections::HashSet<String> =
        list_skill_names().unwrap_or_default().into_iter().collect();

    let installed_mcp: std::collections::HashSet<String> =
        list_mcp_server_configs().unwrap_or_default().into_iter().collect();

    let bundled_names: std::collections::HashSet<&str> =
        bundled_skill_names().into_iter().collect();

    let skill_statuses: Vec<SkillDependencyStatus> = bundled
        .skills
        .iter()
        .map(|skill_name| SkillDependencyStatus {
            name: skill_name.clone(),
            installed: installed_names.contains(skill_name.as_str()),
            bundled: bundled_names.contains(skill_name.as_str()),
        })
        .collect();

    let missing_mcp_servers: Vec<String> = bundled
        .mcp_servers
        .into_iter()
        .filter(|s| !installed_mcp.contains(s.as_str()))
        .collect();

    let report = TemplateDependencyReport {
        skills: skill_statuses,
        missing_mcp_servers,
    };

    serde_json::to_string(&report).map_err(|e| e.to_string())
}



// ── Projects ─────────────────────────────────────────────────────────────────
//
// Project configs are stored in the project directory at `.automatic/project.json`.
// A lightweight registry entry at `~/.automatic/projects/{name}.json` maps project
// names to their directories so we can enumerate them.  When a project has no
// directory set yet, the full config lives in the registry file as a fallback.

/// Returns the path to the full project config inside the project directory.
fn project_config_path(directory: &str) -> PathBuf {
    PathBuf::from(directory).join(".automatic").join("project.json")
}

pub fn list_projects() -> Result<Vec<String>, String> {
    let projects_dir = get_projects_dir()?;

    if !projects_dir.exists() {
        return Ok(Vec::new());
    }

    let mut projects = Vec::new();
    let entries = fs::read_dir(&projects_dir).map_err(|e| e.to_string())?;

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if is_valid_name(stem) {
                        projects.push(stem.to_string());
                    }
                }
            }
        }
    }

    Ok(projects)
}

pub fn read_project(name: &str) -> Result<String, String> {
    if !is_valid_name(name) {
        return Err("Invalid project name".into());
    }
    let projects_dir = get_projects_dir()?;
    let registry_path = projects_dir.join(format!("{}.json", name));

    if !registry_path.exists() {
        return Err(format!("Project '{}' not found", name));
    }

    let raw = fs::read_to_string(&registry_path).map_err(|e| e.to_string())?;
    let registry_project = match serde_json::from_str::<Project>(&raw) {
        Ok(p) => p,
        Err(_) => Project {
            name: name.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            ..Default::default()
        },
    };

    // If directory is set, try to read full config from the project directory
    if !registry_project.directory.is_empty() {
        let config_path = project_config_path(&registry_project.directory);
        if config_path.exists() {
            let project_raw = fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
            let full_project = match serde_json::from_str::<Project>(&project_raw) {
                Ok(p) => p,
                Err(_) => registry_project, // fall back to registry data
            };
            let formatted =
                serde_json::to_string_pretty(&full_project).map_err(|e| e.to_string())?;
            return Ok(formatted);
        }
    }

    // No project-directory config found — use registry data (legacy or no-directory case)
    let formatted = serde_json::to_string_pretty(&registry_project).map_err(|e| e.to_string())?;
    Ok(formatted)
}

pub fn save_project(name: &str, data: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid project name".into());
    }

    let project: Project =
        serde_json::from_str(data).map_err(|e| format!("Invalid project data: {}", e))?;
    let pretty = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;

    let projects_dir = get_projects_dir()?;
    if !projects_dir.exists() {
        fs::create_dir_all(&projects_dir).map_err(|e| e.to_string())?;
    }

    let registry_path = projects_dir.join(format!("{}.json", name));

    if !project.directory.is_empty() {
        // Write full config to project directory
        let automatic_dir = PathBuf::from(&project.directory).join(".automatic");
        if !automatic_dir.exists() {
            fs::create_dir_all(&automatic_dir).map_err(|e| e.to_string())?;
        }
        let config_path = automatic_dir.join("project.json");
        fs::write(&config_path, &pretty).map_err(|e| e.to_string())?;

        // Write lightweight registry entry
        let ref_data = serde_json::json!({
            "name": project.name,
            "directory": project.directory,
        });
        let ref_pretty = serde_json::to_string_pretty(&ref_data).map_err(|e| e.to_string())?;
        fs::write(&registry_path, &ref_pretty).map_err(|e| e.to_string())?;
    } else {
        // No directory yet — write full config to registry
        fs::write(&registry_path, &pretty).map_err(|e| e.to_string())?;
    }

    Ok(())
}

pub fn rename_project(old_name: &str, new_name: &str) -> Result<(), String> {
    if !is_valid_name(old_name) {
        return Err("Invalid current project name".into());
    }
    if !is_valid_name(new_name) {
        return Err("Invalid new project name".into());
    }
    if old_name == new_name {
        return Ok(());
    }

    let projects_dir = get_projects_dir()?;
    let old_registry = projects_dir.join(format!("{}.json", old_name));
    let new_registry = projects_dir.join(format!("{}.json", new_name));

    if !old_registry.exists() {
        return Err(format!("Project '{}' not found", old_name));
    }
    // Only block if the target file exists and is a genuinely different project
    // (not just a case change on a case-insensitive filesystem like macOS APFS).
    if new_registry.exists() {
        // Compare canonical paths: on a case-insensitive FS, a case-only rename
        // will resolve both paths to the same inode.
        let old_canon = old_registry.canonicalize().map_err(|e| e.to_string())?;
        let new_canon = new_registry.canonicalize().map_err(|e| e.to_string())?;
        if old_canon != new_canon {
            return Err(format!("A project named '{}' already exists", new_name));
        }
    }

    // Read the full project (via read_project which resolves directory-based configs)
    let raw = read_project(old_name)?;
    let mut project: Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    // Update the name field
    project.name = new_name.to_string();
    project.updated_at = chrono::Utc::now().to_rfc3339();

    let pretty = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;

    // Write the in-directory config with the updated name
    if !project.directory.is_empty() {
        let config_path = project_config_path(&project.directory);
        if config_path.exists() || PathBuf::from(&project.directory).join(".automatic").exists() {
            let automatic_dir = PathBuf::from(&project.directory).join(".automatic");
            if !automatic_dir.exists() {
                fs::create_dir_all(&automatic_dir).map_err(|e| e.to_string())?;
            }
            fs::write(&config_path, &pretty).map_err(|e| e.to_string())?;
        }

        // Write new registry entry (lightweight pointer)
        let ref_data = serde_json::json!({
            "name": project.name,
            "directory": project.directory,
        });
        let ref_pretty = serde_json::to_string_pretty(&ref_data).map_err(|e| e.to_string())?;
        fs::write(&new_registry, &ref_pretty).map_err(|e| e.to_string())?;
    } else {
        // No directory — write full config to new registry entry
        fs::write(&new_registry, &pretty).map_err(|e| e.to_string())?;
    }

    // On a case-insensitive filesystem (macOS APFS/HFS+), a case-only rename
    // means old_registry and new_registry point to the same inode.  In that
    // case fs::write already updated the content above, so we just need to
    // rename the file to get the new casing on disk.  A plain remove would
    // delete the only copy.
    let same_file = old_registry.canonicalize().ok() == new_registry.canonicalize().ok();
    if same_file {
        // fs::rename handles case-only renames correctly on APFS/HFS+
        fs::rename(&old_registry, &new_registry).map_err(|e| e.to_string())?;
    } else {
        fs::remove_file(&old_registry).map_err(|e| e.to_string())?;
    }

    Ok(())
}

pub fn delete_project(name: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid project name".into());
    }
    let projects_dir = get_projects_dir()?;
    let registry_path = projects_dir.join(format!("{}.json", name));

    // Try to read the project to clean up the project-directory config
    if registry_path.exists() {
        if let Ok(raw) = fs::read_to_string(&registry_path) {
            if let Ok(project) = serde_json::from_str::<Project>(&raw) {
                if !project.directory.is_empty() {
                    let config_path = project_config_path(&project.directory);
                    if config_path.exists() {
                        let _ = fs::remove_file(&config_path);
                    }
                    // Remove .automatic dir if it's now empty
                    let automatic_dir = PathBuf::from(&project.directory).join(".automatic");
                    if automatic_dir.exists() {
                        let _ = fs::remove_dir(&automatic_dir); // only succeeds if empty
                    }
                }
            }
        }

        fs::remove_file(&registry_path).map_err(|e| e.to_string())?;
    }

    Ok(())
}

// ── Editor Detection & Open ───────────────────────────────────────────────────

/// Known editors with their detection strategy (macOS-first, cross-platform fallback).
#[derive(Debug, Clone, Serialize)]
pub struct EditorInfo {
    /// Stable identifier used when calling `open_in_editor`.
    pub id: String,
    /// Human-readable label shown in the UI.
    pub label: String,
    /// Whether this editor was detected as installed on the current machine.
    pub installed: bool,
}

/// Probe whether a given app bundle path exists OR a CLI command is on PATH.
fn app_installed(app_path: &str, cli_name: Option<&str>) -> bool {
    if std::path::Path::new(app_path).exists() {
        return true;
    }
    if let Some(cli) = cli_name {
        // Use `which` to check if the CLI is on PATH
        std::process::Command::new("which")
            .arg(cli)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    } else {
        false
    }
}

/// Return all supported editors with their installation status.
pub fn check_installed_editors() -> Vec<EditorInfo> {
    vec![
        EditorInfo {
            id: "finder".into(),
            label: "Finder".into(),
            // Finder is always available on macOS
            installed: cfg!(target_os = "macos"),
        },
        EditorInfo {
            id: "vscode".into(),
            label: "VS Code".into(),
            installed: app_installed(
                "/Applications/Visual Studio Code.app",
                Some("code"),
            ),
        },
        EditorInfo {
            id: "cursor".into(),
            label: "Cursor".into(),
            installed: app_installed("/Applications/Cursor.app", Some("cursor")),
        },
        EditorInfo {
            id: "zed".into(),
            label: "Zed".into(),
            installed: app_installed("/Applications/Zed.app", Some("zed")),
        },
        EditorInfo {
            id: "textmate".into(),
            label: "TextMate".into(),
            installed: app_installed("/Applications/TextMate.app", Some("mate")),
        },
        EditorInfo {
            id: "antigravity".into(),
            label: "Antigravity".into(),
            installed: app_installed("/Applications/Antigravity.app", None),
        },
        EditorInfo {
            id: "xcode".into(),
            label: "Xcode".into(),
            installed: app_installed("/Applications/Xcode.app", Some("xed")),
        },
    ]
}

/// Open a directory in the specified editor.
///
/// `editor_id` must match one of the `id` values returned by `check_installed_editors`.
/// `path` must be an absolute directory path.
pub fn open_in_editor(editor_id: &str, path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("No project directory set".into());
    }

    let status = match editor_id {
        "finder" => {
            // `open` on macOS opens Finder at the directory
            std::process::Command::new("open")
                .arg(path)
                .spawn()
                .map_err(|e| format!("Failed to open Finder: {}", e))?;
            return Ok(());
        }
        "vscode" => {
            // Prefer the CLI; fall back to `open -a`
            if which_available("code") {
                std::process::Command::new("code").arg(path).spawn()
            } else {
                std::process::Command::new("open")
                    .args(["-a", "Visual Studio Code", path])
                    .spawn()
            }
        }
        "cursor" => {
            if which_available("cursor") {
                std::process::Command::new("cursor").arg(path).spawn()
            } else {
                std::process::Command::new("open")
                    .args(["-a", "Cursor", path])
                    .spawn()
            }
        }
        "zed" => {
            if which_available("zed") {
                std::process::Command::new("zed").arg(path).spawn()
            } else {
                std::process::Command::new("open")
                    .args(["-a", "Zed", path])
                    .spawn()
            }
        }
        "textmate" => {
            if which_available("mate") {
                std::process::Command::new("mate").arg(path).spawn()
            } else {
                std::process::Command::new("open")
                    .args(["-a", "TextMate", path])
                    .spawn()
            }
        }
        "antigravity" => std::process::Command::new("open")
            .args(["-a", "Antigravity", path])
            .spawn(),
        "xcode" => {
            if which_available("xed") {
                std::process::Command::new("xed").arg(path).spawn()
            } else {
                std::process::Command::new("open")
                    .args(["-a", "Xcode", path])
                    .spawn()
            }
        }
        other => return Err(format!("Unknown editor id: {}", other)),
    };

    status.map(|_| ()).map_err(|e| e.to_string())
}

/// Convert the `.icns` file for `editor_id` to a PNG and return it as a
/// base64-encoded string suitable for use in a `data:image/png;base64,...` URL.
///
/// The PNG is cached in `/tmp/automatic-icons/` to avoid re-running `sips` on
/// every call.  Uses macOS `sips` (always available on macOS) to do the
/// conversion.  Returns an error string if the editor id is unknown, the
/// `.icns` file does not exist, or `sips` / IO fails.
pub fn get_editor_icon(editor_id: &str) -> Result<String, String> {
    let icns_path: &str = match editor_id {
        "finder" => "/System/Library/CoreServices/Finder.app/Contents/Resources/Finder.icns",
        "vscode" => "/Applications/Visual Studio Code.app/Contents/Resources/Code.icns",
        "cursor" => "/Applications/Cursor.app/Contents/Resources/Cursor.icns",
        "zed" => "/Applications/Zed.app/Contents/Resources/Zed.icns",
        "textmate" => "/Applications/TextMate.app/Contents/Resources/TextMate.icns",
        "antigravity" => "/Applications/Antigravity.app/Contents/Resources/Antigravity.icns",
        "xcode" => "/Applications/Xcode.app/Contents/Resources/Xcode.icns",
        other => return Err(format!("Unknown editor id: {}", other)),
    };

    if !std::path::Path::new(icns_path).exists() {
        return Err(format!("Icon file not found: {}", icns_path));
    }

    let cache_dir = std::path::Path::new("/tmp/automatic-icons");
    std::fs::create_dir_all(cache_dir)
        .map_err(|e| format!("Failed to create icon cache dir: {}", e))?;

    let out_path = cache_dir.join(format!("{}.png", editor_id));
    let out_str = out_path
        .to_str()
        .ok_or_else(|| "Invalid output path".to_string())?
        .to_string();

    // Convert .icns → PNG if not already cached
    if !out_path.exists() {
        let output = std::process::Command::new("sips")
            .args(["-s", "format", "png", icns_path, "--out", &out_str])
            .output()
            .map_err(|e| format!("Failed to run sips: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("sips failed: {}", stderr));
        }
    }

    // Read the PNG and return it as a base64 data URI so the frontend can
    // embed it directly without needing the Tauri asset protocol.
    let bytes = std::fs::read(&out_path)
        .map_err(|e| format!("Failed to read cached icon: {}", e))?;

    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let b64 = STANDARD.encode(&bytes);

    Ok(format!("data:image/png;base64,{}", b64))
}

/// Return true when `name` resolves to an executable via `which`.
fn which_available(name: &str) -> bool {
    std::process::Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
