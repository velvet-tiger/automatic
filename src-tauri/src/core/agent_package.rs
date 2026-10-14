//! Import of Agent Plugin packages (Agent Plugins 1.0.0,
//! <https://agent-plugins.org/specification>) into Automatic's library.
//!
//! A package is a directory with a root `plugin.json`. Its `skills/`
//! directory becomes library skills and its `mcp.json` becomes library MCP
//! servers. Automatic does not launch servers, so each stdio server is
//! stored with absolute paths that point back into the package folder. The
//! package must therefore stay where it is after import.
//!
//! "Agent package" is used in code instead of "plugin" because Automatic
//! already has unrelated internal app plugins (`app_plugins.rs`,
//! `plugins.rs`).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::asset_security::{enforce_text_asset, validate_relative_asset_path, AssetKind};
use super::paths::{get_automatic_dir, home_dir};

pub const AGENT_PACKAGE_PLUGIN_SCHEMA: &str =
    "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json";
/// Both schema URLs are pinned to 1.0.0, so requiring this exact value also
/// enforces the rule that `mcp.json` uses the same version as `plugin.json`.
pub const AGENT_PACKAGE_MCP_SCHEMA: &str =
    "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json";

const ROOT_PLACEHOLDER: &str = "${PLUGIN_ROOT}";
const DATA_PLACEHOLDER: &str = "${PLUGIN_DATA}";
const ROOT_ENV: &str = "PLUGIN_ROOT";
const DATA_ENV: &str = "PLUGIN_DATA";

const PLUGIN_FIELDS: &[&str] = &[
    "$schema",
    "name",
    "version",
    "description",
    "author",
    "homepage",
    "repository",
    "license",
    "keywords",
    "extensions",
];
const STDIO_FIELDS: &[&str] = &["type", "command", "args", "env", "cwd"];
const REMOTE_FIELDS: &[&str] = &["type", "url", "headers"];

// ── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentPackageItemKind {
    Skill,
    McpServer,
}

/// A skill or MCP server that was not imported, and why.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentPackageSkippedItem {
    pub kind: AgentPackageItemKind,
    pub name: String,
    pub reason: String,
}

/// A conforming skill directory inside the package's `skills/` folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPackageSkill {
    pub name: String,
    /// Absolute path of the skill directory inside the package.
    pub dir: String,
}

/// A conforming MCP server, already mapped to Automatic's config format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPackageMcpServer {
    pub name: String,
    pub config: Value,
}

/// Everything `read_agent_package` found in a valid package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPackage {
    /// Absolute, symlink-resolved package root.
    pub root: String,
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    /// The directory `${PLUGIN_DATA}` expands to. It is not created by a read.
    pub data_dir: String,
    pub skills: Vec<AgentPackageSkill>,
    pub mcp_servers: Vec<AgentPackageMcpServer>,
    pub invalid: Vec<AgentPackageSkippedItem>,
    pub warnings: Vec<String>,
}

/// The outcome of `import_agent_package`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPackageImportResult {
    pub package_name: String,
    pub package_root: String,
    pub data_dir: String,
    pub imported_skills: Vec<String>,
    pub imported_mcp_servers: Vec<String>,
    /// Items skipped because the library already has that name.
    pub conflicts: Vec<AgentPackageSkippedItem>,
    /// Items skipped because they do not conform, or failed a security scan.
    pub invalid: Vec<AgentPackageSkippedItem>,
    pub warnings: Vec<String>,
}

/// One valid package found in `~/.agents/plugins`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPackageSummary {
    pub path: String,
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub skill_count: usize,
    pub mcp_server_count: usize,
}

/// A folder in `~/.agents/plugins` that is not a valid package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPackageListError {
    pub path: String,
    pub error: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstalledAgentPackages {
    pub packages: Vec<AgentPackageSummary>,
    pub errors: Vec<AgentPackageListError>,
}

/// The two directories placeholders expand to.
struct PackagePaths {
    root: PathBuf,
    root_str: String,
    data: PathBuf,
    data_str: String,
}

/// Metadata from a validated `plugin.json`.
struct PluginManifest {
    name: String,
    version: Option<String>,
    description: Option<String>,
}

// ── Reading ──────────────────────────────────────────────────────────────────

/// True when `path` is a directory with a `plugin.json` file at its root.
/// Used by the UI to decide which import to run. It does not validate.
pub fn is_agent_package_dir(path: &str) -> bool {
    Path::new(path).join("plugin.json").is_file()
}

/// Validate the package at `path` and describe what an import would add.
///
/// Read-only: nothing is written. Returns `Err` when the package is rejected
/// as a whole (no `plugin.json`, invalid `name` or `$schema`, wrong field
/// types). Problems with single skills or servers are listed in `invalid`;
/// problems that disable a whole component are listed in `warnings`.
pub fn read_agent_package(path: &str) -> Result<AgentPackage, String> {
    let root = fs::canonicalize(path)
        .map_err(|e| format!("Cannot open Agent Plugin package '{}': {}", path, e))?;
    if !root.is_dir() {
        return Err(format!(
            "Agent Plugin package '{}' must be a folder",
            root.display()
        ));
    }
    let mut warnings = Vec::new();
    let manifest = read_plugin_manifest(&root, &mut warnings)?;
    let paths = package_paths(root, &manifest.name)?;
    let mut invalid = Vec::new();
    let skills = discover_skills(&paths.root, &mut invalid, &mut warnings);
    let mcp_servers = read_mcp_servers(&paths, &mut invalid, &mut warnings);
    Ok(AgentPackage {
        root: paths.root_str,
        name: manifest.name,
        version: manifest.version,
        description: manifest.description,
        data_dir: paths.data_str,
        skills,
        mcp_servers,
        invalid,
        warnings,
    })
}

fn package_paths(root: PathBuf, name: &str) -> Result<PackagePaths, String> {
    let data = agent_package_data_dir(name)?;
    Ok(PackagePaths {
        root_str: path_to_string(&root)?,
        data_str: path_to_string(&data)?,
        root,
        data,
    })
}

/// `<automatic dir>/agent-packages/<name>/data`. `name` has already passed
/// the package name rules, so it is one plain path component.
fn agent_package_data_dir(name: &str) -> Result<PathBuf, String> {
    Ok(get_automatic_dir()?
        .join("agent-packages")
        .join(name)
        .join("data"))
}

