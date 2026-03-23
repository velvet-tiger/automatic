use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use super::paths::get_automatic_dir;
// ── User Agents ──────────────────────────────────────────────────────────────

/// A user-defined agent stored as Markdown with YAML frontmatter in
/// `~/.automatic/agents/{machine_name}.md`.
/// The machine name (filename stem) is an immutable lowercase slug.
/// The display `name` is extracted from the frontmatter `name` field.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserAgent {
    /// Human-readable display name (from frontmatter `name` field).
    pub name: String,
    /// Full Markdown content including frontmatter.
    pub content: String,
}

/// Summary returned by `list_user_agents` — machine name + display name.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserAgentEntry {
    pub id: String,
    pub name: String,
    /// Source: "automatic" for bundled, "local" for user-created, "codex" for discovered from Codex
    #[serde(default)]
    pub source: String,
    /// Author display name: "Automatic", "You", "OpenAI", etc.
    #[serde(default)]
    pub author: String,
}

/// Validate an agent machine name: lowercase alphanumeric + hyphens only,
/// must start with a letter, no consecutive hyphens, not empty.
pub fn is_valid_agent_machine_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 128 {
        return false;
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
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
    !name.ends_with('-')
}

pub fn get_user_agents_dir() -> Result<PathBuf, String> {
    Ok(get_automatic_dir()?.join("agents"))
}

