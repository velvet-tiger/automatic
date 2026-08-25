use serde_json::{Map, Value};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::{Agent, GlobalMcpTarget, GlobalMcpWriteReport};

/// Codex CLI agent — writes `.codex/config.toml` and stores skills under
/// `<project>/.agents/skills/<name>/SKILL.md`.
pub struct CodexCli;

impl Agent for CodexCli {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "codex"
    }

    fn label(&self) -> &'static str {
        "Codex CLI"
    }

    fn config_description(&self) -> &'static str {
        ".codex/config.toml"
    }

    fn project_file_name(&self) -> &'static str {
        "AGENTS.md"
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_in(&self, dir: &Path) -> bool {
        dir.join(".codex").join("config.toml").exists()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".agents").join("skills")]
    }

    // ── Config writing ──────────────────────────────────────────────────

    /// TOML has no variable interpolation and Codex performs none, so a
    /// `${KEY}` placeholder would be handed to the server verbatim.  Codex's
    /// own mechanism is `env_vars`: a list of names to forward from the host
    /// environment, kept out of the `env` table so no value is written at all.
    fn rewrite_inherited_env(&self, server: &mut Map<String, Value>, keys: &[String]) {
        if let Some(Value::Object(env)) = server.get_mut("env") {
            for key in keys {
                env.remove(key);
            }
            if env.is_empty() {
                server.remove("env");
            }
        }

        let mut forwarded: Vec<Value> = server
            .get("env_vars")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for key in keys {
            let entry = Value::String(key.clone());
            if !forwarded.contains(&entry) {
                forwarded.push(entry);
            }
        }
        server.insert("env_vars".to_string(), Value::Array(forwarded));
    }

    fn mcp_merge_inputs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".codex").join("config.toml")]
    }

    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String> {
        let codex_dir = dir.join(".codex");
        if !codex_dir.exists() {
            fs::create_dir_all(&codex_dir)
                .map_err(|e| format!("Failed to create .codex/: {}", e))?;
        }

        let mut toml_content = String::new();

        for (name, config) in servers {
            let config = config.clone();
            let transport = config
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("stdio");

            toml_content.push_str(&format!("[mcp_servers.{}]\n", name));

            // Codex has no `type`/`transport` key: a `command` selects stdio and
            // a `url` selects streamable HTTP.  Supplying both is a
            // configuration error, so the transport picks exactly one branch.
            match transport {
                "http" | "sse" => {
                    if let Some(url) = config.get("url").and_then(|v| v.as_str()) {
                        toml_content.push_str(&format!("url = \"{}\"\n", escape_toml_string(url)));
                    }

                    // Codex's own OAuth flow activates on `auth = "oauth"`;
                    // unlike Cursor's `auth` block it discovers client
                    // details itself rather than reading them from config,
                    // so this is a marker, not a data carrier. Must precede
                    // the `http_headers` sub-table below: once that table is
                    // opened, a later bare key would belong to it instead of
                    // the server table.
                    if has_oauth_client(&config) {
                        toml_content.push_str("auth = \"oauth\"\n");
                    }

                    // Codex spells static headers `http_headers`; a plain
                    // `headers` table is an unknown key and is ignored.
                    if let Some(headers) = config.get("headers").and_then(|v| v.as_object()) {
                        if !headers.is_empty() {
                            toml_content
                                .push_str(&format!("\n[mcp_servers.{}.http_headers]\n", name));
                            for (key, val) in headers {
                                if let Some(val_str) = val.as_str() {
                                    toml_content.push_str(&format!(
                                        "\"{}\" = \"{}\"\n",
                                        escape_toml_string(key),
                                        escape_toml_string(val_str)
                                    ));
                                }
                            }
                        }
                    }
                }
                _ => {
                    if let Some(command) = config.get("command").and_then(|v| v.as_str()) {
                        toml_content
                            .push_str(&format!("command = \"{}\"\n", escape_toml_string(command)));
                    }

                    if let Some(args) = config.get("args").and_then(|v| v.as_array()) {
                        let args_str: Vec<String> = args
                            .iter()
                            .filter_map(|a| a.as_str())
                            .map(|a| format!("\"{}\"", escape_toml_string(a)))
                            .collect();
                        toml_content.push_str(&format!("args = [{}]\n", args_str.join(", ")));
                    }

                    if let Some(cwd) = config.get("cwd").and_then(|v| v.as_str()) {
                        toml_content.push_str(&format!("cwd = \"{}\"\n", escape_toml_string(cwd)));
                    }

                    // Host variables to forward, populated by
                    // `rewrite_inherited_env` below.  Must precede the `env`
                    // sub-table: once a sub-table is opened, later bare keys
                    // would belong to it instead of the server table.
                    if let Some(env_vars) = config.get("env_vars").and_then(|v| v.as_array()) {
                        let names: Vec<String> = env_vars
                            .iter()
                            .filter_map(|v| v.as_str())
                            .map(|v| format!("\"{}\"", escape_toml_string(v)))
                            .collect();
                        if !names.is_empty() {
                            toml_content.push_str(&format!("env_vars = [{}]\n", names.join(", ")));
                        }
                    }

                    if let Some(env) = config.get("env").and_then(|v| v.as_object()) {
                        if !env.is_empty() {
                            toml_content.push_str(&format!("\n[mcp_servers.{}.env]\n", name));
                            for (key, val) in env {
                                if let Some(val_str) = val.as_str() {
                                    toml_content.push_str(&format!(
                                        "\"{}\" = \"{}\"\n",
                                        escape_toml_string(key),
                                        escape_toml_string(val_str)
                                    ));
                                }
                            }
                        }
                    }
                }
            }

            toml_content.push('\n');
        }

        let path = codex_dir.join("config.toml");
        let existing = read_existing_toml(&path);
        let final_content = merge_toml_mcp_section(&existing, &toml_content);

        fs::write(&path, final_content)
            .map_err(|e| format!("Failed to write .codex/config.toml: {}", e))?;

        Ok(path.display().to_string())
    }

    // ── Cleanup ─────────────────────────────────────────────────────────

    /// Codex CLI merges into `.codex/config.toml` which may contain model or
    /// history settings set by the user.  Strip only the `[mcp_servers.*]`
    /// sections rather than deleting the whole file.
    fn cleanup_mcp_config(&self, dir: &Path) -> Vec<String> {
        let path = dir.join(".codex").join("config.toml");
        if !path.exists() {
            return vec![];
        }
        let existing = read_existing_toml(&path);
        // Pass an empty mcp section to strip all [mcp_servers.*] blocks
        let stripped = merge_toml_mcp_section(&existing, "");
        let trimmed = stripped.trim();
        if trimmed.is_empty() {
            if fs::remove_file(&path).is_ok() {
                return vec![path.display().to_string()];
            }
        } else {
            if fs::write(&path, format!("{}\n", trimmed)).is_ok() {
                return vec![path.display().to_string()];
            }
        }
        vec![]
    }

    fn cleanup_mcp_preview(&self, dir: &Path) -> Vec<String> {
        let path = dir.join(".codex").join("config.toml");
        if path.exists() {
            vec![path.display().to_string()]
        } else {
            vec![]
        }
    }

    // ── Discovery ───────────────────────────────────────────────────────

    fn discover_mcp_servers(&self, _dir: &Path) -> Map<String, Value> {
        // Codex TOML import not implemented yet
        Map::new()
    }

    fn detect_global_install(&self) -> bool {
        super::cli_available("codex")
            || super::home_dir()
                .map(|h| h.join(".codex").exists())
                .unwrap_or(false)
    }

    fn extra_global_skill_dirs(&self) -> Vec<PathBuf> {
        match super::home_dir() {
            Some(home) => vec![home.join(".codex").join("skills")],
            None => vec![],
        }
    }

    fn discover_global_mcp_servers(&self) -> Map<String, Value> {
        let Some(home) = super::home_dir() else {
            return Map::new();
        };
        // ~/.codex/config.toml — user-level Codex CLI config
        let path = home.join(".codex").join("config.toml");
        discover_codex_global_config(&path)
    }

    fn agents_dir(&self, dir: &Path) -> Option<PathBuf> {
        Some(dir.join(".codex").join("agents"))
    }

    fn agents_file_ext(&self) -> &'static str {
        "toml"
    }

    fn convert_agent_content(&self, content: &str, name: &str) -> String {
        convert_md_to_codex_toml(content, name)
    }

    fn capabilities(&self) -> super::AgentCapabilities {
        super::AgentCapabilities {
            hooks: true,
            global_mcp_servers: true,
            ..Default::default()
        }
    }

    fn global_mcp_target(&self) -> Option<GlobalMcpTarget> {
        let home = super::home_dir()?;
        Some(GlobalMcpTarget {
            path: home.join(".codex").join("config.toml"),
            reload_note: Some(
                "Codex, Codex IDE, and the ChatGPT desktop app all pick up changes in new sessions.",
            ),
        })
    }

    fn write_global_mcp_config(
        &self,
        desired: &Map<String, Value>,
        previously_managed: &[String],
    ) -> Result<GlobalMcpWriteReport, String> {
        let target = self.global_mcp_target().ok_or_else(|| {
            "Cannot determine home directory for Codex CLI global MCP config".to_string()
        })?;
        write_codex_global_mcp(&target.path, desired, previously_managed)
    }

    fn hook_events(&self) -> &'static [&'static str] {
        CODEX_SUPPORTED_EVENTS
    }

    fn sync_hooks(
        &self,
        project_dir: &Path,
        hooks: &[crate::core::Hook],
    ) -> Result<Vec<String>, String> {
        sync_codex_hooks(project_dir, hooks)
    }

    fn hook_config_target(&self, dir: &Path) -> Option<super::HookConfigTarget> {
        Some(super::HookConfigTarget::Owned {
            path: dir.join(".codex").join("hooks.json"),
        })
    }
}

