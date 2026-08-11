use std::collections::HashSet;

use serde_json::{json, Map, Value};
use std::fs;
use std::path::PathBuf;

use crate::core::Project;

/// Load MCP server configs from the Automatic registry (~/.automatic/mcp_servers/).
pub(crate) fn load_mcp_server_configs() -> Result<Map<String, Value>, String> {
    let names = crate::core::list_mcp_server_configs()?;
    let mut servers = Map::new();

    for name in names {
        match crate::core::read_mcp_server_config(&name) {
            Ok(raw) => {
                if let Ok(config) = serde_json::from_str::<Value>(&raw) {
                    servers.insert(name, config);
                }
            }
            Err(_) => continue,
        }
    }

    Ok(servers)
}

/// Read all skill contents from the global registry for the given names.
///
/// Uses the raw SKILL.md content (without companion resource formatting) so
/// that the content written to project skill files during sync matches exactly
/// what drift detection compares against.  Companion files are handled
/// separately by `copy_skills_to_project` which copies the full directory.
pub(crate) fn load_skill_contents(skill_names: &[String]) -> Vec<(String, String)> {
    let mut contents = Vec::new();
    for name in skill_names {
        match crate::core::read_skill_raw(name) {
            Ok(content) if !content.is_empty() => {
                contents.push((name.clone(), content));
            }
            _ => {}
        }
    }
    contents
}

/// Build the combined skill contents and custom skill name list for a project,
/// deduplicating custom_skills entries whose names already appear in the
/// project's library-backed `skills` list.  Library wins because
/// `copy_skills_to_project` writes library content to disk; a stale
/// custom_skills snapshot would otherwise produce perpetual drift.
pub(crate) fn build_skill_contents(project: &Project) -> (Vec<(String, String)>, Vec<String>) {
    let mut skill_contents = load_skill_contents(&project.skills);
    let library_skill_names: HashSet<&str> = project.skills.iter().map(|s| s.as_str()).collect();
    let custom_skills = project.custom_skills.as_deref().unwrap_or(&[]);
    for cs in custom_skills {
        if library_skill_names.contains(cs.name.as_str()) {
            continue;
        }
        skill_contents.push((cs.name.clone(), cs.content.clone()));
    }
    let custom_skill_names: Vec<String> = custom_skills
        .iter()
        .filter(|s| !library_skill_names.contains(s.name.as_str()))
        .map(|s| s.name.clone())
        .collect();
    (skill_contents, custom_skill_names)
}

/// Prune `custom_skills` entries whose name already appears in the project's
/// library-backed `skills` list.  Returns `true` if any entries were removed.
pub(crate) fn prune_shadowed_custom_skills(project: &mut Project) -> bool {
    let library_names: HashSet<&str> = project.skills.iter().map(|s| s.as_str()).collect();
    let Some(custom) = project.custom_skills.as_mut() else {
        return false;
    };
    let before = custom.len();
    custom.retain(|cs| !library_names.contains(cs.name.as_str()));
    let changed = custom.len() != before;
    if custom.is_empty() {
        project.custom_skills = None;
    }
    changed
}

/// Which project-scoped custom asset kind diverged from disk.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomAssetKind {
    Skill,
    Rule,
    Agent,
    Command,
}

/// Conflict when a project-scoped custom asset's on-disk file differs from the
/// content stored in the project config.
///
/// Unlike library assets (Automatic is source of truth and overwrites on sync),
/// custom skills/rules/agents/commands are authored per-project. When disk and
/// stored content diverge, the user must choose: adopt the on-disk version into
/// Automatic, or overwrite disk with the stored version. Sync favours on-disk
/// and will not silently clobber it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CustomAssetConflict {
    pub kind: CustomAssetKind,
    /// Display / machine name of the asset (skill dir, rule name, agent name, command name).
    pub name: String,
    /// Relative path of the on-disk file from the project (or silent) root.
    pub path: String,
    pub disk_content: String,
    pub automatic_content: String,
}

