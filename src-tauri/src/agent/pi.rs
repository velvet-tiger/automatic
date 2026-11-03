use serde_json::{json, Map, Value};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, Agent};

/// Pi agent — pi.dev's minimal agent harness.
///
/// - **MCP** — built into Pi.  It reads `.pi/mcp.json` once the project is
///   trusted, and `mcp.json` in its agent directory.  Same JSON shape as Claude
///   Code: `{ "mcpServers": { ... } }`, except that the legacy SSE transport
///   is rejected.  `pi-mcp-adapter` is an optional replacement that reads the
///   same project file.
/// - **Sub-agents** — provided by `pi-subagents` and forks.  Stored as
///   Markdown files with YAML frontmatter under `.pi/agents/`.  The parser
///   reads one line per key, so [`Agent::convert_agent_content`] flattens the
///   frontmatter.
/// - **Commands** — prompt templates, as Markdown files directly in
///   `.pi/prompts/`.
///
/// Project instructions live in `AGENTS.md` at the project root.  Skills are
/// written to `.pi/skills/<name>/SKILL.md`.
///
/// `.pi/` also holds the user's `settings.json`, `extensions/` and
/// `SYSTEM.md`, and ThinkRail runs on the same files, so neither agent owns
/// the directory: each claims only the paths Automatic writes.
pub struct Pi;

impl Agent for Pi {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "pi"
    }

    fn label(&self) -> &'static str {
        "Pi (Beta)"
    }

    fn config_description(&self) -> &'static str {
        ".pi/mcp.json"
    }

    fn project_file_name(&self) -> &'static str {
        "AGENTS.md"
    }

    /// Trust is enforced by Pi, not by Automatic: a synced server stays
    /// inactive until the user trusts the project.
    fn mcp_note(&self) -> Option<&'static str> {
        Some(
            "Pi loads .pi/mcp.json only after you trust the project. Automatic leaves out \
             legacy SSE servers because Pi rejects them. If you replace Pi's MCP support with \
             pi-mcp-adapter, headless runs skip unapproved servers unless \
             settings.projectServers is \"allow\" in ~/.pi/agent/mcp-adapter.json.",
        )
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_global_install(&self) -> bool {
        super::cli_available("pi")
            || super::home_dir()
                .map(|h| h.join(".pi").exists())
                .unwrap_or(false)
    }

    fn detect_in(&self, dir: &Path) -> bool {
        // `.pi/` is Pi's canonical project-level marker directory.  Do NOT
        // match on `AGENTS.md` alone — that file is shared with Codex CLI,
        // Gemini CLI and Warp and would produce false positives.
        dir.join(".pi").is_dir()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".pi").join("skills")]
    }

    fn extra_global_skill_dirs(&self) -> Vec<PathBuf> {
        match pi_agent_dir() {
            Some(agent_dir) => vec![agent_dir.join("skills")],
            None => vec![],
        }
    }

    // ── Capabilities ────────────────────────────────────────────────────

    fn capabilities(&self) -> super::AgentCapabilities {
        super::AgentCapabilities {
            commands: true,
            global_mcp_servers: true,
            ..Default::default()
        }
    }

    // ── Config writing ──────────────────────────────────────────────────

    /// Write MCP servers in Claude-compatible JSON format to `.pi/mcp.json`.
    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String> {
        let pi_dir = dir.join(".pi");
        if !pi_dir.exists() {
            fs::create_dir_all(&pi_dir).map_err(|e| format!("Failed to create .pi/: {}", e))?;
        }

        let output = json!({ "mcpServers": Value::Object(render_pi_servers(servers)) });
        let path = pi_dir.join("mcp.json");
        let content =
            serde_json::to_string_pretty(&output).map_err(|e| format!("JSON error: {}", e))?;
        fs::write(&path, content).map_err(|e| format!("Failed to write .pi/mcp.json: {}", e))?;

        Ok(path.display().to_string())
    }

    fn mcp_unsupported_reason(&self, server: &Value) -> Option<String> {
        is_sse(server).then(|| {
            "Pi rejects the legacy SSE transport. Use the server's streamable HTTP URL."
                .to_string()
        })
    }

    fn global_mcp_target(&self) -> Option<super::GlobalMcpTarget> {
        Some(super::GlobalMcpTarget {
            path: pi_agent_dir()?.join("mcp.json"),
            reload_note: Some("Pi picks up the change in new sessions."),
        })
    }

    fn write_global_mcp_config(
        &self,
        desired: &Map<String, Value>,
        previously_managed: &[String],
    ) -> Result<super::GlobalMcpWriteReport, String> {
        let target = self
            .global_mcp_target()
            .ok_or_else(|| "Home directory not available for Pi global MCP write".to_string())?;
        let rendered = render_pi_servers(desired);
        super::merge_global_mcp_entries_json(
            &target.path,
            "mcpServers",
            &rendered,
            previously_managed,
        )
    }

    // ── Sub-agents ──────────────────────────────────────────────────────

    /// Sub-agents are stored as Markdown + YAML frontmatter under `.pi/agents/`,
    /// matching the format consumed by `pi-subagents` and its forks.
    fn agents_dir(&self, dir: &Path) -> Option<PathBuf> {
        Some(dir.join(".pi").join("agents"))
    }

    fn convert_agent_content(&self, content: &str, _name: &str) -> String {
        super::render_markdown_command(&convert_pi_agent(content).markdown)
    }

    fn agent_content_refusal(&self, content: &str) -> Option<String> {
        convert_pi_agent(content).refusal
    }

    fn agent_content_warnings(&self, content: &str) -> Vec<String> {
        convert_pi_agent(content)
            .dropped_tools
            .into_iter()
            .map(|tool| format!("dropped {tool}, which has no Pi equivalent"))
            .collect()
    }

    // ── Commands ────────────────────────────────────────────────────────

    fn commands_dir(&self, dir: &Path) -> Option<PathBuf> {
        Some(dir.join(".pi").join("prompts"))
    }

    // ── Discovery ───────────────────────────────────────────────────────

    fn discover_mcp_servers(&self, dir: &Path) -> Map<String, Value> {
        let path = dir.join(".pi").join("mcp.json");
        if !path.exists() {
            return Map::new();
        }
        // Same shape as Claude Code — no normalisation needed.
        discover_mcp_servers_from_json(&path, "mcpServers", |v| v)
    }

    fn discover_global_mcp_servers(&self) -> Map<String, Value> {
        let Some(agent_dir) = pi_agent_dir() else {
            return Map::new();
        };
        // Pi's user-level location.  The shared `~/.config/mcp/mcp.json` is
        // intentionally not read here — it is the cross-tool shared config
        // and another agent may already own it.
        discover_mcp_servers_from_json(&agent_dir.join("mcp.json"), "mcpServers", |v| v)
    }

    // ── Cleanup ─────────────────────────────────────────────────────────

    /// Pi exclusively owns `.pi/mcp.json`.  `AGENTS.md` is shared with Codex,
    /// Gemini and Warp and is NOT listed here — removing Pi must not delete
    /// it if any of those agents are still selected.
    fn owned_config_paths(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".pi").join("mcp.json")]
    }
}