// ── Global config discovery ──────────────────────────────────────────────────

/// Parse `~/.codex/config.toml` and return any `[mcp_servers.*]` entries as
/// Automatic canonical MCP server configs.
fn discover_codex_global_config(path: &std::path::Path) -> Map<String, Value> {
    use serde_json::Value;
    use std::fs;

    let mut result = Map::new();

    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return result,
    };

    let doc: toml::Value = match toml::from_str(&content) {
        Ok(v) => v,
        Err(_) => return result,
    };

    let servers = match doc.get("mcp_servers").and_then(|v| v.as_table()) {
        Some(t) => t,
        None => return result,
    };

    for (name, entry) in servers {
        if !crate::core::is_valid_name(name) || name == "automatic" || name == "nexus" {
            continue;
        }
        let table = match entry.as_table() {
            Some(t) => t,
            None => continue,
        };

        let transport = table
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("stdio");

        let mut server = serde_json::Map::new();

        match transport {
            "http" | "sse" => {
                server.insert("type".to_string(), Value::String(transport.to_string()));
                if let Some(url) = table.get("url").and_then(|v| v.as_str()) {
                    server.insert("url".to_string(), Value::String(url.to_string()));
                }
                if let Some(headers) = table.get("headers").and_then(|v| v.as_table()) {
                    let hmap: serde_json::Map<String, Value> = headers
                        .iter()
                        .filter_map(|(k, v)| {
                            v.as_str()
                                .map(|s| (k.clone(), Value::String(s.to_string())))
                        })
                        .collect();
                    server.insert("headers".to_string(), Value::Object(hmap));
                }
            }
            _ => {
                if let Some(cmd) = table.get("command").and_then(|v| v.as_str()) {
                    server.insert("command".to_string(), Value::String(cmd.to_string()));
                }
                if let Some(args) = table.get("args").and_then(|v| v.as_array()) {
                    let arr: Vec<Value> = args
                        .iter()
                        .filter_map(|a| a.as_str().map(|s| Value::String(s.to_string())))
                        .collect();
                    if !arr.is_empty() {
                        server.insert("args".to_string(), Value::Array(arr));
                    }
                }
                if let Some(env) = table.get("env").and_then(|v| v.as_table()) {
                    let emap: serde_json::Map<String, Value> = env
                        .iter()
                        .filter_map(|(k, v)| {
                            v.as_str()
                                .map(|s| (k.clone(), Value::String(s.to_string())))
                        })
                        .collect();
                    if !emap.is_empty() {
                        server.insert("env".to_string(), Value::Object(emap));
                    }
                }
            }
        }

        if !server.is_empty() {
            result.insert(name.clone(), Value::Object(server));
        }
    }

    result
}