fn path_to_string(path: &Path) -> Result<String, String> {
    path.to_str()
        .map(str::to_string)
        .ok_or_else(|| format!("Path '{}' is not valid UTF-8", path.display()))
}

// ── plugin.json ──────────────────────────────────────────────────────────────

fn read_plugin_manifest(root: &Path, warnings: &mut Vec<String>) -> Result<PluginManifest, String> {
    let file = root.join("plugin.json");
    if fs::symlink_metadata(&file).is_err() {
        return Err(format!(
            "'{}' is not an Agent Plugin package: it has no plugin.json",
            root.display()
        ));
    }
    let file = resolve_inside(root, "plugin.json", "plugin.json")?;
    if !file.is_file() {
        return Err("plugin.json must be a regular file".to_string());
    }
    let raw =
        fs::read_to_string(&file).map_err(|e| format!("Failed to read plugin.json: {}", e))?;
    let value: Value =
        serde_json::from_str(&raw).map_err(|e| format!("plugin.json is not valid JSON: {}", e))?;
    let obj = value
        .as_object()
        .ok_or("plugin.json must contain a JSON object")?;
    parse_plugin_manifest(obj, warnings)
}

fn parse_plugin_manifest(
    obj: &Map<String, Value>,
    warnings: &mut Vec<String>,
) -> Result<PluginManifest, String> {
    for key in obj.keys() {
        if !PLUGIN_FIELDS.contains(&key.as_str()) {
            warnings.push(format!(
                "plugin.json has an unknown field '{}'. It was ignored.",
                key
            ));
        }
    }
    match obj.get("$schema") {
        Some(Value::String(s)) if s == AGENT_PACKAGE_PLUGIN_SCHEMA => {}
        Some(_) => {
            return Err(format!(
                "plugin.json '$schema' must be '{}'",
                AGENT_PACKAGE_PLUGIN_SCHEMA
            ))
        }
        None => return Err("plugin.json has no '$schema'".to_string()),
    }
    let name = match obj.get("name") {
        Some(Value::String(name)) => name.clone(),
        Some(_) => return Err("plugin.json 'name' must be a string".to_string()),
        None => return Err("plugin.json has no 'name'".to_string()),
    };
    validate_package_name(&name)?;
    validate_optional_fields(obj, warnings)?;
    Ok(PluginManifest {
        name,
        version: optional_string(obj, "version")?,
        description: optional_string(obj, "description")?,
    })
}

/// Check the types of the optional metadata fields. A wrong type rejects
/// the package, except `extensions`, which is ignored with a warning.
fn validate_optional_fields(
    obj: &Map<String, Value>,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    for key in [
        "version",
        "description",
        "homepage",
        "repository",
        "license",
    ] {
        optional_string(obj, key)?;
    }
    if let Some(keywords) = obj.get("keywords") {
        let all_strings = keywords
            .as_array()
            .is_some_and(|items| items.iter().all(Value::is_string));
        if !all_strings {
            return Err("plugin.json 'keywords' must be an array of strings".to_string());
        }
    }
    if let Some(author) = obj.get("author") {
        validate_author(author)?;
    }
    if obj.get("extensions").is_some_and(|ext| !ext.is_object()) {
        warnings.push("plugin.json 'extensions' is not an object. It was ignored.".to_string());
    }
    Ok(())
}

fn optional_string(obj: &Map<String, Value>, key: &str) -> Result<Option<String>, String> {
    match obj.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(format!("plugin.json '{}' must be a string", key)),
    }
}

fn validate_author(author: &Value) -> Result<(), String> {
    let obj = author
        .as_object()
        .ok_or("plugin.json 'author' must be an object")?;
    for (key, value) in obj {
        if !["name", "email", "url"].contains(&key.as_str()) {
            return Err(format!(
                "plugin.json 'author' has an unknown field '{}'",
                key
            ));
        }
        if !value.is_string() {
            return Err(format!("plugin.json 'author.{}' must be a string", key));
        }
    }
    Ok(())
}

/// Package names: 1-64 characters from `[a-z0-9.-]`, starting and ending
/// with a letter or digit, with no `--` or `..`.
fn validate_package_name(name: &str) -> Result<(), String> {
    let allowed = name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-');
    let edge_ok = |c: Option<char>| c.is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    let valid = (1..=64).contains(&name.len())
        && allowed
        && edge_ok(name.chars().next())
        && edge_ok(name.chars().last())
        && !name.contains("--")
        && !name.contains("..");
    if valid {
        return Ok(());
    }
    Err(format!(
        "plugin.json name '{}' is invalid: use 1-64 lowercase letters, digits, '.' or '-', start and end with a letter or digit, and do not use '--' or '..'",
        name.escape_debug()
    ))
}

// ── Paths ────────────────────────────────────────────────────────────────────

/// Resolve `rel` against the canonical `root` and require that the result,
/// after following symlinks, stays inside `root`. The target need not exist:
/// the deepest existing ancestor is resolved and the rest appended.
fn resolve_inside(root: &Path, rel: &str, label: &str) -> Result<PathBuf, String> {
    if rel.is_empty() {
        return Ok(root.to_path_buf());
    }
    validate_relative_asset_path(rel, label)?;
    let joined = root.join(rel);
    let existing = joined
        .ancestors()
        .find(|p| fs::symlink_metadata(p).is_ok())
        .unwrap_or(root);
    let resolved = fs::canonicalize(existing)
        .map_err(|e| format!("Failed to resolve {} '{}': {}", label, rel, e))?;
    if !resolved.starts_with(root) {
        return Err(format!(
            "{} '{}' points outside the package folder",
            label, rel
        ));
    }
    let rest = joined
        .strip_prefix(existing)
        .map_err(|e| format!("Failed to resolve {} '{}': {}", label, rel, e))?;
    // `join("")` would add a trailing separator, which makes `is_file` fail.
    if rest.as_os_str().is_empty() {
        return Ok(resolved);
    }
    Ok(resolved.join(rest))
}