/// Pi's user-level directory: `$PI_CODING_AGENT_DIR` when set, otherwise
/// `~/.pi/agent`.
fn pi_agent_dir() -> Option<PathBuf> {
    resolve_pi_agent_dir(std::env::var_os("PI_CODING_AGENT_DIR"), super::home_dir())
}

fn resolve_pi_agent_dir(configured: Option<OsString>, home: Option<PathBuf>) -> Option<PathBuf> {
    let configured = configured.filter(|value| !value.is_empty());
    let Some(configured) = configured else {
        return home.map(|home| home.join(".pi").join("agent"));
    };
    // Pi expands a leading `~` itself, so the shell may not have.
    let path = PathBuf::from(&configured);
    match path.strip_prefix("~") {
        Ok(rest) => home.map(|home| home.join(rest)),
        Err(_) => Some(path),
    }
}

fn is_sse(server: &Value) -> bool {
    server.get("type").and_then(|v| v.as_str()) == Some("sse")
}

/// Render servers into Pi's dialect: leave out legacy SSE entries, drop
/// Automatic's own `enabled` and `timeout` fields, and strip `type` from stdio
/// entries (Pi infers the transport from `command` vs `url`).
///
/// `timeout` means seconds to Pi, and nothing says Automatic's value does.
fn render_pi_servers(servers: &Map<String, Value>) -> Map<String, Value> {
    let mut pi_servers = Map::new();
    for (name, config) in servers {
        if is_sse(config) {
            continue;
        }
        let transport = config
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("stdio");

        let mut server = config.clone();
        if let Some(obj) = server.as_object_mut() {
            obj.remove("enabled");
            obj.remove("timeout");
            if transport == "stdio" {
                obj.remove("type");
            }
        }
        pi_servers.insert(name.clone(), server);
    }
    pi_servers
}

/// One sub-agent rewritten for `pi-subagents`.
struct PiAgentConversion {
    markdown: String,
    /// Tool entries with no Pi equivalent, left out of the allow list.
    dropped_tools: Vec<String>,
    /// Why this definition must not be written at all.
    refusal: Option<String>,
}