// ── TOML Helpers ────────────────────────────────────────────────────────────

/// `true` when the canonical server config carries a usable OAuth client id
/// — the same gate Cursor's own `cursor_auth_block` uses before emitting its
/// `auth` block, reused here since Codex's `auth = "oauth"` answers the same
/// underlying question: is this remote server set up for OAuth at all.
fn has_oauth_client(config: &Value) -> bool {
    config
        .get("oauth")
        .and_then(|v| v.as_object())
        .and_then(|o| o.get("clientId"))
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.is_empty())
}

fn escape_toml_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn read_existing_toml(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

/// Render one MCP server entry in Codex's `.codex/config.toml` dialect,
/// including its own `[mcp_servers.<name>]` header (plus any `env` /
/// `http_headers` sub-tables). The output ends with a blank line so multiple
/// rendered sections can be concatenated directly.
///
/// Deliberately duplicated from the entry-render logic in
/// [`Agent::write_mcp_config`] so the project writer stays byte-identical for
/// the drift tests in `agent/mcp_format_tests.rs`.
fn render_codex_server_section(name: &str, config: &Value) -> String {
    let mut toml_content = String::new();
    let config = config.clone();
    let transport = config
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("stdio");

    toml_content.push_str(&format!("[mcp_servers.{}]\n", name));

    match transport {
        "http" | "sse" => {
            if let Some(url) = config.get("url").and_then(|v| v.as_str()) {
                toml_content.push_str(&format!("url = \"{}\"\n", escape_toml_string(url)));
            }

            if has_oauth_client(&config) {
                toml_content.push_str("auth = \"oauth\"\n");
            }

            if let Some(headers) = config.get("headers").and_then(|v| v.as_object()) {
                if !headers.is_empty() {
                    toml_content
                        .push_str(&format!("\n[mcp_servers.{}.http_headers]\n", name));
                    for (key, val) in headers {
                        if let Some(val_str) = val.as_str() {
                            toml_content.push_str(&format!(
                                "\"{}\" = \"{}\"\n",
                                escape_toml_string(key),
                                escape_toml_string(val_str)
                            ));
                        }
                    }
                }
            }
        }
        _ => {
            if let Some(command) = config.get("command").and_then(|v| v.as_str()) {
                toml_content
                    .push_str(&format!("command = \"{}\"\n", escape_toml_string(command)));
            }

            if let Some(args) = config.get("args").and_then(|v| v.as_array()) {
                let args_str: Vec<String> = args
                    .iter()
                    .filter_map(|a| a.as_str())
                    .map(|a| format!("\"{}\"", escape_toml_string(a)))
                    .collect();
                toml_content.push_str(&format!("args = [{}]\n", args_str.join(", ")));
            }

            if let Some(cwd) = config.get("cwd").and_then(|v| v.as_str()) {
                toml_content.push_str(&format!("cwd = \"{}\"\n", escape_toml_string(cwd)));
            }

            if let Some(env_vars) = config.get("env_vars").and_then(|v| v.as_array()) {
                let names: Vec<String> = env_vars
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(|v| format!("\"{}\"", escape_toml_string(v)))
                    .collect();
                if !names.is_empty() {
                    toml_content.push_str(&format!("env_vars = [{}]\n", names.join(", ")));
                }
            }

            if let Some(env) = config.get("env").and_then(|v| v.as_object()) {
                if !env.is_empty() {
                    toml_content.push_str(&format!("\n[mcp_servers.{}.env]\n", name));
                    for (key, val) in env {
                        if let Some(val_str) = val.as_str() {
                            toml_content.push_str(&format!(
                                "\"{}\" = \"{}\"\n",
                                escape_toml_string(key),
                                escape_toml_string(val_str)
                            ));
                        }
                    }
                }
            }
        }
    }

    toml_content.push('\n');
    toml_content
}

/// If `trimmed` is a section header of the form `[mcp_servers.<name>]` or
/// `[mcp_servers.<name>.<subtable>]`, return `<name>`. Handles both bare and
/// double-quoted keys so foreign entries with hyphens or other non-bare
/// characters still classify correctly. Returns `None` for any other header.
fn parse_mcp_server_header(trimmed: &str) -> Option<&str> {
    let inner = trimmed.strip_prefix('[')?.strip_suffix(']')?;
    let rest = inner.strip_prefix("mcp_servers.")?;
    if let Some(after_quote) = rest.strip_prefix('"') {
        let end_quote = after_quote.find('"')?;
        Some(&after_quote[..end_quote])
    } else {
        let end = rest.find('.').unwrap_or(rest.len());
        Some(&rest[..end])
    }
}

/// Section-aware TOML splice for the user-level Codex config file.
///
/// The unqualified [`merge_toml_mcp_section`] used by the project writer
/// nukes every `[mcp_servers.*]` block and re-emits the entire set — the
/// project owns its `.codex/config.toml` outright. The global file is
/// shared: the user may have hand-added their own `[mcp_servers.*]` entries
/// alongside anything Automatic wrote. This function preserves both those
/// foreign entries and every non-`mcp_servers` section byte-for-byte, and
/// only touches names Automatic knows it managed itself.
fn write_codex_global_mcp(
    path: &Path,
    desired: &Map<String, Value>,
    previously_managed: &[String],
) -> Result<GlobalMcpWriteReport, String> {
    let existing = if path.exists() {
        fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?
    } else {
        String::new()
    };

    // Validate parse — a broken TOML file is an error rather than an empty
    // starting point, matching the JSON merge helper's never-clobber policy.
    if !existing.is_empty() {
        toml::from_str::<toml::Value>(&existing).map_err(|e| {
            format!(
                "{} is not valid TOML — refusing to overwrite: {}",
                path.display(),
                e
            )
        })?;
    }

    // Enumerate every top-level name currently under `mcp_servers` in the file.
    let existing_names: HashSet<String> = if existing.is_empty() {
        HashSet::new()
    } else {
        toml::from_str::<toml::Value>(&existing)
            .ok()
            .and_then(|doc| {
                doc.get("mcp_servers")
                    .and_then(|v| v.as_table())
                    .map(|t| t.keys().cloned().collect())
            })
            .unwrap_or_default()
    };

    let previously_managed_set: HashSet<&str> =
        previously_managed.iter().map(|s| s.as_str()).collect();

    // Classify.
    let mut to_remove: HashSet<String> = HashSet::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut removed: Vec<String> = Vec::new();

    for name in previously_managed {
        if desired.contains_key(name) {
            // Will be re-emitted from `desired` below.
            if existing_names.contains(name) {
                to_remove.insert(name.clone());
            }
        } else if existing_names.contains(name) {
            to_remove.insert(name.clone());
            removed.push(name.clone());
        }
    }

    for name in desired.keys() {
        if !previously_managed_set.contains(name.as_str()) && existing_names.contains(name) {
            // Foreign collision — leave the existing entry alone.
            skipped.push(name.clone());
        }
    }

    // Line-scan the existing content, skipping only sections in `to_remove`.
    let mut retained = String::new();
    let mut skip_block = false;
    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            skip_block = parse_mcp_server_header(trimmed)
                .map(|name| to_remove.contains(name))
                .unwrap_or(false);
        }
        if !skip_block {
            retained.push_str(line);
            retained.push('\n');
        }
    }

    // Build the appended block from every desired entry we own.
    let skipped_set: HashSet<&str> = skipped.iter().map(|s| s.as_str()).collect();
    let mut written: Vec<String> = Vec::new();
    let mut appended = String::new();
    for (name, cfg) in desired {
        if skipped_set.contains(name.as_str()) {
            continue;
        }
        appended.push_str(&render_codex_server_section(name, cfg));
        written.push(name.clone());
    }

    // Compose final content. Trim trailing whitespace on the retained block so
    // the join point is predictable, then re-attach a trailing newline.
    let retained_trimmed = retained.trim_end();
    let appended_trimmed = appended.trim_end();

    let mut final_content = String::new();
    if !retained_trimmed.is_empty() {
        final_content.push_str(retained_trimmed);
    }
    if !appended_trimmed.is_empty() {
        if !final_content.is_empty() {
            final_content.push_str("\n\n");
        }
        final_content.push_str(appended_trimmed);
    }
    if !final_content.is_empty() {
        final_content.push('\n');
    }

    let report = GlobalMcpWriteReport {
        path: path.display().to_string(),
        written,
        removed,
        skipped,
        unchanged: final_content.as_bytes() == existing.as_bytes(),
    };

    if report.unchanged {
        return Ok(report);
    }

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
        }
    }
    fs::write(path, final_content)
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;

    Ok(report)
}