/// Managed header written at the top of `.automatic/instructions/*.md` files.
const INSTRUCTIONS_MANAGED_HEADER: &str =
    "<!-- managed by Automatic — do not edit by hand -->\n\n";

fn strip_instructions_managed_header(content: &str) -> String {
    content
        .strip_prefix(INSTRUCTIONS_MANAGED_HEADER)
        .unwrap_or(content)
        .trim_end()
        .to_string()
}

fn custom_rule_slug(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Collect conflicts for all project-scoped custom assets (skills, rules,
/// agents, commands). Missing on-disk files are not conflicts — sync creates
/// them from the stored snapshot.
pub(crate) fn collect_custom_asset_conflicts(
    project: &Project,
    dir: &std::path::Path,
) -> Vec<CustomAssetConflict> {
    let mut conflicts = Vec::new();
    conflicts.extend(collect_custom_skill_conflicts(project, dir));
    conflicts.extend(collect_custom_rule_conflicts(project, dir));
    conflicts.extend(collect_custom_agent_conflicts(project, dir));
    conflicts.extend(collect_custom_command_conflicts(project, dir));
    conflicts
}

/// Compare each non-library `custom_skills` entry against
/// `.agents/skills/<name>/SKILL.md` under `dir`.
pub(crate) fn collect_custom_skill_conflicts(
    project: &Project,
    dir: &std::path::Path,
) -> Vec<CustomAssetConflict> {
    let library_names: HashSet<&str> = project.skills.iter().map(|s| s.as_str()).collect();
    let custom = project.custom_skills.as_deref().unwrap_or(&[]);
    let mut conflicts = Vec::new();

    for cs in custom {
        if library_names.contains(cs.name.as_str()) {
            continue;
        }
        let skill_md = dir
            .join(".agents")
            .join("skills")
            .join(&cs.name)
            .join("SKILL.md");
        if !skill_md.exists() {
            continue;
        }
        let disk_content = match fs::read_to_string(&skill_md) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if disk_content == cs.content {
            continue;
        }
        conflicts.push(CustomAssetConflict {
            kind: CustomAssetKind::Skill,
            name: cs.name.clone(),
            path: format!(".agents/skills/{}/SKILL.md", cs.name),
            disk_content,
            automatic_content: cs.content.clone(),
        });
    }

    conflicts
}

/// Compare each `custom_rules` entry against
/// `.automatic/instructions/custom-<slug>.md` under the real project directory
/// (instructions always live at the project root, even in Silent mode).
pub(crate) fn collect_custom_rule_conflicts(
    project: &Project,
    _dir: &std::path::Path,
) -> Vec<CustomAssetConflict> {
    // Custom rule files always live under the real project directory.
    let root = std::path::Path::new(&project.directory);
    let mut conflicts = Vec::new();

    for cr in &project.custom_rules {
        if cr.content.trim().is_empty() {
            continue;
        }
        let slug = custom_rule_slug(&cr.name);
        let path = root
            .join(".automatic")
            .join("instructions")
            .join(format!("custom-{}.md", slug));
        if !path.exists() {
            continue;
        }
        let raw = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let disk_body = strip_instructions_managed_header(&raw);
        let stored_body = cr.content.trim_end().to_string();
        if disk_body == stored_body {
            continue;
        }
        conflicts.push(CustomAssetConflict {
            kind: CustomAssetKind::Rule,
            name: cr.name.clone(),
            path: format!(".automatic/instructions/custom-{}.md", slug),
            disk_content: disk_body,
            automatic_content: stored_body,
        });
    }

    conflicts
}

/// Compare each `custom_agents` entry against the first on-disk agent file
/// found under any configured agent's `agents_dir`.
pub(crate) fn collect_custom_agent_conflicts(
    project: &Project,
    dir: &std::path::Path,
) -> Vec<CustomAssetConflict> {
    let custom = project.custom_agents.as_deref().unwrap_or(&[]);
    let mut conflicts = Vec::new();

    for ca in custom {
        let machine_name = extract_agent_machine_name(&ca.content)
            .unwrap_or_else(|| ca.name.to_lowercase().replace(' ', "-"));

        for agent_id in &project.agents {
            let Some(agent_instance) = crate::agent::from_id(agent_id) else {
                continue;
            };
            let Some(agents_dir) = agent_instance.agents_dir(dir) else {
                continue;
            };
            let ext = agent_instance.agents_file_ext();
            let file_path = agents_dir.join(format!("{}.{}", machine_name, ext));
            if !file_path.exists() {
                continue;
            }
            let disk_content = match fs::read_to_string(&file_path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let expected =
                agent_instance.convert_agent_content(&ca.content, &machine_name);
            if disk_content == expected {
                break; // matches this agent dir — no conflict
            }
            let relative = file_path
                .strip_prefix(dir)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| file_path.display().to_string());
            conflicts.push(CustomAssetConflict {
                kind: CustomAssetKind::Agent,
                name: ca.name.clone(),
                path: relative,
                disk_content,
                automatic_content: expected,
            });
            break; // one conflict entry per custom agent
        }
    }

    conflicts
}

/// Compare each `custom_commands` entry against `.agents/commands/<name>.md`.
pub(crate) fn collect_custom_command_conflicts(
    project: &Project,
    dir: &std::path::Path,
) -> Vec<CustomAssetConflict> {
    let custom = project.custom_commands.as_deref().unwrap_or(&[]);
    let mut conflicts = Vec::new();

    for cc in custom {
        let cmd_path = dir
            .join(".agents")
            .join("commands")
            .join(format!("{}.md", cc.name));
        if !cmd_path.exists() {
            continue;
        }
        let disk_content = match fs::read_to_string(&cmd_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let expected = crate::agent::render_markdown_command(&cc.content);
        if disk_content == expected {
            continue;
        }
        conflicts.push(CustomAssetConflict {
            kind: CustomAssetKind::Command,
            name: cc.name.clone(),
            path: format!(".agents/commands/{}.md", cc.name),
            disk_content,
            automatic_content: expected,
        });
    }

    conflicts
}

/// Names of conflicting assets of a given kind.
pub(crate) fn conflicting_names(
    conflicts: &[CustomAssetConflict],
    kind: CustomAssetKind,
) -> HashSet<String> {
    conflicts
        .iter()
        .filter(|c| c.kind == kind)
        .map(|c| c.name.clone())
        .collect()
}

/// Find the Automatic binary path.
///
/// Delegates to the canonical resolver so the sync engine, drift check, and
/// MCP registry all emit the identical path string regardless of how the
/// current process was invoked (GUI bundle vs CLI symlink).
pub(crate) fn find_automatic_binary() -> String {
    crate::core::automatic_binary_path()
}

/// Strip any legacy `<!-- automatic:skills:start -->…<!-- automatic:skills:end -->`
/// managed section from a project file.  Returns the path if the file was
/// modified, or None if no cleanup was needed.
pub(crate) fn clean_project_file(dir: &PathBuf, filename: &str) -> Result<Option<String>, String> {
    let path = dir.join(filename);
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;

    let start_marker = "<!-- automatic:skills:start -->";
    let end_marker = "<!-- automatic:skills:end -->";

    if let (Some(start), Some(end)) = (content.find(start_marker), content.find(end_marker)) {
        let before = &content[..start];
        let after = &content[end + end_marker.len()..];
        let cleaned = format!(
            "{}{}",
            before.trim_end(),
            if after.trim().is_empty() {
                "\n".to_string()
            } else {
                format!("\n\n{}", after.trim_start())
            }
        );
        fs::write(&path, cleaned).map_err(|e| e.to_string())?;
        Ok(Some(path.display().to_string()))
    } else {
        Ok(None)
    }
}

pub(crate) fn add_unique(items: &mut Vec<String>, value: &str) -> bool {
    if items.iter().any(|v| v == value) {
        false
    } else {
        items.push(value.to_string());
        true
    }
}

/// Build the selected MCP server map for a project, applying all
/// transformations that the sync engine uses (stripping internal `_`-prefixed
/// fields, substituting OAuth proxy configs for HTTP servers with stored
/// tokens).
///
/// Both `engine.rs` and `drift.rs` must use this function to ensure the
/// expected config matches what is actually written to disk.
pub(crate) fn build_selected_servers(
    project_name: &str,
    server_names: &[String],
    mcp_config: &Map<String, Value>,
) -> Map<String, Value> {
    let mut selected_servers = Map::new();
    let automatic_binary = find_automatic_binary();

    // Always include the Automatic MCP server
    selected_servers.insert(
        "automatic".to_string(),
        json!({
            "command": automatic_binary,
            "args": ["mcp-serve"],
            "env": {
                "AUTOMATIC_PROJECT": project_name
            }
        }),
    );

    // Add project-selected MCP servers from the Automatic registry.
    // Strip Automatic-internal fields (prefixed with `_`) before writing to agent files.
    //
    // For HTTP servers that have a stored OAuth token in the keychain, we emit
    // a local stdio proxy config instead of the remote URL.  This keeps the
    // token out of every project file — the proxy loads it from the keychain
    // at runtime.
    for server_name in server_names {
        if server_name == "automatic" {
            continue;
        }
        if let Some(server_config) = mcp_config.get(server_name) {
            let cleaned = strip_internal_fields(server_config.clone());

            // Check if this is an HTTP server with a stored OAuth token. Only
            // HTTP and SSE servers can have one, and reading the keychain is
            // expensive enough that stdio servers should not pay for a lookup
            // whose result is discarded.
            let is_http = cleaned
                .get("type")
                .and_then(|v| v.as_str())
                .map(|t| t == "http" || t == "sse")
                .unwrap_or(false);

            if is_http && crate::proxy::has_oauth_token(server_name) {
                // Emit a local proxy config instead of the remote URL.
                selected_servers.insert(
                    server_name.clone(),
                    json!({
                        "command": automatic_binary,
                        "args": ["mcp-proxy", server_name],
                    }),
                );
            } else {
                // Empty env values are left as-is here: they are the canonical
                // "inherit from the environment" marker, and each agent renders
                // them in its own placeholder syntax via
                // `agent::prepare_mcp_servers` at write time.
                selected_servers.insert(server_name.clone(), cleaned);
            }
        }
    }

    selected_servers
}

/// Remove fields whose names start with `_` from a JSON object.
/// These are Automatic-internal metadata fields (e.g. `_author`) that should
/// never be written to agent configuration files.
pub(crate) fn strip_internal_fields(mut value: Value) -> Value {
    if let Value::Object(ref mut map) = value {
        map.retain(|key, _| !key.starts_with('_'));
    }
    value
}

/// Extract the machine name (slug) from agent frontmatter content.
/// Returns the `name` field converted to lowercase with spaces replaced by hyphens.
pub fn extract_agent_machine_name(content: &str) -> Option<String> {
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
                return Some(name.to_lowercase().replace(' ', "-"));
            }
        }
    }
    None
}

