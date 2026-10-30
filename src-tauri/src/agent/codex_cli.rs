use serde_json::{Map, Value};
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::{
    escape_toml_multiline_string, escape_toml_string, toml_table_key, Agent, GlobalMcpTarget,
    GlobalMcpWriteReport, HeaderValuePart,
};

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

    /// `.codex/` is Codex's alone: `config.toml`, hooks and sub-agents.
    fn owned_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".codex")]
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

    /// TOML has no interpolation, so a `${NAME}` inside `http_headers` would
    /// be sent to the server as literal text.  Codex has two keys that read a
    /// header from the environment instead: `bearer_token_env_var` for
    /// `Authorization: Bearer ${NAME}`, and `env_http_headers` for a header
    /// whose whole value is `${NAME}`.  A reference mixed with other text has
    /// no Codex equivalent and stays in `http_headers` unchanged.
    ///
    /// Both keys are derived here and nowhere else.  A stored config that
    /// already carries one is not trusted: the editor shows `headers` only, so
    /// a passed-through key would send an environment variable the user never
    /// saw.  They are dropped before the headers are read.
    fn rewrite_header_env_references(&self, server: &mut Map<String, Value>) {
        server.remove("bearer_token_env_var");
        server.remove("env_http_headers");

        let Some(Value::Object(headers)) = server.get_mut("headers") else {
            return;
        };

        let mut bearer: Option<(String, String)> = None;
        let mut env_http_headers = Map::new();
        for (header, value) in headers.iter() {
            let Some(text) = value.as_str() else {
                continue;
            };
            match super::parse_header_value(text).as_slice() {
                [HeaderValuePart::EnvReference(name)] => {
                    env_http_headers.insert(header.clone(), Value::String(name.clone()));
                }
                [HeaderValuePart::Literal(scheme), HeaderValuePart::EnvReference(name)]
                    if header.eq_ignore_ascii_case("authorization")
                        && scheme == "Bearer "
                        && bearer.is_none() =>
                {
                    bearer = Some((header.clone(), name.clone()));
                }
                _ => {}
            }
        }

        for header in env_http_headers.keys() {
            headers.remove(header);
        }
        if let Some((header, _)) = &bearer {
            headers.remove(header);
        }
        if headers.is_empty() {
            server.remove("headers");
        }
        if let Some((_, name)) = bearer {
            server.insert("bearer_token_env_var".to_string(), Value::String(name));
        }
        if !env_http_headers.is_empty() {
            server.insert("env_http_headers".to_string(), Value::Object(env_http_headers));
        }
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

            toml_content.push_str(&format!("[mcp_servers.{}]\n", toml_table_key(name)));

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
                    toml_content.push_str(&render_codex_bearer_token_env_var(&config));

                    // Codex spells static headers `http_headers`; a plain
                    // `headers` table is an unknown key and is ignored.
                    if let Some(headers) = config.get("headers").and_then(|v| v.as_object()) {
                        if !headers.is_empty() {
                            toml_content
                                .push_str(&format!("\n[mcp_servers.{}.http_headers]\n", toml_table_key(name)));
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
                    toml_content.push_str(&render_codex_env_http_headers(name, &config));
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
                            toml_content
                                .push_str(&format!("\n[mcp_servers.{}.env]\n", toml_table_key(name)));
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

    /// Codex CLI merges into `.codex/config.toml`, which also holds
    /// `[mcp_servers.*]` sections the user hand-added alongside their model
    /// and history settings.  Strip only the sections named in
    /// `managed_names`; every other `[mcp_servers.*]` block and every
    /// non-`mcp_servers` section survives.  Delete the file only when the
    /// strip empties it whole.
    fn cleanup_mcp_config(&self, dir: &Path, managed_names: &[String]) -> Vec<String> {
        let path = dir.join(".codex").join("config.toml");
        if !path.exists() {
            return vec![];
        }
        let existing = read_existing_toml(&path);
        if existing.is_empty() {
            return vec![];
        }

        // Enumerate the `[mcp_servers.*]` entries actually in the file.
        let existing_names: HashSet<String> = toml::from_str::<toml::Value>(&existing)
            .ok()
            .and_then(|doc| {
                doc.get("mcp_servers")
                    .and_then(|v| v.as_table())
                    .map(|t| t.keys().cloned().collect())
            })
            .unwrap_or_default();

        // Only strip names that are both managed by Automatic and present.
        let to_remove: HashSet<&str> = managed_names
            .iter()
            .map(|s| s.as_str())
            .filter(|n| existing_names.contains(*n))
            .collect();
        if to_remove.is_empty() {
            return vec![];
        }

        // Line-scan and skip only the matching `[mcp_servers.<name>]` blocks
        // (and their sub-tables such as `[mcp_servers.<name>.env]`).
        let mut retained = String::new();
        let mut skip_block = false;
        for line in existing.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                skip_block = parse_mcp_server_header(trimmed)
                    .map(|name| to_remove.contains(name.as_str()))
                    .unwrap_or(false);
            }
            if !skip_block {
                retained.push_str(line);
                retained.push('\n');
            }
        }

        let final_trimmed = retained.trim_end();
        if final_trimmed.is_empty() {
            if fs::remove_file(&path).is_ok() {
                return vec![path.display().to_string()];
            }
            return vec![];
        }

        let final_content = format!("{}\n", final_trimmed);
        if final_content.as_bytes() == existing.as_bytes() {
            return vec![];
        }
        if fs::write(&path, final_content).is_ok() {
            return vec![path.display().to_string()];
        }
        vec![]
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

    /// Compare by field: a hand-written TOML file never matches the rendered
    /// one byte for byte, so a byte comparison would keep an adopted file in
    /// conflict for ever.  Formatting alone is not a difference.  A comment
    /// is, because sync would delete it.  The byte comparison stays first
    /// because a match needs no parsing.
    fn agent_file_differences(&self, disk: &str, expected: &str) -> Vec<String> {
        if disk == expected {
            return Vec::new();
        }
        codex_agent_differences(disk, expected)
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

/// Render the `bearer_token_env_var` key set by
/// [`Agent::rewrite_header_env_references`], or nothing when the server has
/// none.  It is a bare key, so callers must emit it before any sub-table.
fn render_codex_bearer_token_env_var(config: &Value) -> String {
    match config.get("bearer_token_env_var").and_then(|v| v.as_str()) {
        Some(name) => format!("bearer_token_env_var = \"{}\"\n", escape_toml_string(name)),
        None => String::new(),
    }
}

/// Render the `env_http_headers` sub-table set by
/// [`Agent::rewrite_header_env_references`]: header name to the environment
/// variable Codex reads its value from.  Empty when the server has none.
fn render_codex_env_http_headers(name: &str, config: &Value) -> String {
    let Some(headers) = config.get("env_http_headers").and_then(|v| v.as_object()) else {
        return String::new();
    };
    if headers.is_empty() {
        return String::new();
    }
    let mut section = format!(
        "\n[mcp_servers.{}.env_http_headers]\n",
        toml_table_key(name)
    );
    for (header, variable) in headers {
        if let Some(variable) = variable.as_str() {
            section.push_str(&format!(
                "\"{}\" = \"{}\"\n",
                escape_toml_string(header),
                escape_toml_string(variable)
            ));
        }
    }
    section
}

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

    toml_content.push_str(&format!("[mcp_servers.{}]\n", toml_table_key(name)));

    match transport {
        "http" | "sse" => {
            if let Some(url) = config.get("url").and_then(|v| v.as_str()) {
                toml_content.push_str(&format!("url = \"{}\"\n", escape_toml_string(url)));
            }

            if has_oauth_client(&config) {
                toml_content.push_str("auth = \"oauth\"\n");
            }
            toml_content.push_str(&render_codex_bearer_token_env_var(&config));

            if let Some(headers) = config.get("headers").and_then(|v| v.as_object()) {
                if !headers.is_empty() {
                    toml_content
                        .push_str(&format!("\n[mcp_servers.{}.http_headers]\n", toml_table_key(name)));
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
            toml_content.push_str(&render_codex_env_http_headers(name, &config));
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
                    toml_content
                                .push_str(&format!("\n[mcp_servers.{}.env]\n", toml_table_key(name)));
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
/// `[mcp_servers.<name>.<subtable>]`, return `<name>`, decoded.  The key is
/// parsed as TOML so a quoted name matches the decoded names `toml` reports,
/// whatever it holds (a dot, an escaped quote, `\\u000A`).  Returns `None`
/// for any other header.
fn parse_mcp_server_header(trimmed: &str) -> Option<String> {
    let inner = trimmed.strip_prefix('[')?.strip_suffix(']')?;
    let path = toml_edit::Key::parse(inner).ok()?;
    match path.as_slice() {
        [table, name, ..] if table.get() == "mcp_servers" => Some(name.get().to_string()),
        _ => None,
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
                .map(|name| to_remove.contains(name.as_str()))
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
        toml.push_str(&format!(
            "model = \"{}\"\n",
            escape_toml_string(codex_model)
        ));
    }

    if frontmatter.contains_key("tools") {
        toml.push_str("sandbox_mode = \"read-only\"\n");
    }

    // Frontmatter is free text.  Writing a bad value here raw would make the
    // whole file unloadable, so only an integer is written.
    if let Some(max_turns) = frontmatter
        .get("maxTurns")
        .and_then(|value| value.parse::<i64>().ok())
    {
        toml.push_str(&format!("max_turns = {}\n", max_turns));
    }

    if let Some(reasoning) = frontmatter.get("modelReasoningEffort") {
        toml.push_str(&format!(
            "model_reasoning_effort = \"{}\"\n",
            escape_toml_string(reasoning)
        ));
    }

    let body_trimmed = body.trim();
    if !body_trimmed.is_empty() {
        toml.push_str(&format!(
            "\ndeveloper_instructions = \"\"\"\n{}\n\"\"\"\n",
            escape_toml_multiline_string(body_trimmed)
        ));
    }

    toml
}

/// The sorted names of the fields that differ between two Codex agent files.
/// `content` stands for the whole file when either one is not valid TOML, and
/// `comments` means `disk` has a comment `expected` lacks: sync would delete
/// it, so it counts as something the user wrote.
fn codex_agent_differences(disk: &str, expected: &str) -> Vec<String> {
    let (Ok(disk_table), Ok(expected_table)) = (
        toml::from_str::<toml::Table>(disk),
        toml::from_str::<toml::Table>(expected),
    ) else {
        return vec!["content".to_string()];
    };
    let (Some(disk_comments), Some(expected_comments)) =
        (toml_comments(disk), toml_comments(expected))
    else {
        return vec!["content".to_string()];
    };

    let keys: BTreeSet<&String> = disk_table.keys().chain(expected_table.keys()).collect();
    let mut differences: BTreeSet<String> = keys
        .into_iter()
        .filter(|key| {
            !codex_agent_field_matches(key, disk_table.get(*key), expected_table.get(*key))
        })
        .cloned()
        .collect();
    if !disk_comments.is_subset(&expected_comments) {
        differences.insert("comments".to_string());
    }
    differences.into_iter().collect()
}

/// Every comment in a TOML file, trimmed.  This reads the parser's decor, the
/// text it keeps between tokens, so a `#` inside a string is not a comment.
/// Returns `None` when the file does not parse.
fn toml_comments(text: &str) -> Option<BTreeSet<String>> {
    let document = text.parse::<toml_edit::DocumentMut>().ok()?;
    let mut comments = BTreeSet::new();
    collect_table_comments(document.as_table(), &mut comments);
    collect_raw_comments(Some(document.trailing()), &mut comments);
    Some(comments)
}

fn collect_table_comments(table: &toml_edit::Table, out: &mut BTreeSet<String>) {
    collect_decor_comments(table.decor(), out);
    for (name, item) in table.iter() {
        if let Some(key) = table.key(name) {
            collect_key_comments(key, out);
        }
        match item {
            toml_edit::Item::Value(value) => collect_value_comments(value, out),
            toml_edit::Item::Table(child) => collect_table_comments(child, out),
            toml_edit::Item::ArrayOfTables(tables) => {
                for child in tables.iter() {
                    collect_table_comments(child, out);
                }
            }
            toml_edit::Item::None => {}
        }
    }
}

fn collect_value_comments(value: &toml_edit::Value, out: &mut BTreeSet<String>) {
    collect_decor_comments(value.decor(), out);
    match value {
        toml_edit::Value::Array(array) => {
            for element in array.iter() {
                collect_value_comments(element, out);
            }
            collect_raw_comments(Some(array.trailing()), out);
        }
        toml_edit::Value::InlineTable(table) => {
            collect_raw_comments(Some(table.trailing()), out);
            for (name, element) in table.iter() {
                if let Some(key) = table.key(name) {
                    collect_key_comments(key, out);
                }
                collect_value_comments(element, out);
            }
        }
        _ => {}
    }
}

fn collect_key_comments(key: &toml_edit::Key, out: &mut BTreeSet<String>) {
    collect_decor_comments(key.leaf_decor(), out);
    collect_decor_comments(key.dotted_decor(), out);
}

fn collect_decor_comments(decor: &toml_edit::Decor, out: &mut BTreeSet<String>) {
    collect_raw_comments(decor.prefix(), out);
    collect_raw_comments(decor.suffix(), out);
}

/// Decor holds only whitespace and comments, so a `#` here starts a comment.
fn collect_raw_comments(raw: Option<&toml_edit::RawString>, out: &mut BTreeSet<String>) {
    let text = raw.and_then(toml_edit::RawString::as_str).unwrap_or("");
    for line in text.lines().map(str::trim) {
        if line.starts_with('#') {
            out.insert(line.to_string());
        }
    }
}

/// `true` when one field of two Codex agent files means the same thing.
/// The defaults mirror what `convert_codex_toml_to_markdown` fills in for an
/// absent field.  Every other field must be present on both sides, so a
/// setting Automatic cannot keep (`sandbox_mode = "workspace-write"`, an
/// unknown key) always counts as a difference.
fn codex_agent_field_matches(
    key: &str,
    disk: Option<&toml::Value>,
    expected: Option<&toml::Value>,
) -> bool {
    let or_default = |value: Option<&toml::Value>, default: &str| {
        value
            .cloned()
            .unwrap_or_else(|| toml::Value::String(default.to_string()))
    };
    let trimmed = |value: Option<&toml::Value>| match or_default(value, "") {
        toml::Value::String(text) => toml::Value::String(text.trim().to_string()),
        other => other,
    };

    match key {
        // Sync writes `true`.  `false` is the user's edit, and the render
        // cannot keep it.
        "automatic_managed" => {
            let managed = |value: Option<&toml::Value>| {
                value.is_none_or(|value| value.as_bool() == Some(true))
            };
            managed(disk) && managed(expected)
        }
        "name" if disk.is_none() || expected.is_none() => true,
        "model" => or_default(disk, "inherit") == or_default(expected, "inherit"),
        "description" => or_default(disk, "") == or_default(expected, ""),
        "developer_instructions" => trimmed(disk) == trimmed(expected),
        _ => disk == expected,
    }
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
    // Codex ignores any `matcher` on this event and limits its timeout to
    // one through three seconds (https://learn.chatgpt.com/docs/hooks).
    "Interrupt",
];

fn sync_codex_hooks(
    project_dir: &Path,
    hooks: &[crate::core::Hook],
) -> Result<Vec<String>, String> {
    let hooks_file = project_dir.join(".codex").join("hooks.json");
    let spec = super::HookWriteSpec {
        supported_events: CODEX_SUPPORTED_EVENTS,
        project_dir: project_dir.to_path_buf(),
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

    const RENDERED_AGENT: &str = "automatic_managed = true\nname = \"planner\"\ndescription = \"Plans\"\nmodel = \"inherit\"\n\ndeveloper_instructions = \"\"\"\nPlan.\n\"\"\"\n";

    fn differences(disk: &str, expected: &str) -> Vec<String> {
        CodexCli.agent_file_differences(disk, expected)
    }

    fn stdio_server() -> Value {
        json!({"type": "stdio", "command": "npx"})
    }

    /// A server name comes from project config.  It must stay one key, or it
    /// could close the header and add a table such as
    /// `[sandbox_workspace_write]` to the user's Codex config.
    const CRAFTED_NAME: &str = "x]\n[sandbox_workspace_write]\nnetwork_access = true\n[mcp_servers.y";

    #[test]
    fn crafted_server_name_adds_no_table_to_the_project_config() {
        let dir = tempdir().unwrap();
        let mut servers = Map::new();
        servers.insert(CRAFTED_NAME.to_string(), stdio_server());

        CodexCli.write_mcp_config(dir.path(), &servers).unwrap();

        let raw = fs::read_to_string(dir.path().join(".codex/config.toml")).unwrap();
        let doc = toml::from_str::<toml::Table>(&raw).expect("valid TOML");
        assert_eq!(doc.keys().collect::<Vec<_>>(), vec!["mcp_servers"], "{raw}");
        let names: Vec<&String> = doc["mcp_servers"].as_table().unwrap().keys().collect();
        assert_eq!(names, vec![CRAFTED_NAME], "{raw}");
    }

    #[test]
    fn crafted_server_name_stays_one_key_in_every_sub_table() {
        let config = json!({
            "type": "http",
            "url": "https://example.test",
            "headers": {"X-One": "1"},
            "env_http_headers": {"X-Two": "TOKEN"},
        });
        let http = render_codex_server_section(CRAFTED_NAME, &config);
        let stdio = render_codex_server_section(
            CRAFTED_NAME,
            &json!({"type": "stdio", "command": "npx", "env": {"A": "1"}}),
        );

        for section in [http, stdio] {
            let doc = toml::from_str::<toml::Table>(&section).expect("valid TOML");
            assert_eq!(doc.keys().collect::<Vec<_>>(), vec!["mcp_servers"], "{section}");
            let servers = doc["mcp_servers"].as_table().unwrap();
            assert_eq!(servers.len(), 1, "{section}");
            assert!(servers.contains_key(CRAFTED_NAME), "{section}");
        }
    }

    /// A rendered header must decode to the name `toml` reports, or removal
    /// would never find a quoted server.
    #[test]
    fn mcp_header_parser_decodes_quoted_names() {
        for name in [CRAFTED_NAME, "a.b", "say \"hi\"", "back\\slash", "plain-name"] {
            let key = toml_table_key(name);
            assert_eq!(
                parse_mcp_server_header(&format!("[mcp_servers.{key}]")).as_deref(),
                Some(name)
            );
            assert_eq!(
                parse_mcp_server_header(&format!("[mcp_servers.{key}.env]")).as_deref(),
                Some(name)
            );
        }
        assert_eq!(parse_mcp_server_header("[mcp_servers]"), None);
        assert_eq!(parse_mcp_server_header("[model]"), None);
    }

    #[test]
    fn cleanup_removes_a_quoted_server_and_keeps_the_rest() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(".codex/config.toml");
        let mut servers = Map::new();
        servers.insert(CRAFTED_NAME.to_string(), stdio_server());
        CodexCli.write_mcp_config(dir.path(), &servers).unwrap();
        let existing = fs::read_to_string(&path).unwrap();
        fs::write(
            &path,
            format!("[model]\nprovider = \"a\"\n\n{existing}\n[mcp_servers.user-server]\ncommand = \"u\"\n"),
        )
        .unwrap();

        CodexCli.cleanup_mcp_config(dir.path(), &[CRAFTED_NAME.to_string()]);

        let raw = fs::read_to_string(&path).unwrap();
        let doc = toml::from_str::<toml::Table>(&raw).expect("valid TOML");
        assert!(doc.contains_key("model"), "{raw}");
        let names: Vec<&String> = doc["mcp_servers"].as_table().unwrap().keys().collect();
        assert_eq!(names, vec!["user-server"], "{raw}");
    }

    #[test]
    fn global_writer_removes_a_quoted_server_it_managed() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut desired = Map::new();
        desired.insert(CRAFTED_NAME.to_string(), stdio_server());
        write_codex_global_mcp(&path, &desired, &[]).unwrap();

        let report =
            write_codex_global_mcp(&path, &Map::new(), &[CRAFTED_NAME.to_string()]).unwrap();

        assert_eq!(report.removed, vec![CRAFTED_NAME]);
        let raw = fs::read_to_string(&path).unwrap_or_default();
        assert!(!raw.contains("sandbox_workspace_write"), "{raw}");
    }

    /// Existing names must render byte-identically, so no user sees a config
    /// rewrite from the quoting.
    #[test]
    fn config_output_for_bare_names_is_pinned() {
        let dir = tempdir().unwrap();
        let mut servers = Map::new();
        servers.insert(
            "github".to_string(),
            json!({"type": "stdio", "command": "npx", "args": ["-y", "gh"], "env": {"A": "1"}}),
        );
        servers.insert(
            "remote_api-2".to_string(),
            json!({"type": "http", "url": "https://example.test/mcp", "headers": {"X-Key": "v"}}),
        );

        CodexCli.write_mcp_config(dir.path(), &servers).unwrap();

        let raw = fs::read_to_string(dir.path().join(".codex/config.toml")).unwrap();
        assert_eq!(
            raw,
            "[mcp_servers.github]\ncommand = \"npx\"\nargs = [\"-y\", \"gh\"]\n\n[mcp_servers.github.env]\n\"A\" = \"1\"\n\n[mcp_servers.remote_api-2]\nurl = \"https://example.test/mcp\"\n\n[mcp_servers.remote_api-2.http_headers]\n\"X-Key\" = \"v\"\n\n"
        );
    }

    #[test]
    fn agent_differences_ignore_formatting_order_and_the_marker() {
        let disk = "developer_instructions = \"Plan.\"\nmodel = \"inherit\"\ndescription = \"Plans\"\nname = \"planner\"\n";
        assert_eq!(differences(disk, RENDERED_AGENT), Vec::<String>::new());
    }

    /// A comment can sit anywhere the parser keeps decor.
    #[test]
    fn agent_differences_report_a_comment_wherever_it_sits() {
        for disk in [
            format!("# above\n{RENDERED_AGENT}"),
            format!("{RENDERED_AGENT}# below\n"),
            RENDERED_AGENT.replacen("name = \"planner\"", "name = \"planner\" # beside", 1),
            RENDERED_AGENT.replacen("name =", "# between\nname =", 1),
            RENDERED_AGENT.replacen("name = \"planner\"", "name = \"planner\"\nalso = [\n  1, # inside\n  # last\n]", 1),
            format!("{RENDERED_AGENT}\n# before\n[extra] # beside\n# in table\nvalue = 1\n"),
        ] {
            let found = differences(&disk, RENDERED_AGENT);
            assert!(found.contains(&"comments".to_string()), "{found:?} for {disk}");
        }
    }

    /// `#` inside a string is content.
    #[test]
    fn agent_differences_do_not_read_a_hash_in_a_string_as_a_comment() {
        let expected = "name = \"#1 planner\"\ndeveloper_instructions = \"\"\"\n# Heading\nPlan. # not a comment\n\"\"\"\n";
        let disk = "developer_instructions = '''\n# Heading\nPlan. # not a comment\n'''\nname = '#1 planner'\n";
        assert_eq!(differences(disk, expected), Vec::<String>::new());
    }

    #[test]
    fn agent_differences_accept_a_comment_expected_also_has() {
        let expected = format!("# shared\n{RENDERED_AGENT}");
        let disk = format!("{RENDERED_AGENT}# shared\n");
        assert_eq!(differences(&disk, &expected), Vec::<String>::new());
    }

    /// Comments sort with the fields, so the list stays in one order.
    #[test]
    fn agent_differences_sort_comments_with_the_fields() {
        let disk = format!("# note\n{}", RENDERED_AGENT.replace("Plans", "Changed"));
        assert_eq!(differences(&disk, RENDERED_AGENT), vec!["comments", "description"]);
    }

    #[test]
    fn agent_differences_treat_automatic_managed_false_as_a_change() {
        let disk = RENDERED_AGENT.replace("automatic_managed = true", "automatic_managed = false");
        assert_eq!(differences(&disk, RENDERED_AGENT), vec!["automatic_managed"]);

        let text = RENDERED_AGENT.replace("automatic_managed = true", "automatic_managed = \"true\"");
        assert_eq!(differences(&text, RENDERED_AGENT), vec!["automatic_managed"]);

        let absent = RENDERED_AGENT.replace("automatic_managed = true\n", "");
        assert_eq!(differences(&absent, RENDERED_AGENT), Vec::<String>::new());
    }

    #[test]
    fn agent_differences_fill_in_a_missing_model_description_and_name() {
        let expected = "automatic_managed = true\nname = \"planner\"\ndescription = \"\"\nmodel = \"inherit\"\n";
        assert_eq!(differences("", expected), Vec::<String>::new());
    }

    #[test]
    fn agent_differences_name_each_changed_field_in_order() {
        let disk = "name = \"other\"\ndescription = \"Changed\"\nmodel = \"gpt-5.4\"\ndeveloper_instructions = \"Changed.\"\n";
        assert_eq!(
            differences(disk, RENDERED_AGENT),
            vec!["description", "developer_instructions", "model", "name"]
        );
    }

    #[test]
    fn agent_differences_report_a_key_present_on_one_side() {
        let extra = RENDERED_AGENT.replacen("name =", "custom_flag = true\nname =", 1);
        assert_eq!(differences(&extra, RENDERED_AGENT), vec!["custom_flag"]);

        let read_only =
            RENDERED_AGENT.replacen("name =", "sandbox_mode = \"read-only\"\nname =", 1);
        assert_eq!(differences(&read_only, RENDERED_AGENT), vec!["sandbox_mode"]);
        assert_eq!(differences(RENDERED_AGENT, &read_only), vec!["sandbox_mode"]);
    }

    #[test]
    fn agent_differences_report_content_for_unparseable_toml() {
        assert_eq!(differences("not = = toml", RENDERED_AGENT), vec!["content"]);
        assert_eq!(differences(RENDERED_AGENT, "not = = toml"), vec!["content"]);
    }

    /// A byte match is a match, whether or not the file parses.
    #[test]
    fn agent_differences_accept_an_identical_file_that_does_not_parse() {
        let unparseable = "not = = toml\n";
        assert_eq!(differences(unparseable, unparseable), Vec::<String>::new());
    }

    /// A control character in the body must not break the TOML either.
    #[test]
    fn agent_body_with_control_characters_renders_valid_toml() {
        let body = "Ring \u{7}, form \u{c}, escape \u{1b}, delete \u{7f}, bare \r return.\n\tTabbed.";
        let rendered = CodexCli.convert_agent_content(
            &format!("---\nname: planner\n---\n{body}\n"),
            "planner",
        );

        assert!(rendered.contains("Ring \\u0007,"), "{rendered}");
        assert!(rendered.contains("\n\tTabbed."), "{rendered}");
        let table = toml::from_str::<toml::Table>(&rendered).expect("valid TOML");
        let stored = table["developer_instructions"].as_str().expect("string");
        assert_eq!(stored.trim(), body);
    }

    /// A CRLF line break is allowed, so it stays literal.
    #[test]
    fn agent_body_keeps_crlf_literal() {
        let body = "First.\r\nSecond.";
        let rendered = CodexCli.convert_agent_content(
            &format!("---\nname: planner\n---\n{body}\n"),
            "planner",
        );

        assert!(rendered.contains(body), "{rendered:?}");
        assert!(toml::from_str::<toml::Table>(&rendered).is_ok(), "{rendered:?}");
    }

    /// Frontmatter is free text.  No value in it may make the file
    /// unloadable, and a `maxTurns` that is not an integer is left out.
    #[test]
    fn agent_frontmatter_values_render_valid_toml() {
        let content = "---\nname: pla\u{7}nner\ndescription: Say \"hi\" \\ \u{1}\nmodel: gpt\"5\\\u{7}\nmaxTurns: many\nmodelReasoningEffort: hi\"gh\\\u{7}\n---\nPlan.\n";

        let rendered = CodexCli.convert_agent_content(content, "planner");

        let table = toml::from_str::<toml::Table>(&rendered).expect("valid TOML");
        assert_eq!(table["name"].as_str(), Some("pla\u{7}nner"));
        assert_eq!(table["model"].as_str(), Some("gpt\"5\\\u{7}"));
        assert_eq!(
            table["model_reasoning_effort"].as_str(),
            Some("hi\"gh\\\u{7}")
        );
        assert!(!table.contains_key("max_turns"), "{rendered}");
    }

    #[test]
    fn agent_max_turns_is_written_when_it_is_an_integer() {
        let rendered = CodexCli.convert_agent_content(
            "---\nname: planner\nmaxTurns: 5\n---\nPlan.\n",
            "planner",
        );

        assert!(rendered.contains("max_turns = 5\n"), "{rendered}");
    }

    /// A single-line string cannot hold a raw line break or control
    /// character, but tab is allowed and stays literal.
    #[test]
    fn escape_toml_string_produces_a_valid_single_line_string() {
        let value = "line\nbreak\r\u{7}\ttab \"quoted\" back\\slash";
        let escaped = escape_toml_string(value);

        assert!(escaped.contains('\t'), "{escaped}");
        let table = toml::from_str::<toml::Table>(&format!("value = \"{escaped}\"\n"))
            .expect("valid TOML");
        assert_eq!(table["value"].as_str(), Some(value));
    }

    /// A backslash or `"""` in the body must not break the TOML, or Codex
    /// cannot load the file.
    #[test]
    fn agent_body_with_a_backslash_and_triple_quotes_renders_valid_toml() {
        let body = "Match \\d and \\\"this\\\".\nStop at \"\"\"here\"\"\" or \"\"\"\"\".";
        let rendered = CodexCli.convert_agent_content(
            &format!("---\nname: planner\n---\n{body}\n"),
            "planner",
        );

        let table = toml::from_str::<toml::Table>(&rendered).expect("valid TOML");

        let stored = table["developer_instructions"].as_str().expect("string");
        assert_eq!(stored.trim(), body);
    }

    /// Text with neither character renders as it always did, so a file sync
    /// already wrote stays byte-identical.
    #[test]
    fn agent_body_with_plain_quotes_renders_unchanged() {
        let body = "Say \"hello\" and \"\"empty\"\".\nSecond line.";
        let rendered = CodexCli.convert_agent_content(
            &format!("---\nname: planner\n---\n{body}\n"),
            "planner",
        );

        assert!(
            rendered.contains(&format!("\"\"\"\n{body}\n\"\"\"\n")),
            "{rendered}"
        );
    }

    #[test]
    fn hook_events_declares_twelve_events() {
        let events = CodexCli.hook_events();
        assert_eq!(
            events.len(),
            12,
            "expected 12 documented Codex CLI hook events, found {}",
            events.len()
        );
        for event in [
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
            "Interrupt",
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

    #[test]
    fn global_section_reads_header_env_references_from_the_environment() {
        let mut servers = Map::new();
        servers.insert(
            "github".to_string(),
            json!({
                "type": "http",
                "url": "https://api.githubcopilot.com/mcp/",
                "headers": {
                    "Authorization": "Bearer ${GITHUB_PERSONAL_ACCESS_TOKEN}",
                    "X-Api-Key": "${EXAMPLE_API_KEY}",
                },
            }),
        );
        let prepared = crate::agent::prepare_global_mcp_servers(&CodexCli, &servers);
        let section = render_codex_server_section("github", &prepared["github"]);

        assert!(section.contains(r#"bearer_token_env_var = "GITHUB_PERSONAL_ACCESS_TOKEN""#));
        assert!(section.contains("[mcp_servers.github.env_http_headers]"));
        assert!(section.contains(r#""X-Api-Key" = "EXAMPLE_API_KEY""#));
        assert!(!section.contains("${"), "{section}");
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
    fn codex_hook_sync_writes_the_interrupt_event() {
        let dir = tempdir().unwrap();
        let hooks = vec![codex_cmd_hook("audit", "Interrupt", "echo interrupted")];
        let written = CodexCli.sync_hooks(dir.path(), &hooks).unwrap();
        assert!(!written.is_empty());

        let raw = fs::read_to_string(dir.path().join(".codex/hooks.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let group = &v["hooks"]["Interrupt"][0];
        assert!(
            group.get("matcher").is_none(),
            "a hook without a matcher must not gain one"
        );
        assert_eq!(group["hooks"][0]["command"], "echo interrupted");
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
