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
    Ok(home.join(".nexus/projects"))
}

pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && !name.contains('\\') && name != "." && name != ".."
}

// ── Data Structures ──────────────────────────────────────────────────────────

/// Remote origin of a skill imported from skills.sh.
/// Stored in ~/.nexus/skills.json keyed by skill name.
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
    /// Remote origin from ~/.nexus/skills.json, if this was imported from skills.sh
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SkillSource>,
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
}

// ── API Keys ─────────────────────────────────────────────────────────────────

pub fn save_api_key(provider: &str, key: &str) -> Result<(), String> {
    let entry = Entry::new("nexus_desktop", provider).map_err(|e| e.to_string())?;
    entry.set_password(key).map_err(|e| e.to_string())
}

pub fn get_api_key(provider: &str) -> Result<String, String> {
    let entry = Entry::new("nexus_desktop", provider).map_err(|e| e.to_string())?;
    entry.get_password().map_err(|e| e.to_string())
}

// ── Skills ───────────────────────────────────────────────────────────────────

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
/// with remote origin info joined from ~/.nexus/skills.json.
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
        .map(|name| SkillEntry {
            in_agents: agents_names.contains(&name),
            in_claude: claude_names.contains(&name),
            source: registry.get(&name).cloned(),
            name,
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
    if !is_valid_name(name) {
        return Err("Invalid skill name".into());
    }

    let agents_path = get_agents_skills_dir()?.join(name).join("SKILL.md");
    if agents_path.exists() {
        return fs::read_to_string(agents_path).map_err(|e| e.to_string());
    }

    let claude_path = get_claude_skills_dir()?.join(name).join("SKILL.md");
    if claude_path.exists() {
        return fs::read_to_string(claude_path).map_err(|e| e.to_string());
    }

    Ok("".to_string())
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
        .header("User-Agent", "nexus-desktop/1.0")
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

/// Fetch the SKILL.md content for a remote skill by constructing the GitHub
/// raw content URL from the skill's `source` ("owner/repo") and `name`.
///
/// Tries the canonical path `skills/<name>/SKILL.md` first, then falls back
/// to a root-level `SKILL.md` (for single-skill repos).
pub async fn fetch_remote_skill_content(source: &str, name: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    // Try the standard nested path first
    let candidates = [
        format!(
            "https://raw.githubusercontent.com/{}/main/skills/{}/SKILL.md",
            source, name
        ),
        format!(
            "https://raw.githubusercontent.com/{}/main/{}/SKILL.md",
            source, name
        ),
        format!(
            "https://raw.githubusercontent.com/{}/main/SKILL.md",
            source
        ),
    ];

    for url in &candidates {
        let resp = client
            .get(url)
            .header("User-Agent", "nexus-desktop/1.0")
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if resp.status().is_success() {
            return resp
                .text()
                .await
                .map_err(|e| format!("Failed to read response: {}", e));
        }
    }

    Err(format!("Could not fetch SKILL.md for '{}'", name))
}

// ── Skills Registry (~/.nexus/skills.json) ───────────────────────────────────
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
    Ok(home.join(".nexus/skills.json"))
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

// ── Templates ────────────────────────────────────────────────────────────────

pub fn get_templates_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".nexus/templates"))
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
const DEFAULT_SKILLS: &[(&str, &str)] = &[(
    "automatic",
    include_str!("../skills/automatic/SKILL.md"),
)];

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

/// Built-in templates shipped with the app.  Each entry is (name, content).
/// These are written to `~/.nexus/templates/` on first run (or when missing),
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

/// Write any missing default templates to `~/.nexus/templates/`.
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

// ── Project Files ────────────────────────────────────────────────────────────

/// Read a project file from the project's directory, stripping any
/// Nexus-managed sections (skills markers).  Returns the user-authored
/// content only.
pub fn read_project_file(directory: &str, filename: &str) -> Result<String, String> {
    if directory.is_empty() {
        return Err("Project has no directory configured".into());
    }

    let path = PathBuf::from(directory).join(filename);
    if !path.exists() {
        return Ok(String::new());
    }

    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    Ok(strip_managed_section(&content))
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

/// Strip the `<!-- nexus:skills:start -->...<!-- nexus:skills:end -->` section.
fn strip_managed_section(content: &str) -> String {
    let start_marker = "<!-- nexus:skills:start -->";
    let end_marker = "<!-- nexus:skills:end -->";

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
    Ok(home.join(".nexus/mcp_servers"))
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

/// Import MCP servers from Claude Desktop config into the Nexus registry.
/// Returns the list of server names that were imported.
pub fn import_mcp_servers_from_claude() -> Result<Vec<String>, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    let config_path = home.join("Library/Application Support/Claude/claude_desktop_config.json");

    if !config_path.exists() {
        return Err("Claude Desktop config not found".into());
    }

    let raw = fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
    let parsed: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid JSON: {}", e))?;

    let servers = parsed
        .get("mcpServers")
        .and_then(|v| v.as_object())
        .ok_or("No mcpServers found in Claude Desktop config")?;

    let mut imported = Vec::new();

    for (name, config) in servers {
        if !is_valid_name(name) {
            continue;
        }
        let data =
            serde_json::to_string_pretty(config).map_err(|e| format!("JSON error: {}", e))?;
        save_mcp_server_config(name, &data)?;
        imported.push(name.clone());
    }

    Ok(imported)
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
    Ok(home.join(".nexus/plugins"))
}

pub fn get_sessions_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".nexus/sessions.json"))
}

