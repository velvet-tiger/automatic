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
             settings.projectServers is \"allow\" in ~/.pi/agent/mcp-adapter.json. Automatic \
             sees PI_CODING_AGENT_DIR only if it was started with that variable set, not from \
             your shell profile, so check the target path shown here.",
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
            "This agent rejects the legacy SSE transport. Use the server's streamable HTTP URL."
                .to_string()
        })
    }

    fn global_mcp_target(&self) -> Option<super::GlobalMcpTarget> {
        Some(super::GlobalMcpTarget {
            path: pi_agent_dir()?.join("mcp.json"),
            reload_note: Some("The change applies to new sessions."),
        })
    }

    fn legacy_global_mcp_path(&self) -> Option<PathBuf> {
        super::home_dir().map(|home| home.join(".pi").join("agent").join("mcp.json"))
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

    fn convert_agent_content(&self, content: &str, name: &str) -> String {
        super::render_markdown_command(&convert_pi_agent(content, name).markdown)
    }

    fn agent_content_refusal(&self, content: &str) -> Option<String> {
        convert_pi_agent(content, "").refusal
    }

    fn agent_content_warnings(&self, content: &str) -> Vec<String> {
        convert_pi_agent(content, "").warnings
    }

    fn agent_markdown_is_lossy(&self) -> bool {
        true
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
    let path = match path.strip_prefix("~") {
        Ok(rest) => home?.join(rest),
        Err(_) => path,
    };
    // A relative value means whatever Pi's working directory is, and Automatic
    // must not guess that for a file it writes.
    path.is_absolute().then_some(path)
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
    /// What the user should hear about: dropped tools, and fields Pi needs.
    warnings: Vec<String>,
    /// Why this definition must not be written at all.
    refusal: Option<String>,
}

/// Claude's read-only tools, which is what `permissionMode: plan` allows.
const PI_READ_ONLY_TOOLS: [&str; 4] = ["read", "grep", "find", "ls"];

/// Rewrite a Claude-style sub-agent for `pi-subagents`.
///
/// Its parser reads one `key: value` per line, last key winning, and trims
/// keys as JavaScript does.  So a multi-line value would be misread, and a key
/// Rust reads differently could set `tools` behind this code's back: any key
/// this code cannot classify refuses the definition.  A `model` it cannot
/// resolve fails the spawn with no fallback, so only a `provider/id` pin is
/// kept.
///
/// `tools` must fail closed.  With no `tools` key the sub-agent gets Pi's
/// defaults (`read`, `bash`, `edit`, `write`), so dropping the key from a
/// restricted definition would widen it.  Extension tools arrive only with
/// `extensions: true`.  An empty list grants nothing, and `parseNameList`
/// accepts `tools: []`.  Pi has no deny list or permission modes, so those
/// narrow the allow list.  What cannot be narrowed is refused, and the output
/// still carries `tools: []` so that no write path can widen it.
fn convert_pi_agent(content: &str, name: &str) -> PiAgentConversion {
    let (Some(yaml), Some(span)) = (super::frontmatter_block(content), super::frontmatter_span(content))
    else {
        return convert_without_frontmatter(content, name);
    };
    let line_break = &content[3..span.start];
    let body = &content[span.body_start..];

    let (entries, unclosed_flow) = frontmatter_entries(yaml);
    let mut lines: Vec<String> = Vec::new();
    let mut problems: Vec<String> = yaml.lines().filter_map(unreadable_line).collect();
    if unclosed_flow {
        problems.push("a list or map is never closed".to_string());
    }
    let mut tools_at: Option<usize> = None;
    let mut allow_lists: Vec<Vec<String>> = Vec::new();
    let mut denied: Vec<String> = Vec::new();
    let mut plan_mode = false;
    let mut extensions = false;
    let mut max_turns: Option<u32> = None;
    let mut has_name = false;
    let mut has_description = false;
    for entry in entries {
        if let Some(problem) = unreadable_key(&entry.key) {
            problems.push(problem);
            continue;
        }
        let normal = normalised_key(&entry.key);
        let value = clean_scalar(&entry.value);
        match (tool_key(&normal), normal.as_str()) {
            (Some(ToolKey::Allow), _) => {
                tools_at.get_or_insert(lines.len());
                allow_lists.push(tool_items(&entry.head, &entry.continuation));
            }
            (Some(ToolKey::Deny), _) => {
                tools_at.get_or_insert(lines.len());
                denied.extend(tool_items(&entry.head, &entry.continuation));
            }
            (None, "model") => {
                if value.contains('/') {
                    lines.push(format!("model: {value}"));
                }
            }
            // Several spellings may each set a limit, and Pi reads only the
            // last `max_turns`, so the tightest one is written once.
            (None, "maxturns") => match value.parse::<u32>() {
                Ok(turns) if turns > 0 => {
                    tools_at.get_or_insert(lines.len());
                    max_turns = Some(max_turns.map_or(turns, |limit| limit.min(turns)));
                }
                _ => problems.push(format!("its turn limit '{value}' is not a positive number")),
            },
            (None, "permissionmode") => match value {
                "plan" => {
                    tools_at.get_or_insert(lines.len());
                    plan_mode = true;
                }
                // These loosen prompting.  Pi's sub-agents never prompt.
                "default" | "acceptEdits" | "bypassPermissions" => {}
                other => problems.push(format!("Pi cannot express permission mode '{other}'")),
            },
            (None, "extensions") => {
                // Pi reads only the exact key, last one winning, so a variant
                // could turn extensions on while this code thinks they are off.
                if entry.key.trim() != "extensions" {
                    problems.push(format!("cannot read the key '{}'", entry.key.escape_default()));
                    continue;
                }
                // Pi enables them only for `true`, but any other spelling may
                // be meant the same way.  Any occurrence that is on counts.
                extensions |= !matches!(value, "" | "false");
                lines.push(entry.line);
            }
            (None, _) if normal.contains("tool") => {
                problems.push(format!("Pi would ignore the tool setting '{}'", entry.key));
            }
            (None, "name") if value.is_empty() => {}
            (None, key) => {
                has_name |= key == "name";
                has_description |= key == "description" && !value.is_empty();
                lines.push(entry.line);
            }
        }
    }

    if let Some(turns) = max_turns {
        let at = tools_at.unwrap_or(lines.len()).min(lines.len());
        lines.insert(at, format!("max_turns: {turns}"));
        tools_at = Some(at + 1);
    }

    let mut warnings = Vec::new();
    let mut allowed = narrowed_allow_list(&allow_lists, &mut warnings);
    if plan_mode {
        allowed = Some(match allowed {
            Some(names) => names.into_iter().filter(|n| PI_READ_ONLY_TOOLS.contains(n)).collect(),
            None => PI_READ_ONLY_TOOLS.to_vec(),
        });
    }

    problems.extend(denied.iter().filter_map(|item| deny_entry_problem(item)));
    if allowed.is_none() && !denied.is_empty() {
        problems.push(format!(
            "it denies {} and Pi has no deny list, so these tools would be granted",
            denied.join(", ")
        ));
    }
    if extensions && !denied.is_empty() {
        // An extension can register a tool of any name, Task and mcp__*
        // included, and Pi cannot deny it.
        problems.push(format!(
            "it enables extensions and denies {}, which an extension could provide",
            denied.join(", ")
        ));
    }
    let denied_names = denied_tool_names(&denied);

    // The last line of defence: nothing but the line below may set `tools`.
    let slipped = lines.len();
    lines.retain(|line| !sets_pi_tools(line));
    if lines.len() != slipped {
        problems.push("a key reads as tools to Pi".to_string());
    }

    let refusal = (!problems.is_empty()).then(|| problems.join("; "));
    if allowed.is_some() || refusal.is_some() {
        let names: Vec<&str> = allowed
            .unwrap_or_default()
            .into_iter()
            .filter(|name| refusal.is_none() && !denied_names.contains(name))
            .collect();
        let line = if names.is_empty() {
            "tools: []".to_string()
        } else {
            format!("tools: {}", names.join(", "))
        };
        lines.insert(tools_at.unwrap_or(lines.len()).min(lines.len()), line);
    }

    if !has_name && !name.is_empty() {
        lines.insert(0, format!("name: {}", fallback_name(name)));
    }
    if !has_description {
        warnings.push("it has no description, so Pi will skip it".to_string());
    }
    if body.trim().is_empty() {
        warnings.push("it has no instructions, so Pi will skip it".to_string());
    }

    let mut markdown = format!("---{line_break}");
    for line in lines {
        markdown.push_str(&line);
        markdown.push_str(line_break);
    }
    markdown.push_str("---");
    markdown.push_str(body);
    PiAgentConversion {
        markdown,
        warnings,
        refusal,
    }
}

/// A file with no frontmatter block.  Pi skips it, so it gets a name.  An
/// opening line with no closing line may have meant to restrict something, so
/// that is refused rather than turned into an unrestricted sub-agent.
fn convert_without_frontmatter(content: &str, name: &str) -> PiAgentConversion {
    let line_break = if content.contains("\r\n") { "\r\n" } else { "\n" };
    let refusal = content
        .starts_with("---")
        .then(|| "its frontmatter is never closed".to_string());
    let name_line = (!name.is_empty()).then(|| format!("name: {}{line_break}", fallback_name(name)));
    let tools_line = refusal.as_ref().map(|_| format!("tools: []{line_break}"));
    let markdown = format!(
        "---{line_break}{}{}---{line_break}{content}",
        name_line.unwrap_or_default(),
        tools_line.unwrap_or_default()
    );
    PiAgentConversion {
        markdown,
        warnings: vec!["it has no description, so Pi will skip it".to_string()],
        refusal,
    }
}

fn fallback_name(name: &str) -> String {
    super::render_frontmatter_value(name).unwrap_or_else(|| sanitised_value(name))
}

/// The allow lists a file declares, reduced to what they all grant in Pi's
/// names.  `tools: inherit` restricts nothing, so it counts as no list.
fn narrowed_allow_list(
    allow_lists: &[Vec<String>],
    warnings: &mut Vec<String>,
) -> Option<Vec<&'static str>> {
    let mut allowed: Option<Vec<&'static str>> = None;
    for list in allow_lists {
        if matches!(list.as_slice(), [only] if only.eq_ignore_ascii_case("inherit")) {
            continue;
        }
        let mut names: Vec<&'static str> = Vec::new();
        for item in list {
            match pi_tool_name(item) {
                Some(name) if !names.contains(&name) => names.push(name),
                Some(_) => {}
                None => warnings.push(format!("dropped {item}, which has no Pi equivalent")),
            }
        }
        // A file that declares two lists gets only what both grant.
        allowed = Some(match allowed {
            Some(previous) => previous.into_iter().filter(|n| names.contains(n)).collect(),
            None => names,
        });
    }
    allowed
}