/// Replace existing `[mcp_servers.*]` sections in TOML while preserving
/// everything else.
pub fn merge_toml_mcp_section(existing: &str, mcp_section: &str) -> String {
    if existing.is_empty() {
        return mcp_section.to_string();
    }

    let mut output = String::new();
    let mut skip = false;

    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("[mcp_servers") {
            skip = true;
            continue;
        }
        if skip && trimmed.starts_with('[') && !trimmed.starts_with("[mcp_servers") {
            skip = false;
        }
        if !skip {
            output.push_str(line);
            output.push('\n');
        }
    }

    let trimmed = output.trim_end();
    if trimmed.is_empty() {
        mcp_section.to_string()
    } else {
        format!("{}\n\n{}", trimmed, mcp_section)
    }
}

// ── Agent Content Conversion ────────────────────────────────────────────────

/// Convert Markdown with YAML frontmatter to Codex TOML agent format.
/// Input: Markdown content with YAML frontmatter (the Automatic canonical format).
/// Output: TOML content for Codex agents.
fn convert_md_to_codex_toml(content: &str, fallback_name: &str) -> String {
    let (frontmatter, body) = super::parse_frontmatter(content);

    // Marks this file as Automatic-written so cleanup can tell it apart from
    // a TOML agent the user placed in .codex/agents/ by hand — the sub-agent
    // counterpart to Gemini CLI's identical convention for command TOML.
    let mut toml = String::from("automatic_managed = true\n");

    let name = frontmatter
        .get("name")
        .map(|s| s.as_str())
        .unwrap_or(fallback_name);
    toml.push_str(&format!("name = \"{}\"\n", escape_toml_string(name)));

    if let Some(desc) = frontmatter.get("description") {
        toml.push_str(&format!("description = \"{}\"\n", escape_toml_string(desc)));
    }

    if let Some(model) = frontmatter.get("model") {
        let codex_model = match model.as_str() {
            "inherit" => "inherit",
            "sonnet" => "gpt-5.4",
            "haiku" => "gpt-5.4-mini",
            "opus" => "gpt-5.4",
            other => other,
        };
        toml.push_str(&format!("model = \"{}\"\n", codex_model));
    }

    if frontmatter.contains_key("tools") {
        toml.push_str("sandbox_mode = \"read-only\"\n");
    }

    if let Some(max_turns) = frontmatter.get("maxTurns") {
        toml.push_str(&format!("max_turns = {}\n", max_turns));
    }

    if let Some(reasoning) = frontmatter.get("modelReasoningEffort") {
        toml.push_str(&format!("model_reasoning_effort = \"{}\"\n", reasoning));
    }

    let body_trimmed = body.trim();
    if !body_trimmed.is_empty() {
        toml.push_str(&format!(
            "\ndeveloper_instructions = \"\"\"\n{}\n\"\"\"\n",
            body_trimmed
        ));
    }

    toml
}