/// Extract the `name` field from YAML frontmatter in a Markdown file.
/// Returns `None` if frontmatter is missing or invalid.
fn extract_name_from_frontmatter(content: &str) -> Option<String> {
    if !content.starts_with("---\n") {
        return None;
    }
    let end = content[4..].find("\n---")?;
    let yaml = &content[4..end + 4];
    for line in yaml.lines() {
        let line = line.trim();
        if let Some(name_val) = line.strip_prefix("name:") {
            let name = name_val.trim().trim_matches('"').trim_matches('\'');
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

pub fn list_user_agents() -> Result<Vec<UserAgentEntry>, String> {
    let dir = get_user_agents_dir()?;

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut agents = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| e.to_string())?;

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "md") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if is_valid_agent_machine_name(stem) {
                        if let Ok(raw) = fs::read_to_string(&path) {
                            let name = extract_name_from_frontmatter(&raw)
                                .unwrap_or_else(|| stem.to_string());
                            agents.push(UserAgentEntry {
                                id: stem.to_string(),
                                name,
                                source: if is_bundled_agent(stem) {
                                    "automatic".to_string()
                                } else {
                                    "local".to_string()
                                },
                                author: if is_bundled_agent(stem) {
                                    "Automatic".to_string()
                                } else {
                                    "You".to_string()
                                },
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(agents)
}

/// Parsed OpenAI agent from Codex's openai.yaml format.
#[derive(Debug, Clone)]
struct CodexAgent {
    display_name: String,
    short_description: Option<String>,
    default_prompt: Option<String>,
}

/// Parse an openai.yaml file from Codex and extract agent info.
fn parse_codex_openai_yaml(content: &str) -> Option<CodexAgent> {
    // Simple YAML parsing for the specific format used by Codex
    let mut display_name: Option<String> = None;
    let mut short_description: Option<String> = None;
    let mut default_prompt: Option<String> = None;
    let mut in_interface = false;
    let mut in_default_prompt = false;
    let mut prompt_lines: Vec<String> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == "interface:" {
            in_interface = true;
            in_default_prompt = false;
            continue;
        }

        if trimmed == "dependencies:" {
            in_interface = false;
            in_default_prompt = false;
            continue;
        }

        if in_interface {
            if trimmed.starts_with("display_name:") {
                let val = trimmed
                    .strip_prefix("display_name:")
                    .map(|s| s.trim().trim_matches('"').to_string());
                display_name = val;
            } else if trimmed.starts_with("short_description:") {
                let val = trimmed
                    .strip_prefix("short_description:")
                    .map(|s| s.trim().trim_matches('"').to_string());
                short_description = val;
            } else if trimmed == "default_prompt:" {
                in_default_prompt = true;
            } else if in_default_prompt && trimmed.starts_with('"') && trimmed.ends_with('"') {
                // Single-line prompt
                default_prompt = Some(trimmed.trim_matches('"').to_string());
                in_default_prompt = false;
            } else if !in_default_prompt && trimmed.starts_with("default_prompt:") {
                // Check if it's inline or multiline
                let rest = trimmed.strip_prefix("default_prompt:").unwrap_or("").trim();
                if rest.starts_with('"') && rest.ends_with('"') {
                    default_prompt = Some(rest.trim_matches('"').to_string());
                } else if rest.is_empty() {
                    in_default_prompt = true;
                }
            }
        }

        // Handle multiline default_prompt (lines starting with spaces under it)
        if in_default_prompt && line.starts_with("    ") && !trimmed.is_empty() {
            prompt_lines.push(trimmed.to_string());
        }
    }

    // Handle multiline prompt
    if default_prompt.is_none() && !prompt_lines.is_empty() {
        default_prompt = Some(prompt_lines.join(" "));
    }

    display_name.map(|name| CodexAgent {
        display_name: name,
        short_description,
        default_prompt,
    })
}

/// Discover Codex agents from ~/.codex/vendor_imports/skills/skills/.curated/*/agents/openai.yaml
fn discover_codex_agents() -> Vec<UserAgentEntry> {
    let mut agents = Vec::new();

    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return agents,
    };

    // Path: ~/.codex/vendor_imports/skills/skills/.curated/{skill}/agents/openai.yaml
    let curated_path = home
        .join(".codex")
        .join("vendor_imports")
        .join("skills")
        .join("skills")
        .join(".curated");

    if !curated_path.exists() {
        return agents;
    }

    if let Ok(skill_dirs) = fs::read_dir(&curated_path) {
        for skill_dir in skill_dirs.flatten() {
            let agents_path = skill_dir.path().join("agents").join("openai.yaml");
            if agents_path.exists() {
                if let Ok(content) = fs::read_to_string(&agents_path) {
                    if let Some(agent) = parse_codex_openai_yaml(&content) {
                        // Create machine name from skill directory + "-openai"
                        let skill_name = skill_dir.file_name().to_string_lossy().to_string();
                        let id = format!("codex-{}-openai", skill_name);

                        agents.push(UserAgentEntry {
                            id,
                            name: agent.display_name,
                            source: "codex".to_string(),
                            author: "OpenAI".to_string(),
                        });
                    }
                }
            }
        }
    }

    agents
}

/// List all user agents, including discovered Codex OpenAI agents.
pub fn list_all_user_agents() -> Result<Vec<UserAgentEntry>, String> {
    let mut agents = list_user_agents()?;

    // Append discovered Codex agents, avoiding duplicates
    let codex_agents = discover_codex_agents();
    for codex_agent in codex_agents {
        if !agents.iter().any(|a| a.id == codex_agent.id) {
            agents.push(codex_agent);
        }
    }

    // Sort by name
    agents.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(agents)
}

/// Read the full agent (name + content) by machine name.
/// For Codex agents, reads from openai.yaml and converts to MD format.
/// Returns JSON: `{"name": "...", "content": "..."}`.
pub fn read_user_agent(machine_name: &str) -> Result<String, String> {
    // Check if it's a Codex agent
    if machine_name.starts_with("codex-") && machine_name.ends_with("-openai") {
        return read_codex_agent(machine_name);
    }

    // Regular local/bundled agent
    if !is_valid_agent_machine_name(machine_name) {
        return Err("Invalid agent machine name".into());
    }
    let dir = get_user_agents_dir()?;
    let path = dir.join(format!("{}.md", machine_name));

    if path.exists() {
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        let name =
            extract_name_from_frontmatter(&content).unwrap_or_else(|| machine_name.to_string());
        let agent = UserAgent { name, content };
        serde_json::to_string_pretty(&agent).map_err(|e| e.to_string())
    } else {
        Err(format!("Agent '{}' not found", machine_name))
    }
}

/// Read a Codex OpenAI agent by machine name and convert to MD format.
fn read_codex_agent(machine_name: &str) -> Result<String, String> {
    // Parse skill name from machine name: codex-{skill}-openai
    let skill_name = machine_name
        .strip_prefix("codex-")
        .and_then(|s| s.strip_suffix("-openai"))
        .ok_or("Invalid Codex agent name format")?;

    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    let yaml_path = home
        .join(".codex")
        .join("vendor_imports")
        .join("skills")
        .join("skills")
        .join(".curated")
        .join(skill_name)
        .join("agents")
        .join("openai.yaml");

    if !yaml_path.exists() {
        return Err(format!("Codex agent '{}' not found", machine_name));
    }

    let content = fs::read_to_string(&yaml_path).map_err(|e| e.to_string())?;
    let agent = parse_codex_openai_yaml(&content).ok_or("Failed to parse Codex agent YAML")?;

    // Convert to MD format with frontmatter
    let description = agent.short_description.unwrap_or_default();
    let prompt = agent.default_prompt.unwrap_or_default();

    let md_content = format!(
        "---\nname: {}\ndescription: {}\ntools: inherit\nmodel: inherit\nauthor: OpenAI\nsource: codex\n---\n\n{}\n",
        agent.display_name,
        description,
        prompt
    );

    let result = UserAgent {
        name: agent.display_name,
        content: md_content,
    };

    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

pub fn save_user_agent(machine_name: &str, name: &str, content: &str) -> Result<(), String> {
    if !is_valid_agent_machine_name(machine_name) {
        return Err(
            "Invalid agent machine name. Use lowercase letters, digits, and hyphens only.".into(),
        );
    }
    if name.trim().is_empty() {
        return Err("Agent display name cannot be empty".into());
    }

    let dir = get_user_agents_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    let path = dir.join(format!("{}.md", machine_name));
    fs::write(&path, content).map_err(|e| e.to_string())?;

    Ok(())
}

pub fn delete_user_agent(machine_name: &str) -> Result<(), String> {
    if !is_valid_agent_machine_name(machine_name) {
        return Err("Invalid agent machine name".into());
    }
    let dir = get_user_agents_dir()?;
    let path = dir.join(format!("{}.md", machine_name));

    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Built-in agents shipped with the app. Each entry is
/// (machine_name, display_name, content).
/// Written to `~/.automatic/agents/{machine_name}.md` on first run (or when missing),
/// but never overwrite existing files — user edits are preserved.
const DEFAULT_USER_AGENTS: &[(&str, &str, &str)] = &[
    (
        "automatic-code-reviewer",
        "Code Reviewer",
        include_str!("../../agents/automatic/code-reviewer.md"),
    ),
    (
        "automatic-debugger",
        "Debugger",
        include_str!("../../agents/automatic/debugger.md"),
    ),
    (
        "automatic-planner",
        "Planner",
        include_str!("../../agents/automatic/planner.md"),
    ),
];

/// Write default user agents to `~/.automatic/agents/`.
///
/// When `force` is `false`, existing files are left untouched so user edits
/// are preserved. When `force` is `true`, every bundled agent is overwritten
/// unconditionally — used by the "Reinstall Defaults" reset path.
pub fn install_default_user_agents_inner(force: bool) -> Result<(), String> {
    let dir = get_user_agents_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    for (machine_name, _display_name, content) in DEFAULT_USER_AGENTS {
        let path = dir.join(format!("{}.md", machine_name));
        if force || !path.exists() {
            fs::write(&path, content).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

/// Write any missing default user agents to `~/.automatic/agents/`.
/// Existing files are left untouched, so user edits are always preserved.
pub fn install_default_user_agents() -> Result<(), String> {
    install_default_user_agents_inner(false)
}

/// Check if a machine name refers to a bundled (read-only) agent.
pub fn is_bundled_agent(machine_name: &str) -> bool {
    DEFAULT_USER_AGENTS
        .iter()
        .any(|(name, _, _)| *name == machine_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn temp_agents_dir() -> (TempDir, PathBuf) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let agents_dir = tmp.path().join("agents");
        fs::create_dir_all(&agents_dir).expect("create agents dir");
        (tmp, agents_dir)
    }

    fn write_agent(agents_dir: &PathBuf, machine_name: &str, content: &str) {
        let path = agents_dir.join(format!("{}.md", machine_name));
        fs::write(&path, content).expect("write agent");
    }

    fn read_agent_from_dir(agents_dir: &PathBuf, machine_name: &str) -> UserAgent {
        let raw = fs::read_to_string(agents_dir.join(format!("{}.md", machine_name)))
            .expect("read agent file");
        let name = extract_name_from_frontmatter(&raw).unwrap_or_else(|| machine_name.to_string());
        UserAgent { name, content: raw }
    }

    // ── is_valid_agent_machine_name ────────────────────────────────────────────

    #[test]
    fn valid_machine_names_are_accepted() {
        assert!(is_valid_agent_machine_name("my-agent"));
        assert!(is_valid_agent_machine_name("agent1"));
        assert!(is_valid_agent_machine_name("a"));
        assert!(is_valid_agent_machine_name("automatic-code-reviewer"));
        assert!(is_valid_agent_machine_name("agent-with-numbers-123"));
    }

    #[test]
    fn empty_name_is_rejected() {
        assert!(!is_valid_agent_machine_name(""));
    }

    #[test]
    fn name_starting_with_digit_is_rejected() {
        assert!(!is_valid_agent_machine_name("1agent"));
    }

    #[test]
    fn name_starting_with_hyphen_is_rejected() {
        assert!(!is_valid_agent_machine_name("-agent"));
    }

    #[test]
    fn name_ending_with_hyphen_is_rejected() {
        assert!(!is_valid_agent_machine_name("agent-"));
    }

    #[test]
    fn consecutive_hyphens_are_rejected() {
        assert!(!is_valid_agent_machine_name("my--agent"));
    }

    #[test]
    fn uppercase_letters_are_rejected() {
        assert!(!is_valid_agent_machine_name("MyAgent"));
        assert!(!is_valid_agent_machine_name("MY-AGENT"));
    }

    #[test]
    fn special_characters_are_rejected() {
        assert!(!is_valid_agent_machine_name("my_agent"));
        assert!(!is_valid_agent_machine_name("my agent"));
        assert!(!is_valid_agent_machine_name("my/agent"));
        assert!(!is_valid_agent_machine_name("my.agent"));
    }

    #[test]
    fn name_over_128_chars_is_rejected() {
        let long = "a".repeat(129);
        assert!(!is_valid_agent_machine_name(&long));
    }

    #[test]
    fn name_of_exactly_128_chars_is_accepted() {
        let name = format!("a{}", "b".repeat(127));
        assert!(is_valid_agent_machine_name(&name));
    }

    // ── frontmatter parsing ──────────────────────────────────────────────────────

    #[test]
    fn extract_name_from_valid_frontmatter() {
        let content = "---\nname: my-agent\ndescription: Test agent\n---\n\nBody content";
        assert_eq!(
            extract_name_from_frontmatter(content),
            Some("my-agent".to_string())
        );
    }

    #[test]
    fn extract_name_with_quotes() {
        let content = "---\nname: \"My Agent\"\n---\n\nBody";
        assert_eq!(
            extract_name_from_frontmatter(content),
            Some("My Agent".to_string())
        );
    }

    #[test]
    fn extract_name_missing_frontmatter() {
        let content = "# My Agent\n\nNo frontmatter here";
        assert_eq!(extract_name_from_frontmatter(content), None);
    }

    #[test]
    fn extract_name_empty_frontmatter() {
        let content = "---\n---\n\nBody";
        assert_eq!(extract_name_from_frontmatter(content), None);
    }

    // ── CRUD operations ─────────────────────────────────────────────────────────

    #[test]
    fn save_and_read_agent_roundtrip() {
        let (_tmp, agents_dir) = temp_agents_dir();
        let content = "---\nname: Test Agent\n---\n\nBody content";
        write_agent(&agents_dir, "test-agent", content);

        let agent = read_agent_from_dir(&agents_dir, "test-agent");
        assert_eq!(agent.name, "Test Agent");
        assert!(agent.content.contains("Body content"));
    }

    #[test]
    fn bundled_agents_are_bundled() {
        assert!(is_bundled_agent("automatic-code-reviewer"));
        assert!(is_bundled_agent("automatic-debugger"));
        assert!(is_bundled_agent("automatic-planner"));
        assert!(!is_bundled_agent("custom-agent"));
    }
}