/// JavaScript's `trim`, which also strips U+FEFF.  Rust's keeps it.
fn js_trim(text: &str) -> &str {
    text.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

/// Whether Pi's parser would read `line` as setting `tools`.
fn sets_pi_tools(line: &str) -> bool {
    line.split_once(':').is_some_and(|(key, _)| js_trim(key) == "tools")
}

/// A top-level line that YAML reads as something this code does not model.
fn unreadable_line(line: &str) -> Option<String> {
    line.starts_with(['?', '{'])
        .then(|| format!("cannot read the line '{line}'"))
}

/// Why `key` cannot be classified, if it cannot.
fn unreadable_key(key: &str) -> Option<String> {
    let plain = unquoted(key);
    let safe = !plain.is_empty()
        && plain.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'));
    (!safe).then(|| format!("cannot read the key '{}'", key.escape_default()))
}

fn normalised_key(key: &str) -> String {
    unquoted(key)
        .chars()
        .filter(|c| !matches!(c, '-' | '_'))
        .collect::<String>()
        .to_lowercase()
}

/// A scalar without its comment and quotes.
fn clean_scalar(value: &str) -> &str {
    unquoted(without_yaml_comment(value))
}

/// Why a deny entry cannot be honoured.  An entry that is not exactly a tool,
/// a scoped tool, or a tool Pi does not have might deny something this code
/// failed to read.
fn deny_entry_problem(item: &str) -> Option<String> {
    let (base, scoped) = match item.split_once('(') {
        Some((base, rest)) => (base, rest.ends_with(')')),
        None => (item, true),
    };
    let known = !base.contains(char::is_whitespace)
        && (pi_tool_name_any_case(base).is_some() || is_tool_pi_lacks(base));
    (!known || !scoped).then(|| format!("cannot read the denied tool '{item}'"))
}

fn is_tool_pi_lacks(name: &str) -> bool {
    const TOOLS: [&str; 14] = [
        "agent",
        "askuserquestion",
        "bashoutput",
        "exitplanmode",
        "killshell",
        "multiedit",
        "notebookedit",
        "notebookread",
        "skill",
        "slashcommand",
        "task",
        "todowrite",
        "webfetch",
        "websearch",
    ];
    let lower = name.to_lowercase();
    TOOLS.contains(&lower.as_str()) || lower.starts_with("mcp__")
}

/// Every Pi tool named anywhere in a deny list, matched on word tokens in any
/// case, so an entry this code could not parse still removes what it names.  A
/// scoped denial denies part of the tool, so all of it goes.
fn denied_tool_names(denied: &[String]) -> Vec<&'static str> {
    denied
        .iter()
        .flat_map(|item| item.split(|c: char| !c.is_alphanumeric() && c != '_'))
        .filter_map(|token| {
            if token.eq_ignore_ascii_case("MultiEdit") {
                Some("edit")
            } else {
                pi_tool_name_any_case(token)
            }
        })
        .collect()
}