/// Rewrite a Claude-style sub-agent for `pi-subagents`.
///
/// Its parser reads one `key: value` per line, so a multi-line value would be
/// misread.  A `model` it cannot resolve fails the spawn with no fallback, so
/// only a `provider/id` pin is kept.
///
/// `tools` must fail closed.  With no `tools` key the sub-agent gets Pi's
/// defaults (`read`, `bash`, `edit`, `write`) and extension tools, so dropping
/// the key from a restricted definition would widen it.  An empty list is an
/// allow list that grants nothing, and `parseNameList` accepts `tools: []`.
/// Pi has no deny list either, so a denied tool leaves the allow list.  With
/// no allow list to remove it from the definition is refused, and the output
/// still carries `tools: []` so that no write path can widen it.
fn convert_pi_agent(content: &str) -> PiAgentConversion {
    let unchanged = |markdown: String| PiAgentConversion {
        markdown,
        dropped_tools: Vec::new(),
        refusal: None,
    };
    let (Some(yaml), Some(span)) = (super::frontmatter_block(content), super::frontmatter_span(content))
    else {
        return unchanged(content.to_string());
    };
    let line_break = &content[3..span.start];

    let mut lines: Vec<String> = Vec::new();
    let mut tools_at: Option<usize> = None;
    let mut allow_lists: Vec<Vec<String>> = Vec::new();
    let mut denied: Vec<String> = Vec::new();
    for entry in frontmatter_entries(yaml) {
        match tool_key(&entry.key) {
            Some(ToolKey::Allow) => {
                tools_at.get_or_insert(lines.len());
                allow_lists.push(tool_items(&entry.head, &entry.continuation));
            }
            Some(ToolKey::Deny) => {
                tools_at.get_or_insert(lines.len());
                denied.extend(tool_items(&entry.head, &entry.continuation));
            }
            None if entry.key == "model" => {
                if unquoted(&entry.value).contains('/') {
                    lines.push(entry.line);
                }
            }
            None => lines.push(entry.line),
        }
    }

    let mut dropped_tools = Vec::new();
    let mut allowed: Option<Vec<&'static str>> = None;
    for list in &allow_lists {
        let mut names: Vec<&'static str> = Vec::new();
        for item in list {
            match pi_tool_name(item) {
                Some(name) if !names.contains(&name) => names.push(name),
                Some(_) => {}
                None => dropped_tools.push(item.clone()),
            }
        }
        // A file that declares two lists gets only what both grant.
        allowed = Some(match allowed {
            Some(previous) => previous.into_iter().filter(|n| names.contains(n)).collect(),
            None => names,
        });
    }

    let refusal = (allowed.is_none() && !denied.is_empty()).then(|| {
        format!(
            "it denies {} and Pi has no deny list, so these tools would be granted",
            denied.join(", ")
        )
    });
    if allowed.is_some() || refusal.is_some() {
        let denied_names: Vec<&str> = denied
            .iter()
            .filter_map(|item| pi_tool_name(item.split('(').next().unwrap_or(item)))
            .collect();
        let names: Vec<&str> = allowed
            .unwrap_or_default()
            .into_iter()
            .filter(|name| !denied_names.contains(name))
            .collect();
        let line = if names.is_empty() {
            "tools: []".to_string()
        } else {
            format!("tools: {}", names.join(", "))
        };
        lines.insert(tools_at.unwrap_or(lines.len()), line);
    }

    let mut markdown = format!("---{line_break}");
    for line in lines {
        markdown.push_str(&line);
        markdown.push_str(line_break);
    }
    markdown.push_str("---");
    markdown.push_str(&content[span.body_start..]);
    PiAgentConversion {
        markdown,
        dropped_tools,
        refusal,
    }
}

enum ToolKey {
    Allow,
    Deny,
}

/// Classify a frontmatter key, in any of the spellings agent formats use.  Pi
/// reads only `tools`, so any other spelling would be ignored there.
fn tool_key(key: &str) -> Option<ToolKey> {
    let normal: String = unquoted(key)
        .chars()
        .filter(|c| !matches!(c, '-' | '_'))
        .collect::<String>()
        .to_lowercase();
    match normal.as_str() {
        "tools" | "allowedtools" => Some(ToolKey::Allow),
        "disallowedtools" | "deniedtools" | "denytools" | "excludetools" | "excludedtools" => {
            Some(ToolKey::Deny)
        }
        _ => None,
    }
}

struct FrontmatterEntry {
    key: String,
    head: String,
    continuation: Vec<String>,
    /// The value on one line.
    value: String,
    /// The `key: value` line to write.
    line: String,
}