/// The name used in marketplace.json and for `claude plugin` commands.
const MARKETPLACE_NAME: &str = "nexus-plugins";

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
# Reads hook JSON from stdin, writes an entry to ~/.nexus/sessions.json.
set -euo pipefail

SESSIONS_FILE="$HOME/.nexus/sessions.json"

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
# Removes the session entry from ~/.nexus/sessions.json.
set -euo pipefail

SESSIONS_FILE="$HOME/.nexus/sessions.json"

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

/// Resolve the Nexus binary path.  Uses the current executable when available
/// (gives an absolute path that survives being called from any directory),
/// otherwise falls back to "nexus" on PATH.
fn find_nexus_binary() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "nexus".to_string())
}

/// Write the full Nexus plugin to disk.
fn write_nexus_plugin(plugin_dir: &std::path::Path) -> Result<(), String> {
    // .claude-plugin/plugin.json
    let manifest_dir = plugin_dir.join(".claude-plugin");
    ensure_dir(&manifest_dir)?;

    let plugin_json = serde_json::json!({
        "name": "nexus",
        "description": "Nexus desktop app integration — session tracking and MCP tools",
        "version": PLUGIN_VERSION
    });
    write_file(
        &manifest_dir.join("plugin.json"),
        &serde_json::to_string_pretty(&plugin_json).map_err(|e| format!("JSON error: {}", e))?,
    )?;

    // .mcp.json — makes the Nexus MCP server available in every session
    let nexus_binary = find_nexus_binary();
    let mcp_json = serde_json::json!({
        "mcpServers": {
            "nexus": {
                "command": nexus_binary,
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
/// marketplace.json and the full Nexus plugin.
///
/// Layout:
///   ~/.nexus/plugins/
///   ├── .claude-plugin/
///   │   └── marketplace.json
///   └── nexus/
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
        "owner": { "name": "Nexus" },
        "metadata": {
            "description": "Plugins bundled with the Nexus desktop app"
        },
        "plugins": [
            {
                "name": "nexus",
                "source": "./nexus",
                "description": "Nexus desktop app integration — session tracking via hooks",
                "version": PLUGIN_VERSION
            }
        ]
    });
    write_file(
        &manifest_dir.join("marketplace.json"),
        &serde_json::to_string_pretty(&marketplace_json)
            .map_err(|e| format!("JSON error: {}", e))?,
    )?;

    // ── nexus plugin ────────────────────────────────────────────────────
    write_nexus_plugin(&plugins_dir.join("nexus"))?;

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

    // Install the nexus plugin (idempotent — reinstall is a no-op)
    let install_result = std::process::Command::new("claude")
        .args(["plugin", "install", &format!("nexus@{}", MARKETPLACE_NAME)])
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
// Stored as JSON files in `~/.nexus/project_templates/{name}.json`.

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
}

pub fn get_project_templates_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".nexus/project_templates"))
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

// ── Projects ─────────────────────────────────────────────────────────────────
//
// Project configs are stored in the project directory at `.nexus/project.json`.
// A lightweight registry entry at `~/.nexus/projects/{name}.json` maps project
// names to their directories so we can enumerate them.  When a project has no
// directory set yet, the full config lives in the registry file as a fallback.

/// Returns the path to the full project config inside the project directory.
fn project_config_path(directory: &str) -> PathBuf {
    PathBuf::from(directory).join(".nexus").join("project.json")
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
        let nexus_dir = PathBuf::from(&project.directory).join(".nexus");
        if !nexus_dir.exists() {
            fs::create_dir_all(&nexus_dir).map_err(|e| e.to_string())?;
        }
        let config_path = nexus_dir.join("project.json");
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
        if config_path.exists() || PathBuf::from(&project.directory).join(".nexus").exists() {
            let nexus_dir = PathBuf::from(&project.directory).join(".nexus");
            if !nexus_dir.exists() {
                fs::create_dir_all(&nexus_dir).map_err(|e| e.to_string())?;
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
                    // Remove .nexus dir if it's now empty
                    let nexus_dir = PathBuf::from(&project.directory).join(".nexus");
                    if nexus_dir.exists() {
                        let _ = fs::remove_dir(&nexus_dir); // only succeeds if empty
                    }
                }
            }
        }

        fs::remove_file(&registry_path).map_err(|e| e.to_string())?;
    }

    Ok(())
}