/// Replace `${PLUGIN_ROOT}` and `${PLUGIN_DATA}` in one left-to-right pass.
/// Text produced by a replacement is never scanned again.
fn expand_placeholders(input: &str, paths: &PackagePaths) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(c) = rest.chars().next() {
        if let Some(tail) = rest.strip_prefix(ROOT_PLACEHOLDER) {
            out.push_str(&paths.root_str);
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix(DATA_PLACEHOLDER) {
            out.push_str(&paths.data_str);
            rest = tail;
        } else {
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

// ── Skills ───────────────────────────────────────────────────────────────────

/// Find each immediate child of `skills/` that holds a regular `SKILL.md`.
/// There is no recursive search. A missing `skills/` is not an error.
fn discover_skills(
    root: &Path,
    invalid: &mut Vec<AgentPackageSkippedItem>,
    warnings: &mut Vec<String>,
) -> Vec<AgentPackageSkill> {
    let skills_path = root.join("skills");
    if fs::symlink_metadata(&skills_path).is_err() {
        return Vec::new();
    }
    let dir = match resolve_inside(root, "skills", "skills folder") {
        Ok(dir) if dir.is_dir() => dir,
        Ok(_) => {
            warnings.push("'skills' is not a folder. No skills were read.".to_string());
            return Vec::new();
        }
        Err(e) => {
            warnings.push(format!("{}. No skills were read.", e));
            return Vec::new();
        }
    };
    let mut found = Vec::new();
    for (name, path) in sorted_children(&dir, warnings) {
        match check_skill_dir(root, &name, &path) {
            Ok(Some(skill)) => found.push(skill),
            Ok(None) => {}
            Err(reason) => invalid.push(skipped(AgentPackageItemKind::Skill, &name, reason)),
        }
    }
    found
}

fn sorted_children(dir: &Path, warnings: &mut Vec<String>) -> Vec<(String, PathBuf)> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            warnings.push(format!("Failed to read '{}': {}", dir.display(), e));
            return Vec::new();
        }
    };
    let mut children: Vec<(String, PathBuf)> = entries
        .filter_map(Result::ok)
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().to_string(),
                entry.path(),
            )
        })
        .collect();
    children.sort();
    children
}

/// `Ok(None)` for a plain file in `skills/`, which is not a skill and not
/// worth reporting. `Err` for a directory that is not a conforming skill.
fn check_skill_dir(
    root: &Path,
    name: &str,
    path: &Path,
) -> Result<Option<AgentPackageSkill>, String> {
    let rel = format!("skills/{}", name);
    let dir = resolve_inside(root, &rel, "skill folder")?;
    if !dir.is_dir() {
        return Ok(None);
    }
    super::remote_sources::ensure_resource_name("skill", name)?;
    if !has_exact_child(&dir, "SKILL.md") {
        return Err("the folder has no SKILL.md file".to_string());
    }
    let skill_md = resolve_inside(root, &format!("{}/SKILL.md", rel), "SKILL.md")?;
    if !skill_md.is_file() {
        return Err("SKILL.md is not a regular file".to_string());
    }
    ensure_children_inside(root, &dir)?;
    Ok(Some(AgentPackageSkill {
        name: name.to_string(),
        dir: path_to_string(path)?,
    }))
}

/// Compare names exactly. `Path::exists` would accept `skill.md` on a
/// case-insensitive file system.
fn has_exact_child(dir: &Path, name: &str) -> bool {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .any(|e| e.file_name() == name)
        })
        .unwrap_or(false)
}

/// Every entry of a skill folder must resolve inside the package. The skill
/// importer copies companion folders by following symlinks at the top level.
fn ensure_children_inside(root: &Path, dir: &Path) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("Failed to read the folder: {}", e))?;
    for entry in entries.filter_map(Result::ok) {
        let resolved = fs::canonicalize(entry.path()).map_err(|e| {
            format!(
                "Failed to resolve '{}': {}",
                entry.file_name().to_string_lossy(),
                e
            )
        })?;
        if !resolved.starts_with(root) {
            return Err(format!(
                "'{}' points outside the package folder",
                entry.file_name().to_string_lossy()
            ));
        }
    }
    Ok(())
}

fn skipped(kind: AgentPackageItemKind, name: &str, reason: String) -> AgentPackageSkippedItem {
    AgentPackageSkippedItem {
        kind,
        name: name.to_string(),
        reason,
    }
}

// ── mcp.json ─────────────────────────────────────────────────────────────────

/// Read and map `mcp.json`. A file-level problem disables MCP for the
/// package with a warning; a bad entry skips only that entry.
fn read_mcp_servers(
    paths: &PackagePaths,
    invalid: &mut Vec<AgentPackageSkippedItem>,
    warnings: &mut Vec<String>,
) -> Vec<AgentPackageMcpServer> {
    if fs::symlink_metadata(paths.root.join("mcp.json")).is_err() {
        return Vec::new();
    }
    let servers = match read_mcp_file(&paths.root) {
        Ok(servers) => servers,
        Err(e) => {
            warnings.push(format!("{}. No MCP servers were read.", e));
            return Vec::new();
        }
    };
    let mut mapped = Vec::new();
    for (name, entry) in servers {
        match map_mcp_entry(&name, &entry, paths) {
            Ok(config) => mapped.push(AgentPackageMcpServer { name, config }),
            Err(reason) => invalid.push(skipped(AgentPackageItemKind::McpServer, &name, reason)),
        }
    }
    mapped
}

fn read_mcp_file(root: &Path) -> Result<Map<String, Value>, String> {
    let file = resolve_inside(root, "mcp.json", "mcp.json")?;
    if !file.is_file() {
        return Err("mcp.json is not a regular file".to_string());
    }
    let raw = fs::read_to_string(&file).map_err(|e| format!("Failed to read mcp.json: {}", e))?;
    let value: Value =
        serde_json::from_str(&raw).map_err(|e| format!("mcp.json is not valid JSON: {}", e))?;
    let obj = value
        .as_object()
        .ok_or("mcp.json must contain a JSON object")?;
    if let Some(key) = obj.keys().find(|k| *k != "$schema" && *k != "mcpServers") {
        return Err(format!("mcp.json has an unknown field '{}'", key));
    }
    if obj.get("$schema").and_then(Value::as_str) != Some(AGENT_PACKAGE_MCP_SCHEMA) {
        return Err(format!(
            "mcp.json '$schema' must be '{}', the same version as plugin.json",
            AGENT_PACKAGE_MCP_SCHEMA
        ));
    }
    match obj.get("mcpServers") {
        Some(Value::Object(servers)) => Ok(servers.clone()),
        Some(_) => Err("mcp.json 'mcpServers' must be an object".to_string()),
        None => Err("mcp.json has no 'mcpServers'".to_string()),
    }
}