/// One entry per top-level key of `yaml`, with its value on a single line.
/// `line` is the source line when the key already fits on one.
fn frontmatter_entries(yaml: &str) -> Vec<FrontmatterEntry> {
    let mut groups: Vec<(String, String, Vec<String>)> = Vec::new();
    for line in yaml.lines().filter(|line| !line.starts_with('#')) {
        let top_level = !line.starts_with([' ', '\t', '-'])
            && !groups.last().is_some_and(|(_, head, rest)| in_flow_list(head, rest));
        match line.split_once(':').filter(|_| top_level) {
            Some((key, value)) => {
                groups.push((key.trim().to_string(), value.trim().to_string(), Vec::new()));
            }
            None => {
                if let Some((_, _, continuation)) = groups.last_mut() {
                    continuation.push(line.trim().to_string());
                }
            }
        }
    }

    groups
        .into_iter()
        .map(|(key, head, continuation)| {
            if continuation.iter().all(|line| line.is_empty()) && !is_block_scalar(&head) {
                let line = format!("{key}: {head}").trim_end().to_string();
                let value = head.clone();
                return FrontmatterEntry { key, head, continuation, value, line };
            }
            let value = joined_value(&head, &continuation);
            let rendered = super::render_frontmatter_value(&value).unwrap_or_else(|| {
                // A value holding both quote styles cannot be quoted safely.
                // Pi's parser reads it plain, so drop the quotes instead.
                value.replace(['\'', '"'], "")
            });
            let line = format!("{key}: {rendered}");
            FrontmatterEntry { key, head, continuation, value, line }
        })
        .collect()
}

fn in_flow_list(head: &str, continuation: &[String]) -> bool {
    head.starts_with('[')
        && !head.contains(']')
        && !continuation.iter().any(|line| line.contains(']'))
}

fn is_block_scalar(head: &str) -> bool {
    head.starts_with(['|', '>'])
}

fn joined_value(head: &str, continuation: &[String]) -> String {
    let lines = continuation.iter().filter(|line| !line.is_empty());
    let parts: Vec<&str> = if is_block_scalar(head) {
        lines.map(String::as_str).collect()
    } else if continuation.iter().any(|line| line.starts_with("- ")) {
        let items = lines.map(|line| line.strip_prefix("- ").unwrap_or(line));
        return std::iter::once(head)
            .filter(|h| !h.is_empty())
            .chain(items)
            .collect::<Vec<_>>()
            .join(", ");
    } else {
        std::iter::once(head)
            .filter(|h| !h.is_empty())
            .chain(lines.map(String::as_str))
            .collect()
    };
    parts.join(" ")
}