/// Sync project-local custom agents to a project's agents directory.
///
/// For each custom agent:
/// 1. Extract the machine name from the agent's frontmatter
/// 2. Write to `agents_dir/{machine_name}.md`
///
/// Agents whose names appear in `skip_names` are left untouched on disk
/// (conflict: favour on-disk content until the user resolves it).
///
/// Returns the list of files written.
pub(crate) fn sync_custom_agents(
    agents_dir: &std::path::Path,
    custom_agents: &[crate::core::CustomAgent],
    agent: &dyn crate::agent::Agent,
    skip_names: &HashSet<String>,
) -> Result<Vec<String>, String> {
    if custom_agents.is_empty() {
        return Ok(Vec::new());
    }

    if !agents_dir.exists() {
        fs::create_dir_all(agents_dir).map_err(|e| e.to_string())?;
    }

    let mut written = Vec::new();

    for custom_agent in custom_agents {
        if skip_names.contains(&custom_agent.name) {
            continue;
        }
        let machine_name = extract_agent_machine_name(&custom_agent.content)
            .unwrap_or_else(|| custom_agent.name.to_lowercase().replace(' ', "-"));
        let converted_content = agent.convert_agent_content(&custom_agent.content, &machine_name);
        let path = agents_dir.join(agent.agent_file_name(&machine_name));

        fs::write(&path, &converted_content).map_err(|e| e.to_string())?;
        written.push(path.display().to_string());
    }

    Ok(written)
}