/// Validate one `mcpServers` entry and map it to Automatic's format.
fn map_mcp_entry(name: &str, entry: &Value, paths: &PackagePaths) -> Result<Value, String> {
    super::remote_sources::ensure_resource_name("MCP server", name)?;
    let obj = entry.as_object().ok_or("the entry must be an object")?;
    let transport = obj
        .get("type")
        .and_then(Value::as_str)
        .ok_or("the entry has no 'type'")?;
    let config = match transport {
        "stdio" => {
            ensure_only_fields(obj, STDIO_FIELDS, transport)?;
            map_stdio(obj, paths)?
        }
        "streamable-http" | "sse" => {
            ensure_only_fields(obj, REMOTE_FIELDS, transport)?;
            map_remote(obj, transport)?
        }
        other => return Err(format!("unknown type '{}'", other)),
    };
    if let Some(reason) = super::mcp_docker::mcp_sync_block_reason(&config) {
        return Err(reason);
    }
    Ok(config)
}

fn ensure_only_fields(
    obj: &Map<String, Value>,
    allowed: &[&str],
    transport: &str,
) -> Result<(), String> {
    match obj.keys().find(|k| !allowed.contains(&k.as_str())) {
        Some(key) => Err(format!(
            "field '{}' is not allowed for type '{}'",
            key, transport
        )),
        None => Ok(()),
    }
}

fn map_stdio(obj: &Map<String, Value>, paths: &PackagePaths) -> Result<Value, String> {
    let command = obj
        .get("command")
        .and_then(Value::as_str)
        .ok_or("'command' is required and must be a string")?;
    let command = resolve_command(command, &paths.root)?;
    let args = string_array(obj.get("args"), "args")?
        .iter()
        .map(|arg| Value::String(expand_placeholders(arg, paths)))
        .collect::<Vec<_>>();
    let env = map_env(obj.get("env"), paths)?;
    let cwd = match obj.get("cwd") {
        None => paths.root.clone(),
        Some(Value::String(raw)) => resolve_cwd(raw, paths)?,
        Some(_) => return Err("'cwd' must be a string".to_string()),
    };
    let mut config = Map::new();
    config.insert("type".to_string(), Value::String("stdio".to_string()));
    config.insert("command".to_string(), Value::String(command));
    config.insert("args".to_string(), Value::Array(args));
    config.insert("env".to_string(), Value::Object(env));
    config.insert("cwd".to_string(), Value::String(path_to_string(&cwd)?));
    Ok(Value::Object(config))
}

/// A command is one token: a bare executable name looked up on `PATH`, or a
/// `./` path inside the package. A `./` path becomes absolute so the agent
/// can start it from any working directory.
fn resolve_command(command: &str, root: &Path) -> Result<String, String> {
    if command.is_empty() || command.chars().any(char::is_whitespace) {
        return Err("'command' must be one token with no spaces".to_string());
    }
    if let Some(rel) = command.strip_prefix("./") {
        let path = resolve_inside(root, rel, "command")?;
        if !path.is_file() {
            return Err(format!(
                "command '{}' is not a file in the package",
                command
            ));
        }
        return path_to_string(&path);
    }
    if command.contains('/') || command.contains('\\') {
        return Err(format!(
            "command '{}' must be a bare name or a './' path inside the package",
            command
        ));
    }
    Ok(command.to_string())
}

fn string_array(value: Option<&Value>, label: &str) -> Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    value
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .map(|v| v.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()
        })
        .ok_or_else(|| format!("'{}' must be an array of strings", label))
}

/// Expand placeholders in env values, then set `PLUGIN_ROOT` and
/// `PLUGIN_DATA` last so they always hold the client's values.
fn map_env(value: Option<&Value>, paths: &PackagePaths) -> Result<Map<String, Value>, String> {
    let mut env = Map::new();
    if let Some(value) = value {
        let obj = value.as_object().ok_or("'env' must be an object")?;
        for (key, val) in obj {
            if key == ROOT_ENV || key == DATA_ENV {
                return Err(format!("'env' must not set '{}'", key));
            }
            let text = val
                .as_str()
                .ok_or_else(|| format!("env value '{}' must be a string", key))?;
            env.insert(key.clone(), Value::String(expand_placeholders(text, paths)));
        }
    }
    env.insert(ROOT_ENV.to_string(), Value::String(paths.root_str.clone()));
    env.insert(DATA_ENV.to_string(), Value::String(paths.data_str.clone()));
    Ok(env)
}

fn resolve_cwd(raw: &str, paths: &PackagePaths) -> Result<PathBuf, String> {
    let under = |prefix: &str| {
        if raw == prefix {
            Some("")
        } else {
            raw.strip_prefix(prefix).and_then(|r| r.strip_prefix('/'))
        }
    };
    if let Some(rel) = under(ROOT_PLACEHOLDER) {
        return resolve_inside(&paths.root, rel, "cwd");
    }
    if let Some(rel) = under(DATA_PLACEHOLDER) {
        if rel.is_empty() {
            return Ok(paths.data.clone());
        }
        validate_relative_asset_path(rel, "cwd")?;
        return Ok(paths.data.join(rel));
    }
    if let Some(rel) = raw.strip_prefix("./") {
        return resolve_inside(&paths.root, rel, "cwd");
    }
    Err(format!(
        "cwd '{}' must be a './' path, or start with ${{PLUGIN_ROOT}} or ${{PLUGIN_DATA}}",
        raw
    ))
}

fn map_remote(obj: &Map<String, Value>, transport: &str) -> Result<Value, String> {
    let url = obj
        .get("url")
        .and_then(Value::as_str)
        .ok_or("'url' is required and must be a string")?;
    validate_remote_url(url)?;
    let kind = if transport == "sse" { "sse" } else { "http" };
    let mut config = Map::new();
    config.insert("type".to_string(), Value::String(kind.to_string()));
    config.insert("url".to_string(), Value::String(url.to_string()));
    if let Some(headers) = obj.get("headers") {
        config.insert(
            "headers".to_string(),
            Value::Object(validate_headers(headers)?),
        );
    }
    Ok(Value::Object(config))
}