fn unquoted(value: &str) -> &str {
    let value = value.trim();
    let quoted = value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')));
    if quoted {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

/// The entries of a tool list in any YAML form: comma-separated, flow, block,
/// quoted, or spread over several lines.
fn tool_items(head: &str, continuation: &[String]) -> Vec<String> {
    let mut parts: Vec<&str> = Vec::new();
    if !is_block_scalar(head) {
        parts.push(without_yaml_comment(head));
    }
    for line in continuation {
        let line = without_yaml_comment(line).trim();
        parts.push(if line == "-" {
            ""
        } else {
            line.strip_prefix("- ").unwrap_or(line)
        });
    }
    let text = parts.join(",");
    let text = text.trim().trim_matches(',').trim();
    let text = text.strip_prefix('[').unwrap_or(text);
    let text = text.trim_end().trim_end_matches(',').trim_end();
    let text = text.strip_suffix(']').unwrap_or(text);
    split_outside_quotes(text)
        .iter()
        .map(|item| unquoted(item).to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

/// Split on commas that sit outside quotes and parentheses, so that
/// `Bash(git add, git commit)` stays one entry.
fn split_outside_quotes(text: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut depth = 0usize;
    for c in text.chars() {
        match (quote, c) {
            (Some(open), _) if c == open => quote = None,
            (None, '"' | '\'') => quote = Some(c),
            (None, '(') => depth += 1,
            (None, ')') => depth = depth.saturating_sub(1),
            (None, ',') if depth == 0 => {
                items.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    items.push(current);
    items
}

fn without_yaml_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut previous = ' ';
    for (index, c) in line.char_indices() {
        match (quote, c) {
            (Some(open), _) if c == open => quote = None,
            (None, '"' | '\'') => quote = Some(c),
            (None, '#') if previous.is_whitespace() => return &line[..index],
            _ => {}
        }
        previous = c;
    }
    line
}

/// Pi's name for a tool entry, or `None` when it has no equivalent.  A scoped
/// entry such as `Bash(git:*)` has none: Pi cannot scope a tool, and `bash`
/// would grant more than was declared.
fn pi_tool_name(tool: &str) -> Option<&'static str> {
    match tool {
        "Read" | "read" => Some("read"),
        "Grep" | "grep" => Some("grep"),
        "Glob" | "find" => Some("find"),
        "LS" | "ls" => Some("ls"),
        "Bash" | "bash" => Some("bash"),
        "Edit" | "edit" => Some("edit"),
        "Write" | "write" => Some("write"),
        _ => None,
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_mcp_note_names_the_headless_setting() {
        let note = Pi.mcp_note().expect("Pi should carry an MCP note");
        assert!(note.contains("settings.projectServers"));
        assert!(note.contains("~/.pi/agent/mcp-adapter.json"));
    }

    #[test]
    fn test_detect_in_matches_pi_directory() {
        let dir = tempdir().unwrap();
        assert!(!Pi.detect_in(dir.path()));

        // AGENTS.md alone must NOT trigger detection (shared with other agents).
        fs::write(dir.path().join("AGENTS.md"), "").unwrap();
        assert!(!Pi.detect_in(dir.path()));

        // `.pi/` directory is the canonical Pi project marker.
        fs::create_dir(dir.path().join(".pi")).unwrap();
        assert!(Pi.detect_in(dir.path()));
    }

    #[test]
    fn test_write_mcp_config_stdio_strips_internal_fields() {
        let dir = tempdir().unwrap();
        let mut servers = Map::new();
        servers.insert(
            "github".to_string(),
            json!({
                "type": "stdio",
                "command": "npx",
                "args": ["@modelcontextprotocol/server-github"],
                "enabled": true,
                "timeout": 30
            }),
        );

        let result = Pi.write_mcp_config(dir.path(), &servers).unwrap();
        assert!(result.contains(".pi/mcp.json") || result.contains(".pi\\mcp.json"));

        let written = fs::read_to_string(dir.path().join(".pi").join("mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&written).unwrap();
        let server = &parsed["mcpServers"]["github"];

        assert_eq!(server["command"], "npx");
        assert!(
            server.get("type").is_none(),
            "stdio entries should not include `type`"
        );
        assert!(
            server.get("enabled").is_none(),
            "internal `enabled` flag should be stripped"
        );
        assert!(
            server.get("timeout").is_none(),
            "internal `timeout` field should be stripped"
        );
    }

    #[test]
    fn test_write_mcp_config_http_preserves_url() {
        let dir = tempdir().unwrap();
        let mut servers = Map::new();
        servers.insert(
            "remote".to_string(),
            json!({
                "type": "http",
                "url": "https://example.com/mcp"
            }),
        );

        Pi.write_mcp_config(dir.path(), &servers).unwrap();
        let written = fs::read_to_string(dir.path().join(".pi").join("mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&written).unwrap();
        let server = &parsed["mcpServers"]["remote"];

        assert_eq!(server["type"], "http");
        assert_eq!(server["url"], "https://example.com/mcp");
    }

    #[test]
    fn test_discover_mcp_servers_reads_pi_mcp_json() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join(".pi")).unwrap();
        fs::write(
            dir.path().join(".pi").join("mcp.json"),
            r#"{ "mcpServers": { "github": { "command": "npx", "args": ["@modelcontextprotocol/server-github"] } } }"#,
        )
        .unwrap();

        let servers = Pi.discover_mcp_servers(dir.path());
        assert!(servers.contains_key("github"));
        assert_eq!(servers["github"]["command"], "npx");
    }

    #[test]
    fn test_owned_config_paths_includes_only_pi_mcp_json() {
        let dir = tempdir().unwrap();
        let owned = Pi.owned_config_paths(dir.path());
        assert_eq!(owned, vec![dir.path().join(".pi").join("mcp.json")]);

        // AGENTS.md must never appear here — it's shared with Codex/Gemini/Warp.
        assert!(!owned.contains(&dir.path().join("AGENTS.md")));
    }

    #[test]
    fn test_cleanup_preserves_shared_agents_md() {
        let dir = tempdir().unwrap();
        let agents_md = dir.path().join("AGENTS.md");
        fs::write(&agents_md, "# shared\n").unwrap();

        let remaining = vec!["codex".to_string()];
        let plan = super::super::plan_agent_removal(&Pi, dir.path(), &remaining, &[]).unwrap();
        super::super::apply_removal_plan(&Pi, dir.path(), &plan).unwrap();
        assert!(agents_md.exists(), "Pi must not delete AGENTS.md while Codex uses it");
    }

    #[test]
    fn test_cleanup_removes_pi_dir() {
        let dir = tempdir().unwrap();
        let pi_dir = dir.path().join(".pi");
        fs::create_dir(&pi_dir).unwrap();
        fs::write(pi_dir.join("mcp.json"), "{}").unwrap();

        let plan = super::super::plan_agent_removal(&Pi, dir.path(), &[], &[]).unwrap();
        super::super::apply_removal_plan(&Pi, dir.path(), &plan).unwrap();
        assert!(!pi_dir.exists());
    }

    #[test]
    fn test_removal_keeps_the_users_pi_files() {
        let dir = tempdir().unwrap();
        let pi_dir = dir.path().join(".pi");
        fs::create_dir_all(pi_dir.join("extensions")).unwrap();
        fs::write(pi_dir.join("mcp.json"), "{}").unwrap();
        fs::write(pi_dir.join("settings.json"), "{}").unwrap();
        fs::write(pi_dir.join("extensions").join("mine.ts"), "").unwrap();

        let plan = super::super::plan_agent_removal(&Pi, dir.path(), &[], &[]).unwrap();
        super::super::apply_removal_plan(&Pi, dir.path(), &plan).unwrap();

        assert!(!pi_dir.join("mcp.json").exists());
        assert!(pi_dir.join("settings.json").exists());
        assert!(pi_dir.join("extensions").join("mine.ts").exists());
    }

    #[test]
    fn test_capabilities_enables_mcp_and_subagents() {
        let caps = Pi.capabilities();
        assert!(caps.mcp_servers, "Pi has built-in MCP");
        assert!(caps.agents, "Pi supports sub-agents via pi-subagents");
        assert!(caps.skills);
        assert!(caps.instructions);
        assert!(caps.commands, "Pi prompt templates sync to .pi/prompts");
        assert!(caps.global_mcp_servers, "Pi reads a user-level mcp.json");
        assert!(!caps.hooks, "Pi has no hook concept");
    }

    #[test]
    fn test_agents_dir_returns_pi_agents() {
        let dir = tempdir().unwrap();
        assert_eq!(
            Pi.agents_dir(dir.path()),
            Some(dir.path().join(".pi").join("agents"))
        );
    }

    #[test]
    fn test_agents_file_ext_is_md() {
        assert_eq!(Pi.agents_file_ext(), "md");
    }

    #[test]
    fn test_skill_sync_writes_to_pi_skills() {
        let dir = tempdir().unwrap();
        let skills = vec![("my-skill".to_string(), "# My Skill\n".to_string())];
        let selected = vec!["my-skill".to_string()];

        let written = Pi.sync_skills(dir.path(), &skills, &selected, &[]).unwrap();
        assert_eq!(written.len(), 1);

        let content = fs::read_to_string(dir.path().join(".pi/skills/my-skill/SKILL.md")).unwrap();
        assert_eq!(content, "# My Skill\n");
    }

    #[test]
    fn test_write_mcp_config_leaves_out_sse_and_keeps_the_rest() {
        let dir = tempdir().unwrap();
        let mut servers = Map::new();
        servers.insert(
            "legacy".to_string(),
            json!({ "type": "sse", "url": "https://example.com/sse" }),
        );
        servers.insert(
            "remote".to_string(),
            json!({
                "type": "http",
                "url": "https://example.com/mcp",
                "enabled": true,
                "timeout": 30
            }),
        );

        Pi.write_mcp_config(dir.path(), &servers).unwrap();
        let written = fs::read_to_string(dir.path().join(".pi").join("mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&written).unwrap();

        assert!(parsed["mcpServers"].get("legacy").is_none());
        let remote = &parsed["mcpServers"]["remote"];
        assert_eq!(remote["type"], "http");
        assert!(remote.get("enabled").is_none());
        assert!(remote.get("timeout").is_none());
    }

    #[test]
    fn test_only_sse_is_unsupported() {
        let sse = json!({ "type": "sse", "url": "https://example.com/sse" });
        let http = json!({ "type": "http", "url": "https://example.com/mcp" });
        assert!(Pi.mcp_unsupported_reason(&sse).is_some());
        assert!(Pi.mcp_unsupported_reason(&http).is_none());
    }

    #[test]
    fn test_agent_dir_defaults_under_home() {
        let home = PathBuf::from("/home/me");
        assert_eq!(
            resolve_pi_agent_dir(None, Some(home.clone())),
            Some(home.join(".pi").join("agent"))
        );
        assert_eq!(
            resolve_pi_agent_dir(Some(OsString::new()), Some(home.clone())),
            Some(home.join(".pi").join("agent"))
        );
    }

    #[test]
    fn test_agent_dir_honours_the_environment() {
        let home = PathBuf::from("/home/me");
        assert_eq!(
            resolve_pi_agent_dir(Some("/work/pi".into()), Some(home.clone())),
            Some(PathBuf::from("/work/pi"))
        );
        assert_eq!(
            resolve_pi_agent_dir(Some("~/work/pi".into()), Some(home.clone())),
            Some(home.join("work").join("pi"))
        );
        assert_eq!(resolve_pi_agent_dir(Some("~/x".into()), None), None);
    }

    #[test]
    fn test_commands_sync_to_pi_prompts() {
        let dir = tempdir().unwrap();
        assert_eq!(
            Pi.commands_dir(dir.path()),
            Some(dir.path().join(".pi").join("prompts"))
        );
    }

    #[test]
    fn test_convert_agent_flattens_multi_line_values() {
        let content = "---\nname: planner\ndescription: >\n  Plans the work\n  in stages\ntools:\n  - Read\n  - Grep\n---\nPlan.\n";
        let rendered = Pi.convert_agent_content(content, "planner");
        assert!(rendered.contains("\ndescription: Plans the work in stages\n"), "{rendered}");
        assert!(rendered.contains("\ntools: read, grep\n"), "{rendered}");
        assert!(rendered.contains("\nautomatic-managed: true\n"));
        assert!(rendered.ends_with("---\nPlan.\n"), "{rendered}");
    }

    #[test]
    fn test_convert_agent_keeps_a_colon_in_a_flattened_value_readable() {
        let content = "---\nname: planner\ndescription: |\n  Note: plans\n---\nPlan.\n";
        let rendered = Pi.convert_agent_content(content, "planner");
        assert!(rendered.contains("\ndescription: 'Note: plans'\n"), "{rendered}");
    }

    #[test]
    fn test_convert_agent_drops_model_aliases_and_keeps_pins() {
        for alias in ["sonnet", "opus", "haiku", "inherit"] {
            let content = format!("---\nname: a\ndescription: b\nmodel: {alias}\n---\nBody\n");
            let rendered = Pi.convert_agent_content(&content, "a");
            assert!(!rendered.contains("model:"), "{rendered}");
        }
        let content = "---\nname: a\ndescription: b\nmodel: anthropic/claude-x\n---\nBody\n";
        let rendered = Pi.convert_agent_content(content, "a");
        assert!(rendered.contains("\nmodel: anthropic/claude-x\n"), "{rendered}");
    }

    fn converted(frontmatter: &str) -> String {
        Pi.convert_agent_content(&format!("---\nname: a\ndescription: b\n{frontmatter}---\nBody\n"), "a")
    }

    fn tools_line(frontmatter: &str) -> Option<String> {
        converted(frontmatter)
            .lines()
            .find(|line| line.starts_with("tools:"))
            .map(str::to_string)
    }

    #[test]
    fn test_convert_agent_maps_every_known_tool() {
        assert_eq!(
            tools_line("tools: Read, Glob, LS, Bash, Edit, Write\n").as_deref(),
            Some("tools: read, find, ls, bash, edit, write")
        );
    }

    #[test]
    fn test_tools_keep_the_mappable_subset() {
        let frontmatter = "tools: Read, WebFetch, mcp__x__y\n";
        assert_eq!(tools_line(frontmatter).as_deref(), Some("tools: read"));
        let content = format!("---\nname: a\ndescription: b\n{frontmatter}---\nBody\n");
        assert_eq!(Pi.agent_content_warnings(&content).len(), 2);
        assert!(Pi.agent_content_refusal(&content).is_none());
    }

    #[test]
    fn test_scoped_bash_never_becomes_bash() {
        assert_eq!(
            tools_line("tools: Read, Bash(git add, git commit), Bash(ls:*)\n").as_deref(),
            Some("tools: read")
        );
        assert_eq!(tools_line("tools: Bash(git:*)\n").as_deref(), Some("tools: []"));
        assert_eq!(tools_line("tools: Read(src/**)\n").as_deref(), Some("tools: []"));
    }

    #[test]
    fn test_empty_list_stays_an_explicit_empty_list() {
        assert_eq!(tools_line("tools: []\n").as_deref(), Some("tools: []"));
        assert_eq!(tools_line("tools:\n").as_deref(), Some("tools: []"));
    }

    #[test]
    fn test_nothing_mapping_grants_nothing() {
        assert_eq!(tools_line("tools: WebFetch, Task\n").as_deref(), Some("tools: []"));
        assert_eq!(tools_line("tools: '*'\n").as_deref(), Some("tools: []"));
    }

    #[test]
    fn test_no_tools_key_stays_without_one() {
        assert_eq!(tools_line(""), None);
    }

    #[test]
    fn test_deny_list_removes_names_from_the_allow_list() {
        let rendered = converted("tools: Read, Bash, Write\ndisallowedTools: Bash, Write\n");
        assert!(rendered.contains("\ntools: read\n"), "{rendered}");
        assert!(!rendered.contains("disallowedTools"), "{rendered}");
        // A scoped denial still denies part of the tool, so all of it goes.
        assert_eq!(
            tools_line("tools: Read, Bash\ndisallowed-tools: Bash(rm:*)\n").as_deref(),
            Some("tools: read")
        );
    }

    #[test]
    fn test_deny_list_without_an_allow_list_is_refused() {
        let content = "---\nname: a\ndescription: b\ndisallowedTools: Bash\n---\nBody\n";
        assert!(Pi.agent_content_refusal(content).is_some_and(|r| r.contains("Bash")));
        // No write path can widen it, even one that ignores the refusal.
        assert!(Pi.convert_agent_content(content, "a").contains("\ntools: []\n"));
    }

    #[test]
    fn test_empty_deny_list_restricts_nothing() {
        let content = "---\nname: a\ndescription: b\ndisallowedTools: []\n---\nBody\n";
        assert!(Pi.agent_content_refusal(content).is_none());
        assert_eq!(tools_line("disallowedTools: []\n"), None);
    }

    #[test]
    fn test_tool_lists_parse_in_every_yaml_form() {
        for frontmatter in [
            "tools: \"Read\", 'Grep'\n",
            "tools: [\"Read\", 'Grep']\n",
            "tools: [Read, Grep] # note\n",
            "tools:\n  - Read\n  - \"Grep\"\n",
            "tools:\n- Read # first\n- Grep\n",
            "tools: [\n  Read,\n  Grep\n]\n",
            "tools: [\nRead,\nGrep,\n]\n",
            "tools: |\n  Read\n  Grep\n",
            "\"tools\": Read, Grep\n",
            "Tools: Read, Grep\n",
            "allowed-tools: Read, Grep\n",
        ] {
            assert_eq!(
                tools_line(frontmatter).as_deref(),
                Some("tools: read, grep"),
                "{frontmatter}"
            );
        }
    }

    #[test]
    fn test_quoted_scoped_entries_stay_whole() {
        assert_eq!(
            tools_line("tools: [\"Bash(a, b)\", Read]\n").as_deref(),
            Some("tools: read")
        );
        assert_eq!(
            tools_line("tools:\n  - 'Bash(git:*)'\n  - Read\n").as_deref(),
            Some("tools: read")
        );
    }

    #[test]
    fn test_two_allow_lists_grant_only_what_both_grant() {
        assert_eq!(
            tools_line("tools: Read, Bash\nallowedTools: Read, Write\n").as_deref(),
            Some("tools: read")
        );
    }

    #[test]
    fn test_converted_tools_compare_clean_and_stay_stable() {
        for frontmatter in [
            "tools: Read, Bash(git:*), WebFetch\n",
            "tools: WebFetch\n",
            "tools: []\n",
            "tools: Read, Bash\ndisallowedTools: Bash\n",
            "tools:\n  - Read\n",
        ] {
            let expected = converted(frontmatter);
            assert!(
                Pi.agent_file_differences(&expected, &expected).is_empty(),
                "{frontmatter}"
            );
            assert_eq!(Pi.convert_agent_content(&expected, "a"), expected, "{frontmatter}");
        }
    }

    #[test]
    fn test_convert_agent_ignores_comment_lines() {
        let content = "---\nname: a\n# a note\ndescription: b\n---\nBody\n";
        let rendered = Pi.convert_agent_content(content, "a");
        assert!(rendered.starts_with("---\nname: a\ndescription: b\n"), "{rendered}");
    }

    #[test]
    fn test_convert_agent_keeps_crlf_and_is_stable() {
        let content = "---\r\nname: a\r\ndescription: b\r\n---\r\nBody\r\n";
        let once = Pi.convert_agent_content(content, "a");
        assert!(once.starts_with("---\r\nname: a\r\n"), "{once:?}");
        assert_eq!(Pi.convert_agent_content(&once, "a"), once);
    }

    #[test]
    fn test_converted_agent_has_no_differences_from_itself() {
        let content = "---\nname: a\ndescription: >\n  long\nmodel: sonnet\ntools: Read\n---\nBody\n";
        let expected = Pi.convert_agent_content(content, "a");
        assert!(Pi.agent_file_differences(&expected, &expected).is_empty());
    }
}