// ── Hooks ────────────────────────────────────────────────────────────────────
//
// Codex CLI supports a smaller event set than Claude Code. We write the full
// `.codex/hooks.json` file (Automatic owns it; users wanting more control can
// fall back to `.codex/config.toml`'s inline `[hooks]` table, which we do not
// touch). Script-type handlers are written to `.codex/hooks/{slug}.sh`.

const CODEX_SUPPORTED_EVENTS: &[&str] = &[
    "SessionStart",
    "SessionEnd",
    "PreToolUse",
    "PermissionRequest",
    "PostToolUse",
    "PreCompact",
    "PostCompact",
    "UserPromptSubmit",
    "SubagentStart",
    "SubagentStop",
    "Stop",
];

fn sync_codex_hooks(
    project_dir: &Path,
    hooks: &[crate::core::Hook],
) -> Result<Vec<String>, String> {
    let hooks_file = project_dir.join(".codex").join("hooks.json");
    let spec = super::HookWriteSpec {
        supported_events: CODEX_SUPPORTED_EVENTS,
        scripts_dir: project_dir.join(".codex").join("hooks"),
        script_command: |file_name| format!("./.codex/hooks/{}", file_name),
        handler: super::standard_command_handler,
        group_extras: super::no_group_extras,
    };
    super::write_owned_hooks_file(&hooks_file, hooks, &spec)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn hook_events_declares_eleven_events() {
        let events = CodexCli.hook_events();
        assert_eq!(
            events.len(),
            11,
            "expected 11 documented Codex CLI hook events, found {}",
            events.len()
        );
        for event in [
            "SessionEnd",
            "PreCompact",
            "PostCompact",
            "SubagentStart",
            "SubagentStop",
        ] {
            assert!(
                events.contains(&event),
                "'{event}' is documented upstream but missing from CODEX_SUPPORTED_EVENTS"
            );
        }
    }

    fn stdio_servers() -> Map<String, Value> {
        let mut s = Map::new();
        s.insert(
            "automatic".to_string(),
            json!({"type":"stdio","command":"/usr/local/bin/automatic","args":["mcp-serve"]}),
        );
        s.insert(
            "github".to_string(),
            json!({"type":"stdio","command":"npx","args":["-y","@modelcontextprotocol/server-github"],"env":{"GITHUB_TOKEN":"ghp_test123"}}),
        );
        s
    }

    fn http_servers() -> Map<String, Value> {
        let mut s = Map::new();
        s.insert(
            "remote-api".to_string(),
            json!({"type":"http","url":"https://api.example.com/mcp","headers":{"Authorization":"Bearer tok_abc123"},"oauth":{"clientId":"client_123","scope":"read"}}),
        );
        s
    }

    #[test]
    fn test_detect() {
        let dir = tempdir().unwrap();
        assert!(!CodexCli.detect_in(dir.path()));

        fs::create_dir_all(dir.path().join(".codex")).unwrap();
        fs::write(dir.path().join(".codex/config.toml"), "").unwrap();
        assert!(CodexCli.detect_in(dir.path()));
    }

    #[test]
    fn test_write_stdio() {
        let dir = tempdir().unwrap();
        CodexCli
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join(".codex/config.toml")).unwrap();
        assert!(content.contains("[mcp_servers.automatic]"));
        assert!(content.contains("[mcp_servers.github]"));
        assert!(content.contains("GITHUB_TOKEN"));
    }

    #[test]
    fn test_write_http() {
        let dir = tempdir().unwrap();
        CodexCli
            .write_mcp_config(dir.path(), &http_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join(".codex/config.toml")).unwrap();
        assert!(content.contains("[mcp_servers.remote-api]"));
        // Codex infers streamable HTTP from the presence of `url`; it has no
        // `type`/`transport` key.
        assert!(!content.contains("type = "));
        assert!(content.contains("url = \"https://api.example.com/mcp\""));
        assert!(content.contains("[mcp_servers.remote-api.http_headers]"));
        assert!(content.contains("Authorization"));
    }

    // ── Hooks ───────────────────────────────────────────────────────────────

    fn codex_cmd_hook(name: &str, event: &str, command: &str) -> crate::core::Hook {
        crate::core::Hook {
            name: name.to_string(),
            agent: "codex".to_string(),
            event: event.to_string(),
            matcher: None,
            handler: crate::core::HookHandler::Command {
                command: command.to_string(),
            },
            timeout_sec: Some(45),
            plugin_id: None,
            _author: None,
        }
    }

    #[test]
    fn codex_hook_sync_writes_dedicated_file() {
        let dir = tempdir().unwrap();
        let hooks = vec![codex_cmd_hook("hi", "SessionStart", "echo hi")];
        let written = CodexCli.sync_hooks(dir.path(), &hooks).unwrap();
        let path = dir.path().join(".codex/hooks.json");
        assert!(path.exists());
        assert!(written.iter().any(|w| w.ends_with("hooks.json")));

        let raw = fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let handler = &v["hooks"]["SessionStart"][0]["hooks"][0];
        assert_eq!(handler["type"], "command");
        assert_eq!(handler["command"], "echo hi");
        assert_eq!(handler["timeout"], 45);
    }

    #[test]
    fn codex_hook_sync_skips_unsupported_events() {
        let dir = tempdir().unwrap();
        // Claude-only event — Codex must skip it without failing the sync.
        let hooks = vec![codex_cmd_hook("setup", "Setup", "echo nope")];
        let written = CodexCli.sync_hooks(dir.path(), &hooks).unwrap();
        assert!(written.is_empty());
        assert!(!dir.path().join(".codex/hooks.json").exists());
    }

    #[test]
    fn codex_hook_sync_accepts_the_five_newly_added_events() {
        let dir = tempdir().unwrap();
        let hooks = vec![
            codex_cmd_hook("a", "SessionEnd", "echo a"),
            codex_cmd_hook("b", "PreCompact", "echo b"),
            codex_cmd_hook("c", "PostCompact", "echo c"),
            codex_cmd_hook("d", "SubagentStart", "echo d"),
            codex_cmd_hook("e", "SubagentStop", "echo e"),
        ];
        let written = CodexCli.sync_hooks(dir.path(), &hooks).unwrap();
        assert!(!written.is_empty());
        let raw = fs::read_to_string(dir.path().join(".codex/hooks.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        for event in [
            "SessionEnd",
            "PreCompact",
            "PostCompact",
            "SubagentStart",
            "SubagentStop",
        ] {
            assert!(
                v["hooks"].get(event).is_some(),
                "expected '{event}' to be written to hooks.json"
            );
        }
    }

    #[test]
    fn codex_hook_sync_removes_file_when_no_hooks() {
        let dir = tempdir().unwrap();
        // Pre-existing hooks file from an earlier sync.
        let hooks = vec![codex_cmd_hook("temp", "Stop", "echo bye")];
        CodexCli.sync_hooks(dir.path(), &hooks).unwrap();
        assert!(dir.path().join(".codex/hooks.json").exists());

        CodexCli.sync_hooks(dir.path(), &[]).unwrap();
        assert!(!dir.path().join(".codex/hooks.json").exists());
    }

    #[test]
    fn test_toml_merge() {
        let existing =
            "[model]\nprovider = \"anthropic\"\n\n[mcp_servers.old_server]\ncommand = \"old\"\n";
        let new_mcp = "[mcp_servers.automatic]\ncommand = \"automatic\"\n\n";
        let merged = merge_toml_mcp_section(existing, new_mcp);

        assert!(merged.contains("[model]"));
        assert!(merged.contains("[mcp_servers.automatic]"));
        assert!(!merged.contains("[mcp_servers.old_server]"));
    }
}