/// Absolute http(s) URL with no userinfo or fragment. Plain http is only
/// allowed for `localhost` or a loopback IP literal.
fn validate_remote_url(raw: &str) -> Result<(), String> {
    let url = url::Url::parse(raw).map_err(|e| format!("url '{}' is invalid: {}", raw, e))?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(format!("url '{}' must use http or https", raw));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(format!(
            "url '{}' must not contain a user name or password",
            raw
        ));
    }
    if url.fragment().is_some() {
        return Err(format!("url '{}' must not contain a fragment", raw));
    }
    let host = url
        .host()
        .ok_or_else(|| format!("url '{}' has no host", raw))?;
    let is_loopback = match host {
        url::Host::Domain(domain) => domain.eq_ignore_ascii_case("localhost"),
        url::Host::Ipv4(ip) => ip.is_loopback(),
        url::Host::Ipv6(ip) => ip.is_loopback(),
    };
    if url.scheme() == "http" && !is_loopback {
        return Err(format!(
            "url '{}' must use https; http is only allowed for localhost",
            raw
        ));
    }
    Ok(())
}

fn validate_headers(value: &Value) -> Result<Map<String, Value>, String> {
    let obj = value.as_object().ok_or("'headers' must be an object")?;
    let mut seen = std::collections::HashSet::new();
    for (key, val) in obj {
        if !val.is_string() {
            return Err(format!("header '{}' must be a string", key));
        }
        if !seen.insert(key.to_ascii_lowercase()) {
            return Err(format!(
                "header '{}' appears more than once with different letter case",
                key
            ));
        }
    }
    Ok(obj.clone())
}

// ── Import ───────────────────────────────────────────────────────────────────

/// Read the package at `path`, then add its skills and MCP servers to the
/// library. Names already in the library are skipped and never overwritten.
/// Creates the package's `${PLUGIN_DATA}` folder; never deletes it.
pub fn import_agent_package(path: &str) -> Result<AgentPackageImportResult, String> {
    let package = read_agent_package(path)?;
    fs::create_dir_all(&package.data_dir).map_err(|e| {
        format!(
            "Failed to create the data folder '{}': {}",
            package.data_dir, e
        )
    })?;
    let mut result = AgentPackageImportResult {
        package_name: package.name.clone(),
        package_root: package.root.clone(),
        data_dir: package.data_dir.clone(),
        imported_skills: Vec::new(),
        imported_mcp_servers: Vec::new(),
        conflicts: Vec::new(),
        invalid: package.invalid.clone(),
        warnings: package.warnings.clone(),
    };
    for skill in &package.skills {
        import_one_skill(skill, &mut result);
    }
    let existing_servers = super::mcp_servers::list_mcp_server_configs()?;
    for server in &package.mcp_servers {
        import_one_server(server, &existing_servers, &mut result);
    }
    assign_skill_collection(&package.name, &mut result);
    Ok(result)
}

fn import_one_skill(skill: &AgentPackageSkill, result: &mut AgentPackageImportResult) {
    let kind = AgentPackageItemKind::Skill;
    if super::skills::skill_exists(&skill.name) || super::mcp_servers::is_builtin_skill(&skill.name)
    {
        result
            .conflicts
            .push(skipped(kind, &skill.name, conflict_reason()));
        return;
    }
    let skill_md = Path::new(&skill.dir).join("SKILL.md");
    match super::skills::import_skill_from_local_path(&skill_md.to_string_lossy()) {
        Ok(_) => result.imported_skills.push(skill.name.clone()),
        Err(e) => result.invalid.push(skipped(kind, &skill.name, e)),
    }
}

fn import_one_server(
    server: &AgentPackageMcpServer,
    existing: &[String],
    result: &mut AgentPackageImportResult,
) {
    let kind = AgentPackageItemKind::McpServer;
    if existing.contains(&server.name) || super::mcp_servers::is_builtin_mcp_server(&server.name) {
        result
            .conflicts
            .push(skipped(kind, &server.name, conflict_reason()));
        return;
    }
    match save_server(server) {
        Ok(()) => result.imported_mcp_servers.push(server.name.clone()),
        Err(e) => result.invalid.push(skipped(kind, &server.name, e)),
    }
}

/// Same path as a remote-source install: security scan, blank anything that
/// looks like ciphertext, then the registry's own save (which encrypts env).
fn save_server(server: &AgentPackageMcpServer) -> Result<(), String> {
    let content = serde_json::to_string_pretty(&server.config)
        .map_err(|e| format!("Failed to serialise MCP server '{}': {}", server.name, e))?;
    enforce_text_asset(
        AssetKind::Template,
        &format!("MCP server '{}'", server.name),
        &content,
    )?;
    let content = super::mcp_servers::blank_ciphertext_like_env_values(&content);
    super::mcp_servers::save_mcp_server_config(&server.name, &content)
        .map_err(|e| format!("Failed to save MCP server '{}': {}", server.name, e))
}

fn conflict_reason() -> String {
    "the library already has an item with this name. It was not changed.".to_string()
}

/// Group the imported skills under a collection named after the package, as
/// a `skill.json` import does.
fn assign_skill_collection(package_name: &str, result: &mut AgentPackageImportResult) {
    if result.imported_skills.is_empty() {
        return;
    }
    if let Err(e) = super::skills::set_skills_collection(&result.imported_skills, package_name) {
        result
            .warnings
            .push(format!("The skills were imported but not grouped: {}", e));
    }
}

// ── Installed packages ──────────────────────────────────────────────────────

/// List the packages in `~/.agents/plugins`, one level deep. Read-only.
/// A folder that is not a valid package is listed in `errors`.
pub fn list_installed_agent_packages() -> Result<InstalledAgentPackages, String> {
    let dir = home_dir()?.join(".agents").join("plugins");
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(InstalledAgentPackages::default())
        }
        Err(e) => return Err(format!("Failed to read '{}': {}", dir.display(), e)),
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    paths.sort();
    let mut installed = InstalledAgentPackages::default();
    for path in paths {
        let path_str = path.to_string_lossy().to_string();
        match read_agent_package(&path_str) {
            Ok(package) => installed.packages.push(summarise(path_str, package)),
            Err(error) => installed.errors.push(AgentPackageListError {
                path: path_str,
                error,
            }),
        }
    }
    Ok(installed)
}