enum ToolKey {
    Allow,
    Deny,
}

/// Classify a normalised key, in any of the spellings agent formats use.  Pi
/// reads only `tools`, so any other spelling would be ignored there.
fn tool_key(normal: &str) -> Option<ToolKey> {
    match normal {
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

/// One entry per top-level key of `yaml`, with its value on a single line, and
/// whether a flow value was still open at the end.  `line` is the source line
/// when the key already fits on one.
fn frontmatter_entries(yaml: &str) -> (Vec<FrontmatterEntry>, bool) {
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

    let unclosed = groups.last().is_some_and(|(_, head, rest)| in_flow_list(head, rest));
    let entries = groups
        .into_iter()
        .map(|(key, head, continuation)| {
            if continuation.iter().all(|line| line.is_empty()) && !is_block_scalar(&head) {
                let line = format!("{key}: {head}").trim_end().to_string();
                let value = head.clone();
                return FrontmatterEntry { key, head, continuation, value, line };
            }
            let value = joined_value(&head, &continuation);
            let rendered = super::render_frontmatter_value(&value)
                .unwrap_or_else(|| sanitised_value(&value));
            let line = format!("{key}: {rendered}");
            FrontmatterEntry { key, head, continuation, value, line }
        })
        .collect();
    (entries, unclosed)
}

/// A value that cannot be quoted safely, made safe to write plain.  Pi's
/// parser reads it plain, so quotes and anything that could end the line go.
fn sanitised_value(value: &str) -> String {
    value
        .chars()
        .filter(|c| {
            !c.is_control() && !matches!(c, '\'' | '"' | '\u{2028}' | '\u{2029}')
        })
        .collect()
}

fn in_flow_list(head: &str, continuation: &[String]) -> bool {
    let (open, close) = match head.chars().next() {
        Some('[') => ('[', ']'),
        Some('{') => ('{', '}'),
        _ => return false,
    };
    let lines = std::iter::once(head).chain(continuation.iter().map(String::as_str));
    let depth = |c: char| lines.clone().flat_map(str::chars).filter(|x| *x == c).count();
    depth(open) > depth(close)
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

fn pi_tool_name_any_case(tool: &str) -> Option<&'static str> {
    match tool.to_ascii_lowercase().as_str() {
        "read" => Some("read"),
        "grep" => Some("grep"),
        "glob" | "find" => Some("find"),
        "ls" => Some("ls"),
        "bash" => Some("bash"),
        "edit" => Some("edit"),
        "write" => Some("write"),
        _ => None,
    }
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
    fn test_conversion_is_stable_and_an_edit_shows_as_a_difference() {
        for frontmatter in [
            "tools: Read, Bash(git:*), WebFetch\n",
            "tools: WebFetch\n",
            "tools: []\n",
            "tools: Read, Bash\ndisallowedTools: Bash\n",
            "tools:\n  - Read\n",
        ] {
            let expected = converted(frontmatter);
            assert_eq!(Pi.convert_agent_content(&expected, "a"), expected, "{frontmatter}");
            let edited = expected.replace("tools:", "tools: bash,");
            assert_eq!(
                Pi.agent_file_differences(&edited, &expected),
                vec!["content"],
                "{frontmatter}"
            );
        }
    }

    /// Exactly what `pi-subagents` does: split on the first colon, JS-trim the
    /// key, last key wins.
    fn tools_pi_reads(rendered: &str) -> Option<String> {
        let block = rendered.split("\n---").next().unwrap_or(rendered);
        block
            .lines()
            .filter_map(|line| line.split_once(':'))
            .filter(|(key, _)| js_trim(key) == "tools")
            .map(|(_, value)| value.trim().to_string())
            .next_back()
    }

    fn refused(frontmatter: &str) -> String {
        let content = format!("---\nname: a\ndescription: b\n{frontmatter}---\nBody\n");
        assert!(
            Pi.agent_content_refusal(&content).is_some(),
            "not refused: {frontmatter:?}"
        );
        let rendered = Pi.convert_agent_content(&content, "a");
        assert_eq!(tools_pi_reads(&rendered).as_deref(), Some("[]"), "{rendered}");
        let keys = rendered.lines().filter(|line| sets_pi_tools(line)).count();
        assert_eq!(keys, 1, "{rendered}");
        rendered
    }

    #[test]
    fn test_keys_that_cannot_be_classified_refuse() {
        for frontmatter in [
            "tools: Read\n\u{feff}tools: bash, write\n",
            "\"to\\x6fls\": bash\n",
            "\"disallowed\\x54ools\": Bash\n",
            "? tools\n: bash\n",
            "<<: {tools: Read}\n",
            "{tools: bash}\n",
            "description: [draft\ntools: Read\n",
            ": bash\n",
            "disallowTools: Bash\n",
            "toolset: bash\n",
        ] {
            refused(frontmatter);
        }
    }

    #[test]
    fn test_an_unclosed_block_is_refused_not_opened_up() {
        let content = "---\nname: a\ntools: Read\nBody\n";
        assert!(Pi.agent_content_refusal(content).is_some());
        assert!(Pi.convert_agent_content(content, "a").contains("\ntools: []\n"));
    }

    #[test]
    fn test_max_turns_maps_and_a_bad_limit_refuses() {
        assert!(converted("maxTurns: 5\n").contains("\nmax_turns: 5\n"));
        assert!(!converted("maxTurns: 5\n").contains("maxTurns"));
        refused("maxTurns: lots\n");
    }

    #[test]
    fn test_plan_mode_narrows_to_read_only_tools() {
        assert_eq!(
            tools_line("permissionMode: plan\n").as_deref(),
            Some("tools: read, grep, find, ls")
        );
        assert_eq!(
            tools_line("tools: Read, Bash, Grep\npermissionMode: plan\n").as_deref(),
            Some("tools: read, grep")
        );
        assert!(!converted("permissionMode: plan\n").contains("permissionMode"));
    }

    #[test]
    fn test_permission_modes_pi_cannot_express_refuse() {
        refused("permissionMode: dontAsk\n");
        refused("permissionMode: somethingNew\n");
        assert_eq!(tools_line("permissionMode: default\n"), None);
    }

    #[test]
    fn test_unparsable_deny_entries_refuse_and_still_remove_tools() {
        for denied in ["Foo(, Bash", "Bash(don't), Write", "Bash (rm:*)", "Bash Write"] {
            refused(&format!("tools: Read, Bash, Write\ndisallowedTools: {denied}\n"));
        }
        let names = denied_tool_names(&["Foo(, Bash".to_string(), "Bash Write".to_string()]);
        assert!(names.contains(&"bash") && names.contains(&"write"), "{names:?}");
    }

    #[test]
    fn test_denied_names_match_in_any_case() {
        assert_eq!(
            tools_line("tools: Read, Bash\ndisallowedTools: BASH\n").as_deref(),
            Some("tools: read")
        );
    }

    #[test]
    fn test_extensions_with_a_deny_list_refuse() {
        refused(
            "tools: Read\nextensions: true\ndisallowedTools: mcp__github__create_issue, WebFetch, Task\n",
        );
        refused("tools: Read, Bash\nextensions: yes\ndisallowedTools: Bash\n");
        assert_eq!(
            tools_line("tools: Read\nextensions: false\ndisallowedTools: WebFetch\n").as_deref(),
            Some("tools: read")
        );
    }

    #[test]
    fn test_a_later_extensions_variant_cannot_hide_extensions() {
        for variant in ["Extensions: false", "extensions_: false", "\"extensions\": false"] {
            let rendered = refused(&format!(
                "tools: Read\nextensions: true\n{variant}\ndisallowedTools: WebFetch\n"
            ));
            assert!(!rendered.contains(variant), "{rendered}");
        }
    }

    #[test]
    fn test_extensions_stay_on_once_any_occurrence_is_on() {
        refused("tools: Read\nextensions: true\nextensions: false\ndisallowedTools: WebFetch\n");
    }

    #[test]
    fn test_several_turn_limits_write_the_tightest_once() {
        let rendered = converted("max_turns: 5\nmaxTurns: 50\n");
        assert_eq!(rendered.matches("max_turns:").count(), 1, "{rendered}");
        assert!(rendered.contains("\nmax_turns: 5\n"), "{rendered}");
    }

    #[test]
    fn test_denying_a_tool_pi_lacks_is_harmless() {
        assert_eq!(
            tools_line("tools: Read\ndisallowedTools: WebFetch, mcp__x__y\n").as_deref(),
            Some("tools: read")
        );
    }

    #[test]
    fn test_model_comment_and_quotes_are_cleaned() {
        assert!(converted("model: 'anthropic/x' # pinned\n").contains("\nmodel: anthropic/x\n"));
        assert!(!converted("model: sonnet # anthropic/x\n").contains("model:"));
    }

    #[test]
    fn test_missing_name_is_injected_and_missing_description_warns() {
        let content = "---\ndescription: b\n---\nBody\n";
        assert!(Pi.convert_agent_content(content, "fallback").contains("name: fallback\n"));
        let content = "---\nname: a\n---\nBody\n";
        assert!(Pi.agent_content_warnings(content).iter().any(|w| w.contains("description")));
        let plain = Pi.convert_agent_content("Just a body.\n", "fallback");
        assert!(plain.contains("name: fallback\n"), "{plain}");
    }

    #[test]
    fn test_tools_inherit_restricts_nothing_unless_something_else_does() {
        assert_eq!(tools_line("tools: inherit\n"), None);
        assert_eq!(
            tools_line("tools: inherit\npermissionMode: plan\n").as_deref(),
            Some("tools: read, grep, find, ls")
        );
        refused("tools: inherit\ndisallowedTools: Bash\n");
    }

    #[test]
    fn test_unquotable_values_lose_control_characters() {
        assert_eq!(sanitised_value("a'b\"c\u{2028}d\u{7}e"), "abcde");
    }

    #[test]
    fn test_relative_agent_dir_is_refused() {
        assert_eq!(
            resolve_pi_agent_dir(Some("work/pi".into()), Some(PathBuf::from("/home/me"))),
            None
        );
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
}