/// Clean up Automatic-managed custom agent files from an agents directory.
/// Used when removing an agent from a project.
///
/// Gated on [`crate::agent::is_managed_agent_file`] rather than file
/// extension alone: an extension-only check would delete every file sharing
/// that extension, managed or not, which is destructive for a directory a
/// user might place hand-authored agents into directly (`.github/agents/`)
/// rather than one that is effectively Automatic's own (`.claude/agents/`
/// has always worked this way, but that was never actually safe — just
/// unexercised, since nobody hand-authors into `.claude/agents/`).
///
/// Returns the list of files removed.
pub(crate) fn cleanup_custom_agents(agents_dir: &std::path::Path) -> Vec<String> {
    let mut removed = Vec::new();

    if agents_dir.exists() {
        if let Ok(entries) = fs::read_dir(agents_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && crate::agent::is_managed_agent_file(&path) {
                    if fs::remove_file(&path).is_ok() {
                        removed.push(path.display().to_string());
                    }
                }
            }
        }
        // Remove the agents directory itself if now empty
        let _ = fs::remove_dir(agents_dir);
    }

    removed
}

/// Sync workspace user agents (from `~/.automatic/agents/`) to a project's
/// agents directory.
///
/// For each selected agent:
/// 1. Read the agent content from the global registry
/// 2. Convert to the target format if needed (e.g., TOML for Codex)
/// 3. Write to `agents_dir/{agent_file_name(machine_name)}`
/// 4. Remove stale *managed* agent files not in the selected list (but NOT
///    custom agents, and NOT anything the user placed there by hand)
pub(crate) fn sync_user_agents(
    agents_dir: &std::path::Path,
    user_agent_names: &[String],
    custom_agent_names: &[String],
    agent: &dyn crate::agent::Agent,
) -> Result<Vec<String>, String> {
    if !agents_dir.exists() {
        fs::create_dir_all(agents_dir).map_err(|e| e.to_string())?;
    }

    let mut written = Vec::new();
    // Full filenames, not bare machine names: `agent_file_name` may add a
    // compound extension (Copilot's `{name}.agent.md`), and `Path::extension`
    // only ever sees the last dot-segment, so comparing by filename is the
    // only way to recover the same name that went in — see
    // `Agent::agent_file_name`'s doc comment.
    let mut expected_file_names: std::collections::HashSet<String> =
        std::collections::HashSet::new();

    // Write each selected agent
    for name in user_agent_names {
        if let Ok(content) = crate::core::read_subagent(name) {
            let user_agent: crate::core::Subagent =
                serde_json::from_str(&content).map_err(|e| e.to_string())?;
            let machine_name = extract_agent_machine_name(&user_agent.content)
                .unwrap_or_else(|| name.to_lowercase().replace(' ', "-"));
            let converted_content = agent.convert_agent_content(&user_agent.content, &machine_name);
            let file_name = agent.agent_file_name(&machine_name);
            let path = agents_dir.join(&file_name);

            fs::write(&path, &converted_content).map_err(|e| e.to_string())?;
            written.push(path.display().to_string());
            expected_file_names.insert(file_name);
        }
    }

    // Also add custom agent filenames to the expected set so they're not
    // removed as stale.
    for name in custom_agent_names {
        expected_file_names.insert(agent.agent_file_name(name));
    }

    // Remove stale *managed* agent files: not in the expected set, and
    // carrying the automatic-managed marker. A file that fails the marker
    // check is left alone unconditionally — it might be stale, or it might
    // be something the user wrote directly in this directory; without the
    // marker there is no way to tell, so the safe default is to preserve it.
    if agents_dir.exists() {
        if let Ok(entries) = fs::read_dir(agents_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
                    continue;
                };
                if expected_file_names.contains(file_name)
                    || !crate::agent::is_managed_agent_file(&path)
                {
                    continue;
                }
                if fs::remove_file(&path).is_ok() {
                    written.push(path.display().to_string());
                }
            }
        }
    }

    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, ClaudeCode, GitHubCopilot};
    use crate::core::{save_subagent, CustomAgent};
    use std::sync::{Mutex, OnceLock};
    use tempfile::TempDir;

    fn tmp() -> TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct HomeGuard {
        original: Option<String>,
    }

    impl HomeGuard {
        fn set(path: &std::path::Path) -> Self {
            let original = std::env::var("HOME").ok();
            unsafe {
                std::env::set_var("HOME", path);
            }
            Self { original }
        }
    }

    impl Drop for HomeGuard {
        fn drop(&mut self) {
            unsafe {
                if let Some(original) = &self.original {
                    std::env::set_var("HOME", original);
                } else {
                    std::env::remove_var("HOME");
                }
            }
        }
    }

    #[test]
    fn sync_custom_agents_uses_frontmatter_name_and_fallback_name() {
        let dir = tmp();
        let agents_dir = dir.path().join("agents");
        let custom_agents = vec![
            CustomAgent {
                name: "Display Name".to_string(),
                content: "---\nname: Planner Agent\n---\n\nPlan carefully.\n".to_string(),
            },
            CustomAgent {
                name: "Fallback Agent".to_string(),
                content: "No frontmatter here.\n".to_string(),
            },
        ];

        let written = sync_custom_agents(&agents_dir, &custom_agents, &ClaudeCode, &HashSet::new())
            .expect("sync");

        assert_eq!(written.len(), 2);
        assert!(agents_dir.join("planner-agent.md").exists());
        assert!(agents_dir.join("fallback-agent.md").exists());
        assert!(fs::read_to_string(agents_dir.join("planner-agent.md"))
            .expect("read planner")
            .contains("Plan carefully."));
    }

    #[test]
    fn sync_user_agents_writes_selected_and_removes_stale_non_custom_files() {
        let _lock = env_lock().lock().expect("env lock");
        let home = tmp();
        let _home_guard = HomeGuard::set(home.path());
        let project = tmp();
        let agents_dir = project.path().join("agents");
        fs::create_dir_all(&agents_dir).expect("create agents dir");

        save_subagent(
            "reviewer-agent",
            "Reviewer Agent",
            "---\nname: Reviewer Agent\n---\n\nReview thoroughly.\n",
        )
        .expect("save reviewer subagent");

        // Simulates a file Automatic itself wrote in an earlier sync for a
        // user agent that has since been removed from the project: carries
        // the managed marker, so the stale sweep must remove it.
        let stale_managed_path = agents_dir.join("stale-agent.md");
        fs::write(
            &stale_managed_path,
            "---\nname: Stale Agent\nautomatic-managed: true\n---\n\nOld content.\n",
        )
        .expect("write stale managed agent");

        // A file the user placed in this directory directly — no marker.
        // Regardless of whether its name happens to collide with anything
        // expected, the stale sweep must never touch it: there is no way to
        // tell it apart from Automatic's own output without the marker, so
        // the safe default is to leave it alone.
        let hand_authored_path = agents_dir.join("hand-authored.md");
        fs::write(
            &hand_authored_path,
            "---\nname: Hand Authored\n---\n\nI wrote this myself.\n",
        )
        .expect("write hand-authored agent");

        let custom_path = agents_dir.join("custom-agent.md");
        fs::write(&custom_path, "---\nname: Custom Agent\n---\n\nKeep me.\n")
            .expect("write custom agent");

        let written = sync_user_agents(
            &agents_dir,
            &["reviewer-agent".to_string()],
            &["custom-agent".to_string()],
            &ClaudeCode,
        )
        .expect("sync user agents");

        assert!(written.iter().any(|p| p.ends_with("reviewer-agent.md")));
        assert!(written.iter().any(|p| p.ends_with("stale-agent.md")));
        assert!(agents_dir.join("reviewer-agent.md").exists());
        assert!(!stale_managed_path.exists());
        assert!(hand_authored_path.exists());
        assert!(custom_path.exists());
        assert!(fs::read_to_string(agents_dir.join("reviewer-agent.md"))
            .expect("read synced user agent")
            .contains("Review thoroughly."));
    }

    /// GitHub Copilot's `{name}.agent.md` compound extension is exactly the
    /// case that broke the old `file_stem()`-based stale sweep: `mine.agent.md`
    /// stems to `mine.agent`, which fails `is_valid_agent_machine_name` and
    /// so was neither recognised as expected nor swept as stale. The marker
    /// gate now makes that moot — the file simply carries no
    /// `automatic-managed: true` marker, so it survives on that basis alone,
    /// regardless of whether its name happens to collide with Automatic's
    /// own convention.
    #[test]
    fn copilot_hand_written_agent_file_survives_the_stale_sweep() {
        let _lock = env_lock().lock().expect("env lock");
        let home = tmp();
        let _home_guard = HomeGuard::set(home.path());
        let project = tmp();
        let agents_dir = project.path().join("agents");
        fs::create_dir_all(&agents_dir).expect("create agents dir");

        let hand_written_path = agents_dir.join("mine.agent.md");
        fs::write(
            &hand_written_path,
            "---\nname: Mine\n---\n\nI wrote this myself.\n",
        )
        .expect("write hand-written copilot agent");

        sync_user_agents(&agents_dir, &[], &[], &GitHubCopilot).expect("sync user agents");

        assert!(
            hand_written_path.exists(),
            "hand-written .github/agents/mine.agent.md must survive the stale sweep"
        );
    }

    /// The same file must also survive `cleanup_custom_agents`, called when
    /// Copilot is removed from the project entirely — not just the
    /// in-place stale sweep `sync_user_agents` runs on every sync.
    #[test]
    fn copilot_hand_written_agent_file_survives_agent_removal() {
        let project = tmp();
        let agents_dir = project.path().join("agents");
        fs::create_dir_all(&agents_dir).expect("create agents dir");

        let hand_written_path = agents_dir.join("mine.agent.md");
        fs::write(
            &hand_written_path,
            "---\nname: Mine\n---\n\nI wrote this myself.\n",
        )
        .expect("write hand-written copilot agent");

        let managed_path = agents_dir.join("reviewer-agent.agent.md");
        fs::write(
            &managed_path,
            GitHubCopilot
                .convert_agent_content("---\nname: Reviewer\n---\n\nReview.\n", "reviewer-agent"),
        )
        .expect("write managed copilot agent");

        let removed = cleanup_custom_agents(&agents_dir);

        assert!(
            hand_written_path.exists(),
            "hand-written .github/agents/mine.agent.md must survive removing Copilot from the project"
        );
        assert!(!managed_path.exists());
        assert!(removed
            .iter()
            .any(|p| p.ends_with("reviewer-agent.agent.md")));
    }
}