fn summarise(path: String, package: AgentPackage) -> AgentPackageSummary {
    AgentPackageSummary {
        path,
        name: package.name,
        version: package.version,
        description: package.description,
        skill_count: package.skills.len(),
        mcp_server_count: package.mcp_servers.len(),
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::paths::with_test_home;
    use serde_json::json;
    use tempfile::TempDir;

    fn write(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn plugin_json(name: &str) -> Value {
        json!({ "$schema": AGENT_PACKAGE_PLUGIN_SCHEMA, "name": name, "version": "1.0.0" })
    }

    fn write_plugin(root: &Path, manifest: &Value) {
        write(&root.join("plugin.json"), &manifest.to_string());
    }

    fn write_mcp(root: &Path, servers: Value) {
        let doc = json!({ "$schema": AGENT_PACKAGE_MCP_SCHEMA, "mcpServers": servers });
        write(&root.join("mcp.json"), &doc.to_string());
    }

    fn write_skill(root: &Path, name: &str) {
        write(
            &root.join("skills").join(name).join("SKILL.md"),
            &format!(
                "---\nname: {}\ndescription: Test skill.\n---\n\nDo the thing.\n",
                name
            ),
        );
    }

    /// A temp home plus a package folder, with the home installed for the
    /// duration of `test`.
    fn with_package<T>(test: impl FnOnce(&Path) -> T) -> T {
        let home = TempDir::new().unwrap();
        let pkg = TempDir::new().unwrap();
        let root = fs::canonicalize(pkg.path()).unwrap();
        with_test_home(home.path().to_path_buf(), || test(&root))
    }

    fn read(root: &Path) -> Result<AgentPackage, String> {
        read_agent_package(root.to_str().unwrap())
    }

    fn server<'a>(package: &'a AgentPackage, name: &str) -> &'a Value {
        &package
            .mcp_servers
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("server {} missing: {:?}", name, package.invalid))
            .config
    }

    fn invalid_names(package: &AgentPackage) -> Vec<String> {
        package.invalid.iter().map(|i| i.name.clone()).collect()
    }

    fn write_full_package(root: &Path) {
        write_plugin(root, &plugin_json("demo-pkg"));
        write_skill(root, "alpha");
        write_skill(root, "beta");
        write(&root.join("bin/server"), "#!/bin/sh\n");
        write_mcp(
            root,
            json!({
                "local": {
                    "type": "stdio",
                    "command": "./bin/server",
                    "args": ["--root", "${PLUGIN_ROOT}/conf", "--data=${PLUGIN_DATA}"],
                    "env": { "MODE": "${PLUGIN_ROOT}" }
                },
                "remote": {
                    "type": "streamable-http",
                    "url": "https://example.com/mcp",
                    "headers": { "X-Key": "abc" }
                },
                "events": { "type": "sse", "url": "https://example.com/sse" }
            }),
        );
    }

    #[test]
    fn valid_package_reads_skills_and_maps_servers() {
        with_package(|root| {
            write_full_package(root);
            let package = read(root).unwrap();
            let root_str = root.to_str().unwrap();
            assert_eq!(package.name, "demo-pkg");
            let names: Vec<_> = package.skills.iter().map(|s| s.name.as_str()).collect();
            assert_eq!(names, vec!["alpha", "beta"]);
            assert!(package.invalid.is_empty(), "{:?}", package.invalid);

            let local = server(&package, "local");
            assert_eq!(local["type"], "stdio");
            assert_eq!(local["command"], format!("{}/bin/server", root_str));
            assert_eq!(local["args"][1], format!("{}/conf", root_str));
            assert_eq!(local["args"][2], format!("--data={}", package.data_dir));
            assert_eq!(local["cwd"], root_str);
            assert_eq!(local["env"]["MODE"], root_str);
            assert_eq!(local["env"]["PLUGIN_ROOT"], root_str);
            assert_eq!(local["env"]["PLUGIN_DATA"], package.data_dir.as_str());
            assert!(package.data_dir.ends_with("agent-packages/demo-pkg/data"));

            let remote = server(&package, "remote");
            assert_eq!(
                remote,
                &json!({"type": "http", "url": "https://example.com/mcp", "headers": {"X-Key": "abc"}})
            );
            assert_eq!(
                server(&package, "events"),
                &json!({"type": "sse", "url": "https://example.com/sse"})
            );
        });
    }

    #[test]
    fn import_adds_skills_and_servers_and_creates_data_dir() {
        with_package(|root| {
            write_full_package(root);
            let result = import_agent_package(root.to_str().unwrap()).unwrap();
            assert_eq!(result.imported_skills, vec!["alpha", "beta"]);
            let mut servers = result.imported_mcp_servers.clone();
            servers.sort();
            assert_eq!(servers, vec!["events", "local", "remote"]);
            assert!(result.conflicts.is_empty() && result.invalid.is_empty());
            assert!(Path::new(&result.data_dir).is_dir());
            assert!(crate::core::skills::skill_exists("alpha"));
            let saved: Value = serde_json::from_str(
                &crate::core::mcp_servers::read_mcp_server_config("local").unwrap(),
            )
            .unwrap();
            assert_eq!(saved["env"]["PLUGIN_ROOT"], root.to_str().unwrap());
        });
    }

    #[test]
    fn existing_library_names_are_skipped_not_overwritten() {
        with_package(|root| {
            write_full_package(root);
            crate::core::skills::save_skill("alpha", "---\nname: alpha\n---\noriginal\n").unwrap();
            crate::core::mcp_servers::save_mcp_server_config(
                "remote",
                r#"{"type":"http","url":"https://keep.example/mcp"}"#,
            )
            .unwrap();

            let result = import_agent_package(root.to_str().unwrap()).unwrap();
            let conflicts: Vec<_> = result.conflicts.iter().map(|c| c.name.as_str()).collect();
            assert_eq!(conflicts, vec!["alpha", "remote"]);
            assert_eq!(result.imported_skills, vec!["beta"]);
            assert!(crate::core::skills::read_skill_raw("alpha")
                .unwrap()
                .contains("original"));
            let kept = crate::core::mcp_servers::read_mcp_server_config("remote").unwrap();
            assert!(kept.contains("keep.example"));
        });
    }

    #[test]
    fn missing_or_wrong_schema_rejects_package() {
        with_package(|root| {
            write(&root.join("plugin.json"), r#"{"name":"demo"}"#);
            assert!(read(root).unwrap_err().contains("$schema"));
            let mut manifest = plugin_json("demo");
            manifest["$schema"] =
                json!("https://agent-plugins.org/schemas/2.0.0/plugin.schema.json");
            write_plugin(root, &manifest);
            assert!(read(root).unwrap_err().contains("$schema"));
        });
    }

    #[test]
    fn missing_plugin_json_rejects_package() {
        with_package(|root| {
            write_skill(root, "alpha");
            assert!(read(root).unwrap_err().contains("no plugin.json"));
        });
    }

    #[test]
    fn invalid_names_reject_package() {
        with_package(|root| {
            let long = "a".repeat(65);
            for name in ["A", "-a", "a-", "a--b", "a..b", "", "a_b", long.as_str()] {
                write_plugin(root, &plugin_json(name));
                assert!(read(root).is_err(), "name {:?} should be rejected", name);
            }
            let max = "a".repeat(64);
            for name in ["a", "a.b-c", "9lives", max.as_str()] {
                write_plugin(root, &plugin_json(name));
                assert!(read(root).is_ok(), "name {:?} should be accepted", name);
            }
        });
    }

    #[test]
    fn wrong_field_types_reject_package() {
        with_package(|root| {
            let cases = [
                ("version", json!(1)),
                ("keywords", json!(["ok", 2])),
                ("author", json!("someone")),
                ("author", json!({"name": "a", "twitter": "b"})),
            ];
            for (key, value) in cases {
                let mut manifest = plugin_json("demo");
                manifest[key] = value;
                write_plugin(root, &manifest);
                assert!(read(root).is_err(), "{} should be rejected", key);
            }
        });
    }

    #[test]
    fn unknown_fields_and_bad_extensions_warn_and_still_import() {
        with_package(|root| {
            let mut manifest = plugin_json("demo");
            manifest["colour"] = json!("blue");
            manifest["extensions"] = json!("nope");
            write_plugin(root, &manifest);
            write_skill(root, "alpha");
            let result = import_agent_package(root.to_str().unwrap()).unwrap();
            assert_eq!(result.imported_skills, vec!["alpha"]);
            assert!(result.warnings.iter().any(|w| w.contains("'colour'")));
            assert!(result.warnings.iter().any(|w| w.contains("extensions")));
        });
    }

    #[test]
    fn inline_mcp_servers_in_plugin_json_are_ignored() {
        with_package(|root| {
            let mut manifest = plugin_json("demo");
            manifest["mcpServers"] = json!({"x": {"type": "stdio", "command": "node"}});
            write_plugin(root, &manifest);
            let package = read(root).unwrap();
            assert!(package.mcp_servers.is_empty());
            assert!(package.warnings.iter().any(|w| w.contains("'mcpServers'")));
        });
    }

    #[test]
    fn nested_skills_are_not_discovered() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write(&root.join("skills/a/b/SKILL.md"), "---\nname: b\n---\n");
            let package = read(root).unwrap();
            assert!(package.skills.is_empty());
            assert_eq!(invalid_names(&package), vec!["a"]);
        });
    }

    #[test]
    fn lowercase_skill_md_is_not_a_skill() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write(&root.join("skills/a/skill.md"), "x");
            let package = read(root).unwrap();
            assert!(package.skills.is_empty());
        });
    }

    #[test]
    fn skills_as_file_warns_and_mcp_still_loads() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write(&root.join("skills"), "not a folder");
            write_mcp(
                root,
                json!({"r": {"type": "sse", "url": "https://e.com/s"}}),
            );
            let package = read(root).unwrap();
            assert!(package.skills.is_empty());
            assert!(package.warnings.iter().any(|w| w.contains("'skills'")));
            assert_eq!(package.mcp_servers.len(), 1);
        });
    }

    #[test]
    fn mcp_schema_mismatch_disables_mcp_only() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write_skill(root, "alpha");
            let doc = json!({
                "$schema": "https://agent-plugins.org/schemas/1.1.0/mcp.schema.json",
                "mcpServers": {"r": {"type": "sse", "url": "https://e.com/s"}}
            });
            write(&root.join("mcp.json"), &doc.to_string());
            let package = read(root).unwrap();
            assert!(package.mcp_servers.is_empty());
            assert_eq!(package.skills.len(), 1);
            assert!(package.warnings.iter().any(|w| w.contains("mcp.json")));
        });
    }

    #[test]
    fn mcp_extra_field_or_bad_json_disables_mcp() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            let doc = json!({"$schema": AGENT_PACKAGE_MCP_SCHEMA, "mcpServers": {}, "x": 1});
            write(&root.join("mcp.json"), &doc.to_string());
            assert!(read(root)
                .unwrap()
                .warnings
                .iter()
                .any(|w| w.contains("'x'")));
            write(&root.join("mcp.json"), "{not json");
            assert!(read(root)
                .unwrap()
                .warnings
                .iter()
                .any(|w| w.contains("valid JSON")));
        });
    }

    #[cfg(unix)]
    #[test]
    fn command_paths_must_stay_inside_package() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            let outside = TempDir::new().unwrap();
            write(&outside.path().join("evil"), "#!/bin/sh\n");
            fs::create_dir_all(root.join("bin")).unwrap();
            std::os::unix::fs::symlink(outside.path().join("evil"), root.join("bin/link")).unwrap();
            write_mcp(
                root,
                json!({
                    "up": {"type": "stdio", "command": "./../x"},
                    "link": {"type": "stdio", "command": "./bin/link"},
                    "abs": {"type": "stdio", "command": "/usr/bin/env"},
                    "two": {"type": "stdio", "command": "node server.js"},
                    "bare": {"type": "stdio", "command": "node"}
                }),
            );
            let package = read(root).unwrap();
            let mut bad = invalid_names(&package);
            bad.sort();
            assert_eq!(bad, vec!["abs", "link", "two", "up"]);
            assert_eq!(server(&package, "bare")["command"], "node");
        });
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_skill_folder_escaping_root_is_rejected() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            let outside = TempDir::new().unwrap();
            write(&outside.path().join("SKILL.md"), "---\nname: x\n---\n");
            fs::create_dir_all(root.join("skills")).unwrap();
            std::os::unix::fs::symlink(outside.path(), root.join("skills/escape")).unwrap();
            let package = read(root).unwrap();
            assert!(package.skills.is_empty());
            assert_eq!(invalid_names(&package), vec!["escape"]);
        });
    }

    #[test]
    fn env_must_not_set_plugin_variables() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write_mcp(
                root,
                json!({
                    "a": {"type": "stdio", "command": "node", "env": {"PLUGIN_ROOT": "/x"}},
                    "b": {"type": "stdio", "command": "node", "env": {"PLUGIN_DATA": "/x"}},
                    "c": {"type": "stdio", "command": "node", "env": {"OK": "1"}}
                }),
            );
            let package = read(root).unwrap();
            assert_eq!(invalid_names(&package), vec!["a", "b"]);
            assert_eq!(server(&package, "c")["env"]["OK"], "1");
        });
    }

    #[test]
    fn cross_variant_unknown_type_and_unknown_fields_skip_entry() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write_mcp(
                root,
                json!({
                    "mixed": {"type": "stdio", "command": "node", "url": "https://e.com"},
                    "mixed2": {"type": "sse", "url": "https://e.com", "args": []},
                    "odd": {"type": "websocket", "url": "wss://e.com"},
                    "notype": {"command": "node"}
                }),
            );
            let package = read(root).unwrap();
            assert!(package.mcp_servers.is_empty());
            assert_eq!(package.invalid.len(), 4);
        });
    }

    #[test]
    fn remote_url_rules() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write_mcp(
                root,
                json!({
                    "plain": {"type": "streamable-http", "url": "http://example.com/mcp"},
                    "local": {"type": "streamable-http", "url": "http://localhost:3000/mcp"},
                    "loop4": {"type": "streamable-http", "url": "http://127.0.0.1:3000/mcp"},
                    "loop6": {"type": "sse", "url": "http://[::1]:3000/sse"},
                    "user": {"type": "streamable-http", "url": "https://me:pw@example.com/mcp"},
                    "frag": {"type": "streamable-http", "url": "https://example.com/mcp#x"},
                    "rel": {"type": "streamable-http", "url": "/mcp"},
                    "ftp": {"type": "streamable-http", "url": "ftp://example.com/mcp"}
                }),
            );
            let package = read(root).unwrap();
            let mut bad = invalid_names(&package);
            bad.sort();
            assert_eq!(bad, vec!["frag", "ftp", "plain", "rel", "user"]);
            assert_eq!(package.mcp_servers.len(), 3);
        });
    }

    #[test]
    fn duplicate_headers_by_case_are_rejected() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write(
                &root.join("mcp.json"),
                &format!(
                    r#"{{"$schema":"{}","mcpServers":{{"h":{{"type":"sse","url":"https://e.com","headers":{{"X-Key":"a","x-key":"b"}}}}}}}}"#,
                    AGENT_PACKAGE_MCP_SCHEMA
                ),
            );
            let package = read(root).unwrap();
            assert_eq!(invalid_names(&package), vec!["h"]);
        });
    }

    #[test]
    fn placeholder_expansion_is_single_and_not_recursive() {
        let paths = PackagePaths {
            root: PathBuf::from("/pkg/${PLUGIN_DATA}"),
            root_str: "/pkg/${PLUGIN_DATA}".to_string(),
            data: PathBuf::from("/data"),
            data_str: "/data".to_string(),
        };
        assert_eq!(
            expand_placeholders("${PLUGIN_ROOT}:${PLUGIN_DATA}:${OTHER}", &paths),
            "/pkg/${PLUGIN_DATA}:/data:${OTHER}"
        );
        assert_eq!(
            expand_placeholders("$${PLUGIN_ROOT}}", &paths),
            "$/pkg/${PLUGIN_DATA}}"
        );
    }

    #[test]
    fn cwd_forms_resolve_and_escapes_are_rejected() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            fs::create_dir_all(root.join("work")).unwrap();
            write_mcp(
                root,
                json!({
                    "dot": {"type": "stdio", "command": "node", "cwd": "./work"},
                    "root": {"type": "stdio", "command": "node", "cwd": "${PLUGIN_ROOT}/work"},
                    "data": {"type": "stdio", "command": "node", "cwd": "${PLUGIN_DATA}/cache"},
                    "up": {"type": "stdio", "command": "node", "cwd": "${PLUGIN_ROOT}/../x"},
                    "abs": {"type": "stdio", "command": "node", "cwd": "/tmp"}
                }),
            );
            let package = read(root).unwrap();
            let work = format!("{}/work", root.to_str().unwrap());
            assert_eq!(server(&package, "dot")["cwd"], work.as_str());
            assert_eq!(server(&package, "root")["cwd"], work.as_str());
            assert_eq!(
                server(&package, "data")["cwd"],
                format!("{}/cache", package.data_dir).as_str()
            );
            let mut bad = invalid_names(&package);
            bad.sort();
            assert_eq!(bad, vec!["abs", "up"]);
        });
    }

    #[test]
    fn installed_packages_are_listed_from_home() {
        let home = TempDir::new().unwrap();
        with_test_home(home.path().to_path_buf(), || {
            assert!(list_installed_agent_packages().unwrap().packages.is_empty());
            let plugins = home.path().join(".agents/plugins");
            write_plugin(&plugins.join("good"), &plugin_json("good"));
            write_skill(&plugins.join("good"), "alpha");
            fs::create_dir_all(plugins.join("empty")).unwrap();
            let listed = list_installed_agent_packages().unwrap();
            assert_eq!(listed.packages.len(), 1);
            assert_eq!(listed.packages[0].name, "good");
            assert_eq!(listed.packages[0].skill_count, 1);
            assert_eq!(listed.errors.len(), 1);
            assert!(listed.errors[0].path.ends_with("empty"));
        });
    }
}
